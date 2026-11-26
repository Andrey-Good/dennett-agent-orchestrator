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

use core::fmt::Write as _;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use dennett_contracts::{
    CapabilityActivation, CapabilityId, CapabilityKind, CapabilityOrigin, CapabilityOwnership,
    CapabilityPossession, CapabilityProjectBinding, CapabilityRecord, CapabilityScopeKind,
    CapabilitySpec, CapabilityTrust, CompatibilityStatus, CorrelationId, CreateCapabilityRequest,
    DeleteCapabilityResponse, DennettError, DiscoverCapabilitiesRequest,
    DiscoverCapabilitiesResponse, ListCapabilitiesResponse, MAX_CAPABILITY_DESCRIPTION_BYTES,
    MAX_CAPABILITY_LIST_ITEMS, MAX_CAPABILITY_MARKDOWN_BYTES, MAX_CAPABILITY_NAME_LEN,
    MAX_CAPABILITY_VERSION_LEN, PluginMcpServerEntry, ProjectId, SessionSkillDetailResponse,
    SessionSkillSummary, SetProjectCapabilityBindingRequest, Sha256Hex, TimestampMs,
    UpdateCapabilityRequest,
};
use dennett_runtime_antigravity::{ActiveSkillPayload, RuntimeMcpEnvEntry, RuntimeMcpServerSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use tokio_postgres::Row;
use uuid::Uuid;

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::memory_auth::ControlTokenClaims;
use crate::migrations::sha256_hex;
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{append_audit_event, fetch_project, fetch_session, map_pg_err, truncate_utf8};
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

/// Maximum bytes inlined directly into `[Dennett Active Skills Index]` for a `pinned` skill (16 KiB).
pub(crate) const MAX_PINNED_SKILL_INLINE_BYTES: usize = 16 * 1024;

/// One enabled Dennett skill resolved for a concrete session (project or standalone).
#[derive(Debug, Clone)]
pub(crate) struct ResolvedSessionSkill {
    /// Public wire summary returned by `GET /v1/capabilities/session-skills`.
    pub(crate) summary: SessionSkillSummary,
    /// Full UTF-8 `SKILL.md` markdown persisted on `capability_records.spec_json`.
    pub(crate) skill_markdown: String,
    /// Optional validated read-only source path on disk.
    pub(crate) source_path: Option<String>,
    /// Lowercase SHA-256 content digest.
    pub(crate) content_sha256: String,
}

/// Resolved active A08 capabilities materialized into a single runtime session turn.
#[derive(Debug, Default, Clone)]
pub(crate) struct ResolvedSessionCapabilities {
    /// Active skills materialized into `<gemini_dir>/config/skills/` or local prompt.
    pub(crate) active_skills: Vec<ActiveSkillPayload>,
    /// All enabled skills (`pinned`, `active`, `on_demand`) for provider-independent prompt index delivery.
    pub(crate) session_skills: Vec<ResolvedSessionSkill>,
    /// Active non-builtin MCP servers (with `SecretBroker` env slots already resolved).
    pub(crate) extra_mcp_servers: Vec<RuntimeMcpServerSpec>,
    /// Logical names of active A08 MCP servers recognized by `PolicyEngine`.
    pub(crate) registered_mcp_server_names: BTreeSet<String>,
}

/// Formats the compact provider-independent `[Dennett Active Skills Index]` system prompt block
/// (`41 §7.10`) without any instructions asking the model to narrate tool calls.
#[must_use]
pub(crate) fn format_session_skills_prompt_index(skills: &[ResolvedSessionSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\n[Dennett Active Skills Index]\n\
         The following Dennett skills are enabled for this session. \
         Pinned skills include their instructions inline below; for active or on-demand skills, \
         call `dennett_skill_read(slug)` (or `dennett_skill_list`) to read full instructions when relevant.",
    );

    let mut has_listed = false;
    for skill in skills {
        if skill.summary.activation_state == CapabilityActivation::Pinned {
            let truncated = truncate_utf8(&skill.skill_markdown, MAX_PINNED_SKILL_INLINE_BYTES);
            let suffix = if truncated.len() < skill.skill_markdown.len() {
                "\n...[truncated at 16 KiB; call dennett_skill_read(slug) for full instructions]"
            } else {
                ""
            };
            let _ = write!(
                out,
                "\n\n### Pinned Skill: {} (v{})\n{}{}",
                skill.summary.slug, skill.summary.version, truncated, suffix
            );
        } else {
            if !has_listed {
                out.push_str("\n\nAvailable skills (call `dennett_skill_read(slug)` to load):");
                has_listed = true;
            }
            let desc = if skill.summary.description.trim().is_empty() {
                skill.summary.display_name.as_str()
            } else {
                skill.summary.description.as_str()
            };
            let _ = write!(
                out,
                "\n- {} (v{}): {}",
                skill.summary.slug, skill.summary.version, desc
            );
        }
    }

    out
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

        let mut display_name = req.display_name.trim().to_owned();
        let mut description = req.description.trim().to_owned();
        let mut version = req.version.trim().to_owned();
        let mut trust_state = req.trust_state;
        let mut spec = req.spec.clone();
        let mut source_path = req.source_path.as_deref().map(|s| s.trim().to_owned());

        // D3-UI-02: When `source_path` is provided for a Skill or Plugin, inspect the folder
        // strictly read-only on disk rather than accepting client-side placeholders.
        if let Some(ref raw_path) = source_path {
            let folder_path = Path::new(raw_path);
            let folder_basename = folder_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            let name_not_overridden =
                is_default_display_name(&display_name, req.slug.trim(), folder_basename.as_deref());

            match &spec {
                CapabilitySpec::Skill { .. } => {
                    let inspected = inspect_skill_folder_readonly(folder_path)?;
                    spec = CapabilitySpec::Skill {
                        skill_markdown: inspected.markdown,
                        has_scripts: inspected.has_scripts,
                        entry_files: inspected.entry_files,
                    };
                    if let Some(front_desc) = inspected.description
                        && !front_desc.is_empty()
                    {
                        truncate_utf8(&front_desc, MAX_CAPABILITY_DESCRIPTION_BYTES)
                            .clone_into(&mut description);
                    }
                    if name_not_overridden
                        && let Some(front_name) = inspected.display_name
                        && !front_name.is_empty()
                    {
                        truncate_utf8(&front_name, MAX_CAPABILITY_NAME_LEN)
                            .clone_into(&mut display_name);
                    }
                    if version == "1.0.0"
                        && let Some(front_ver) = inspected.version
                        && !front_ver.is_empty()
                    {
                        truncate_utf8(&front_ver, MAX_CAPABILITY_VERSION_LEN)
                            .clone_into(&mut version);
                    }
                    if inspected.has_scripts && trust_state != CapabilityTrust::Revoked {
                        trust_state = CapabilityTrust::Restricted;
                    }
                    source_path = Some(clean_display_path(folder_path));
                    spec.validate()?;
                }
                CapabilitySpec::Plugin {
                    enabled_components, ..
                } => {
                    let inspected = inspect_plugin_folder_readonly(folder_path)?;
                    let has_secret_slots = inspected
                        .mcp_servers
                        .iter()
                        .any(|s| !s.secret_slots.is_empty());
                    spec = CapabilitySpec::Plugin {
                        manifest_version: inspected.manifest_version,
                        skills: inspected.skills,
                        mcp_servers: inspected.mcp_servers,
                        has_hooks: inspected.has_hooks,
                        enabled_components: enabled_components.clone(),
                    };
                    if let Some(desc) = inspected.description
                        && !desc.is_empty()
                    {
                        truncate_utf8(&desc, MAX_CAPABILITY_DESCRIPTION_BYTES)
                            .clone_into(&mut description);
                    }
                    if name_not_overridden
                        && let Some(name) = inspected.display_name
                        && !name.is_empty()
                    {
                        truncate_utf8(&name, MAX_CAPABILITY_NAME_LEN).clone_into(&mut display_name);
                    }
                    if version == "1.0.0"
                        && let Some(ver) = inspected.version
                        && !ver.is_empty()
                    {
                        truncate_utf8(&ver, MAX_CAPABILITY_VERSION_LEN).clone_into(&mut version);
                    }
                    if (inspected.has_hooks || has_secret_slots || inspected.has_scripts)
                        && trust_state != CapabilityTrust::Revoked
                    {
                        trust_state = CapabilityTrust::Restricted;
                    }
                    source_path = Some(clean_display_path(folder_path));
                    spec.validate()?;
                }
                _ => {}
            }
        }

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
        let content_sha256 = compute_capability_sha256(&version, &spec);
        let spec_json = serde_json::to_value(&spec).map_err(|err| DennettError::Internal {
            detail: format!("failed serializing capability spec: {err}"),
        })?;
        let possession = match req.activation_state {
            CapabilityActivation::Active | CapabilityActivation::Pinned
                if trust_state != CapabilityTrust::Restricted
                    && trust_state != CapabilityTrust::Revoked =>
            {
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
                    &spec.kind().as_str(),
                    &display_name.as_str(),
                    &description.as_str(),
                    &version.as_str(),
                    &req.origin_kind.as_str(),
                    &req.ownership.as_str(),
                    &req.scope_kind.as_str(),
                    &req.project_id.map(ProjectId::as_uuid),
                    &possession.as_str(),
                    &trust_state.as_str(),
                    &req.activation_state.as_str(),
                    &source_path.as_deref(),
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

    /// Lists skills enabled for the session authorized by `claims` (`GET /v1/capabilities/session-skills`).
    ///
    /// - Standalone sessions (`claims.project_id == None`): global enabled skills ONLY.
    /// - Project sessions (`claims.project_id == Some(pid)`): global + own-project enabled skills
    ///   with own-project override precedence over global skills of the same slug.
    /// - Disabled, forbidden, restricted/revoked, incompatible, or foreign-project skills are NEVER exposed.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::AccessDenied`], or [`DennettError::DatabaseUnavailable`].
    pub async fn list_session_skills(
        &self,
        claims: &ControlTokenClaims,
    ) -> Result<Vec<SessionSkillSummary>, DennettError> {
        self.verify_control_token_session_scope(claims).await?;
        let resolved = self.resolve_session_skills_internal(claims.project_id).await?;
        Ok(resolved.into_iter().map(|s| s.summary).collect())
    }

    /// Reads the full `SKILL.md` content for one enabled skill scoped strictly by `claims`
    /// (`GET /v1/capabilities/session-skills/{slug}`).
    ///
    /// Returns `403 AccessDenied` for disabled, blocked (`restricted`/`revoked`), or foreign-project
    /// skills, and `404 NotFound` for unknown slugs. When `source_path` is present on disk,
    /// verifies canonicalization and rejects symlink/traversal escapes before reading `SKILL.md`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::AccessDenied`],
    /// [`DennettError::NotFound`], or [`DennettError::DatabaseUnavailable`].
    pub async fn read_session_skill(
        &self,
        claims: &ControlTokenClaims,
        slug: &str,
    ) -> Result<SessionSkillDetailResponse, DennettError> {
        let trimmed = slug.trim();
        if trimmed.is_empty()
            || trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
            || trimmed.as_bytes().contains(&0)
        {
            return Err(DennettError::InvalidInput {
                field: String::from("slug"),
                reason: String::from(
                    "skill slug must be non-empty without '/', '\\', '..', or NUL bytes",
                ),
            });
        }
        self.verify_control_token_session_scope(claims).await?;

        let resolved = self.resolve_session_skills_internal(claims.project_id).await?;
        if let Some(found) = resolved.into_iter().find(|s| s.summary.slug == trimmed) {
            let source_path = found.source_path.clone();
            let fallback_md = found.skill_markdown.clone();
            let skill_md_content = tokio::task::spawn_blocking(move || {
                read_skill_markdown_safe(source_path.as_deref(), &fallback_md)
            })
            .await
            .map_err(|err| DennettError::Internal {
                detail: format!("session skill reader worker failed: {err}"),
            })??;
            return Ok(SessionSkillDetailResponse {
                skill: found.summary,
                skill_md_content,
            });
        }

        // Distinguish disabled/blocked/foreign-project skills (403) from completely unknown slugs (404).
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let exists_elsewhere: bool = conn
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM capability_records WHERE kind = 'skill' AND slug = $1)",
                &[&trimmed],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
            .get(0);
        if exists_elsewhere {
            return Err(DennettError::AccessDenied {
                scope: format!("session:{}", claims.session_id),
                reason: format!(
                    "skill `{trimmed}` is disabled, blocked, or outside this session's authorized project scope"
                ),
            });
        }
        Err(DennettError::NotFound {
            resource: String::from("session_skill"),
            id: trimmed.to_owned(),
        })
    }

    async fn verify_control_token_session_scope(
        &self,
        claims: &ControlTokenClaims,
    ) -> Result<(), DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let session = fetch_session(&conn, claims.session_id, self.pg_target()).await?;
        if session.project_id != claims.project_id {
            return Err(DennettError::AccessDenied {
                scope: format!("session:{}", claims.session_id),
                reason: String::from(
                    "control token project scope does not match persisted session project binding",
                ),
            });
        }
        Ok(())
    }

    pub(crate) async fn resolve_session_skills_internal(
        &self,
        project_id: Option<ProjectId>,
    ) -> Result<Vec<ResolvedSessionSkill>, DennettError> {
        let listed = self
            .list_capabilities(project_id, None, Some(CapabilityKind::Skill), None)
            .await?;
        Ok(select_session_skills_from_records(
            listed.capabilities,
            project_id,
        ))
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
        let mut skill_records = Vec::new();
        let mut seen_mcp_names: BTreeSet<String> = BTreeSet::new();

        for cap in listed.capabilities {
            if cap.kind == CapabilityKind::Skill {
                skill_records.push(cap);
                continue;
            }
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

        let session_skills = select_session_skills_from_records(skill_records, project_id);
        for skill in &session_skills {
            let should_materialize_disk = match skill.summary.activation_state {
                CapabilityActivation::Pinned | CapabilityActivation::Active => true,
                CapabilityActivation::OnDemand => {
                    attached_set.contains(skill.summary.slug.as_str())
                }
                _ => false,
            };
            if should_materialize_disk {
                out.active_skills.push(ActiveSkillPayload {
                    slug: skill.summary.slug.clone(),
                    version: skill.summary.version.clone(),
                    content_sha256: skill.content_sha256.clone(),
                    skill_markdown: skill.skill_markdown.clone(),
                });
            }
        }
        out.session_skills = session_skills;

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

#[expect(
    clippy::too_many_lines,
    reason = "resolves scope precedence (project over global), trust filtering, deduplication, and activation ranking in one pass"
)]
fn select_session_skills_from_records(
    records: Vec<CapabilityRecord>,
    project_id: Option<ProjectId>,
) -> Vec<ResolvedSessionSkill> {
    struct Candidate {
        scope_priority: u8,
        created_at: TimestampMs,
        skill: ResolvedSessionSkill,
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    for cap in records {
        if cap.kind != CapabilityKind::Skill {
            continue;
        }
        if matches!(
            cap.trust_state,
            CapabilityTrust::Revoked | CapabilityTrust::Restricted
        ) || cap.compatibility_status == CompatibilityStatus::Incompatible
        {
            continue;
        }

        let (scope_priority, effective_activation, effective_scope) =
            if let Some(pid) = project_id {
                let binding = cap
                    .project_bindings
                    .iter()
                    .find(|b| b.project_id == pid)
                    .map(|b| b.activation_state);
                match cap.scope_kind {
                    CapabilityScopeKind::Project => {
                        if cap.project_id != Some(pid) {
                            continue;
                        }
                        (
                            0_u8,
                            binding.unwrap_or(cap.activation_state),
                            CapabilityScopeKind::Project,
                        )
                    }
                    CapabilityScopeKind::CandidateUnattached => {
                        let Some(bound_state) = binding else {
                            continue;
                        };
                        (1_u8, bound_state, CapabilityScopeKind::Project)
                    }
                    CapabilityScopeKind::Global => (
                        2_u8,
                        binding.unwrap_or(cap.activation_state),
                        CapabilityScopeKind::Global,
                    ),
                    _ => continue,
                }
            } else {
                if cap.scope_kind != CapabilityScopeKind::Global {
                    continue;
                }
                (2_u8, cap.activation_state, CapabilityScopeKind::Global)
            };

        if !matches!(
            effective_activation,
            CapabilityActivation::Pinned
                | CapabilityActivation::Active
                | CapabilityActivation::OnDemand
        ) {
            continue;
        }

        let CapabilitySpec::Skill { skill_markdown, .. } = cap.spec else {
            continue;
        };

        candidates.push(Candidate {
            scope_priority,
            created_at: cap.created_at,
            skill: ResolvedSessionSkill {
                summary: SessionSkillSummary {
                    slug: cap.slug,
                    display_name: cap.display_name,
                    description: cap.description,
                    version: cap.version,
                    scope: effective_scope,
                    activation_state: effective_activation,
                    trust_state: cap.trust_state,
                },
                skill_markdown,
                source_path: cap.source_path,
                content_sha256: cap.content_sha256.as_str().to_owned(),
            },
        });
    }

    // Own-project overrides global for identical slugs; newest revision wins ties within same scope.
    candidates.sort_by(|a, b| {
        a.scope_priority
            .cmp(&b.scope_priority)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.skill.summary.slug.cmp(&b.skill.summary.slug))
    });

    let mut seen_slugs: BTreeSet<String> = BTreeSet::new();
    let mut resolved: Vec<ResolvedSessionSkill> = Vec::new();
    for cand in candidates {
        if seen_slugs.insert(cand.skill.summary.slug.clone()) {
            resolved.push(cand.skill);
        }
    }

    let activation_rank = |state: CapabilityActivation| -> u8 {
        match state {
            CapabilityActivation::Pinned => 0,
            CapabilityActivation::Active => 1,
            CapabilityActivation::OnDemand => 2,
            _ => 3,
        }
    };
    resolved.sort_by(|a, b| {
        activation_rank(a.summary.activation_state)
            .cmp(&activation_rank(b.summary.activation_state))
            .then_with(|| a.summary.slug.cmp(&b.summary.slug))
    });

    resolved
}

fn read_skill_markdown_safe(
    source_path: Option<&str>,
    fallback_markdown: &str,
) -> Result<String, DennettError> {
    let Some(raw) = source_path.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(fallback_markdown.to_owned());
    };
    if raw.as_bytes().contains(&0) {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("skill source_path contains forbidden NUL byte"),
        });
    }
    let src_path = Path::new(raw);
    if src_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("skill source_path contains forbidden parent traversal (`..`)"),
        });
    }

    let src_meta = match fs::symlink_metadata(src_path) {
        Ok(m) => m,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(fallback_markdown.to_owned()),
        Err(err) => {
            return Err(DennettError::AccessDenied {
                scope: String::from("capabilities.session_skill"),
                reason: format!("cannot inspect skill source_path: {err}"),
            });
        }
    };
    if is_symlink_or_reparse_point(&src_meta) {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("skill source_path symlink/reparse point is forbidden"),
        });
    }

    let (base_dir, skill_md_path) = if src_meta.is_dir() {
        (src_path.to_path_buf(), src_path.join("SKILL.md"))
    } else if src_meta.is_file()
        && src_path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("SKILL.md"))
        && let Some(parent) = src_path.parent()
    {
        (parent.to_path_buf(), src_path.to_path_buf())
    } else {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("skill source_path must be a directory or SKILL.md file"),
        });
    };

    let md_meta = match fs::symlink_metadata(&skill_md_path) {
        Ok(m) => m,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(fallback_markdown.to_owned()),
        Err(err) => {
            return Err(DennettError::AccessDenied {
                scope: String::from("capabilities.session_skill"),
                reason: format!("cannot inspect SKILL.md: {err}"),
            });
        }
    };
    if is_symlink_or_reparse_point(&md_meta) || !md_meta.is_file() {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("SKILL.md must be a regular non-symlink file"),
        });
    }
    if md_meta.len() > MAX_CAPABILITY_MARKDOWN_BYTES as u64 {
        return Err(DennettError::InvalidInput {
            field: String::from("skill_md_content"),
            reason: format!("SKILL.md exceeds maximum size of {MAX_CAPABILITY_MARKDOWN_BYTES} bytes"),
        });
    }

    let canon_dir = fs::canonicalize(&base_dir).map_err(|err| DennettError::AccessDenied {
        scope: String::from("capabilities.session_skill"),
        reason: format!("failed canonicalizing skill directory: {err}"),
    })?;
    let canon_file = fs::canonicalize(&skill_md_path).map_err(|err| DennettError::AccessDenied {
        scope: String::from("capabilities.session_skill"),
        reason: format!("failed canonicalizing SKILL.md: {err}"),
    })?;
    if !canon_file.starts_with(&canon_dir) {
        return Err(DennettError::AccessDenied {
            scope: String::from("capabilities.session_skill"),
            reason: String::from("canonical SKILL.md path escaped skill source directory"),
        });
    }

    let content = fs::read_to_string(&canon_file).map_err(|err| DennettError::Internal {
        detail: format!("failed reading SKILL.md as UTF-8: {err}"),
    })?;
    if content.trim().is_empty() {
        return Ok(fallback_markdown.to_owned());
    }
    Ok(content)
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

fn is_default_display_name(current: &str, slug: &str, folder_basename: Option<&str>) -> bool {
    let trimmed = current.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(slug)
        || folder_basename.is_some_and(|base| trimmed.eq_ignore_ascii_case(base))
}

#[derive(Debug, Clone)]
struct InspectedSkillFolder {
    markdown: String,
    display_name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    has_scripts: bool,
    entry_files: Vec<String>,
}

#[derive(Debug, Clone)]
struct InspectedPluginFolder {
    slug_hint: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    manifest_version: String,
    skills: Vec<String>,
    mcp_servers: Vec<PluginMcpServerEntry>,
    has_hooks: bool,
    has_scripts: bool,
}

fn inspect_skill_folder_readonly(dir: &Path) -> Result<InspectedSkillFolder, DennettError> {
    let dir_meta = fs::symlink_metadata(dir).map_err(|_| DennettError::InvalidInput {
        field: String::from("source_path"),
        reason: format!(
            "skill folder `{}` does not exist or is not readable",
            clean_display_path(dir)
        ),
    })?;
    if is_symlink_or_reparse_point(&dir_meta) {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("skill folder cannot be a symlink or junction"),
        });
    }
    if !dir_meta.is_dir() {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("selected skill path must be a directory containing SKILL.md"),
        });
    }

    let skill_md_path = dir.join("SKILL.md");
    let md_meta = fs::symlink_metadata(&skill_md_path).map_err(|_| DennettError::InvalidInput {
        field: String::from("source_path"),
        reason: format!(
            "skill folder `{}` does not contain a readable SKILL.md file",
            clean_display_path(dir)
        ),
    })?;
    if is_symlink_or_reparse_point(&md_meta) || !md_meta.is_file() {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("SKILL.md must be a regular non-symlink file"),
        });
    }
    if md_meta.len() > MAX_CAPABILITY_MARKDOWN_BYTES as u64 {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: format!(
                "SKILL.md exceeds maximum size of {MAX_CAPABILITY_MARKDOWN_BYTES} bytes"
            ),
        });
    }
    let markdown =
        fs::read_to_string(&skill_md_path).map_err(|err| DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: format!("failed reading SKILL.md as UTF-8: {err}"),
        })?;
    if markdown.trim().is_empty() {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("SKILL.md cannot be empty"),
        });
    }

    let (display_name, description, version) = parse_skill_frontmatter(&markdown);
    let (has_scripts, entry_files) = inspect_skill_bundle_readonly(dir);
    Ok(InspectedSkillFolder {
        markdown,
        display_name,
        description,
        version,
        has_scripts,
        entry_files,
    })
}

fn is_executable_script_name(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("sh")
            || ext.eq_ignore_ascii_case("ps1")
            || ext.eq_ignore_ascii_case("bat")
            || ext.eq_ignore_ascii_case("cmd")
            || ext.eq_ignore_ascii_case("py")
            || ext.eq_ignore_ascii_case("js")
            || ext.eq_ignore_ascii_case("mjs")
            || ext.eq_ignore_ascii_case("cjs")
            || ext.eq_ignore_ascii_case("ts")
            || ext.eq_ignore_ascii_case("rb")
            || ext.eq_ignore_ascii_case("pl")
            || ext.eq_ignore_ascii_case("exe")
    })
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
        if meta.is_dir() {
            if name.eq_ignore_ascii_case("scripts") {
                has_scripts = true;
            }
            if !name.starts_with('.')
                && let Ok(sub_entries) = fs::read_dir(&path)
            {
                for sub in sub_entries.flatten().take(MAX_CAPABILITY_LIST_ITEMS) {
                    let sub_path = sub.path();
                    let Ok(sub_meta) = fs::symlink_metadata(&sub_path) else {
                        continue;
                    };
                    if is_symlink_or_reparse_point(&sub_meta) || !sub_meta.is_file() {
                        continue;
                    }
                    let sub_name = sub.file_name().to_string_lossy().into_owned();
                    if is_executable_script_name(&sub_name) {
                        has_scripts = true;
                    }
                }
            }
        } else if is_executable_script_name(&name) {
            has_scripts = true;
        }
        if files.len() < MAX_CAPABILITY_LIST_ITEMS {
            files.push(name);
        }
    }
    files.sort();
    (has_scripts, files)
}

fn parse_mcp_servers_map(
    servers: &serde_json::Map<String, JsonValue>,
) -> Vec<(String, PluginMcpServerEntry)> {
    let mut out = Vec::new();
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
        let entry = PluginMcpServerEntry {
            name: raw_name.clone(),
            command: command.to_owned(),
            args,
            env_vars,
            secret_slots,
        };
        if entry.validate().is_ok() {
            out.push((raw_name.clone(), entry));
        }
    }
    out
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

    for (raw_name, entry) in parse_mcp_servers_map(servers) {
        let slug = sanitize_discovered_slug(&raw_name);
        out.push(DiscoveredCandidate {
            slug,
            display_name: raw_name,
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
                command: entry.command,
                args: entry.args,
                env_vars: entry.env_vars,
                secret_slots: entry.secret_slots,
            },
        });
    }
}

fn locate_plugin_manifest_readonly(dir: &Path) -> Result<Option<PathBuf>, DennettError> {
    let direct_candidates = [dir.join("plugin.json"), dir.join("manifest.json")];
    if let Ok(meta) = fs::symlink_metadata(&direct_candidates[0]) {
        if is_symlink_or_reparse_point(&meta) || !meta.is_file() {
            return Err(DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: String::from("plugin.json must be a regular non-symlink file"),
            });
        }
        return Ok(Some(direct_candidates[0].clone()));
    }

    let claude_dir = dir.join(".claude-plugin");
    if let Ok(claude_meta) = fs::symlink_metadata(&claude_dir)
        && !is_symlink_or_reparse_point(&claude_meta)
        && claude_meta.is_dir()
    {
        let claude_manifest = claude_dir.join("plugin.json");
        if let Ok(meta) = fs::symlink_metadata(&claude_manifest) {
            if is_symlink_or_reparse_point(&meta) || !meta.is_file() {
                return Err(DennettError::InvalidInput {
                    field: String::from("source_path"),
                    reason: String::from(
                        ".claude-plugin/plugin.json must be a regular non-symlink file",
                    ),
                });
            }
            return Ok(Some(claude_manifest));
        }
    }

    if let Ok(meta) = fs::symlink_metadata(&direct_candidates[1]) {
        if is_symlink_or_reparse_point(&meta) || !meta.is_file() {
            return Err(DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: String::from("manifest.json must be a regular non-symlink file"),
            });
        }
        return Ok(Some(direct_candidates[1].clone()));
    }

    Ok(None)
}

fn json_value_indicates_hooks(val: &JsonValue) -> bool {
    match val {
        JsonValue::Null => false,
        JsonValue::Bool(b) => *b,
        JsonValue::Array(arr) => !arr.is_empty(),
        JsonValue::Object(obj) => !obj.is_empty(),
        JsonValue::String(s) => !s.trim().is_empty(),
        JsonValue::Number(_) => true,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "strictly read-only inspection of plugin manifests, skills/, hooks/, and mcp.json bundles"
)]
fn inspect_plugin_folder_readonly(dir: &Path) -> Result<InspectedPluginFolder, DennettError> {
    let dir_meta = fs::symlink_metadata(dir).map_err(|_| DennettError::InvalidInput {
        field: String::from("source_path"),
        reason: format!(
            "plugin folder `{}` does not exist or is not readable",
            clean_display_path(dir)
        ),
    })?;
    if is_symlink_or_reparse_point(&dir_meta) {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("plugin folder cannot be a symlink or junction"),
        });
    }
    if !dir_meta.is_dir() {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: String::from("selected plugin path must be a directory"),
        });
    }

    let mut slug_hint = None;
    let mut display_name = None;
    let mut description = None;
    let mut version = None;
    let mut manifest_version = String::from("1.0");
    let mut skill_set = BTreeSet::new();
    let mut mcp_servers: Vec<PluginMcpServerEntry> = Vec::new();
    let mut seen_mcp_names = BTreeSet::new();
    let mut has_hooks = false;
    let mut has_scripts = false;
    let mut has_manifest = false;

    if let Some(manifest_path) = locate_plugin_manifest_readonly(dir)? {
        let meta =
            fs::symlink_metadata(&manifest_path).map_err(|err| DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: format!("cannot stat plugin manifest: {err}"),
            })?;
        if meta.len() > 512 * 1024 {
            return Err(DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: String::from("plugin manifest exceeds maximum size of 512 KiB"),
            });
        }
        let raw = fs::read_to_string(&manifest_path).map_err(|err| DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: format!("failed reading plugin manifest as UTF-8: {err}"),
        })?;
        let manifest: JsonValue =
            serde_json::from_str(&raw).map_err(|err| DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: format!(
                    "invalid plugin manifest JSON `{}`: {err}",
                    clean_display_path(&manifest_path)
                ),
            })?;
        if !manifest.is_object() {
            return Err(DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: String::from("plugin manifest root must be a JSON object"),
            });
        }
        has_manifest = true;

        slug_hint = manifest
            .get("slug")
            .or_else(|| manifest.get("name"))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(sanitize_discovered_slug);
        display_name = manifest
            .get("display_name")
            .or_else(|| manifest.get("name"))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        description = manifest
            .get("description")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        version = manifest
            .get("version")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        if let Some(mv) = manifest
            .get("manifest_version")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            mv.clone_into(&mut manifest_version);
        }
        if let Some(arr) = manifest.get("skills").and_then(JsonValue::as_array) {
            for v in arr.iter().take(MAX_CAPABILITY_LIST_ITEMS) {
                if let Some(s) = v.as_str() {
                    skill_set.insert(sanitize_discovered_slug(s));
                }
            }
        }
        if manifest
            .get("has_hooks")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || manifest
                .get("hooks")
                .is_some_and(json_value_indicates_hooks)
        {
            has_hooks = true;
        }
        if let Some(arr) = manifest.get("mcp_servers").and_then(JsonValue::as_array) {
            for item in arr.iter().take(MAX_CAPABILITY_LIST_ITEMS) {
                if let Ok(entry_spec) = serde_json::from_value::<PluginMcpServerEntry>(item.clone())
                    && entry_spec.validate().is_ok()
                    && seen_mcp_names.insert(entry_spec.name.clone())
                {
                    mcp_servers.push(entry_spec);
                }
            }
        }
        if let Some(servers_obj) = manifest.get("mcpServers").and_then(JsonValue::as_object) {
            for (_, entry_spec) in parse_mcp_servers_map(servers_obj) {
                if mcp_servers.len() < MAX_CAPABILITY_LIST_ITEMS
                    && seen_mcp_names.insert(entry_spec.name.clone())
                {
                    mcp_servers.push(entry_spec);
                }
            }
        }
    }

    // Inspect bundled `skills/` directory strictly read-only.
    let skills_dir = dir.join("skills");
    if let Ok(sm) = fs::symlink_metadata(&skills_dir)
        && !is_symlink_or_reparse_point(&sm)
        && sm.is_dir()
        && let Ok(entries) = fs::read_dir(&skills_dir)
    {
        for entry in entries.flatten().take(MAX_CAPABILITY_LIST_ITEMS) {
            let child = entry.path();
            if let Ok(inspected_skill) = inspect_skill_folder_readonly(&child) {
                let raw_name = entry.file_name().to_string_lossy().into_owned();
                skill_set.insert(sanitize_discovered_slug(&raw_name));
                if inspected_skill.has_scripts {
                    has_scripts = true;
                }
            }
        }
    }

    // Inspect bundled `hooks.json` or non-empty `hooks/` directory strictly read-only.
    if let Ok(hm) = fs::symlink_metadata(dir.join("hooks.json"))
        && !is_symlink_or_reparse_point(&hm)
        && hm.is_file()
    {
        has_hooks = true;
    }
    let hooks_dir = dir.join("hooks");
    if let Ok(hm) = fs::symlink_metadata(&hooks_dir)
        && !is_symlink_or_reparse_point(&hm)
        && hm.is_dir()
        && let Ok(mut entries) = fs::read_dir(&hooks_dir)
        && entries.any(|ent| {
            ent.is_ok_and(|e| {
                fs::symlink_metadata(e.path()).is_ok_and(|m| !is_symlink_or_reparse_point(&m))
            })
        })
    {
        has_hooks = true;
    }

    // Inspect bundled MCP JSON files (`mcp.json`, `.mcp.json`, `.dennett/mcp.json`).
    for mcp_path in [
        dir.join("mcp.json"),
        dir.join(".mcp.json"),
        dir.join(".dennett").join("mcp.json"),
    ] {
        let Ok(mm) = fs::symlink_metadata(&mcp_path) else {
            continue;
        };
        if is_symlink_or_reparse_point(&mm) || !mm.is_file() || mm.len() > 512 * 1024 {
            continue;
        }
        let raw = fs::read_to_string(&mcp_path).map_err(|err| DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: format!(
                "failed reading plugin MCP config `{}`: {err}",
                clean_display_path(&mcp_path)
            ),
        })?;
        let parsed: JsonValue =
            serde_json::from_str(&raw).map_err(|err| DennettError::InvalidInput {
                field: String::from("source_path"),
                reason: format!(
                    "invalid plugin MCP config JSON `{}`: {err}",
                    clean_display_path(&mcp_path)
                ),
            })?;
        if let Some(servers_obj) = parsed.get("mcpServers").and_then(JsonValue::as_object) {
            for (_, entry_spec) in parse_mcp_servers_map(servers_obj) {
                if mcp_servers.len() < MAX_CAPABILITY_LIST_ITEMS
                    && seen_mcp_names.insert(entry_spec.name.clone())
                {
                    mcp_servers.push(entry_spec);
                }
            }
        }
    }

    let skills: Vec<String> = skill_set
        .into_iter()
        .take(MAX_CAPABILITY_LIST_ITEMS)
        .collect();
    if !has_manifest && skills.is_empty() && mcp_servers.is_empty() && !has_hooks {
        return Err(DennettError::InvalidInput {
            field: String::from("source_path"),
            reason: format!(
                "plugin folder `{}` does not contain a valid manifest (plugin.json, .claude-plugin/plugin.json, manifest.json) or valid skills/, hooks/, or mcp.json structure",
                clean_display_path(dir)
            ),
        });
    }

    Ok(InspectedPluginFolder {
        slug_hint,
        display_name,
        description,
        version,
        manifest_version,
        skills,
        mcp_servers,
        has_hooks,
        has_scripts,
    })
}

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
        let inspected = match inspect_plugin_folder_readonly(&dir) {
            Ok(info) => info,
            Err(err) => {
                // Only warn when a manifest file existed but failed to parse; silently skip unrelated folders.
                if dir.join("plugin.json").exists()
                    || dir.join("manifest.json").exists()
                    || dir.join(".claude-plugin").join("plugin.json").exists()
                {
                    warnings.push(format!(
                        "could not inspect plugin `{}`: {err}",
                        clean_display_path(&dir)
                    ));
                }
                continue;
            }
        };

        let raw_name = entry.file_name().to_string_lossy().into_owned();
        let slug = inspected
            .slug_hint
            .unwrap_or_else(|| sanitize_discovered_slug(&raw_name));
        let display_name = inspected.display_name.unwrap_or(raw_name);
        let description = inspected.description.unwrap_or_default();
        let version = inspected.version.unwrap_or_else(|| String::from("1.0.0"));

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
                manifest_version: inspected.manifest_version,
                skills: inspected.skills,
                mcp_servers: inspected.mcp_servers,
                has_hooks: inspected.has_hooks,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YamlBlockStyle {
    Folded,
    Literal,
}

fn parse_yaml_block_scalar_header(raw: &str) -> Option<YamlBlockStyle> {
    let mut chars = raw.chars();
    let style = match chars.next()? {
        '>' => YamlBlockStyle::Folded,
        '|' => YamlBlockStyle::Literal,
        _ => return None,
    };
    let rest = chars.as_str();
    let (indicator, trailing) = match rest.find(|c: char| c.is_ascii_whitespace()) {
        Some(pos) => (&rest[..pos], rest[pos..].trim_start()),
        None => (rest, ""),
    };
    if !trailing.is_empty() && !trailing.starts_with('#') {
        return None;
    }
    let mut seen_chomp = false;
    let mut seen_indent = false;
    for ch in indicator.chars() {
        match ch {
            '-' | '+' if !seen_chomp => seen_chomp = true,
            '1'..='9' if !seen_indent => seen_indent = true,
            _ => return None,
        }
    }
    Some(style)
}

fn unquote_yaml_scalar(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        if trimmed.starts_with('"') {
            return inner.replace("\\\"", "\"").replace("\\\\", "\\");
        }
        return inner.replace("''", "'");
    }
    // Strip inline `# comment` only when preceded by whitespace.
    if let Some(pos) = trimmed.find(" #") {
        return trimmed[..pos].trim_end().to_owned();
    }
    trimmed.to_owned()
}

fn collect_yaml_block_scalar(
    lines: &[&str],
    start_idx: usize,
    style: YamlBlockStyle,
) -> (Option<String>, usize) {
    let mut i = start_idx;
    let mut block_indent: Option<usize> = None;
    let mut raw_lines: Vec<&str> = Vec::new();

    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            raw_lines.push("");
            i += 1;
            continue;
        }
        let indent = line
            .bytes()
            .take_while(|b| *b == b' ' || *b == b'\t')
            .count();
        if indent == 0 {
            break;
        }
        let base_indent = *block_indent.get_or_insert(indent);
        if indent < base_indent {
            break;
        }
        raw_lines.push(&line[base_indent..]);
        i += 1;
    }

    while raw_lines.last().is_some_and(|l| l.is_empty()) {
        raw_lines.pop();
    }

    let formatted = match style {
        YamlBlockStyle::Folded => {
            let mut out = String::new();
            let mut prev_was_empty = false;
            for line in raw_lines {
                if line.is_empty() {
                    if !out.is_empty() {
                        prev_was_empty = true;
                    }
                    continue;
                }
                if !out.is_empty() {
                    if prev_was_empty {
                        out.push('\n');
                    } else {
                        out.push(' ');
                    }
                }
                out.push_str(line.trim());
                prev_was_empty = false;
            }
            out
        }
        YamlBlockStyle::Literal => raw_lines.join("\n").trim_end().to_owned(),
    };

    let result = if formatted.trim().is_empty() {
        None
    } else {
        Some(formatted)
    };
    (result, i)
}

fn parse_skill_frontmatter(markdown: &str) -> (Option<String>, Option<String>, Option<String>) {
    let clean = markdown.trim_start_matches('\u{feff}').trim_start();
    let mut all_lines = clean.lines();
    if all_lines.next().map(str::trim) != Some("---") {
        return (None, None, None);
    }

    let mut header_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in all_lines {
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            closed = true;
            break;
        }
        header_lines.push(line);
    }
    if !closed {
        return (None, None, None);
    }

    let mut name = None;
    let mut desc = None;
    let mut version = None;
    let mut metadata_version = None;
    let mut in_metadata = false;
    let mut i = 0;

    while i < header_lines.len() {
        let line = header_lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let indent = line
            .bytes()
            .take_while(|b| *b == b' ' || *b == b'\t')
            .count();
        let Some((raw_key, raw_val)) = line.split_once(':') else {
            i += 1;
            continue;
        };
        let key = raw_key.trim().to_ascii_lowercase();
        let val_trimmed = raw_val.trim();

        if indent > 0 {
            if in_metadata && key == "version" && metadata_version.is_none() {
                let parsed = unquote_yaml_scalar(val_trimmed);
                if !parsed.is_empty() {
                    metadata_version = Some(parsed);
                }
            }
            i += 1;
            continue;
        }

        in_metadata = key == "metadata" && val_trimmed.is_empty();

        let (parsed_val, next_i) = if let Some(style) = parse_yaml_block_scalar_header(val_trimmed)
        {
            collect_yaml_block_scalar(&header_lines, i + 1, style)
        } else if val_trimmed.is_empty() && key == "description" {
            // Indented plain continuation block directly beneath `description:`
            collect_yaml_block_scalar(&header_lines, i + 1, YamlBlockStyle::Folded)
        } else {
            let mut value = unquote_yaml_scalar(val_trimmed);
            let mut cursor = i + 1;
            if key == "description"
                && !value.is_empty()
                && !val_trimmed.starts_with('"')
                && !val_trimmed.starts_with('\'')
            {
                while cursor < header_lines.len() {
                    let cont = header_lines[cursor];
                    let cont_indent = cont
                        .bytes()
                        .take_while(|b| *b == b' ' || *b == b'\t')
                        .count();
                    let cont_trimmed = cont.trim();
                    if cont_indent == 0 || cont_trimmed.is_empty() || cont_trimmed.starts_with('#')
                    {
                        break;
                    }
                    value.push(' ');
                    value.push_str(cont_trimmed);
                    cursor += 1;
                }
            }
            let opt = if value.is_empty() { None } else { Some(value) };
            (opt, cursor)
        };

        if let Some(val) = parsed_val {
            match key.as_str() {
                "name" | "title" => name = Some(val),
                "description" => desc = Some(val),
                "version" => version = Some(val),
                _ => {}
            }
        }
        i = next_i;
    }

    (name, desc, version.or(metadata_version))
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

    #[test]
    fn d3_ui_03_parse_skill_frontmatter_handles_yaml_block_scalars_and_crlf() {
        let folded_strip = "---\r\nname: agent-browser\r\ndescription: >-\r\n  Browser automation via vercel-labs agent-browser:\r\n  open sites, inspect accessibility tree, and take screenshots.\r\nmetadata:\r\n  version: \"2.4.1\"\r\n---\r\n# Body\r\n";
        let (name, desc, ver) = parse_skill_frontmatter(folded_strip);
        assert_eq!(name.as_deref(), Some("agent-browser"));
        assert_eq!(
            desc.as_deref(),
            Some(
                "Browser automation via vercel-labs agent-browser: open sites, inspect accessibility tree, and take screenshots."
            )
        );
        assert_eq!(ver.as_deref(), Some("2.4.1"));

        let folded_paragraphs = "---\nname: movies-db\ndescription: >\n  First line\n  second line.\n\n  Second paragraph.\nversion: 1.2.0\n---\n";
        let (name2, desc2, ver2) = parse_skill_frontmatter(folded_paragraphs);
        assert_eq!(name2.as_deref(), Some("movies-db"));
        assert_eq!(
            desc2.as_deref(),
            Some("First line second line.\nSecond paragraph.")
        );
        assert_eq!(ver2.as_deref(), Some("1.2.0"));

        let literal_strip =
            "---\nname: literal-skill\ndescription: |-\n  Line one.\n  Line two.\n---\n";
        let (name3, desc3, _) = parse_skill_frontmatter(literal_strip);
        assert_eq!(name3.as_deref(), Some("literal-skill"));
        assert_eq!(desc3.as_deref(), Some("Line one.\nLine two."));

        let literal_keep =
            "---\ntitle: \"Quoted: Skill\"\ndescription: | # comment\n  Alpha\n  Beta\n---\n";
        let (name4, desc4, _) = parse_skill_frontmatter(literal_keep);
        assert_eq!(name4.as_deref(), Some("Quoted: Skill"));
        assert_eq!(desc4.as_deref(), Some("Alpha\nBeta"));
    }

    #[test]
    fn d3_ui_02_folder_inspection_reads_real_skill_and_plugin_and_rejects_empty_dirs() {
        let root =
            std::env::temp_dir().join(format!("dennett-cap-inspect-{}", Uuid::new_v4().simple()));
        let empty_dir = root.join("empty-folder");
        let skill_dir = root.join("scripted-skill");
        let plugin_dir = root.join("claude-layout-plugin");
        fs::create_dir_all(&empty_dir).expect("create empty dir");
        fs::create_dir_all(skill_dir.join("scripts")).expect("create skill scripts dir");
        fs::create_dir_all(plugin_dir.join(".claude-plugin")).expect("create claude plugin dir");
        fs::create_dir_all(plugin_dir.join("hooks")).expect("create plugin hooks dir");
        fs::create_dir_all(plugin_dir.join("skills").join("nested-audit"))
            .expect("create nested skill dir");

        // Empty folder must be rejected for both Skill and Plugin!
        let skill_err =
            inspect_skill_folder_readonly(&empty_dir).expect_err("empty skill dir must fail");
        assert!(matches!(skill_err, DennettError::InvalidInput { .. }));
        let plugin_err =
            inspect_plugin_folder_readonly(&empty_dir).expect_err("empty plugin dir must fail");
        assert!(matches!(plugin_err, DennettError::InvalidInput { .. }));

        // Real skill with YAML >- description and scripts/run.py!
        let real_md = "---\nname: Scripted Helper\ndescription: >-\n  Automates build checks\n  with a Python script.\nversion: 1.5.0\n---\n# Scripted Helper\nRun checks carefully.\n";
        fs::write(skill_dir.join("SKILL.md"), real_md).expect("write SKILL.md");
        fs::write(skill_dir.join("scripts").join("run.py"), "print('hi')\n").expect("write run.py");

        let inspected_skill =
            inspect_skill_folder_readonly(&skill_dir).expect("inspect valid skill");
        assert_eq!(inspected_skill.markdown, real_md);
        assert_eq!(
            inspected_skill.display_name.as_deref(),
            Some("Scripted Helper")
        );
        assert_eq!(
            inspected_skill.description.as_deref(),
            Some("Automates build checks with a Python script.")
        );
        assert_eq!(inspected_skill.version.as_deref(), Some("1.5.0"));
        assert!(inspected_skill.has_scripts);
        assert!(inspected_skill.entry_files.iter().any(|f| f == "SKILL.md"));
        assert!(inspected_skill.entry_files.iter().any(|f| f == "scripts"));

        // Real plugin with .claude-plugin/plugin.json, hooks/, skills/, and mcp.json with secret env!
        fs::write(
            plugin_dir.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"Audit Pack","description":"Real plugin manifest","version":"2.0.0"}"#,
        )
        .expect("write plugin.json");
        fs::write(plugin_dir.join("hooks").join("pre_tool.json"), "{}").expect("write hook");
        fs::write(
            plugin_dir
                .join("skills")
                .join("nested-audit")
                .join("SKILL.md"),
            "# Nested Audit\nCheck invariants.\n",
        )
        .expect("write nested skill");
        fs::write(
            plugin_dir.join("mcp.json"),
            r#"{"mcpServers":{"docs":{"command":"docs-mcp","args":["--ro"],"env":{"API_TOKEN":"raw-secret-never-copied"}}}}"#,
        )
        .expect("write mcp.json");

        let inspected_plugin =
            inspect_plugin_folder_readonly(&plugin_dir).expect("inspect valid plugin");
        assert_eq!(inspected_plugin.display_name.as_deref(), Some("Audit Pack"));
        assert_eq!(
            inspected_plugin.description.as_deref(),
            Some("Real plugin manifest")
        );
        assert_eq!(inspected_plugin.version.as_deref(), Some("2.0.0"));
        assert!(inspected_plugin.has_hooks);
        assert_eq!(inspected_plugin.skills, vec![String::from("nested-audit")]);
        assert_eq!(inspected_plugin.mcp_servers.len(), 1);
        assert_eq!(inspected_plugin.mcp_servers[0].secret_slots.len(), 1);
        assert_eq!(inspected_plugin.mcp_servers[0].env_vars.len(), 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn d3_session_skills_prompt_index_and_safe_markdown_reader_enforce_bounds_and_traversal_guards() {
        let root = std::env::temp_dir().join(format!(
            "dennett-session-skill-safe-{}",
            Uuid::new_v4().simple()
        ));
        let skill_dir = root.join("ok-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "# Fresh Disk Skill\nAlways test.").expect("write");

        let disk_body = read_skill_markdown_safe(
            Some(skill_dir.to_string_lossy().as_ref()),
            "# Fallback",
        )
        .expect("read valid skill dir");
        assert_eq!(disk_body, "# Fresh Disk Skill\nAlways test.");

        let trav_err = read_skill_markdown_safe(Some("../outside"), "# Fallback")
            .expect_err("traversal must fail closed");
        assert!(matches!(trav_err, DennettError::AccessDenied { .. }));

        let nul_err = read_skill_markdown_safe(Some("bad\0path"), "# Fallback")
            .expect_err("NUL byte must fail closed");
        assert!(matches!(nul_err, DennettError::AccessDenied { .. }));

        let oversized_pinned = "A".repeat(MAX_PINNED_SKILL_INLINE_BYTES + 2048);
        let skills = vec![
            ResolvedSessionSkill {
                summary: SessionSkillSummary {
                    slug: String::from("pinned-big"),
                    display_name: String::from("Pinned Big"),
                    description: String::from("Large pinned skill"),
                    version: String::from("1.0.0"),
                    scope: CapabilityScopeKind::Global,
                    activation_state: CapabilityActivation::Pinned,
                    trust_state: CapabilityTrust::Trusted,
                },
                skill_markdown: oversized_pinned,
                source_path: None,
                content_sha256: "0".repeat(64),
            },
            ResolvedSessionSkill {
                summary: SessionSkillSummary {
                    slug: String::from("rust-audit"),
                    display_name: String::from("Rust Audit"),
                    description: String::from("Checks invariants"),
                    version: String::from("2.1.0"),
                    scope: CapabilityScopeKind::Project,
                    activation_state: CapabilityActivation::Active,
                    trust_state: CapabilityTrust::Trusted,
                },
                skill_markdown: String::from("# Full body"),
                source_path: None,
                content_sha256: "1".repeat(64),
            },
        ];

        let prompt_block = format_session_skills_prompt_index(&skills);
        assert!(prompt_block.contains("[Dennett Active Skills Index]"));
        assert!(prompt_block.contains("### Pinned Skill: pinned-big (v1.0.0)"));
        assert!(prompt_block.contains("truncated at 16 KiB"));
        assert!(prompt_block.contains("- rust-audit (v2.1.0): Checks invariants"));
        assert!(prompt_block.contains("dennett_skill_read(slug)"));
        // Verify ZERO prompt instruction forcing model to narrate tool calls.
        assert!(!prompt_block.to_ascii_lowercase().contains("narrate"));

        let _ = fs::remove_dir_all(&root);
    }
}
