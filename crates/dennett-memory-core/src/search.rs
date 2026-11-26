//! Separable, rebuildable multi-language PostgreSQL FTS search index with fresh-tail overlay.
//!
//! Architectural invariants (D3 Memory Fabric):
//! - Uses stock PostgreSQL 16 built-in dictionaries (`simple`, `english`, `russian`) + GIN indexes.
//! - Zero external extensions (`pgvector` is intentionally forbidden).
//! - `memory_search_index` is strictly derived and disposable: truncating or corrupting it never
//!   loses canonical notes/claims/documents, surfaces `IndexStatus::Degraded` honestly, keeps
//!   writes immediately searchable via the fresh-tail overlay, and recovers cleanly via rebuild.

use deadpool_postgres::GenericClient;
use dennett_contracts::{
    AdmissionStatus, AuthPrincipal, DennettError, IndexState, IndexStatus, MemoryCapability,
    MemoryEventId, MemoryScope, SearchNotesRequest, SearchNotesResponse, TimestampMs,
    TriggerIndexRebuildRequest, TriggerIndexRebuildResponse,
};
use tokio::time;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::service::{
    MemoryService, decode_note_row, fetch_links_for_events, require_scope_capability,
    resolve_search_scopes, scope_stream_id,
};

/// Canonical tokenizer/FTS pipeline version recorded in `memory_index_state`.
pub const SEARCH_MODEL_VERSION: &str = "fts-v1";

/// Validated, SQL-injection-safe search query terms ready for parameterized PostgreSQL execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSearchQuery {
    /// Sanitized plain text passed to PostgreSQL `plainto_tsquery`.
    pub plain_text: String,
    /// Escaped `ILIKE` substring pattern (`%...%`) using `\` as the escape character.
    pub like_pattern: String,
}

/// Sanitizes and prepares an arbitrary user search string for parameterized FTS + `ILIKE` lookup.
///
/// Never panics on arbitrary Unicode, strips NUL/ASCII control characters cleanly, and returns
/// `None` if no searchable non-whitespace characters remain.
#[must_use]
pub fn prepare_search_query(raw: &str) -> Option<PreparedSearchQuery> {
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch == '\0' || (ch.is_control() && !ch.is_whitespace()) {
                ' '
            } else {
                ch
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        return None;
    }
    let like_pattern = format!("%{}%", escape_like_pattern(trimmed));
    Some(PreparedSearchQuery {
        plain_text: trimmed.to_owned(),
        like_pattern,
    })
}

/// Escapes `%`, `_`, and `\` for PostgreSQL `ILIKE ... ESCAPE '\'` predicates.
#[must_use]
pub fn escape_like_pattern(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

const fn status_severity(status: IndexStatus) -> u8 {
    match status {
        IndexStatus::Healthy => 0,
        IndexStatus::Lagging => 1,
        IndexStatus::Rebuilding => 2,
        _ => 3,
    }
}

fn stale_warning_for_status(status: IndexStatus) -> Option<String> {
    match status {
        IndexStatus::Healthy => None,
        IndexStatus::Lagging => Some(String::from(
            "search index watermark trails canonical stream head; fresh tail overlay active",
        )),
        IndexStatus::Rebuilding => Some(String::from(
            "search index rebuild in progress; fresh tail overlay active",
        )),
        IndexStatus::Degraded => Some(String::from(
            "search index is degraded or missing entries; fresh tail overlay active — run index rebuild",
        )),
        _ => Some(String::from(
            "search index status degraded; fresh tail overlay active",
        )),
    }
}

async fn is_scope_eligible_for_eager_index<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<bool, DennettError> {
    let row = client
        .query_opt(
            "SELECT status, watermark_seq FROM memory_index_state WHERE scope = $1 FOR UPDATE",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let Some(row) = row else {
        return Ok(true);
    };
    let status: String = row.get(0);
    let watermark_seq: i64 = row.get(1);
    Ok(status == "healthy" && watermark_seq >= stream_seq.saturating_sub(1))
}

async fn upsert_healthy_watermark<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    event_id: MemoryEventId,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    client
        .execute(
            "INSERT INTO memory_index_state (scope, watermark_event_id, watermark_seq, model_version, status, updated_at)
             VALUES ($1, $2, $3, $4, 'healthy', now())
             ON CONFLICT (scope)
             DO UPDATE SET
                 watermark_event_id = CASE
                     WHEN $3 >= memory_index_state.watermark_seq THEN EXCLUDED.watermark_event_id
                     ELSE memory_index_state.watermark_event_id
                 END,
                 watermark_seq = GREATEST(memory_index_state.watermark_seq, EXCLUDED.watermark_seq),
                 updated_at = now()
             WHERE memory_index_state.status = 'healthy'",
            &[
                &scope.as_str(),
                &event_id.as_uuid(),
                &stream_seq,
                &SEARCH_MODEL_VERSION,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    Ok(())
}

/// Advances the healthy search-index watermark for non-indexed canonical events without
/// masking an existing `lagging`, `rebuilding`, or `degraded` state.
pub(crate) async fn maybe_advance_watermark_only<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    event_id: MemoryEventId,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if is_scope_eligible_for_eager_index(client, scope, stream_seq, svc).await? {
        upsert_healthy_watermark(client, scope, event_id, stream_seq, svc).await?;
    }
    Ok(())
}

/// Upserts a note into `memory_search_index` when the scope index is healthy and caught up.
///
/// When the scope index is lagging, rebuilding, or degraded, the write is intentionally left
/// in the fresh-tail overlay so the degraded/lagging signal remains honest until rebuild.
pub(crate) async fn maybe_index_note_in_tx<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    note_id: Uuid,
    admission_status: AdmissionStatus,
    event_id: MemoryEventId,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if !is_scope_eligible_for_eager_index(client, scope, stream_seq, svc).await? {
        return Ok(());
    }

    if admission_status == AdmissionStatus::Rejected {
        client
            .execute(
                "DELETE FROM memory_search_index WHERE source_type = 'note' AND source_id = $1",
                &[&note_id],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
    } else {
        let entry_id = Uuid::new_v4();
        client
            .execute(
                "INSERT INTO memory_search_index (
                    entry_id, source_type, source_id, scope,
                    tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
                 )
                 SELECT
                    $1, 'note', n.note_id, n.scope,
                    to_tsvector('simple',
                        n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                        || COALESCE(n.topic_key, '') || ' '
                        || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                    to_tsvector('english',
                        n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                        || COALESCE(n.topic_key, '') || ' '
                        || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                    to_tsvector('russian',
                        n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                        || COALESCE(n.topic_key, '') || ' '
                        || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                    now(),
                    n.updated_at
                 FROM memory_notes n
                 WHERE n.note_id = $2 AND n.forgotten_at IS NULL
                 ON CONFLICT (source_type, source_id)
                 DO UPDATE SET
                    tsv_simple = EXCLUDED.tsv_simple,
                    tsv_english = EXCLUDED.tsv_english,
                    tsv_russian = EXCLUDED.tsv_russian,
                    indexed_at = now(),
                    source_updated_at = EXCLUDED.source_updated_at",
                &[&entry_id, &note_id],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
    }

    upsert_healthy_watermark(client, scope, event_id, stream_seq, svc).await
}

/// Upserts a structured claim into `memory_search_index` when the scope index is healthy.
pub(crate) async fn maybe_index_claim_in_tx<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    claim_id: Uuid,
    event_id: MemoryEventId,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if !is_scope_eligible_for_eager_index(client, scope, stream_seq, svc).await? {
        return Ok(());
    }

    let entry_id = Uuid::new_v4();
    client
        .execute(
            "INSERT INTO memory_search_index (
                entry_id, source_type, source_id, scope,
                tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
             )
             SELECT
                $1, 'claim', c.claim_id, c.scope,
                to_tsvector('simple',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                to_tsvector('english',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                to_tsvector('russian',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                now(),
                c.created_at
             FROM memory_claims c
             WHERE c.claim_id = $2
               AND c.forgotten_at IS NULL
               AND c.epistemic_status <> 'retracted'
             ON CONFLICT (source_type, source_id)
             DO UPDATE SET
                tsv_simple = EXCLUDED.tsv_simple,
                tsv_english = EXCLUDED.tsv_english,
                tsv_russian = EXCLUDED.tsv_russian,
                indexed_at = now(),
                source_updated_at = EXCLUDED.source_updated_at",
            &[&entry_id, &claim_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    upsert_healthy_watermark(client, scope, event_id, stream_seq, svc).await
}

/// Upserts a versioned dossier document into `memory_search_index` when the scope index is healthy.
pub(crate) async fn maybe_index_document_in_tx<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    document_id: Uuid,
    event_id: MemoryEventId,
    stream_seq: i64,
    svc: &MemoryService,
) -> Result<(), DennettError> {
    if !is_scope_eligible_for_eager_index(client, scope, stream_seq, svc).await? {
        return Ok(());
    }

    let entry_id = Uuid::new_v4();
    client
        .execute(
            "INSERT INTO memory_search_index (
                entry_id, source_type, source_id, scope,
                tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
             )
             SELECT
                $1, 'document', d.document_id, d.scope,
                to_tsvector('simple', d.document_key || ' ' || d.title || ' ' || d.current_body),
                to_tsvector('english', d.document_key || ' ' || d.title || ' ' || d.current_body),
                to_tsvector('russian', d.document_key || ' ' || d.title || ' ' || d.current_body),
                now(),
                d.updated_at
             FROM memory_documents d
             WHERE d.document_id = $2 AND d.forgotten_at IS NULL
             ON CONFLICT (source_type, source_id)
             DO UPDATE SET
                tsv_simple = EXCLUDED.tsv_simple,
                tsv_english = EXCLUDED.tsv_english,
                tsv_russian = EXCLUDED.tsv_russian,
                indexed_at = now(),
                source_updated_at = EXCLUDED.source_updated_at",
            &[&entry_id, &document_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    upsert_healthy_watermark(client, scope, event_id, stream_seq, svc).await
}

/// Computes the honest live [`IndexState`] for one scope by comparing persisted state against
/// canonical stream head and `memory_search_index` coverage.
pub(crate) async fn compute_scope_index_state<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    svc: &MemoryService,
) -> Result<(IndexState, Option<String>), DennettError> {
    let state_row = client
        .query_opt(
            "SELECT watermark_event_id, watermark_seq, model_version, status,
                    TRUNC(EXTRACT(EPOCH FROM last_rebuild_at) * 1000)::bigint,
                    TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint
             FROM memory_index_state
             WHERE scope = $1",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let (
        watermark_event_id,
        watermark_seq,
        model_version,
        stored_status,
        last_rebuild_at,
        updated_at,
    ) = if let Some(row) = state_row {
        let status: IndexStatus = row.get::<_, String>(3).parse()?;
        (
            row.get::<_, Option<Uuid>>(0).map(MemoryEventId::from_uuid),
            row.get::<_, i64>(1),
            row.get::<_, String>(2),
            status,
            row.get::<_, Option<i64>>(4).map(TimestampMs::from_millis),
            row.get::<_, Option<i64>>(5).map(TimestampMs::from_millis),
        )
    } else {
        (
            None,
            0_i64,
            String::from(SEARCH_MODEL_VERSION),
            IndexStatus::Healthy,
            None,
            None,
        )
    };

    let stream_id = scope_stream_id(scope);
    let head_seq: i64 = client
        .query_opt(
            "SELECT committed_seq FROM stream_counters WHERE stream_id = $1",
            &[&stream_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?
        .map_or(0, |r| r.get(0));

    let counts_row = client
        .query_one(
            "SELECT
                (SELECT COUNT(*)::bigint FROM memory_notes
                 WHERE scope = $1 AND forgotten_at IS NULL AND admission_status <> 'rejected')
                + (SELECT COUNT(*)::bigint FROM memory_claims
                   WHERE scope = $1 AND forgotten_at IS NULL AND epistemic_status <> 'retracted')
                + (SELECT COUNT(*)::bigint FROM memory_documents
                   WHERE scope = $1 AND forgotten_at IS NULL) AS canonical_count,
                (SELECT COUNT(*)::bigint FROM memory_search_index
                 WHERE scope = $1) AS indexed_count",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;
    let canonical_count: i64 = counts_row.get(0);
    let indexed_count: i64 = counts_row.get(1);

    let effective_status = match stored_status {
        IndexStatus::Rebuilding => IndexStatus::Rebuilding,
        IndexStatus::Lagging => {
            if canonical_count > 0 && indexed_count == 0 && watermark_seq > 0 {
                IndexStatus::Degraded
            } else {
                IndexStatus::Lagging
            }
        }
        IndexStatus::Healthy => {
            if canonical_count > 0 && indexed_count == 0 {
                IndexStatus::Degraded
            } else if head_seq > watermark_seq {
                IndexStatus::Lagging
            } else if canonical_count > indexed_count && watermark_seq > 0 {
                IndexStatus::Degraded
            } else {
                IndexStatus::Healthy
            }
        }
        _ => IndexStatus::Degraded,
    };

    let warning = stale_warning_for_status(effective_status);
    let state = IndexState {
        scope: scope.clone(),
        watermark_event_id,
        watermark_seq,
        model_version,
        status: effective_status,
        last_rebuild_at,
        updated_at,
    };
    Ok((state, warning))
}

/// Computes worst-case index health and aggregated watermark metadata across multiple scopes.
pub(crate) async fn compute_aggregate_index_status<C: GenericClient>(
    client: &C,
    scopes: &[String],
    svc: &MemoryService,
) -> Result<(IndexStatus, Option<String>, i64, Option<TimestampMs>), DennettError> {
    let mut worst_status = IndexStatus::Healthy;
    let mut worst_warning: Option<String> = None;
    let mut max_watermark: i64 = 0;
    let mut latest_rebuild: Option<TimestampMs> = None;

    for raw_scope in scopes {
        let scope = MemoryScope::try_new(raw_scope.clone())?;
        let (state, warning) = compute_scope_index_state(client, &scope, svc).await?;
        if status_severity(state.status) > status_severity(worst_status) {
            worst_status = state.status;
            worst_warning = warning;
        }
        if state.watermark_seq > max_watermark {
            max_watermark = state.watermark_seq;
        }
        if let Some(ts) = state.last_rebuild_at {
            latest_rebuild = Some(latest_rebuild.map_or(ts, |prev| prev.max(ts)));
        }
    }

    Ok((worst_status, worst_warning, max_watermark, latest_rebuild))
}

/// Rebuilds all `memory_search_index` entries for `scope` from canonical active tables inside an
/// existing transaction or connection, advancing `memory_index_state` to `healthy`.
#[expect(
    clippy::too_many_lines,
    reason = "atomic index rebuild repopulates notes, claims, and documents from canonical tables and advances watermark"
)]
pub(crate) async fn rebuild_scope_search_index_in_tx<C: GenericClient>(
    client: &C,
    scope: &MemoryScope,
    svc: &MemoryService,
) -> Result<TriggerIndexRebuildResponse, DennettError> {
    client
        .execute(
            "INSERT INTO memory_index_state (scope, watermark_seq, model_version, status, updated_at)
             VALUES ($1, 0, $2, 'rebuilding', now())
             ON CONFLICT (scope)
             DO UPDATE SET status = 'rebuilding', updated_at = now()",
            &[&scope.as_str(), &SEARCH_MODEL_VERSION],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    client
        .execute(
            "DELETE FROM memory_search_index WHERE scope = $1",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let notes_rebuilt = client
        .execute(
            "INSERT INTO memory_search_index (
                entry_id, source_type, source_id, scope,
                tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
             )
             SELECT
                md5('note:' || n.note_id::text)::uuid,
                'note',
                n.note_id,
                n.scope,
                to_tsvector('simple',
                    n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                    || COALESCE(n.topic_key, '') || ' '
                    || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                to_tsvector('english',
                    n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                    || COALESCE(n.topic_key, '') || ' '
                    || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                to_tsvector('russian',
                    n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' '
                    || COALESCE(n.topic_key, '') || ' '
                    || COALESCE((SELECT string_agg(t, ' ') FROM jsonb_array_elements_text(n.tags) AS t), '')),
                now(),
                n.updated_at
             FROM memory_notes n
             WHERE n.scope = $1
               AND n.forgotten_at IS NULL
               AND n.admission_status <> 'rejected'",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let claims_rebuilt = client
        .execute(
            "INSERT INTO memory_search_index (
                entry_id, source_type, source_id, scope,
                tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
             )
             SELECT
                md5('claim:' || c.claim_id::text)::uuid,
                'claim',
                c.claim_id,
                c.scope,
                to_tsvector('simple',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                to_tsvector('english',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                to_tsvector('russian',
                    c.topic_key || ' ' || COALESCE(c.claim_key, '') || ' ' || c.natural_language || ' '
                    || COALESCE((SELECT string_agg(v, ' ') FROM jsonb_array_elements_text(c.caveats) AS v), '')),
                now(),
                c.created_at
             FROM memory_claims c
             WHERE c.scope = $1
               AND c.forgotten_at IS NULL
               AND c.epistemic_status <> 'retracted'",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let docs_rebuilt = client
        .execute(
            "INSERT INTO memory_search_index (
                entry_id, source_type, source_id, scope,
                tsv_simple, tsv_english, tsv_russian, indexed_at, source_updated_at
             )
             SELECT
                md5('document:' || d.document_id::text)::uuid,
                'document',
                d.document_id,
                d.scope,
                to_tsvector('simple', d.document_key || ' ' || d.title || ' ' || d.current_body),
                to_tsvector('english', d.document_key || ' ' || d.title || ' ' || d.current_body),
                to_tsvector('russian', d.document_key || ' ' || d.title || ' ' || d.current_body),
                now(),
                d.updated_at
             FROM memory_documents d
             WHERE d.scope = $1
               AND d.forgotten_at IS NULL",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let latest_ev_row = client
        .query_opt(
            "SELECT event_id, stream_seq
             FROM memory_events
             WHERE scope = $1
             ORDER BY stream_seq DESC
             LIMIT 1",
            &[&scope.as_str()],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let stream_id = scope_stream_id(scope);
    let counter_seq: i64 = client
        .query_opt(
            "SELECT committed_seq FROM stream_counters WHERE stream_id = $1",
            &[&stream_id],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?
        .map_or(0, |r| r.get(0));

    let watermark_event_uuid: Option<Uuid> = latest_ev_row.as_ref().map(|r| r.get(0));
    let event_seq: i64 = latest_ev_row.as_ref().map_or(0, |r| r.get(1));
    let watermark_seq = counter_seq.max(event_seq);

    let state_row = client
        .query_one(
            "INSERT INTO memory_index_state (
                scope, watermark_event_id, watermark_seq, model_version, status, last_rebuild_at, updated_at
             ) VALUES ($1, $2, $3, $4, 'healthy', now(), now())
             ON CONFLICT (scope)
             DO UPDATE SET
                watermark_event_id = EXCLUDED.watermark_event_id,
                watermark_seq = EXCLUDED.watermark_seq,
                model_version = EXCLUDED.model_version,
                status = 'healthy',
                last_rebuild_at = now(),
                updated_at = now()
             RETURNING watermark_event_id, watermark_seq, model_version, status,
                       TRUNC(EXTRACT(EPOCH FROM last_rebuild_at) * 1000)::bigint,
                       TRUNC(EXTRACT(EPOCH FROM updated_at) * 1000)::bigint",
            &[
                &scope.as_str(),
                &watermark_event_uuid,
                &watermark_seq,
                &SEARCH_MODEL_VERSION,
            ],
        )
        .await
        .map_err(|err| svc.map_pg_err(&err))?;

    let entries_rebuilt = notes_rebuilt
        .saturating_add(claims_rebuilt)
        .saturating_add(docs_rebuilt);

    Ok(TriggerIndexRebuildResponse {
        index_state: IndexState {
            scope: scope.clone(),
            watermark_event_id: state_row
                .get::<_, Option<Uuid>>(0)
                .map(MemoryEventId::from_uuid),
            watermark_seq: state_row.get(1),
            model_version: state_row.get(2),
            status: state_row.get::<_, String>(3).parse()?,
            last_rebuild_at: state_row
                .get::<_, Option<i64>>(4)
                .map(TimestampMs::from_millis),
            updated_at: state_row
                .get::<_, Option<i64>>(5)
                .map(TimestampMs::from_millis),
        },
        entries_rebuilt,
    })
}

/// Executes scoped multi-language FTS + fresh-tail overlay search over active [`MemoryNote`]s.
pub(crate) async fn execute_search_notes(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    req: SearchNotesRequest,
) -> Result<SearchNotesResponse, DennettError> {
    let limit = req.validated_limit()?;
    let allowed_scopes = resolve_search_scopes(principal, req.scope.as_ref())?;
    if allowed_scopes.is_empty() {
        return Ok(SearchNotesResponse {
            notes: Vec::new(),
            claims: Vec::new(),
            documents: Vec::new(),
            total_matched: 0,
            truncated: false,
            index_status: IndexStatus::Healthy,
            stale_warning: None,
        });
    }

    svc.ensure_migrated().await?;
    let prepared = req.query.as_deref().and_then(prepare_search_query);
    let limit_i64 = i64::from(limit);
    let include_history = req.include_history;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let (index_status, stale_warning, _, _) =
            compute_aggregate_index_status(&conn, &allowed_scopes, svc).await?;

        let rows = query_scoped_notes_v2(
            &conn,
            &allowed_scopes,
            prepared.as_ref(),
            include_history,
            limit_i64,
            svc,
        )
        .await?;

        let total_matched: u64 = rows
            .first()
            .map_or(0, |r| u64::try_from(r.get::<_, i64>(18)).unwrap_or(0));

        let event_uuids: Vec<Uuid> = rows.iter().map(|r| r.get::<_, Uuid>(8)).collect();
        let mut links_by_event = fetch_links_for_events(&conn, &event_uuids, svc).await?;

        let mut notes = Vec::with_capacity(rows.len());
        for row in rows {
            let event_uuid: Uuid = row.get(8);
            let links = links_by_event.remove(&event_uuid).unwrap_or_default();
            let note = decode_note_row(&row, links)?;
            // Defense-in-depth post-filter verification (Architecture 81 §5).
            require_scope_capability(principal, &note.scope, MemoryCapability::Read)?;
            notes.push(note);
        }

        let returned_count = u64::try_from(notes.len()).unwrap_or(u64::MAX);
        Ok(SearchNotesResponse {
            notes,
            claims: Vec::new(),
            documents: Vec::new(),
            total_matched,
            truncated: total_matched > returned_count,
            index_status,
            stale_warning,
        })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::search_notes"),
        })?
}

#[expect(
    clippy::too_many_lines,
    reason = "combined indexed FTS + canonical fresh-tail + optional history SQL query keeps ranking and filtering unified"
)]
async fn query_scoped_notes_v2<C: GenericClient>(
    client: &C,
    allowed_scopes: &[String],
    prepared: Option<&PreparedSearchQuery>,
    include_history: bool,
    limit_i64: i64,
    svc: &MemoryService,
) -> Result<Vec<Row>, DennettError> {
    if let Some(q) = prepared {
        client
            .query(
                "SELECT n.note_id, n.scope, n.current_revision, n.current_revision_id,
                        n.title, n.body, n.tags, n.rationale, n.last_event_id,
                        n.recorded_by, n.author_label,
                        TRUNC(EXTRACT(EPOCH FROM n.created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM n.updated_at) * 1000)::bigint,
                        n.admission_status, n.salience, n.topic_key,
                        n.trust_level, n.instruction_active,
                        COUNT(*) OVER()::bigint AS total_matched
                 FROM memory_notes n
                 LEFT JOIN memory_search_index idx
                   ON idx.source_type = 'note'
                  AND idx.source_id = n.note_id
                  AND idx.scope = n.scope
                 WHERE n.scope = ANY($1::text[])
                   AND n.forgotten_at IS NULL
                   AND n.admission_status <> 'rejected'
                   AND (
                       (idx.entry_id IS NOT NULL AND idx.source_updated_at >= n.updated_at AND (
                           idx.tsv_simple @@ plainto_tsquery('simple', $2)
                           OR idx.tsv_english @@ plainto_tsquery('english', $2)
                           OR idx.tsv_russian @@ plainto_tsquery('russian', $2)
                           OR n.title ILIKE $3 ESCAPE '\\'
                           OR n.body ILIKE $3 ESCAPE '\\'
                           OR COALESCE(n.rationale, '') ILIKE $3 ESCAPE '\\'
                           OR COALESCE(n.topic_key, '') ILIKE $3 ESCAPE '\\'
                           OR EXISTS (
                               SELECT 1
                               FROM jsonb_array_elements_text(n.tags) AS tag_val
                               WHERE tag_val ILIKE $3 ESCAPE '\\'
                           )
                       ))
                       OR
                       ((idx.entry_id IS NULL OR idx.source_updated_at < n.updated_at) AND (
                           to_tsvector('simple',
                               n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' ' || COALESCE(n.topic_key, ''))
                               @@ plainto_tsquery('simple', $2)
                           OR to_tsvector('english',
                               n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' ' || COALESCE(n.topic_key, ''))
                               @@ plainto_tsquery('english', $2)
                           OR to_tsvector('russian',
                               n.title || ' ' || n.body || ' ' || COALESCE(n.rationale, '') || ' ' || COALESCE(n.topic_key, ''))
                               @@ plainto_tsquery('russian', $2)
                           OR n.title ILIKE $3 ESCAPE '\\'
                           OR n.body ILIKE $3 ESCAPE '\\'
                           OR COALESCE(n.rationale, '') ILIKE $3 ESCAPE '\\'
                           OR COALESCE(n.topic_key, '') ILIKE $3 ESCAPE '\\'
                           OR EXISTS (
                               SELECT 1
                               FROM jsonb_array_elements_text(n.tags) AS tag_val
                               WHERE tag_val ILIKE $3 ESCAPE '\\'
                           )
                       ))
                       OR
                       ($4::boolean = true AND EXISTS (
                           SELECT 1
                           FROM memory_note_revisions r
                           WHERE r.note_id = n.note_id
                             AND (
                                 to_tsvector('simple', r.title || ' ' || r.body || ' ' || COALESCE(r.rationale, ''))
                                     @@ plainto_tsquery('simple', $2)
                                 OR to_tsvector('english', r.title || ' ' || r.body || ' ' || COALESCE(r.rationale, ''))
                                     @@ plainto_tsquery('english', $2)
                                 OR to_tsvector('russian', r.title || ' ' || r.body || ' ' || COALESCE(r.rationale, ''))
                                     @@ plainto_tsquery('russian', $2)
                                 OR r.title ILIKE $3 ESCAPE '\\'
                                 OR r.body ILIKE $3 ESCAPE '\\'
                                 OR COALESCE(r.rationale, '') ILIKE $3 ESCAPE '\\'
                             )
                       ))
                   )
                 ORDER BY n.updated_at DESC, n.note_id ASC
                 LIMIT $5",
                &[
                    &allowed_scopes,
                    &q.plain_text,
                    &q.like_pattern,
                    &include_history,
                    &limit_i64,
                ],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))
    } else {
        client
            .query(
                "SELECT n.note_id, n.scope, n.current_revision, n.current_revision_id,
                        n.title, n.body, n.tags, n.rationale, n.last_event_id,
                        n.recorded_by, n.author_label,
                        TRUNC(EXTRACT(EPOCH FROM n.created_at) * 1000)::bigint,
                        TRUNC(EXTRACT(EPOCH FROM n.updated_at) * 1000)::bigint,
                        n.admission_status, n.salience, n.topic_key,
                        n.trust_level, n.instruction_active,
                        COUNT(*) OVER()::bigint AS total_matched
                 FROM memory_notes n
                 WHERE n.scope = ANY($1::text[])
                   AND n.forgotten_at IS NULL
                   AND n.admission_status <> 'rejected'
                 ORDER BY n.updated_at DESC, n.note_id ASC
                 LIMIT $2",
                &[&allowed_scopes, &limit_i64],
            )
            .await
            .map_err(|err| svc.map_pg_err(&err))
    }
}

/// Fetches the live effective [`IndexState`] for one authorized scope (or worst-case authorized scope).
pub(crate) async fn execute_get_index_status(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    scope: Option<&MemoryScope>,
) -> Result<IndexState, DennettError> {
    let allowed_scopes = resolve_search_scopes(principal, scope)?;
    if allowed_scopes.is_empty() {
        return Err(DennettError::ForbiddenScope {
            scope: String::from("*"),
            reason: String::from("principal has no authorized scopes"),
        });
    }

    svc.ensure_migrated().await?;

    let op_fut = async {
        let conn = svc.get_conn(svc.op_timeout()).await?;
        let mut selected: Option<IndexState> = None;
        for raw_scope in &allowed_scopes {
            let sc = MemoryScope::try_new(raw_scope.clone())?;
            let (state, _) = compute_scope_index_state(&conn, &sc, svc).await?;
            let replace = selected
                .as_ref()
                .is_none_or(|cur| status_severity(state.status) > status_severity(cur.status));
            if replace {
                selected = Some(state);
            }
        }
        selected.ok_or_else(|| DennettError::ForbiddenScope {
            scope: String::from("*"),
            reason: String::from("no authorized scopes available"),
        })
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::get_index_status"),
        })?
}

/// Rebuilds the separable search index for one scope (`MemoryCapability::Admin`).
pub(crate) async fn execute_trigger_index_rebuild(
    svc: &MemoryService,
    principal: &AuthPrincipal,
    req: TriggerIndexRebuildRequest,
) -> Result<TriggerIndexRebuildResponse, DennettError> {
    require_scope_capability(principal, &req.scope, MemoryCapability::Admin)?;
    svc.ensure_migrated().await?;

    let op_fut = async {
        let mut conn = svc.get_conn(svc.op_timeout()).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| svc.map_pg_err(&err))?;
        let resp = rebuild_scope_search_index_in_tx(&tx, &req.scope, svc).await?;
        tx.commit().await.map_err(|err| svc.map_pg_err(&err))?;
        Ok(resp)
    };

    time::timeout(svc.op_timeout(), op_fut)
        .await
        .map_err(|_| DennettError::Timeout {
            operation: String::from("memory_service::trigger_index_rebuild"),
        })?
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn prepare_search_query_escapes_wildcards_and_strips_control_chars() {
        assert_eq!(prepare_search_query("   \0\x07  "), None);
        let prepared = prepare_search_query("100%_done\\ok ' OR 1=1;--").expect("non-empty query");
        assert_eq!(prepared.plain_text, "100%_done\\ok ' OR 1=1;--");
        assert_eq!(prepared.like_pattern, "%100\\%\\_done\\\\ok ' OR 1=1;--%");
    }

    proptest! {
        #[test]
        fn proptest_search_query_never_panics_and_strips_nul(raw in "\\PC{0,512}") {
            if let Some(prepared) = prepare_search_query(&raw) {
                prop_assert!(!prepared.plain_text.is_empty());
                prop_assert!(!prepared.plain_text.contains('\0'));
                prop_assert!(prepared.like_pattern.starts_with('%'));
                prop_assert!(prepared.like_pattern.ends_with('%'));
                prop_assert!(!prepared.like_pattern.contains('\0'));
            }
        }
    }
}
