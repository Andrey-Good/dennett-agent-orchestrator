//! External effect port (`ExternalEffectPort`), deterministic `SimulatedExternalConnector`,
//! `EffectClaim` protocol, `LostAck -> UNKNOWN` state protection, and explicit reconciliation
//! (Contract B / `50 §11.3–§11.5`).
//!
//! Safety invariants:
//! - External network/connector I/O NEVER runs inside an open PostgreSQL transaction.
//! - Reusing `effect_idempotency_key` with different parameters fails closed (`IdempotencyMismatch`).
//! - On `LostAck` or daemon crash during `dispatching`, the claim is persisted as `unknown`
//!   (linked task -> `unknown_external`) and blind retry is STRICTLY REFUSED across retries and
//!   full `dennett-head` restarts.
//! - Explicit reconciliation (`POST /v1/effects/{effect_id}/reconcile`) inspects provider state
//!   with ZERO repeat dispatches (`provider_execution_count` stays `1`).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use dennett_contracts::{
    CorrelationId, DennettError, DispatchEffectRequest, DispatchEffectResponse, EffectClaimId,
    EffectClaimRecord, ExternalEffectStatus, InboxCardId, ListEffectsResponse,
    ManualEffectResolution, ReconcileEffectRequest, ReconcileEffectResponse, SimEffectBehavior,
    TaskId, TaskStatus,
};
use dennett_runtime_antigravity::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use uuid::Uuid;

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::migrations::sha256_hex;
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{
    EFFECT_CLAIM_SELECT_COLS, append_audit_event, decode_effect_claim_row, fetch_agent,
    fetch_project, fetch_session, fetch_task, map_pg_err, truncate_utf8,
};

/// Default connector identifier used in Phase D2 for deterministic external effect verification.
pub const SIMULATED_EXTERNAL_CONNECTOR_ID: &str = "simulated_external";

/// Input passed to [`ExternalEffectPort::dispatch`] outside any open SQL transaction.
#[derive(Debug, Clone)]
pub struct ExternalDispatchInput {
    /// Globally unique idempotency key forwarded to the external provider.
    pub idempotency_key: String,
    /// Connector identifier.
    pub connector_id: String,
    /// Operation name (e.g. `"send_webhook"`).
    pub operation: String,
    /// Target recipient or endpoint.
    pub target_recipient: String,
    /// Serialized JSON parameters.
    pub parameters_json: JsonValue,
    /// Deterministic simulated behavior for D2 fault injection.
    pub simulated_behavior: SimEffectBehavior,
}

/// Receipt returned when an external provider confirms execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEffectReceipt {
    /// Provider-assigned opaque operation identifier.
    pub provider_operation_id: String,
    /// Serialized provider receipt metadata.
    pub receipt_json: JsonValue,
    /// Total execution count recorded at the provider for this idempotency key.
    pub provider_execution_count: u64,
}

/// Typed outcome returned by [`ExternalEffectPort::dispatch`].
#[derive(Debug, Clone)]
pub enum ExternalDispatchOutcome {
    /// Provider executed and acknowledged the operation cleanly.
    Confirmed(ProviderEffectReceipt),
    /// Provider explicitly refused execution (`provider_execution_count == 0`).
    Denied {
        /// Safe secret-free rejection explanation.
        reason: String,
    },
    /// Transport/provider failed cleanly BEFORE accepting execution (`provider_execution_count == 0`).
    FailedBeforeAccept {
        /// Safe failure summary.
        reason: String,
    },
    /// Operation may have been executed remotely (in simulation: WAS executed!), but acknowledgement
    /// was lost. Caller MUST transition claim to `unknown` and refuse blind retries!
    LostAck {
        /// Safe explanation of the lost acknowledgement.
        reason: String,
    },
}

/// Reconciliation lookup result returned by [`ExternalEffectPort::reconcile`].
#[derive(Debug, Clone)]
pub struct ExternalReconcileResult {
    /// Confirmed provider receipt if the provider has a record of executing `idempotency_key`.
    pub receipt: Option<ProviderEffectReceipt>,
    /// Total execution count observed at the provider for this `idempotency_key`.
    pub provider_execution_count: u64,
}

/// Narrow port isolating `dennett-head` from concrete external connectors (Contract B).
pub trait ExternalEffectPort: Send + Sync {
    /// Dispatches one external effect outside any SQL transaction.
    fn dispatch(&self, input: ExternalDispatchInput) -> BoxFuture<'_, ExternalDispatchOutcome>;

    /// Queries the external provider to reconcile an `unknown` effect WITHOUT re-executing it.
    fn reconcile<'a>(
        &'a self,
        idempotency_key: &'a str,
    ) -> BoxFuture<'a, Result<ExternalReconcileResult, DennettError>>;

    /// Returns the observed provider-side execution count for `idempotency_key` (0 if never executed).
    fn provider_execution_count(&self, idempotency_key: &str) -> u64;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SimProviderRecord {
    provider_operation_id: String,
    operation: String,
    target_recipient: String,
    execution_count: u64,
    receipt_json: JsonValue,
}

/// Deterministic D2 external connector supporting `Success`, `Denied`, `FailedBeforeAccept`,
/// and `LostAck`, with optional JSON ledger persistence across `dennett-head` process restarts.
pub struct SimulatedExternalConnector {
    ledger_path: Option<PathBuf>,
    ledger: Mutex<HashMap<String, SimProviderRecord>>,
}

impl SimulatedExternalConnector {
    /// Creates an in-memory [`SimulatedExternalConnector`] (suitable for unit tests).
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            ledger_path: None,
            ledger: Mutex::new(HashMap::new()),
        }
    }

    /// Creates a [`SimulatedExternalConnector`] backed by an isolated JSON ledger file so
    /// provider state survives full `dennett-head` process restarts during D2 tests.
    #[must_use]
    pub fn with_ledger_file(path: PathBuf) -> Self {
        let initial = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<HashMap<String, SimProviderRecord>>(&raw).ok())
            .unwrap_or_default();
        Self {
            ledger_path: Some(path),
            ledger: Mutex::new(initial),
        }
    }

    fn persist_locked(&self, map: &HashMap<String, SimProviderRecord>) {
        let Some(ref path) = self.ledger_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(map) {
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, &bytes).is_ok() {
                let _ = fs::rename(&tmp, path);
            } else {
                let _ = fs::write(path, &bytes);
            }
        }
    }
}

impl Default for SimulatedExternalConnector {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl ExternalEffectPort for SimulatedExternalConnector {
    fn dispatch(&self, input: ExternalDispatchInput) -> BoxFuture<'_, ExternalDispatchOutcome> {
        Box::pin(async move {
            match input.simulated_behavior {
                SimEffectBehavior::Denied => ExternalDispatchOutcome::Denied {
                    reason: format!(
                        "simulated provider denied `{}` to `{}`",
                        input.operation, input.target_recipient
                    ),
                },
                SimEffectBehavior::FailedBeforeAccept => {
                    ExternalDispatchOutcome::FailedBeforeAccept {
                        reason: format!(
                            "simulated transport failed before provider accepted `{}`",
                            input.operation
                        ),
                    }
                }
                SimEffectBehavior::Success => {
                    let receipt = {
                        let Ok(mut guard) = self.ledger.lock() else {
                            return ExternalDispatchOutcome::FailedBeforeAccept {
                                reason: String::from("simulated connector mutex poisoned"),
                            };
                        };
                        let entry =
                            guard
                                .entry(input.idempotency_key.clone())
                                .or_insert_with(|| {
                                    let op_id = format!(
                                        "sim-ext-{}",
                                        &Uuid::new_v4().simple().to_string()[..12]
                                    );
                                    SimProviderRecord {
                                        provider_operation_id: op_id.clone(),
                                        operation: input.operation.clone(),
                                        target_recipient: input.target_recipient.clone(),
                                        execution_count: 0,
                                        receipt_json: json!({
                                            "provider": SIMULATED_EXTERNAL_CONNECTOR_ID,
                                            "provider_operation_id": op_id,
                                            "operation": input.operation,
                                            "recipient": input.target_recipient,
                                            "accepted": true
                                        }),
                                    }
                                });
                        entry.execution_count = entry.execution_count.saturating_add(1);
                        let res = ProviderEffectReceipt {
                            provider_operation_id: entry.provider_operation_id.clone(),
                            receipt_json: entry.receipt_json.clone(),
                            provider_execution_count: entry.execution_count,
                        };
                        self.persist_locked(&guard);
                        res
                    };
                    ExternalDispatchOutcome::Confirmed(receipt)
                }
                SimEffectBehavior::LostAck => {
                    // CRITICAL CONTRACT B SCENARIO:
                    // Provider EXECUTES the effect (`execution_count = 1`), persists it in its own
                    // ledger, and then drops the acknowledgement back to Head!
                    if let Ok(mut guard) = self.ledger.lock() {
                        let entry =
                            guard
                                .entry(input.idempotency_key.clone())
                                .or_insert_with(|| {
                                    let op_id = format!(
                                        "sim-ext-{}",
                                        &Uuid::new_v4().simple().to_string()[..12]
                                    );
                                    SimProviderRecord {
                                        provider_operation_id: op_id.clone(),
                                        operation: input.operation.clone(),
                                        target_recipient: input.target_recipient.clone(),
                                        execution_count: 0,
                                        receipt_json: json!({
                                            "provider": SIMULATED_EXTERNAL_CONNECTOR_ID,
                                            "provider_operation_id": op_id,
                                            "operation": input.operation,
                                            "recipient": input.target_recipient,
                                            "accepted": true,
                                            "note": "executed remotely before acknowledgement drop"
                                        }),
                                    }
                                });
                        entry.execution_count = entry.execution_count.saturating_add(1);
                        self.persist_locked(&guard);
                    }
                    ExternalDispatchOutcome::LostAck {
                        reason: format!(
                            "acknowledgement lost after dispatching `{}` to `{}`; remote execution outcome is UNKNOWN",
                            input.operation, input.target_recipient
                        ),
                    }
                }
                _ => ExternalDispatchOutcome::FailedBeforeAccept {
                    reason: String::from("unsupported simulated effect behavior"),
                },
            }
        })
    }

    fn reconcile<'a>(
        &'a self,
        idempotency_key: &'a str,
    ) -> BoxFuture<'a, Result<ExternalReconcileResult, DennettError>> {
        Box::pin(async move {
            let guard = self.ledger.lock().map_err(|_| DennettError::Internal {
                detail: String::from("simulated connector mutex poisoned"),
            })?;
            // Zero new executions: strictly read-only provider query!
            if let Some(entry) = guard.get(idempotency_key) {
                Ok(ExternalReconcileResult {
                    receipt: Some(ProviderEffectReceipt {
                        provider_operation_id: entry.provider_operation_id.clone(),
                        receipt_json: entry.receipt_json.clone(),
                        provider_execution_count: entry.execution_count,
                    }),
                    provider_execution_count: entry.execution_count,
                })
            } else {
                Ok(ExternalReconcileResult {
                    receipt: None,
                    provider_execution_count: 0,
                })
            }
        })
    }

    fn provider_execution_count(&self, idempotency_key: &str) -> u64 {
        self.ledger
            .lock()
            .ok()
            .and_then(|g| g.get(idempotency_key).map(|e| e.execution_count))
            .unwrap_or(0)
    }
}

impl HeadService {
    /// Returns the total execution count observed by the active [`ExternalEffectPort`] for
    /// `idempotency_key` (for automated Contract B duplicate-prevention verification).
    #[must_use]
    pub fn provider_effect_execution_count(&self, idempotency_key: &str) -> u64 {
        self.effect_port().provider_execution_count(idempotency_key)
    }

    /// Startup crash recovery: transitions any `effect_claims` left stranded in `dispatching`
    /// across a hard `dennett-head` restart into `unknown` (and their linked tasks into
    /// `unknown_external`) with an open `unknown_effect` Inbox card — NEVER blindly re-sending!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::DatabaseUnavailable`] on PostgreSQL error.
    pub async fn recover_stale_dispatching_effects(
        &self,
        correlation_id: &CorrelationId,
    ) -> Result<usize, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let sql = format!(
            "UPDATE effect_claims
             SET status = 'unknown',
                 error_summary = COALESCE(error_summary, 'dennett-head restarted while external dispatch was in flight; outcome unknown'),
                 updated_at = now()
             WHERE status = 'dispatching'
             RETURNING {EFFECT_CLAIM_SELECT_COLS}"
        );
        let rows = tx
            .query(&sql, &[])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let mut recovered = 0;
        for row in &rows {
            let claim = decode_effect_claim_row(row)?;
            self.mark_unknown_effect_side_effects_in_tx(&tx, &claim, correlation_id)
                .await?;
            recovered += 1;
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        Ok(recovered)
    }

    /// Dispatches one external effect through [`ExternalEffectPort`] following the 2-transaction
    /// `EffectClaim` protocol (`50 §11.3`), keeping all external I/O strictly outside SQL locks.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::NotFound`],
    /// [`DennettError::IdempotencyMismatch`] (on conflicting key reuse), [`DennettError::Cancelled`]
    /// (if linked task/session is already stopped), or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "EffectClaim 2-phase commit cleanly separates pre-dispatch claim, out-of-tx external dispatch, and post-dispatch status/inbox recording"
    )]
    pub async fn dispatch_external_effect(
        &self,
        req: DispatchEffectRequest,
        correlation_id: &CorrelationId,
    ) -> Result<DispatchEffectResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let params_val: JsonValue = serde_json::from_str(&req.parameters_json).map_err(|err| {
            DennettError::InvalidInput {
                field: String::from("parameters_json"),
                reason: format!("must be valid JSON: {err}"),
            }
        })?;
        let connector_id = req
            .connector_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(SIMULATED_EXTERNAL_CONNECTOR_ID)
            .to_owned();
        let safe_recipient = self.redact(req.target_recipient.trim());
        let safe_params_str = self.redact(&params_val.to_string());
        let safe_params_val: JsonValue =
            serde_json::from_str(&safe_params_str).unwrap_or_else(|_| json!({}));
        let fingerprint_src = format!(
            "{connector_id}\0{}\0{safe_recipient}\0{}",
            req.operation.trim(),
            safe_params_val
        );
        let params_sha = sha256_hex(fingerprint_src.as_bytes());

        // Block any new external effect immediately if the linked task is already cancelled!
        if let Some(tid) = req.task_id
            && self.is_task_cancel_signalled(tid).await
        {
            return Err(DennettError::Cancelled {
                operation: format!("external_effect:{}:{}", connector_id, req.operation),
            });
        }

        // Transaction 1: Validate entities, deduplicate or refuse blind retry on `unknown`/`dispatching`,
        // and persist `dispatching` claim before releasing the SQL transaction.
        let (claim, already_existing) = {
            let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;

            let _ = fetch_project(&tx, req.project_id, self.pg_target()).await?;
            let _ = fetch_session(&tx, req.session_id, self.pg_target()).await?;
            let _ = fetch_agent(&tx, req.agent_id, self.pg_target()).await?;

            if let Some(tid) = req.task_id {
                let task = fetch_task(&tx, tid, self.pg_target()).await?;
                if task.status == TaskStatus::Cancelled {
                    return Err(DennettError::Cancelled {
                        operation: format!("external_effect:task:{tid}"),
                    });
                }
            }

            let existing_sql = format!(
                "SELECT {EFFECT_CLAIM_SELECT_COLS}
                 FROM effect_claims
                 WHERE effect_idempotency_key = $1
                 FOR UPDATE"
            );
            if let Some(row) = tx
                .query_opt(&existing_sql, &[&req.effect_idempotency_key.as_str()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
            {
                let existing = decode_effect_claim_row(&row)?;
                if existing.parameters_sha256 != params_sha {
                    return Err(DennettError::IdempotencyMismatch {
                        idempotency_key: req.effect_idempotency_key.to_string(),
                        reason: String::from(
                            "reused effect_idempotency_key with different operation, recipient, or parameters",
                        ),
                    });
                }
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                (existing, true)
            } else {
                let effect_id = EffectClaimId::new();
                let insert_sql = format!(
                    "INSERT INTO effect_claims (
                        effect_id, project_id, session_id, task_id, run_id, agent_id,
                        effect_idempotency_key, connector_id, operation, target_recipient,
                        parameters_json, parameters_sha256, status, correlation_id
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'dispatching', $13)
                     RETURNING {EFFECT_CLAIM_SELECT_COLS}"
                );
                let row = tx
                    .query_one(
                        &insert_sql,
                        &[
                            &effect_id.as_uuid(),
                            &req.project_id.as_uuid(),
                            &req.session_id.as_uuid(),
                            &req.task_id.map(TaskId::as_uuid),
                            &req.run_id.map(dennett_contracts::RunId::as_uuid),
                            &req.agent_id.as_uuid(),
                            &req.effect_idempotency_key.as_str(),
                            &connector_id,
                            &req.operation.trim(),
                            &truncate_utf8(&safe_recipient, 512),
                            &safe_params_val,
                            &params_sha.as_str(),
                            &correlation_id.as_str(),
                        ],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                let created = decode_effect_claim_row(&row)?;
                tx.commit()
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                (created, false)
            }
        };

        // Existing claim: NEVER blindly re-dispatch!
        // - `Confirmed`: return deduplicated receipt.
        // - `Unknown` or `Dispatching`: refuse blind retry, report `external_outcome_unknown = true`.
        // - `Denied` / `Failed`: return deduplicated terminal outcome (caller must mint a new key to retry deliberately).
        if already_existing {
            let unknown = matches!(
                claim.status,
                ExternalEffectStatus::Unknown | ExternalEffectStatus::Dispatching
            );
            return Ok(DispatchEffectResponse {
                effect: claim,
                deduplicated: true,
                external_outcome_unknown: unknown,
            });
        }

        // External connector call strictly OUTSIDE any SQL transaction!
        let dispatch_outcome = self
            .effect_port()
            .dispatch(ExternalDispatchInput {
                idempotency_key: claim.effect_idempotency_key.as_str().to_owned(),
                connector_id: claim.connector_id.clone(),
                operation: claim.operation.clone(),
                target_recipient: claim.target_recipient.clone(),
                parameters_json: safe_params_val,
                simulated_behavior: req.simulated_behavior.unwrap_or_default(),
            })
            .await;

        // Transaction 2: Persist terminal or `unknown` status + Inbox card + control event.
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let (new_status, provider_op_id, receipt_json, error_summary, event_type) =
            match dispatch_outcome {
                ExternalDispatchOutcome::Confirmed(receipt) => (
                    ExternalEffectStatus::Confirmed,
                    Some(receipt.provider_operation_id),
                    Some(receipt.receipt_json),
                    None,
                    "effect.confirmed",
                ),
                ExternalDispatchOutcome::Denied { reason } => (
                    ExternalEffectStatus::Denied,
                    None,
                    None,
                    Some(self.redact(&reason)),
                    "effect.denied",
                ),
                ExternalDispatchOutcome::FailedBeforeAccept { reason } => (
                    ExternalEffectStatus::Failed,
                    None,
                    None,
                    Some(self.redact(&reason)),
                    "effect.failed",
                ),
                ExternalDispatchOutcome::LostAck { reason } => (
                    ExternalEffectStatus::Unknown,
                    None,
                    None,
                    Some(self.redact(&reason)),
                    "effect.unknown",
                ),
            };

        let update_sql = format!(
            "UPDATE effect_claims
             SET status = $2,
                 provider_operation_id = $3,
                 receipt_json = $4,
                 error_summary = $5,
                 updated_at = now()
             WHERE effect_id = $1
             RETURNING {EFFECT_CLAIM_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &claim.effect_id.as_uuid(),
                    &new_status.as_str(),
                    &provider_op_id,
                    &receipt_json,
                    &error_summary
                        .as_deref()
                        .map(|s| truncate_utf8(s, 2000).to_owned()),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_claim = decode_effect_claim_row(&updated_row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(updated_claim.session_id),
            updated_claim.run_id,
            "external_effects",
            event_type,
            &self.redact(&format!(
                "effect {} ({} -> {}) status={}",
                updated_claim.effect_id,
                updated_claim.operation,
                updated_claim.target_recipient,
                updated_claim.status
            )),
        )
        .await?;

        if updated_claim.status == ExternalEffectStatus::Unknown {
            self.mark_unknown_effect_side_effects_in_tx(&tx, &updated_claim, correlation_id)
                .await?;
        } else {
            let entity_key = updated_claim.task_id.map_or_else(
                || format!("effect:{}", updated_claim.effect_id),
                |tid| format!("task:{tid}"),
            );
            let _ = append_control_event_in_tx(
                &tx,
                self.pg_target(),
                ControlEventAppend {
                    entity_key: &entity_key,
                    event_type,
                    project_id: Some(updated_claim.project_id),
                    session_id: Some(updated_claim.session_id),
                    task_id: updated_claim.task_id,
                    correlation_id,
                    payload: json!({
                        "effect_id": updated_claim.effect_id,
                        "status": updated_claim.status,
                        "operation": updated_claim.operation,
                        "target_recipient": updated_claim.target_recipient,
                        "provider_operation_id": updated_claim.provider_operation_id,
                    }),
                },
            )
            .await?;
        }

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let unknown = updated_claim.status == ExternalEffectStatus::Unknown;
        Ok(DispatchEffectResponse {
            effect: updated_claim,
            deduplicated: false,
            external_outcome_unknown: unknown,
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "transitions linked task to unknown_external, creates high-urgency unknown_effect inbox card, and appends audit + control journal events atomically"
    )]
    pub(crate) async fn mark_unknown_effect_side_effects_in_tx<
        C: deadpool_postgres::GenericClient,
    >(
        &self,
        tx: &C,
        claim: &EffectClaimRecord,
        correlation_id: &CorrelationId,
    ) -> Result<(), DennettError> {
        if let Some(tid) = claim.task_id {
            let wait_msg = truncate_utf8(
                &self.redact(&format!(
                    "Внешний эффект `{}` ({}) в статусе UNKNOWN — требуется сверка",
                    claim.operation, claim.target_recipient
                )),
                2000,
            )
            .to_owned();
            tx.execute(
                "UPDATE tasks
                 SET status = 'unknown_external',
                     wait_detail = $2,
                     updated_at = now()
                 WHERE task_id = $1",
                &[&tid.as_uuid(), &wait_msg],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }
        if let Some(rid) = claim.run_id {
            tx.execute(
                "UPDATE runs
                 SET status = 'unknown_external',
                     stop_reason = COALESCE(stop_reason, 'unknown_external'),
                     finished_at = COALESCE(finished_at, now())
                 WHERE run_id = $1",
                &[&rid.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        // Open an Action Inbox card for this unknown effect if one is not already open.
        let existing_card = tx
            .query_opt(
                "SELECT card_id FROM inbox_cards
                 WHERE effect_id = $1 AND status = 'open'
                 LIMIT 1",
                &[&claim.effect_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        if existing_card.is_none() {
            let card_id = InboxCardId::new();
            let subject = truncate_utf8(
                &self.redact(&format!(
                    "Неизвестно, выполнено ли: {} ({})",
                    claim.operation, claim.target_recipient
                )),
                500,
            )
            .to_owned();
            let detail = truncate_utf8(
                &self.redact(&format!(
                    "Запрос `{}` к `{}` (ключ `{}`) отправлен, но подтверждение потеряно. Слепой повтор заблокирован; выполните сверку у сервиса.",
                    claim.operation, claim.target_recipient, claim.effect_idempotency_key
                )),
                4000,
            )
            .to_owned();
            tx.execute(
                "INSERT INTO inbox_cards (
                    card_id, project_id, session_id, task_id, run_id, agent_id,
                    kind, status, urgency, subject, detail, effect_id
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'unknown_effect', 'open', 'critical', $7, $8, $9)",
                &[
                    &card_id.as_uuid(),
                    &claim.project_id.as_uuid(),
                    &claim.session_id.as_uuid(),
                    &claim.task_id.map(TaskId::as_uuid),
                    &claim.run_id.map(dennett_contracts::RunId::as_uuid),
                    &claim.agent_id.as_uuid(),
                    &subject,
                    &detail,
                    &claim.effect_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let entity_key = claim.task_id.map_or_else(
            || format!("effect:{}", claim.effect_id),
            |tid| format!("task:{tid}"),
        );
        let _ = append_control_event_in_tx(
            tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &entity_key,
                event_type: "effect.unknown",
                project_id: Some(claim.project_id),
                session_id: Some(claim.session_id),
                task_id: claim.task_id,
                correlation_id,
                payload: json!({
                    "effect_id": claim.effect_id,
                    "effect_idempotency_key": claim.effect_idempotency_key,
                    "operation": claim.operation,
                    "target_recipient": claim.target_recipient,
                    "status": "unknown",
                }),
            },
        )
        .await?;

        Ok(())
    }

    /// Lists persisted `effect_claims` with optional `task_id`, `session_id`, and `status` filters.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_effects(
        &self,
        task_id: Option<TaskId>,
        session_id: Option<dennett_contracts::SessionId>,
        status_filter: Option<&str>,
    ) -> Result<ListEffectsResponse, DennettError> {
        let parsed_status = status_filter
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ExternalEffectStatus::from_str)
            .transpose()?;

        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let sql = format!(
            "SELECT {EFFECT_CLAIM_SELECT_COLS}
             FROM effect_claims
             WHERE ($1::uuid IS NULL OR task_id = $1)
               AND ($2::uuid IS NULL OR session_id = $2)
               AND ($3::text IS NULL OR status = $3)
             ORDER BY created_at DESC"
        );
        let status_str = parsed_status.map(|s| s.as_str().to_owned());
        let rows = conn
            .query(
                &sql,
                &[
                    &task_id.map(TaskId::as_uuid),
                    &session_id.map(dennett_contracts::SessionId::as_uuid),
                    &status_str,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let effects = rows
            .iter()
            .map(decode_effect_claim_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListEffectsResponse { effects })
    }

    /// Reconciles an `unknown` external effect claim via [`ExternalEffectPort::reconcile`] or
    /// explicit manual owner resolution (`POST /v1/effects/{effect_id}/reconcile`) with ZERO
    /// duplicate provider dispatches.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "reconciliation coordinates provider lookup outside SQL transaction with atomic claim/inbox/task resolution"
    )]
    pub async fn reconcile_external_effect(
        &self,
        effect_id: EffectClaimId,
        req: ReconcileEffectRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ReconcileEffectResponse, DennettError> {
        let _ = self.ensure_migrated().await?;

        let claim = {
            let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
            let sql = format!(
                "SELECT {EFFECT_CLAIM_SELECT_COLS} FROM effect_claims WHERE effect_id = $1"
            );
            let row = conn
                .query_opt(&sql, &[&effect_id.as_uuid()])
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let Some(row) = row else {
                return Err(DennettError::NotFound {
                    resource: String::from("effect_claim"),
                    id: effect_id.to_string(),
                });
            };
            decode_effect_claim_row(&row)?
        };

        // Query external provider strictly outside SQL transaction (read-only lookup: 0 retries!).
        let provider_lookup = self
            .effect_port()
            .reconcile(claim.effect_idempotency_key.as_str())
            .await?;

        let (resolved_status, provider_op_id, receipt_json, provider_found, resolution_note) =
            match req.manual_resolution {
                Some(ManualEffectResolution::MarkConfirmed) => (
                    ExternalEffectStatus::Confirmed,
                    provider_lookup
                        .receipt
                        .as_ref()
                        .map(|r| r.provider_operation_id.clone())
                        .or_else(|| claim.provider_operation_id.clone()),
                    provider_lookup
                        .receipt
                        .as_ref()
                        .map(|r| r.receipt_json.clone())
                        .or_else(|| {
                            Some(json!({
                                "manual_resolution": "mark_confirmed",
                                "reason": req.reason.as_deref().unwrap_or("confirmed by owner"),
                            }))
                        }),
                    provider_lookup.receipt.is_some(),
                    req.reason
                        .as_deref()
                        .unwrap_or("Владелец вручную подтвердил выполнение внешнего эффекта")
                        .to_owned(),
                ),
                Some(ManualEffectResolution::MarkNotExecuted) => (
                    ExternalEffectStatus::Failed,
                    None,
                    None,
                    false,
                    req.reason
                        .as_deref()
                        .unwrap_or("Владелец пометил внешний эффект как не выполненный")
                        .to_owned(),
                ),
                None => {
                    if let Some(receipt) = provider_lookup.receipt {
                        (
                            ExternalEffectStatus::Confirmed,
                            Some(receipt.provider_operation_id),
                            Some(receipt.receipt_json),
                            true,
                            String::from(
                                "Сверка с внешним сервисом подтвердила однократное выполнение без повторной отправки",
                            ),
                        )
                    } else if claim.status == ExternalEffectStatus::Confirmed {
                        (
                            ExternalEffectStatus::Confirmed,
                            claim.provider_operation_id.clone(),
                            claim
                                .receipt_json
                                .as_deref()
                                .and_then(|s| serde_json::from_str::<JsonValue>(s).ok()),
                            false,
                            String::from("Эффект уже был подтверждён ранее"),
                        )
                    } else {
                        (
                            ExternalEffectStatus::Failed,
                            None,
                            None,
                            false,
                            String::from(
                                "Внешний сервис не зафиксировал выполнение по ключу идемпотентности",
                            ),
                        )
                    }
                }
                Some(_) => {
                    return Err(DennettError::InvalidInput {
                        field: String::from("manual_resolution"),
                        reason: String::from("unsupported manual effect resolution"),
                    });
                }
            };

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let update_sql = format!(
            "UPDATE effect_claims
             SET status = $2,
                 provider_operation_id = COALESCE($3, provider_operation_id),
                 receipt_json = COALESCE($4, receipt_json),
                 error_summary = CASE WHEN $2 = 'confirmed' THEN NULL ELSE $5 END,
                 reconciled_at = now(),
                 updated_at = now()
             WHERE effect_id = $1
             RETURNING {EFFECT_CLAIM_SELECT_COLS}"
        );
        let safe_note = truncate_utf8(&self.redact(&resolution_note), 2000).to_owned();
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &effect_id.as_uuid(),
                    &resolved_status.as_str(),
                    &provider_op_id,
                    &receipt_json,
                    &safe_note,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated_claim = decode_effect_claim_row(&updated_row)?;

        // Close open `unknown_effect` Action Inbox cards for this effect.
        tx.execute(
            "UPDATE inbox_cards
             SET status = 'resolved',
                 resolution_summary = $2,
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE effect_id = $1 AND status = 'open'",
            &[&effect_id.as_uuid(), &safe_note],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Update linked task out of `unknown_external` if no other unknown effects remain.
        let mut updated_task = None;
        if let Some(tid) = updated_claim.task_id {
            let remaining_unknown: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM effect_claims
                     WHERE task_id = $1 AND status IN ('unknown', 'dispatching')",
                    &[&tid.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?
                .get(0);
            let task = fetch_task(&tx, tid, self.pg_target()).await?;
            if task.status == TaskStatus::UnknownExternal && remaining_unknown == 0 {
                // If user previously pressed Stop (cancelled) on this task, honor Cancelled; otherwise Succeeded/Failed.
                let was_stopped: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM control_events
                            WHERE task_id = $1
                              AND event_type IN ('task.cancelled', 'task.stopped_with_unknown_external')
                        )",
                        &[&tid.as_uuid()],
                    )
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?
                    .get(0);
                let next_status = if was_stopped {
                    TaskStatus::Cancelled
                } else if resolved_status == ExternalEffectStatus::Confirmed {
                    TaskStatus::Succeeded
                } else {
                    TaskStatus::Failed
                };
                let task_sql = format!(
                    "UPDATE tasks
                     SET status = $2,
                         wait_detail = NULL,
                         finished_at = COALESCE(finished_at, now()),
                         updated_at = now()
                     WHERE task_id = $1
                     RETURNING {}",
                    crate::store::TASK_SELECT_COLS
                );
                let task_row = tx
                    .query_one(&task_sql, &[&tid.as_uuid(), &next_status.as_str()])
                    .await
                    .map_err(|err| map_pg_err(self.pg_target(), &err))?;
                updated_task = Some(crate::store::decode_task_row(&task_row)?);
            } else {
                updated_task = Some(task);
            }
        }

        let entity_key = updated_claim.task_id.map_or_else(
            || format!("effect:{}", updated_claim.effect_id),
            |tid| format!("task:{tid}"),
        );
        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &entity_key,
                event_type: "effect.reconciled",
                project_id: Some(updated_claim.project_id),
                session_id: Some(updated_claim.session_id),
                task_id: updated_claim.task_id,
                correlation_id,
                payload: json!({
                    "effect_id": updated_claim.effect_id,
                    "status": updated_claim.status,
                    "provider_found": provider_found,
                    "provider_execution_count": provider_lookup.provider_execution_count,
                    "provider_operation_id": updated_claim.provider_operation_id,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(ReconcileEffectResponse {
            effect: updated_claim,
            task: updated_task,
            provider_found,
            provider_execution_count: provider_lookup.provider_execution_count,
        })
    }
}
