//! Multi-runtime provider connections registry, probing, activation, and zero-fallback routing
//! for `dennett-head` (Contract M, A09, V29, D3).
//!
//! Supported live providers in D3:
//! - `antigravity_acp` (`oauth-personal` Google AI Ultra + `gemini-api-key` headless profile)
//! - `local_http` (`local-endpoint` loopback HTTP servers: Ollama, LM Studio, `llama.cpp`)
//!
//! Deferred providers (`r-claude`, `r-codex`) appear strictly as static read-only
//! [`PlannedProviderInfo`] entries with zero dependencies and zero login buttons.

use std::str::FromStr;
use std::sync::Arc;

use dennett_contracts::{
    ActivateConnectionRequest, ConnectionActivationRole, CorrelationId,
    DeleteProviderConnectionResponse, DennettError, ListProviderConnectionsResponse,
    PlannedProviderInfo, ProbeConnectionResponse, ProviderConnectionId, ProviderConnectionRecord,
    ProviderHealthStatus, ProviderKind, ProviderQuotaError, RuntimeAuthProfile,
    RuntimeCatalogResponse, TimestampMs, UpsertProviderConnectionRequest,
};
use dennett_runtime_antigravity::{
    ANTIGRAVITY_ADAPTER_ID, AgentRuntimePort, AntigravityAcpRuntime, AntigravityRuntimeConfig,
};
use dennett_runtime_local::{LocalHttpRuntime, LocalRuntimeConfig, validate_local_endpoint_url};
use serde_json::json;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::secrets::GEMINI_API_KEY_SLOT;
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{append_audit_event, map_pg_err, truncate_utf8};

/// Deterministic UUID seeded in migration `0008` for Google Antigravity (`oauth-personal`).
pub const SEEDED_OAUTH_PERSONAL_CONNECTION_ID: ProviderConnectionId =
    ProviderConnectionId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001));

/// Deterministic UUID seeded in migration `0008` for Google Antigravity (`gemini-api-key`).
pub const SEEDED_GEMINI_API_KEY_CONNECTION_ID: ProviderConnectionId =
    ProviderConnectionId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0002));

const CONNECTION_SELECT_COLS: &str = "
    connection_id, provider_kind, display_name, adapter_id, auth_profile,
    endpoint_url, secret_slot_key, is_default_interactive, is_default_background,
    enabled, last_health_status, last_health_detail,
    TRUNC(EXTRACT(EPOCH FROM last_probed_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
";

/// Static read-only list of deferred runtime providers outside D0–D9 scope (Contract M).
#[must_use]
pub fn static_planned_providers() -> Vec<PlannedProviderInfo> {
    vec![
        PlannedProviderInfo {
            id: String::from("r-claude"),
            label: String::from("R-CLAUDE"),
            status: String::from("planned"),
            note: Some(String::from(
                "Deferred beyond D0-D9 rollout; no dependency or login bundled.",
            )),
        },
        PlannedProviderInfo {
            id: String::from("r-codex"),
            label: String::from("R-CODEX"),
            status: String::from("planned"),
            note: Some(String::from(
                "Deferred beyond D0-D9 rollout; no dependency or login bundled.",
            )),
        },
    ]
}

/// Cached dynamically instantiated runtime adapter tied to a specific connection and secret state.
pub(crate) struct CachedDynamicRuntime {
    pub(crate) runtime: Arc<dyn AgentRuntimePort>,
    pub(crate) local_runtime: Option<Arc<LocalHttpRuntime>>,
    pub(crate) endpoint_url: Option<String>,
    pub(crate) secret_slot_key: Option<String>,
    pub(crate) secret_version: Option<i64>,
    pub(crate) auth_profile: RuntimeAuthProfile,
}

/// Resolved runtime target for a chat or task turn.
pub(crate) struct ResolvedTurnRuntime {
    pub(crate) runtime: Arc<dyn AgentRuntimePort>,
    pub(crate) connection_id: Option<ProviderConnectionId>,
}

fn decode_connection_row(row: &Row) -> Result<ProviderConnectionRecord, DennettError> {
    Ok(ProviderConnectionRecord {
        connection_id: ProviderConnectionId::from_uuid(row.get(0)),
        provider_kind: ProviderKind::from_str(&row.get::<_, String>(1))?,
        display_name: row.get(2),
        adapter_id: row.get(3),
        auth_profile: RuntimeAuthProfile::from_str(&row.get::<_, String>(4))?,
        endpoint_url: row.get(5),
        secret_slot_key: row.get(6),
        is_default_interactive: row.get(7),
        is_default_background: row.get(8),
        enabled: row.get(9),
        last_health_status: ProviderHealthStatus::from_str(&row.get::<_, String>(10))?,
        last_health_detail: row.get(11),
        last_probed_at: row.get::<_, Option<i64>>(12).map(TimestampMs::from_millis),
        created_at: TimestampMs::from_millis(row.get(13)),
        updated_at: TimestampMs::from_millis(row.get(14)),
    })
}

fn classify_probe_error(err: &DennettError) -> ProviderHealthStatus {
    match err {
        DennettError::AuthRequired { .. } | DennettError::Unauthorized { .. } => {
            ProviderHealthStatus::AuthRequired
        }
        DennettError::ProviderQuotaExceeded { .. } => ProviderHealthStatus::QuotaLimited,
        DennettError::RuntimeVersionUnsupported { .. } => ProviderHealthStatus::Incompatible,
        _ => ProviderHealthStatus::Unreachable,
    }
}

impl HeadService {
    /// Lists all persisted provider connections alongside read-only `planned_providers`
    /// (`GET /v1/runtime/connections`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL failure.
    pub async fn list_provider_connections(
        &self,
    ) -> Result<ListProviderConnectionsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {CONNECTION_SELECT_COLS}
             FROM provider_connections
             ORDER BY is_default_interactive DESC, is_default_background DESC, created_at ASC"
        );
        let rows = conn
            .query(&sql, &[])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let connections = rows
            .iter()
            .map(decode_connection_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListProviderConnectionsResponse {
            connections,
            planned_providers: static_planned_providers(),
        })
    }

    /// Fetches a single persisted [`ProviderConnectionRecord`] by ID.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn get_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {CONNECTION_SELECT_COLS} FROM provider_connections WHERE connection_id = $1"
        );
        let row = conn
            .query_opt(&sql, &[&connection_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("provider_connection"),
                id: connection_id.to_string(),
            });
        };
        decode_connection_row(&row)
    }

    /// Creates a new provider connection (`POST /v1/runtime/connections`).
    ///
    /// Validates SSRF loopback rules for `local_http` endpoints before persisting.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::SafetyGuardRejected`],
    /// or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates endpoint/auth profile, maintains default-flag uniqueness, persists connection, and writes audit event"
    )]
    pub async fn create_provider_connection(
        &self,
        req: UpsertProviderConnectionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        req.validate()?;
        let normalized_endpoint = self.validate_connection_endpoint(&req)?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let connection_id = ProviderConnectionId::new();
        let adapter_id = req
            .adapter_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| req.provider_kind.default_adapter_id())
            .to_owned();
        let set_interactive = req.is_default_interactive.unwrap_or(false);
        let set_background = req.is_default_background.unwrap_or(false);
        let enabled = req.enabled.unwrap_or(true);

        if set_interactive {
            tx.execute(
                "UPDATE provider_connections SET is_default_interactive = FALSE WHERE is_default_interactive IS TRUE",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }
        if set_background {
            tx.execute(
                "UPDATE provider_connections SET is_default_background = FALSE WHERE is_default_background IS TRUE",
                &[],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let sql = format!(
            "INSERT INTO provider_connections (
                connection_id, provider_kind, display_name, adapter_id, auth_profile,
                endpoint_url, secret_slot_key, is_default_interactive, is_default_background,
                enabled, last_health_status
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'unknown')
             RETURNING {CONNECTION_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &sql,
                &[
                    &connection_id.as_uuid(),
                    &req.provider_kind.as_str(),
                    &req.display_name.trim(),
                    &adapter_id.as_str(),
                    &req.auth_profile.as_method_id(),
                    &normalized_endpoint,
                    &req.secret_slot_key.as_deref(),
                    &set_interactive,
                    &set_background,
                    &enabled,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let record = decode_connection_row(&row)?;
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "runtime_router",
            "provider_connection_created",
            &format!(
                "created provider connection `{}` ({connection_id}) kind={} profile={}",
                record.display_name, record.provider_kind, record.auth_profile
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("connection:{connection_id}"),
                event_type: "runtime.connection_created",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "connection_id": connection_id,
                    "provider_kind": record.provider_kind,
                    "auth_profile": record.auth_profile,
                    "enabled": record.enabled,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if set_interactive || set_background {
            self.invalidate_runtime_catalog_cache().await;
        }
        Ok(record)
    }

    /// Updates an existing provider connection (`PATCH /v1/runtime/connections/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::SafetyGuardRejected`],
    /// [`DennettError::NotFound`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "loads and locks row, validates mutated fields, updates default uniqueness and row, and writes audit event"
    )]
    pub async fn update_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: UpsertProviderConnectionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        req.validate()?;
        let normalized_endpoint = self.validate_connection_endpoint(&req)?;
        let _ = self.ensure_migrated().await?;

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing_sql = format!(
            "SELECT {CONNECTION_SELECT_COLS} FROM provider_connections WHERE connection_id = $1 FOR UPDATE"
        );
        let existing_row = tx
            .query_opt(&existing_sql, &[&connection_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(existing_row) = existing_row else {
            return Err(DennettError::NotFound {
                resource: String::from("provider_connection"),
                id: connection_id.to_string(),
            });
        };
        let existing = decode_connection_row(&existing_row)?;

        let next_interactive = req
            .is_default_interactive
            .unwrap_or(existing.is_default_interactive);
        let next_background = req
            .is_default_background
            .unwrap_or(existing.is_default_background);
        let next_enabled = req.enabled.unwrap_or(existing.enabled);
        let adapter_id = req
            .adapter_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| req.provider_kind.default_adapter_id())
            .to_owned();

        if next_interactive && !existing.is_default_interactive {
            tx.execute(
                "UPDATE provider_connections SET is_default_interactive = FALSE
                 WHERE is_default_interactive IS TRUE AND connection_id <> $1",
                &[&connection_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }
        if next_background && !existing.is_default_background {
            tx.execute(
                "UPDATE provider_connections SET is_default_background = FALSE
                 WHERE is_default_background IS TRUE AND connection_id <> $1",
                &[&connection_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let update_sql = format!(
            "UPDATE provider_connections
             SET provider_kind = $2,
                 display_name = $3,
                 adapter_id = $4,
                 auth_profile = $5,
                 endpoint_url = $6,
                 secret_slot_key = $7,
                 is_default_interactive = $8,
                 is_default_background = $9,
                 enabled = $10,
                 updated_at = GREATEST(now(), updated_at + interval '1 millisecond')
             WHERE connection_id = $1
             RETURNING {CONNECTION_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &update_sql,
                &[
                    &connection_id.as_uuid(),
                    &req.provider_kind.as_str(),
                    &req.display_name.trim(),
                    &adapter_id.as_str(),
                    &req.auth_profile.as_method_id(),
                    &normalized_endpoint,
                    &req.secret_slot_key.as_deref(),
                    &next_interactive,
                    &next_background,
                    &next_enabled,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let record = decode_connection_row(&row)?;
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "runtime_router",
            "provider_connection_updated",
            &format!(
                "updated provider connection `{}` ({connection_id}) enabled={}",
                record.display_name, record.enabled
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.evict_dynamic_runtime(connection_id).await;
        self.invalidate_runtime_catalog_cache().await;
        Ok(record)
    }

    /// Deletes a persisted provider connection (`DELETE /v1/runtime/connections/{id}`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn delete_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        correlation_id: &CorrelationId,
    ) -> Result<DeleteProviderConnectionResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let deleted = tx
            .execute(
                "DELETE FROM provider_connections WHERE connection_id = $1",
                &[&connection_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        if deleted == 0 {
            return Err(DennettError::NotFound {
                resource: String::from("provider_connection"),
                id: connection_id.to_string(),
            });
        }

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "runtime_router",
            "provider_connection_deleted",
            &format!("deleted provider connection {connection_id}"),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.evict_dynamic_runtime(connection_id).await;
        self.invalidate_runtime_catalog_cache().await;
        Ok(DeleteProviderConnectionResponse {
            connection_id,
            deleted: true,
        })
    }

    /// Activates `connection_id` as the default interactive, background, or both runtime
    /// (`POST /v1/runtime/connections/{id}/activate`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`] if disabled, or
    /// [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "atomically swaps interactive/background default flags inside one transaction and appends audit event"
    )]
    pub async fn activate_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        req: ActivateConnectionRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProviderConnectionRecord, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing_sql = format!(
            "SELECT {CONNECTION_SELECT_COLS} FROM provider_connections WHERE connection_id = $1 FOR UPDATE"
        );
        let row = tx
            .query_opt(&existing_sql, &[&connection_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("provider_connection"),
                id: connection_id.to_string(),
            });
        };
        let existing = decode_connection_row(&row)?;
        if !existing.enabled {
            return Err(DennettError::Conflict {
                resource: String::from("provider_connection"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "cannot activate disabled provider connection `{}` ({connection_id})",
                    existing.display_name
                ),
            });
        }

        let set_interactive = matches!(
            req.role,
            ConnectionActivationRole::Interactive | ConnectionActivationRole::Both
        );
        let set_background = matches!(
            req.role,
            ConnectionActivationRole::Background | ConnectionActivationRole::Both
        );

        if set_interactive {
            tx.execute(
                "UPDATE provider_connections SET is_default_interactive = FALSE
                 WHERE is_default_interactive IS TRUE AND connection_id <> $1",
                &[&connection_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }
        if set_background {
            tx.execute(
                "UPDATE provider_connections SET is_default_background = FALSE
                 WHERE is_default_background IS TRUE AND connection_id <> $1",
                &[&connection_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let updated_sql = format!(
            "UPDATE provider_connections
             SET is_default_interactive = CASE WHEN $2 THEN TRUE ELSE is_default_interactive END,
                 is_default_background = CASE WHEN $3 THEN TRUE ELSE is_default_background END,
                 updated_at = GREATEST(now(), updated_at + interval '1 millisecond')
             WHERE connection_id = $1
             RETURNING {CONNECTION_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &updated_sql,
                &[&connection_id.as_uuid(), &set_interactive, &set_background],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let record = decode_connection_row(&updated_row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "runtime_router",
            "provider_connection_activated",
            &format!(
                "activated provider connection `{}` ({connection_id}) for role={}",
                record.display_name, req.role
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("connection:{connection_id}"),
                event_type: "runtime.connection_activated",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "connection_id": connection_id,
                    "role": req.role,
                    "adapter_id": record.adapter_id,
                    "auth_profile": record.auth_profile,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.invalidate_runtime_catalog_cache().await;
        Ok(record)
    }

    /// Probes a specific provider connection live without silent fallback and persists updated
    /// health status/detail (`POST /v1/runtime/connections/{id}/probe`).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "probes runtime strictly without silent fallback, classifies health/catalog, persists health state, and audits"
    )]
    pub async fn probe_provider_connection(
        &self,
        connection_id: ProviderConnectionId,
        correlation_id: &CorrelationId,
    ) -> Result<ProbeConnectionResponse, DennettError> {
        let record = self.get_provider_connection(connection_id).await?;

        let probe_result = match self.resolve_runtime_for_connection(&record).await {
            Ok(runtime) => runtime.probe().await,
            Err(err) => Err(err),
        };

        let (health_status, health_detail, catalog) = match probe_result {
            Ok(desc) => {
                let catalog = RuntimeCatalogResponse {
                    adapter_id: desc.adapter_id,
                    auth_profile: desc.auth_profile,
                    is_simulated: desc.is_simulated,
                    agy_version: desc.agy_version,
                    tier: desc.tier,
                    account_label: crate::service::detect_os_account_label_pub(),
                    default_model_id: desc.default_model_id,
                    default_thinking_level: desc.default_thinking_level,
                    models: desc.models,
                    thinking_options: desc.thinking_options,
                    quotas: desc.quotas,
                    connection_id: Some(connection_id),
                    quota_error: None,
                    probed_at: TimestampMs::now(),
                };
                (ProviderHealthStatus::Healthy, None, Some(catalog))
            }
            Err(err) => {
                let status = classify_probe_error(&err);
                let safe_detail = self.redact(&err.to_string());
                let truncated = truncate_utf8(&safe_detail, 1000).to_owned();
                let catalog_quota = match &err {
                    DennettError::ProviderQuotaExceeded {
                        provider,
                        detail,
                        retry_after_secs,
                    } => Some(RuntimeCatalogResponse {
                        adapter_id: record.adapter_id.clone(),
                        auth_profile: record.auth_profile,
                        is_simulated: false,
                        agy_version: None,
                        tier: None,
                        account_label: None,
                        default_model_id: None,
                        default_thinking_level: None,
                        models: Vec::new(),
                        thinking_options: Vec::new(),
                        quotas: Vec::new(),
                        connection_id: Some(connection_id),
                        quota_error: Some(ProviderQuotaError {
                            provider: provider.clone(),
                            auth_profile: record.auth_profile,
                            detail: self.redact(detail),
                            retry_after_secs: *retry_after_secs,
                        }),
                        probed_at: TimestampMs::now(),
                    }),
                    _ => None,
                };
                (status, Some(truncated), catalog_quota)
            }
        };

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let update_sql = format!(
            "UPDATE provider_connections
             SET last_health_status = $2,
                 last_health_detail = $3,
                 last_probed_at = now()
             WHERE connection_id = $1
             RETURNING {CONNECTION_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &connection_id.as_uuid(),
                    &health_status.as_str(),
                    &health_detail.as_deref(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_connection_row(&updated_row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "runtime_router",
            "provider_connection_probed",
            &format!(
                "probed connection `{}` ({connection_id}) -> status={health_status}",
                updated.display_name
            ),
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(ProbeConnectionResponse {
            connection: updated,
            catalog,
        })
    }

    /// Resolves the active default interactive connection, if one is enabled in PostgreSQL.
    pub(crate) async fn fetch_default_interactive_connection(
        &self,
    ) -> Result<Option<ProviderConnectionRecord>, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {CONNECTION_SELECT_COLS}
             FROM provider_connections
             WHERE is_default_interactive IS TRUE AND enabled IS TRUE
             LIMIT 1"
        );
        let row = conn
            .query_opt(&sql, &[])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        row.as_ref().map(decode_connection_row).transpose()
    }

    /// Resolves the concrete [`AgentRuntimePort`] for an interactive chat turn or durable task
    /// generation strictly without silent provider fallback (A09 / Contract M).
    ///
    /// Precedence:
    /// 1. Explicit `auth_profile_override` (e.g. durable background task pinned to `gemini-api-key`
    ///    or `local-endpoint`).
    /// 2. Agent's `preferred_connection_id` from its immutable `AgentConfigVersion`.
    /// 3. Active default interactive `provider_connections` row (if configured).
    /// 4. Fallback to Head's primary interactive runtime only when no `provider_connections` default exists.
    pub(crate) async fn resolve_runtime_for_turn(
        &self,
        preferred_connection_id: Option<ProviderConnectionId>,
        auth_profile_override: Option<RuntimeAuthProfile>,
    ) -> Result<ResolvedTurnRuntime, DennettError> {
        // 1. Explicit auth_profile_override takes precedence for background/isolated tasks.
        if let Some(requested_profile) = auth_profile_override {
            // If an explicit preferred_connection_id already matches this profile, honor it.
            if let Some(conn_id) = preferred_connection_id {
                let record = self.get_provider_connection(conn_id).await?;
                if record.enabled && record.auth_profile == requested_profile {
                    let rt = self.resolve_runtime_for_connection(&record).await?;
                    return Ok(ResolvedTurnRuntime {
                        runtime: rt,
                        connection_id: Some(record.connection_id),
                    });
                }
            }

            // Look for an enabled provider_connection matching `requested_profile` (preferring default_background).
            if let Some(record) = self
                .find_enabled_connection_by_profile(requested_profile)
                .await?
            {
                let rt = self.resolve_runtime_for_connection(&record).await?;
                return Ok(ResolvedTurnRuntime {
                    runtime: rt,
                    connection_id: Some(record.connection_id),
                });
            }

            // Otherwise route directly via static RuntimeRouter (fails closed with AuthRequired if unconfigured).
            let rt = self.runtime_router().for_profile(Some(requested_profile))?;
            return Ok(ResolvedTurnRuntime {
                runtime: rt,
                connection_id: None,
            });
        }

        // 2. Agent's explicit `preferred_connection_id` (versioned on AgentConfigVersion).
        if let Some(conn_id) = preferred_connection_id {
            let record = self.get_provider_connection(conn_id).await?;
            if !record.enabled {
                return Err(DennettError::Conflict {
                    resource: String::from("provider_connection"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "preferred provider connection `{}` ({conn_id}) is disabled; refusing silent fallback",
                        record.display_name
                    ),
                });
            }
            let rt = self.resolve_runtime_for_connection(&record).await?;
            return Ok(ResolvedTurnRuntime {
                runtime: rt,
                connection_id: Some(record.connection_id),
            });
        }

        // 3. Default interactive provider connection from PostgreSQL.
        if let Some(default_conn) = self.fetch_default_interactive_connection().await? {
            let is_untouched_seeded_oauth = default_conn.connection_id
                == SEEDED_OAUTH_PERSONAL_CONNECTION_ID
                && default_conn.created_at == default_conn.updated_at;
            if is_untouched_seeded_oauth
                && self.runtime().auth_profile() != RuntimeAuthProfile::OAuthPersonal
            {
                // LIVE-A01-01: When `dennett-head` was explicitly booted with a non-OAuth
                // `--auth-profile` (e.g. `gemini-api-key`) and the migration seed row
                // `00000000-0000-4000-8000-000000000001` has never been explicitly edited
                // or activated by the user, honor the boot profile instead of silently
                // switching to `oauth-personal`.
                if !self.runtime().is_simulated()
                    && let Some(boot_conn) = self
                        .find_enabled_connection_by_profile(self.runtime().auth_profile())
                        .await?
                    && boot_conn.adapter_id == self.runtime().adapter_id()
                {
                    let rt = self.resolve_runtime_for_connection(&boot_conn).await?;
                    return Ok(ResolvedTurnRuntime {
                        runtime: rt,
                        connection_id: Some(boot_conn.connection_id),
                    });
                }
                return Ok(ResolvedTurnRuntime {
                    runtime: Arc::clone(self.runtime()),
                    connection_id: None,
                });
            }
            let rt = self.resolve_runtime_for_connection(&default_conn).await?;
            return Ok(ResolvedTurnRuntime {
                runtime: rt,
                connection_id: Some(default_conn.connection_id),
            });
        }

        // 4. Head's boot runtime when no default connection row is marked active.
        Ok(ResolvedTurnRuntime {
            runtime: Arc::clone(self.runtime()),
            connection_id: None,
        })
    }

    async fn find_enabled_connection_by_profile(
        &self,
        profile: RuntimeAuthProfile,
    ) -> Result<Option<ProviderConnectionRecord>, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {CONNECTION_SELECT_COLS}
             FROM provider_connections
             WHERE auth_profile = $1 AND enabled IS TRUE
             ORDER BY is_default_background DESC, is_default_interactive DESC, created_at ASC
             LIMIT 1"
        );
        let row = conn
            .query_opt(&sql, &[&profile.as_method_id()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        row.as_ref().map(decode_connection_row).transpose()
    }

    /// Instantiates or returns a cached concrete [`AgentRuntimePort`] for `record` strictly
    /// without silent fallback to another engine or authentication profile.
    #[expect(
        clippy::too_many_lines,
        reason = "constructs LocalHttpRuntime or AntigravityAcpRuntime strictly for the requested connection with zero silent fallback"
    )]
    pub(crate) async fn resolve_runtime_for_connection(
        &self,
        record: &ProviderConnectionRecord,
    ) -> Result<Arc<dyn AgentRuntimePort>, DennettError> {
        match record.provider_kind {
            ProviderKind::LocalHttp => {
                let raw_url =
                    record
                        .endpoint_url
                        .as_deref()
                        .ok_or_else(|| DennettError::InvalidInput {
                            field: String::from("endpoint_url"),
                            reason: format!(
                                "local_http connection `{}` is missing `endpoint_url`",
                                record.connection_id
                            ),
                        })?;
                let (bearer_token, secret_version) = match record.secret_slot_key.as_deref() {
                    Some(slot) => match self.resolve_secret_plaintext(slot).await? {
                        Some((val, _, ver)) => (Some(val.to_string()), Some(ver)),
                        None => (None, None),
                    },
                    None => (None, None),
                };

                {
                    let cache = self.dynamic_runtimes_lock().lock().await;
                    if let Some(cached) = cache.get(&record.connection_id)
                        && cached.endpoint_url.as_deref() == Some(raw_url)
                        && cached.secret_slot_key == record.secret_slot_key
                        && cached.secret_version == secret_version
                    {
                        return Ok(Arc::clone(&cached.runtime));
                    }
                }

                let local = Arc::new(LocalHttpRuntime::new(LocalRuntimeConfig {
                    endpoint_url: raw_url.to_owned(),
                    allow_non_loopback: self.allow_non_loopback_local_runtime(),
                    bearer_token,
                    default_model: None,
                    ..LocalRuntimeConfig::default()
                })?);
                let trait_obj: Arc<dyn AgentRuntimePort> = local.clone();

                let replaced = {
                    let mut cache = self.dynamic_runtimes_lock().lock().await;
                    cache.insert(
                        record.connection_id,
                        CachedDynamicRuntime {
                            runtime: Arc::clone(&trait_obj),
                            local_runtime: Some(local),
                            endpoint_url: Some(raw_url.to_owned()),
                            secret_slot_key: record.secret_slot_key.clone(),
                            secret_version,
                            auth_profile: RuntimeAuthProfile::LocalEndpoint,
                        },
                    )
                };
                if let Some(old) = replaced
                    && let Some(old_local) = old.local_runtime
                {
                    old_local.close_all_sessions().await;
                }
                Ok(trait_obj)
            }
            ProviderKind::AntigravityAcp => {
                // In simulated CI mode, strictly map onto injected interactive/background runtimes
                // and fail closed (`AuthRequired`) if the requested profile isn't configured!
                if self.runtime().is_simulated() {
                    return self.runtime_router().for_profile(Some(record.auth_profile));
                }

                // Live `agy acp` mode:
                match record.auth_profile {
                    RuntimeAuthProfile::OAuthPersonal => {
                        if self.runtime().auth_profile() == RuntimeAuthProfile::OAuthPersonal
                            && self.runtime().adapter_id() == ANTIGRAVITY_ADAPTER_ID
                        {
                            return Ok(Arc::clone(self.runtime()));
                        }
                        if let Some(bg) = self.background_runtime()
                            && bg.auth_profile() == RuntimeAuthProfile::OAuthPersonal
                        {
                            return Ok(Arc::clone(bg));
                        }
                        let cache = self.dynamic_runtimes_lock().lock().await;
                        if let Some(cached) = cache.get(&record.connection_id) {
                            return Ok(Arc::clone(&cached.runtime));
                        }
                        drop(cache);
                        let mut cfg = AntigravityRuntimeConfig::oauth_personal(
                            self.antigravity_profile_root(),
                        );
                        if let Some(bin) = self.antigravity_binary() {
                            cfg.binary = bin;
                        }
                        let rt: Arc<dyn AgentRuntimePort> =
                            Arc::new(AntigravityAcpRuntime::new(cfg));
                        self.dynamic_runtimes_lock().lock().await.insert(
                            record.connection_id,
                            CachedDynamicRuntime {
                                runtime: Arc::clone(&rt),
                                local_runtime: None,
                                endpoint_url: None,
                                secret_slot_key: None,
                                secret_version: None,
                                auth_profile: RuntimeAuthProfile::OAuthPersonal,
                            },
                        );
                        Ok(rt)
                    }
                    RuntimeAuthProfile::GeminiApiKey => {
                        let slot = record
                            .secret_slot_key
                            .as_deref()
                            .unwrap_or(GEMINI_API_KEY_SLOT);
                        let resolved = self.resolve_secret_plaintext(slot).await?;
                        let Some((api_key, _, secret_version)) = resolved else {
                            return Err(DennettError::AuthRequired {
                                profile: RuntimeAuthProfile::GeminiApiKey,
                                detail: format!(
                                    "secret slot `{slot}` is not configured in SecretBroker or GEMINI_API_KEY env; refusing silent fallback to oauth-personal"
                                ),
                            });
                        };

                        {
                            let cache = self.dynamic_runtimes_lock().lock().await;
                            if let Some(cached) = cache.get(&record.connection_id)
                                && cached.secret_slot_key.as_deref() == Some(slot)
                                && cached.secret_version == Some(secret_version)
                            {
                                return Ok(Arc::clone(&cached.runtime));
                            }
                        }

                        let mut cfg = AntigravityRuntimeConfig::gemini_api_key(
                            self.antigravity_profile_root(),
                            Some(api_key.to_string()),
                        );
                        if let Some(bin) = self.antigravity_binary() {
                            cfg.binary = bin;
                        }
                        let rt: Arc<dyn AgentRuntimePort> =
                            Arc::new(AntigravityAcpRuntime::new(cfg));
                        self.dynamic_runtimes_lock().lock().await.insert(
                            record.connection_id,
                            CachedDynamicRuntime {
                                runtime: Arc::clone(&rt),
                                local_runtime: None,
                                endpoint_url: None,
                                secret_slot_key: Some(slot.to_owned()),
                                secret_version: Some(secret_version),
                                auth_profile: RuntimeAuthProfile::GeminiApiKey,
                            },
                        );
                        Ok(rt)
                    }
                    _ => Err(DennettError::InvalidInput {
                        field: String::from("auth_profile"),
                        reason: format!(
                            "antigravity_acp connection cannot use auth_profile `{}`",
                            record.auth_profile
                        ),
                    }),
                }
            }
            _ => Err(DennettError::InvalidInput {
                field: String::from("provider_kind"),
                reason: format!("unsupported provider_kind `{}`", record.provider_kind),
            }),
        }
    }

    /// Hot-invalidates cached runtimes referencing `slot_key` after secret rotation or deletion.
    pub(crate) async fn invalidate_runtimes_for_secret_slot(&self, slot_key: &str) {
        let mut to_close_locals = Vec::new();
        {
            let mut cache = self.dynamic_runtimes_lock().lock().await;
            let matching_ids: Vec<ProviderConnectionId> = cache
                .iter()
                .filter(|(_, entry)| {
                    entry.secret_slot_key.as_deref() == Some(slot_key)
                        || (slot_key == GEMINI_API_KEY_SLOT
                            && entry.auth_profile == RuntimeAuthProfile::GeminiApiKey)
                })
                .map(|(id, _)| *id)
                .collect();
            for id in matching_ids {
                if let Some(removed) = cache.remove(&id)
                    && let Some(local) = removed.local_runtime
                {
                    to_close_locals.push(local);
                }
            }
        }
        for local in to_close_locals {
            local.set_bearer_token(None);
            local.close_all_sessions().await;
        }
        self.invalidate_runtime_catalog_cache().await;
    }

    pub(crate) async fn evict_dynamic_runtime(&self, connection_id: ProviderConnectionId) {
        let removed = self
            .dynamic_runtimes_lock()
            .lock()
            .await
            .remove(&connection_id);
        if let Some(entry) = removed
            && let Some(local) = entry.local_runtime
        {
            local.close_all_sessions().await;
        }
    }

    fn validate_connection_endpoint(
        &self,
        req: &UpsertProviderConnectionRequest,
    ) -> Result<Option<String>, DennettError> {
        match req.provider_kind {
            ProviderKind::LocalHttp => {
                let raw =
                    req.endpoint_url
                        .as_deref()
                        .ok_or_else(|| DennettError::InvalidInput {
                            field: String::from("endpoint_url"),
                            reason: String::from("local_http connections require `endpoint_url`"),
                        })?;
                let validated =
                    validate_local_endpoint_url(raw, self.allow_non_loopback_local_runtime())?;
                Ok(Some(validated.origin_base))
            }
            ProviderKind::AntigravityAcp => Ok(None),
            _ => Ok(req.endpoint_url.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use dennett_runtime_local::LOCAL_HTTP_ADAPTER_ID;

    use super::*;

    #[test]
    fn planned_providers_are_read_only_and_explicitly_planned() {
        let planned = static_planned_providers();
        assert_eq!(planned.len(), 2);
        assert!(planned.iter().all(|p| p.status == "planned"));
        assert!(planned.iter().any(|p| p.id == "r-claude"));
        assert!(planned.iter().any(|p| p.id == "r-codex"));
        assert_eq!(LOCAL_HTTP_ADAPTER_ID, "local-http-runtime");
    }
}
