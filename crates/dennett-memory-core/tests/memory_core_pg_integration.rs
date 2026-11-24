//! Real PostgreSQL integration tests for `dennett-memory-core` (migrations, SQL constraints,
//! V19 multi-anchor/NULL ordinal links, idempotency races, optimistic locking).
//!
//! Requires `DENNETT_TEST_PROFILE=1` and `DENNETT_TEST_MEMORY_PG_URL`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;

use dennett_contracts::{
    AuthPrincipal, CorrectNoteRequest, CorrelationId, CreateNoteRequest, DennettError,
    EvidenceAnchor, EvidenceLinkInput, EvidenceRelationship, IdempotencyKey, IngestEvidenceRequest,
    MemoryCapability, MemoryScope, PrincipalId, ProfileMode,
};
use dennett_memory_core::{
    LATEST_SCHEMA_VERSION, MemoryService, ValidatedPgTarget, query_schema_version, run_migrations,
};
use tokio::task::JoinSet;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn require_test_pg_target() -> ValidatedPgTarget {
    assert_eq!(
        env::var("DENNETT_TEST_PROFILE").ok().as_deref(),
        Some("1"),
        "DENNETT_TEST_PROFILE=1 is required for PG integration tests"
    );
    let url = env::var("DENNETT_TEST_MEMORY_PG_URL")
        .expect("DENNETT_TEST_MEMORY_PG_URL must be set via `dennett-check integration`");
    ValidatedPgTarget::parse(&url, ProfileMode::TestIsolated)
        .expect("DENNETT_TEST_MEMORY_PG_URL must be a valid *_test DSN")
}

fn test_principal(name: &str, scopes: &[MemoryScope]) -> AuthPrincipal {
    AuthPrincipal {
        principal_id: PrincipalId::try_new(name).expect("valid principal"),
        issuer: String::from("dennett-standalone"),
        token_id: format!("tok-{name}"),
        allowed_scopes: scopes.iter().cloned().collect(),
        capabilities: BTreeSet::from([MemoryCapability::Read, MemoryCapability::Write]),
        expires_at: None,
    }
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    MemoryScope::try_new(format!("{prefix}-{suffix}")).expect("valid unique scope")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test verifies migration idempotency, SQL CHECK constraints, append-only triggers, and tamper guards on one disposable schema"
)]
async fn migration_is_idempotent_checks_tampering_and_enforces_sql_constraints() {
    let target = require_test_pg_target();
    let (mut client, connection) = target
        .config()
        .connect(NoTls)
        .await
        .expect("connect to test postgres");
    let conn_task = tokio::spawn(async move {
        let _ = connection.await;
    });

    // Use a disposable schema so tampering / constraint checks never pollute `public`.
    let suffix = &Uuid::new_v4().simple().to_string()[..12];
    let schema = format!("d1_mig_probe_{suffix}");
    client
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; SET search_path TO {schema};"
        ))
        .await
        .expect("create disposable schema");

    async {
        // Before migration, read-only schema check returns None.
        let before = query_schema_version(&client, Some(target.raw_url()), target.parsed_password())
            .await
            .expect("query schema version before migration");
        assert_eq!(before, None);

        // First migration applies v1 cleanly.
        let v1 = run_migrations(&mut client, Some(target.raw_url()), target.parsed_password())
            .await
            .expect("first migration pass");
        assert_eq!(v1, LATEST_SCHEMA_VERSION);

        // Second migration pass is strictly idempotent and leaves exactly 1 row.
        let v2 = run_migrations(&mut client, Some(target.raw_url()), target.parsed_password())
            .await
            .expect("second idempotent migration pass");
        assert_eq!(v2, LATEST_SCHEMA_VERSION);

        let count: i64 = client
            .query_one("SELECT COUNT(*)::bigint FROM memory_schema_migrations", &[])
            .await
            .expect("count migrations")
            .get(0);
        assert_eq!(count, 1, "idempotent migration must not duplicate migration rows");

        let after = query_schema_version(&client, Some(target.raw_url()), target.parsed_password())
            .await
            .expect("query schema version after migration");
        assert_eq!(after, Some(LATEST_SCHEMA_VERSION));

        // Direct SQL constraint check: negative ordinal in event_evidence_links MUST fail.
        let ev_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let zero_sha = "0".repeat(64);
        client
            .execute(
                "INSERT INTO evidence_objects (evidence_id, scope, sha256, media_type, byte_len, content, ingested_by)
                 VALUES ($1, 'sql-check', $2, 'text/plain', 3, 'abc', 'agent:sql')",
                &[&ev_id, &zero_sha],
            )
            .await
            .expect("insert evidence fixture");
        client
            .execute(
                "INSERT INTO memory_events (event_id, stream_id, stream_seq, scope, event_kind, principal_id, correlation_id, idempotency_key, payload_fingerprint, target_entity_id)
                 VALUES ($1, 'memory:scope:sql-check', 1, 'sql-check', 'evidence_ingested', 'agent:sql', 'cid-1', 'idem-1', $2, $3)",
                &[&event_id, &zero_sha, &ev_id],
            )
            .await
            .expect("insert event fixture");

        let neg_ord = client
            .execute(
                "INSERT INTO event_evidence_links (link_id, event_id, evidence_id, relationship, ordinal, revision)
                 VALUES ($1, $2, $3, 'supports', -1, 1)",
                &[&Uuid::new_v4(), &event_id, &ev_id],
            )
            .await;
        assert!(
            neg_ord.is_err(),
            "SQL CHECK constraint must reject negative ordinal"
        );

        // Append-only trigger check: UPDATE or DELETE on memory_events MUST fail.
        let update_event = client
            .execute(
                "UPDATE memory_events SET scope = 'tampered' WHERE event_id = $1",
                &[&event_id],
            )
            .await;
        assert!(
            update_event.is_err(),
            "append-only trigger must forbid UPDATE on memory_events"
        );
        let delete_event = client
            .execute("DELETE FROM memory_events WHERE event_id = $1", &[&event_id])
            .await;
        assert!(
            delete_event.is_err(),
            "append-only trigger must forbid DELETE on memory_events"
        );

        // Checksum tampering check: modifying checksum causes run_migrations to fail closed.
        let fake_sha = "f".repeat(64);
        client
            .execute(
                "UPDATE memory_schema_migrations SET checksum = $1 WHERE version = 1",
                &[&fake_sha],
            )
            .await
            .expect("tamper checksum");
        let tampered =
            run_migrations(&mut client, Some(target.raw_url()), target.parsed_password()).await;
        assert!(
            matches!(tampered, Err(DennettError::SafetyGuardRejected { .. })),
            "tampered migration checksum must fail closed, got: {tampered:?}"
        );

        // Future schema version check: binary must refuse to run against newer schema version.
        client
            .execute(
                "INSERT INTO memory_schema_migrations (version, name, checksum) VALUES (99, 'future_v99', $1)",
                &[&fake_sha],
            )
            .await
            .expect("insert future version");
        let future =
            run_migrations(&mut client, Some(target.raw_url()), target.parsed_password()).await;
        assert!(
            matches!(future, Err(DennettError::SafetyGuardRejected { .. })),
            "future schema version must fail closed, got: {future:?}"
        );
    }
    .await;

    let _ = client
        .batch_execute(&format!(
            "RESET search_path; DROP SCHEMA IF EXISTS {schema} CASCADE;"
        ))
        .await;
    drop(client);
    conn_task.abort();
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test builds full multi-anchor V19 fixture and asserts event, link, and cross-scope denial invariants"
)]
async fn v19_multiple_anchors_null_ordinal_and_cross_scope_link_denial() {
    let target = require_test_pg_target();
    let service = MemoryService::new(target).expect("build MemoryService");

    let scope_a = unique_scope("v19-alpha");
    let scope_b = unique_scope("v19-beta");
    let principal = test_principal("agent:v19", &[scope_a.clone(), scope_b.clone()]);
    let cid = CorrelationId::generate();

    let ev_a = service
        .ingest_evidence(
            &principal,
            &cid,
            IngestEvidenceRequest {
                scope: scope_a.clone(),
                media_type: String::from("text/markdown"),
                content: String::from("Line 1\nDecision A\nLine 3\nQuote B\n"),
                source_uri: Some(String::from("docs/design.md")),
                observed_at: None,
                metadata: BTreeMap::from([(String::from("source"), String::from("v19-test"))]),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("ingest evidence in scope A");

    let ev_b = service
        .ingest_evidence(
            &principal,
            &cid,
            IngestEvidenceRequest {
                scope: scope_b.clone(),
                media_type: String::from("text/plain"),
                content: String::from("Foreign scope evidence"),
                source_uri: None,
                observed_at: None,
                metadata: BTreeMap::new(),
                idempotency_key: IdempotencyKey::generate(),
            },
        )
        .await
        .expect("ingest evidence in scope B");

    let supports = EvidenceRelationship::try_new("supports").expect("valid relationship");

    // V19: 3 links to the SAME evidence_id with the SAME relationship, two with ordinal=None and
    // distinct anchors, plus one with ordinal=Some(0).
    let created = service
        .create_note(
            &principal,
            &cid,
            CreateNoteRequest {
                scope: scope_a.clone(),
                title: String::from("Multi-anchor V19 note"),
                body: String::from("Links three anchors from one source"),
                tags: vec![String::from("v19")],
                observed_at: None,
                evidence_links: vec![
                    EvidenceLinkInput {
                        evidence_id: ev_a.evidence.evidence_id,
                        relationship: supports.clone(),
                        anchor: Some(EvidenceAnchor {
                            kind: String::from("line_range"),
                            start: Some(1),
                            end: Some(2),
                            locator: Some(String::from("Decision A")),
                            attributes: BTreeMap::new(),
                        }),
                        ordinal: None,
                    },
                    EvidenceLinkInput {
                        evidence_id: ev_a.evidence.evidence_id,
                        relationship: supports.clone(),
                        anchor: Some(EvidenceAnchor {
                            kind: String::from("line_range"),
                            start: Some(3),
                            end: Some(4),
                            locator: Some(String::from("Quote B")),
                            attributes: BTreeMap::new(),
                        }),
                        ordinal: None,
                    },
                    EvidenceLinkInput {
                        evidence_id: ev_a.evidence.evidence_id,
                        relationship: supports.clone(),
                        anchor: Some(EvidenceAnchor {
                            kind: String::from("git_fragment"),
                            start: None,
                            end: None,
                            locator: Some(String::from("HEAD:docs/design.md")),
                            attributes: BTreeMap::from([(
                                String::from("commit"),
                                String::from("26c8c4e"),
                            )]),
                        }),
                        ordinal: Some(0),
                    },
                ],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
        )
        .await
        .expect("create multi-anchor note");

    assert_eq!(created.note.evidence_links.len(), 3);
    let unique_link_ids: BTreeSet<_> = created
        .note
        .evidence_links
        .iter()
        .map(|l| l.link_id)
        .collect();
    assert_eq!(
        unique_link_ids.len(),
        3,
        "each anchor must receive a unique link_id"
    );

    let null_ordinals = created
        .note
        .evidence_links
        .iter()
        .filter(|l| l.ordinal.is_none())
        .count();
    assert_eq!(null_ordinals, 2);

    let event = service
        .get_event(&principal, created.event_id)
        .await
        .expect("fetch note creation event");
    assert_eq!(event.evidence_links.len(), 3);
    assert_eq!(
        event.stream_seq, 2,
        "stream_counters must increment monotonically (1=evidence, 2=note)"
    );

    // Cross-scope evidence linking MUST be rejected even if the principal can access both scopes.
    let cross_scope = service
        .create_note(
            &principal,
            &cid,
            CreateNoteRequest {
                scope: scope_a,
                title: String::from("Cross-scope link attempt"),
                body: String::from("Should fail"),
                tags: vec![],
                observed_at: None,
                evidence_links: vec![EvidenceLinkInput {
                    evidence_id: ev_b.evidence.evidence_id,
                    relationship: supports,
                    anchor: None,
                    ordinal: None,
                }],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
        )
        .await;
    assert!(
        matches!(cross_scope, Err(DennettError::ForbiddenScope { .. })),
        "linking foreign-scope evidence must fail with ForbiddenScope, got: {cross_scope:?}"
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "integration test runs concurrent idempotency and optimistic lock races sequentially on one note"
)]
async fn idempotency_replay_mismatch_and_concurrent_optimistic_lock_races() {
    let target = require_test_pg_target();
    let service = MemoryService::new(target).expect("build MemoryService");

    let scope = unique_scope("race");
    let principal = test_principal("agent:race", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    // 1. Concurrent idempotency race: 6 simultaneous identical `create_note` calls with same key.
    let shared_key = IdempotencyKey::generate();
    let create_req = CreateNoteRequest {
        scope: scope.clone(),
        title: String::from("Concurrent create"),
        body: String::from("Only one note may be created"),
        tags: vec![String::from("race")],
        observed_at: None,
        evidence_links: vec![],
        author_label: None,
        idempotency_key: shared_key.clone(),
        admission_status: None,
        salience: None,
        topic_key: None,
    };

    let mut create_tasks = JoinSet::new();
    for _ in 0..6 {
        let svc = service.clone();
        let p = principal.clone();
        let c = cid.clone();
        let r = create_req.clone();
        create_tasks.spawn(async move { svc.create_note(&p, &c, r).await });
    }

    let mut created_note_ids = BTreeSet::new();
    let mut created_event_ids = BTreeSet::new();
    let mut fresh_count = 0_usize;
    let mut replay_count = 0_usize;
    while let Some(joined) = create_tasks.join_next().await {
        let resp = joined
            .expect("join create task")
            .expect("concurrent idempotent create must succeed");
        created_note_ids.insert(resp.note.note_id);
        created_event_ids.insert(resp.event_id);
        if resp.idempotent_replay {
            replay_count += 1;
        } else {
            fresh_count += 1;
        }
    }
    assert_eq!(created_note_ids.len(), 1, "must create exactly one note");
    assert_eq!(created_event_ids.len(), 1, "must emit exactly one event");
    assert_eq!(fresh_count, 1, "exactly one caller wins initial insert");
    assert_eq!(
        replay_count, 5,
        "remaining five callers receive idempotent replay"
    );

    let note_id = *created_note_ids.iter().next().expect("note id present");

    // 2. Reusing same idempotency_key with different body -> IdempotencyMismatch.
    let mismatch = service
        .create_note(
            &principal,
            &cid,
            CreateNoteRequest {
                body: String::from("DIFFERENT PAYLOAD"),
                ..create_req
            },
        )
        .await;
    assert!(
        matches!(mismatch, Err(DennettError::IdempotencyMismatch { .. })),
        "same key with different payload must return IdempotencyMismatch, got: {mismatch:?}"
    );

    // 3. Concurrent optimistic lock race: 6 simultaneous corrections with DIFFERENT keys expecting rev 1.
    let mut corr_tasks = JoinSet::new();
    for idx in 0..6_u32 {
        let svc = service.clone();
        let p = principal.clone();
        let c = cid.clone();
        let sc = scope.clone();
        corr_tasks.spawn(async move {
            let req = CorrectNoteRequest {
                note_id,
                scope: sc,
                expected_revision: 1,
                title: format!("Correction #{idx}"),
                body: format!("Winner candidate {idx}"),
                tags: vec![String::from("corrected")],
                rationale: format!("Race attempt {idx}"),
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
            };
            (req.clone(), svc.correct_note(&p, &c, req).await)
        });
    }

    let mut winner_req: Option<CorrectNoteRequest> = None;
    let mut winner_event_id = None;
    let mut conflict_count = 0_usize;

    while let Some(joined) = corr_tasks.join_next().await {
        let (req, res) = joined.expect("join correction task");
        match res {
            Ok(resp) => {
                assert_eq!(resp.note.current_revision, 2);
                assert!(!resp.idempotent_replay);
                winner_req = Some(req);
                winner_event_id = Some(resp.event_id);
            }
            Err(DennettError::Conflict {
                expected_revision: Some(1),
                actual_revision: Some(2),
                ..
            }) => {
                conflict_count += 1;
            }
            other => panic!("unexpected result in optimistic lock race: {other:?}"),
        }
    }

    assert_eq!(
        conflict_count, 5,
        "exactly 5 of 6 concurrent corrections must conflict"
    );
    let winner_req = winner_req.expect("exactly one correction must win");

    // 4. Replaying the winning correction with its original expected_revision=1 & idempotency_key
    //    MUST succeed as idempotent_replay=true even though current_revision is now 2!
    let replayed = service
        .correct_note(&principal, &cid, winner_req)
        .await
        .expect("replaying winning correction must succeed idempotently");
    assert!(replayed.idempotent_replay);
    assert_eq!(replayed.note.current_revision, 2);
    assert_eq!(Some(replayed.event_id), winner_event_id);

    // 5. Concurrent same-idempotency_key `correct_note` race (Defect 1 regression):
    //    6 simultaneous identical corrections expecting rev 2 with the SAME idempotency key
    //    must ALL succeed (1 fresh rev 3 + 5 idempotent replays), ZERO spurious 409 Conflicts!
    let shared_corr_key = IdempotencyKey::generate();
    let shared_corr_req = CorrectNoteRequest {
        note_id,
        scope: scope.clone(),
        expected_revision: 2,
        title: String::from("Concurrent idempotent rev 3"),
        body: String::from("Retried while FOR UPDATE lock is held"),
        tags: vec![String::from("idempotent-race")],
        rationale: String::from("Prove FOR UPDATE re-checks idempotency"),
        evidence_links: vec![],
        author_label: None,
        idempotency_key: shared_corr_key,
    };
    let mut same_key_corr_tasks = JoinSet::new();
    for _ in 0..6 {
        let svc = service.clone();
        let p = principal.clone();
        let c = cid.clone();
        let r = shared_corr_req.clone();
        same_key_corr_tasks.spawn(async move { svc.correct_note(&p, &c, r).await });
    }
    let mut rev3_event_ids = BTreeSet::new();
    let mut rev3_fresh = 0_usize;
    let mut rev3_replays = 0_usize;
    while let Some(joined) = same_key_corr_tasks.join_next().await {
        let resp = joined
            .expect("join concurrent idempotent correction")
            .expect("concurrent same-key correct_note must never fail with 409 Conflict");
        assert_eq!(resp.note.current_revision, 3);
        rev3_event_ids.insert(resp.event_id);
        if resp.idempotent_replay {
            rev3_replays += 1;
        } else {
            rev3_fresh += 1;
        }
    }
    assert_eq!(
        rev3_event_ids.len(),
        1,
        "must commit exactly one rev 3 event"
    );
    assert_eq!(
        rev3_fresh, 1,
        "exactly one concurrent correction wins insert"
    );
    assert_eq!(rev3_replays, 5, "remaining five receive idempotent replay");

    // 6. History must hold exactly revisions 1, 2, and 3 in order.
    let history = service
        .get_note_history(&principal, note_id)
        .await
        .expect("fetch note history");
    assert_eq!(history.current_revision, 3);
    assert_eq!(history.revisions.len(), 3);
    assert_eq!(history.revisions[0].revision, 1);
    assert_eq!(history.revisions[1].revision, 2);
    assert_eq!(history.revisions[2].revision, 3);
}

#[tokio::test]
async fn nul_bytes_and_extreme_timestamps_return_invalid_input_and_valid_bounds_persist() {
    let target = require_test_pg_target();
    let service = MemoryService::new(target).expect("build MemoryService");

    let scope = unique_scope("bounds");
    let principal = test_principal("agent:bounds", std::slice::from_ref(&scope));
    let cid = CorrelationId::generate();

    // NUL byte in note body must return InvalidInput, never DatabaseUnavailable.
    let nul_note = service
        .create_note(
            &principal,
            &cid,
            CreateNoteRequest {
                scope: scope.clone(),
                title: String::from("ok"),
                body: String::from("contains\0nul"),
                tags: vec![],
                observed_at: None,
                evidence_links: vec![],
                author_label: None,
                idempotency_key: IdempotencyKey::generate(),
                admission_status: None,
                salience: None,
                topic_key: None,
            },
        )
        .await;
    assert!(
        matches!(nul_note, Err(DennettError::InvalidInput { .. })),
        "NUL byte must fail with InvalidInput, got: {nul_note:?}"
    );

    // Extreme observed_at = i64::MAX / i64::MIN must return InvalidInput, never DatabaseUnavailable.
    for extreme in [i64::MIN, i64::MAX] {
        let res = service
            .ingest_evidence(
                &principal,
                &cid,
                IngestEvidenceRequest {
                    scope: scope.clone(),
                    media_type: String::from("text/plain"),
                    content: String::from("extreme ts"),
                    source_uri: None,
                    observed_at: Some(dennett_contracts::TimestampMs::from_millis(extreme)),
                    metadata: BTreeMap::new(),
                    idempotency_key: IdempotencyKey::generate(),
                },
            )
            .await;
        assert!(
            matches!(res, Err(DennettError::InvalidInput { .. })),
            "extreme timestamp {extreme} must fail with InvalidInput, got: {res:?}"
        );
    }

    // Valid canonical boundary timestamps (MIN_TIMESTAMP_MS and MAX_TIMESTAMP_MS) persist cleanly in PG!
    for bound in [
        dennett_contracts::MIN_TIMESTAMP_MS,
        dennett_contracts::MAX_TIMESTAMP_MS,
    ] {
        let ev = service
            .ingest_evidence(
                &principal,
                &cid,
                IngestEvidenceRequest {
                    scope: scope.clone(),
                    media_type: String::from("text/plain"),
                    content: format!("boundary {bound}"),
                    source_uri: None,
                    observed_at: Some(dennett_contracts::TimestampMs::from_millis(bound)),
                    metadata: BTreeMap::new(),
                    idempotency_key: IdempotencyKey::generate(),
                },
            )
            .await
            .expect("valid boundary timestamp must persist in PostgreSQL");
        assert_eq!(
            ev.evidence
                .observed_at
                .map(dennett_contracts::TimestampMs::as_millis),
            Some(bound)
        );
    }
}
