//! A08 Capabilities Registry (Skills, MCP Servers, Plugins), Read-Only Discovery, Rug-Pull
//! Trust Downgrade, 1-Click Rollback, and Isolated Runtime Materialization (`services/head`).
//!
//! Security & Product Invariants (Spec 41, Spec 30 §16.5, A08):
//! - Scanning the user's personal profile (`~/.gemini/`) is **strictly read-only**: Dennett
//!   NEVER writes, creates, or modifies any file inside `~/.gemini/`.
//! - Personal findings ALWAYS receive `origin_kind = PersonalDiscovered`,
//!   `scope_kind = CandidateUnattached`, and `activation_state = Disabled`, and are NEVER
//!   materialized into any project until explicitly enabled/bound by the owner.
//! - Updating a capability preserves `previous_snapshot_json` for 1-click rollback.
//! - Expanding an executable surface (`has_scripts`, `has_hooks`, changed MCP command/args, or
//!   added/changed `secret_slots`) automatically downgrades `trust_state` to `Restricted`
//!   (rug-pull protection) and blocks materialization until explicitly re-trusted.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use dennett_contracts::{
    CapabilityActivation, CapabilityId, CapabilityKind, CapabilityOrigin, CapabilityOwnership,
    CapabilityPossession, CapabilityProjectBinding, CapabilityRecord, CapabilityScopeKind,
    CapabilitySpec, CapabilityTrust, CompatibilityStatus, CorrelationId, CreateCapabilityRequest,
    DeleteCapabilityResponse, DennettError, DiscoverCapabilitiesRequest,
    DiscoverCapabilitiesResponse, ListCapabilitiesResponse, MAX_CAPABILITY_LIST_ITEMS,
    MAX_CAPABILITY_MARKDOWN_BYTES, PluginMcpServerEntry, ProjectId,
    SetProjectCapabilityBindingRequest, Sha256Hex, TimestampMs, UpdateCapabilityRequest,
};
use dennett_runtime_antigravity::{ActiveSkillPayload, RuntimeMcpEnvEntry, RuntimeMcpServerSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use tokio_postgres::Row;
use uuid::Uuid;

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::migrations::sha256_hex;
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{append_audit_event, fetch_project, map_pg_err};
use crate::workspace_files::{clean_display_path, is_symlink_or_reparse_point};

const CAPABILITY_SELECT_COLS: &str = "
    capability_id, slug, kind, display_name, description, version,
    origin_kind, ownership, scope_kind, project_id,
    possession_state, trust_state, activation_state,
    compatibility_status, compatibility_detail, source_path,
    content_sha256, spec_json, previous_snapshot_json IS NOT NULL, revision,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

/// Resolved active A08 capabilities materialized into a single runtime session turn.
#[derive(Debug, Default, Clone)]
pub(crate) struct ResolvedSessionCapabilities {
    /// Active skills materialized into `<gemini_dir>/config/skills/` or local prompt.
    pub(crate) active_skills: Vec<ActiveSkillPayload>,
    /// Active non-builtin MCP servers (with `SecretBroker` env slots already resolved).
    pub(crate) extra_mcp_servers: Vec<RuntimeMcpServerSpec>,
    /// Logical names of active A08 MCP servers recognized by `PolicyEngine`.
    pub(crate) registered_mcp_server_names: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapabilitySnapshot {
    display_name: String,
    description: String,
    version: String,
    scope_kind: CapabilityScopeKind,
    project_id: Option<ProjectId>,
    trust_state: CapabilityTrust,
    activation_state: CapabilityActivation,
    content_sha256: Sha256Hex,
    spec: CapabilitySpec,
}

#[derive(Debug, Clone)]
struct DiscoveredCandidate {
    slug: String,
    display_name: String,
    description: String,
    version: String,
    origin_kind: CapabilityOrigin,
    ownership: CapabilityOwnership,
    scope_kind: CapabilityScopeKind,
    project_id: Option<ProjectId>,
    source_path: String,
    spec: CapabilitySpec,
}

/// Computes a deterministic SHA-256 content digest for a capability's version + spec payload.
#[must_use]
pub fn compute_capability_sha256(version: &str, spec: &CapabilitySpec) -> Sha256Hex {
    let serialized = serde_json::to_string(spec).unwrap_or_default();
    let payload = format!("v1:{version}:{serialized}");
    sha256_hex(payload.as_bytes())
}

/// Returns `true` when transitioning from `old` to `new` expands the executable surface or secret
/// access footprint of a capability (Spec 30 §16.5 rug-pull protection).
#[must_use]
pub fn executable_surface_expanded(old: &CapabilitySpec, new: &CapabilitySpec) -> bool {
    match (old, new) {
        (
            CapabilitySpec::Skill {
                has_scripts: old_scripts,
                ..
            },
            CapabilitySpec::Skill {
                has_scripts: new_scripts,
                ..
            },
        ) => !*old_scripts && *new_scripts,
        (
            CapabilitySpec::McpServer {
                command: old_cmd,
                args: old_args,
                secret_slots: old_slots,
                ..
            },
            CapabilitySpec::McpServer {
                command: new_cmd,
                args: new_args,
                secret_slots: new_slots,
                ..
            },
        ) => {
            old_cmd != new_cmd
                || old_args != new_args
                || secret_slots_expanded(old_slots, new_slots)
        }
        (
            CapabilitySpec::Plugin {
                has_hooks: old_hooks,
                mcp_servers: old_mcps,
                ..
            },
            CapabilitySpec::Plugin {
                has_hooks: new_hooks,
                mcp_servers: new_mcps,
                ..
            },
        ) => {
            if !*old_hooks && *new_hooks {
                return true;
            }
            for new_srv in new_mcps {
                let Some(old_srv) = old_mcps.iter().find(|s| s.name == new_srv.name) else {
                    return true;
                };
                if old_srv.command != new_srv.command
                    || old_srv.args != new_srv.args
                    || secret_slots_expanded(&old_srv.secret_slots, &new_srv.secret_slots)
                {
                    return true;
                }
            }
            false
        }
        _ => true,
    }
}

fn secret_slots_expanded(
    old_slots: &BTreeMap<String, String>,
    new_slots: &BTreeMap<String, String>,
) -> bool {
    new_slots
        .iter()
        .any(|(env_key, slot_key)| old_slots.get(env_key) != Some(slot_key))
}

fn decode_capability_row(
    row: &Row,
    project_bindings: Vec<CapabilityProjectBinding>,
) -> Result<CapabilityRecord, DennettError> {
    let spec: CapabilitySpec =
        serde_json::from_value(row.get::<_, JsonValue>(17)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt capability_records.spec_json: {err}"),
            }
        })?;
    Ok(CapabilityRecord {
        capability_id: CapabilityId::from_uuid(row.get(0)),
        slug: row.get(1),
        kind: CapabilityKind::from_str(&row.get::<_, String>(2))?,
        display_name: row.get(3),
        description: row.get(4),
        version: row.get(5),
        origin_kind: CapabilityOrigin::from_str(&row.get::<_, String>(6))?,
        ownership: CapabilityOwnership::from_str(&row.get::<_, String>(7))?,
        scope_kind: CapabilityScopeKind::from_str(&row.get::<_, String>(8))?,
        project_id: row.get::<_, Option<Uuid>>(9).map(ProjectId::from_uuid),
        possession_state: CapabilityPossession::from_str(&row.get::<_, String>(10))?,
        trust_state: CapabilityTrust::from_str(&row.get::<_, String>(11))?,
        activation_state: CapabilityActivation::from_str(&row.get::<_, String>(12))?,
        compatibility_status: CompatibilityStatus::from_str(&row.get::<_, String>(13))?,
        compatibility_detail: row.get(14),
        source_path: row.get(15),
        content_sha256: Sha256Hex::try_new(row.get::<_, String>(16))?,
        spec,
        can_rollback: row.get(18),
        revision: row.get(19),
        project_bindings,
        created_at: TimestampMs::from_millis(row.get(20)),
        updated_at: TimestampMs::from_millis(row.get(21)),
    })
}

fn decode_binding_row(row: &Row) -> Result<CapabilityProjectBinding, DennettError> {
    Ok(CapabilityProjectBinding {
        capability_id: CapabilityId::from_uuid(row.get(0)),
        project_id: ProjectId::from_uuid(row.get(1)),
        activation_state: CapabilityActivation::from_str(&row.get::<_, String>(2))?,
        updated_at: TimestampMs::from_millis(row.get(3)),
    })
}

impl HeadService {
    /// Lists registered capabilities with optional `project_id`, `scope_kind`, `kind`, and `origin`
    /// filters (`GET /v1/capabilities`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    pub async fn list_capabilities(
        &self,
        project_id: Option<ProjectId>,
        scope_kind: Option<CapabilityScopeKind>,
        kind: Option<CapabilityKind>,
        origin: Option<CapabilityOrigin>,
    ) -> Result<ListCapabilitiesResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;

        let binding_rows = conn
            .query(
                "SELECT capability_id, project_id, activation_state,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM capability_project_bindings
                 ORDER BY updated_at DESC",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut bindings_by_cap: HashMap<CapabilityId, Vec<CapabilityProjectBinding>> =
            HashMap::new();
        for row in &binding_rows {
            let b = decode_binding_row(row)?;
            bindings_by_cap.entry(b.capability_id).or_default().push(b);
        }

        let sql = format!(
            "SELECT {CAPABILITY_SELECT_COLS}
             FROM capability_records
             WHERE ($1::uuid IS NULL OR project_id = $1 OR scope_kind IN ('global', 'candidate_unattached'))
               AND ($2::text IS NULL OR scope_kind = $2)
               AND ($3::text IS NULL OR kind = $3)
               AND ($4::text IS NULL OR origin_kind = $4)
             ORDER BY created_at DESC"
        );
        let rows = conn
            .query(
                &sql,
                &[
                    &project_id.map(ProjectId::as_uuid),
                    &scope_kind.map(|s| s.as_str().to_owned()),
                    &kind.map(|k| k.as_str().to_owned()),
                    &origin.map(|o| o.as_str().to_owned()),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut capabilities = Vec::with_capacity(rows.len());
        for row in &rows {
            let cid = CapabilityId::from_uuid(row.get(0));
            let b = bindings_by_cap.remove(&cid).unwrap_or_default();
            capabilities.push(decode_capability_row(row, b)?);
        }

        Ok(ListCapabilitiesResponse { capabilities })
    }

    /// Runs read-only discovery for project-local capabilities (`.agents/skills`, `.dennett/*`)
    /// and optionally the user's personal `~/.gemini/` profile (`POST /v1/capabilities/discover`).
    ///
    /// Personal findings are strictly read-only and ALWAYS registered with
    /// `origin_kind = PersonalDiscovered`, `scope_kind = CandidateUnattached`, and
    /// `activation_state = Disabled`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] if `req.project_id` does not exist, or
    /// [`DennettError::DatabaseUnavailable`] / [`DennettError::Internal`] on IO/DB failure.
    #[expect(
        clippy::too_many_lines,
        reason = "coordinates read-only project/personal filesystem scans and transactional idempotent upsert with rug-pull downgrade detection"
    )]
    pub async fn discover_capabilities(
        &self,
        req: DiscoverCapabilitiesRequest,
        correlation_id: &CorrelationId,
    ) -> Result<DiscoverCapabilitiesResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        let project_scan_target: Option<(ProjectId, PathBuf)> = if let Some(pid) = req.project_id {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let project = fetch_project(&conn, pid, self.pg_target()).await?;
            project
                .workspace_path
                .map(|ws| (project.project_id, PathBuf::from(ws)))
        } else {
            None
        };

        let personal_dir_opt = if req.scan_personal_gemini_readonly {
            self.resolve_personal_profile_dir()
        } else {
            None
        };

        let scan_personal = req.scan_personal_gemini_readonly;
        let (candidates, warnings) = tokio::task::spawn_blocking(move || {
            let mut found = Vec::new();
            let mut warns = Vec::new();

            if let Some((pid, ws_root)) = project_scan_target {
                scan_project_workspace_readonly(&ws_root, pid, &mut found, &mut warns);
            }
            if scan_personal {
                if let Some(personal_dir) = personal_dir_opt {
                    scan_personal_gemini_readonly(&personal_dir, &mut found, &mut warns);
                } else {
                    warns.push(String::from(
                        "personal ~/.gemini directory could not be resolved on this host",
                    ));
                }
            }
            (found, warns)
        })
        .await
        .map_err(|err| DennettError::Internal {
            detail: format!("capability discovery worker failed: {err}"),
        })?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut discovered = Vec::with_capacity(candidates.len());
        let mut newly_registered: u64 = 0;
        let mut updated_existing: u64 = 0;

        for cand in candidates {
            let content_sha256 = compute_capability_sha256(&cand.version, &cand.spec);
            let spec_json =
                serde_json::to_value(&cand.spec).map_err(|err| DennettError::Internal {
                    detail: format!("failed serializing discovered capability spec: {err}"),
                })?;

            let existing_sql = format!(
                "SELECT {CAPABILITY_SELECT_COLS}
                 FROM capability_records
                 WHERE slug = $1
                   AND kind = $2
                   AND scope_kind = $3
                   AND COALESCE(project_id, '00000000-0000-4000-8000-000000000000'::uuid)
                       = COALESCE($4::uuid, '00000000-0000-4000-8000-000000000000'::uuid)
                   AND origin_kind = $5
                 FOR UPDATE"
            );
            let existing_row = tx
                .query_opt(
                    &existing_sql,
                    &[
                        &cand.slug.as_str(),
                        &cand.spec.kind().as_str(),
                        &cand.scope_kind.as_str(),
                        &cand.project_id.map(ProjectId::as_uuid),
                        &cand.origin_kind.as_str(),
                    ],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            if let Some(existing_row) = existing_row {
                let existing = decode_capability_row(&existing_row, Vec::new())?;
                if existing.content_sha256 == content_sha256
                    && existing.version == cand.version
                    && existing.display_name == cand.display_name
                {
                    let bindings =
                        fetch_bindings_for_cap(&tx, existing.capability_id, self.pg_target())
                            .await?;
                    let mut rec = existing;
                    rec.project_bindings = bindings;
                    discovered.push(rec);
                    continue;
                }

                let snapshot = CapabilitySnapshot {
                    display_name: existing.display_name.clone(),
                    description: existing.description.clone(),
                    version: existing.version.clone(),
                    scope_kind: existing.scope_kind,
                    project_id: existing.project_id,
                    trust_state: existing.trust_state,
                    activation_state: existing.activation_state,
                    content_sha256: existing.content_sha256.clone(),
                    spec: existing.spec.clone(),
                };
                let snapshot_json =
                    serde_json::to_value(&snapshot).map_err(|err| DennettError::Internal {
                        detail: format!("failed serializing capability snapshot: {err}"),
                    })?;
                let next_trust = if executable_surface_expanded(&existing.spec, &cand.spec) {
                    CapabilityTrust::Restricted
                } else {
                    existing.trust_state
                };

                let update_sql = format!(
                    "UPDATE capability_records
                     SET display_name = $2,
                         description = $3,
                         version = $4,
                         trust_state = $5,
                         source_path = $6,
                         content_sha256 = $7,
                         spec_json = $8,
                         previous_snapshot_json = $9,
                         revision = revision + 1,
                         updated_at = now()
                     WHERE capability_id = $1
                     RETURNING {CAPABILITY_SELECT_COLS}"
                );
                let row = tx
                    .query_one(
                        &update_sql,
                        &[
                            &existing.capability_id.as_uuid(),
                            &cand.display_name.as_str(),
                            &cand.description.as_str(),
                            &cand.version.as_str(),
                            &next_trust.as_str(),
                            &Some(cand.source_path.as_str()),
                            &content_sha256.as_str(),
                            &spec_json,
                            &snapshot_json,
                        ],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                let bindings =
                    fetch_bindings_for_cap(&tx, existing.capability_id, self.pg_target()).await?;
                discovered.push(decode_capability_row(&row, bindings)?);
                updated_existing = updated_existing.saturating_add(1);
            } else {
                let cap_id = CapabilityId::new();
                let insert_sql = format!(
                    "INSERT INTO capability_records (
                        capability_id, slug, kind, display_name, description, version,
                        origin_kind, ownership, scope_kind, project_id,
                        possession_state, trust_state, activation_state,
                        compatibility_status, source_path, content_sha256, spec_json
                     ) VALUES (
                        $1, $2, $3, $4, $5, $6,
                        $7, $8, $9, $10,
                        'discovered', 'unreviewed', 'disabled',
                        'compatible', $11, $12, $13
                     )
                     RETURNING {CAPABILITY_SELECT_COLS}"
                );
                let row = tx
                    .query_one(
                        &insert_sql,
                        &[
                            &cap_id.as_uuid(),
                            &cand.slug.as_str(),
                            &cand.spec.kind().as_str(),
                            &cand.display_name.as_str(),
                            &cand.description.as_str(),
                            &cand.version.as_str(),
                            &cand.origin_kind.as_str(),
                            &cand.ownership.as_str(),
                            &cand.scope_kind.as_str(),
                            &cand.project_id.map(ProjectId::as_uuid),
                            &Some(cand.source_path.as_str()),
                            &content_sha256.as_str(),
                            &spec_json,
                        ],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                discovered.push(decode_capability_row(&row, Vec::new())?);
                newly_registered = newly_registered.saturating_add(1);
            }
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capabilities_discovered",
            &format!(
                "discovered {} capabilities (new={newly_registered}, updated={updated_existing}, personal_readonly={})",
                discovered.len(),
                req.scan_personal_gemini_readonly
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(DiscoverCapabilitiesResponse {
            discovered,
            newly_registered,
            updated_existing,
            warnings,
        })
    }

    /// Explicitly creates/registers a skill, MCP server, or plugin capability (`POST /v1/capabilities`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`] (missing project),
    /// [`DennettError::Conflict`] (duplicate identity), or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates capability spec, verifies project scope, inserts DB row, and appends audit event atomically"
    )]
    pub async fn create_capability(
        &self,
        req: CreateCapabilityRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CapabilityRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if let Some(pid) = req.project_id {
            let _ = fetch_project(&tx, pid, self.pg_target()).await?;
        }

        let cap_id = CapabilityId::new();
        let content_sha256 = compute_capability_sha256(&req.version, &req.spec);
        let spec_json = serde_json::to_value(&req.spec).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing capability spec: {err}"),
        })?;
        let possession = match req.activation_state {
            CapabilityActivation::Active | CapabilityActivation::Pinned => {
                CapabilityPossession::Connected
            }
            _ if req.scope_kind == CapabilityScopeKind::CandidateUnattached => {
                CapabilityPossession::Discovered
            }
            _ => CapabilityPossession::Installed,
        };

        let insert_sql = format!(
            "INSERT INTO capability_records (
                capability_id, slug, kind, display_name, description, version,
                origin_kind, ownership, scope_kind, project_id,
                possession_state, trust_state, activation_state,
                compatibility_status, source_path, content_sha256, spec_json
             ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13,
                'compatible', $14, $15, $16
             )
             RETURNING {CAPABILITY_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &insert_sql,
                &[
                    &cap_id.as_uuid(),
                    &req.slug.trim(),
                    &req.spec.kind().as_str(),
                    &req.display_name.trim(),
                    &req.description.trim(),
                    &req.version.trim(),
                    &req.origin_kind.as_str(),
                    &req.ownership.as_str(),
                    &req.scope_kind.as_str(),
                    &req.project_id.map(ProjectId::as_uuid),
                    &possession.as_str(),
                    &req.trust_state.as_str(),
                    &req.activation_state.as_str(),
                    &req.source_path.as_deref(),
                    &content_sha256.as_str(),
                    &spec_json,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let record = decode_capability_row(&row, Vec::new())?;
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capability_created",
            &format!(
                "registered {} capability `{}` v{} ({cap_id}) scope={} activation={}",
                record.kind,
                record.slug,
                record.version,
                record.scope_kind,
                record.activation_state
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("capability:{cap_id}"),
                event_type: "capability.created",
                project_id: record.project_id,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "capability_id": cap_id,
                    "slug": record.slug,
                    "kind": record.kind,
                    "version": record.version,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(record)
    }

    /// Updates a registered capability (`PATCH /v1/capabilities/{capability_id}`), saving a
    /// rollback snapshot and enforcing rug-pull trust downgrade to `Restricted` whenever the
    /// executable surface expands.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "atomically checks kind consistency, saves previous rollback snapshot, enforces rug-pull trust downgrade, updates record, and appends audit/journal events"
    )]
    pub async fn update_capability(
        &self,
        capability_id: CapabilityId,
        req: UpdateCapabilityRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CapabilityRecord, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let fetch_sql = format!(
            "SELECT {CAPABILITY_SELECT_COLS}
             FROM capability_records
             WHERE capability_id = $1
             FOR UPDATE"
        );
        let existing_row = tx
            .query_opt(&fetch_sql, &[&capability_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(existing_row) = existing_row else {
            return Err(DennettError::NotFound {
                resource: String::from("capability"),
                id: capability_id.to_string(),
            });
        };
        let existing = decode_capability_row(&existing_row, Vec::new())?;

        if let Some(ref new_spec) = req.spec
            && new_spec.kind() != existing.kind
        {
            return Err(DennettError::InvalidInput {
                field: String::from("spec"),
                reason: format!(
                    "cannot change capability kind from `{}` to `{}`",
                    existing.kind,
                    new_spec.kind()
                ),
            });
        }

        let next_scope_kind = req.scope_kind.unwrap_or(existing.scope_kind);
        let next_project_id = match next_scope_kind {
            CapabilityScopeKind::Project => {
                let pid = req.project_id.or(existing.project_id).ok_or_else(|| {
                    DennettError::InvalidInput {
                        field: String::from("project_id"),
                        reason: String::from(
                            "project_id is required when `scope_kind` is `project`",
                        ),
                    }
                })?;
                let _ = fetch_project(&tx, pid, self.pg_target()).await?;
                Some(pid)
            }
            CapabilityScopeKind::Global | CapabilityScopeKind::CandidateUnattached => None,
            _ => req.project_id.or(existing.project_id),
        };

        let next_display_name = req
            .display_name
            .as_deref()
            .map_or(existing.display_name.as_str(), str::trim)
            .to_owned();
        let next_description = req
            .description
            .as_deref()
            .map_or(existing.description.as_str(), str::trim)
            .to_owned();
        let next_version = req
            .version
            .as_deref()
            .map_or(existing.version.as_str(), str::trim)
            .to_owned();
        let next_spec = req.spec.clone().unwrap_or_else(|| existing.spec.clone());
        let next_sha256 = compute_capability_sha256(&next_version, &next_spec);

        let surface_expanded =
            req.spec.is_some() && executable_surface_expanded(&existing.spec, &next_spec);

        // Rug-pull protection: expanding executable surface (scripts/hooks/command/secret_slots)
        // ALWAYS resets trust_state to Restricted until the owner explicitly re-trusts in a
        // subsequent review action.
        let next_trust_state = if surface_expanded {
            CapabilityTrust::Restricted
        } else {
            req.trust_state.unwrap_or(existing.trust_state)
        };
        let next_activation_state = req.activation_state.unwrap_or(existing.activation_state);
        if next_trust_state == CapabilityTrust::Revoked
            && matches!(
                next_activation_state,
                CapabilityActivation::Active | CapabilityActivation::Pinned
            )
        {
            return Err(DennettError::Conflict {
                resource: String::from("capability"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot activate a revoked capability"),
            });
        }

        let next_possession = match next_activation_state {
            CapabilityActivation::Active | CapabilityActivation::Pinned => {
                CapabilityPossession::Connected
            }
            _ if next_scope_kind != CapabilityScopeKind::CandidateUnattached
                && existing.possession_state == CapabilityPossession::Discovered =>
            {
                CapabilityPossession::Installed
            }
            _ => existing.possession_state,
        };

        let save_snapshot = req.version.is_some()
            || req.spec.is_some()
            || req.display_name.is_some()
            || req.description.is_some()
            || req.scope_kind.is_some();
        let snapshot_json_opt: Option<JsonValue> = if save_snapshot {
            let snap = CapabilitySnapshot {
                display_name: existing.display_name.clone(),
                description: existing.description.clone(),
                version: existing.version.clone(),
                scope_kind: existing.scope_kind,
                project_id: existing.project_id,
                trust_state: existing.trust_state,
                activation_state: existing.activation_state,
                content_sha256: existing.content_sha256.clone(),
                spec: existing.spec.clone(),
            };
            Some(
                serde_json::to_value(&snap).map_err(|err| DennettError::Internal {
                    detail: format!("failed serializing rollback snapshot: {err}"),
                })?,
            )
        } else {
            None
        };
        let spec_json = serde_json::to_value(&next_spec).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing capability spec: {err}"),
        })?;

        let update_sql = format!(
            "UPDATE capability_records
             SET display_name = $2,
                 description = $3,
                 version = $4,
                 scope_kind = $5,
                 project_id = $6,
                 possession_state = $7,
                 trust_state = $8,
                 activation_state = $9,
                 content_sha256 = $10,
                 spec_json = $11,
                 previous_snapshot_json = COALESCE($12, previous_snapshot_json),
                 revision = revision + 1,
                 updated_at = now()
             WHERE capability_id = $1
             RETURNING {CAPABILITY_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &update_sql,
                &[
                    &capability_id.as_uuid(),
                    &next_display_name.as_str(),
                    &next_description.as_str(),
                    &next_version.as_str(),
                    &next_scope_kind.as_str(),
                    &next_project_id.map(ProjectId::as_uuid),
                    &next_possession.as_str(),
                    &next_trust_state.as_str(),
                    &next_activation_state.as_str(),
                    &next_sha256.as_str(),
                    &spec_json,
                    &snapshot_json_opt,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let bindings = fetch_bindings_for_cap(&tx, capability_id, self.pg_target()).await?;
        let record = decode_capability_row(&row, bindings)?;

        if surface_expanded {
            let _ = append_audit_event(
                &tx,
                self.pg_target(),
                correlation_id,
                None,
                None,
                "capabilities",
                "capability_trust_downgraded",
                &format!(
                    "rug-pull guard downgraded `{}` ({capability_id}) trust_state to `restricted` after executable surface expansion",
                    record.slug
                ),
            )
            .await?;
        }
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capability_updated",
            &format!(
                "updated capability `{}` ({capability_id}) v{} rev={} trust={} activation={}",
                record.slug,
                record.version,
                record.revision,
                record.trust_state,
                record.activation_state
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("capability:{capability_id}"),
                event_type: "capability.updated",
                project_id: record.project_id,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "capability_id": capability_id,
                    "slug": record.slug,
                    "version": record.version,
                    "revision": record.revision,
                    "trust_state": record.trust_state,
                    "activation_state": record.activation_state,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(record)
    }

    /// Rolls a capability back to its stored `previous_snapshot_json`
    /// (`POST /v1/capabilities/{capability_id}/rollback`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`] if no snapshot exists,
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn rollback_capability(
        &self,
        capability_id: CapabilityId,
        correlation_id: &CorrelationId,
    ) -> Result<CapabilityRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let row = tx
            .query_opt(
                "SELECT slug, previous_snapshot_json
                 FROM capability_records
                 WHERE capability_id = $1
                 FOR UPDATE",
                &[&capability_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("capability"),
                id: capability_id.to_string(),
            });
        };
        let slug: String = row.get(0);
        let prev_val: Option<JsonValue> = row.get(1);
        let Some(prev_val) = prev_val else {
            return Err(DennettError::Conflict {
                resource: String::from("capability"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "capability `{slug}` has no previous snapshot available for rollback"
                ),
            });
        };
        let snap: CapabilitySnapshot =
            serde_json::from_value(prev_val).map_err(|err| DennettError::Internal {
                detail: format!("corrupt capability previous_snapshot_json: {err}"),
            })?;
        let spec_json = serde_json::to_value(&snap.spec).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing rollback spec: {err}"),
        })?;

        let update_sql = format!(
            "UPDATE capability_records
             SET display_name = $2,
                 description = $3,
                 version = $4,
                 scope_kind = $5,
                 project_id = $6,
                 trust_state = $7,
                 activation_state = $8,
                 content_sha256 = $9,
                 spec_json = $10,
                 previous_snapshot_json = NULL,
                 revision = revision + 1,
                 updated_at = now()
             WHERE capability_id = $1
             RETURNING {CAPABILITY_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &capability_id.as_uuid(),
                    &snap.display_name.as_str(),
                    &snap.description.as_str(),
                    &snap.version.as_str(),
                    &snap.scope_kind.as_str(),
                    &snap.project_id.map(ProjectId::as_uuid),
                    &snap.trust_state.as_str(),
                    &snap.activation_state.as_str(),
                    &snap.content_sha256.as_str(),
                    &spec_json,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let bindings = fetch_bindings_for_cap(&tx, capability_id, self.pg_target()).await?;
        let record = decode_capability_row(&updated_row, bindings)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capability_rolled_back",
            &format!(
                "rolled back capability `{}` ({capability_id}) to v{} rev={}",
                record.slug, record.version, record.revision
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(record)
    }

    /// Removes a capability record from Dennett's PostgreSQL registry WITHOUT deleting any user
    /// files on disk (`DELETE /v1/capabilities/{capability_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn delete_capability(
        &self,
        capability_id: CapabilityId,
        correlation_id: &CorrelationId,
    ) -> Result<DeleteCapabilityResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let deleted = tx
            .execute(
                "DELETE FROM capability_records WHERE capability_id = $1",
                &[&capability_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        if deleted == 0 {
            return Err(DennettError::NotFound {
                resource: String::from("capability"),
                id: capability_id.to_string(),
            });
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capability_deleted",
            &format!("deleted capability registry record {capability_id}"),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(DeleteCapabilityResponse {
            capability_id,
            deleted: true,
        })
    }

    /// Sets or updates a per-project capability activation override
    /// (`PUT /v1/projects/{project_id}/capabilities/{capability_id}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`] (if revoked/restricted),
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn set_project_capability_binding(
        &self,
        project_id: ProjectId,
        capability_id: CapabilityId,
        req: SetProjectCapabilityBindingRequest,
        correlation_id: &CorrelationId,
    ) -> Result<CapabilityProjectBinding, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let _ = fetch_project(&tx, project_id, self.pg_target()).await?;
        let cap_row = tx
            .query_opt(
                "SELECT slug, trust_state FROM capability_records WHERE capability_id = $1",
                &[&capability_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(cap_row) = cap_row else {
            return Err(DennettError::NotFound {
                resource: String::from("capability"),
                id: capability_id.to_string(),
            });
        };
        let slug: String = cap_row.get(0);
        let trust = CapabilityTrust::from_str(&cap_row.get::<_, String>(1))?;

        if matches!(
            req.activation_state,
            CapabilityActivation::Active | CapabilityActivation::Pinned
        ) && matches!(
            trust,
            CapabilityTrust::Revoked | CapabilityTrust::Restricted
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("capability"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "cannot activate `{slug}` in project {project_id}: capability trust_state is `{trust}`"
                ),
            });
        }

        let row = tx
            .query_one(
                "INSERT INTO capability_project_bindings (capability_id, project_id, activation_state)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (capability_id, project_id)
                 DO UPDATE SET activation_state = EXCLUDED.activation_state, updated_at = now()
                 RETURNING capability_id, project_id, activation_state,
                           TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
                &[
                    &capability_id.as_uuid(),
                    &project_id.as_uuid(),
                    &req.activation_state.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let binding = decode_binding_row(&row)?;

        if matches!(
            req.activation_state,
            CapabilityActivation::Active | CapabilityActivation::Pinned
        ) {
            let _ = tx
                .execute(
                    "UPDATE capability_records
                     SET possession_state = 'connected', updated_at = now()
                     WHERE capability_id = $1 AND possession_state <> 'connected'",
                    &[&capability_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "capabilities",
            "capability_project_bound",
            &format!(
                "bound capability `{slug}` ({capability_id}) to project {project_id} state={}",
                binding.activation_state
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(binding)
    }

    /// Resolves all active A08 skills and MCP servers for an upcoming runtime turn, strictly
    /// enforcing:
    /// - `CandidateUnattached` (including `PersonalDiscovered` from `~/.gemini/`) is NEVER active
    ///   unless explicitly bound to `project_id` via `capability_project_bindings`.
    /// - `Revoked`, `Restricted` (rug-pulled), and `Incompatible` capabilities are NEVER active.
    /// - Per-project `Forbidden` / `Disabled` overrides always suppress global capabilities.
    /// - `secret_slots` on active MCP servers are resolved via `SecretBroker` and registered in
    ///   Head's live secret redactor before returning.
    #[expect(
        clippy::too_many_lines,
        reason = "resolves trust/scope/project overrides, deduplicates skills/MCPs, and binds SecretBroker slots in one pass"
    )]
    pub(crate) async fn resolve_active_capabilities_for_session(
        &self,
        project_id: Option<ProjectId>,
        agent_attached_skills: &[String],
    ) -> Result<ResolvedSessionCapabilities, DennettError> {
        let listed = self.list_capabilities(project_id, None, None, None).await?;
        let attached_set: BTreeSet<&str> =
            agent_attached_skills.iter().map(String::as_str).collect();

        let mut out = ResolvedSessionCapabilities::default();
        let mut seen_skill_slugs: BTreeSet<String> = BTreeSet::new();
        let mut seen_mcp_names: BTreeSet<String> = BTreeSet::new();

        for cap in listed.capabilities {
            if matches!(
                cap.trust_state,
                CapabilityTrust::Revoked | CapabilityTrust::Restricted
            ) || cap.compatibility_status == CompatibilityStatus::Incompatible
            {
                continue;
            }

            let project_override = project_id.and_then(|pid| {
                cap.project_bindings
                    .iter()
                    .find(|b| b.project_id == pid)
                    .map(|b| b.activation_state)
            });

            let effective_activation = match project_override {
                Some(override_state) => override_state,
                None => match cap.scope_kind {
                    CapabilityScopeKind::Project => {
                        if cap.project_id == project_id && project_id.is_some() {
                            cap.activation_state
                        } else {
                            CapabilityActivation::Disabled
                        }
                    }
                    CapabilityScopeKind::Global => cap.activation_state,
                    _ => CapabilityActivation::Disabled,
                },
            };

            let should_materialize = match effective_activation {
                CapabilityActivation::Active | CapabilityActivation::Pinned => true,
                CapabilityActivation::OnDemand => attached_set.contains(cap.slug.as_str()),
                _ => false,
            };
            if !should_materialize {
                continue;
            }

            match cap.spec {
                CapabilitySpec::Skill { skill_markdown, .. } => {
                    if seen_skill_slugs.insert(cap.slug.clone()) {
                        out.active_skills.push(ActiveSkillPayload {
                            slug: cap.slug,
                            version: cap.version,
                            content_sha256: cap.content_sha256.as_str().to_owned(),
                            skill_markdown,
                        });
                    }
                }
                CapabilitySpec::McpServer {
                    command,
                    args,
                    env_vars,
                    secret_slots,
                } => {
                    if !matches!(cap.slug.as_str(), "dennett-memory" | "dennett-control")
                        && seen_mcp_names.insert(cap.slug.clone())
                    {
                        let spec = self
                            .build_capability_mcp_spec(
                                &cap.slug,
                                &command,
                                &args,
                                &env_vars,
                                &secret_slots,
                            )
                            .await?;
                        out.registered_mcp_server_names.insert(cap.slug);
                        out.extra_mcp_servers.push(spec);
                    }
                }
                CapabilitySpec::Plugin {
                    mcp_servers,
                    enabled_components,
                    ..
                } => {
                    let filter_components = !enabled_components.is_empty();
                    for srv in mcp_servers {
                        if filter_components && !enabled_components.iter().any(|c| c == &srv.name) {
                            continue;
                        }
                        if matches!(srv.name.as_str(), "dennett-memory" | "dennett-control") {
                            continue;
                        }
                        if seen_mcp_names.insert(srv.name.clone()) {
                            let spec = self
                                .build_capability_mcp_spec(
                                    &srv.name,
                                    &srv.command,
                                    &srv.args,
                                    &srv.env_vars,
                                    &srv.secret_slots,
                                )
                                .await?;
                            out.registered_mcp_server_names.insert(srv.name);
                            out.extra_mcp_servers.push(spec);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(out)
    }

    async fn build_capability_mcp_spec(
        &self,
        name: &str,
        command: &str,
        args: &[String],
        env_vars: &BTreeMap<String, String>,
        secret_slots: &BTreeMap<String, String>,
    ) -> Result<RuntimeMcpServerSpec, DennettError> {
        let mut env = Vec::with_capacity(env_vars.len() + secret_slots.len());
        for (k, v) in env_vars {
            env.push(RuntimeMcpEnvEntry {
                name: k.clone(),
                value: v.clone(),
            });
        }
        for (env_name, slot_key) in secret_slots {
            if let Some((plaintext, _, _)) = self.resolve_secret_plaintext(slot_key).await? {
                self.register_secret_for_redaction(&plaintext);
                env.push(RuntimeMcpEnvEntry {
                    name: env_name.clone(),
                    value: plaintext.to_string(),
                });
            }
        }
        Ok(RuntimeMcpServerSpec {
            name: name.to_owned(),
            command: command.to_owned(),
            args: args.to_vec(),
            env,
        })
    }
}

async fn fetch_bindings_for_cap<C: deadpool_postgres::GenericClient>(
    client: &C,
    capability_id: CapabilityId,
    target: &crate::pg_target::ValidatedControlPgTarget,
) -> Result<Vec<CapabilityProjectBinding>, DennettError> {
    let rows = client
        .query(
            "SELECT capability_id, project_id, activation_state,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM capability_project_bindings
             WHERE capability_id = $1
             ORDER BY updated_at DESC",
            &[&capability_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(target, &err))?;
    rows.iter().map(decode_binding_row).collect()
}

fn scan_project_workspace_readonly(
    ws_root: &Path,
    project_id: ProjectId,
    out: &mut Vec<DiscoveredCandidate>,
    warnings: &mut Vec<String>,
) {
    for skills_dir in [
        ws_root.join(".agents").join("skills"),
        ws_root.join(".dennett").join("skills"),
    ] {
        scan_skills_dir_readonly(
            &skills_dir,
            CapabilityOrigin::ProjectLocal,
            CapabilityOwnership::Project,
            CapabilityScopeKind::Project,
            Some(project_id),
            out,
            warnings,
        );
    }

    scan_mcp_json_readonly(
        &ws_root.join(".dennett").join("mcp.json"),
        CapabilityOrigin::ProjectLocal,
        CapabilityOwnership::Project,
        CapabilityScopeKind::Project,
        Some(project_id),
        out,
        warnings,
    );

    scan_plugins_dir_readonly(
        &ws_root.join(".dennett").join("plugins"),
        CapabilityOrigin::ProjectLocal,
        CapabilityOwnership::Project,
        CapabilityScopeKind::Project,
        Some(project_id),
        out,
        warnings,
    );
}

fn scan_personal_gemini_readonly(
    personal_dir: &Path,
    out: &mut Vec<DiscoveredCandidate>,
    warnings: &mut Vec<String>,
) {
    if !personal_dir.exists() {
        return;
    }
    for skills_dir in [
        personal_dir.join("config").join("skills"),
        personal_dir.join("skills"),
    ] {
        scan_skills_dir_readonly(
            &skills_dir,
            CapabilityOrigin::PersonalDiscovered,
            CapabilityOwnership::User,
            CapabilityScopeKind::CandidateUnattached,
            None,
            out,
            warnings,
        );
    }

    for mcp_file in [
        personal_dir.join("config").join("mcp.json"),
        personal_dir.join("settings.json"),
    ] {
        scan_mcp_json_readonly(
            &mcp_file,
            CapabilityOrigin::PersonalDiscovered,
            CapabilityOwnership::User,
            CapabilityScopeKind::CandidateUnattached,
            None,
            out,
            warnings,
        );
    }

    scan_plugins_dir_readonly(
        &personal_dir.join("plugins"),
        CapabilityOrigin::PersonalDiscovered,
        CapabilityOwnership::User,
        CapabilityScopeKind::CandidateUnattached,
        None,
        out,
        warnings,
    );
}

fn scan_skills_dir_readonly(
    skills_root: &Path,
    origin_kind: CapabilityOrigin,
    ownership: CapabilityOwnership,
    scope_kind: CapabilityScopeKind,
    project_id: Option<ProjectId>,
    out: &mut Vec<DiscoveredCandidate>,
    warnings: &mut Vec<String>,
) {
    let Ok(meta) = fs::symlink_metadata(skills_root) else {
        return;
    };
    if is_symlink_or_reparse_point(&meta) || !meta.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(skills_root) else {
        warnings.push(format!(
            "could not read skills directory `{}`",
            clean_display_path(skills_root)
        ));
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ent_meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_symlink_or_reparse_point(&ent_meta) || !ent_meta.is_dir() {
            continue;
        }
        let skill_md_path = path.join("SKILL.md");
        let Ok(md_meta) = fs::symlink_metadata(&skill_md_path) else {
            continue;
        };
        if is_symlink_or_reparse_point(&md_meta) || !md_meta.is_file() {
            continue;
        }
        if md_meta.len() > MAX_CAPABILITY_MARKDOWN_BYTES as u64 {
            warnings.push(format!(
                "skipped oversized SKILL.md at `{}`",
                clean_display_path(&skill_md_path)
            ));
            continue;
        }
        let Ok(markdown) = fs::read_to_string(&skill_md_path) else {
            continue;
        };
        if markdown.trim().is_empty() {
            continue;
        }

        let raw_folder = entry.file_name().to_string_lossy().into_owned();
        let slug = sanitize_discovered_slug(&raw_folder);
        let (front_name, front_desc, front_ver) = parse_skill_frontmatter(&markdown);
        let display_name = front_name.unwrap_or_else(|| slug.clone());
        let description = front_desc.unwrap_or_default();
        let version = front_ver.unwrap_or_else(|| String::from("1.0.0"));

        let (has_scripts, entry_files) = inspect_skill_bundle_readonly(&path);
        out.push(DiscoveredCandidate {
            slug,
            display_name,
            description,
            version,
            origin_kind,
            ownership,
            scope_kind,
            project_id,
            source_path: clean_display_path(&path),
            spec: CapabilitySpec::Skill {
                skill_markdown: markdown,
                has_scripts,
                entry_files,
            },
        });
    }
}

fn inspect_skill_bundle_readonly(skill_dir: &Path) -> (bool, Vec<String>) {
    let mut has_scripts = false;
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(skill_dir) else {
        return (false, files);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if is_symlink_or_reparse_point(&meta) {
            continue;
        }
        if meta.is_dir() && name.eq_ignore_ascii_case("scripts") {
            has_scripts = true;
        }
        if Path::new(&name).extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("sh")
                || ext.eq_ignore_ascii_case("ps1")
                || ext.eq_ignore_ascii_case("bat")
                || ext.eq_ignore_ascii_case("cmd")
                || ext.eq_ignore_ascii_case("py")
                || ext.eq_ignore_ascii_case("js")
                || ext.eq_ignore_ascii_case("ts")
        }) {
            has_scripts = true;
        }
        if files.len() < MAX_CAPABILITY_LIST_ITEMS {
            files.push(name);
        }
    }
    files.sort();
    (has_scripts, files)
}

fn scan_mcp_json_readonly(
    file_path: &Path,
    origin_kind: CapabilityOrigin,
    ownership: CapabilityOwnership,
    scope_kind: CapabilityScopeKind,
    project_id: Option<ProjectId>,
    out: &mut Vec<DiscoveredCandidate>,
    warnings: &mut Vec<String>,
) {
    let Ok(meta) = fs::symlink_metadata(file_path) else {
        return;
    };
    if is_symlink_or_reparse_point(&meta) || !meta.is_file() || meta.len() > 512 * 1024 {
        return;
    }
    let Ok(raw) = fs::read_to_string(file_path) else {
        return;
    };
    let Ok(parsed) = serde_json::from_str::<JsonValue>(&raw) else {
        warnings.push(format!(
            "could not parse MCP JSON `{}`",
            clean_display_path(file_path)
        ));
        return;
    };

    let Some(servers) = parsed.get("mcpServers").and_then(JsonValue::as_object) else {
        return;
    };

    for (raw_name, cfg) in servers {
        let Some(command) = cfg
            .get("command")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let args: Vec<String> = cfg
            .get("args")
            .and_then(JsonValue::as_array)
            .map_or_default(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .take(MAX_CAPABILITY_LIST_ITEMS)
                    .collect()
            });
        let mut env_vars = BTreeMap::new();
        let mut secret_slots = BTreeMap::new();
        if let Some(env_obj) = cfg.get("env").and_then(JsonValue::as_object) {
            for (k, v) in env_obj {
                if let Some(val_str) = v.as_str() {
                    let upper = k.to_ascii_uppercase();
                    if upper.contains("SECRET")
                        || upper.contains("TOKEN")
                        || upper.contains("KEY")
                        || upper.contains("PASSWORD")
                    {
                        // Never copy personal plaintext secrets into capability_records;
                        // map them into a SecretBroker slot reference instead!
                        let slot = format!(
                            "mcp:{}:{}",
                            sanitize_discovered_slug(raw_name),
                            sanitize_discovered_slug(k)
                        );
                        secret_slots.insert(k.clone(), slot);
                    } else {
                        env_vars.insert(k.clone(), val_str.to_owned());
                    }
                }
            }
        }
        if let Some(slots_obj) = cfg
            .get("secret_slots")
            .or_else(|| cfg.get("secretSlots"))
            .and_then(JsonValue::as_object)
        {
            for (k, v) in slots_obj {
                if let Some(slot_str) = v.as_str()
                    && dennett_contracts::validate_secret_slot_key(slot_str).is_ok()
                {
                    secret_slots.insert(k.clone(), slot_str.to_owned());
                }
            }
        }

        let slug = sanitize_discovered_slug(raw_name);
        out.push(DiscoveredCandidate {
            slug: slug.clone(),
            display_name: raw_name.clone(),
            description: format!(
                "MCP server discovered from {}",
                clean_display_path(file_path)
            ),
            version: String::from("1.0.0"),
            origin_kind,
            ownership,
            scope_kind,
            project_id,
            source_path: clean_display_path(file_path),
            spec: CapabilitySpec::McpServer {
                command: command.to_owned(),
                args,
                env_vars,
                secret_slots,
            },
        });
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses plugin manifests, nested skills/MCP/hooks descriptors, and compatibility flags strictly read-only"
)]
fn scan_plugins_dir_readonly(
    plugins_root: &Path,
    origin_kind: CapabilityOrigin,
    ownership: CapabilityOwnership,
    scope_kind: CapabilityScopeKind,
    project_id: Option<ProjectId>,
    out: &mut Vec<DiscoveredCandidate>,
    warnings: &mut Vec<String>,
) {
    let Ok(meta) = fs::symlink_metadata(plugins_root) else {
        return;
    };
    if is_symlink_or_reparse_point(&meta) || !meta.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(plugins_root) else {
        return;
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(ent_meta) = fs::symlink_metadata(&dir) else {
            continue;
        };
        if is_symlink_or_reparse_point(&ent_meta) || !ent_meta.is_dir() {
            continue;
        }
        let manifest_path = if dir.join("plugin.json").is_file() {
            dir.join("plugin.json")
        } else if dir.join("manifest.json").is_file() {
            dir.join("manifest.json")
        } else {
            continue;
        };
        let Ok(raw) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<JsonValue>(&raw) else {
            warnings.push(format!(
                "could not parse plugin manifest `{}`",
                clean_display_path(&manifest_path)
            ));
            continue;
        };

        let raw_name = entry.file_name().to_string_lossy().into_owned();
        let slug = sanitize_discovered_slug(
            manifest
                .get("slug")
                .or_else(|| manifest.get("name"))
                .and_then(JsonValue::as_str)
                .unwrap_or(&raw_name),
        );
        let display_name = manifest
            .get("display_name")
            .or_else(|| manifest.get("name"))
            .and_then(JsonValue::as_str)
            .unwrap_or(&raw_name)
            .to_owned();
        let description = manifest
            .get("description")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_owned();
        let version = manifest
            .get("version")
            .and_then(JsonValue::as_str)
            .unwrap_or("1.0.0")
            .to_owned();
        let manifest_version = manifest
            .get("manifest_version")
            .and_then(JsonValue::as_str)
            .unwrap_or("1.0")
            .to_owned();
        let skills: Vec<String> = manifest
            .get("skills")
            .and_then(JsonValue::as_array)
            .map_or_default(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(sanitize_discovered_slug))
                    .take(MAX_CAPABILITY_LIST_ITEMS)
                    .collect()
            });
        let has_hooks = manifest
            .get("has_hooks")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || manifest.get("hooks").is_some()
            || dir.join("hooks.json").exists();

        let mut mcp_servers = Vec::new();
        if let Some(arr) = manifest.get("mcp_servers").and_then(JsonValue::as_array) {
            for item in arr.iter().take(MAX_CAPABILITY_LIST_ITEMS) {
                if let Ok(entry_spec) = serde_json::from_value::<PluginMcpServerEntry>(item.clone())
                    && entry_spec.validate().is_ok()
                {
                    mcp_servers.push(entry_spec);
                }
            }
        }

        out.push(DiscoveredCandidate {
            slug,
            display_name,
            description,
            version,
            origin_kind,
            ownership,
            scope_kind,
            project_id,
            source_path: clean_display_path(&dir),
            spec: CapabilitySpec::Plugin {
                manifest_version,
                skills,
                mcp_servers,
                has_hooks,
                enabled_components: Vec::new(),
            },
        });
    }
}

fn sanitize_discovered_slug(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 96 {
            break;
        }
    }
    let cleaned = out.trim_matches(['-', '.']).replace("..", "-");
    if cleaned.is_empty() {
        String::from("discovered-capability")
    } else {
        cleaned
    }
}

fn parse_skill_frontmatter(markdown: &str) -> (Option<String>, Option<String>, Option<String>) {
    let trimmed = markdown.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (None, None, None);
    };
    let Some((header, _)) = rest.split_once("\n---") else {
        return (None, None, None);
    };
    let mut name = None;
    let mut desc = None;
    let mut version = None;
    for line in header.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let val = v.trim().trim_matches(['"', '\'']).to_owned();
        if val.is_empty() {
            continue;
        }
        match k.trim().to_ascii_lowercase().as_str() {
            "name" | "title" => name = Some(val),
            "description" => desc = Some(val),
            "version" => version = Some(val),
            _ => {}
        }
    }
    (name, desc, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rug_pull_detector_flags_scripts_hooks_command_and_secret_slot_expansion() {
        let skill_safe = CapabilitySpec::Skill {
            skill_markdown: String::from("# Safe"),
            has_scripts: false,
            entry_files: vec![String::from("SKILL.md")],
        };
        let skill_md_edit = CapabilitySpec::Skill {
            skill_markdown: String::from("# Updated docs"),
            has_scripts: false,
            entry_files: vec![String::from("SKILL.md")],
        };
        let skill_with_script = CapabilitySpec::Skill {
            skill_markdown: String::from("# Now runs scripts"),
            has_scripts: true,
            entry_files: vec![String::from("SKILL.md"), String::from("run.sh")],
        };
        assert!(!executable_surface_expanded(&skill_safe, &skill_md_edit));
        assert!(executable_surface_expanded(&skill_safe, &skill_with_script));

        let mcp_v1 = CapabilitySpec::McpServer {
            command: String::from("my-mcp"),
            args: vec![String::from("--read-only")],
            env_vars: BTreeMap::new(),
            secret_slots: BTreeMap::new(),
        };
        let mcp_v2_secret = CapabilitySpec::McpServer {
            command: String::from("my-mcp"),
            args: vec![String::from("--read-only")],
            env_vars: BTreeMap::new(),
            secret_slots: BTreeMap::from([(String::from("API_KEY"), String::from("mcp:my:key"))]),
        };
        assert!(executable_surface_expanded(&mcp_v1, &mcp_v2_secret));

        let plugin_v1 = CapabilitySpec::Plugin {
            manifest_version: String::from("1.0"),
            skills: vec![String::from("s1")],
            mcp_servers: Vec::new(),
            has_hooks: false,
            enabled_components: Vec::new(),
        };
        let plugin_v2_hooks = CapabilitySpec::Plugin {
            manifest_version: String::from("1.0"),
            skills: vec![String::from("s1")],
            mcp_servers: Vec::new(),
            has_hooks: true,
            enabled_components: Vec::new(),
        };
        assert!(executable_surface_expanded(&plugin_v1, &plugin_v2_hooks));
    }
}
