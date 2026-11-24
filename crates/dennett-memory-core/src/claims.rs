//! Structured claims, temporal supersession chains, automatic contradiction detection,
//! and explicit conflict adjudication (`memory_claims`, `memory_conflicts`).
//!
//! Key invariants:
//! - An older claim (`valid_from` earlier than the target's `valid_from`) NEVER silently overrides
//!   a newer decision without an explicit `resolution_event_id` referencing a `conflict_resolved` event.
//! - Competing active claims on the same `(scope, topic_key, claim_key)` automatically open a
//!   `memory_conflicts` record and mark participating claims `contested`.
//! - Concurrent supersession attempts on the same claim serialize via `SELECT ... FOR UPDATE`;
//!   the winner supersedes cleanly and the loser receives `DennettError::Conflict`.

use std::collections::{BTreeMap, BTreeSet};

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AuthPrincipal, ClaimId, ConflictId, ConflictStatus, ConflictType, CorrelationId,
    CreateClaimRequest, DennettError, EpistemicStatus, EventEvidenceLink, IdempotencyKey,
    ListClaimsResponse, ListConflictsResponse, MemoryCapability, MemoryClaim, MemoryConflict,
    MemoryEventId, MemoryEventKind, MemoryNoteId, MemoryScope, ResolveConflictRequest, Sha256Hex,
    SupersedeClaimRequest, TimestampMs, TrustLevel,
};
use serde_json::Value as JsonValue;
use tokio::time;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::fingerprint::{
    fingerprint_create_claim, fingerprint_resolve_conflict, fingerprint_supersede_claim,
};
use crate::search::{maybe_advance_watermark_only, maybe_index_claim_in_tx};
use crate::service::{
    MemoryService, NewMemoryEvent, StoredIdempotencyRecord, fetch_links_for_events,
    insert_evidence_links, insert_memory_event, lookup_idempotency,
    require_resource_scope_capability, require_scope_capability, resolve_search_scopes,
    verify_evidence_links_in_scope,
};

/// Creates a structured [`MemoryClaim`], enforcing supersession guards or detecting conflicts.
pub(crate) async fn execute_create_claim(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: CreateClaimRequest,
) -> Result<MemoryClaim, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    if let Some(target_id) = req.supersedes_claim_id {
        let sup_req = SupersedeClaimRequest {
            claim_id: target_id,
            scope: req.scope.clone(),
            new_claim: req,
            resolution_event_id: None,
        };
        return execute_supersede_claim(svc, principal, correlation_id, sup_req).await;
    }

    let fingerprint = fingerprint_create_claim(&req);
    let op_fut = create_claim_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::create_claim"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional claim creation verifies idempotency, evidence links, temporal/conflict checks, and search index atomically"
)]
async fn create_claim_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &CreateClaimRequest,
    fingerprint: &Sha256Hex,
) -> Result<MemoryClaim, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_claim_replay(
            &tx,
            existing,
            MemoryEventKind::ClaimCreated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    verify_evidence_links_in_scope(&tx, &req.scope, &req.evidence_links, svc).await?;
    verify_source_note_in_scope(&tx, &req.scope, req.source_note_id, svc).await?;

    // Serialize concurrent claim inserts on the same (scope, topic_key) to avoid missed conflicts.
    let lock_key = format!("claim-topic:{}:{}", req.scope.as_str(), req.topic_key);
    tx.execute(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
        &[&lock_key],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    // Re-check idempotency after acquiring topic lock.
    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_claim_replay(
            &tx,
            existing,
            MemoryEventKind::ClaimCreated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    let is_active_incoming = !matches!(
        req.epistemic_status,
        EpistemicStatus::Superseded | EpistemicStatus::Retracted
    );

    let competing_rows = if is_active_incoming {
        tx.query(
            "SELECT claim_id, natural_language,
                    TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint
             FROM memory_claims
             WHERE scope = $1
               AND topic_key = $2
               AND claim_key IS NOT DISTINCT FROM $3
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')
             FOR UPDATE",
            &[&req.scope.as_str(), &req.topic_key, &req.claim_key],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?
    } else {
        Vec::new()
    };

    let incoming_norm = req.natural_language.trim();
    let mut conflicting_ids: Vec<Uuid> = Vec::new();
    let mut has_temporal_mismatch = false;

    for row in &competing_rows {
        let existing_id: Uuid = row.get(0);
        let existing_text: String = row.get(1);
        let existing_vf: Option<i64> = row.get(2);
        if existing_text.trim() != incoming_norm {
            conflicting_ids.push(existing_id);
            if let (Some(new_vf), Some(old_vf)) =
                (req.valid_from.map(TimestampMs::as_millis), existing_vf)
                && new_vf != old_vf
            {
                has_temporal_mismatch = true;
            }
        }
    }

    let claim_id = ClaimId::new();
    let event_id = MemoryEventId::new();
    let effective_status = if conflicting_ids.is_empty() {
        req.epistemic_status
    } else {
        EpistemicStatus::Contested
    };

    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id,
            scope: &req.scope,
            kind: MemoryEventKind::ClaimCreated,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.idempotency_key,
            fingerprint,
            target_entity_id: claim_id.as_uuid(),
            target_revision: None,
            observed_ms: req.valid_from.map(TimestampMs::as_millis),
        },
        svc,
    )
    .await?
    else {
        let _ = tx.rollback().await;
        return recover_claim_idempotency_race(
            &conn,
            &req.scope,
            MemoryEventKind::ClaimCreated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    };

    insert_evidence_links(&tx, event_id, 1, &req.evidence_links, svc).await?;

    insert_claim_row(
        &tx,
        NewClaimRow {
            claim_id,
            scope: &req.scope,
            topic_key: &req.topic_key,
            claim_key: req.claim_key.as_deref(),
            natural_language: &req.natural_language,
            epistemic_status: effective_status,
            supersedes_claim_id: None,
            valid_from: req.valid_from,
            valid_to: req.valid_to,
            source_event_id: event_id,
            source_note_id: req.source_note_id,
            caveats: &req.caveats,
            provenance: &req.provenance,
            trust_level: TrustLevel::Native,
            instruction_active: true,
        },
        svc,
    )
    .await?;

    maybe_index_claim_in_tx(
        &tx,
        &req.scope,
        claim_id.as_uuid(),
        event_id,
        stream_seq,
        svc,
    )
    .await?;

    if !conflicting_ids.is_empty() {
        let conflict_type = if has_temporal_mismatch {
            ConflictType::Temporal
        } else {
            ConflictType::Semantic
        };
        record_claim_conflict_in_tx(
            &tx,
            principal,
            correlation_id,
            &req.scope,
            &req.topic_key,
            conflict_type,
            &conflicting_ids,
            claim_id.as_uuid(),
            svc,
        )
        .await?;
    }

    let claim = fetch_claim_row(&tx, claim_id, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(claim)
}

/// Supersedes an existing active [`MemoryClaim`] atomically under row-level locking.
pub(crate) async fn execute_supersede_claim(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: SupersedeClaimRequest,
) -> Result<MemoryClaim, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    let fingerprint = fingerprint_supersede_claim(&req);
    let op_fut = supersede_claim_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::supersede_claim"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional claim supersession holds row lock, enforces temporal guard, emits event, and updates search index atomically"
)]
async fn supersede_claim_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &SupersedeClaimRequest,
    fingerprint: &Sha256Hex,
) -> Result<MemoryClaim, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(existing) =
        lookup_idempotency(&tx, &req.scope, &req.new_claim.idempotency_key, svc).await?
    {
        return resolve_claim_replay(
            &tx,
            existing,
            MemoryEventKind::ClaimSuperseded,
            &req.new_claim.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    // Lock target claim row exclusively so concurrent supersessions serialize cleanly.
    let target_row = tx
        .query_opt(
            "SELECT scope, epistemic_status,
                    TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint
             FROM memory_claims
             WHERE claim_id = $1 AND forgotten_at IS NULL
             FOR UPDATE",
            &[&req.claim_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(target_row) = target_row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_claim"),
            id: req.claim_id.to_string(),
        });
    };

    let target_scope = MemoryScope::try_new(target_row.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &target_scope, MemoryCapability::Write)?;
    if target_scope != req.scope {
        return Err(DennettError::ForbiddenScope {
            scope: req.scope.to_string(),
            reason: String::from("claim does not belong to the requested scope"),
        });
    }

    // Re-check idempotency after acquiring FOR UPDATE lock.
    if let Some(existing) =
        lookup_idempotency(&tx, &req.scope, &req.new_claim.idempotency_key, svc).await?
    {
        return resolve_claim_replay(
            &tx,
            existing,
            MemoryEventKind::ClaimSuperseded,
            &req.new_claim.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    let target_status: EpistemicStatus = target_row.get::<_, String>(1).parse()?;
    if matches!(
        target_status,
        EpistemicStatus::Superseded | EpistemicStatus::Retracted
    ) {
        return Err(DennettError::Conflict {
            resource: String::from("memory_claim"),
            expected_revision: None,
            actual_revision: None,
            reason: format!(
                "claim `{}` is already `{target_status}` and cannot be superseded concurrently",
                req.claim_id
            ),
        });
    }

    // Temporal invariant: an older claim NEVER supersedes a newer decision without an explicit
    // verified `conflict_resolved` resolution event.
    let target_valid_from: Option<TimestampMs> = target_row
        .get::<_, Option<i64>>(2)
        .map(TimestampMs::from_millis);
    if let (Some(new_vf), Some(old_vf)) = (req.new_claim.valid_from, target_valid_from)
        && new_vf < old_vf
    {
        match req.resolution_event_id {
            None => {
                return Err(DennettError::Conflict {
                    resource: String::from("memory_claim"),
                    expected_revision: None,
                    actual_revision: None,
                    reason: format!(
                        "older claim (valid_from={new_vf}) cannot supersede newer claim `{}` (valid_from={old_vf}) without explicit resolution_event_id",
                        req.claim_id
                    ),
                });
            }
            Some(res_event_id) => {
                verify_resolution_event_in_scope(&tx, &req.scope, res_event_id, svc).await?;
            }
        }
    }

    verify_evidence_links_in_scope(&tx, &req.scope, &req.new_claim.evidence_links, svc).await?;
    verify_source_note_in_scope(&tx, &req.scope, req.new_claim.source_note_id, svc).await?;

    let new_claim_id = ClaimId::new();
    let event_id = MemoryEventId::new();

    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id,
            scope: &req.scope,
            kind: MemoryEventKind::ClaimSuperseded,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.new_claim.idempotency_key,
            fingerprint,
            target_entity_id: new_claim_id.as_uuid(),
            target_revision: None,
            observed_ms: req.new_claim.valid_from.map(TimestampMs::as_millis),
        },
        svc,
    )
    .await?
    else {
        let _ = tx.rollback().await;
        return recover_claim_idempotency_race(
            &conn,
            &req.scope,
            MemoryEventKind::ClaimSuperseded,
            &req.new_claim.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    };

    insert_evidence_links(&tx, event_id, 1, &req.new_claim.evidence_links, svc).await?;

    let updated = tx
        .execute(
            "UPDATE memory_claims
             SET epistemic_status = 'superseded'
             WHERE claim_id = $1
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')",
            &[&req.claim_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if updated != 1 {
        return Err(DennettError::Conflict {
            resource: String::from("memory_claim"),
            expected_revision: None,
            actual_revision: None,
            reason: String::from("concurrent supersession already modified target claim"),
        });
    }

    let replacement_status = if matches!(
        req.new_claim.epistemic_status,
        EpistemicStatus::Superseded | EpistemicStatus::Retracted
    ) {
        EpistemicStatus::Asserted
    } else {
        req.new_claim.epistemic_status
    };

    insert_claim_row(
        &tx,
        NewClaimRow {
            claim_id: new_claim_id,
            scope: &req.scope,
            topic_key: &req.new_claim.topic_key,
            claim_key: req.new_claim.claim_key.as_deref(),
            natural_language: &req.new_claim.natural_language,
            epistemic_status: replacement_status,
            supersedes_claim_id: Some(req.claim_id),
            valid_from: req.new_claim.valid_from,
            valid_to: req.new_claim.valid_to,
            source_event_id: event_id,
            source_note_id: req.new_claim.source_note_id,
            caveats: &req.new_claim.caveats,
            provenance: &req.new_claim.provenance,
            trust_level: TrustLevel::Native,
            instruction_active: true,
        },
        svc,
    )
    .await?;

    maybe_index_claim_in_tx(
        &tx,
        &req.scope,
        new_claim_id.as_uuid(),
        event_id,
        stream_seq,
        svc,
    )
    .await?;

    let claim = fetch_claim_row(&tx, new_claim_id, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(claim)
}

/// Fetches a single non-forgotten [`MemoryClaim`] after verifying read scope access.
pub(crate) async fn execute_get_claim(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    claim_id: ClaimId,
) -> Result<MemoryClaim, DennettError> {
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let claim = fetch_claim_row(&conn, claim_id, svc).await?;
        require_resource_scope_capability(principal, &claim.scope, MemoryCapability::Read)?;
        Ok(claim)
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::get_claim"),
        })?
}

/// Lists non-forgotten [`MemoryClaim`] records across authorized scopes with optional topic filter.
pub(crate) async fn execute_list_claims(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
    topic_key: Option<&str>,
) -> Result<ListClaimsResponse, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Ok(ListClaimsResponse { claims: Vec::new() });
    }
    svc.ensure_migrated().await?;

    let trimmed_topic = topic_key
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(ToOwned::to_owned);

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let rows = if let Some(ref topic) = trimmed_topic {
            conn.query(
                "SELECT claim_id, scope, topic_key, claim_key, natural_language,
                        epistemic_status, supersedes_claim_id,
                        TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM valid_to) * 1000)::bigint,
                        source_event_id, source_note_id, caveats, provenance,
                        trust_level, instruction_active,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM memory_claims
                 WHERE scope = ANY($1::text[])
                   AND topic_key = $2
                   AND forgotten_at IS NULL
                 ORDER BY created_at DESC, claim_id ASC",
                &[&allowed_scopes, topic],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?
        } else {
            conn.query(
                "SELECT claim_id, scope, topic_key, claim_key, natural_language,
                        epistemic_status, supersedes_claim_id,
                        TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM valid_to) * 1000)::bigint,
                        source_event_id, source_note_id, caveats, provenance,
                        trust_level, instruction_active,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM memory_claims
                 WHERE scope = ANY($1::text[])
                   AND forgotten_at IS NULL
                 ORDER BY created_at DESC, claim_id ASC",
                &[&allowed_scopes],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?
        };

        let event_uuids: Vec<Uuid> = rows.iter().map(|r| r.get::<_, Uuid>(9)).collect();
        let mut links_by_event = fetch_links_for_events(&conn, &event_uuids, svc).await?;

        let mut claims = Vec::with_capacity(rows.len());
        for row in rows {
            let ev_uuid: Uuid = row.get(9);
            let links = links_by_event.remove(&ev_uuid).unwrap_or_default();
            let claim = decode_claim_row(&row, links)?;
            require_scope_capability(principal, &claim.scope, MemoryCapability::Read)?;
            claims.push(claim);
        }

        Ok(ListClaimsResponse { claims })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::list_claims"),
        })?
}

/// Lists all open [`MemoryConflict`] rows across authorized scopes.
pub(crate) async fn execute_list_open_conflicts(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
) -> Result<ListConflictsResponse, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Ok(ListConflictsResponse {
            conflicts: Vec::new(),
        });
    }
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let rows = conn
            .query(
                "SELECT conflict_id, scope, topic_key, conflict_type, candidate_claim_ids,
                        blocking, status, resolution_event_id, resolution_note,
                        TRUNC(EXTRACT(EPOCH FROM detected_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM resolved_at) * 1000)::bigint
                 FROM memory_conflicts
                 WHERE scope = ANY($1::text[])
                   AND status = 'open'
                 ORDER BY detected_at DESC, conflict_id ASC",
                &[&allowed_scopes],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut conflicts = Vec::with_capacity(rows.len());
        for row in rows {
            let conflict = decode_conflict_row(&row)?;
            require_scope_capability(principal, &conflict.scope, MemoryCapability::Read)?;
            conflicts.push(conflict);
        }
        Ok(ListConflictsResponse { conflicts })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::list_open_conflicts"),
        })?
}

/// Resolves or adjudicates an open [`MemoryConflict`] and updates candidate claim statuses.
pub(crate) async fn execute_resolve_conflict(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: ResolveConflictRequest,
) -> Result<MemoryConflict, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    let fingerprint = fingerprint_resolve_conflict(&req);
    let op_fut = resolve_conflict_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::resolve_conflict"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional conflict resolution validates winning candidate, updates statuses, and appends resolution event atomically"
)]
async fn resolve_conflict_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &ResolveConflictRequest,
    fingerprint: &Sha256Hex,
) -> Result<MemoryConflict, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_conflict_replay(&tx, existing, &req.idempotency_key, fingerprint, svc)
            .await;
    }

    let locked = tx
        .query_opt(
            "SELECT scope, candidate_claim_ids, status
             FROM memory_conflicts
             WHERE conflict_id = $1
             FOR UPDATE",
            &[&req.conflict_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(locked) = locked else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_conflict"),
            id: req.conflict_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(locked.get::<_, String>(0))?;
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Write)?;
    if stored_scope != req.scope {
        return Err(DennettError::ForbiddenScope {
            scope: req.scope.to_string(),
            reason: String::from("conflict does not belong to the requested scope"),
        });
    }

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_conflict_replay(&tx, existing, &req.idempotency_key, fingerprint, svc)
            .await;
    }

    let current_status: ConflictStatus = locked.get::<_, String>(2).parse()?;
    if current_status != ConflictStatus::Open {
        return Err(DennettError::Conflict {
            resource: String::from("memory_conflict"),
            expected_revision: None,
            actual_revision: None,
            reason: format!(
                "conflict `{}` is already `{current_status}`",
                req.conflict_id
            ),
        });
    }

    let candidate_uuids: Vec<Uuid> = locked.get(1);
    let target_status = req.target_status.unwrap_or(ConflictStatus::Resolved);

    if let Some(winner_id) = req.winning_claim_id {
        if !candidate_uuids.contains(&winner_id.as_uuid()) {
            return Err(DennettError::InvalidInput {
                field: String::from("winning_claim_id"),
                reason: String::from(
                    "winning_claim_id must be one of the conflict candidate_claim_ids",
                ),
            });
        }
        // Verify winner is still active and non-forgotten.
        let winner_exists = tx
            .query_opt(
                "SELECT 1 FROM memory_claims WHERE claim_id = $1 AND scope = $2 AND forgotten_at IS NULL",
                &[&winner_id.as_uuid(), &req.scope.as_str()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        if winner_exists.is_none() {
            return Err(DennettError::NotFound {
                resource: String::from("memory_claim"),
                id: winner_id.to_string(),
            });
        }
    }

    let event_id = MemoryEventId::new();
    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id,
            scope: &req.scope,
            kind: MemoryEventKind::ConflictResolved,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.idempotency_key,
            fingerprint,
            target_entity_id: req.conflict_id.as_uuid(),
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
                resource: String::from("memory_conflict"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("concurrent idempotency conflict; retry request"),
            });
        };
        return resolve_conflict_replay(&conn, existing, &req.idempotency_key, fingerprint, svc)
            .await;
    };

    tx.execute(
        "UPDATE memory_conflicts
         SET status = $1,
             resolution_event_id = $2,
             resolution_note = $3,
             resolved_at = now()
         WHERE conflict_id = $4",
        &[
            &target_status.as_str(),
            &event_id.as_uuid(),
            &req.resolution_note,
            &req.conflict_id.as_uuid(),
        ],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    if target_status == ConflictStatus::Resolved
        && let Some(winner_id) = req.winning_claim_id
    {
        tx.execute(
            "UPDATE memory_claims
             SET epistemic_status = 'asserted'
             WHERE claim_id = $1 AND forgotten_at IS NULL",
            &[&winner_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

        let losers: Vec<Uuid> = candidate_uuids
            .iter()
            .copied()
            .filter(|id| *id != winner_id.as_uuid())
            .collect();
        if !losers.is_empty() {
            tx.execute(
                "UPDATE memory_claims
                 SET epistemic_status = 'superseded'
                 WHERE claim_id = ANY($1::uuid[])
                   AND forgotten_at IS NULL
                   AND epistemic_status <> 'retracted'",
                &[&losers],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        }
    }

    maybe_advance_watermark_only(&tx, &req.scope, event_id, stream_seq, svc).await?;

    let conflict = fetch_conflict_row(&tx, req.conflict_id, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(conflict)
}

pub(crate) struct NewClaimRow<'a> {
    pub(crate) claim_id: ClaimId,
    pub(crate) scope: &'a MemoryScope,
    pub(crate) topic_key: &'a str,
    pub(crate) claim_key: Option<&'a str>,
    pub(crate) natural_language: &'a str,
    pub(crate) epistemic_status: EpistemicStatus,
    pub(crate) supersedes_claim_id: Option<ClaimId>,
    pub(crate) valid_from: Option<TimestampMs>,
    pub(crate) valid_to: Option<TimestampMs>,
    pub(crate) source_event_id: MemoryEventId,
    pub(crate) source_note_id: Option<MemoryNoteId>,
    pub(crate) caveats: &'a [String],
    pub(crate) provenance: &'a BTreeMap<String, String>,
    pub(crate) trust_level: TrustLevel,
    pub(crate) instruction_active: bool,
}

pub(crate) async fn insert_claim_row<C: GenericClient>(
    client: &C,
    row: NewClaimRow<'_>,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let caveats_json = serde_json::to_value(row.caveats).map_err(|err| DennettError::Internal {
        detail: format!("failed to serialize claim caveats: {err}"),
    })?;
    let provenance_json =
        serde_json::to_value(row.provenance).map_err(|err| DennettError::Internal {
            detail: format!("failed to serialize claim provenance: {err}"),
        })?;
    let start_ms = row.valid_from.map(TimestampMs::as_millis);
    let end_ms = row.valid_to.map(TimestampMs::as_millis);
    let sup_uuid = row.supersedes_claim_id.map(ClaimId::as_uuid);
    let note_uuid = row.source_note_id.map(MemoryNoteId::as_uuid);

    client
        .execute(
            "INSERT INTO memory_claims (
                claim_id, scope, topic_key, claim_key, natural_language,
                epistemic_status, supersedes_claim_id, valid_from, valid_to,
                source_event_id, source_note_id, caveats, provenance,
                trust_level, instruction_active
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                TIMESTAMPTZ '1970-01-01 00:00:00+00' + ($8::bigint * INTERVAL '1 millisecond'),
                TIMESTAMPTZ '1970-01-01 00:00:00+00' + ($9::bigint * INTERVAL '1 millisecond'),
                $10, $11, $12, $13, $14, $15
             )",
            &[
                &row.claim_id.as_uuid(),
                &row.scope.as_str(),
                &row.topic_key,
                &row.claim_key,
                &row.natural_language,
                &row.epistemic_status.as_str(),
                &sup_uuid,
                &start_ms,
                &end_ms,
                &row.source_event_id.as_uuid(),
                &note_uuid,
                &caveats_json,
                &provenance_json,
                &row.trust_level.as_str(),
                &row.instruction_active,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal conflict recorder takes explicit transaction, principal, scope, topic, and candidates"
)]
pub(crate) async fn record_claim_conflict_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    scope: &MemoryScope,
    topic_key: &str,
    conflict_type: ConflictType,
    existing_claim_ids: &[Uuid],
    new_claim_id: Uuid,
    svc: &MemoryService,
) -> Result<ConflictId, DennettError> {
    let mut candidate_set: BTreeSet<Uuid> = existing_claim_ids.iter().copied().collect();
    candidate_set.insert(new_claim_id);
    let candidate_vec: Vec<Uuid> = candidate_set.iter().copied().collect();

    // Mark all participating claims `contested`.
    client
        .execute(
            "UPDATE memory_claims
             SET epistemic_status = 'contested'
             WHERE claim_id = ANY($1::uuid[])
               AND forgotten_at IS NULL
               AND epistemic_status NOT IN ('superseded', 'retracted')",
            &[&candidate_vec],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    // Check if an open conflict already exists for this scope+topic overlapping these claims.
    let existing_conflict = client
        .query_opt(
            "SELECT conflict_id, candidate_claim_ids
             FROM memory_conflicts
             WHERE scope = $1
               AND topic_key = $2
               AND status = 'open'
               AND candidate_claim_ids && $3::uuid[]
             ORDER BY detected_at ASC
             LIMIT 1
             FOR UPDATE",
            &[&scope.as_str(), &topic_key, &candidate_vec],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let conflict_id = if let Some(row) = existing_conflict {
        let cid = ConflictId::from_uuid(row.get(0));
        let mut merged: Vec<Uuid> = row.get(1);
        for id in &candidate_vec {
            if !merged.contains(id) {
                merged.push(*id);
            }
        }
        client
            .execute(
                "UPDATE memory_conflicts
                 SET candidate_claim_ids = $1,
                     blocking = true
                 WHERE conflict_id = $2",
                &[&merged, &cid.as_uuid()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        cid
    } else {
        let cid = ConflictId::new();
        client
            .execute(
                "INSERT INTO memory_conflicts (
                    conflict_id, scope, topic_key, conflict_type,
                    candidate_claim_ids, blocking, status
                 ) VALUES ($1, $2, $3, $4, $5, true, 'open')",
                &[
                    &cid.as_uuid(),
                    &scope.as_str(),
                    &topic_key,
                    &conflict_type.as_str(),
                    &candidate_vec,
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        cid
    };

    let conflict_event_id = MemoryEventId::new();
    let conflict_idem = IdempotencyKey::try_new(format!("conflict-det-{conflict_event_id}"))?;
    let fp = crate::fingerprint::sha256_hex(conflict_idem.as_str().as_bytes());
    if let Some(stream_seq) = insert_memory_event(
        client,
        NewMemoryEvent {
            event_id: conflict_event_id,
            scope,
            kind: MemoryEventKind::ConflictDetected,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &conflict_idem,
            fingerprint: &fp,
            target_entity_id: conflict_id.as_uuid(),
            target_revision: None,
            observed_ms: None,
        },
        svc,
    )
    .await?
    {
        maybe_advance_watermark_only(client, scope, conflict_event_id, stream_seq, svc).await?;
    }

    Ok(conflict_id)
}

async fn verify_source_note_in_scope<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    source_note_id: Option<MemoryNoteId>,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let Some(note_id) = source_note_id else {
        return Ok(());
    };
    let row = client
        .query_opt(
            "SELECT scope FROM memory_notes WHERE note_id = $1 AND forgotten_at IS NULL",
            &[&note_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_note"),
            id: note_id.to_string(),
        });
    };
    let note_scope: String = row.get(0);
    if note_scope != scope.as_str() {
        return Err(DennettError::ForbiddenScope {
            scope: scope.to_string(),
            reason: format!("source_note_id `{note_id}` does not belong to scope `{scope}`"),
        });
    }
    Ok(())
}

async fn verify_resolution_event_in_scope<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    event_id: MemoryEventId,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    let row = client
        .query_opt(
            "SELECT scope, event_kind FROM memory_events WHERE event_id = $1",
            &[&event_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_event"),
            id: event_id.to_string(),
        });
    };
    let ev_scope: String = row.get(0);
    let ev_kind: String = row.get(1);
    if ev_scope != scope.as_str() {
        return Err(DennettError::ForbiddenScope {
            scope: scope.to_string(),
            reason: String::from("resolution_event_id does not belong to target scope"),
        });
    }
    if ev_kind != MemoryEventKind::ConflictResolved.as_str() {
        return Err(DennettError::InvalidInput {
            field: String::from("resolution_event_id"),
            reason: format!(
                "resolution_event_id must reference a `conflict_resolved` event, got `{ev_kind}`"
            ),
        });
    }
    Ok(())
}

async fn resolve_claim_replay<C: GenericClient>(
    client: &C,
    existing: StoredIdempotencyRecord,
    expected_kind: MemoryEventKind,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<MemoryClaim, DennettError> {
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
    fetch_claim_row(client, ClaimId::from_uuid(existing.target_entity_id), svc).await
}

async fn recover_claim_idempotency_race<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    expected_kind: MemoryEventKind,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<MemoryClaim, DennettError> {
    let Some(existing) = lookup_idempotency(client, scope, key, svc).await? else {
        return Err(DennettError::Conflict {
            resource: String::from("memory_claim"),
            expected_revision: None,
            actual_revision: None,
            reason: String::from("concurrent idempotency conflict; retry request"),
        });
    };
    resolve_claim_replay(
        client,
        existing,
        expected_kind,
        key,
        incoming_fingerprint,
        svc,
    )
    .await
}

async fn resolve_conflict_replay<C: GenericClient>(
    client: &C,
    existing: StoredIdempotencyRecord,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<MemoryConflict, DennettError> {
    if existing.kind != MemoryEventKind::ConflictResolved
        || existing.payload_fingerprint != incoming_fingerprint.as_str()
    {
        return Err(DennettError::IdempotencyMismatch {
            idempotency_key: key.to_string(),
            reason: String::from(
                "idempotency_key was already used with a different operation or payload",
            ),
        });
    }
    fetch_conflict_row(
        client,
        ConflictId::from_uuid(existing.target_entity_id),
        svc,
    )
    .await
}

pub(crate) async fn fetch_claim_row<C: GenericClient>(
    client: &C,
    claim_id: ClaimId,
    svc: &MemoryService,
) -> Result<MemoryClaim, DennettError> {
    let row = client
        .query_opt(
            "SELECT claim_id, scope, topic_key, claim_key, natural_language,
                    epistemic_status, supersedes_claim_id,
                    TRUNC(EXTRACT(EPOCH FROM valid_from) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM valid_to) * 1000)::bigint,
                    source_event_id, source_note_id, caveats, provenance,
                    trust_level, instruction_active,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
             FROM memory_claims
             WHERE claim_id = $1 AND forgotten_at IS NULL",
            &[&claim_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_claim"),
            id: claim_id.to_string(),
        });
    };

    let ev_uuid: Uuid = row.get(9);
    let mut links = fetch_links_for_events(client, &[ev_uuid], svc).await?;
    decode_claim_row(&row, links.remove(&ev_uuid).unwrap_or_default())
}

async fn fetch_conflict_row<C: GenericClient>(
    client: &C,
    conflict_id: ConflictId,
    svc: &MemoryService,
) -> Result<MemoryConflict, DennettError> {
    let row = client
        .query_opt(
            "SELECT conflict_id, scope, topic_key, conflict_type, candidate_claim_ids,
                    blocking, status, resolution_event_id, resolution_note,
                    TRUNC(EXTRACT(EPOCH FROM detected_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM resolved_at) * 1000)::bigint
             FROM memory_conflicts
             WHERE conflict_id = $1",
            &[&conflict_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_conflict"),
            id: conflict_id.to_string(),
        });
    };
    decode_conflict_row(&row)
}

pub(crate) fn decode_claim_row(
    row: &Row,
    evidence_links: Vec<EventEvidenceLink>,
) -> Result<MemoryClaim, DennettError> {
    let caveats: Vec<String> =
        serde_json::from_value(row.get::<_, JsonValue>(11)).map_err(|err| {
            DennettError::Internal {
                detail: format!("corrupt claim caveats JSON: {err}"),
            }
        })?;
    let provenance: BTreeMap<String, String> = serde_json::from_value(row.get::<_, JsonValue>(12))
        .map_err(|err| DennettError::Internal {
            detail: format!("corrupt claim provenance JSON: {err}"),
        })?;

    Ok(MemoryClaim {
        claim_id: ClaimId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        topic_key: row.get(2),
        claim_key: row.get(3),
        natural_language: row.get(4),
        epistemic_status: row.get::<_, String>(5).parse()?,
        supersedes_claim_id: row.get::<_, Option<Uuid>>(6).map(ClaimId::from_uuid),
        valid_from: row.get::<_, Option<i64>>(7).map(TimestampMs::from_millis),
        valid_to: row.get::<_, Option<i64>>(8).map(TimestampMs::from_millis),
        source_event_id: MemoryEventId::from_uuid(row.get(9)),
        source_note_id: row.get::<_, Option<Uuid>>(10).map(MemoryNoteId::from_uuid),
        caveats,
        provenance,
        trust_level: row.get::<_, String>(13).parse()?,
        instruction_active: row.get(14),
        evidence_links,
        created_at: TimestampMs::from_millis(row.get(15)),
    })
}

fn decode_conflict_row(row: &Row) -> Result<MemoryConflict, DennettError> {
    let raw_ids: Vec<Uuid> = row.get(4);
    Ok(MemoryConflict {
        conflict_id: ConflictId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        topic_key: row.get(2),
        conflict_type: row.get::<_, String>(3).parse()?,
        candidate_claim_ids: raw_ids.into_iter().map(ClaimId::from_uuid).collect(),
        blocking: row.get(5),
        status: row.get::<_, String>(6).parse()?,
        resolution_event_id: row.get::<_, Option<Uuid>>(7).map(MemoryEventId::from_uuid),
        resolution_note: row.get(8),
        detected_at: TimestampMs::from_millis(row.get(9)),
        resolved_at: row.get::<_, Option<i64>>(10).map(TimestampMs::from_millis),
    })
}
