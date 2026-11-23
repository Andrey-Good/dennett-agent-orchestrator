//! Standalone `dennett-memoryd` process integration tests (running without Head).
//!
//! Requires `DENNETT_TEST_PROFILE=1` and `DENNETT_TEST_MEMORY_PG_URL`.

use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use dennett_contracts::{ProfileMode, ServiceHealth, ServiceKind};
use dennett_testkit::profile::{
    DEFAULT_TEST_MEMORY_DB, DEFAULT_TEST_MEMORY_ROLE, TEST_PROFILE_ENV,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time;

fn require_memory_pg_url() -> String {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for memoryd integration tests"
    );
    match env::var("DENNETT_TEST_MEMORY_PG_URL") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => panic!("DENNETT_TEST_MEMORY_PG_URL is missing; run via `dennett-check integration`"),
    }
}

fn find_binary_near_current_exe(exe_name: &str) -> Option<PathBuf> {
    let current = env::current_exe().ok()?;
    for dir in current.ancestors().skip(1).take(6) {
        let candidate = dir.join(exe_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn locate_memoryd_binary() -> PathBuf {
    if let Some(explicit) = env::var_os("DENNETT_MEMORYD_BIN").filter(|v| !v.is_empty()) {
        return PathBuf::from(explicit);
    }

    let exe_name = if cfg!(windows) {
        "dennett-memoryd.exe"
    } else {
        "dennett-memoryd"
    };

    if let Some(found) = find_binary_near_current_exe(exe_name) {
        return found;
    }

    let status = std::process::Command::new("cargo")
        .args(["build", "-p", "dennett-memoryd", "--bin", "dennett-memoryd"])
        .status();
    assert!(
        matches!(status, Ok(s) if s.success()),
        "failed to build `dennett-memoryd` binary for process integration tests"
    );

    if let Some(found) = find_binary_near_current_exe(exe_name) {
        return found;
    }

    panic!("could not locate compiled `dennett-memoryd` binary");
}

async fn read_listening_addr(child: &mut Child) -> String {
    let Some(stdout) = child.stdout.take() else {
        panic!("memoryd stdout was not captured");
    };
    let mut reader = BufReader::new(stdout);
    let mut listen_line = String::new();
    match time::timeout(Duration::from_secs(5), reader.read_line(&mut listen_line)).await {
        Ok(Ok(n)) if n > 0 => {}
        other => panic!("did not receive LISTENING line from memoryd: {other:?}"),
    }
    match listen_line.trim().strip_prefix("LISTENING ") {
        Some(a) => a.to_owned(),
        None => panic!("unexpected startup line from memoryd: `{listen_line}`"),
    }
}

async fn send_http_request(addr: &str, raw_request: &str) -> (u16, String) {
    let mut stream = match time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => panic!("failed connecting to memoryd at {addr}: {err}"),
        Err(elapsed) => panic!("timed out connecting to memoryd at {addr}: {elapsed}"),
    };

    if let Err(err) = stream.write_all(raw_request.as_bytes()).await {
        panic!("failed writing HTTP request to memoryd: {err}");
    }
    let mut raw_resp = Vec::new();
    match time::timeout(Duration::from_secs(5), stream.read_to_end(&mut raw_resp)).await {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => panic!("failed reading HTTP response from memoryd: {err}"),
        Err(elapsed) => panic!("timed out reading HTTP response from memoryd: {elapsed}"),
    }

    let text = String::from_utf8_lossy(&raw_resp);
    let Some((headers, body)) = text.split_once("\r\n\r\n") else {
        panic!("malformed HTTP response from memoryd: {text}");
    };
    let status_line = headers.lines().next().unwrap_or_default();
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("could not parse HTTP status line `{status_line}`"));
    (code, body.to_owned())
}

fn shutdown_request(addr: &str) -> String {
    format!(
        "POST /shutdown HTTP/1.1\r\nHost: {addr}\r\nX-Dennett-Shutdown: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

#[tokio::test]
async fn memoryd_health_once_succeeds_standalone_against_postgres() {
    let pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    let output = match Command::new(&bin)
        .arg("--health-once")
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_TEST_MEMORY_PG_URL", &pg_url)
        .env_remove("DENNETT_MEMORY_PG_URL")
        .output()
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("failed running `{}`: {err}", bin.display()),
    };

    assert!(
        output.status.success(),
        "dennett-memoryd --health-once failed ({}): stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let health: ServiceHealth = match serde_json::from_str(stdout.trim()) {
        Ok(h) => h,
        Err(err) => panic!("invalid ServiceHealth JSON `{stdout}`: {err}"),
    };

    assert_eq!(health.service, ServiceKind::Memoryd);
    assert_eq!(health.stage, "D0");
    assert_eq!(health.profile_mode, ProfileMode::TestIsolated);
    assert!(
        !health.memory_fabric_implemented,
        "D0 scaffold must honestly report memory_fabric_implemented=false"
    );
    assert!(health.postgres_connected);
    assert_eq!(
        health.postgres_role.as_deref(),
        Some(DEFAULT_TEST_MEMORY_ROLE)
    );
    assert_eq!(
        health.postgres_database.as_deref(),
        Some(DEFAULT_TEST_MEMORY_DB)
    );
}

#[tokio::test]
async fn memoryd_serve_healthz_and_graceful_loopback_shutdown() {
    let pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    let mut child = match Command::new(&bin)
        .args(["--serve", "127.0.0.1:0"])
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_TEST_MEMORY_PG_URL", &pg_url)
        .env_remove("DENNETT_MEMORY_PG_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning dennett-memoryd --serve: {err}"),
    };

    let addr = read_listening_addr(&mut child).await;
    let req = format!(
        "GET /healthz HTTP/1.1\r\nHost: {addr}\r\nX-Correlation-Id: d0-live-healthz\r\nConnection: close\r\n\r\n"
    );
    let (status, body) = send_http_request(&addr, &req).await;
    assert_eq!(status, 200, "GET /healthz failed with body: {body}");

    let health: ServiceHealth = match serde_json::from_str(&body) {
        Ok(h) => h,
        Err(err) => panic!("invalid /healthz JSON `{body}`: {err}"),
    };
    assert_eq!(health.service, ServiceKind::Memoryd);
    assert!(!health.memory_fabric_implemented);
    assert!(health.postgres_connected);
    assert_eq!(health.correlation_id.as_str(), "d0-live-healthz");

    let (shutdown_status, shutdown_body) = send_http_request(&addr, &shutdown_request(&addr)).await;
    assert_eq!(
        shutdown_status, 200,
        "POST /shutdown failed: {shutdown_body}"
    );

    let exit = match time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(s)) => s,
        other => panic!("memoryd did not exit cleanly after /shutdown: {other:?}"),
    };
    assert!(
        exit.success(),
        "memoryd exited with non-zero status: {exit}"
    );
}

#[tokio::test]
async fn memoryd_shutdown_requires_csrf_header_and_is_disabled_in_production() {
    let pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    // 1. In test_isolated mode, POST /shutdown without X-Dennett-Shutdown: 1 or with non-loopback Host -> 403.
    let mut iso_child = match Command::new(&bin)
        .args(["--serve", "127.0.0.1:0"])
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_TEST_MEMORY_PG_URL", &pg_url)
        .env_remove("DENNETT_MEMORY_PG_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning isolated memoryd: {err}"),
    };

    let iso_addr = read_listening_addr(&mut iso_child).await;
    let no_header_req = format!(
        "POST /shutdown HTTP/1.1\r\nHost: {iso_addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let (status_no_hdr, body_no_hdr) = send_http_request(&iso_addr, &no_header_req).await;
    assert_eq!(
        status_no_hdr, 403,
        "POST /shutdown without X-Dennett-Shutdown must return 403, got {status_no_hdr}: {body_no_hdr}"
    );

    let bad_host_req = "POST /shutdown HTTP/1.1\r\nHost: evil.example:18080\r\nX-Dennett-Shutdown: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    let (status_bad_host, _) = send_http_request(&iso_addr, bad_host_req).await;
    assert_eq!(
        status_bad_host, 403,
        "POST /shutdown with non-loopback Host must return 403"
    );

    // Server must still be alive after rejected shutdown attempts.
    let health_req =
        format!("GET /healthz HTTP/1.1\r\nHost: {iso_addr}\r\nConnection: close\r\n\r\n");
    let (health_status, _) = send_http_request(&iso_addr, &health_req).await;
    assert_eq!(health_status, 200);

    let (ok_shutdown, _) = send_http_request(&iso_addr, &shutdown_request(&iso_addr)).await;
    assert_eq!(ok_shutdown, 200);
    let _ = time::timeout(Duration::from_secs(5), iso_child.wait()).await;

    // 2. In production mode (without DENNETT_TEST_PROFILE=1), POST /shutdown must return 404.
    let mut prod_child = match Command::new(&bin)
        .args(["--serve", "127.0.0.1:0", "--pg-url", &pg_url])
        .env_remove(TEST_PROFILE_ENV)
        .env_remove("DENNETT_MEMORY_PG_URL")
        .env_remove("DENNETT_TEST_MEMORY_PG_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning production-mode memoryd: {err}"),
    };

    let prod_addr = read_listening_addr(&mut prod_child).await;
    let (prod_shutdown, _) = send_http_request(&prod_addr, &shutdown_request(&prod_addr)).await;
    assert_eq!(
        prod_shutdown, 404,
        "POST /shutdown must be disabled (404) outside test_isolated mode"
    );
    let _ = prod_child.kill().await;
    let _ = prod_child.wait().await;
}

#[tokio::test]
async fn memoryd_health_once_fails_closed_without_leaking_db_password() {
    let _pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();
    let secret_pw = "D0_SENSITIVE_PG_PASS_948271";
    let bad_url =
        format!("postgres://dennett_memory_role:{secret_pw}@127.0.0.1:1/dennett_memory_test");

    let output = match Command::new(&bin)
        .args(["--health-once", "--pg-url", &bad_url])
        .env(TEST_PROFILE_ENV, "1")
        .env_remove("DENNETT_MEMORY_PG_URL")
        .env_remove("DENNETT_TEST_MEMORY_PG_URL")
        .output()
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("failed running memoryd negative probe: {err}"),
    };

    assert!(
        !output.status.success(),
        "memoryd --health-once must exit non-zero when PostgreSQL is unreachable"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(secret_pw) && !stderr.contains(secret_pw),
        "PostgreSQL password leaked! stdout=`{stdout}` stderr=`{stderr}`"
    );

    let err_json: Value = match serde_json::from_str(stderr.trim()) {
        Ok(v) => v,
        Err(err) => panic!("stderr must be structured DennettError JSON, got `{stderr}`: {err}"),
    };
    assert_eq!(
        err_json.get("kind").and_then(Value::as_str),
        Some("database_unavailable"),
        "expected DatabaseUnavailable error kind, got: {err_json}"
    );
}

#[tokio::test]
async fn memoryd_serve_returns_503_and_redacts_password_when_pg_unreachable() {
    let _pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();
    let secret_pw = "D0_SERVE_SECRET_PW_314159";
    let bad_url =
        format!("postgres://dennett_memory_role:{secret_pw}@127.0.0.1:1/dennett_memory_test");

    let mut child = match Command::new(&bin)
        .args(["--serve", "127.0.0.1:0", "--pg-url", &bad_url])
        .env(TEST_PROFILE_ENV, "1")
        .env_remove("DENNETT_MEMORY_PG_URL")
        .env_remove("DENNETT_TEST_MEMORY_PG_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning memoryd with unreachable PG: {err}"),
    };

    let addr = read_listening_addr(&mut child).await;
    let req = format!("GET /healthz HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    let (status, body) = send_http_request(&addr, &req).await;
    assert_eq!(
        status, 503,
        "GET /healthz must return 503 when PG is down, got {status}: {body}"
    );
    assert!(
        !body.contains(secret_pw),
        "503 response leaked DB password: {body}"
    );

    let parsed: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(err) => panic!("invalid 503 JSON `{body}`: {err}"),
    };
    assert_eq!(parsed["postgres_connected"], Value::Bool(false));
    assert_eq!(parsed["memory_fabric_implemented"], Value::Bool(false));
    assert_eq!(
        parsed["error"]["kind"],
        Value::String(String::from("database_unavailable"))
    );

    let (shutdown_status, _) = send_http_request(&addr, &shutdown_request(&addr)).await;
    assert_eq!(shutdown_status, 200);
    let _ = time::timeout(Duration::from_secs(5), child.wait()).await;
}

#[tokio::test]
async fn h0_01_memoryd_serve_arg_with_multibyte_escape_exits_cleanly_without_panic() {
    let _pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    let output = match Command::new(&bin)
        .args(["--serve", "password='\\я'"])
        .env(TEST_PROFILE_ENV, "1")
        .output()
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("failed running memoryd CLI UTF-8 probe: {err}"),
    };

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked"),
        "memoryd must never panic on multibyte escaped CLI args: {stderr}"
    );
    let err_json: Value = match serde_json::from_str(stderr.trim()) {
        Ok(v) => v,
        Err(err) => panic!("expected structured JSON error in stderr `{stderr}`: {err}"),
    };
    assert_eq!(
        err_json["kind"],
        Value::String(String::from("invalid_input"))
    );
}

#[tokio::test]
async fn h0_02_memoryd_isolated_rejects_dsn_override_exploits_and_prod_env_fallback() {
    let pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    for exploit_dsn in [
        "postgres://u:p@127.0.0.1:1/fake_test?dbname=production_db",
        "host=127.0.0.1 port=1 user=u password=p dbname=fake_test dbname=production_db",
    ] {
        let output = match Command::new(&bin)
            .args(["--health-once", "--pg-url", exploit_dsn])
            .env(TEST_PROFILE_ENV, "1")
            .env_remove("DENNETT_MEMORY_PG_URL")
            .env_remove("DENNETT_TEST_MEMORY_PG_URL")
            .output()
            .await
        {
            Ok(o) => o,
            Err(err) => panic!("failed running memoryd DSN exploit probe: {err}"),
        };

        assert!(
            !output.status.success(),
            "exploit DSN `{exploit_dsn}` must exit non-zero"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let err_json: Value = match serde_json::from_str(stderr.trim()) {
            Ok(v) => v,
            Err(err) => panic!("expected JSON error in stderr `{stderr}`: {err}"),
        };
        assert_eq!(
            err_json["kind"],
            Value::String(String::from("safety_guard_rejected")),
            "exploit DSN `{exploit_dsn}` must fail with safety_guard_rejected, got: {err_json}"
        );
    }

    // With DENNETT_TEST_PROFILE=1 and only DENNETT_MEMORY_PG_URL set, memoryd must NOT fall back.
    let no_fallback = match Command::new(&bin)
        .arg("--health-once")
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_MEMORY_PG_URL", &pg_url)
        .env_remove("DENNETT_TEST_MEMORY_PG_URL")
        .output()
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("failed running memoryd no-fallback probe: {err}"),
    };
    assert!(
        !no_fallback.status.success(),
        "TestIsolated must not fall back to DENNETT_MEMORY_PG_URL"
    );
    let nf_stderr = String::from_utf8_lossy(&no_fallback.stderr);
    let nf_json: Value = match serde_json::from_str(nf_stderr.trim()) {
        Ok(v) => v,
        Err(err) => panic!("expected JSON error in stderr `{nf_stderr}`: {err}"),
    };
    assert_eq!(
        nf_json["kind"],
        Value::String(String::from("database_unavailable"))
    );
}

#[tokio::test]
async fn h0_11_memoryd_rejects_large_body_shutdown_with_403_without_tcp_rst() {
    let pg_url = require_memory_pg_url();
    let bin = locate_memoryd_binary();

    let mut child = match Command::new(&bin)
        .args(["--serve", "127.0.0.1:0"])
        .env(TEST_PROFILE_ENV, "1")
        .env("DENNETT_TEST_MEMORY_PG_URL", &pg_url)
        .env_remove("DENNETT_MEMORY_PG_URL")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(err) => panic!("failed spawning memoryd for H0-11 body-drain probe: {err}"),
    };

    let addr = read_listening_addr(&mut child).await;
    let body = "A".repeat(4096);
    let req = format!(
        "POST /shutdown HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    for attempt in 0..5 {
        let (status, resp_body) = send_http_request(&addr, &req).await;
        assert_eq!(
            status, 403,
            "attempt {attempt}: expected clean 403 without Winsock TCP RST, got {status}: {resp_body}"
        );
    }

    let (shutdown_status, _) = send_http_request(&addr, &shutdown_request(&addr)).await;
    assert_eq!(shutdown_status, 200);
    let _ = time::timeout(Duration::from_secs(5), child.wait()).await;
}
