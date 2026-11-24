//! Real PostgreSQL 16 concurrency, journal visibility (`V15`/`V18`), per-key ordered outbox
//! (`V16`/`V17`), direct generation fencing (`V07`), and auth-profile isolation (`A10`)
//! integration tests for `dennett-head`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    CorrelationId, CreateAgentRequest, CreateProjectRequest, CreateSessionRequest,
    CreateTaskRequest, DennettError, IdempotencyKey, MemoryScope, MessageId, ProfileMode,
    ProjectId, RunId, RunStatus, RuntimeAuthProfile, TaskStatus, TimestampMs, UpdateTaskRequest,
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

async fn make_head_service() -> (String, ValidatedControlPgTarget, HeadService) {
    let (url, target) = require_control_pg_target();
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
    .expect("construct HeadService");
    let _ = svc.ensure_migrated().await.expect("ensure migrated v4");
    (url, target, svc)
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
    let (_pg_url, _target, svc) = make_head_service().await;
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
    let (pg_url, _target, svc) = make_head_service().await;
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
            },
            &cid,
        )
        .await
        .expect("agent");

    let session = svc
        .create_session(
            CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: proj.project_id,
                title: String::from("V07 Direct Session"),
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
            },
            &cid,
        )
        .await
        .expect("create api-key task");

    svc.execute_task_generation(api_task.task_id, 1, None, cid.clone())
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
}
