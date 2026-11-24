//! Human-readable versioned topic dossiers (`memory_documents`, `memory_document_versions`).
//!
//! Supports optimistic concurrency via `expected_version`, immutable version history,
//! scope-verified provenance links to claims and notes, and separable search indexing.

use std::collections::{BTreeSet, HashMap};

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AuthPrincipal, ClaimId, CorrelationId, CreateDocumentRequest, DennettError, DocumentId,
    DocumentVersion, DocumentVersionId, GetDocumentVersionsResponse, IdempotencyKey,
    ListDocumentsResponse, MemoryCapability, MemoryDocument, MemoryEventId, MemoryEventKind,
    MemoryNoteId, MemoryScope, PrincipalId, Sha256Hex, TimestampMs, TrustLevel,
    UpdateDocumentRequest,
};
use tokio::time;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::fingerprint::{fingerprint_create_document, fingerprint_update_document};
use crate::search::maybe_index_document_in_tx;
use crate::service::{
    MemoryService, NewMemoryEvent, StoredIdempotencyRecord, insert_memory_event,
    lookup_idempotency, require_resource_scope_capability, require_scope_capability,
    resolve_search_scopes,
};

/// Creates a version-1 [`MemoryDocument`] dossier with immutable version 1 snapshot.
pub(crate) async fn execute_create_document(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: CreateDocumentRequest,
) -> Result<MemoryDocument, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    let fingerprint = fingerprint_create_document(&req);
    let op_fut = create_document_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::create_document"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional dossier creation validates provenance, writes version 1, emits event, and updates search index atomically"
)]
async fn create_document_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &CreateDocumentRequest,
    fingerprint: &Sha256Hex,
) -> Result<MemoryDocument, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_document_replay(
            &tx,
            existing,
            MemoryEventKind::DocumentCreated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    verify_provenance_in_scope(
        &tx,
        &req.scope,
        &req.provenance_claim_ids,
        &req.provenance_note_ids,
        svc,
    )
    .await?;

    let existing_key = tx
        .query_opt(
            "SELECT document_id FROM memory_documents
             WHERE scope = $1 AND document_key = $2 AND forgotten_at IS NULL",
            &[&req.scope.as_str(), &req.document_key],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    if existing_key.is_some() {
        return Err(DennettError::Conflict {
            resource: String::from("memory_document"),
            expected_revision: None,
            actual_revision: None,
            reason: format!(
                "document with key `{}` already exists in scope `{}`",
                req.document_key, req.scope
            ),
        });
    }

    let document_id = DocumentId::new();
    let version_id = DocumentVersionId::new();
    let event_id = MemoryEventId::new();

    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id,
            scope: &req.scope,
            kind: MemoryEventKind::DocumentCreated,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.idempotency_key,
            fingerprint,
            target_entity_id: document_id.as_uuid(),
            target_revision: Some(1),
            observed_ms: None,
        },
        svc,
    )
    .await?
    else {
        let _ = tx.rollback().await;
        return recover_document_idempotency_race(
            &conn,
            &req.scope,
            MemoryEventKind::DocumentCreated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    };

    let claim_uuids: Vec<Uuid> = req
        .provenance_claim_ids
        .iter()
        .map(|id| id.as_uuid())
        .collect();
    let note_uuids: Vec<Uuid> = req
        .provenance_note_ids
        .iter()
        .map(|id| id.as_uuid())
        .collect();

    tx.execute(
        "INSERT INTO memory_documents (
            document_id, scope, document_key, title, current_version, current_body,
            provenance_claim_ids, provenance_note_ids, source_event_id, principal_id,
            trust_level, instruction_active
         ) VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9, 'native', true)",
        &[
            &document_id.as_uuid(),
            &req.scope.as_str(),
            &req.document_key,
            &req.title,
            &req.body,
            &claim_uuids,
            &note_uuids,
            &event_id.as_uuid(),
            &principal.principal_id.as_str(),
        ],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    tx.execute(
        "INSERT INTO memory_document_versions (
            version_id, document_id, version, body, provenance_claim_ids, diff_summary, event_id
         ) VALUES ($1, $2, 1, $3, $4, $5, $6)",
        &[
            &version_id.as_uuid(),
            &document_id.as_uuid(),
            &req.body,
            &claim_uuids,
            &Some("Initial version"),
            &event_id.as_uuid(),
        ],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    maybe_index_document_in_tx(
        &tx,
        &req.scope,
        document_id.as_uuid(),
        event_id,
        stream_seq,
        svc,
    )
    .await?;

    let doc = fetch_document_row(&tx, document_id, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(doc)
}

/// Fetches a non-forgotten [`MemoryDocument`] after verifying read scope access.
pub(crate) async fn execute_get_document(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    document_id: DocumentId,
) -> Result<MemoryDocument, DennettError> {
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let doc = fetch_document_row(&conn, document_id, svc).await?;
        require_resource_scope_capability(principal, &doc.scope, MemoryCapability::Read)?;
        Ok(doc)
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::get_document"),
        })?
}

/// Lists all non-forgotten [`MemoryDocument`] dossiers across authorized scopes.
pub(crate) async fn execute_list_documents(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
) -> Result<ListDocumentsResponse, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Ok(ListDocumentsResponse {
            documents: Vec::new(),
        });
    }
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let rows = conn
            .query(
                "SELECT document_id, scope, document_key, title, current_version, current_body,
                        provenance_claim_ids, provenance_note_ids, source_event_id, principal_id,
                        trust_level, instruction_active,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
                 FROM memory_documents
                 WHERE scope = ANY($1::text[])
                   AND forgotten_at IS NULL
                 ORDER BY updated_at DESC, document_id ASC",
                &[&allowed_scopes],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut documents = Vec::with_capacity(rows.len());
        for row in rows {
            let doc = decode_document_row(&row)?;
            require_scope_capability(principal, &doc.scope, MemoryCapability::Read)?;
            documents.push(doc);
        }
        Ok(ListDocumentsResponse { documents })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::list_documents"),
        })?
}

/// Updates an existing [`MemoryDocument`] with optimistic locking (`expected_version`).
pub(crate) async fn execute_update_document(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: UpdateDocumentRequest,
) -> Result<MemoryDocument, DennettError> {
    req.validate()?;
    require_scope_capability(principal, &req.scope, MemoryCapability::Write)?;
    svc.ensure_migrated().await?;

    let fingerprint = fingerprint_update_document(&req);
    let op_fut = update_document_tx(svc, principal, correlation_id, &req, &fingerprint);

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::update_document"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "transactional dossier update enforces optimistic version lock, writes new version, emits event, and updates search index atomically"
)]
async fn update_document_tx(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    req: &UpdateDocumentRequest,
    fingerprint: &Sha256Hex,
) -> Result<MemoryDocument, DennettError> {
    let mut conn = svc.get_conn(svc.op_timeout()).await?;
    let tx = conn
        .transaction()
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(existing) = lookup_idempotency(&tx, &req.scope, &req.idempotency_key, svc).await? {
        return resolve_document_replay(
            &tx,
            existing,
            MemoryEventKind::DocumentUpdated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    let locked = tx
        .query_opt(
            "SELECT scope, current_version FROM memory_documents
             WHERE document_id = $1 AND forgotten_at IS NULL
             FOR UPDATE",
            &[&req.document_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(locked) = locked else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_document"),
            id: req.document_id.to_string(),
        });
    };

    let stored_scope = MemoryScope::try_new(locked.get::<_, String>(0))?;
    let actual_version: i32 = locked.get(1);
    require_resource_scope_capability(principal, &stored_scope, MemoryCapability::Write)?;
    if stored_scope != req.scope {
        return Err(DennettError::ForbiddenScope {
            scope: req.scope.to_string(),
            reason: String::from("document does not belong to the requested scope"),
        });
    }

    if let Some(existing) =
        lookup_idempotency(&tx, &stored_scope, &req.idempotency_key, svc).await?
    {
        return resolve_document_replay(
            &tx,
            existing,
            MemoryEventKind::DocumentUpdated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    }

    if actual_version != req.expected_version {
        return Err(DennettError::Conflict {
            resource: String::from("memory_document"),
            expected_revision: Some(i64::from(req.expected_version)),
            actual_revision: Some(i64::from(actual_version)),
            reason: format!(
                "optimistic version mismatch: expected version {}, current version is {actual_version}",
                req.expected_version
            ),
        });
    }

    verify_provenance_in_scope(
        &tx,
        &stored_scope,
        &req.provenance_claim_ids,
        &req.provenance_note_ids,
        svc,
    )
    .await?;

    let new_version = actual_version
        .checked_add(1)
        .ok_or_else(|| DennettError::Internal {
            detail: String::from("document version counter overflowed i32"),
        })?;
    let version_id = DocumentVersionId::new();
    let event_id = MemoryEventId::new();

    let Some(stream_seq) = insert_memory_event(
        &tx,
        NewMemoryEvent {
            event_id,
            scope: &stored_scope,
            kind: MemoryEventKind::DocumentUpdated,
            principal_id: &principal.principal_id,
            correlation_id,
            idempotency_key: &req.idempotency_key,
            fingerprint,
            target_entity_id: req.document_id.as_uuid(),
            target_revision: Some(i64::from(new_version)),
            observed_ms: None,
        },
        svc,
    )
    .await?
    else {
        let _ = tx.rollback().await;
        return recover_document_idempotency_race(
            &conn,
            &stored_scope,
            MemoryEventKind::DocumentUpdated,
            &req.idempotency_key,
            fingerprint,
            svc,
        )
        .await;
    };

    let claim_uuids: Vec<Uuid> = req
        .provenance_claim_ids
        .iter()
        .map(|id| id.as_uuid())
        .collect();
    let note_uuids: Vec<Uuid> = req
        .provenance_note_ids
        .iter()
        .map(|id| id.as_uuid())
        .collect();

    tx.execute(
        "INSERT INTO memory_document_versions (
            version_id, document_id, version, body, provenance_claim_ids, diff_summary, event_id
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &version_id.as_uuid(),
            &req.document_id.as_uuid(),
            &new_version,
            &req.body,
            &claim_uuids,
            &req.diff_summary,
            &event_id.as_uuid(),
        ],
    )
    .await
    .map_err(|err| svc.map_pg_err(&err))?;

    let updated = tx
        .execute(
            "UPDATE memory_documents
             SET current_version = $1,
                 title = $2,
                 current_body = $3,
                 provenance_claim_ids = $4,
                 provenance_note_ids = $5,
                 source_event_id = $6,
                 principal_id = $7,
                 updated_at = now()
             WHERE document_id = $8 AND current_version = $9 AND forgotten_at IS NULL",
            &[
                &new_version,
                &req.title,
                &req.body,
                &claim_uuids,
                &note_uuids,
                &event_id.as_uuid(),
                &principal.principal_id.as_str(),
                &req.document_id.as_uuid(),
                &actual_version,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if updated != 1 {
        return Err(DennettError::Conflict {
            resource: String::from("memory_document"),
            expected_revision: Some(i64::from(req.expected_version)),
            actual_revision: None,
            reason: String::from("concurrent update modified document version"),
        });
    }

    maybe_index_document_in_tx(
        &tx,
        &stored_scope,
        req.document_id.as_uuid(),
        event_id,
        stream_seq,
        svc,
    )
    .await?;

    let doc = fetch_document_row(&tx, req.document_id, svc).await?;
    tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
    Ok(doc)
}

/// Fetches the complete immutable version history of a non-forgotten [`MemoryDocument`].
pub(crate) async fn execute_get_document_versions(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    document_id: DocumentId,
) -> Result<GetDocumentVersionsResponse, DennettError> {
    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let head = conn
            .query_opt(
                "SELECT scope, current_version
                 FROM memory_documents
                 WHERE document_id = $1 AND forgotten_at IS NULL",
                &[&document_id.as_uuid()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let Some(head) = head else {
            return Err(DennettError::NotFound {
                resource: String::from("memory_document"),
                id: document_id.to_string(),
            });
        };

        let scope = MemoryScope::try_new(head.get::<_, String>(0))?;
        let current_version: i32 = head.get(1);
        require_resource_scope_capability(principal, &scope, MemoryCapability::Read)?;

        let rows = conn
            .query(
                "SELECT version_id, document_id, version, body, provenance_claim_ids,
                        diff_summary, event_id,
                        TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint
                 FROM memory_document_versions
                 WHERE document_id = $1
                 ORDER BY version ASC",
                &[&document_id.as_uuid()],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        let mut versions = Vec::with_capacity(rows.len());
        for row in rows {
            let raw_claims: Vec<Uuid> = row.get(4);
            versions.push(DocumentVersion {
                version_id: DocumentVersionId::from_uuid(row.get(0)),
                document_id: DocumentId::from_uuid(row.get(1)),
                version: row.get(2),
                body: row.get(3),
                provenance_claim_ids: raw_claims.into_iter().map(ClaimId::from_uuid).collect(),
                diff_summary: row.get(5),
                event_id: MemoryEventId::from_uuid(row.get(6)),
                created_at: TimestampMs::from_millis(row.get(7)),
            });
        }

        Ok(GetDocumentVersionsResponse {
            document_id,
            scope,
            current_version,
            versions,
        })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::get_document_versions"),
        })?
}

/// Inserts or updates a dossier inside an existing transaction (used by deterministic consolidation).
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "internal helper writes both memory_documents and memory_document_versions atomically"
)]
pub(crate) async fn upsert_dossier_in_tx<C: GenericClient>(
    client: &C,
    principal: &AuthPrincipal,
    correlation_id: &CorrelationId,
    scope: &MemoryScope,
    document_key: &str,
    title: &str,
    body: &str,
    provenance_claim_ids: &[Uuid],
    provenance_note_ids: &[Uuid],
    diff_summary: &str,
    trust_level: TrustLevel,
    instruction_active: bool,
    svc: &MemoryService,
) -> Result<DocumentId, DennettError> {
    let existing = client
        .query_opt(
            "SELECT document_id, current_version FROM memory_documents
             WHERE scope = $1 AND document_key = $2 AND forgotten_at IS NULL
             FOR UPDATE",
            &[&scope.as_str(), &document_key],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    if let Some(row) = existing {
        let doc_id = DocumentId::from_uuid(row.get(0));
        let cur_ver: i32 = row.get(1);
        let new_ver = cur_ver.saturating_add(1);
        let version_id = DocumentVersionId::new();
        let event_id = MemoryEventId::new();
        let idem = IdempotencyKey::try_new(format!("doc-upd-{event_id}"))?;
        let fp = crate::fingerprint::sha256_hex(
            format!("{}:{}:{new_ver}:{body}", scope.as_str(), document_key).as_bytes(),
        );

        let stream_seq = insert_memory_event(
            client,
            NewMemoryEvent {
                event_id,
                scope,
                kind: MemoryEventKind::DocumentUpdated,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &idem,
                fingerprint: &fp,
                target_entity_id: doc_id.as_uuid(),
                target_revision: Some(i64::from(new_ver)),
                observed_ms: None,
            },
            svc,
        )
        .await?
        .unwrap_or(0);

        client
            .execute(
                "INSERT INTO memory_document_versions (
                    version_id, document_id, version, body, provenance_claim_ids, diff_summary, event_id
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &version_id.as_uuid(),
                    &doc_id.as_uuid(),
                    &new_ver,
                    &body,
                    &provenance_claim_ids,
                    &Some(diff_summary),
                    &event_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        client
            .execute(
                "UPDATE memory_documents
                 SET current_version = $1,
                     title = $2,
                     current_body = $3,
                     provenance_claim_ids = $4,
                     provenance_note_ids = $5,
                     source_event_id = $6,
                     principal_id = $7,
                     updated_at = now()
                 WHERE document_id = $8",
                &[
                    &new_ver,
                    &title,
                    &body,
                    &provenance_claim_ids,
                    &provenance_note_ids,
                    &event_id.as_uuid(),
                    &principal.principal_id.as_str(),
                    &doc_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        if stream_seq > 0 {
            maybe_index_document_in_tx(client, scope, doc_id.as_uuid(), event_id, stream_seq, svc)
                .await?;
        }
        Ok(doc_id)
    } else {
        let doc_id = DocumentId::new();
        let version_id = DocumentVersionId::new();
        let event_id = MemoryEventId::new();
        let idem = IdempotencyKey::try_new(format!("doc-new-{event_id}"))?;
        let fp = crate::fingerprint::sha256_hex(
            format!("{}:{}:1:{body}", scope.as_str(), document_key).as_bytes(),
        );

        let stream_seq = insert_memory_event(
            client,
            NewMemoryEvent {
                event_id,
                scope,
                kind: MemoryEventKind::DocumentCreated,
                principal_id: &principal.principal_id,
                correlation_id,
                idempotency_key: &idem,
                fingerprint: &fp,
                target_entity_id: doc_id.as_uuid(),
                target_revision: Some(1),
                observed_ms: None,
            },
            svc,
        )
        .await?
        .unwrap_or(0);

        client
            .execute(
                "INSERT INTO memory_documents (
                    document_id, scope, document_key, title, current_version, current_body,
                    provenance_claim_ids, provenance_note_ids, source_event_id, principal_id,
                    trust_level, instruction_active
                 ) VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &doc_id.as_uuid(),
                    &scope.as_str(),
                    &document_key,
                    &title,
                    &body,
                    &provenance_claim_ids,
                    &provenance_note_ids,
                    &event_id.as_uuid(),
                    &principal.principal_id.as_str(),
                    &trust_level.as_str(),
                    &instruction_active,
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        client
            .execute(
                "INSERT INTO memory_document_versions (
                    version_id, document_id, version, body, provenance_claim_ids, diff_summary, event_id
                 ) VALUES ($1, $2, 1, $3, $4, $5, $6)",
                &[
                    &version_id.as_uuid(),
                    &doc_id.as_uuid(),
                    &body,
                    &provenance_claim_ids,
                    &Some(diff_summary),
                    &event_id.as_uuid(),
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;

        if stream_seq > 0 {
            maybe_index_document_in_tx(client, scope, doc_id.as_uuid(), event_id, stream_seq, svc)
                .await?;
        }
        Ok(doc_id)
    }
}

async fn verify_provenance_in_scope<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    claim_ids: &[ClaimId],
    note_ids: &[MemoryNoteId],
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if !claim_ids.is_empty() {
        let unique: BTreeSet<Uuid> = claim_ids.iter().map(|id| id.as_uuid()).collect();
        let vec_ids: Vec<Uuid> = unique.into_iter().collect();
        let rows = client
            .query(
                "SELECT claim_id, scope FROM memory_claims
                 WHERE claim_id = ANY($1::uuid[]) AND forgotten_at IS NULL",
                &[&vec_ids],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        let mut scopes: HashMap<Uuid, String> = HashMap::with_capacity(rows.len());
        for row in rows {
            scopes.insert(row.get(0), row.get(1));
        }
        for cid in claim_ids {
            let Some(sc) = scopes.get(&cid.as_uuid()) else {
                return Err(DennettError::NotFound {
                    resource: String::from("memory_claim"),
                    id: cid.to_string(),
                });
            };
            if sc != scope.as_str() {
                return Err(DennettError::ForbiddenScope {
                    scope: scope.to_string(),
                    reason: format!("provenance claim `{cid}` does not belong to scope `{scope}`"),
                });
            }
        }
    }

    if !note_ids.is_empty() {
        let unique: BTreeSet<Uuid> = note_ids.iter().map(|id| id.as_uuid()).collect();
        let vec_ids: Vec<Uuid> = unique.into_iter().collect();
        let rows = client
            .query(
                "SELECT note_id, scope FROM memory_notes
                 WHERE note_id = ANY($1::uuid[]) AND forgotten_at IS NULL",
                &[&vec_ids],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        let mut scopes: HashMap<Uuid, String> = HashMap::with_capacity(rows.len());
        for row in rows {
            scopes.insert(row.get(0), row.get(1));
        }
        for nid in note_ids {
            let Some(sc) = scopes.get(&nid.as_uuid()) else {
                return Err(DennettError::NotFound {
                    resource: String::from("memory_note"),
                    id: nid.to_string(),
                });
            };
            if sc != scope.as_str() {
                return Err(DennettError::ForbiddenScope {
                    scope: scope.to_string(),
                    reason: format!("provenance note `{nid}` does not belong to scope `{scope}`"),
                });
            }
        }
    }

    Ok(())
}

async fn resolve_document_replay<C: GenericClient>(
    client: &C,
    existing: StoredIdempotencyRecord,
    expected_kind: MemoryEventKind,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<MemoryDocument, DennettError> {
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
    fetch_document_row(
        client,
        DocumentId::from_uuid(existing.target_entity_id),
        svc,
    )
    .await
}

async fn recover_document_idempotency_race<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    expected_kind: MemoryEventKind,
    key: &IdempotencyKey,
    incoming_fingerprint: &Sha256Hex,
    svc: &MemoryService,
) -> Result<MemoryDocument, DennettError> {
    let Some(existing) = lookup_idempotency(client, scope, key, svc).await? else {
        return Err(DennettError::Conflict {
            resource: String::from("memory_document"),
            expected_revision: None,
            actual_revision: None,
            reason: String::from("concurrent idempotency conflict; retry request"),
        });
    };
    resolve_document_replay(
        client,
        existing,
        expected_kind,
        key,
        incoming_fingerprint,
        svc,
    )
    .await
}

async fn fetch_document_row<C: GenericClient>(
    client: &C,
    document_id: DocumentId,
    svc: &MemoryService,
) -> Result<MemoryDocument, DennettError> {
    let row = client
        .query_opt(
            "SELECT document_id, scope, document_key, title, current_version, current_body,
                    provenance_claim_ids, provenance_note_ids, source_event_id, principal_id,
                    trust_level, instruction_active,
                    TRUNC(EXTRACT(EPOCH FROM created_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM memory_documents
             WHERE document_id = $1 AND forgotten_at IS NULL",
            &[&document_id.as_uuid()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Err(DennettError::NotFound {
            resource: String::from("memory_document"),
            id: document_id.to_string(),
        });
    };
    decode_document_row(&row)
}

fn decode_document_row(row: &Row) -> Result<MemoryDocument, DennettError> {
    let raw_claims: Vec<Uuid> = row.get(6);
    let raw_notes: Vec<Uuid> = row.get(7);
    let source_event_uuid: Uuid = row.get(8);
    Ok(MemoryDocument {
        document_id: DocumentId::from_uuid(row.get(0)),
        scope: MemoryScope::try_new(row.get::<_, String>(1))?,
        document_key: row.get(2),
        title: row.get(3),
        current_version: row.get(4),
        current_body: row.get(5),
        provenance_claim_ids: raw_claims.into_iter().map(ClaimId::from_uuid).collect(),
        provenance_note_ids: raw_notes.into_iter().map(MemoryNoteId::from_uuid).collect(),
        source_event_id: Some(MemoryEventId::from_uuid(source_event_uuid)),
        principal_id: PrincipalId::try_new(row.get::<_, String>(9))?,
        trust_level: row.get::<_, String>(10).parse()?,
        instruction_active: row.get(11),
        created_at: TimestampMs::from_millis(row.get(12)),
        updated_at: TimestampMs::from_millis(row.get(13)),
    })
}
