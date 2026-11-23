//! Live `agy acp` (ACP v1) D0 compatibility risk probes (A01, A03, A05, A06).
//!
//! Marked `#[ignore]` so offline unit/integration runs stay deterministic; execute via:
//! `uv run --project ops/runner dennett-check runtime-probe`

use std::env;
use std::fs;
use std::time::Duration;

use dennett_contracts::{DennettError, RuntimeAuthProfile};
use dennett_testkit::acp_probe::{
    AcpClient, AcpSpawnConfig, MIN_AGY_VERSION, McpEnvEntry, McpServerSpec, PermissionDecision,
    detect_agy_version, resolve_agy_binary,
};
use dennett_testkit::profile::{FaultInjectionGuard, TEST_PROFILE_ENV, TestProfile};
use uuid::Uuid;

const LIVE_PROBE_ENV: &str = "DENNETT_LIVE_ACP_PROBE";
const PROBE_MODEL_ENV: &str = "DENNETT_D0_PROBE_MODEL";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const TURN_TIMEOUT: Duration = Duration::from_secs(90);

fn require_live_probe_guard(
    label: &str,
) -> (
    TestProfile,
    dennett_testkit::profile::TempProfileDir,
    FaultInjectionGuard,
) {
    assert_eq!(
        env::var(TEST_PROFILE_ENV).ok().as_deref(),
        Some("1"),
        "{TEST_PROFILE_ENV}=1 is required for live ACP probes"
    );
    assert_eq!(
        env::var(LIVE_PROBE_ENV).ok().as_deref(),
        Some("1"),
        "{LIVE_PROBE_ENV}=1 is required for live ACP probes"
    );

    let (profile, temp_dir) = match TestProfile::create_on_disk(label) {
        Ok(pair) => pair,
        Err(err) => panic!("failed to create isolated probe directory: {err}"),
    };
    let guard = match FaultInjectionGuard::acquire(&profile) {
        Ok(g) => g,
        Err(err) => panic!("failed to acquire FaultInjectionGuard: {err}"),
    };
    (profile, temp_dir, guard)
}

#[tokio::test]
#[ignore = "live agy acp D0 compatibility probe; run via dennett-check runtime-probe"]
async fn acp_handshake_and_isolated_api_key_missing_fails_closed() {
    let (profile, _temp_dir, _guard) = require_live_probe_guard("acp-auth-iso");
    let binary = resolve_agy_binary();

    let version = match detect_agy_version(&binary).await {
        Ok(v) => v,
        Err(err) => panic!("agy version probe failed: {err}"),
    };
    assert!(
        version >= MIN_AGY_VERSION,
        "agy version {version} must be >= {MIN_AGY_VERSION}"
    );

    // Prepare an isolated Gemini API-key profile with NO GEMINI_API_KEY in env.
    // `--app-data-dir` MUST be a relative subdirectory name under `--gemini-dir`.
    let gemini_dir = profile.data_dir().join("gemini-isolated");
    let app_data_rel = "isolated-app";
    let app_data_dir = gemini_dir.join(app_data_rel);
    if let Err(err) = fs::create_dir_all(&app_data_dir) {
        panic!("failed creating isolated gemini/app dirs: {err}");
    }

    let settings_payload = r#"{"modelProvider":"gemini"}"#;
    for target in [
        gemini_dir.join("settings.json"),
        app_data_dir.join("settings.json"),
    ] {
        if let Err(err) = fs::write(&target, settings_payload) {
            panic!(
                "failed writing isolated settings.json at {}: {err}",
                target.display()
            );
        }
    }

    let mut cfg = AcpSpawnConfig::new(profile.workspace_dir().to_path_buf());
    cfg.binary = binary;
    cfg.extra_args = vec![
        String::from("--gemini-dir"),
        gemini_dir.to_string_lossy().into_owned(),
        String::from("--app-data-dir"),
        String::from(app_data_rel),
        String::from("--include-default-customizations=false"),
    ];

    let mut client = match AcpClient::spawn(&cfg) {
        Ok(c) => c,
        Err(err) => panic!("failed spawning isolated agy acp: {err}"),
    };

    let init = match client.initialize(HANDSHAKE_TIMEOUT).await {
        Ok(r) => r,
        Err(err) => panic!(
            "ACP initialize failed: {err}; stderr: {}",
            client.stderr_snapshot()
        ),
    };
    assert_eq!(init.protocol_version, 1, "expected ACP protocolVersion 1");
    assert!(
        init.agent_name.contains("Antigravity"),
        "expected Antigravity ACP Server, got `{}`",
        init.agent_name
    );
    assert!(
        init.auth_method_ids.iter().any(|m| m == "oauth-personal"),
        "expected oauth-personal auth method in {:?}",
        init.auth_method_ids
    );
    assert!(
        init.auth_method_ids.iter().any(|m| m == "gemini-api-key"),
        "expected gemini-api-key auth method in {:?}",
        init.auth_method_ids
    );

    let auth_res = client
        .authenticate(RuntimeAuthProfile::GeminiApiKey, HANDSHAKE_TIMEOUT)
        .await;
    assert!(
        matches!(
            auth_res,
            Err(DennettError::RuntimeProtocol {
                code: Some(-32602),
                ..
            })
        ),
        "expected JSON-RPC -32602 when authenticating gemini-api-key without GEMINI_API_KEY, got: {auth_res:?}"
    );

    let sess_res = client
        .session_new(profile.workspace_dir(), &[], HANDSHAKE_TIMEOUT)
        .await;
    assert!(
        matches!(
            sess_res,
            Err(DennettError::RuntimeProtocol {
                code: Some(-32000),
                ..
            })
        ),
        "expected JSON-RPC -32000 (Authentication required) without OAuth fallback, got: {sess_res:?}"
    );

    let _ = client.shutdown().await;
}

fn select_probe_model(available_models: &[String]) -> Option<String> {
    if let Ok(explicit) = env::var(PROBE_MODEL_ENV)
        && !explicit.trim().is_empty()
    {
        return Some(explicit.trim().to_owned());
    }
    available_models
        .iter()
        .find(|m| m.to_ascii_lowercase().contains("flash"))
        .cloned()
}

async fn assert_mcp_memory_read_allowed(
    client: &mut AcpClient,
    session_id: &str,
    expected_token: &str,
) {
    let mcp_prompt = "Call the MCP tool `memory_probe_read` with key \"d0-canary\" and scope \"project-alpha\". \
                      After receiving the tool result, reply with ONLY the exact token string and nothing else.";
    let mcp_outcome = match client
        .prompt(session_id, mcp_prompt, TURN_TIMEOUT, |req| {
            let text_hint =
                format!("{} {} {}", req.title, req.kind, req.raw_input).to_ascii_lowercase();
            if text_hint.contains("memory_probe_read") || text_hint.contains("dennett-memory-probe")
            {
                PermissionDecision::AllowOnce
            } else {
                PermissionDecision::RejectOnce
            }
        })
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("MCP memory tool prompt failed: {err}"),
    };

    assert!(
        mcp_outcome.text.contains(expected_token),
        "agent reply did not contain expected MCP memory token `{expected_token}`; reply=`{}`; tools={:?}; perms={:?}",
        mcp_outcome.text,
        mcp_outcome.tool_events,
        mcp_outcome.permission_events
    );
    assert!(
        !mcp_outcome.permission_events.is_empty() && !mcp_outcome.tool_events.is_empty(),
        "expected BOTH permission callback and tool_call events for MCP memory invocation; perms={:?}; tools={:?}",
        mcp_outcome.permission_events,
        mcp_outcome.tool_events
    );
    assert!(
        mcp_outcome.permission_events.iter().any(|ev| {
            ev.decision == PermissionDecision::AllowOnce && ev.chosen_option_id.is_some()
        }),
        "expected explicit AllowOnce selection for MCP memory read; perms={:?}",
        mcp_outcome.permission_events
    );
}

async fn assert_shell_command_rejected(
    client: &mut AcpClient,
    session_id: &str,
    workspace_dir: &std::path::Path,
) {
    let forbidden_file = workspace_dir.join("must_not_be_created.txt");
    let deny_prompt = "Use your shell / run_command tool right now to create a file named \
                       `must_not_be_created.txt` containing the text `blocked` in the current working directory. \
                       Do not use write_file; execute a shell command.";
    let deny_outcome = match client
        .prompt(session_id, deny_prompt, TURN_TIMEOUT, |_req| {
            PermissionDecision::RejectOnce
        })
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("denied command prompt failed: {err}"),
    };

    assert!(
        deny_outcome.permission_events.iter().any(|ev| {
            ev.decision == PermissionDecision::RejectOnce && ev.chosen_option_id.is_some()
        }),
        "expected at least one RejectOnce permission decision; observed perms={:?}; tools={:?}; text={}",
        deny_outcome.permission_events,
        deny_outcome.tool_events,
        deny_outcome.text
    );
    assert!(
        !forbidden_file.exists(),
        "denied shell command must not create `{}`",
        forbidden_file.display()
    );
}

#[tokio::test]
#[ignore = "live agy acp D0 compatibility probe; run via dennett-check runtime-probe"]
async fn acp_live_mcp_memory_tool_permission_and_cancel() {
    let (profile, _temp_dir, _guard) = require_live_probe_guard("acp-live-mcp");
    let fixture_bin = env!("CARGO_BIN_EXE_dennett-d0-mcp-fixture");
    let expected_token = format!(
        "D0_MEM_TOKEN_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );

    let cfg = AcpSpawnConfig::new(profile.workspace_dir().to_path_buf());
    let mut client = match AcpClient::spawn(&cfg) {
        Ok(c) => c,
        Err(err) => panic!("failed spawning agy acp: {err}"),
    };

    let init = match client.initialize(HANDSHAKE_TIMEOUT).await {
        Ok(r) => r,
        Err(err) => panic!(
            "ACP initialize failed: {err}; stderr: {}",
            client.stderr_snapshot()
        ),
    };
    assert_eq!(init.protocol_version, 1);

    let mcp_servers = vec![McpServerSpec {
        name: String::from("dennett-memory-probe"),
        command: String::from(fixture_bin),
        args: Vec::new(),
        env: vec![
            McpEnvEntry {
                name: String::from("DENNETT_PROBE_MEMORY_TOKEN"),
                value: expected_token.clone(),
            },
            McpEnvEntry {
                name: String::from("DENNETT_PROBE_ALLOWED_SCOPE"),
                value: String::from("project-alpha"),
            },
        ],
    }];

    let session = match client
        .session_new(profile.workspace_dir(), &mcp_servers, HANDSHAKE_TIMEOUT)
        .await
    {
        Ok(s) => s,
        Err(err) => panic!(
            "ACP session/new failed: {err}; stderr: {}",
            client.stderr_snapshot()
        ),
    };
    assert!(
        !session.session_id.is_empty(),
        "sessionId must be non-empty"
    );
    assert!(
        session
            .tier
            .as_deref()
            .is_some_and(|t| !t.trim().is_empty()),
        "expected non-empty `_meta.antigravity.tier` confirming interactive oauth-personal subscription profile, got: {:?}",
        session.tier
    );

    // Choose model dynamically per M.1 (never hardcode a model ID or panic if a specific one disappears).
    if let Some(model_id) = select_probe_model(&session.available_models)
        && let Err(err) = client
            .set_config_option(&session.session_id, "model", &model_id, HANDSHAKE_TIMEOUT)
            .await
    {
        panic!("failed setting selected ACP model `{model_id}`: {err}");
    }

    assert_mcp_memory_read_allowed(&mut client, &session.session_id, &expected_token).await;
    assert_shell_command_rejected(&mut client, &session.session_id, profile.workspace_dir()).await;

    let cancel_prompt = "Write a detailed numbered list from 1 to 400 where every item is a full \
                         two-sentence description of a distinct fictional exoplanet. Do not shorten or omit items.";
    let cancel_outcome = match client
        .prompt_and_cancel_on_first_update(
            &session.session_id,
            cancel_prompt,
            TURN_TIMEOUT,
            |_req| PermissionDecision::RejectOnce,
        )
        .await
    {
        Ok(o) => o,
        Err(err) => panic!("session/cancel probe failed: {err}"),
    };
    assert_eq!(
        cancel_outcome.stop_reason, "cancelled",
        "expected stopReason=`cancelled` after session/cancel; got `{}`",
        cancel_outcome.stop_reason
    );

    let _ = client.shutdown().await;
}
