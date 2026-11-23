//! Fast deterministic unit and local fixture safety tests for `dennett-testkit`.

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use dennett_contracts::{DennettError, ProfileMode};
use dennett_testkit::acp_probe::{
    AcpClient, AcpSpawnConfig, MIN_AGY_VERSION, Semver, ensure_agy_version_supported,
    parse_agy_version,
};
use dennett_testkit::pg_harness::replace_url_database;
use dennett_testkit::profile::{FaultInjectionGuard, TEST_PROFILE_ENV, TestProfile};
use dennett_testkit::redact::{REDACTED_PLACEHOLDER, redact_secrets};
use proptest::prelude::*;
use serde_json::{Value, json};

#[test]
fn isolated_profile_paths_validate_and_production_profile_is_rejected() {
    let profile = match TestProfile::new_isolated("unit-ok") {
        Ok(p) => p,
        Err(err) => panic!("expected valid isolated profile: {err}"),
    };
    assert_eq!(profile.mode(), ProfileMode::TestIsolated);
    assert!(!profile.allow_live_external_effects());
    assert!(
        profile.validate_paths().is_ok(),
        "isolated profile paths must pass validation"
    );

    let prod = TestProfile::production_like(env::temp_dir().join("dennett-test-prod-sim"));
    assert_eq!(prod.mode(), ProfileMode::Production);
    assert!(matches!(
        FaultInjectionGuard::acquire(&prod),
        Err(DennettError::SafetyGuardRejected { .. })
    ));
}

#[test]
fn fault_injection_guard_fails_closed_without_env_marker() {
    let profile = match TestProfile::new_isolated("unit-no-marker") {
        Ok(p) => p,
        Err(err) => panic!("failed building profile: {err}"),
    };

    assert_ne!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV} must not be set in default fast unit tests"
    );

    let res = FaultInjectionGuard::acquire(&profile);
    assert!(
        matches!(res, Err(DennettError::SafetyGuardRejected { .. })),
        "FaultInjectionGuard::acquire must fail closed without {TEST_PROFILE_ENV}=1, got: {res:?}"
    );
}

#[test]
fn redact_secrets_strips_explicit_and_pattern_credentials() {
    let explicit_key = "my-custom-runtime-secret-42";
    let sample = format!(
        "err uri=postgres://dennett_memory_role:pg;p)a's/999@127.0.0.1:55432/dennett_memory_test?password=query_pw_77 \
         kv password = kv_secret_123 quoted password='a\\'escaped_secret_44' \
         env GEMINI_API_KEY=AIzaSyDummyKeyValue1234567890abcdef GOOGLE_API_KEY=goog_key_88 \
         hdr x-goog-api-key: hdr_key_99 \
         py {{'GEMINI_API_KEY': 'py_key_55'}} \
         json {{\"access_token\":\"json_acc_11\",\"refresh_token\":\"json_ref_22\"}} \
         auth Authorization: Bearer   ya29.a0ARrdaM_example_oauth_token_9999 \
         tab Bearer\ttab_bearer_tok_33 refresh 1//0gRefreshTokenExample987654 \
         jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3OCJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV \
         custom={explicit_key}"
    );

    let cleaned = redact_secrets(&sample, &[explicit_key]);

    for forbidden in [
        "pg;p)a's/999",
        "query_pw_77",
        "kv_secret_123",
        "escaped_secret_44",
        "AIzaSyDummyKeyValue1234567890abcdef",
        "goog_key_88",
        "hdr_key_99",
        "py_key_55",
        "json_acc_11",
        "json_ref_22",
        "ya29.a0ARrdaM_example_oauth_token_9999",
        "tab_bearer_tok_33",
        "0gRefreshTokenExample987654",
        "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV",
        explicit_key,
    ] {
        assert!(
            !cleaned.contains(forbidden),
            "secret `{forbidden}` leaked in redacted output: {cleaned}"
        );
    }
    assert!(
        cleaned.contains(REDACTED_PLACEHOLDER),
        "redaction placeholder missing: {cleaned}"
    );
    assert!(
        cleaned.contains("127.0.0.1:55432/dennett_memory_test"),
        "non-secret diagnostic host/db context should remain intact: {cleaned}"
    );
}

#[test]
fn parse_and_validate_agy_versions() {
    assert_eq!(parse_agy_version("1.5.15"), Ok(Semver::new(1, 5, 15)));
    assert_eq!(parse_agy_version("agy 1.5.0\n"), Ok(Semver::new(1, 5, 0)));
    assert_eq!(
        parse_agy_version("antigravity v1.6.2-beta.1 (abc123)"),
        Ok(Semver::new(1, 6, 2))
    );
    assert!(parse_agy_version("no version here").is_err());

    assert_eq!(
        ensure_agy_version_supported("1.5.15"),
        Ok(Semver::new(1, 5, 15))
    );
    assert_eq!(
        ensure_agy_version_supported(&MIN_AGY_VERSION.to_string()),
        Ok(MIN_AGY_VERSION)
    );
    assert!(matches!(
        ensure_agy_version_supported("1.4.99"),
        Err(DennettError::RuntimeVersionUnsupported { .. })
    ));
}

#[test]
fn replace_url_database_preserves_host_and_query() {
    let src =
        "postgres://dennett_memory_role:pw@127.0.0.1:55432/dennett_memory_test?sslmode=disable";
    let rewritten = match replace_url_database(src, "dennett_control_test") {
        Ok(u) => u,
        Err(err) => panic!("URL rewrite should succeed: {err}"),
    };
    assert_eq!(
        rewritten,
        "postgres://dennett_memory_role:pw@127.0.0.1:55432/dennett_control_test?sslmode=disable"
    );
    assert!(replace_url_database(src, "bad-db;drop").is_err());
}

#[test]
fn acp_spawn_rejects_dangerously_skip_permissions_flag() {
    let mut cfg = AcpSpawnConfig::new(env::temp_dir());
    cfg.extra_args
        .push(String::from("--dangerously-skip-permissions"));
    let res = AcpClient::spawn(&cfg);
    assert!(
        matches!(res, Err(DennettError::SafetyGuardRejected { .. })),
        "ACP spawn must reject --dangerously-skip-permissions before launching a process"
    );
}

fn mcp_stdio_raw(stdin: &mut impl Write, reader: &mut impl BufRead, raw_line: &str) -> Value {
    if let Err(err) = writeln!(stdin, "{raw_line}").and_then(|()| stdin.flush()) {
        panic!("write to fixture stdin failed: {err}");
    }
    let mut resp_line = String::new();
    match reader.read_line(&mut resp_line) {
        Ok(n) if n > 0 => match serde_json::from_str(&resp_line) {
            Ok(v) => v,
            Err(err) => panic!("invalid JSON from fixture `{resp_line}`: {err}"),
        },
        Ok(_) => panic!("fixture closed stdout prematurely"),
        Err(err) => panic!("read from fixture stdout failed: {err}"),
    }
}

fn mcp_stdio_rpc(stdin: &mut impl Write, reader: &mut impl BufRead, payload: &Value) -> Value {
    let line = match serde_json::to_string(payload) {
        Ok(s) => s,
        Err(err) => panic!("serialize failed: {err}"),
    };
    mcp_stdio_raw(stdin, reader, &line)
}

#[test]
fn mcp_fixture_stdio_allows_scoped_and_denies_foreign_or_malformed() {
    let bin = env!("CARGO_BIN_EXE_dennett-d0-mcp-fixture");
    let expected_token = "UNIT_FIXTURE_TOKEN_7741";

    let mut child = match Command::new(bin)
        .env("DENNETT_PROBE_MEMORY_TOKEN", expected_token)
        .env("DENNETT_PROBE_ALLOWED_SCOPE", "project-alpha")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning dennett-d0-mcp-fixture: {err}"),
    };

    let Some(mut stdin) = child.stdin.take() else {
        panic!("fixture stdin missing");
    };
    let Some(stdout) = child.stdout.take() else {
        panic!("fixture stdout missing");
    };
    let mut reader = BufReader::new(stdout);

    let init = mcp_stdio_rpc(
        &mut stdin,
        &mut reader,
        &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } }),
    );
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        Value::String(String::from("dennett-d0-mcp-fixture"))
    );

    let allowed = mcp_stdio_rpc(
        &mut stdin,
        &mut reader,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "memory_probe_read", "arguments": { "key": "k1", "scope": "project-alpha" } } }),
    );
    assert_eq!(allowed["result"]["isError"], Value::Bool(false));
    assert!(
        allowed["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains(expected_token)
    );

    let denied = mcp_stdio_rpc(
        &mut stdin,
        &mut reader,
        &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "memory_probe_read", "arguments": { "key": "k1", "scope": "project-secret-other" } } }),
    );
    assert_eq!(denied["result"]["isError"], Value::Bool(true));
    let denied_text = denied["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(denied_text.contains("PERMISSION_DENIED"));
    assert!(!denied_text.contains(expected_token));

    // Non-string scope values (null, number, object) must return INVALID_INPUT, not default to allowed_scope.
    for (req_id, bad_scope) in [(4, Value::Null), (5, json!(123)), (6, json!({}))] {
        let resp = mcp_stdio_rpc(
            &mut stdin,
            &mut reader,
            &json!({ "jsonrpc": "2.0", "id": req_id, "method": "tools/call", "params": { "name": "memory_probe_read", "arguments": { "key": "k1", "scope": bad_scope } } }),
        );
        assert_eq!(resp["result"]["isError"], Value::Bool(true));
        let txt = resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(
            txt.contains("INVALID_INPUT"),
            "non-string scope must fail with INVALID_INPUT, got: {txt}"
        );
        assert!(!txt.contains(expected_token));
    }

    // Oversized line (> 64 KiB) must be rejected cleanly with JSON-RPC -32600 without OOM/crash.
    let huge = "x".repeat(70 * 1024);
    let huge_resp = mcp_stdio_raw(&mut stdin, &mut reader, &huge);
    assert_eq!(huge_resp["error"]["code"], json!(-32600));

    drop(stdin);
    let status = match child.wait() {
        Ok(s) => s,
        Err(err) => panic!("failed waiting on fixture exit: {err}"),
    };
    assert!(status.success(), "fixture should exit cleanly on EOF");
}

#[test]
fn intentional_failure_canary() {
    assert_ne!(
        env::var("DENNETT_FORCE_INTENTIONAL_FAILURE")
            .ok()
            .as_deref(),
        Some("1"),
        "INTENTIONAL_FAILURE_CANARY_TRIGGERED: runner self-check verified failure propagation"
    );
}

proptest! {
    #[test]
    fn prop_redact_secrets_never_leaks_generated_credentials(
        explicit in "[A-Za-z0-9_-]{10,24}",
        google_tail in "[A-Za-z0-9_-]{20,32}",
        oauth_tail in "[A-Za-z0-9._-]{12,24}",
        refresh_tail in "[A-Za-z0-9._-]{12,24}",
        pg_pass in "[A-Za-z0-9!#$%^&*()_+\\-=;,.>'/]{10,24}",
    ) {
        let gkey = format!("AIza{google_tail}");
        let oauth = format!("ya29.{oauth_tail}");
        let refresh = format!("1//{refresh_tail}");
        let payload = format!(
            "custom={explicit} key={gkey} Authorization: Bearer\t{oauth} refresh={refresh} \
             db=postgres://role:{pg_pass}@127.0.0.1:55432/dennett_memory_test"
        );
        let cleaned = redact_secrets(&payload, &[explicit.as_str()]);
        prop_assert!(!cleaned.contains(&explicit));
        prop_assert!(!cleaned.contains(&gkey));
        prop_assert!(!cleaned.contains(&oauth));
        prop_assert!(!cleaned.contains(&refresh));
        prop_assert!(!cleaned.contains(&pg_pass));
        prop_assert!(cleaned.contains("127.0.0.1:55432/dennett_memory_test"));
    }

    #[test]
    fn prop_redact_secrets_is_idempotent_on_arbitrary_unicode(
        text in any::<String>(),
        secret in any::<String>(),
    ) {
        let once = redact_secrets(&text, &[secret.as_str()]);
        let twice = redact_secrets(&once, &[secret.as_str()]);
        prop_assert_eq!(once, twice);
    }

    #[test]
    fn prop_parse_agy_version_roundtrips_and_enforces_minimum(
        major in 0_u64..10,
        minor in 0_u64..30,
        patch in 0_u64..50,
    ) {
        let formatted = format!("agy v{major}.{minor}.{patch}-rc.1 (commit)");
        let expected = Semver::new(major, minor, patch);
        prop_assert_eq!(parse_agy_version(&formatted), Ok(expected));
        let supported = ensure_agy_version_supported(&formatted);
        if expected >= MIN_AGY_VERSION {
            prop_assert_eq!(supported, Ok(expected));
        } else {
            prop_assert!(
                matches!(
                    supported,
                    Err(DennettError::RuntimeVersionUnsupported { .. })
                ),
                "expected RuntimeVersionUnsupported for {formatted}, got: {supported:?}"
            );
        }
    }

    #[test]
    fn prop_replace_url_database_preserves_authority_and_query(
        old_db in "[a-z][a-z0-9_]{2,18}",
        new_db in "[a-z][a-z0-9_]{2,18}",
        query in "[a-z]{2,8}=[a-z0-9]{1,8}",
    ) {
        let src = format!("postgres://u:pw@127.0.0.1:55432/{old_db}?{query}");
        let expected = format!("postgres://u:pw@127.0.0.1:55432/{new_db}?{query}");
        prop_assert_eq!(replace_url_database(&src, &new_db), Ok(expected));
    }
}
