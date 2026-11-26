//! Real PostgreSQL 16 concurrency, journal visibility (`V15`/`V18`), per-key ordered outbox
//! (`V16`/`V17`), direct generation fencing (`V07`), and auth-profile isolation (`A10`)
//! integration tests for `dennett-head`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    ArtifactCompleteness, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    ArtifactLifecycleState, BudgetExceedPolicy, CapabilityGrantId, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreateProjectRequest, CreateSessionRequest,
    CreateTaskRequest, DennettError, EffectClaimId, IdempotencyKey, InboxCardId, InboxCardKind,
    MemoryScope, MessageId, MissedRunPolicy, PermissionRequestId, PermissionRequestStatus,
    PermissionResolutionDecision, ProfileMode, ProjectId, ResolvePermissionRequest, RunId,
    RunStatus, RuntimeAuthProfile, ScheduleKind, SendMessageRequest, SessionKind, TaskBudget,
    TaskSchedule, TaskSignalOutcome, TaskSignalRequest, TaskStatus, TimestampMs, UpdateTaskRequest,
    WorkspaceWritePolicy,
};
use dennett_head::{
    ControlEventAppend, HeadService, HeadServiceExtras, ValidatedControlPgTarget,
    append_control_event_in_tx,
};
use dennett_runtime_antigravity::{SimulatedRuntime, TurnOutcome};
use serde_json::json;
use tokio::sync::Barrier;
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;

fn require_control_pg_target() -> (String, ValidatedControlPgTarget) {
    assert_eq!(
        env::var("DENNETT_TEST_PROFILE").ok().as_deref(),
        Some("1"),
        "DENNETT_TEST_PROFILE=1 is required for Head concurrency integration tests"
    );
    let url = env::var("DENNETT_TEST_CONTROL_PG_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .expect("DENNETT_TEST_CONTROL_PG_URL is required");
    let target = ValidatedControlPgTarget::resolve_from_sources(
        Some(&url),
        Some(&url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve test PG target")
    .expect("test PG target present");
    (url, target)
}

fn make_test_pool(target: &ValidatedControlPgTarget) -> Pool {
    let mgr = Manager::from_config(
        target.config().clone(),
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    Pool::builder(mgr)
        .max_size(8)
        .build()
        .expect("build deadpool test pool")
}

async fn make_isolated_schema_head_service(
    prefix: &str,
) -> (
    String,
    ValidatedControlPgTarget,
    HeadService,
    String,
    String,
) {
    let (base_url, _base_target) = require_control_pg_target();
    let short = &Uuid::new_v4().simple().to_string()[..12];
    let schema_name = format!("iso_{prefix}_{short}");

    let (admin, conn) = tokio_postgres::connect(&base_url, NoTls)
        .await
        .expect("connect base PG to create isolated test schema");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    admin
        .batch_execute(&format!("CREATE SCHEMA {schema_name}"))
        .await
        .expect("create isolated test schema");

    let sep = if base_url.contains('?') { '&' } else { '?' };
    let schema_url = format!("{base_url}{sep}options=-csearch_path%3D{schema_name}");
    let target = ValidatedControlPgTarget::resolve_from_sources(
        Some(&schema_url),
        Some(&schema_url),
        None,
        ProfileMode::TestIsolated,
    )
    .expect("resolve isolated schema PG target")
    .expect("isolated schema PG target present");
    let runtime = Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal));
    let svc = HeadService::new_with_extras(
        target.clone(),
        runtime,
        None,
        None,
        ProfileMode::TestIsolated,
        Vec::new(),
        HeadServiceExtras::default(),
    )
    .expect("construct isolated HeadService");
    let _ = svc
        .ensure_migrated()
        .await
        .expect("ensure isolated schema migrated v4");
    (schema_url, target, svc, base_url, schema_name)
}

async fn drop_isolated_schema(base_url: &str, schema_name: &str) {
    if let Ok((admin, conn)) = tokio_postgres::connect(base_url, NoTls).await {
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let _ = admin
            .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema_name} CASCADE"))
            .await;
    }
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let short = &Uuid::new_v4().simple().to_string()[..10];
    MemoryScope::try_new(format!("{prefix}-{short}")).expect("valid memory scope")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies V15 barrier-delayed TX A vs concurrent TX B gapless visibility and append-only trigger in one real PG test"
)]
async fn v15_barrier_delayed_tx_never_creates_visibility_hole_and_control_events_append_only() {
    let (pg_url, target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("v15").await;
    let cid = CorrelationId::generate();
    let pool = make_test_pool(&target);

    let snap_before = svc
        .get_state_snapshot(None, true)
        .await
        .expect("initial snapshot");
    let before_seq = snap_before.snapshot_cursor.committed_seq;

    let first_entity_key = format!("v15-entity-a-{}", Uuid::new_v4().simple());
    let second_entity_key = format!("v15-entity-b-{}", Uuid::new_v4().simple());

    let mut conn_a = pool.get().await.expect("pool conn A");
    let mut conn_b = pool.get().await.expect("pool conn B");

    // Barrier guarantees TX A holds the `control:journal` row lock BEFORE TX B attempts to append.
    let barrier = Arc::new(Barrier::new(2));
    let barrier_a = Arc::clone(&barrier);
    let target_a = target.clone();
    let cid_a = cid.clone();
    let writer_first_key = first_entity_key.clone();

    let tx_a_handle = tokio::spawn(async move {
        let tx = conn_a.transaction().await.expect("begin tx A");
        let ev = append_control_event_in_tx(
            &tx,
            &target_a,
            ControlEventAppend {
                entity_key: &writer_first_key,
                event_type: "v15.tx_a",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id: &cid_a,
                payload: json!({ "writer": "A" }),
            },
        )
        .await
        .expect("append tx A");
        // Signal TX B to start while TX A is still uncommitted!
        barrier_a.wait().await;
        time::sleep(Duration::from_millis(250)).await;
        tx.commit().await.expect("commit tx A");
        ev
    });

    let target_b = target.clone();
    let cid_b = cid.clone();
    let writer_second_key = second_entity_key.clone();
    let tx_b_handle = tokio::spawn(async move {
        barrier.wait().await;
        let tx = conn_b.transaction().await.expect("begin tx B");
        let ev = append_control_event_in_tx(
            &tx,
            &target_b,
            ControlEventAppend {
                entity_key: &writer_second_key,
                event_type: "v15.tx_b",
                project_id: None,
                session_id: None,
                task_id: None,
                correlation_id: &cid_b,
                payload: json!({ "writer": "B" }),
            },
        )
        .await
        .expect("append tx B");
        tx.commit().await.expect("commit tx B");
        ev
    });

    // Concurrent subscriber polling `/v1/events` MUST NEVER observe TX B without TX A!
    let svc_poll = svc.clone();
    let poll_first_key = first_entity_key.clone();
    let poll_second_key = second_entity_key.clone();
    let poller = tokio::spawn(async move {
        let mut cursor = before_seq;
        let mut saw_a = false;
        let mut saw_b = false;
        let deadline = time::Instant::now() + Duration::from_secs(10);
        while !(saw_a && saw_b) && time::Instant::now() < deadline {
            let page = svc_poll
                .get_events(cursor, None, None, Some(200))
                .await
                .expect("poll events");
            for ev in &page.events {
                if ev.entity_key == poll_first_key {
                    saw_a = true;
                }
                if ev.entity_key == poll_second_key {
                    assert!(
                        saw_a,
                        "V15 VISIBILITY HOLE: observed TX B ({}) before TX A ({})!",
                        ev.cursor_seq, poll_first_key
                    );
                    saw_b = true;
                }
            }
            cursor = page.scanned_through_seq;
            time::sleep(Duration::from_millis(15)).await;
        }
        assert!(saw_a && saw_b, "poller must observe both TX A and TX B");
    });

    let ev_a = tx_a_handle.await.expect("join tx A");
    let ev_b = tx_b_handle.await.expect("join tx B");
    poller.await.expect("join poller");

    assert!(
        ev_a.cursor_seq < ev_b.cursor_seq,
        "serialized journal counter must assign ev_a ({}) < ev_b ({})",
        ev_a.cursor_seq,
        ev_b.cursor_seq
    );

    // Append-only trigger on `control_events` MUST reject both UPDATE and DELETE.
    let (pg_check, conn_check) = tokio_postgres::connect(&pg_url, NoTls)
        .await
        .expect("connect check");
    tokio::spawn(async move {
        let _ = conn_check.await;
    });
    let upd_err = pg_check
        .execute(
            "UPDATE control_events SET event_type = 'tampered' WHERE cursor_seq = $1",
            &[&ev_a.cursor_seq],
        )
        .await;
    assert!(
        upd_err.is_err(),
        "trg_control_events_append_only must reject UPDATE"
    );
    let del_err = pg_check
        .execute(
            "DELETE FROM control_events WHERE cursor_seq = $1",
            &[&ev_a.cursor_seq],
        )
        .await;
    assert!(
        del_err.is_err(),
        "trg_control_events_append_only must reject DELETE"
    );
    drop(pg_check);
    drop(pool);
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "proves V16 per-key ordering under concurrent workers, stale lease_generation fencing, nack backoff, and V17 idempotent replay"
)]
async fn v16_v17_ordered_outbox_concurrent_workers_fencing_and_idempotent_replay() {
    let (_pg_url, _target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("v16").await;
    let cid = CorrelationId::generate();
    let suffix = &Uuid::new_v4().simple().to_string()[..10];
    let dest = format!("test-dest-{suffix}");
    let key_a = format!("entity-a-{suffix}");
    let key_b = format!("entity-b-{suffix}");
    let key_c = format!("entity-c-{suffix}");

    // Enqueue 3 events on `key_a` and 2 events on `key_b`.
    let alpha_first_idem = IdempotencyKey::try_new(format!("outbox-{suffix}-a1")).expect("key");
    let a1 = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_a,
            1,
            &alpha_first_idem,
            &json!({ "n": 1 }),
            &cid,
        )
        .await
        .expect("enqueue a1");
    // Re-enqueuing duplicate idempotency_key MUST return the same item without allocating a `key_seq` gap!
    let a1_dup = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_a,
            1,
            &alpha_first_idem,
            &json!({ "n": 1 }),
            &cid,
        )
        .await
        .expect("enqueue a1 dup");
    assert_eq!(a1.item_id, a1_dup.item_id);
    assert_eq!(a1.key_seq, 1);

    let a2 = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_a,
            2,
            &IdempotencyKey::try_new(format!("outbox-{suffix}-a2")).expect("key"),
            &json!({ "n": 2 }),
            &cid,
        )
        .await
        .expect("enqueue a2");
    let a3 = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_a,
            3,
            &IdempotencyKey::try_new(format!("outbox-{suffix}-a3")).expect("key"),
            &json!({ "n": 3 }),
            &cid,
        )
        .await
        .expect("enqueue a3");
    assert_eq!((a1.key_seq, a2.key_seq, a3.key_seq), (1, 2, 3));

    let b1 = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_b,
            4,
            &IdempotencyKey::try_new(format!("outbox-{suffix}-b1")).expect("key"),
            &json!({ "n": 1 }),
            &cid,
        )
        .await
        .expect("enqueue b1");
    let b2 = svc
        .enqueue_ordered_outbox(
            &dest,
            &key_b,
            5,
            &IdempotencyKey::try_new(format!("outbox-{suffix}-b2")).expect("key"),
            &json!({ "n": 2 }),
            &cid,
        )
        .await
        .expect("enqueue b2");
    assert_eq!((b1.key_seq, b2.key_seq), (1, 2));

    // Step 1: While Worker 1 holds `key_a` seq=1, Worker 2 MUST NOT overtake it on `key_a`
    //         and MUST instead claim `key_b` seq=1 in parallel!
    let now = TimestampMs::now();
    let w1_claim = svc
        .claim_ordered_outbox_batch(&dest, "worker-1", 1, 5_000, now)
        .await
        .expect("w1 claim");
    assert_eq!(w1_claim.len(), 1);
    let w2_claim = svc
        .claim_ordered_outbox_batch(&dest, "worker-2", 5, 5_000, now)
        .await
        .expect("w2 claim");
    assert_eq!(
        w2_claim.len(),
        1,
        "while one key is leased, Worker 2 must only claim the OTHER independent key, never seq=2 of the leased key!"
    );
    assert_ne!(w1_claim[0].ordering_key, w2_claim[0].ordering_key);
    assert_eq!(w1_claim[0].key_seq, 1);
    assert_eq!(w2_claim[0].key_seq, 1);

    // Ack both seq=1 items, then run 2 concurrent workers to drain remaining items and prove strict per-key ordering!
    let delivered_order: Arc<tokio::sync::Mutex<HashMap<String, Vec<i64>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    {
        let mut guard = delivered_order.lock().await;
        guard
            .entry(w1_claim[0].ordering_key.clone())
            .or_default()
            .push(w1_claim[0].key_seq);
        guard
            .entry(w2_claim[0].ordering_key.clone())
            .or_default()
            .push(w2_claim[0].key_seq);
    }
    svc.ack_ordered_outbox_item(w1_claim[0].item_id, w1_claim[0].lease_generation)
        .await
        .expect("ack w1");
    svc.ack_ordered_outbox_item(w2_claim[0].item_id, w2_claim[0].lease_generation)
        .await
        .expect("ack w2");

    let run_worker = |worker_name: &'static str| {
        let svc_c = svc.clone();
        let dest_c = dest.clone();
        let log_c = Arc::clone(&delivered_order);
        tokio::spawn(async move {
            for _ in 0..20 {
                let claimed = svc_c
                    .claim_ordered_outbox_batch(&dest_c, worker_name, 1, 5_000, TimestampMs::now())
                    .await
                    .expect("claim batch");
                if claimed.is_empty() {
                    time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
                for item in claimed {
                    // Small jitter to maximize concurrent interleaving.
                    time::sleep(Duration::from_millis(15)).await;
                    log_c
                        .lock()
                        .await
                        .entry(item.ordering_key.clone())
                        .or_default()
                        .push(item.key_seq);
                    svc_c
                        .ack_ordered_outbox_item(item.item_id, item.lease_generation)
                        .await
                        .expect("ack item");
                }
            }
        })
    };

    let (r1, r2) = tokio::join!(run_worker("worker-a"), run_worker("worker-b"));
    r1.expect("worker-a");
    r2.expect("worker-b");

    let order_map = delivered_order.lock().await;
    assert_eq!(
        order_map.get(&key_a).map(Vec::as_slice),
        Some([1_i64, 2, 3].as_slice()),
        "key_a must be delivered in strict monotonic order [1, 2, 3]"
    );
    assert_eq!(
        order_map.get(&key_b).map(Vec::as_slice),
        Some([1_i64, 2].as_slice()),
        "key_b must be delivered in strict monotonic order [1, 2]"
    );

    // Step 2 (V16 fencing + V17 crash-before-ack idempotent replay):
    let gamma_idem = IdempotencyKey::try_new(format!("outbox-{suffix}-c1")).expect("key");
    let c1 = svc
        .enqueue_ordered_outbox(&dest, &key_c, 6, &gamma_idem, &json!({ "n": 1 }), &cid)
        .await
        .expect("enqueue c1");
    let t0 = TimestampMs::now();
    let claim1 = svc
        .claim_ordered_outbox_batch(&dest, "crashed-worker", 1, 100, t0)
        .await
        .expect("claim c1");
    assert_eq!(claim1.len(), 1);
    assert_eq!(claim1[0].item_id, c1.item_id);
    assert_eq!(claim1[0].lease_generation, 1);

    // Simulate idempotent consumer receiving c1 from Worker 1 before Worker 1 crashes!
    let mut consumer_seen_keys: HashSet<String> = HashSet::new();
    let mut consumer_applied_count: usize = 0;
    if consumer_seen_keys.insert(claim1[0].idempotency_key.as_str().to_owned()) {
        consumer_applied_count += 1;
    }

    // Lease expires -> Worker 2 reclaims c1 with lease_generation = 2.
    let t1 = TimestampMs::from_millis(t0.as_millis() + 500);
    let claim2 = svc
        .claim_ordered_outbox_batch(&dest, "recovery-worker", 1, 5_000, t1)
        .await
        .expect("reclaim expired c1");
    assert_eq!(claim2.len(), 1);
    assert_eq!(claim2[0].item_id, c1.item_id);
    assert_eq!(claim2[0].lease_generation, 2);

    // Stale Worker 1 wakes up and tries to ack/nack with old lease_generation=1 -> 409 Conflict!
    let ack_err = svc.ack_ordered_outbox_item(c1.item_id, 1).await;
    assert!(
        matches!(ack_err, Err(DennettError::Conflict { .. })),
        "stale lease_generation=1 ack MUST be fenced with Conflict, got: {ack_err:?}"
    );
    let reject_err = svc
        .nack_ordered_outbox_item(c1.item_id, 1, "stale err", 5, 500, t1)
        .await;
    assert!(
        matches!(reject_err, Err(DennettError::Conflict { .. })),
        "stale lease_generation=1 nack MUST be fenced with Conflict, got: {reject_err:?}"
    );

    // Worker 2 replays to consumer (idempotently deduplicated -> count stays 1!) and acks with gen=2.
    if consumer_seen_keys.insert(claim2[0].idempotency_key.as_str().to_owned()) {
        consumer_applied_count += 1;
    }
    assert_eq!(
        consumer_applied_count, 1,
        "V17 idempotent consumer must apply replayed item exactly once"
    );
    svc.ack_ordered_outbox_item(c1.item_id, 2)
        .await
        .expect("fresh lease_generation=2 ack succeeds");
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies V18 snapshot+subscribe lossless/dupe-free catchup under concurrent writes, foreign project filtering, retention watermark, and authority epoch resync"
)]
async fn v18_snapshot_plus_subscribe_zero_loss_zero_dupes_and_resync_conflicts() {
    let (_pg_url, target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("v18").await;
    let cid = CorrelationId::generate();
    let pool = make_test_pool(&target);

    let proj_a = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("V18 Project A"),
                workspace_path: None,
                memory_scope: Some(unique_scope("v18-a")),
            },
            &cid,
        )
        .await
        .expect("project A");
    let proj_b = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("V18 Project B"),
                workspace_path: None,
                memory_scope: Some(unique_scope("v18-b")),
            },
            &cid,
        )
        .await
        .expect("project B");

    // Take initial snapshot for Project A while concurrent writers append 12 events to A and 12 to B!
    let snap = svc
        .get_state_snapshot(Some(proj_a.project_id), false)
        .await
        .expect("snapshot A");
    let snap_cursor = snap.snapshot_cursor.committed_seq;
    let snap_epoch = snap.snapshot_cursor.authority_epoch;

    let write_batch = |pid: ProjectId, tag: &'static str| {
        let pool_c = pool.clone();
        let target_c = target.clone();
        let cid_c = cid.clone();
        tokio::spawn(async move {
            let mut conn = pool_c.get().await.expect("pool writer conn");
            let mut written_seqs = Vec::new();
            for i in 0..12_u32 {
                let tx = conn.transaction().await.expect("begin tx");
                let ev = append_control_event_in_tx(
                    &tx,
                    &target_c,
                    ControlEventAppend {
                        entity_key: &format!("project:{pid}"),
                        event_type: tag,
                        project_id: Some(pid),
                        session_id: None,
                        task_id: None,
                        correlation_id: &cid_c,
                        payload: json!({ "idx": i }),
                    },
                )
                .await
                .expect("append");
                tx.commit().await.expect("commit");
                written_seqs.push(ev.cursor_seq);
            }
            written_seqs
        })
    };

    let (seqs_a, seqs_b) = tokio::try_join!(
        write_batch(proj_a.project_id, "v18.a"),
        write_batch(proj_b.project_id, "v18.b")
    )
    .expect("concurrent writers");

    // Paginate with small `limit = 5` filtering strictly on `proj_a`:
    // MUST return every Project A event with ZERO loss, ZERO dupes, ZERO Project B leaks,
    // while advancing `scanned_through_seq` past Project B's rows!
    let mut cursor = snap_cursor;
    let mut seen_a: Vec<i64> = Vec::new();
    for _ in 0..20 {
        let page = svc
            .get_events(cursor, Some(snap_epoch), Some(proj_a.project_id), Some(5))
            .await
            .expect("get_events page");
        for ev in &page.events {
            assert_eq!(
                ev.project_id,
                Some(proj_a.project_id),
                "foreign project event leaked into filtered stream"
            );
            seen_a.push(ev.cursor_seq);
        }
        assert!(page.scanned_through_seq >= cursor);
        cursor = page.scanned_through_seq;
        if !page.has_more {
            break;
        }
    }
    assert_eq!(
        seen_a, seqs_a,
        "Project A subscription must match exact written cursor_seqs with 0 loss and 0 duplicates"
    );
    assert!(
        cursor >= *seqs_b.last().expect("seqs_b non-empty"),
        "scanned_through_seq must advance past foreign project rows"
    );

    // Retention watermark advancement -> older cursor returns deterministic 409 Conflict!
    let new_retained = cursor;
    svc.advance_retention_watermark(new_retained)
        .await
        .expect("advance retention watermark");
    let ret_err = svc
        .get_events(
            snap_cursor,
            Some(snap_epoch),
            Some(proj_a.project_id),
            Some(10),
        )
        .await;
    assert!(
        matches!(ret_err, Err(DennettError::Conflict { .. })),
        "cursor behind retained_from_seq MUST fail closed with 409 Conflict, got: {ret_err:?}"
    );

    // Authority epoch bump -> old epoch returns deterministic 409 Conflict!
    let bumped = svc.bump_authority_epoch().await.expect("bump epoch");
    assert!(bumped.authority_epoch > snap_epoch);
    let epoch_err = svc
        .get_events(bumped.committed_seq, Some(snap_epoch), None, Some(10))
        .await;
    assert!(
        matches!(epoch_err, Err(DennettError::Conflict { .. })),
        "stale authority_epoch MUST fail closed with 409 Conflict, got: {epoch_err:?}"
    );
    drop(pool);
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies direct V07 late gen-1 complete_task_run fencing and A10 fail-closed gemini-api-key separation"
)]
async fn v07_direct_late_complete_task_run_fenced_and_auth_profile_override_fails_closed() {
    let (pg_url, _target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("v07").await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("v07-direct");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("V07 Direct Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");

    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("V07 Direct Agent"),
                description: String::from("Tests direct generation fencing"),
                system_prompt: String::from("Be brief."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");

    let session = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("V07 Direct Session"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("session");

    // 1. Create task at generation 1 without auto-executing, seed a gen-1 in-flight run row directly.
    let task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: Some(session.session_id),
                parent_task_id: None,
                title: Some(String::from("Fenced Task")),
                goal: String::from("Old Goal Gen 1"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("create task");
    assert_eq!(task.generation, 1);

    let (pg, conn) = tokio_postgres::connect(&pg_url, NoTls)
        .await
        .expect("connect PG");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let stale_run_id = RunId::new();
    let stale_msg_id = MessageId::new();
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id, user_message_id,
            status, task_id, task_generation, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, 'running', $6, 1, $7)",
        &[
            &stale_run_id.as_uuid(),
            &session.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &stale_msg_id.as_uuid(),
            &task.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed gen-1 run");

    // 2. Bump task goal -> generation = 2 (without auto-running yet).
    let updated = svc
        .update_task(
            task.task_id,
            UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Fresh Goal Gen 2")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
            &cid,
        )
        .await
        .expect("update task goal");
    assert_eq!(updated.generation, 2);
    assert_eq!(updated.goal, "Fresh Goal Gen 2");

    // 3. Stale worker from generation 1 tries to complete late!
    let late_outcome = TurnOutcome {
        assistant_text: String::from("STALE_GEN1_MUST_NOT_OVERWRITE_TASK"),
        stop_reason: String::from("end_turn"),
        status: RunStatus::Succeeded,
        events: Vec::new(),
        usage: None,
        human_wait_ms: 0,
    };
    let persisted_status = svc
        .complete_task_run(
            task.task_id,
            stale_run_id,
            1,
            stale_msg_id,
            false,
            &late_outcome,
            42,
            None,
            None,
            &[],
            false,
            &cid,
        )
        .await
        .expect("fenced complete_task_run");
    assert_eq!(persisted_status, RunStatus::Superseded);

    let detail = svc
        .get_task_detail(task.task_id)
        .await
        .expect("task detail");
    assert_eq!(detail.task.generation, 2);
    assert_eq!(detail.task.goal, "Fresh Goal Gen 2");
    assert_ne!(
        detail.task.status,
        TaskStatus::Succeeded,
        "stale gen-1 completion must NEVER mark gen-2 task Succeeded"
    );
    assert!(
        detail
            .task
            .partial_result_summary
            .as_deref()
            .is_none_or(|s| !s.contains("STALE_GEN1")),
        "stale gen-1 text must NEVER overwrite gen-2 summary"
    );
    let fenced_run = detail
        .runs
        .iter()
        .find(|r| r.run_id == stale_run_id)
        .expect("stale run present");
    assert_eq!(fenced_run.status, RunStatus::Superseded);
    assert_eq!(
        fenced_run.stop_reason.as_deref(),
        Some("superseded_by_generation")
    );

    // 4. A10 Auth profile separation: task requiring `GeminiApiKey` on an `OAuthPersonal`-only Head
    //    MUST fail closed with `AUTH_REQUIRED` — NEVER silently falling back to `OAuthPersonal`!
    let api_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Background API-Key Task")),
                goal: String::from("Run unattended"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: Some(RuntimeAuthProfile::GeminiApiKey),
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("create api-key task");

    svc.execute_task_generation(api_task.task_id, 1, cid.clone())
        .await
        .expect("execute_task_generation records fail-closed auth error");

    let api_detail = svc
        .get_task_detail(api_task.task_id)
        .await
        .expect("api task detail");
    assert_eq!(api_detail.task.status, TaskStatus::Failed);
    assert_eq!(
        api_detail.task.error_code.as_deref(),
        Some("AUTH_REQUIRED"),
        "missing gemini-api-key runtime must fail closed with AUTH_REQUIRED without silent fallback"
    );
    drop(pg);
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies H2-01 approval timeout expiry, idempotent Deny on stale non-running run, and ActiveRunGuard Drop cleanup on dropped send_message future"
)]
async fn h2_01_approval_timeout_abandoned_drop_and_idempotent_deny() {
    let (pg_url, _target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("h201").await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("h201");

    let ws_dir = env::temp_dir().join(format!("dennett-h201-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&ws_dir).expect("create temp ws");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("H2-01 Approval Cleanup Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("H2-01 Ask Agent"),
                description: String::from("Interactive ask_before_write agent"),
                system_prompt: String::from("Write carefully."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");
    let session = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("H2-01 Session"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("session");

    // Part A: Short turn timeout while paused in ask_before_write -> approval expires cleanly,
    // zero orphaned pending requests or open inbox cards remain, and file is NOT written!
    let _timeout_or_denied = svc
        .send_message(
            session.session_id,
            SendMessageRequest {
                content: String::from("[SIM:TOOL:write_file:note.txt|hello]"),
                timeout_ms: Some(1_200),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await;

    assert!(
        !ws_dir.join("note.txt").exists(),
        "unapproved write_file must NEVER write to disk"
    );
    let all_perms_after_a = svc
        .list_permission_requests(None, Some(session.session_id))
        .await
        .expect("list all permissions");
    assert!(
        !all_perms_after_a.requests.is_empty(),
        "H2-01 test must genuinely exercise a real ask_before_write permission request"
    );
    assert!(
        all_perms_after_a
            .requests
            .iter()
            .all(|r| r.status != PermissionRequestStatus::Pending),
        "H2-01: no orphaned pending permission requests may remain after turn ends: {:?}",
        all_perms_after_a.requests
    );
    let open_inbox = svc
        .list_inbox_cards(Some("open"), Some(proj.project_id))
        .await
        .expect("list open inbox");
    assert_eq!(
        open_inbox.unresolved_count, 0,
        "H2-01: unresolved inbox count must drop to 0 after approval expiry/turn exit"
    );
    let detail_after_a = svc
        .get_session_detail(session.session_id)
        .await
        .expect("session detail");
    assert!(
        detail_after_a
            .runs
            .iter()
            .all(|r| r.status != RunStatus::Running),
        "H2-01: no run may remain stuck in Running after timeout/exit"
    );

    // Part B: Seed a stale pending permission request + open inbox card + waiting_approval task
    // whose run is ALREADY failed -> ApproveOnce MUST return 409 Conflict, while Deny MUST close
    // idempotently without trapping the owner or flipping the dead task back to Running!
    let (pg, conn) = tokio_postgres::connect(&pg_url, NoTls)
        .await
        .expect("connect PG");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let stale_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: Some(session.session_id),
                parent_task_id: None,
                title: Some(String::from("Stale Approval Task")),
                goal: String::from("Test stale deny"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("stale task");
    let dead_run_id = RunId::new();
    let dead_msg_id = MessageId::new();
    let stale_req_id = PermissionRequestId::new();
    let stale_card_id = InboxCardId::new();
    let sha_zero = "0".repeat(64);
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id, user_message_id,
            status, stop_reason, task_id, task_generation, correlation_id, finished_at
         ) VALUES ($1, $2, $3, $4, $5, 'failed', 'timeout', $6, 1, $7, now())",
        &[
            &dead_run_id.as_uuid(),
            &session.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &dead_msg_id.as_uuid(),
            &stale_task.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed failed run");
    pg.execute(
        "UPDATE tasks SET status = 'waiting_approval', active_run_id = $2 WHERE task_id = $1",
        &[&stale_task.task_id.as_uuid(), &dead_run_id.as_uuid()],
    )
    .await
    .expect("mark task waiting_approval");
    pg.execute(
        "INSERT INTO permission_requests (
            request_id, project_id, session_id, run_id, task_id, agent_id,
            tool_call_id, tool_name, tool_kind, target_summary, normalized_targets,
            parameters_sha256, reason, status, expires_at, correlation_id
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            'tc-stale', 'write_file', 'write', 'stale.txt', '[\"stale.txt\"]'::jsonb,
            $7, 'stale pending request', 'pending', now() + interval '5 minutes', $8
         )",
        &[
            &stale_req_id.as_uuid(),
            &proj.project_id.as_uuid(),
            &session.session_id.as_uuid(),
            &dead_run_id.as_uuid(),
            &stale_task.task_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &sha_zero,
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed stale permission request");
    pg.execute(
        "INSERT INTO inbox_cards (
            card_id, project_id, session_id, task_id, run_id, agent_id,
            kind, status, urgency, subject, detail, permission_request_id
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            'permission_approval', 'open', 'high', 'Разрешить write_file (stale.txt)', 'stale card', $7
         )",
        &[
            &stale_card_id.as_uuid(),
            &proj.project_id.as_uuid(),
            &session.session_id.as_uuid(),
            &stale_task.task_id.as_uuid(),
            &dead_run_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &stale_req_id.as_uuid(),
        ],
    )
    .await
    .expect("seed stale inbox card");

    let approve_dead = svc
        .resolve_permission_request(
            stale_req_id,
            ResolvePermissionRequest {
                decision: PermissionResolutionDecision::ApproveOnce,
                expected_revision: Some(1),
                reason: None,
                max_uses: None,
                expires_in_secs: None,
            },
            &cid,
        )
        .await;
    assert!(
        matches!(approve_dead, Err(DennettError::Conflict { .. })),
        "ApproveOnce on already-failed run MUST fail with 409 Conflict, got: {approve_dead:?}"
    );

    let deny_dead = svc
        .resolve_permission_request(
            stale_req_id,
            ResolvePermissionRequest {
                decision: PermissionResolutionDecision::Deny,
                expected_revision: Some(1),
                reason: Some(String::from("Owner dismissed stale prompt")),
                max_uses: None,
                expires_in_secs: None,
            },
            &cid,
        )
        .await
        .expect("idempotent Deny on dead run must succeed cleanly");
    assert_eq!(deny_dead.request.status, PermissionRequestStatus::Denied);

    let stale_task_after = svc
        .get_task_detail(stale_task.task_id)
        .await
        .expect("stale task detail");
    assert_eq!(
        stale_task_after.task.status,
        TaskStatus::Failed,
        "idempotent Deny on dead run must mark stale waiting_approval task Failed (never Running)"
    );
    let inbox_after_b = svc
        .list_inbox_cards(Some("open"), Some(proj.project_id))
        .await
        .expect("inbox after stale deny");
    assert_eq!(inbox_after_b.unresolved_count, 0);

    // Part C (D2-ARCH-01 / V03): Dropping an in-flight `send_message` caller future (simulating
    // HTTP client disconnect or Desktop UI close) MUST NOT cancel the detached server run!
    // Replaying the SAME `client_message_id` joins the active run cleanly (deduplicated=true),
    // while explicit `cancel_session_run` and server `turn_timeout` cleanly release the session!
    let detached_cmid = format!("detached-v03-{}", Uuid::new_v4().simple());
    let svc_detach = svc.clone();
    let cid_detach = cid.clone();
    let sid = session.session_id;
    let cmid_first = detached_cmid.clone();
    let in_flight = tokio::spawn(async move {
        let _ = svc_detach
            .send_message(
                sid,
                SendMessageRequest {
                    content: String::from("[SIM:SLOW:700]"),
                    timeout_ms: Some(10_000),
                    client_message_id: Some(cmid_first),
                },
                cid_detach,
            )
            .await;
    });

    // Wait until the run row appears in PG as 'running', then drop/abort the caller's handle!
    let mut saw_running = false;
    for _ in 0..40 {
        let det = svc.get_session_detail(sid).await.expect("poll detail");
        if det.runs.iter().any(|r| r.status == RunStatus::Running) {
            saw_running = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(saw_running, "in-flight send_message should enter Running");
    in_flight.abort();
    let _ = in_flight.await;

    // Duplicate retry with the SAME `client_message_id` joins the STILL-RUNNING detached turn
    // instead of returning 409 Conflict, and observes the succeeded run (`V03`)!
    let joined = svc
        .send_message(
            sid,
            SendMessageRequest {
                content: String::from("[SIM:SLOW:700]"),
                timeout_ms: Some(10_000),
                client_message_id: Some(detached_cmid),
            },
            cid.clone(),
        )
        .await
        .expect("D2-ARCH-01: duplicate client_message_id must join active detached run cleanly");
    assert!(
        joined.deduplicated,
        "joined duplicate client_message_id must return deduplicated=true"
    );
    assert_eq!(
        joined.run.status,
        RunStatus::Succeeded,
        "V03: detached server run must survive client disconnect and finish Succeeded"
    );

    // Explicit `/cancel` stops a long detached turn and immediately frees the session slot!
    let svc_cancel_test = svc.clone();
    let cid_cancel_test = cid.clone();
    let cancel_turn_handle = tokio::spawn(async move {
        svc_cancel_test
            .send_message(
                sid,
                SendMessageRequest {
                    content: String::from("[SIM:SLOW:10000]"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                cid_cancel_test,
            )
            .await
    });
    let mut saw_cancel_running = false;
    for _ in 0..40 {
        let det = svc
            .get_session_detail(sid)
            .await
            .expect("poll cancel detail");
        if det.runs.iter().any(|r| r.status == RunStatus::Running) {
            saw_cancel_running = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        saw_cancel_running,
        "long detached turn should enter Running"
    );
    let cancel_resp = svc
        .cancel_session_run(sid, &cid)
        .await
        .expect("explicit cancel_session_run");
    assert!(cancel_resp.cancelled);
    let cancelled_turn = cancel_turn_handle
        .await
        .expect("join cancelled turn")
        .expect("cancelled turn returns Cancelled outcome");
    assert_eq!(cancelled_turn.run.status, RunStatus::Cancelled);

    // Server `turn_timeout` cleanly fails and frees the session slot without stranded `Running` rows!
    let timed_out = svc
        .send_message(
            sid,
            SendMessageRequest {
                content: String::from("[SIM:SLOW:5000]"),
                timeout_ms: Some(300),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await;
    assert!(
        matches!(timed_out, Err(DennettError::Timeout { .. })),
        "server turn_timeout must return Timeout, got: {timed_out:?}"
    );
    let det_after_timeout = svc
        .get_session_detail(sid)
        .await
        .expect("detail after server timeout");
    assert!(
        det_after_timeout
            .runs
            .iter()
            .all(|r| r.status != RunStatus::Running),
        "server turn_timeout must leave zero runs stuck in Running"
    );

    // Subsequent message on the same session must succeed immediately!
    let followup = svc
        .send_message(
            sid,
            SendMessageRequest {
                content: String::from("Followup after cancel and timeout"),
                timeout_ms: Some(5_000),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await
        .expect("followup after cancel and timeout must succeed");
    assert_eq!(followup.run.status, RunStatus::Succeeded);

    drop(pg);
    drop(svc);
    let _ = fs::remove_dir_all(&ws_dir);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies H2-02 startup crash recovery across queued, stranded running, orphaned approval, durable budget approval, and confirmed-effect tasks"
)]
async fn h2_02_startup_crash_recovery_and_scheduler_queued_dispatch() {
    let (pg_url, _target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("h202").await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("h202");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("H2-02 Recovery Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("H2-02 Agent"),
                description: String::from("Recovery test agent"),
                system_prompt: String::from("Be concise."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");

    let mk_task = |title: &'static str, goal: &'static str| {
        let svc_c = svc.clone();
        let pid = proj.project_id;
        let aid = agent.agent_id;
        let cid_c = cid.clone();
        async move {
            svc_c
                .create_task(
                    CreateTaskRequest {
                        project_id: pid,
                        agent_id: aid,
                        session_id: None,
                        parent_task_id: None,
                        title: Some(String::from(title)),
                        goal: String::from(goal),
                        schedule: None,
                        wait_for_signal: None,
                        budget: None,
                        attached_skills: Vec::new(),
                        auth_profile_override: None,
                        execute_immediately: Some(false),
                        target_session_id: None,
                        notify_session_id: None,
                        model_override: None,
                        thinking_override: None,
                        connection_override_id: None,
                        skill_slugs_override: None,
                        trigger: None,
                    },
                    &cid_c,
                )
                .await
                .expect("create task")
        }
    };

    // (a) Plain queued task waiting for scheduler pickup
    let t_queued = mk_task("Queued Task", "Run queued cleanly").await;
    // (b) Stranded running task with zero external effects -> safely re-queued
    let t_stranded = mk_task("Stranded Clean Task", "Recover clean running task").await;
    // (c) Orphaned waiting_approval (permission_approval) -> permission expired, safely re-queued
    let t_orphan_perm = mk_task("Orphan Approval Task", "Recover orphaned approval").await;
    // (d) Durable waiting_approval (budget_exceeded, no active_run) -> MUST stay waiting_approval!
    let t_durable_budget = mk_task("Durable Budget Task", "Preserve durable budget wait").await;
    // (e) Stranded running task WITH a confirmed external effect -> MUST fail closed, NEVER re-queue!
    let t_confirmed_eff = mk_task("Confirmed Effect Task", "Do not re-run confirmed effect").await;

    let (pg, conn) = tokio_postgres::connect(&pg_url, NoTls)
        .await
        .expect("connect PG");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let sha_zero = "1".repeat(64);

    // Seed (b)
    let run_b = RunId::new();
    let msg_b = MessageId::new();
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id, user_message_id,
            status, task_id, task_generation, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, 'running', $6, 1, $7)",
        &[
            &run_b.as_uuid(),
            &t_stranded.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &msg_b.as_uuid(),
            &t_stranded.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed run_b");
    pg.execute(
        "UPDATE tasks SET status = 'running', active_run_id = $2 WHERE task_id = $1",
        &[&t_stranded.task_id.as_uuid(), &run_b.as_uuid()],
    )
    .await
    .expect("update t_stranded");

    // Seed (c)
    let run_c = RunId::new();
    let msg_c = MessageId::new();
    let perm_c = PermissionRequestId::new();
    let card_c = InboxCardId::new();
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id, user_message_id,
            status, task_id, task_generation, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, 'running', $6, 1, $7)",
        &[
            &run_c.as_uuid(),
            &t_orphan_perm.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &msg_c.as_uuid(),
            &t_orphan_perm.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed run_c");
    pg.execute(
        "UPDATE tasks SET status = 'waiting_approval', active_run_id = $2 WHERE task_id = $1",
        &[&t_orphan_perm.task_id.as_uuid(), &run_c.as_uuid()],
    )
    .await
    .expect("update t_orphan_perm");
    pg.execute(
        "INSERT INTO permission_requests (
            request_id, project_id, session_id, run_id, task_id, agent_id,
            tool_call_id, tool_name, tool_kind, target_summary, normalized_targets,
            parameters_sha256, reason, status, expires_at, correlation_id
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            'tc-c', 'write_file', 'write', 'c.txt', '[\"c.txt\"]'::jsonb,
            $7, 'orphaned after crash', 'pending', now() + interval '5 minutes', $8
         )",
        &[
            &perm_c.as_uuid(),
            &proj.project_id.as_uuid(),
            &t_orphan_perm.session_id.as_uuid(),
            &run_c.as_uuid(),
            &t_orphan_perm.task_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &sha_zero,
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed perm_c");
    pg.execute(
        "INSERT INTO inbox_cards (
            card_id, project_id, session_id, task_id, run_id, agent_id,
            kind, status, urgency, subject, detail, permission_request_id
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            'permission_approval', 'open', 'high', 'Orphaned approval', 'Needs expiry on restart', $7
         )",
        &[
            &card_c.as_uuid(),
            &proj.project_id.as_uuid(),
            &t_orphan_perm.session_id.as_uuid(),
            &t_orphan_perm.task_id.as_uuid(),
            &run_c.as_uuid(),
            &agent.agent_id.as_uuid(),
            &perm_c.as_uuid(),
        ],
    )
    .await
    .expect("seed card_c");

    // Seed (d) durable budget approval
    let card_d = InboxCardId::new();
    pg.execute(
        "UPDATE tasks SET status = 'waiting_approval', error_code = 'BUDGET_EXCEEDED', active_run_id = NULL WHERE task_id = $1",
        &[&t_durable_budget.task_id.as_uuid()],
    )
    .await
    .expect("update t_durable_budget");
    pg.execute(
        "INSERT INTO inbox_cards (
            card_id, project_id, session_id, task_id, agent_id,
            kind, status, urgency, subject, detail
         ) VALUES (
            $1, $2, $3, $4, $5,
            'budget_exceeded', 'open', 'high', 'Budget exceeded', 'Must survive restart'
         )",
        &[
            &card_d.as_uuid(),
            &proj.project_id.as_uuid(),
            &t_durable_budget.session_id.as_uuid(),
            &t_durable_budget.task_id.as_uuid(),
            &agent.agent_id.as_uuid(),
        ],
    )
    .await
    .expect("seed card_d");

    // Seed (e) stranded running task WITH confirmed external effect
    let run_e = RunId::new();
    let msg_e = MessageId::new();
    let eff_e = EffectClaimId::new();
    pg.execute(
        "INSERT INTO runs (
            run_id, session_id, agent_id, agent_config_version_id, user_message_id,
            status, task_id, task_generation, correlation_id
         ) VALUES ($1, $2, $3, $4, $5, 'running', $6, 1, $7)",
        &[
            &run_e.as_uuid(),
            &t_confirmed_eff.session_id.as_uuid(),
            &agent.agent_id.as_uuid(),
            &agent.current_config_version_id.as_uuid(),
            &msg_e.as_uuid(),
            &t_confirmed_eff.task_id.as_uuid(),
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed run_e");
    pg.execute(
        "UPDATE tasks SET status = 'running', active_run_id = $2 WHERE task_id = $1",
        &[&t_confirmed_eff.task_id.as_uuid(), &run_e.as_uuid()],
    )
    .await
    .expect("update t_confirmed_eff");
    pg.execute(
        "INSERT INTO effect_claims (
            effect_id, project_id, session_id, task_id, run_id, agent_id,
            effect_idempotency_key, connector_id, operation, target_recipient,
            parameters_json, parameters_sha256, status, provider_operation_id, correlation_id
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            'idem-confirmed-e', 'simulated_external', 'send_email', 'alice@example.com',
            '{}'::jsonb, $7, 'confirmed', 'prov-op-e', $8
         )",
        &[
            &eff_e.as_uuid(),
            &proj.project_id.as_uuid(),
            &t_confirmed_eff.session_id.as_uuid(),
            &t_confirmed_eff.task_id.as_uuid(),
            &run_e.as_uuid(),
            &agent.agent_id.as_uuid(),
            &sha_zero,
            &cid.as_str(),
        ],
    )
    .await
    .expect("seed eff_e");

    // Simulate Head restart recovery!
    let recovered = svc
        .recover_after_startup(&cid)
        .await
        .expect("recover_after_startup");
    assert!(
        recovered >= 3,
        "expected >=3 recovered runs/tasks, got {recovered}"
    );

    assert_eq!(
        svc.get_task_detail(t_stranded.task_id)
            .await
            .expect("d_b")
            .task
            .status,
        TaskStatus::Queued
    );
    assert_eq!(
        svc.get_task_detail(t_orphan_perm.task_id)
            .await
            .expect("d_c")
            .task
            .status,
        TaskStatus::Queued
    );
    assert!(
        svc.list_permission_requests(Some("pending"), None)
            .await
            .expect("pending perms")
            .requests
            .is_empty(),
        "orphaned pending permissions must expire on startup recovery"
    );
    let d_durable = svc
        .get_task_detail(t_durable_budget.task_id)
        .await
        .expect("d_d");
    assert_eq!(
        d_durable.task.status,
        TaskStatus::WaitingApproval,
        "durable budget_exceeded waiting_approval task must survive restart intact"
    );
    assert_eq!(d_durable.open_inbox_cards.len(), 1);
    let d_eff = svc
        .get_task_detail(t_confirmed_eff.task_id)
        .await
        .expect("d_e");
    assert_eq!(
        d_eff.task.status,
        TaskStatus::Failed,
        "task interrupted after confirmed external effect must fail closed, never blindly re-run"
    );
    assert_eq!(
        d_eff.task.error_code.as_deref(),
        Some("HEAD_RESTARTED_DURING_RUN")
    );

    // Scheduler tick MUST dispatch all 3 queued/recovered tasks to Succeeded!
    let _dispatched = svc
        .tick_scheduler_at(TimestampMs::now(), &cid)
        .await
        .expect("tick_scheduler_at");

    for tid in [t_queued.task_id, t_stranded.task_id, t_orphan_perm.task_id] {
        let mut succeeded = false;
        for _ in 0..60 {
            let det = svc.get_task_detail(tid).await.expect("poll recovered task");
            if det.task.status == TaskStatus::Succeeded {
                succeeded = true;
                break;
            }
            time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            succeeded,
            "H2-02: queued/recovered task {tid} must be auto-dispatched to Succeeded by scheduler"
        );
    }

    // Durable budget wait & confirmed-effect failure MUST remain untouched!
    assert_eq!(
        svc.get_task_detail(t_durable_budget.task_id)
            .await
            .expect("d_d after")
            .task
            .status,
        TaskStatus::WaitingApproval
    );
    assert_eq!(
        svc.get_task_detail(t_confirmed_eff.task_id)
            .await
            .expect("d_e after")
            .task
            .status,
        TaskStatus::Failed
    );

    drop(pg);
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies H2-03 remove_project & delete_project full cleanup + deleted_record filtering and H2-07 logical multi-revision artifact remove"
)]
async fn h2_03_h2_07_project_remove_delete_cleanup_deleted_filtering_and_artifact_group_remove() {
    let (pg_url, _target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("h203").await;
    let cid = CorrelationId::generate();
    let scope_a = unique_scope("h203-a");
    let scope_b = unique_scope("h203-b");

    let ws_a = env::temp_dir().join(format!("dennett-h203a-{}", Uuid::new_v4().simple()));
    let ws_b = env::temp_dir().join(format!("dennett-h203b-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&ws_a).expect("ws_a");
    fs::create_dir_all(&ws_b).expect("ws_b");
    let keep_a = ws_a.join("keep_a.txt");
    let keep_b = ws_b.join("keep_b.txt");
    fs::write(&keep_a, "KEEP_A").expect("write keep_a");
    fs::write(&keep_b, "KEEP_B").expect("write keep_b");

    let proj_a = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Project To Remove"),
                workspace_path: Some(ws_a.to_string_lossy().into_owned()),
                memory_scope: Some(scope_a.clone()),
            },
            &cid,
        )
        .await
        .expect("proj_a");
    let proj_b = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("Project To Delete"),
                workspace_path: Some(ws_b.to_string_lossy().into_owned()),
                memory_scope: Some(scope_b.clone()),
            },
            &cid,
        )
        .await
        .expect("proj_b");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("Lifecycle Cleanup Agent"),
                description: String::from("Tests H2-03 & H2-07"),
                system_prompt: String::from("Be brief."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a, scope_b]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");
    let session_a = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj_a.project_id),
                title: String::from("Session A"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("session_a");

    // H2-07: Multi-revision artifact (`spec.md` v1 & v2) -> `Remove` marks ALL non-deleted revisions `removed`!
    let art_v1 = svc
        .create_artifact(
            session_a.session_id,
            CreateArtifactRequest {
                run_id: None,
                name: String::from("spec.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("v1 draft")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Partial,
            },
            &cid,
        )
        .await
        .expect("art_v1");
    let art_v2 = svc
        .create_artifact(
            session_a.session_id,
            CreateArtifactRequest {
                run_id: None,
                name: String::from("spec.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("v2 final")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Final,
            },
            &cid,
        )
        .await
        .expect("art_v2");
    assert_eq!((art_v1.revision, art_v2.revision), (1, 2));

    let rm_art = svc
        .apply_artifact_lifecycle(
            session_a.session_id,
            art_v2.artifact_id,
            ArtifactLifecycleRequest {
                action: ArtifactLifecycleAction::Remove,
                reason: Some(String::from("hide logical artifact")),
            },
            &cid,
        )
        .await
        .expect("remove logical artifact");
    assert_eq!(
        rm_art.artifact.lifecycle_state,
        ArtifactLifecycleState::Removed
    );
    let revs_after = svc
        .list_artifact_revisions(session_a.session_id, "spec.md")
        .await
        .expect("list revisions");
    assert_eq!(revs_after.artifacts.len(), 2);
    assert!(
        revs_after
            .artifacts
            .iter()
            .all(|a| a.lifecycle_state == ArtifactLifecycleState::Removed),
        "H2-07: removing any revision must hide ALL non-deleted revisions of that logical artifact"
    );

    // Seed active/waiting tasks, open inbox cards, and active capability grants on BOTH projects
    let future_fire = TimestampMs::from_millis(TimestampMs::now().as_millis() + 600_000);
    let task_a = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj_a.project_id,
                agent_id: agent.agent_id,
                session_id: Some(session_a.session_id),
                parent_task_id: None,
                title: Some(String::from("Timer Task A")),
                goal: String::from("Scheduled task on project A"),
                schedule: Some(TaskSchedule {
                    timezone: String::from("UTC"),
                    original_expression: Some(String::from("later")),
                    kind: ScheduleKind::OnceAt {
                        fire_at: future_fire,
                    },
                    missed_run_policy: MissedRunPolicy::ExecuteOnce,
                    next_fire_at: None,
                }),
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("task_a");
    let task_b = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj_b.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Signal Task B")),
                goal: String::from("Waiting signal task on project B"),
                schedule: None,
                wait_for_signal: Some(String::from("ready")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("task_b");

    let (pg, conn) = tokio_postgres::connect(&pg_url, NoTls)
        .await
        .expect("connect PG");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    for (pid, sid, tid) in [
        (proj_a.project_id, session_a.session_id, task_a.task_id),
        (proj_b.project_id, task_b.session_id, task_b.task_id),
    ] {
        let gid = CapabilityGrantId::new();
        let card_id = InboxCardId::new();
        pg.execute(
            "INSERT INTO capability_grants (
                grant_id, scope, project_id, agent_id, session_id, tool_pattern, reason
             ) VALUES ($1, $2, $3, $4, $5, 'workspace_write', 'seeded active grant')",
            &[
                &gid.as_uuid(),
                &format!("project:{pid}"),
                &pid.as_uuid(),
                &agent.agent_id.as_uuid(),
                &sid.as_uuid(),
            ],
        )
        .await
        .expect("seed grant");
        pg.execute(
            "INSERT INTO inbox_cards (
                card_id, project_id, session_id, task_id, agent_id,
                kind, status, urgency, subject, detail
             ) VALUES ($1, $2, $3, $4, $5, 'clarification', 'open', 'normal', 'Need input', 'Detail')",
            &[
                &card_id.as_uuid(),
                &pid.as_uuid(),
                &sid.as_uuid(),
                &tid.as_uuid(),
                &agent.agent_id.as_uuid(),
            ],
        )
        .await
        .expect("seed inbox card");
    }

    // Remove Project A -> cancels tasks, revokes grants, withdraws inbox, hides from snapshot & lists!
    let removed = svc
        .remove_project(proj_a.project_id, &cid)
        .await
        .expect("remove_project");
    assert!(!removed.user_workspace_files_deleted);
    assert_eq!(
        fs::read_to_string(&keep_a).expect("keep_a intact"),
        "KEEP_A"
    );
    assert_eq!(
        svc.get_task_detail(task_a.task_id)
            .await
            .expect("task_a detail")
            .task
            .status,
        TaskStatus::Cancelled
    );

    let snap_after_remove = svc
        .get_state_snapshot(None, true)
        .await
        .expect("snapshot after remove");
    assert!(
        snap_after_remove
            .tasks
            .iter()
            .all(|t| t.project_id != proj_a.project_id),
        "removed project's tasks must not leak into state snapshot"
    );
    assert!(
        snap_after_remove
            .inbox_cards
            .iter()
            .all(|c| c.project_id != proj_a.project_id),
        "removed project's inbox cards must not leak into state snapshot"
    );
    assert!(
        snap_after_remove
            .active_grants
            .iter()
            .all(|g| g.project_id != proj_a.project_id),
        "removed project's grants must be revoked and excluded from snapshot"
    );
    assert_eq!(snap_after_remove.unresolved_inbox_count, 1);

    // Delete Project B (confirm=true) -> same clean teardown + zero leaks!
    let deleted = svc
        .delete_project(proj_b.project_id, true, &cid)
        .await
        .expect("delete_project");
    assert!(!deleted.user_workspace_files_deleted);
    assert_eq!(
        fs::read_to_string(&keep_b).expect("keep_b intact"),
        "KEEP_B"
    );
    assert_eq!(
        svc.get_task_detail(task_b.task_id)
            .await
            .expect("task_b detail")
            .task
            .status,
        TaskStatus::Cancelled
    );

    let snap_after_delete = svc
        .get_state_snapshot(None, true)
        .await
        .expect("snapshot after delete");
    assert_eq!(snap_after_delete.unresolved_inbox_count, 0);
    assert_eq!(snap_after_delete.tasks.len(), 0);
    assert_eq!(snap_after_delete.inbox_cards.len(), 0);
    assert_eq!(snap_after_delete.active_grants.len(), 0);
    assert_eq!(
        svc.list_tasks(None, None)
            .await
            .expect("list_tasks")
            .tasks
            .len(),
        0
    );
    assert_eq!(
        svc.list_inbox_cards(Some("open"), None)
            .await
            .expect("list_inbox")
            .cards
            .len(),
        0
    );
    assert_eq!(
        svc.list_capability_grants(None, false)
            .await
            .expect("list_grants")
            .grants
            .len(),
        0
    );

    drop(pg);
    drop(svc);
    let _ = fs::remove_dir_all(&ws_a);
    let _ = fs::remove_dir_all(&ws_b);
    drop_isolated_schema(&base_url, &schema_name).await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies H2-04 signal name matching & goal edit preservation, H2-05 same-session serialization, and H2-06 isolated background gemini-api-key runtime routing"
)]
async fn h2_04_h2_05_h2_06_signal_matching_session_serialization_and_background_api_runtime() {
    let (_pg_url, target, svc, base_url, schema_name) =
        make_isolated_schema_head_service("h204").await;
    let cid = CorrelationId::generate();
    let scope = unique_scope("h204");

    let proj = svc
        .create_project(
            CreateProjectRequest {
                name: String::from("H2-04/05/06 Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            &cid,
        )
        .await
        .expect("project");
    let agent = svc
        .create_agent(
            CreateAgentRequest {
                name: String::from("H2-04/05/06 Agent"),
                description: String::from("Signals, session serialization, and dual runtime"),
                system_prompt: String::from("Be brief."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            &cid,
        )
        .await
        .expect("agent");

    // H2-04 Part 1: Named signal waiting + non-matching signal ignored + goal edit preserves waiting_signal!
    let sig_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Wait CI Signal")),
                goal: String::from("Deploy v1 on ci_green"),
                schedule: None,
                wait_for_signal: Some(String::from("ci_green")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("sig_task");
    assert_eq!(sig_task.status, TaskStatus::WaitingSignal);
    assert_eq!(sig_task.expected_signal_name.as_deref(), Some("ci_green"));

    let wrong_sig = svc
        .send_task_signal(
            sig_task.task_id,
            TaskSignalRequest {
                idempotency_key: IdempotencyKey::try_new("sig-wrong-1").expect("key"),
                signal_name: String::from("other_signal"),
                payload_text: String::from("should not wake"),
            },
            &cid,
        )
        .await
        .expect("wrong signal");
    assert_eq!(
        wrong_sig.signal.outcome_status,
        TaskSignalOutcome::QueuedOnTask,
        "non-matching signal name MUST NOT wake waiting_signal task"
    );
    assert_eq!(wrong_sig.task.status, TaskStatus::WaitingSignal);
    assert_eq!(
        wrong_sig.task.expected_signal_name.as_deref(),
        Some("ci_green")
    );

    let edited_sig = svc
        .update_task(
            sig_task.task_id,
            UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Deploy v2 UPDATED on ci_green")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
            &cid,
        )
        .await
        .expect("edit goal while waiting_signal");
    assert_eq!(edited_sig.generation, 2);
    assert_eq!(
        edited_sig.status,
        TaskStatus::WaitingSignal,
        "editing goal without clear_schedule must preserve WaitingSignal"
    );
    assert_eq!(edited_sig.expected_signal_name.as_deref(), Some("ci_green"));

    let matching_sig = svc
        .send_task_signal(
            sig_task.task_id,
            TaskSignalRequest {
                idempotency_key: IdempotencyKey::try_new("sig-match-1").expect("key"),
                signal_name: String::from("ci_green"),
                payload_text: String::from("build 42 ok"),
            },
            &cid,
        )
        .await
        .expect("matching signal");
    assert_eq!(
        matching_sig.signal.outcome_status,
        TaskSignalOutcome::WokeTask
    );
    assert_eq!(matching_sig.task.expected_signal_name, None);

    let mut sig_done = false;
    for _ in 0..60 {
        let det = svc
            .get_task_detail(sig_task.task_id)
            .await
            .expect("poll sig");
        if det.task.status == TaskStatus::Succeeded {
            assert_eq!(det.task.generation, 2);
            assert!(
                det.task
                    .partial_result_summary
                    .as_deref()
                    .is_some_and(|s| s.contains("Deploy v2 UPDATED")),
                "woken task must execute generation 2 goal: {:?}",
                det.task.partial_result_summary
            );
            sig_done = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sig_done, "matching signal must execute task to Succeeded");

    // H2-04 Part 2: Goal edit supersedes open `budget_exceeded` inbox card!
    let budget_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Budget Supersede Task")),
                goal: String::from("[SIM:TOKENS:300] Exceed budget"),
                schedule: None,
                wait_for_signal: None,
                budget: Some(TaskBudget {
                    max_turns: Some(5),
                    max_tokens: Some(50),
                    max_wall_secs: Some(60),
                    exceed_policy: BudgetExceedPolicy::AskApproval,
                }),
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(true),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("budget_task");
    let mut saw_budget_card = false;
    for _ in 0..60 {
        let det = svc
            .get_task_detail(budget_task.task_id)
            .await
            .expect("poll");
        if det.task.status == TaskStatus::WaitingApproval
            && det
                .open_inbox_cards
                .iter()
                .any(|c| c.kind == InboxCardKind::BudgetExceeded)
        {
            saw_budget_card = true;
            break;
        }
        time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        saw_budget_card,
        "budget task must open budget_exceeded card"
    );
    let _updated_budget = svc
        .update_task(
            budget_task.task_id,
            UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Shorter goal")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
            &cid,
        )
        .await
        .expect("update budget task goal");
    let after_budget_edit = svc
        .get_task_detail(budget_task.task_id)
        .await
        .expect("after budget edit");
    assert!(
        after_budget_edit.open_inbox_cards.is_empty(),
        "H2-04: goal edit must supersede open budget_exceeded inbox cards"
    );

    // H2-05: Same-session serialization between interactive `send_message` and `execute_task_generation`!
    let shared_session = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Shared Serialized Session"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("shared session");
    let shared_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: Some(shared_session.session_id),
                parent_task_id: None,
                title: Some(String::from("Shared Session Task")),
                goal: String::from("Task on same session"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("shared task");

    let interactive_svc = svc.clone();
    let interactive_cid = cid.clone();
    let sid_shared = shared_session.session_id;
    let h_msg = tokio::spawn(async move {
        interactive_svc
            .send_message(
                sid_shared,
                SendMessageRequest {
                    content: String::from("[SIM:SLOW:900]"),
                    timeout_ms: Some(5_000),
                    client_message_id: None,
                },
                interactive_cid,
            )
            .await
    });

    // Wait until interactive turn holds the session lock
    time::sleep(Duration::from_millis(180)).await;

    // Concurrent second interactive `send_message` on same session MUST return 409 Conflict!
    let concurrent_msg = svc
        .send_message(
            sid_shared,
            SendMessageRequest {
                content: String::from("Competing interactive turn"),
                timeout_ms: Some(5_000),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await;
    assert!(
        matches!(concurrent_msg, Err(DennettError::Conflict { .. })),
        "H2-05: concurrent send_message on busy session must return 409 Conflict, got: {concurrent_msg:?}"
    );

    // Concurrent `execute_task_generation` on the SAME session waits cleanly for the session lock
    // instead of clobbering `active_runs`!
    let queued_task_svc = svc.clone();
    let queued_task_cid = cid.clone();
    let tid_shared = shared_task.task_id;
    let h_task = tokio::spawn(async move {
        queued_task_svc
            .execute_task_generation(tid_shared, 1, queued_task_cid)
            .await
    });

    let msg_res = h_msg.await.expect("join h_msg").expect("h_msg succeeded");
    assert_eq!(msg_res.run.status, RunStatus::Succeeded);
    h_task
        .await
        .expect("join h_task")
        .expect("h_task succeeded after waiting for session lock");
    assert_eq!(
        svc.get_task_detail(tid_shared)
            .await
            .expect("shared task detail")
            .task
            .status,
        TaskStatus::Succeeded
    );

    // Reverse direction: slow task holds session lock -> interactive `send_message` gets clean 409!
    let slow_task = svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: Some(sid_shared),
                parent_task_id: None,
                title: Some(String::from("Slow Background Task")),
                goal: String::from("[SIM:SLOW:900]"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("slow task");
    let slow_bg_svc = svc.clone();
    let slow_bg_cid = cid.clone();
    let tid_slow = slow_task.task_id;
    let h_slow_task = tokio::spawn(async move {
        slow_bg_svc
            .execute_task_generation(tid_slow, 1, slow_bg_cid)
            .await
    });
    time::sleep(Duration::from_millis(180)).await;
    let blocked_msg = svc
        .send_message(
            sid_shared,
            SendMessageRequest {
                content: String::from("Should get 409 while task holds session"),
                timeout_ms: Some(5_000),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await;
    assert!(
        matches!(blocked_msg, Err(DennettError::Conflict { .. })),
        "H2-05: interactive send_message must return 409 Conflict while task holds session lock, got: {blocked_msg:?}"
    );
    h_slow_task
        .await
        .expect("join slow task")
        .expect("slow task succeeded");

    // H2-06: Dual runtime routing — HeadService with primary `OAuthPersonal` AND isolated
    // `background_runtime: Some(GeminiApiKey)` routes `auth_profile_override: GeminiApiKey`
    // cleanly without touching `OAuthPersonal`!
    let dual_svc = HeadService::new_with_extras(
        target.clone(),
        Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal)),
        None,
        None,
        ProfileMode::TestIsolated,
        Vec::new(),
        HeadServiceExtras {
            background_runtime: Some(Arc::new(SimulatedRuntime::new(
                RuntimeAuthProfile::GeminiApiKey,
            ))),
            ..HeadServiceExtras::default()
        },
    )
    .expect("construct dual-runtime HeadService");
    let _ = dual_svc.ensure_migrated().await.expect("migrated");

    let bg_api_task = dual_svc
        .create_task(
            CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Headless API Key Task")),
                goal: String::from("Execute on gemini-api-key background runtime"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: Some(RuntimeAuthProfile::GeminiApiKey),
                execute_immediately: Some(false),
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
            },
            &cid,
        )
        .await
        .expect("create bg api task");
    dual_svc
        .execute_task_generation(bg_api_task.task_id, 1, cid.clone())
        .await
        .expect("execute on background gemini-api-key runtime");

    let bg_detail = dual_svc
        .get_task_detail(bg_api_task.task_id)
        .await
        .expect("bg detail");
    assert_eq!(bg_detail.task.status, TaskStatus::Succeeded);
    let bg_session_detail = dual_svc
        .get_session_detail(bg_api_task.session_id)
        .await
        .expect("bg session detail");
    assert_eq!(
        bg_session_detail
            .session
            .runtime_binding
            .as_ref()
            .map(|b| b.auth_profile),
        Some(RuntimeAuthProfile::GeminiApiKey),
        "H2-06: task with auth_profile_override=GeminiApiKey must bind to isolated background_runtime"
    );

    let interactive_session = dual_svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Interactive OAuth Session"),
                session_kind: SessionKind::UserChat,
            },
            &cid,
        )
        .await
        .expect("interactive session");
    let interactive_resp = dual_svc
        .send_message(
            interactive_session.session_id,
            SendMessageRequest {
                content: String::from("Interactive turn stays on OAuth"),
                timeout_ms: Some(5_000),
                client_message_id: None,
            },
            cid.clone(),
        )
        .await
        .expect("interactive turn");
    assert_eq!(
        interactive_resp.runtime_binding.auth_profile,
        RuntimeAuthProfile::OAuthPersonal,
        "H2-06: interactive chat must stay strictly on OAuthPersonal"
    );

    drop(dual_svc);
    drop(svc);
    drop_isolated_schema(&base_url, &schema_name).await;
}
