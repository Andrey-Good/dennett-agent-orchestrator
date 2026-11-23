//! Real PostgreSQL isolation integration tests for Dennett D0.
//!
//! Requires an active isolated test PostgreSQL instance and:
//! - `DENNETT_TEST_PROFILE=1`
//! - `DENNETT_TEST_MEMORY_PG_URL`
//! - `DENNETT_TEST_CONTROL_PG_URL`

use std::env;

use dennett_contracts::DennettError;
use dennett_testkit::pg_harness::{
    probe_connection, replace_url_database, verify_cross_domain_connect_denied,
    verify_transactional_roundtrip,
};
use dennett_testkit::profile::{
    DEFAULT_TEST_CONTROL_DB, DEFAULT_TEST_CONTROL_ROLE, DEFAULT_TEST_MEMORY_DB,
    DEFAULT_TEST_MEMORY_ROLE, FaultInjectionGuard, TEST_PROFILE_ENV, TestProfile,
};

fn require_env(name: &str) -> String {
    match env::var(name) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => panic!(
            "required integration env `{name}` is missing; run via `dennett-check integration`"
        ),
    }
}

fn require_isolated_guard() -> FaultInjectionGuard {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for PostgreSQL integration tests"
    );
    let profile = match TestProfile::new_isolated("pg-integration") {
        Ok(p) => p,
        Err(err) => panic!("failed to create test profile descriptor: {err}"),
    };
    match FaultInjectionGuard::acquire(&profile) {
        Ok(g) => g,
        Err(err) => panic!("failed to acquire FaultInjectionGuard: {err}"),
    }
}

#[tokio::test]
async fn memory_and_control_databases_accept_their_own_roles_and_transactions() {
    let guard = require_isolated_guard();
    let memory_url = require_env("DENNETT_TEST_MEMORY_PG_URL");
    let control_url = require_env("DENNETT_TEST_CONTROL_PG_URL");

    let mem_info = match probe_connection(&memory_url).await {
        Ok(info) => info,
        Err(err) => panic!("memory DB connection failed: {err}"),
    };
    assert_eq!(mem_info.role, DEFAULT_TEST_MEMORY_ROLE);
    assert_eq!(mem_info.database, DEFAULT_TEST_MEMORY_DB);

    let ctrl_info = match probe_connection(&control_url).await {
        Ok(info) => info,
        Err(err) => panic!("control DB connection failed: {err}"),
    };
    assert_eq!(ctrl_info.role, DEFAULT_TEST_CONTROL_ROLE);
    assert_eq!(ctrl_info.database, DEFAULT_TEST_CONTROL_DB);

    if let Err(err) = verify_transactional_roundtrip(&memory_url, &guard).await {
        panic!("memory DB transactional commit/rollback probe failed: {err}");
    }
    if let Err(err) = verify_transactional_roundtrip(&control_url, &guard).await {
        panic!("control DB transactional commit/rollback probe failed: {err}");
    }
}

#[tokio::test]
async fn cross_domain_and_default_postgres_connections_are_explicitly_denied() {
    let _guard = require_isolated_guard();
    let memory_url = require_env("DENNETT_TEST_MEMORY_PG_URL");
    let control_url = require_env("DENNETT_TEST_CONTROL_PG_URL");

    let memory_to_control = match replace_url_database(&memory_url, DEFAULT_TEST_CONTROL_DB) {
        Ok(u) => u,
        Err(err) => panic!("failed rewriting memory URL to control DB: {err}"),
    };
    if let Err(err) = verify_cross_domain_connect_denied(&memory_to_control).await {
        panic!("memory_role -> control_db cross-domain denial check failed: {err}");
    }

    let control_to_memory = match replace_url_database(&control_url, DEFAULT_TEST_MEMORY_DB) {
        Ok(u) => u,
        Err(err) => panic!("failed rewriting control URL to memory DB: {err}"),
    };
    if let Err(err) = verify_cross_domain_connect_denied(&control_to_memory).await {
        panic!("control_role -> memory_db cross-domain denial check failed: {err}");
    }

    // Least-privilege test roles must also be denied CONNECT on the default `postgres` database.
    let memory_to_postgres = match replace_url_database(&memory_url, "postgres") {
        Ok(u) => u,
        Err(err) => panic!("failed rewriting memory URL to postgres DB: {err}"),
    };
    if let Err(err) = verify_cross_domain_connect_denied(&memory_to_postgres).await {
        panic!("memory_role -> postgres DB denial check failed: {err}");
    }

    let control_to_postgres = match replace_url_database(&control_url, "postgres") {
        Ok(u) => u,
        Err(err) => panic!("failed rewriting control URL to postgres DB: {err}"),
    };
    if let Err(err) = verify_cross_domain_connect_denied(&control_to_postgres).await {
        panic!("control_role -> postgres DB denial check failed: {err}");
    }
}

#[test]
fn fault_injection_guard_blocks_production_and_escaped_subpaths() {
    let guard = require_isolated_guard();

    let prod = TestProfile::production_like(env::temp_dir().join("dennett-test-prod-sim"));
    assert!(matches!(
        FaultInjectionGuard::acquire(&prod),
        Err(DennettError::SafetyGuardRejected { .. })
    ));

    let escaped = env::temp_dir().join("outside-guard-root");
    assert!(matches!(
        guard.validate_subpath(&escaped),
        Err(DennettError::SafetyGuardRejected { .. })
    ));
}
