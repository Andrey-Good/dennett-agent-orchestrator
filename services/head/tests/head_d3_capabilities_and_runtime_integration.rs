//! Live PostgreSQL 16 + loopback HTTP integration tests for Phase D3:
//! - Multi-runtime switching (`SimulatedRuntime` <-> `LocalHttpRuntime`), V29 honest rehydrate,
//!   preservation of `AgentId`/`ProjectId`/`SessionId`/artifacts/history, `ProviderQuotaExceeded` (429),
//!   `AuthRequired` (401), SSRF loopback guard, read-only `planned_providers`, and zero silent fallback.
//! - Write-only `SecretBroker` ChaCha20-Poly1305 encryption at rest, rotation hot-invalidation,
//!   deletion invalidation, `GEMINI_API_KEY` env fallback, and zero plaintext leaks.
//! - `A08` Capabilities Registry: strictly read-only personal `~/.gemini/` discovery (tree SHA-256
//!   unchanged), candidate-unattached isolation, per-project activation binding, rug-pull trust reset
//!   to `Restricted`, 1-click rollback, and MCP `SecretBroker` env injection without leaks.
//! - `A07` Native subagents via `PolicyEngine` + `dennett-control-mcp` (`dct1.`) text cooperation
//!   and `child_scope ⊆ parent_scope` delegation enforcement.
//! - `V02` Owner Memory Fabric proxy graceful degradation (`GET /v1/memory/overview` -> `200 OK`
//!   `DegradedUnreachable`) and unblocked `Stop` while `memoryd` is unreachable.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use dennett_contracts::{
    ActivateConnectionRequest, ArtifactCompleteness, CapabilityActivation, CapabilityOrigin,
    CapabilityOwnership, CapabilityScopeKind, CapabilitySpec, CapabilityTrust,
    ConnectionActivationRole, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreateCapabilityRequest, CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest,
    CreateTaskRequest, DennettError, DiscoverCapabilitiesRequest, ForgetRequest, ForgetTargetType,
    IdempotencyKey, MemoryScope, MemoryStatus, PolicyEffect, ProfileMode, ProviderHealthStatus,
    ProviderKind, RuntimeAuthProfile, SendAgentMessageRequest, SendMessageRequest, SessionKind,
    SetProjectCapabilityBindingRequest, SetSecretRequest, UpdateAgentConfigRequest,
    UpdateCapabilityRequest, UpsertProviderConnectionRequest, WorkspaceWritePolicy,
};
use dennett_head::connections::SEEDED_OAUTH_PERSONAL_CONNECTION_ID;
use dennett_head::{
    HEAD_STAGE_LABEL, HeadAppState, HeadService, HeadServiceExtras, LATEST_CONTROL_SCHEMA_VERSION,
    MAX_CONCURRENT_REQUESTS, SECRET_AEAD_ALGORITHM, ValidatedControlPgTarget, build_head_router,
};
use dennett_head_client::HeadClient;
use dennett_memory_client::MemoryClient;
use dennett_runtime_antigravity::{
    AgentRuntimePort, AntigravityAcpRuntime, AntigravityRuntimeConfig, SimulatedRuntime,
};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio_postgres::NoTls;
use uuid::Uuid;

fn require_control_pg_target() -> String {
    assert_eq!(
        env::var("DENNETT_TEST_PROFILE").ok().as_deref(),
        Some("1"),
        "D3 PG16 integration tests must run under DENNETT_TEST_PROFILE=1"
    );
    env::var("DENNETT_TEST_CONTROL_PG_URL")
        .expect("DENNETT_TEST_CONTROL_PG_URL must be set for PG16 integration tests")
}

fn with_isolated_schema_url(base_url: &str, schema: &str) -> String {
    let sep = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{sep}options=-csearch_path%3D{schema}")
}

struct LiveHeadHarness {
    base_url: String,
    schema: String,
    iso_url: String,
    head_url: String,
    service: HeadService,
    client: HeadClient,
    shutdown_tx: watch::Sender<bool>,
    temp_root: PathBuf,
}

impl LiveHeadHarness {
    async fn start(
        prefix: &str,
        memory_client: Option<MemoryClient>,
        extras: HeadServiceExtras,
    ) -> Self {
        Self::start_with_runtime(
            prefix,
            Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal)),
            memory_client,
            extras,
        )
        .await
    }

    async fn start_with_runtime(
        prefix: &str,
        runtime: Arc<dyn AgentRuntimePort>,
        memory_client: Option<MemoryClient>,
        mut extras: HeadServiceExtras,
    ) -> Self {
        let base_url = require_control_pg_target();
        let short = &Uuid::new_v4().simple().to_string()[..12];
        let schema = format!("d3_{prefix}_{short}");

        let (pg_client, conn) = tokio_postgres::connect(&base_url, NoTls)
            .await
            .expect("connect control PG");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        pg_client
            .batch_execute(&format!("CREATE SCHEMA {schema}"))
            .await
            .expect("create isolated D3 schema");
        drop(pg_client);

        let iso_url = with_isolated_schema_url(&base_url, &schema);
        let iso_target = ValidatedControlPgTarget::resolve_from_sources(
            None,
            Some(&iso_url),
            None,
            ProfileMode::TestIsolated,
        )
        .expect("resolve isolated PG target")
        .expect("validated isolated PG target");

        let temp_root = env::temp_dir().join(format!("dennett-d3-it-{prefix}-{short}"));
        fs::create_dir_all(&temp_root).expect("create temp root");
        if extras.detached_projects_root.is_none() {
            extras.detached_projects_root = Some(temp_root.join("detached"));
        }
        if extras.vault_dir.is_none() {
            extras.vault_dir = Some(temp_root.join("vault"));
        }
        if extras.gemini_api_key_env_override.is_none() {
            extras.gemini_api_key_env_override = Some(None);
        }

        let service = HeadService::new_with_extras(
            iso_target,
            runtime,
            memory_client,
            None,
            ProfileMode::TestIsolated,
            Vec::new(),
            extras,
        )
        .expect("build HeadService");
        let schema_ver = service.ensure_migrated().await.expect("apply v1..v8");
        assert_eq!(schema_ver, LATEST_CONTROL_SCHEMA_VERSION);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback Head listener");
        let addr = listener.local_addr().expect("Head local addr");
        let head_url = format!("http://{addr}");
        service.set_self_http_url(&head_url);

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let state = HeadAppState {
            service: Some(service.clone()),
            profile_mode: ProfileMode::TestIsolated,
            runtime_adapter_id: service.runtime_adapter_id().to_owned(),
            runtime_is_simulated: service.runtime_is_simulated(),
            allow_non_loopback: false,
            owner_token: zeroize::Zeroizing::new(service.owner_token().to_owned()),
            shutdown_tx: shutdown_tx.clone(),
            request_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
        };
        let app = build_head_router(state);
        tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move {
                while !*shutdown_rx.borrow() {
                    if shutdown_rx.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await;
        });

        let client = HeadClient::new(&head_url)
            .expect("construct HeadClient")
            .with_owner_token(service.owner_token());
        Self {
            base_url,
            schema,
            iso_url,
            head_url,
            service,
            client,
            shutdown_tx,
            temp_root,
        }
    }

    async fn cleanup(self) {
        let _ = self.shutdown_tx.send(true);
        if let Ok((client, conn)) = tokio_postgres::connect(&self.base_url, NoTls).await {
            tokio::spawn(async move {
                let _ = conn.await;
            });
            let _ = client
                .batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema))
                .await;
        }
        let _ = fs::remove_dir_all(&self.temp_root);
    }
}

#[derive(Clone, Debug)]
enum MockLocalMode {
    Ok,
    AuthRequired,
    Quota429 { retry_after: u64 },
}

#[derive(Clone)]
struct MockLocalServerState {
    mode: Arc<StdMutex<MockLocalMode>>,
    expected_bearer: Arc<StdMutex<Option<String>>>,
    received_messages: Arc<StdMutex<Vec<JsonValue>>>,
}

impl MockLocalServerState {
    fn new(expected_bearer: Option<String>) -> Self {
        Self {
            mode: Arc::new(StdMutex::new(MockLocalMode::Ok)),
            expected_bearer: Arc::new(StdMutex::new(expected_bearer)),
            received_messages: Arc::new(StdMutex::new(Vec::new())),
        }
    }

    fn set_mode(&self, mode: MockLocalMode) {
        *self.mode.lock().expect("mode lock") = mode;
    }

    fn set_expected_bearer(&self, token: Option<String>) {
        *self.expected_bearer.lock().expect("bearer lock") = token;
    }

    fn check_auth(&self, headers: &HeaderMap) -> bool {
        let expected = self.expected_bearer.lock().expect("bearer lock").clone();
        let Some(exp) = expected else {
            return true;
        };
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|actual| actual == format!("Bearer {exp}"))
    }
}

async fn mock_models_handler(
    State(state): State<MockLocalServerState>,
    headers: HeaderMap,
) -> Response {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"message": "invalid bearer token"}})),
        )
            .into_response();
    }
    let mode = state.mode.lock().expect("mode lock").clone();
    match mode {
        MockLocalMode::AuthRequired => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"message": "local endpoint auth required"}})),
        )
            .into_response(),
        MockLocalMode::Quota429 { retry_after } => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            Json(json!({"error": {"message": "provider quota exceeded on local proxy"}})),
        )
            .into_response(),
        MockLocalMode::Ok => (
            StatusCode::OK,
            Json(json!({
                "object": "list",
                "data": [
                    {"id": "qwen2.5-coder:14b", "object": "model"},
                    {"id": "llama3.3:70b", "object": "model"}
                ]
            })),
        )
            .into_response(),
    }
}

async fn mock_chat_handler(
    State(state): State<MockLocalServerState>,
    headers: HeaderMap,
    Json(body): Json<JsonValue>,
) -> Response {
    if !state.check_auth(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"message": "missing or invalid local bearer token"}})),
        )
            .into_response();
    }
    let mode = state.mode.lock().expect("mode lock").clone();
    match mode {
        MockLocalMode::AuthRequired => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": {"message": "local endpoint rejected credentials"}})),
        )
            .into_response(),
        MockLocalMode::Quota429 { retry_after } => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            Json(json!({"error": {"message": "RESOURCE_EXHAUSTED: provider quota limit hit"}})),
        )
            .into_response(),
        MockLocalMode::Ok => {
            if let Some(msgs) = body.get("messages").cloned() {
                state
                    .received_messages
                    .lock()
                    .expect("msgs lock")
                    .push(msgs);
            }
            let model = body
                .get("model")
                .and_then(JsonValue::as_str)
                .unwrap_or("qwen2.5-coder:14b");
            let msg_count = body
                .get("messages")
                .and_then(JsonValue::as_array)
                .map_or(0, Vec::len);
            (
                StatusCode::OK,
                Json(json!({
                    "id": "chatcmpl-d3-local-1",
                    "object": "chat.completion",
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": format!("LOCAL_HTTP_REPLY model={model} msgs={msg_count}")
                        },
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 24,
                        "completion_tokens": 12,
                        "total_tokens": 36
                    }
                })),
            )
                .into_response()
        }
    }
}

async fn spawn_mock_local_llm(expected_bearer: Option<String>) -> (String, MockLocalServerState) {
    let state = MockLocalServerState::new(expected_bearer);
    let router = Router::new()
        .route("/v1/models", get(mock_models_handler))
        .route("/v1/chat/completions", post(mock_chat_handler))
        .with_state(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock local LLM");
    let addr = listener.local_addr().expect("mock local addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (format!("http://{addr}"), state)
}

fn hash_directory_tree(root: &Path) -> String {
    let mut entries = Vec::new();
    collect_tree_entries(root, root, &mut entries);
    entries.sort();
    let mut hasher = Sha256::new();
    for (rel, bytes) in entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update(&bytes);
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        use core::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

fn collect_tree_entries(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            out.push((format!("D:{rel}"), Vec::new()));
            collect_tree_entries(root, &path, out);
        } else if path.is_file() {
            let bytes = fs::read(&path).unwrap_or_default();
            out.push((format!("F:{rel}"), bytes));
        }
    }
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end verification of multi-runtime switching, V29 rehydrate, 401/429 errors, SSRF guard, and zero silent fallback"
)]
async fn d3_multi_runtime_switch_v29_rehydrate_and_zero_silent_fallback() {
    let harness = LiveHeadHarness::start("runtime", None, HeadServiceExtras::default()).await;
    let (local_url, mock_state) = spawn_mock_local_llm(None).await;

    let health = harness.client.healthz(None).await.expect("healthz");
    assert_eq!(health.stage, HEAD_STAGE_LABEL);
    assert_eq!(health.schema_version, Some(LATEST_CONTROL_SCHEMA_VERSION));

    // 1. Seeded Antigravity connections + read-only planned_providers (R-CLAUDE, R-CODEX).
    let conn_list = harness
        .client
        .list_provider_connections(None)
        .await
        .expect("list connections");
    assert!(
        conn_list.connections.iter().any(
            |c| c.auth_profile == RuntimeAuthProfile::OAuthPersonal && c.is_default_interactive
        )
    );
    assert!(
        conn_list
            .connections
            .iter()
            .any(|c| c.auth_profile == RuntimeAuthProfile::GeminiApiKey && c.is_default_background)
    );
    assert_eq!(conn_list.planned_providers.len(), 2);
    assert!(
        conn_list
            .planned_providers
            .iter()
            .all(|p| p.status == "planned")
    );
    assert!(
        conn_list
            .planned_providers
            .iter()
            .any(|p| p.id == "r-claude")
    );
    assert!(
        conn_list
            .planned_providers
            .iter()
            .any(|p| p.id == "r-codex")
    );

    // 2. Non-loopback LocalHttp URL rejected by default SSRF guard!
    let ssrf_err = harness
        .client
        .create_provider_connection(
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::LocalHttp,
                display_name: String::from("Remote SSRF"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::LocalEndpoint,
                endpoint_url: Some(String::from("https://api.external.example:8443")),
                secret_slot_key: None,
                is_default_interactive: None,
                is_default_background: None,
                enabled: Some(true),
            },
            None,
        )
        .await
        .expect_err("non-loopback local runtime must be rejected by default");
    assert!(matches!(ssrf_err, DennettError::SafetyGuardRejected { .. }));

    // 3. Create agent, project, session, artifact, and run Turn 1 on default SimulatedRuntime(OAuthPersonal).
    let scope = MemoryScope::try_new("project:d3-switch").expect("scope");
    let ws_dir = harness.temp_root.join("ws-switch");
    fs::create_dir_all(&ws_dir).expect("create ws");
    let project = harness
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Switch Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Polyglot"),
                description: String::from("Multi-runtime agent"),
                system_prompt: String::from("Be concise."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let session = harness
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Runtime Switch Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");
    let artifact = harness
        .client
        .create_artifact(
            session.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("design.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# Design stays intact across runtimes")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("create artifact");

    let turn1 = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Remember token ALPHA-TURN-1"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn 1 on simulated Antigravity");
    assert_eq!(
        turn1.runtime_binding.auth_profile,
        RuntimeAuthProfile::OAuthPersonal
    );
    assert!(!turn1.run.rehydrated);

    // 4. Register + probe live LocalHttp connection.
    let local_conn = harness
        .client
        .create_provider_connection(
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::LocalHttp,
                display_name: String::from("Local Mock Ollama"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::LocalEndpoint,
                endpoint_url: Some(local_url.clone()),
                secret_slot_key: None,
                is_default_interactive: Some(false),
                is_default_background: Some(false),
                enabled: Some(true),
            },
            None,
        )
        .await
        .expect("create local connection");
    let probe = harness
        .client
        .probe_provider_connection(local_conn.connection_id, None)
        .await
        .expect("probe local connection");
    assert_eq!(
        probe.connection.last_health_status,
        ProviderHealthStatus::Healthy
    );
    let catalog = probe.catalog.expect("live local catalog");
    assert!(!catalog.is_simulated);
    assert_eq!(catalog.auth_profile, RuntimeAuthProfile::LocalEndpoint);
    assert!(
        catalog
            .models
            .iter()
            .any(|m| m.model_id == "qwen2.5-coder:14b")
    );

    // 5. Switch agent to LocalHttp connection -> creates config v2 while preserving AgentId!
    let updated_agent = harness
        .client
        .update_agent_config(
            agent.agent_id,
            &UpdateAgentConfigRequest {
                system_prompt: String::from("Be concise on local runtime."),
                model_preference: Some(String::from("qwen2.5-coder:14b")),
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: Some(local_conn.connection_id),
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("update agent preferred connection");
    assert_eq!(updated_agent.agent_id, agent.agent_id);
    assert_eq!(updated_agent.current_config.version_number, 2);
    assert_eq!(
        updated_agent.current_config.preferred_connection_id,
        Some(local_conn.connection_id)
    );

    // 6. Send Turn 2 on SAME SessionId -> honest V29 rehydrate onto LocalHttpRuntime!
    let turn2 = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Continue after switching engine"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn 2 on LocalHttpRuntime");
    assert!(
        turn2.run.rehydrated,
        "V29 must explicitly rehydrate on provider switch"
    );
    assert_eq!(turn2.runtime_binding.adapter_id, "local-http-runtime");
    assert_eq!(
        turn2.runtime_binding.auth_profile,
        RuntimeAuthProfile::LocalEndpoint
    );
    assert_eq!(
        turn2.runtime_binding.connection_id,
        Some(local_conn.connection_id)
    );
    assert!(turn2.runtime_binding.endpoint_fingerprint.is_some());
    assert!(turn2.runtime_binding.rehydrate_count >= 1);
    assert!(
        turn2
            .assistant_message
            .as_ref()
            .expect("assistant msg")
            .content
            .contains("LOCAL_HTTP_REPLY model=qwen2.5-coder:14b")
    );
    assert!(
        turn2
            .artifacts
            .iter()
            .any(|a| a.artifact_id == artifact.artifact_id),
        "artifacts must remain attached to the same session across runtime switches"
    );

    // Verify Turn 1 history was injected during V29 rehydrate into the local LLM request!
    let sent_payloads = mock_state.received_messages.lock().expect("msgs").clone();
    let payload_text = serde_json::to_string(&sent_payloads).unwrap_or_default();
    assert!(
        payload_text.contains("ALPHA-TURN-1"),
        "V29 rehydrate must preserve prior Dennett conversation history: {payload_text}"
    );

    // 7. Zero silent fallback: 401 -> AuthRequired (NEVER silently falls back to Antigravity!).
    mock_state.set_mode(MockLocalMode::AuthRequired);
    let auth_err = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Should fail auth"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect_err("401 on LocalHttpRuntime must fail closed without silent fallback");
    assert!(matches!(
        auth_err,
        DennettError::AuthRequired {
            profile: RuntimeAuthProfile::LocalEndpoint,
            ..
        }
    ));

    // 8. Zero silent fallback: 429 -> ProviderQuotaExceeded (HTTP 429, retry_after preserved).
    mock_state.set_mode(MockLocalMode::Quota429 { retry_after: 42 });
    let quota_err = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Should hit provider quota"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect_err("429 on LocalHttpRuntime must map to ProviderQuotaExceeded");
    assert!(matches!(
        quota_err,
        DennettError::ProviderQuotaExceeded {
            retry_after_secs: Some(42),
            ..
        }
    ));

    // 9. Disabling preferred connection fails closed with Conflict (zero silent fallback).
    let _ = harness
        .client
        .update_provider_connection(
            local_conn.connection_id,
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::LocalHttp,
                display_name: String::from("Local Mock Ollama"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::LocalEndpoint,
                endpoint_url: Some(local_url),
                secret_slot_key: None,
                is_default_interactive: Some(false),
                is_default_background: Some(false),
                enabled: Some(false),
            },
            None,
        )
        .await
        .expect("disable connection");
    let disabled_err = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("Should fail disabled"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect_err("disabled preferred connection must fail closed");
    assert!(matches!(disabled_err, DennettError::Conflict { .. }));

    harness.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies SecretBroker ChaCha20-Poly1305 encryption at rest, write-only API, rotation hot-invalidation, deletion, and env fallback"
)]
async fn d3_secret_broker_write_only_encryption_rotation_and_env_fallback() {
    let env_secret = "AIzaSyEnvFallbackSecret999XYZ";
    let secret_v1 = "Bearer-SuperSecret-V1-8888";
    let secret_v2 = "Bearer-SuperSecret-V2-9999";

    let harness = LiveHeadHarness::start(
        "secrets",
        None,
        HeadServiceExtras {
            gemini_api_key_env_override: Some(Some(env_secret.to_owned())),
            ..HeadServiceExtras::default()
        },
    )
    .await;

    // 1. Env fallback metadata for `gemini_api_key` never returns plaintext.
    let initial = harness
        .client
        .list_secrets(None)
        .await
        .expect("list secrets");
    let gemini = initial
        .secrets
        .iter()
        .find(|s| s.slot_key == "gemini_api_key")
        .expect("gemini_api_key metadata present");
    assert!(gemini.configured);
    assert_eq!(gemini.source, dennett_contracts::SecretSource::EnvFallback);
    assert!(gemini.fingerprint_short.is_some());
    assert!(
        !serde_json::to_string(&initial)
            .unwrap_or_default()
            .contains(env_secret)
    );

    // 2. Store secret in vault via write-only PUT /v1/secrets/{slot_key}.
    let meta_v1 = harness
        .client
        .set_secret(
            "local_runtime_token",
            &SetSecretRequest {
                value: secret_v1.to_owned(),
                scope: String::from("global"),
                description: String::from("Local LLM token"),
            },
            None,
        )
        .await
        .expect("set secret v1");
    assert!(meta_v1.configured);
    assert_eq!(meta_v1.version, 1);
    assert_eq!(meta_v1.source, dennett_contracts::SecretSource::Vault);
    let fp1 = meta_v1.fingerprint_short.clone().expect("fingerprint v1");
    assert_eq!(fp1.len(), 16);

    // 3. Direct PG inspection proves AEAD encryption at rest (zero plaintext in DB!).
    let (pg_client, pg_conn) = tokio_postgres::connect(&harness.iso_url, NoTls)
        .await
        .expect("connect isolated schema");
    tokio::spawn(async move {
        let _ = pg_conn.await;
    });
    let row = pg_client
        .query_one(
            "SELECT algorithm, nonce, ciphertext FROM secret_entries WHERE slot_key = 'local_runtime_token'",
            &[],
        )
        .await
        .expect("fetch raw secret row");
    let alg: String = row.get(0);
    let nonce: Vec<u8> = row.get(1);
    let ciphertext: Vec<u8> = row.get(2);
    assert_eq!(alg, SECRET_AEAD_ALGORITHM);
    assert_eq!(nonce.len(), 12);
    assert!(!String::from_utf8_lossy(&ciphertext).contains("SuperSecret"));

    // 4. LocalHttpRuntime wired to `local_runtime_token` succeeds with v1, rotates hot to v2,
    //    and fails AuthRequired after DELETE!
    let (local_url, mock_state) = spawn_mock_local_llm(Some(secret_v1.to_owned())).await;
    let conn = harness
        .client
        .create_provider_connection(
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::LocalHttp,
                display_name: String::from("Authenticated Local"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::LocalEndpoint,
                endpoint_url: Some(local_url),
                secret_slot_key: Some(String::from("local_runtime_token")),
                is_default_interactive: None,
                is_default_background: None,
                enabled: Some(true),
            },
            None,
        )
        .await
        .expect("create auth local connection");

    let probe_v1 = harness
        .client
        .probe_provider_connection(conn.connection_id, None)
        .await
        .expect("probe v1");
    assert_eq!(
        probe_v1.connection.last_health_status,
        ProviderHealthStatus::Healthy
    );

    // Rotate secret to v2 & update server expectation -> hot invalidation picks up v2 immediately!
    let meta_v2 = harness
        .client
        .set_secret(
            "local_runtime_token",
            &SetSecretRequest {
                value: secret_v2.to_owned(),
                scope: String::from("global"),
                description: String::from("Rotated local LLM token"),
            },
            None,
        )
        .await
        .expect("rotate secret v2");
    assert_eq!(meta_v2.version, 2);
    assert_ne!(meta_v2.fingerprint_short.as_deref(), Some(fp1.as_str()));

    mock_state.set_expected_bearer(Some(secret_v2.to_owned()));
    let probe_v2 = harness
        .client
        .probe_provider_connection(conn.connection_id, None)
        .await
        .expect("probe v2");
    assert_eq!(
        probe_v2.connection.last_health_status,
        ProviderHealthStatus::Healthy
    );

    // Delete secret -> slot becomes Unset & probe fails with AuthRequired!
    let deleted_meta = harness
        .client
        .delete_secret("local_runtime_token", None)
        .await
        .expect("delete secret");
    assert!(!deleted_meta.configured);
    assert_eq!(deleted_meta.source, dennett_contracts::SecretSource::Unset);

    let probe_after_del = harness
        .client
        .probe_provider_connection(conn.connection_id, None)
        .await
        .expect("probe after delete");
    assert_eq!(
        probe_after_del.connection.last_health_status,
        ProviderHealthStatus::AuthRequired
    );

    // Verify ZERO plaintext leaks in GET /v1/secrets and GET /v1/diagnostics!
    let listed = harness.client.list_secrets(None).await.expect("list after");
    let diag = harness
        .client
        .get_diagnostics(Some(100), None)
        .await
        .expect("diagnostics");
    let combined = format!(
        "{}\n{}",
        serde_json::to_string(&listed).unwrap_or_default(),
        serde_json::to_string(&diag).unwrap_or_default()
    );
    for secret in [env_secret, secret_v1, secret_v2] {
        assert!(
            !combined.contains(secret),
            "plaintext secret `{secret}` leaked into HTTP/diagnostics output!"
        );
    }

    harness.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end A08 verification of strictly read-only personal ~/.gemini discovery, project bindings, rug-pull trust downgrade, 1-click rollback, and MCP secret injection"
)]
async fn d3_a08_capabilities_readonly_personal_discovery_rug_pull_and_rollback() {
    let personal_dir = env::temp_dir().join(format!(
        "dennett-d3-personal-gemini-{}",
        Uuid::new_v4().simple()
    ));
    let personal_skill_dir = personal_dir
        .join("config")
        .join("skills")
        .join("personal-secret-skill");
    fs::create_dir_all(&personal_skill_dir).expect("create personal skill dir");
    fs::write(
        personal_skill_dir.join("SKILL.md"),
        "---\nname: Personal Skill\ndescription: Strictly read-only personal skill\n---\nPersonal instructions.",
    )
    .expect("write personal SKILL.md");
    fs::write(
        personal_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "personal-mcp": {
                    "command": "echo",
                    "args": ["hello"]
                }
            }
        }))
        .expect("serialize settings"),
    )
    .expect("write personal settings.json");
    let tree_hash_before = hash_directory_tree(&personal_dir);

    let harness = LiveHeadHarness::start(
        "caps",
        None,
        HeadServiceExtras {
            personal_profile_dir: Some(personal_dir.clone()),
            ..HeadServiceExtras::default()
        },
    )
    .await;

    let ws_dir = harness.temp_root.join("ws-caps");
    fs::create_dir_all(ws_dir.join(".agents").join("skills").join("proj-lint"))
        .expect("create project skill dir");
    fs::write(
        ws_dir
            .join(".agents")
            .join("skills")
            .join("proj-lint")
            .join("SKILL.md"),
        "---\nname: Project Lint\ndescription: Lint rules\n---\nRun strict clippy.",
    )
    .expect("write proj SKILL.md");
    fs::create_dir_all(ws_dir.join(".dennett")).expect("create .dennett dir");
    fs::write(
        ws_dir.join(".dennett").join("mcp.json"),
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "docs-helper": {
                    "command": "docs-mcp-bin",
                    "args": ["--readonly"],
                    "secret_slots": {
                        "DOCS_API_KEY": "mcp:docs-helper:DOCS_API_KEY"
                    }
                }
            }
        }))
        .expect("serialize mcp.json"),
    )
    .expect("write .dennett/mcp.json");

    let scope = MemoryScope::try_new("project:d3-caps").expect("scope");
    let project = harness
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Caps Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("CapAgent"),
                description: String::new(),
                system_prompt: String::from("Inspect capabilities."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let session = harness
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("Caps Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    // 1. Discover project + strictly read-only personal ~/.gemini/ candidates.
    let disc = harness
        .client
        .discover_capabilities(
            &DiscoverCapabilitiesRequest {
                project_id: Some(project.project_id),
                scan_personal_gemini_readonly: true,
            },
            None,
        )
        .await
        .expect("discover capabilities");
    let tree_hash_after = hash_directory_tree(&personal_dir);
    assert_eq!(
        tree_hash_before, tree_hash_after,
        "A08 invariant: personal ~/.gemini/ directory MUST NEVER be modified!"
    );

    let personal_skill = disc
        .discovered
        .iter()
        .find(|c| c.slug == "personal-secret-skill")
        .expect("personal skill discovered");
    assert_eq!(
        personal_skill.origin_kind,
        CapabilityOrigin::PersonalDiscovered
    );
    assert_eq!(
        personal_skill.scope_kind,
        CapabilityScopeKind::CandidateUnattached
    );
    assert_eq!(
        personal_skill.activation_state,
        CapabilityActivation::Disabled
    );
    assert_eq!(personal_skill.trust_state, CapabilityTrust::Unreviewed);

    let docs_mcp = disc
        .discovered
        .iter()
        .find(|c| c.slug == "docs-helper")
        .expect("project MCP server discovered");
    assert_eq!(docs_mcp.origin_kind, CapabilityOrigin::ProjectLocal);

    // 2. Unreviewed/disabled personal & project items MUST NOT be materialized into sessions!
    let inspect_before = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect before enable");
    let before_text = &inspect_before
        .assistant_message
        .as_ref()
        .expect("assistant msg")
        .content;
    assert!(!before_text.contains("personal-secret-skill"));
    assert!(!before_text.contains("docs-helper"));

    // 3. Explicitly trust personal skill + bind to project -> now materialized!
    let _ = harness
        .client
        .update_capability(
            personal_skill.capability_id,
            &UpdateCapabilityRequest {
                trust_state: Some(CapabilityTrust::Trusted),
                ..UpdateCapabilityRequest::default()
            },
            None,
        )
        .await
        .expect("trust personal skill");
    let _ = harness
        .client
        .set_project_capability_binding(
            project.project_id,
            personal_skill.capability_id,
            &SetProjectCapabilityBindingRequest {
                activation_state: CapabilityActivation::Active,
            },
            None,
        )
        .await
        .expect("bind personal skill to project");

    let inspect_bound = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect after bind");
    assert!(
        inspect_bound
            .assistant_message
            .as_ref()
            .expect("assistant msg")
            .content
            .contains("personal-secret-skill")
    );

    // 4. Rug-pull update adding executable scripts -> trust_state automatically resets to Restricted!
    let rug_pulled = harness
        .client
        .update_capability(
            personal_skill.capability_id,
            &UpdateCapabilityRequest {
                version: Some(String::from("2.0.0")),
                spec: Some(CapabilitySpec::Skill {
                    skill_markdown: String::from("# Updated with scripts"),
                    has_scripts: true,
                    entry_files: vec![String::from("SKILL.md"), String::from("scripts/run.sh")],
                }),
                ..UpdateCapabilityRequest::default()
            },
            None,
        )
        .await
        .expect("rug-pull update");
    assert_eq!(rug_pulled.trust_state, CapabilityTrust::Restricted);
    assert!(rug_pulled.can_rollback);

    let inspect_restricted = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect after rug pull");
    assert!(
        !inspect_restricted
            .assistant_message
            .as_ref()
            .expect("assistant msg")
            .content
            .contains("personal-secret-skill"),
        "Restricted rug-pulled capability MUST NOT materialize into runtime session"
    );

    // 5. 1-click rollback restores previous version and Trusted state!
    let rolled_back = harness
        .client
        .rollback_capability(personal_skill.capability_id, None)
        .await
        .expect("rollback capability");
    assert_eq!(rolled_back.trust_state, CapabilityTrust::Trusted);
    assert_ne!(rolled_back.version, "2.0.0");

    // 6. Store secret for MCP server + trust/activate it -> mounted without leaking plaintext!
    let mcp_secret = "DOCS_SUPER_SECRET_KEY_7777";
    let _ = harness
        .client
        .set_secret(
            "mcp:docs-helper:DOCS_API_KEY",
            &SetSecretRequest {
                value: mcp_secret.to_owned(),
                scope: String::from("global"),
                description: String::from("Docs MCP key"),
            },
            None,
        )
        .await
        .expect("set MCP secret");
    let _ = harness
        .client
        .update_capability(
            docs_mcp.capability_id,
            &UpdateCapabilityRequest {
                trust_state: Some(CapabilityTrust::Trusted),
                activation_state: Some(CapabilityActivation::Active),
                ..UpdateCapabilityRequest::default()
            },
            None,
        )
        .await
        .expect("trust and activate docs-helper MCP");

    let inspect_mcp = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect with MCP");
    let mcp_text = &inspect_mcp
        .assistant_message
        .as_ref()
        .expect("assistant msg")
        .content;
    assert!(mcp_text.contains("docs-helper"));
    assert!(mcp_text.contains("personal-secret-skill"));
    assert!(!mcp_text.contains(mcp_secret));

    let diag = harness
        .client
        .get_diagnostics(Some(100), None)
        .await
        .expect("diag");
    assert!(
        !serde_json::to_string(&diag)
            .unwrap_or_default()
            .contains(mcp_secret)
    );

    let _ = fs::remove_dir_all(&personal_dir);
    harness.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies A07 native subagent policy enforcement and control-MCP dct1 text cooperation + child_scope subset guard"
)]
async fn d3_a07_native_subagents_and_control_mcp_text_cooperation_bounds() {
    let harness = LiveHeadHarness::start("a07", None, HeadServiceExtras::default()).await;

    let ws_dir = harness.temp_root.join("ws-a07");
    fs::create_dir_all(ws_dir.join("docs")).expect("create docs");
    fs::create_dir_all(ws_dir.join("secrets")).expect("create secrets");

    let scope_alpha = MemoryScope::try_new("project:a07-alpha").expect("scope alpha");
    let scope_extra = MemoryScope::try_new("project:a07-extra").expect("scope extra");
    let project = harness
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("A07 Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope_alpha.clone()),
            },
            None,
        )
        .await
        .expect("create project");

    let parent_agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Coordinator"),
                description: String::new(),
                system_prompt: String::from("Coordinate helpers."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_alpha.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create parent agent");
    let narrow_helper = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("ReadOnlyHelper"),
                description: String::new(),
                system_prompt: String::from("Analyze safely."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_alpha.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create narrow helper");
    let privileged_agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("TooWideAgent"),
                description: String::new(),
                system_prompt: String::from("Over-privileged."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_alpha.clone(), scope_extra]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create privileged agent");

    let session = harness
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: parent_agent.agent_id,
                project_id: Some(project.project_id),
                title: String::from("A07 Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    // 1. Native subagent allowed inside `docs/`, denied outside `docs/`.
    let sub_ok = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:SUBAGENT:helper|write_file|docs/sub.txt|ok]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("subagent within docs/");
    assert!(
        sub_ok
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "invoke_subagent" && d.effect == PolicyEffect::Allow)
    );
    assert!(
        sub_ok
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "write_file" && d.effect == PolicyEffect::Allow)
    );

    let sub_escape = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:SUBAGENT:helper|write_file|secrets/leak.txt|bad]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("subagent child escape check");
    assert!(
        sub_escape
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "write_file" && d.effect == PolicyEffect::Deny)
    );

    // Explicit Deny rule blocks `invoke_subagent` itself!
    let _ = harness
        .client
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("project:{}", project.project_id),
                tool_name: String::from("invoke_subagent"),
                target_pattern: None,
                effect: PolicyEffect::Deny,
                reason: String::from("disable native subagents in this test"),
            },
            None,
        )
        .await
        .expect("create deny rule");
    let sub_blocked = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:SUBAGENT:helper|write_file|docs/sub2.txt|no]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("subagent blocked by policy");
    assert!(
        sub_blocked
            .permission_decisions
            .iter()
            .any(|d| d.tool_name == "invoke_subagent" && d.effect == PolicyEffect::Deny)
    );

    // 2. `dct1.` Control MCP HTTP surface: text coordination + strict `child_scope ⊆ parent_scope`!
    let parent_token = harness
        .service
        .control_token_signer()
        .mint_control_token(
            Some(project.project_id),
            parent_agent.agent_id,
            session.session_id,
            SessionKind::UserChat,
        )
        .expect("mint parent dct1");
    let helper_token = harness
        .service
        .control_token_signer()
        .mint_control_token(
            Some(project.project_id),
            narrow_helper.agent_id,
            session.session_id,
            SessionKind::UserChat,
        )
        .expect("mint helper dct1");

    let parent_mcp_client = HeadClient::new(&harness.head_url)
        .expect("parent mcp client")
        .with_control_token(parent_token);
    let helper_mcp_client = HeadClient::new(&harness.head_url)
        .expect("helper mcp client")
        .with_control_token(helper_token);

    // Plain-text coordination message sent by parent -> readable by helper!
    let sent = parent_mcp_client
        .send_agent_message(
            narrow_helper.agent_id,
            &SendAgentMessageRequest {
                sender_agent_id: Some(parent_agent.agent_id),
                project_id: project.project_id,
                related_task_id: None,
                body_text: String::from("Please summarize docs/sub.txt — keep it factual."),
                idempotency_key: IdempotencyKey::try_new("a07-msg-1").expect("key"),
            },
            None,
        )
        .await
        .expect("send plain-text agent message via dct1");
    assert_eq!(sent.message.sender_agent_id, Some(parent_agent.agent_id));

    let inbox = helper_mcp_client
        .list_agent_messages(narrow_helper.agent_id, true, None)
        .await
        .expect("helper reads own messages via dct1");
    assert_eq!(inbox.messages.len(), 1);
    assert!(inbox.messages[0].body_text.contains("docs/sub.txt"));

    // Parent MUST NOT read helper's inbox with parent's dct1 token!
    let cross_read = parent_mcp_client
        .list_agent_messages(narrow_helper.agent_id, false, None)
        .await
        .expect_err("parent dct1 must not read another agent's inbox");
    assert!(matches!(cross_read, DennettError::AccessDenied { .. }));

    // Delegating to narrower helper succeeds!
    let delegated = parent_mcp_client
        .create_task(
            &CreateTaskRequest {
                project_id: project.project_id,
                agent_id: narrow_helper.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Helper review")),
                goal: String::from("Summarize docs/sub.txt"),
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
            None,
        )
        .await
        .expect("delegate task to narrower helper");
    assert_eq!(delegated.agent_id, narrow_helper.agent_id);

    // Delegating to wider privileged agent FAILS CLOSED (`child_scope ⊆ parent_scope`)!
    let escalated = parent_mcp_client
        .create_task(
            &CreateTaskRequest {
                project_id: project.project_id,
                agent_id: privileged_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Privilege escalation attempt")),
                goal: String::from("Access extra scopes"),
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
            None,
        )
        .await
        .expect_err("delegating to over-privileged helper must fail closed");
    assert!(matches!(
        escalated,
        DennettError::ForbiddenScope { .. } | DennettError::AccessDenied { .. }
    ));

    harness.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end V02 verification covering overview degradation, 503 mutation proxy, and unblocked chat/stop"
)]
async fn d3_v02_memory_fabric_proxy_degrades_honestly_and_keeps_stop_unblocked() {
    // Bind and immediately drop a loopback TCP listener to get a guaranteed-unreachable port.
    let dead_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let dead_addr = dead_listener.local_addr().expect("dead addr");
    drop(dead_listener);

    let dead_mem_client =
        MemoryClient::unauthenticated(format!("http://{dead_addr}")).expect("dead memory client");
    let harness = LiveHeadHarness::start(
        "v02proxy",
        Some(dead_mem_client),
        HeadServiceExtras::default(),
    )
    .await;

    // 1. GET /v1/memory/overview returns 200 OK with DegradedUnreachable and empty collections!
    let scope = MemoryScope::try_new("project:v02-test").expect("scope");
    let overview = harness
        .client
        .get_memory_overview(Some(&scope), None)
        .await
        .expect("V02: overview must succeed with DegradedUnreachable when memoryd is down");
    assert_eq!(overview.memory_status, MemoryStatus::DegradedUnreachable);
    assert!(overview.degraded_reason.is_some());
    assert!(overview.stats.is_none());
    assert_eq!(overview.notes.len(), 0);
    assert_eq!(overview.claims.len(), 0);
    assert_eq!(overview.conflicts.len(), 0);
    assert_eq!(overview.documents.len(), 0);

    // 2. Mutating proxy fails honestly with 503 DatabaseUnavailable!
    let forget_err = harness
        .client
        .forget_memory_item(
            &ForgetRequest {
                scope: scope.clone(),
                target_type: ForgetTargetType::Note,
                target_id: Uuid::new_v4(),
                reason: String::from("test V02"),
                idempotency_key: IdempotencyKey::try_new("v02-forget-1").expect("key"),
            },
            None,
        )
        .await
        .expect_err("mutating memory proxy must fail closed with 503 when memoryd is down");
    assert!(matches!(
        forget_err,
        DennettError::DatabaseUnavailable { .. }
    ));

    // 3. Stop (`POST /v1/sessions/{id}/cancel`) stays completely unblocked!
    let agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("StopAgent"),
                description: String::new(),
                system_prompt: String::from("Run cleanly."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let session = harness
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("V02 Stop Test"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    let send_client = harness.client.clone();
    let sid = session.session_id;
    let turn_handle = tokio::spawn(async move {
        send_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:15000]"),
                    timeout_ms: Some(20_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });

    let wait_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snap = harness
            .client
            .get_state_snapshot(None, false, None)
            .await
            .expect("poll state snapshot for active V02 run");
        if snap.running_session_ids.contains(&session.session_id) {
            break;
        }
        assert!(
            Instant::now() < wait_deadline,
            "V02: timed out waiting for slow simulated turn to enter running_session_ids"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let cancel_started = Instant::now();
    let cancel_resp = harness
        .client
        .cancel_run(session.session_id, Some(&CorrelationId::generate()))
        .await
        .expect("V02: Stop must succeed immediately while memoryd is down");
    assert!(cancel_resp.cancelled);
    assert!(
        cancel_started.elapsed() < Duration::from_secs(3),
        "V02: Stop must not block on unreachable memoryd"
    );

    let turn_outcome = turn_handle
        .await
        .expect("join cancelled V02 turn")
        .expect("cancelled turn completes cleanly");
    assert_eq!(
        turn_outcome.run.status,
        dennett_contracts::RunStatus::Cancelled
    );
    harness.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies LIVE-A01-01 boot gemini-api-key fail-closed on catalog + turn, probe immutability, and explicit activate override"
)]
async fn live_a01_01_boot_gemini_api_key_profile_never_silently_uses_seeded_oauth() {
    let profile_root =
        env::temp_dir().join(format!("dennett-d3-live-a01-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&profile_root).expect("create isolated profile root");

    // Boot a NON-SIMULATED `AntigravityAcpRuntime` configured for `gemini-api-key` with NO key.
    let live_cfg = AntigravityRuntimeConfig::gemini_api_key(profile_root.clone(), None);
    let runtime: Arc<dyn AgentRuntimePort> = Arc::new(AntigravityAcpRuntime::new(live_cfg));
    assert!(!runtime.is_simulated());
    assert_eq!(runtime.auth_profile(), RuntimeAuthProfile::GeminiApiKey);

    let harness = LiveHeadHarness::start_with_runtime(
        "livea01",
        runtime,
        None,
        HeadServiceExtras {
            antigravity_profile_root: Some(profile_root.clone()),
            gemini_api_key_env_override: Some(None),
            ..HeadServiceExtras::default()
        },
    )
    .await;

    // 1. Untouched migration seed `00000000-0000-4000-8000-000000000001` MUST NOT override the
    //    explicit boot `--auth-profile gemini-api-key` in `GET /v1/runtime/catalog`!
    let catalog_err = harness
        .client
        .get_runtime_catalog(true, None)
        .await
        .expect_err("boot gemini-api-key without key must fail closed on catalog");
    assert!(
        matches!(
            catalog_err,
            DennettError::AuthRequired {
                profile: RuntimeAuthProfile::GeminiApiKey,
                ..
            }
        ),
        "expected AuthRequired(GeminiApiKey) on catalog, got: {catalog_err:?}"
    );

    // 2. Interactive turn without preferred_connection_id MUST also fail closed with AuthRequired(GeminiApiKey)
    //    and NEVER silently launch `oauth-personal`!
    let agent = harness
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("LiveKeyBootAgent"),
                description: String::new(),
                system_prompt: String::from("Fail closed without key."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
                autonomy_mode: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let session = harness
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: None,
                title: String::from("LIVE-A01-01 canary"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("create session");

    let turn_err = harness
        .client
        .send_message(
            session.session_id,
            &SendMessageRequest {
                content: String::from("hello without key"),
                timeout_ms: Some(5_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect_err("boot gemini-api-key without key must fail closed on send_message");
    assert!(
        matches!(
            turn_err,
            DennettError::AuthRequired {
                profile: RuntimeAuthProfile::GeminiApiKey,
                ..
            }
        ),
        "expected AuthRequired(GeminiApiKey) on turn, got: {turn_err:?}"
    );

    // 3. Verify the migration seed row still has `created_at == updated_at`.
    let conns = harness
        .client
        .list_provider_connections(None)
        .await
        .expect("list connections");
    let seeded_oauth = conns
        .connections
        .iter()
        .find(|c| c.connection_id == SEEDED_OAUTH_PERSONAL_CONNECTION_ID)
        .expect("seeded oauth row present");
    assert_eq!(seeded_oauth.created_at, seeded_oauth.updated_at);

    // 4. Once the user EXPLICITLY creates & activates a live LocalHttp connection as default interactive,
    //    `get_runtime_catalog` honors the explicit activation!
    let (local_url, _mock_state) = spawn_mock_local_llm(None).await;
    let local_conn = harness
        .client
        .create_provider_connection(
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::LocalHttp,
                display_name: String::from("Explicit Local Override"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::LocalEndpoint,
                endpoint_url: Some(local_url),
                secret_slot_key: None,
                is_default_interactive: Some(false),
                is_default_background: Some(false),
                enabled: Some(true),
            },
            None,
        )
        .await
        .expect("create local connection");
    let activated = harness
        .client
        .activate_provider_connection(
            local_conn.connection_id,
            &ActivateConnectionRequest {
                role: ConnectionActivationRole::Interactive,
            },
            None,
        )
        .await
        .expect("explicitly activate local connection");
    assert!(activated.is_default_interactive);
    assert!(activated.updated_at > activated.created_at);

    let catalog = harness
        .client
        .get_runtime_catalog(true, None)
        .await
        .expect("explicitly activated local connection succeeds on catalog");
    assert_eq!(catalog.auth_profile, RuntimeAuthProfile::LocalEndpoint);
    assert_eq!(catalog.connection_id, Some(local_conn.connection_id));

    harness.cleanup().await;
    let _ = fs::remove_dir_all(&profile_root);
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "end-to-end PG16 verification for D3-UI-02 folder inspection and D3-UI-03 YAML block scalar parsing"
)]
async fn d3_ui_02_and_03_create_capability_inspects_skill_and_plugin_folders_on_disk() {
    let harness = LiveHeadHarness::start("capfolder", None, HeadServiceExtras::default()).await;

    let folders_root = harness.temp_root.join("picker-folders");
    let empty_dir = folders_root.join("empty-dir");
    let skill_dir = folders_root.join("my-scripted-skill");
    let plugin_dir = folders_root.join("my-hooked-plugin");
    fs::create_dir_all(&empty_dir).expect("create empty dir");
    fs::create_dir_all(skill_dir.join("scripts")).expect("create skill scripts dir");
    fs::create_dir_all(plugin_dir.join(".claude-plugin")).expect("create plugin manifest dir");
    fs::create_dir_all(plugin_dir.join("hooks")).expect("create plugin hooks dir");
    fs::create_dir_all(plugin_dir.join("skills").join("code-review"))
        .expect("create plugin nested skill dir");

    let real_skill_md = "---\nname: Build Verifier\ndescription: >-\n  Runs strict build verification\n  and reports missing invariants.\nversion: 1.3.0\n---\n# Build Verifier\nAlways inspect scripts before trust.\n";
    fs::write(skill_dir.join("SKILL.md"), real_skill_md).expect("write SKILL.md");
    fs::write(
        skill_dir.join("scripts").join("run.py"),
        "print('verify')\n",
    )
    .expect("write scripts/run.py");

    fs::write(
        plugin_dir.join(".claude-plugin").join("plugin.json"),
        r#"{"name":"Security Pack","description":"Plugin with hooks and MCP secret slot","version":"2.1.0"}"#,
    )
    .expect("write plugin.json");
    fs::write(plugin_dir.join("hooks").join("guard.json"), "{}").expect("write hook");
    fs::write(
        plugin_dir
            .join("skills")
            .join("code-review")
            .join("SKILL.md"),
        "# Code Review\nVerify invariants.\n",
    )
    .expect("write nested skill");
    fs::write(
        plugin_dir.join("mcp.json"),
        r#"{"mcpServers":{"scanner":{"command":"scan-mcp","args":["--strict"],"env":{"SCANNER_API_KEY":"never-copy-raw-secret"}}}}"#,
    )
    .expect("write mcp.json");

    // 1. Empty folder without SKILL.md MUST fail with InvalidInput (400)!
    let empty_skill_err = harness
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("empty-dir"),
                display_name: String::from("empty-dir"),
                description: String::new(),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                trust_state: CapabilityTrust::Unreviewed,
                activation_state: CapabilityActivation::Disabled,
                source_path: Some(empty_dir.to_string_lossy().into_owned()),
                spec: CapabilitySpec::Skill {
                    skill_markdown: String::from("placeholder"),
                    has_scripts: false,
                    entry_files: Vec::new(),
                },
                agent_id: None,
            },
            None,
        )
        .await
        .expect_err("empty skill folder without SKILL.md must return InvalidInput");
    assert!(matches!(empty_skill_err, DennettError::InvalidInput { .. }));

    // 2. Real skill folder with SKILL.md (`description: >-`) + `scripts/run.py`:
    //    Head MUST replace placeholder markdown/description, detect `has_scripts = true`,
    //    and force `trust_state = Restricted`!
    let created_skill = harness
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("my-scripted-skill"),
                display_name: String::from("my-scripted-skill"),
                description: String::from("placeholder description"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Active,
                source_path: Some(skill_dir.to_string_lossy().into_owned()),
                spec: CapabilitySpec::Skill {
                    skill_markdown: String::from("placeholder"),
                    has_scripts: false,
                    entry_files: Vec::new(),
                },
                agent_id: None,
            },
            None,
        )
        .await
        .expect("create skill from real disk folder");

    assert_eq!(created_skill.display_name, "Build Verifier");
    assert_eq!(
        created_skill.description,
        "Runs strict build verification and reports missing invariants."
    );
    assert_eq!(created_skill.version, "1.3.0");
    assert_eq!(created_skill.trust_state, CapabilityTrust::Restricted);
    match created_skill.spec {
        CapabilitySpec::Skill {
            skill_markdown,
            has_scripts,
            entry_files,
            ..
        } => {
            assert_eq!(skill_markdown, real_skill_md);
            assert!(has_scripts);
            assert!(entry_files.iter().any(|f| f == "SKILL.md"));
            assert!(entry_files.iter().any(|f| f == "scripts"));
        }
        other => panic!("expected Skill spec, got: {other:?}"),
    }

    // 3. Empty folder as Plugin MUST fail with InvalidInput (400)!
    let empty_plugin_err = harness
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("empty-plugin"),
                display_name: String::from("empty-plugin"),
                description: String::new(),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                trust_state: CapabilityTrust::Unreviewed,
                activation_state: CapabilityActivation::Disabled,
                source_path: Some(empty_dir.to_string_lossy().into_owned()),
                spec: CapabilitySpec::Plugin {
                    manifest_version: String::from("1.0"),
                    skills: Vec::new(),
                    mcp_servers: Vec::new(),
                    has_hooks: false,
                    enabled_components: Vec::new(),
                },
                agent_id: None,
            },
            None,
        )
        .await
        .expect_err("empty plugin folder must return InvalidInput");
    assert!(matches!(
        empty_plugin_err,
        DennettError::InvalidInput { .. }
    ));

    // 4. Real plugin folder with `.claude-plugin/plugin.json` + `hooks/` + `skills/` + `mcp.json`:
    let created_plugin = harness
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("my-hooked-plugin"),
                display_name: String::from("my-hooked-plugin"),
                description: String::new(),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Disabled,
                source_path: Some(plugin_dir.to_string_lossy().into_owned()),
                spec: CapabilitySpec::Plugin {
                    manifest_version: String::from("1.0"),
                    skills: Vec::new(),
                    mcp_servers: Vec::new(),
                    has_hooks: false,
                    enabled_components: Vec::new(),
                },
                agent_id: None,
            },
            None,
        )
        .await
        .expect("create plugin from real disk folder");

    assert_eq!(created_plugin.display_name, "Security Pack");
    assert_eq!(
        created_plugin.description,
        "Plugin with hooks and MCP secret slot"
    );
    assert_eq!(created_plugin.version, "2.1.0");
    assert_eq!(created_plugin.trust_state, CapabilityTrust::Restricted);
    match created_plugin.spec {
        CapabilitySpec::Plugin {
            skills,
            mcp_servers,
            has_hooks,
            ..
        } => {
            assert!(has_hooks);
            assert_eq!(skills, vec![String::from("code-review")]);
            assert_eq!(mcp_servers.len(), 1);
            assert_eq!(mcp_servers[0].secret_slots.len(), 1);
            assert_eq!(mcp_servers[0].env_vars.len(), 0);
        }
        other => panic!("expected Plugin spec, got: {other:?}"),
    }

    harness.cleanup().await;
}

// Keep unused-import-free helper sanity check for BTreeMap used in spec fixtures if needed.
#[test]
fn d3_test_file_helper_sanity() {
    let m: BTreeMap<String, String> = BTreeMap::new();
    assert_eq!(m.len(), 0);
}
