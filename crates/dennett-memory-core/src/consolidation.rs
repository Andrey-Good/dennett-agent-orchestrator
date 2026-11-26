//! Interruptible, deterministic background memory consolidation (`memory_consolidation_jobs`).
//!
//! Architectural invariants (D3 Memory Fabric):
//! - Zero LLM calls inside `memoryd`: all 6 consolidation job types execute deterministically
//!   against canonical PostgreSQL tables.
//! - Durable checkpointing (`checkpoint_data` + `watermark_event_id`) enables clean pause/resume
//!   and post-failure recovery without duplicating already-processed work.
//! - Interrupting or failing a job NEVER damages canonical `memory_events` or `evidence_objects`;
//!   partial projections are explicitly marked `incomplete` until resumed to completion.
//! - Dossier and summary consolidation strictly preserves every source claim caveat verbatim.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use deadpool_postgres::GenericClient;
use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AuthPrincipal, ConflictType, ConsolidationJob, ConsolidationJobId, ConsolidationJobStatus,
    ConsolidationJobType, CorrelationId, CreateConsolidationJobRequest, DennettError,
    EpistemicStatus, IdempotencyKey, ListConsolidationJobsResponse, MAX_CLAIM_TEXT_BYTES,
    MemoryCapability, MemoryEventId, MemoryEventKind, MemoryNoteId, MemoryScope, PrincipalId,
    TimestampMs, TrustLevel,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::time;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::claims::{NewClaimRow, insert_claim_row, record_claim_conflict_in_tx};
use crate::documents::upsert_dossier_in_tx;
use crate::search::{maybe_index_claim_in_tx, rebuild_scope_search_index_in_tx};
use crate::service::{
    MemoryService, NewMemoryEvent, insert_memory_event, require_resource_scope_capability,
    require_scope_capability, resolve_search_scopes,
};

/// Durable JSON checkpoint serialized into `memory_consolidation_jobs.checkpoint_data`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationCheckpoint {
    /// Topics already materialized into dossiers by this job.
    #[serde(default)]
    pub processed_topics: Vec<String>,
    /// Entity UUIDs (notes, claims, events) already processed by this job.
    #[serde(default)]
    pub processed_ids: Vec<String>,
    /// Highest canonical `stream_seq` processed so far.
    #[serde(default)]
    pub last_stream_seq: i64,
    /// Total count of work items processed across runs/resumes.
    #[serde(default)]
    pub items_processed: u64,
    /// `true` when the job was paused, step-limited, or interrupted before completion.
    #[serde(default)]
    pub incomplete: bool,
    /// Human-readable checkpoint stage label.
    #[serde(default)]
    pub stage: String,
}

/// Schedules and executes a deterministic [`ConsolidationJob`] in `req.scope`.
pub(crate) async fn execute_create_consolidation_job(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: CreateConsolidationJobRequest,
    max_steps: Option<usize>,
) -> Result<ConsolidationJob, DennettError> {
    require_consolidation_capability(principal, &req.scope, req.job_type)?;
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;

        if let Some(ref idem) = req.idempotency_key {
            let existing = conn
                .query_opt(
                    "SELECT job_id, scope, job_type, status, checkpoint_data, watermark_event_id,
                            caveats_preserved, error_detail,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM started_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM completed_at) * 1000)::bigint
                     FROM memory_consolidation_jobs
                     WHERE scope = $1 AND idempotency_key = $2",
                    &[&req.scope.as_str(), &idem.as_str()],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;

            if let Some(row) = existing {
                let job = decode_job_row(&row)?;
                if job.job_type != req.job_type {
                    return Err(DennettError::IdempotencyMismatch {
                        idempotency_key: idem.to_string(),
                        reason: String::from(
                            "idempotency_key already used for a different consolidation job_type",
                        ),
                    });
                }
                return Ok(job);
            }
        }

        let job_id = ConsolidationJobId::new();
        let initial_cp = ConsolidationCheckpoint {
            stage: String::from("initialized"),
            ..ConsolidationCheckpoint::default()
        };
        let cp_json = serde_json::to_value(&initial_cp).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize consolidation checkpoint: {err}"),
        })?;
        let idem_str = req.idempotency_key.as_ref().map(IdempotencyKey::as_str);

        conn.execute(
            "INSERT INTO memory_consolidation_jobs (
                job_id, scope, job_type, status, checkpoint_data,
                caveats_preserved, idempotency_key, started_at
             ) VALUES ($1, $2, $3, 'running', $4, true, $5, now())",
            &[
                &job_id.as_uuid(),
                &req.scope.as_str(),
                &req.job_type.as_str(),
                &cp_json,
                &idem_str,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        drop(conn);
        run_job_from_checkpoint(svc, principal, correlation_id, job_id, max_steps).await
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::create_consolidation_job"),
        })?
}

/// Lists [`ConsolidationJob`] records across authorized scopes, newest first.
pub(crate) async fn execute_list_consolidation_jobs(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
) -> Result<ListConsolidationJobsResponse, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Ok(ListConsolidationJobsResponse { jobs: Vec::new() });
    }
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let rows = conn
            .query(
                "SELECT job_id, scope, job_type, status, checkpoint_data, watermark_event_id,
                        caveats_preserved, error_detail,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM started_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM completed_at) * 1000)::bigint
                 FROM memory_consolidation_jobs
                 WHERE scope = ANY($1::text[])
                 ORDER BY created_at DESC, job_id ASC",
                &[&allowed_scopes],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let job = decode_job_row(&row)?;
            require_scope_capability(principal, &job.scope, MemoryCapability::Read)?;
            jobs.push(job);
        }
        Ok(ListConsolidationJobsResponse { jobs })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::list_consolidation_jobs"),
        })?
}

/// Pauses an active (`pending` or `running`) [`ConsolidationJob`] at its current durable checkpoint.
pub(crate) async fn execute_pause_consolidation_job(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    job_id: ConsolidationJobId,
) -> Result<ConsolidationJob, DennettError> {
    svc.ensure_migrated().await?;

    let op_fut = async {
        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let row = tx
            .query_opt(
                "SELECT scope, status, checkpoint_data
                 FROM memory_consolidation_jobs
                 WHERE job_id = $1
                 FOR UPDATE",
                &[&job_id.as_uuid()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("consolidation_job"),
                id: job_id.to_string(),
            });
        };

        let scope = MemoryScope::try_new(row.get::<_, String>(0))?;
        require_resource_scope_capability(principal, &scope, MemoryCapability::Write)?;

        let status: ConsolidationJobStatus = row.get::<_, String>(1).parse()?;
        match status {
            ConsolidationJobStatus::Paused => {
                let job = fetch_job_row(&tx, job_id, svc).await?;
                tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
                return Ok(job);
            }
            ConsolidationJobStatus::Pending | ConsolidationJobStatus::Running => {}
            other => {
                return Err(DennettError::Conflict {
                    resource: String::from("consolidation_job"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!("cannot pause consolidation job in `{other}` status"),
                });
            }
        }

        let mut cp = parse_checkpoint(row.get::<_, Option<JsonValue>>(2));
        cp.incomplete = true;
        cp.stage = String::from("paused");
        let cp_val = serde_json::to_value(&cp).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize checkpoint: {err}"),
        })?;

        tx.execute(
            "UPDATE memory_consolidation_jobs
             SET status = 'paused',
                 checkpoint_data = $1,
                 updated_at = now()
             WHERE job_id = $2",
            &[&cp_val, &job_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        let job = fetch_job_row(&tx, job_id, svc).await?;
        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
        Ok(job)
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::pause_consolidation_job"),
        })?
}

/// Resumes a `paused`, `failed`, or `pending` [`ConsolidationJob`] strictly from its saved checkpoint.
pub(crate) async fn execute_resume_consolidation_job(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    job_id: ConsolidationJobId,
) -> Result<ConsolidationJob, DennettError> {
    svc.ensure_migrated().await?;

    let op_fut = async {
        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let row = tx
            .query_opt(
                "SELECT scope, job_type, status
                 FROM memory_consolidation_jobs
                 WHERE job_id = $1
                 FOR UPDATE",
                &[&job_id.as_uuid()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let Some(row) = row else {
            return Err(DennettError::NotFound {
                resource: String::from("consolidation_job"),
                id: job_id.to_string(),
            });
        };

        let scope = MemoryScope::try_new(row.get::<_, String>(0))?;
        let job_type: ConsolidationJobType = row.get::<_, String>(1).parse()?;
        require_resource_scope_capability(principal, &scope, MemoryCapability::Write)?;
        require_consolidation_capability(principal, &scope, job_type)?;

        let status: ConsolidationJobStatus = row.get::<_, String>(2).parse()?;
        if !matches!(
            status,
            ConsolidationJobStatus::Paused
                | ConsolidationJobStatus::Failed
                | ConsolidationJobStatus::Pending
        ) {
            return Err(DennettError::Conflict {
                resource: String::from("consolidation_job"),
                expected_revision: None,
                actual_revision: None,
                reason: format!("cannot resume consolidation job in `{status}` status"),
            });
        }

        tx.execute(
            "UPDATE memory_consolidation_jobs
             SET status = 'running',
                 error_detail = NULL,
                 started_at = COALESCE(started_at, now()),
                 updated_at = now()
             WHERE job_id = $1",
            &[&job_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
        drop(conn);

        run_job_from_checkpoint(svc, principal, correlation_id, job_id, None).await
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::resume_consolidation_job"),
        })?
}

/// Enqueues a deterministic [`ConsolidationJob`] in `pending` status for periodic background
/// execution without running it synchronously.
pub(crate) async fn execute_enqueue_consolidation_job(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    req: CreateConsolidationJobRequest,
) -> Result<ConsolidationJob, DennettError> {
    require_consolidation_capability(principal, &req.scope, req.job_type)?;
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;

        if let Some(ref idem) = req.idempotency_key {
            let existing = conn
                .query_opt(
                    "SELECT job_id, scope, job_type, status, checkpoint_data, watermark_event_id,
                            caveats_preserved, error_detail,
                            TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM started_at) * 1000)::bigint,
                            TRUNC(EXTRACT(EPOCH FROM completed_at) * 1000)::bigint
                     FROM memory_consolidation_jobs
                     WHERE scope = $1 AND idempotency_key = $2",
                    &[&req.scope.as_str(), &idem.as_str()],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;

            if let Some(row) = existing {
                let job = decode_job_row(&row)?;
                if job.job_type != req.job_type {
                    return Err(DennettError::IdempotencyMismatch {
                        idempotency_key: idem.to_string(),
                        reason: String::from(
                            "idempotency_key already used for a different consolidation job_type",
                        ),
                    });
                }
                return Ok(job);
            }
        }

        let job_id = ConsolidationJobId::new();
        let initial_cp = ConsolidationCheckpoint {
            stage: String::from("queued"),
            ..ConsolidationCheckpoint::default()
        };
        let cp_json = serde_json::to_value(&initial_cp).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize consolidation checkpoint: {err}"),
        })?;
        let idem_str = req.idempotency_key.as_ref().map(IdempotencyKey::as_str);

        conn.execute(
            "INSERT INTO memory_consolidation_jobs (
                job_id, scope, job_type, status, checkpoint_data,
                caveats_preserved, idempotency_key
             ) VALUES ($1, $2, $3, 'pending', $4, true, $5)",
            &[
                &job_id.as_uuid(),
                &req.scope.as_str(),
                &req.job_type.as_str(),
                &cp_json,
                &idem_str,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        fetch_job_row(&conn, job_id, svc).await
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::enqueue_consolidation_job"),
        })?
}

/// Runs one bounded background consolidation tick, claiming up to `max_jobs` unpaused `pending`
/// jobs via `FOR UPDATE SKIP LOCKED` and advancing each by at most `max_steps_per_job` steps.
///
/// Strictly skips `paused` (and `running`/`completed`/`failed`/`cancelled`) jobs so manual pause
/// controls are always honored. Returns the number of jobs stepped during this tick.
pub(crate) async fn execute_background_consolidation_tick(
    svc: &MemoryService,
    max_jobs: usize,
    max_steps_per_job: usize,
) -> Result<u64, DennettError> {
    if max_jobs == 0 || max_steps_per_job == 0 {
        return Ok(0);
    }
    svc.ensure_migrated().await?;

    let mut stepped_jobs: u64 = 0;

    for _ in 0..max_jobs {
        let claimed = {
            let mut conn = svc.get_conn(svc.op_timeout()).await?;
            let tx = conn
                .transaction()
                .await
                .map_err(|err| svc.map_pg_err(&err))?;
            let row = tx
                .query_opt(
                    "SELECT job_id, scope
                     FROM memory_consolidation_jobs
                     WHERE status = 'pending'
                     ORDER BY created_at ASC, job_id ASC
                     LIMIT 1
                     FOR UPDATE SKIP LOCKED",
                    &[],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;

            if let Some(row) = row {
                let job_id = ConsolidationJobId::from_uuid(row.get(0));
                let scope = MemoryScope::try_new(row.get::<_, String>(1))?;
                tx.execute(
                    "UPDATE memory_consolidation_jobs
                     SET status = 'running',
                         started_at = COALESCE(started_at, now()),
                         updated_at = now()
                     WHERE job_id = $1 AND status = 'pending'",
                    &[&job_id.as_uuid()],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;
                tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
                Some((job_id, scope))
            } else {
                tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
                None
            }
        };

        let Some((job_id, scope)) = claimed else {
            break;
        };

        let sys_principal = AuthPrincipal {
            principal_id: PrincipalId::try_new("system:memoryd-consolidation")?,
            issuer: String::from(crate::auth::DEFAULT_ISSUER),
            token_id: String::from("system-background-consolidation"),
            allowed_scopes: BTreeSet::from([scope]),
            capabilities: BTreeSet::from([
                MemoryCapability::Read,
                MemoryCapability::Write,
                MemoryCapability::Admin,
            ]),
            expires_at: None,
        };
        let cid = CorrelationId::generate();

        let _ = run_job_from_checkpoint_with_incomplete_status(
            svc,
            &sys_principal,
            &cid,
            job_id,
            Some(max_steps_per_job),
            ConsolidationJobStatus::Pending,
        )
        .await;
        stepped_jobs = stepped_jobs.saturating_add(1);
    }

    Ok(stepped_jobs)
}

async fn run_job_from_checkpoint(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    job_id: ConsolidationJobId,
    max_steps: Option<usize>,
) -> Result<ConsolidationJob, DennettError> {
    run_job_from_checkpoint_with_incomplete_status(
        svc,
        principal,
        correlation_id,
        job_id,
        max_steps,
        ConsolidationJobStatus::Paused,
    )
    .await
}

#[expect(
    clippy::too_many_lines,
    reason = "consolidation job state machine transitions running/pending/paused/completed/failed and persists durable checkpoints atomically"
)]
async fn run_job_from_checkpoint_with_incomplete_status(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    job_id: ConsolidationJobId,
    max_steps: Option<usize>,
    incomplete_status: ConsolidationJobStatus,
) -> Result<ConsolidationJob, DennettError> {
    let conn = svc.get_conn(svc.op_timeout()).await?;
    let meta_row = conn
        .query_one(
            "SELECT scope, job_type, checkpoint_data, watermark_event_id
             FROM memory_consolidation_jobs
             WHERE job_id = $1",
            &[&job_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let scope = MemoryScope::try_new(meta_row.get::<_, String>(0))?;
    let job_type: ConsolidationJobType = meta_row.get::<_, String>(1).parse()?;
    let mut cp = parse_checkpoint(meta_row.get::<_, Option<JsonValue>>(2));
    let mut watermark_event_id: Option<Uuid> = meta_row.get(3);
    drop(conn);

    let run_result = match job_type {
        ConsolidationJobType::DossierUpdate => {
            run_dossier_update(
                svc,
                principal,
                correlation_id,
                job_id,
                &scope,
                &mut cp,
                &mut watermark_event_id,
                max_steps,
            )
            .await
        }
        ConsolidationJobType::EpisodeSummary => {
            run_episode_summary(
                svc,
                principal,
                correlation_id,
                &scope,
                &mut cp,
                &mut watermark_event_id,
            )
            .await
        }
        ConsolidationJobType::ClaimExtraction => {
            run_claim_extraction(
                svc,
                principal,
                correlation_id,
                job_id,
                &scope,
                &mut cp,
                &mut watermark_event_id,
                max_steps,
            )
            .await
        }
        ConsolidationJobType::IndexRebuild => {
            run_index_rebuild_job(svc, &scope, &mut cp, &mut watermark_event_id).await
        }
        ConsolidationJobType::ConflictScan => {
            run_conflict_scan(
                svc,
                principal,
                correlation_id,
                &scope,
                &mut cp,
                &mut watermark_event_id,
            )
            .await
        }
        ConsolidationJobType::RetentionSweep => {
            run_retention_sweep(svc, &scope, &mut cp, &mut watermark_event_id).await
        }
        _ => Err(DennettError::InvalidInput {
            field: String::from("job_type"),
            reason: String::from("unsupported consolidation job type"),
        }),
    };

    match run_result {
        Ok(JobRunOutcome {
            finished,
            caveats_preserved,
        }) => {
            let conn = svc.get_conn(svc.op_timeout()).await?;
            cp.incomplete = !finished;
            cp.stage = if finished {
                String::from("completed")
            } else if incomplete_status == ConsolidationJobStatus::Pending {
                String::from("queued_checkpoint")
            } else {
                String::from("paused_checkpoint")
            };
            let cp_val = serde_json::to_value(&cp).map_err(|err| DennettError::Internal {
                detail: format!("failed to serialize final checkpoint: {err}"),
            })?;

            if finished {
                conn.execute(
                    "UPDATE memory_consolidation_jobs
                     SET status = 'completed',
                         checkpoint_data = $1,
                         watermark_event_id = COALESCE($2, watermark_event_id),
                         caveats_preserved = $3,
                         error_detail = NULL,
                         completed_at = now(),
                         updated_at = now()
                     WHERE job_id = $4 AND status = 'running'",
                    &[
                        &cp_val,
                        &watermark_event_id,
                        &caveats_preserved,
                        &job_id.as_uuid(),
                    ],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;
            } else {
                let next_status = incomplete_status.as_str();
                conn.execute(
                    "UPDATE memory_consolidation_jobs
                     SET status = $1,
                         checkpoint_data = $2,
                         watermark_event_id = COALESCE($3, watermark_event_id),
                         caveats_preserved = $4,
                         updated_at = now()
                     WHERE job_id = $5 AND status = 'running'",
                    &[
                        &next_status,
                        &cp_val,
                        &watermark_event_id,
                        &caveats_preserved,
                        &job_id.as_uuid(),
                    ],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;
            }

            fetch_job_row(&conn, job_id, svc).await
        }
        Err(err) => {
            // Best-effort durable failure transition preserving the last committed checkpoint.
            if let Ok(conn) = svc.get_conn(svc.op_timeout()).await {
                cp.incomplete = true;
                cp.stage = String::from("failed");
                let cp_val = serde_json::to_value(&cp).ok();
                let safe_err = redact_secrets(&err.to_string(), &[]);
                let _ = conn
                    .execute(
                        "UPDATE memory_consolidation_jobs
                         SET status = 'failed',
                             checkpoint_data = COALESCE($1, checkpoint_data),
                             watermark_event_id = COALESCE($2, watermark_event_id),
                             error_detail = $3,
                             updated_at = now()
                         WHERE job_id = $4",
                        &[&cp_val, &watermark_event_id, &safe_err, &job_id.as_uuid()],
                    )
                    .await;
            }
            Err(err)
        }
    }
}

struct JobRunOutcome {
    finished: bool,
    caveats_preserved: bool,
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "deterministic dossier consolidation groups active claims and notes by topic, persists checkpoints per topic, and guarantees caveat preservation"
)]
async fn run_dossier_update(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    job_id: ConsolidationJobId,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
    max_steps: Option<usize>,
) -> Result<JobRunOutcome, DennettError> {
    let conn = svc.get_conn(svc.op_timeout()).await?;

    let claim_rows = conn
        .query(
            "SELECT claim_id, topic_key, claim_key, natural_language, epistemic_status,
                    caveats, source_event_id
             FROM memory_claims
             WHERE scope = $1
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')
             ORDER BY topic_key ASC, created_at ASC, claim_id ASC",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let note_rows = conn
        .query(
            "SELECT note_id, topic_key, title, body, last_event_id
             FROM memory_notes
             WHERE scope = $1
               AND forgotten_at IS NULL
               AND admission_status <> 'rejected'
               AND topic_key IS NOT NULL
             ORDER BY topic_key ASC, created_at ASC, note_id ASC",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let mut topics: BTreeSet<String> = BTreeSet::new();
    let mut claims_by_topic: BTreeMap<String, Vec<ClaimDigest>> = BTreeMap::new();
    let mut notes_by_topic: BTreeMap<String, Vec<NoteDigest>> = BTreeMap::new();

    for row in claim_rows {
        let topic: String = row.get(1);
        topics.insert(topic.clone());
        let caveats: Vec<String> =
            serde_json::from_value(row.get::<_, JsonValue>(5)).unwrap_or_default();
        claims_by_topic.entry(topic).or_default().push(ClaimDigest {
            claim_id: row.get(0),
            claim_key: row.get(2),
            natural_language: row.get(3),
            epistemic_status: row.get(4),
            caveats,
            source_event_id: row.get(6),
        });
    }

    for row in note_rows {
        let topic: String = row.get(1);
        topics.insert(topic.clone());
        notes_by_topic.entry(topic).or_default().push(NoteDigest {
            note_id: row.get(0),
            title: row.get(2),
            body: row.get(3),
            last_event_id: row.get(4),
        });
    }
    drop(conn);

    let done_set: BTreeSet<String> = cp.processed_topics.iter().cloned().collect();
    let remaining_topics: Vec<String> = topics
        .into_iter()
        .filter(|t| !done_set.contains(t))
        .collect();

    let mut all_caveats_preserved = true;

    for (idx, topic) in remaining_topics.iter().enumerate() {
        if let Some(limit) = max_steps
            && idx >= limit
        {
            return Ok(JobRunOutcome {
                finished: false,
                caveats_preserved: all_caveats_preserved,
            });
        }

        let is_partial_stop =
            max_steps.is_some_and(|limit| idx + 1 >= limit && idx + 1 < remaining_topics.len());
        let topic_claims = claims_by_topic.get(topic).map_or(&[][..], Vec::as_slice);
        let topic_notes = notes_by_topic.get(topic).map_or(&[][..], Vec::as_slice);

        let (body, caveats_ok) =
            render_topic_dossier_markdown(topic, topic_claims, topic_notes, is_partial_stop);
        if !caveats_ok {
            all_caveats_preserved = false;
        }

        let claim_ids: Vec<Uuid> = topic_claims.iter().map(|c| c.claim_id).collect();
        let note_ids: Vec<Uuid> = topic_notes.iter().map(|n| n.note_id).collect();
        let doc_key = build_dossier_key(topic);
        let title = format!("Topic Dossier: {topic}");

        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        upsert_dossier_in_tx(
            &tx,
            principal,
            correlation_id,
            scope,
            &doc_key,
            &title,
            &body,
            &claim_ids,
            &note_ids,
            "Deterministic dossier consolidation preserving all source caveats",
            TrustLevel::Native,
            true,
            svc,
        )
        .await?;

        if let Some(last_ev) = topic_claims
            .last()
            .map(|c| c.source_event_id)
            .or_else(|| topic_notes.last().map(|n| n.last_event_id))
        {
            *watermark_event_id = Some(last_ev);
        }

        cp.processed_topics.push(topic.clone());
        cp.items_processed = cp.items_processed.saturating_add(1);
        cp.stage = format!("dossier:{topic}");
        cp.incomplete = is_partial_stop;

        let cp_val = serde_json::to_value(&*cp).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize dossier checkpoint: {err}"),
        })?;
        tx.execute(
            "UPDATE memory_consolidation_jobs
             SET checkpoint_data = $1,
                 watermark_event_id = COALESCE($2, watermark_event_id),
                 caveats_preserved = $3,
                 updated_at = now()
             WHERE job_id = $4",
            &[
                &cp_val,
                &*watermark_event_id,
                &all_caveats_preserved,
                &job_id.as_uuid(),
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    }

    let finished = max_steps.is_none_or(|limit| remaining_topics.len() <= limit);
    Ok(JobRunOutcome {
        finished,
        caveats_preserved: all_caveats_preserved,
    })
}

async fn run_episode_summary(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
) -> Result<JobRunOutcome, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let ev_rows = tx
        .query(
            "SELECT event_id, stream_seq, event_kind, target_entity_id
             FROM memory_events
             WHERE scope = $1 AND stream_seq > $2
             ORDER BY stream_seq ASC",
            &[&scope.as_str(), &cp.last_stream_seq],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let claim_rows = tx
        .query(
            "SELECT claim_id, natural_language, caveats
             FROM memory_claims
             WHERE scope = $1
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')
             ORDER BY created_at ASC, claim_id ASC",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let mut body = format!("# Episode Summary ({})\n\n", scope.as_str());
    let _ = write!(
        body,
        "Processed {} canonical events since stream_seq {}.\n\n",
        ev_rows.len(),
        cp.last_stream_seq
    );

    for row in &ev_rows {
        let ev_id: Uuid = row.get(0);
        let seq: i64 = row.get(1);
        let kind: String = row.get(2);
        let target: Uuid = row.get(3);
        let _ = writeln!(body, "- seq={seq} kind=`{kind}` target=`{target}`");
        cp.last_stream_seq = seq;
        *watermark_event_id = Some(ev_id);
        cp.items_processed = cp.items_processed.saturating_add(1);
    }

    let mut all_caveats: Vec<String> = Vec::new();
    let mut claim_ids: Vec<Uuid> = Vec::new();
    for row in &claim_rows {
        claim_ids.push(row.get(0));
        let caveats: Vec<String> =
            serde_json::from_value(row.get::<_, JsonValue>(2)).unwrap_or_default();
        for c in caveats {
            if !all_caveats.contains(&c) {
                all_caveats.push(c);
            }
        }
    }

    if !all_caveats.is_empty() {
        body.push_str("\n## Caveats & Exceptions\n\n");
        for caveat in &all_caveats {
            let _ = writeln!(body, "- {caveat}");
        }
    }

    let caveats_preserved = all_caveats.iter().all(|c| body.contains(c.as_str()));

    if !ev_rows.is_empty() || !all_caveats.is_empty() {
        upsert_dossier_in_tx(
            &tx,
            principal,
            correlation_id,
            scope,
            "dossier:episode-summary",
            "Episode Summary",
            &body,
            &claim_ids,
            &[],
            "Deterministic episode summary preserving caveats",
            TrustLevel::Native,
            true,
            svc,
        )
        .await?;
    }

    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(JobRunOutcome {
        finished: true,
        caveats_preserved,
    })
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "claim extraction requires transaction context, checkpoint state, and step budget"
)]
async fn run_claim_extraction(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    job_id: ConsolidationJobId,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
    max_steps: Option<usize>,
) -> Result<JobRunOutcome, DennettError> {
    let conn = svc.get_conn(svc.op_timeout()).await?;
    let note_rows = conn
        .query(
            "SELECT n.note_id, n.topic_key, n.title, n.body, n.last_event_id
             FROM memory_notes n
             WHERE n.scope = $1
               AND n.forgotten_at IS NULL
               AND n.admission_status = 'admitted'
               AND n.topic_key IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM memory_claims c
                   WHERE c.source_note_id = n.note_id AND c.forgotten_at IS NULL
               )
             ORDER BY n.created_at ASC, n.note_id ASC",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    drop(conn);

    let done_ids: BTreeSet<String> = cp.processed_ids.iter().cloned().collect();
    let pending: Vec<NoteDigest> = note_rows
        .into_iter()
        .filter_map(|r| {
            let note_id: Uuid = r.get(0);
            if done_ids.contains(&note_id.to_string()) {
                return None;
            }
            Some(NoteDigest {
                note_id,
                title: r.get(2),
                body: r.get(3),
                last_event_id: r.get(4),
            })
        })
        .collect();

    let mut steps = 0_usize;
    for note in &pending {
        if let Some(limit) = max_steps
            && steps >= limit
        {
            return Ok(JobRunOutcome {
                finished: false,
                caveats_preserved: true,
            });
        }

        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let topic_row = tx
            .query_opt(
                "SELECT topic_key FROM memory_notes WHERE note_id = $1 AND forgotten_at IS NULL",
                &[&note.note_id],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        let Some(topic_key) = topic_row.and_then(|r| r.get::<_, Option<String>>(0)) else {
            tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
            continue;
        };

        let caveats = extract_caveat_lines(&note.body);
        let mut statement = format!("{}: {}", note.title.trim(), note.body.trim());
        if statement.len() > MAX_CLAIM_TEXT_BYTES {
            let mut end = MAX_CLAIM_TEXT_BYTES;
            while !statement.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            statement.truncate(end);
        }

        let claim_id = dennett_contracts::ClaimId::new();
        let event_id = MemoryEventId::new();
        let idem = IdempotencyKey::try_new(format!("extract-{job_id}-{}", note.note_id))?;
        let fp = crate::fingerprint::sha256_hex(statement.as_bytes());
        let mut provenance = BTreeMap::new();
        provenance.insert(
            String::from("extractor"),
            String::from("deterministic-consolidation-v1"),
        );

        if let Some(stream_seq) = insert_memory_event(
            &tx,
            NewMemoryEvent {
                event_id,
                scope,
                kind: MemoryEventKind::ClaimCreated,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &idem,
                fingerprint: &fp,
                target_entity_id: claim_id.as_uuid(),
                target_revision: None,
                observed_ms: None,
            },
            svc,
        )
        .await?
        {
            insert_claim_row(
                &tx,
                NewClaimRow {
                    claim_id,
                    scope,
                    topic_key: &topic_key,
                    claim_key: None,
                    natural_language: &statement,
                    epistemic_status: EpistemicStatus::Inferred,
                    supersedes_claim_id: None,
                    valid_from: None,
                    valid_to: None,
                    source_event_id: event_id,
                    source_note_id: Some(MemoryNoteId::from_uuid(note.note_id)),
                    caveats: &caveats,
                    provenance: &provenance,
                    trust_level: TrustLevel::Native,
                    instruction_active: true,
                },
                svc,
            )
            .await?;
            maybe_index_claim_in_tx(&tx, scope, claim_id.as_uuid(), event_id, stream_seq, svc)
                .await?;
            *watermark_event_id = Some(event_id.as_uuid());
        } else {
            *watermark_event_id = Some(note.last_event_id);
        }

        cp.processed_ids.push(note.note_id.to_string());
        cp.items_processed = cp.items_processed.saturating_add(1);
        cp.stage = format!("extracted:{}", note.note_id);
        let cp_val = serde_json::to_value(&*cp).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize checkpoint: {err}"),
        })?;
        tx.execute(
            "UPDATE memory_consolidation_jobs
             SET checkpoint_data = $1,
                 watermark_event_id = COALESCE($2, watermark_event_id),
                 updated_at = now()
             WHERE job_id = $3",
            &[&cp_val, &*watermark_event_id, &job_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
        steps += 1;
    }

    let finished = max_steps.is_none_or(|limit| pending.len() <= limit);
    Ok(JobRunOutcome {
        finished,
        caveats_preserved: true,
    })
}

async fn run_index_rebuild_job(
    svc: &MemoryService,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
) -> Result<JobRunOutcome, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    let resp = rebuild_scope_search_index_in_tx(&tx, scope, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;

    cp.items_processed = cp.items_processed.saturating_add(resp.entries_rebuilt);
    cp.last_stream_seq = resp.index_state.watermark_seq;
    *watermark_event_id = resp
        .index_state
        .watermark_event_id
        .map(MemoryEventId::as_uuid);
    Ok(JobRunOutcome {
        finished: true,
        caveats_preserved: true,
    })
}

async fn run_conflict_scan(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
) -> Result<JobRunOutcome, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let rows = tx
        .query(
            "SELECT claim_id, topic_key, claim_key, natural_language, source_event_id
             FROM memory_claims
             WHERE scope = $1
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')
             ORDER BY topic_key ASC, created_at ASC, claim_id ASC",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let mut groups: BTreeMap<ConflictGroupKey, Vec<ConflictGroupEntry>> = BTreeMap::new();
    for row in rows {
        let cid: Uuid = row.get(0);
        let topic: String = row.get(1);
        let key: Option<String> = row.get(2);
        let text: String = row.get(3);
        let ev: Uuid = row.get(4);
        groups
            .entry((topic, key))
            .or_default()
            .push((cid, text, ev));
    }

    for ((topic, _key), entries) in groups {
        let distinct_texts: BTreeSet<&str> = entries.iter().map(|(_, t, _)| t.trim()).collect();
        if distinct_texts.len() < 2 {
            continue;
        }
        let ids: Vec<Uuid> = entries.iter().map(|(id, _, _)| *id).collect();
        if let Some((&last_id, rest)) = ids.split_last() {
            record_claim_conflict_in_tx(
                &tx,
                principal,
                correlation_id,
                scope,
                &topic,
                ConflictType::Semantic,
                rest,
                last_id,
                svc,
            )
            .await?;
            cp.items_processed = cp.items_processed.saturating_add(1);
            if let Some((_, _, ev)) = entries.last() {
                *watermark_event_id = Some(*ev);
            }
        }
    }

    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(JobRunOutcome {
        finished: true,
        caveats_preserved: true,
    })
}

async fn run_retention_sweep(
    svc: &MemoryService,
    scope: &MemoryScope,
    cp: &mut ConsolidationCheckpoint,
    watermark_event_id: &mut Option<Uuid>,
) -> Result<JobRunOutcome, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let swept_rows = tx
        .query(
            "UPDATE memory_notes
             SET admission_status = 'rejected', updated_at = now()
             WHERE scope = $1
               AND forgotten_at IS NULL
               AND admission_status = 'ephemeral'
               AND salience <= 20
             RETURNING note_id, last_event_id",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let swept_ids: Vec<Uuid> = swept_rows.iter().map(|r| r.get(0)).collect();
    if !swept_ids.is_empty() {
        tx.execute(
            "DELETE FROM memory_search_index
             WHERE source_type = 'note' AND source_id = ANY($1::uuid[])",
            &[&swept_ids],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
        cp.items_processed = cp
            .items_processed
            .saturating_add(u64::try_from(swept_ids.len()).unwrap_or(0));
        if let Some(last) = swept_rows.last() {
            *watermark_event_id = Some(last.get(1));
        }
    }

    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(JobRunOutcome {
        finished: true,
        caveats_preserved: true,
    })
}

type ConflictGroupKey = (String, Option<String>);
type ConflictGroupEntry = (Uuid, String, Uuid);

struct ClaimDigest {
    claim_id: Uuid,
    claim_key: Option<String>,
    natural_language: String,
    epistemic_status: String,
    caveats: Vec<String>,
    source_event_id: Uuid,
}

struct NoteDigest {
    note_id: Uuid,
    title: String,
    body: String,
    last_event_id: Uuid,
}

fn render_topic_dossier_markdown(
    topic: &str,
    claims: &[ClaimDigest],
    notes: &[NoteDigest],
    incomplete: bool,
) -> (String, bool) {
    let mut out = String::new();
    if incomplete {
        out.push_str("> [INCOMPLETE CONSOLIDATION — RESUME PENDING]\n\n");
    }
    let _ = write!(out, "# Topic Dossier: {topic}\n\n");

    let mut all_caveats: Vec<String> = Vec::new();

    if !claims.is_empty() {
        out.push_str("## Active Claims\n\n");
        for claim in claims {
            if let Some(ref key) = claim.claim_key {
                let _ = writeln!(
                    out,
                    "- **[{}/{}]** {}",
                    claim.epistemic_status, key, claim.natural_language
                );
            } else {
                let _ = writeln!(
                    out,
                    "- **[{}]** {}",
                    claim.epistemic_status, claim.natural_language
                );
            }
            for caveat in &claim.caveats {
                if !all_caveats.contains(caveat) {
                    all_caveats.push(caveat.clone());
                }
            }
        }
        out.push('\n');
    }

    if !all_caveats.is_empty() {
        out.push_str("## Caveats & Exceptions\n\n");
        for caveat in &all_caveats {
            let _ = writeln!(out, "- {caveat}");
        }
        out.push('\n');
    }

    if !notes.is_empty() {
        out.push_str("## Supporting Notes\n\n");
        for note in notes {
            let _ = write!(out, "### {}\n\n{}\n\n", note.title, note.body);
        }
    }

    let caveats_preserved = all_caveats.iter().all(|c| out.contains(c.as_str()));
    (out, caveats_preserved)
}

fn build_dossier_key(topic: &str) -> String {
    let raw = format!("dossier:{topic}");
    if raw.len() <= dennett_contracts::MAX_DOCUMENT_KEY_LEN {
        return raw;
    }
    let mut end = dennett_contracts::MAX_DOCUMENT_KEY_LEN;
    while !raw.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    raw[..end].to_owned()
}

fn extract_caveat_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("caveat:") || lower.starts_with("exception:"))
            && !trimmed.is_empty()
            && trimmed.len() <= dennett_contracts::MAX_CAVEAT_BYTES
        {
            out.push(trimmed.to_owned());
        }
    }
    out
}

fn require_consolidation_capability(
    principal: &AuthPrincipal,
    scope: &MemoryScope,
    job_type: ConsolidationJobType,
) -> Result<(), DennettError> {
    if job_type == ConsolidationJobType::IndexRebuild
        && principal.can_access(scope, MemoryCapability::Admin)
    {
        return Ok(());
    }
    require_scope_capability(principal, scope, MemoryCapability::Write)
}

fn parse_checkpoint(raw: Option<JsonValue>) -> ConsolidationCheckpoint {
    raw.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

async fn fetch_job_row<C: GenericClient>(
    client: &C,
    job_id: ConsolidationJobId,
    svc: &MemoryService,
) -> Result<ConsolidationJob, DennettError> {
    let row = client
        .query_opt(
            "SELECT job_id, scope, job_type, status, checkpoint_data, watermark_event_id,
                    caveats_preserved, error_detail,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM started_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM completed_at) * 1000)::bigint
             FROM memory_consolidation_jobs
             WHERE job_id = $1",
            &[&job_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("consolidation_job"),
            id: job_id.to_string(),
        });
    };
    decode_job_row(&row)
}

fn decode_job_row(row: &Row) -> Result<ConsolidationJob, DennettError> {
    let checkpoint_json = row
        .get::<_, Option<JsonValue>>(4)
        .map(|v| serde_json::to_string(&v))
        .transpose()
        .map_err(|err| DennettError::Internal {
            detail: format!("failed to encode checkpoint JSON: {err}"),
        })?;

    Ok(ConsolidationJob {
        job_id: ConsolidationJobId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        job_type: row.get::<_, String>(2).parse()?,
        status: row.get::<_, String>(3).parse()?,
        checkpoint_json,
        watermark_event_id: row.get::<_, Option<Uuid>>(5).map(MemoryEventId::from_uuid),
        caveats_preserved: row.get(6),
        error_detail: row.get(7),
        created_at: TimestampMs::from_millis(row.get(8)),
        updated_at: TimestampMs::from_millis(row.get(9)),
        started_at: row.get::<_, Option<i64>>(10).map(TimestampMs::from_millis),
        completed_at: row.get::<_, Option<i64>>(11).map(TimestampMs::from_millis),
    })
}
