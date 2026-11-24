//! Selective forgetting with controlled append-only scrub, search-index purge,
//! and content-free audit tombstones (`memory_forget_tombstones`).
//!
//! Architectural invariants (D3 Memory Fabric):
//! - Requires explicit [`MemoryCapability::Forget`] on the owning scope.
//! - Immediately scrubs user content in active tables AND revision/version history tables
//!   via transaction-local `SET LOCAL dennett.allow_forget_scrub = 'on'`.
//! - Immediately deletes derived rows from `memory_search_index`.
//! - Writes a content-free audit tombstone carrying an honest `residual_backup_notice`.
//! - Forgotten entities are permanently excluded from reads, search, fresh-tail overlay,
//!   and portable exports (returning `NotFound` on direct lookup, zero resurrection).

use deadpool_postgres::GenericClient;
use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AuthPrincipal, CorrelationId, DennettError, ForgetPropagationStatus, ForgetRequest,
    ForgetResponse, ForgetTargetType, ForgetTombstone, IdempotencyKey, ListTombstonesResponse,
    MemoryCapability, MemoryEventId, MemoryEventKind, MemoryScope, PrincipalId, Sha256Hex,
    TimestampMs, TombstoneId,
};
use tokio::time;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::fingerprint::fingerprint_forget;
use crate::search::maybe_advance_watermark_only;
use crate::service::{
    MemoryService, NewMemoryEvent, StoredIdempotencyRecord, insert_memory_event,
    lookup_idempotency, require_resource_scope_capability, require_scope_capability,
    resolve_search_scopes,
};

/// Honest disclosure included on every forget response and audit tombstone.
pub const RESIDUAL_BACKUP_NOTICE: &str = "Active projections, revision snapshots, and search indexes have been purged immediately; residual copies may still exist in offline PostgreSQL backups until backup retention rotation completes.";

const SCRUBBED_PLACEHOLDER: &str = "[forgotten]";
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Forgets a memory entity (`note`, `evidence`, `claim`, or `document`) with immediate scrub,
/// search-index purge, and content-free audit tombstone.
pub(crate) async fn execute_forget(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: ForgetRequest,
) -> Result<ForgetResponse, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Forget)?;
    svc.ensure_migrated().await?;

    let fingerprint = fingerprint_forget(&req);
    let op_fut = forget_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::forget"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional forget sets scrub GUC, scrubs target and revisions, purges search index, appends event, and writes content-free tombstone atomically"
)]
async fn forget_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &ForgetRequest,
    fingerprint: &Sha256Hex,
) -> Result<ForgetResponse, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let event_kind = forget_event_kind(req.target_type)?;

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_forget_replay(
            &tx,
            existing,
            event_kind,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    // Authorize controlled forget-scrub ONLY for this transaction.
    tx.batch_execute("SET LOCAL dennett.allow_forget_scrub = 'on';")
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    match req.target_type {
        ForgetTargetType::Note => {
            scrub_note_in_tx(&tx, principal, &req.scope, req.target_id, svc).await?;
        }
        ForgetTargetType::Evidence => {
            scrub_evidence_in_tx(&tx, principal, &req.scope, req.target_id, svc).await?;
        }
        ForgetTargetType::Claim => {
            scrub_claim_in_tx(&tx, principal, &req.scope, req.target_id, svc).await?;
        }
        ForgetTargetType::Document => {
            scrub_document_in_tx(&tx, principal, &req.scope, req.target_id, svc).await?;
        }
        _ => {
            return Err(DennettError::InvalidInput {
                field: String::from("target_type"),
                reason: String::from("unsupported forget target type"),
            });
        }
    }

    let forget_event_id = MemoryEventId::new();
    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id: forget_event_id,
            scope: &req.scope,
            kind: event_kind,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.idempotency_key,
            fingerprint,
            target_entity_id: req.target_id,
            target_revision: None,
            observed_ms: None,
        },
        svc,
    )
    .await?
    else {
        let _ = tx.rollback().await;
        let Some(existing) =
            lookup_idempotency(&conn, &req.scope, &req.idempotency_key, svc).await?
        else {
            return Err(DennettError::Conflict {
                resource: String::from("forget"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("concurrent idempotency conflict; retry request"),
            });
        };
        return resolve_forget_replay(
            &conn,
            existing,
            event_kind,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    };

    let tombstone_id = TombstoneId::new();
    let safe_reason = redact_secrets(req.reason.trim(), &[]);

    tx.execute(
        "INSERT INTO memory_forget_tombstones (
            tombstone_id, scope, target_type, target_id, forget_event_id,
            principal_id, reason, residual_backup_notice, propagation_status
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'completed')",
        &[
            &tombstone_id.as_uuid(),
            &req.scope.as_str(),
            &req.target_type.as_str(),
            &req.target_id,
            &forget_event_id.as_uuid(),
            &principal.principal_id.as_str(),
            &safe_reason,
            &RESIDUAL_BACKUP_NOTICE,
        ],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    maybe_advance_watermark_only(&tx, &req.scope, forget_event_id, stream_seq, svc).await?;

    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;

    Ok(ForgetResponse {
        tombstone_id,
        forget_event_id,
        residual_backup_notice: String::from(RESIDUAL_BACKUP_NOTICE),
        propagation_status: ForgetPropagationStatus::Completed,
        idempotent_replay: false,
    })
}

async fn scrub_note_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    requested_scope: &MemoryScope,
    note_id: Uuid,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let row = client
        .query_opt(
            "SELECT scope FROM memory_notes WHERE note_id = $1 AND forgotten_at IS NULL FOR UPDATE",
            &[&note_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_note"),
            id: note_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(row.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Forget)?;
    if &stored_scope != requested_scope {
        return Err(DennettError::ForbiddenScope {
            scope: requested_scope.to_string(),
            reason: String::from("note does not belong to the requested scope"),
        });
    }

    client
        .execute(
            "UPDATE memory_note_revisions
             SET title = $1,
                 body = $1,
                 rationale = NULL,
                 tags = '[]'::jsonb,
                 author_label = NULL
             WHERE note_id = $2",
            &[&SCRUBBED_PLACEHOLDER, &note_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "UPDATE memory_notes
             SET title = $1,
                 body = $1,
                 rationale = NULL,
                 tags = '[]'::jsonb,
                 author_label = NULL,
                 topic_key = NULL,
                 forgotten_at = now(),
                 updated_at = now()
             WHERE note_id = $2",
            &[&SCRUBBED_PLACEHOLDER, &note_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "DELETE FROM memory_search_index WHERE source_type = 'note' AND source_id = $1",
            &[&note_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    Ok(())
}

async fn scrub_evidence_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    requested_scope: &MemoryScope,
    evidence_id: Uuid,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let row = client
        .query_opt(
            "SELECT scope FROM evidence_objects WHERE evidence_id = $1 AND forgotten_at IS NULL FOR UPDATE",
            &[&evidence_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("evidence"),
            id: evidence_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(row.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Forget)?;
    if &stored_scope != requested_scope {
        return Err(DennettError::ForbiddenScope {
            scope: requested_scope.to_string(),
            reason: String::from("evidence does not belong to the requested scope"),
        });
    }

    client
        .execute(
            "UPDATE evidence_objects
             SET content = $1,
                 sha256 = $2,
                 byte_len = 0,
                 source_uri = NULL,
                 metadata = '{}'::jsonb,
                 forgotten_at = now()
             WHERE evidence_id = $3",
            &[&SCRUBBED_PLACEHOLDER, &ZERO_SHA256, &evidence_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    Ok(())
}

async fn scrub_claim_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    requested_scope: &MemoryScope,
    claim_id: Uuid,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let row = client
        .query_opt(
            "SELECT scope FROM memory_claims WHERE claim_id = $1 AND forgotten_at IS NULL FOR UPDATE",
            &[&claim_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_claim"),
            id: claim_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(row.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Forget)?;
    if &stored_scope != requested_scope {
        return Err(DennettError::ForbiddenScope {
            scope: requested_scope.to_string(),
            reason: String::from("claim does not belong to the requested scope"),
        });
    }

    client
        .execute(
            "UPDATE memory_claims
             SET natural_language = $1,
                 caveats = '[]'::jsonb,
                 provenance = '{}'::jsonb,
                 epistemic_status = 'retracted',
                 forgotten_at = now()
             WHERE claim_id = $2",
            &[&SCRUBBED_PLACEHOLDER, &claim_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "DELETE FROM memory_search_index WHERE source_type = 'claim' AND source_id = $1",
            &[&claim_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    // Invalidate open conflicts whose candidate list drops below 2 active claims.
    let open_conflicts = client
        .query(
            "SELECT conflict_id, candidate_claim_ids
             FROM memory_conflicts
             WHERE scope = $1
               AND status = 'open'
               AND $2 = ANY(candidate_claim_ids)
             FOR UPDATE",
            &[&stored_scope.as_str(), &claim_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    for c_row in open_conflicts {
        let conflict_id: Uuid = c_row.get(0);
        let candidates: Vec<Uuid> = c_row.get(1);
        let remaining_rows = client
            .query(
                "SELECT claim_id FROM memory_claims
                 WHERE claim_id = ANY($1::uuid[])
                   AND forgotten_at IS NULL
                   AND epistemic_status NOT IN ('superseded', 'retracted')",
                &[&candidates],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        let remaining_ids: Vec<Uuid> = remaining_rows.into_iter().map(|r| r.get(0)).collect();
        if remaining_ids.len() < 2 {
            client
                .execute(
                    "UPDATE memory_conflicts
                     SET status = 'invalidated',
                         resolution_note = 'candidate claim forgotten',
                         resolved_at = now()
                     WHERE conflict_id = $1",
                    &[&conflict_id],
                )
                .await
                .map_err(|err| svc.map_pg_err(&err))?;
            if remaining_ids.len() == 1 {
                client
                    .execute(
                        "UPDATE memory_claims
                         SET epistemic_status = 'asserted'
                         WHERE claim_id = $1 AND epistemic_status = 'contested'",
                        &[&remaining_ids[0]],
                    )
                    .await
                    .map_err(|err| svc.map_pg_err(&err))?;
            }
        }
    }

    Ok(())
}

async fn scrub_document_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    requested_scope: &MemoryScope,
    document_id: Uuid,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let row = client
        .query_opt(
            "SELECT scope FROM memory_documents WHERE document_id = $1 AND forgotten_at IS NULL FOR UPDATE",
            &[&document_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_document"),
            id: document_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(row.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Forget)?;
    if &stored_scope != requested_scope {
        return Err(DennettError::ForbiddenScope {
            scope: requested_scope.to_string(),
            reason: String::from("document does not belong to the requested scope"),
        });
    }

    client
        .execute(
            "UPDATE memory_document_versions
             SET body = $1,
                 diff_summary = NULL,
                 provenance_claim_ids = '{}'::uuid[]
             WHERE document_id = $2",
            &[&SCRUBBED_PLACEHOLDER, &document_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "UPDATE memory_documents
             SET title = $1,
                 current_body = $1,
                 provenance_claim_ids = '{}'::uuid[],
                 provenance_note_ids = '{}'::uuid[],
                 forgotten_at = now(),
                 updated_at = now()
             WHERE document_id = $2",
            &[&SCRUBBED_PLACEHOLDER, &document_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "DELETE FROM memory_search_index WHERE source_type = 'document' AND source_id = $1",
            &[&document_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    Ok(())
}

/// Lists content-free [`ForgetTombstone`] audit records across authorized scopes.
pub(crate) async fn execute_list_tombstones(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
) -> Result<ListTombstonesResponse, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Ok(ListTombstonesResponse {
            tombstones: Vec::new(),
        });
    }
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let rows = conn
            .query(
                "SELECT tombstone_id, scope, target_type, target_id, forget_event_id,
                        principal_id, reason, residual_backup_notice, propagation_status,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM memory_forget_tombstones
                 WHERE scope = ANY($1::text[])
                 ORDER BY created_at DESC, tombstone_id ASC",
                &[&allowed_scopes],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut tombstones = Vec::with_capacity(rows.len());
        for row in rows {
            let tomb = decode_tombstone_row(&row)?;
            require_scope_capability(principal, &tomb.scope, MemoryCapability::Read)?;
            tombstones.push(tomb);
        }
        Ok(ListTombstonesResponse { tombstones })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::list_tombstones"),
        })?
}

fn forget_event_kind(target_type: ForgetTargetType) -> Result<MemoryEventKind, DennettError> {
    match target_type {
        ForgetTargetType::Note => Ok(MemoryEventKind::NoteForgotten),
        ForgetTargetType::Evidence => Ok(MemoryEventKind::EvidenceForgotten),
        ForgetTargetType::Claim => Ok(MemoryEventKind::ClaimForgotten),
        ForgetTargetType::Document => Ok(MemoryEventKind::DocumentForgotten),
        _ => Err(DennettError::InvalidInput {
            field: String::from("target_type"),
            reason: String::from("unsupported forget target type"),
        }),
    }
}

async fn resolve_forget_replay<C: GenericClient>(
    client: &C,
    existing: StoredIdempotencyRecord,
    expected_kind: MemoryEventKind,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<ForgetResponse, DennettError> {
    if existing.kind != expected_kind
        || existing.payload_fingerprint != incoming_fingerprint.as_str()
    {
        return Err(DennettError::IdempotencyMismatch {
            idempotency_key: key.to_string(),
            reason: String::from(
                "idempotency_key was already used with a different operation or payload",
            ),
        });
    }

    let row = client
        .query_opt(
            "SELECT tombstone_id, forget_event_id, residual_backup_notice, propagation_status
             FROM memory_forget_tombstones
             WHERE forget_event_id = $1",
            &[&existing.event_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("forget_tombstone"),
            id: existing.event_id.to_string(),
        });
    };

    Ok(ForgetResponse {
        tombstone_id: TombstoneId::from_uuid(row.get(0)),
        forget_event_id: MemoryEventId::from_uuid(row.get(1)),
        residual_backup_notice: row.get(2),
        propagation_status: row.get::<_, String>(3).parse()?,
        idempotent_replay: true,
    })
}

fn decode_tombstone_row(row: &Row) -> Result<ForgetTombstone, DennettError> {
    Ok(ForgetTombstone {
        tombstone_id: TombstoneId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        target_type: row.get::<_, String>(2).parse()?,
        target_id: row.get(3),
        forget_event_id: MemoryEventId::from_uuid(row.get(4)),
        principal_id: PrincipalId::try_new(row.get::<_, String>(5))?,
        reason: row.get(6),
        residual_backup_notice: row.get(7),
        propagation_status: row.get::<_, String>(8).parse()?,
        created_at: TimestampMs::from_millis(row.get(9)),
    })
}
