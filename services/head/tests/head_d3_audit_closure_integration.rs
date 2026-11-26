//! Real PostgreSQL 16 + Axum HTTP + `HeadClient` + Control-MCP integration tests
//! verifying all Bucket A D3 Audit Closure deliverables:
//! - `A-SEC-1`: Mandatory `DENNETT_HEAD_TOKEN` owner Bearer auth (`/healthz` exempt, unauthenticated
//!   & invalid Bearer rejected with `401 Unauthorized`, scoped `X-Dennett-Control-Token` boundary).
//! - `A-SEC-3` / `A11`: Cross-project `SecretBroker` scope isolation, standalone chat global MCP
//!   exclusion, built-in `dennett-guide` skill, `CapabilityScopeKind::Agent`, `Plugin.skills`, and
//!   `agent.current_config.attached_skills`.
//! - `A-CORE-1`: Owner's 2-mode autonomy (`Auto` vs `FullAccess`), workspace file & safe local shell
//!   auto-allow, compound/risky shell `Ask` + session-scoped `ApproveTask` grant, A8 web search, and
//!   `[Dennett Runtime Context]` injection.
//! - API Parity: `GET/PATCH /v1/preferences`, `GET /v1/projects/{id}/file`, `POST /v1/sessions/{id}/fork`,
//!   `PATCH /v1/sessions/{id}` (`status`), `DELETE /v1/sessions/{id}`, `DELETE /v1/policies/{rule_id}`,
//!   `POST /v1/runs/emergency-stop`, `POST /v1/tasks/{id}/pause|resume|run-now`, `DELETE /v1/tasks/{id}`.
//! - Scheduling & Delegation: PG16 `YearlyWallTime` (including Feb 29 leap year), `TaskTrigger::OnRunCompleted`
//!   wake-up + completion notification to `notify_session_id`, and `A10` `child ⊆ parent` delegation ceiling.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use dennett_contracts::{
    ArtifactCompleteness, AutonomyMode, CapabilityActivation, CapabilityOrigin,
    CapabilityOwnership, CapabilityScopeKind, CapabilitySpec, CapabilityTrust, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreateCapabilityRequest, CreatePolicyRuleRequest,
    CreateProjectRequest, CreateSessionRequest, CreateTaskRequest, DennettError,
    ForkSessionRequest, IdempotencyKey, MemoryScope, MessageRole, MissedRunPolicy,
    PermissionRequestStatus, PermissionResolutionDecision, PolicyEffect, ProfileMode, ProviderKind,
    ResolvePermissionRequest, RunStatus, RuntimeAuthProfile, ScheduleKind, SendMessageRequest,
    SessionKind, SessionStatus, SetSecretRequest, TaskSchedule, TaskSignalRequest, TaskStatus,
    TaskTrigger, ThinkingLevel, TimestampMs, UpdateAgentConfigRequest,
    UpdateUserPreferencesRequest, UpsertProviderConnectionRequest, WorkspaceWritePolicy,
};
use dennett_head::{
    HeadAppState, HeadService, HeadServiceExtras, LATEST_CONTROL_SCHEMA_VERSION,
    MAX_CONCURRENT_REQUESTS, ValidatedControlPgTarget, build_head_router, compute_next_fire_at_pg,
    write_owner_only_file,
};
use dennett_head_client::HeadClient;
use dennett_runtime_antigravity::SimulatedRuntime;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio::time;
use tokio_postgres::NoTls;
use uuid::Uuid;
use zeroize::Zeroizing;

struct AuditHarness {
    base_pg_url: String,
    schema: String,
    iso_pg_url: String,
    iso_target: ValidatedControlPgTarget,
    head_url: String,
    service: HeadService,
    client: HeadClient,
    shutdown_tx: watch::Sender<bool>,
    temp_root: PathBuf,
}

impl AuditHarness {
    async fn start(prefix: &str, mut extras: HeadServiceExtras) -> Self {
        assert_eq!(
            env::var("DENNETT_TEST_PROFILE").ok().as_deref(),
            Some("1"),
            "audit closure integration tests require DENNETT_TEST_PROFILE=1"
        );
        let base_pg_url = env::var("DENNETT_TEST_CONTROL_PG_URL")
            .expect("DENNETT_TEST_CONTROL_PG_URL is required");
        let short = &Uuid::new_v4().simple().to_string()[..12];
        let schema = format!("aud_{prefix}_{short}");

        let (pg, conn) = tokio_postgres::connect(&base_pg_url, NoTls)
            .await
            .expect("connect PG");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        pg.batch_execute(&format!("CREATE SCHEMA {schema}"))
            .await
            .expect("create schema");
        drop(pg);

        let sep = if base_pg_url.contains('?') { '&' } else { '?' };
        let iso_pg_url = format!("{base_pg_url}{sep}options=-csearch_path%3D{schema}");
        let iso_target = ValidatedControlPgTarget::resolve_from_sources(
            None,
            Some(&iso_pg_url),
            None,
            ProfileMode::TestIsolated,
        )
        .expect("resolve isolated target")
        .expect("validated target");

        let temp_root = env::temp_dir().join(format!("dennett-audit-it-{prefix}-{short}"));
        fs::create_dir_all(&temp_root).expect("create temp root");
        if extras.detached_projects_root.is_none() {
            extras.detached_projects_root = Some(temp_root.join("detached"));
        }
        if extras.vault_dir.is_none() {
            extras.vault_dir = Some(temp_root.join("vault"));
        }

        let runtime = Arc::new(SimulatedRuntime::new(RuntimeAuthProfile::OAuthPersonal));
        let service = HeadService::new_with_extras(
            iso_target.clone(),
            runtime,
            None,
            None,
            ProfileMode::TestIsolated,
            vec![String::from("AUDIT_SECRET_TOKEN_778899")],
            extras,
        )
        .expect("construct HeadService");
        let schema_ver = service.ensure_migrated().await.expect("apply migrations");
        assert_eq!(schema_ver, LATEST_CONTROL_SCHEMA_VERSION);
        assert_eq!(schema_ver, 11);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback listener");
        let addr = listener.local_addr().expect("local addr");
        let head_url = format!("http://{addr}");
        service.set_self_http_url(&head_url);

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let state = HeadAppState {
            service: Some(service.clone()),
            profile_mode: ProfileMode::TestIsolated,
            runtime_adapter_id: service.runtime_adapter_id().to_owned(),
            runtime_is_simulated: service.runtime_is_simulated(),
            allow_non_loopback: false,
            owner_token: Zeroizing::new(service.owner_token().to_owned()),
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
            .expect("HeadClient")
            .with_owner_token(service.owner_token());

        Self {
            base_pg_url,
            schema,
            iso_pg_url,
            iso_target,
            head_url,
            service,
            client,
            shutdown_tx,
            temp_root,
        }
    }

    async fn cleanup(self) {
        let _ = self.shutdown_tx.send(true);
        if let Ok((pg, conn)) = tokio_postgres::connect(&self.base_pg_url, NoTls).await {
            tokio::spawn(async move {
                let _ = conn.await;
            });
            let _ = pg
                .batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema))
                .await;
        }
        let _ = fs::remove_dir_all(&self.temp_root);
    }
}

fn unique_scope(prefix: &str) -> MemoryScope {
    let short = &Uuid::new_v4().simple().to_string()[..10];
    MemoryScope::try_new(format!("project:{prefix}-{short}")).expect("scope")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies A-SEC-1 owner Bearer auth, /healthz exemption, wrong-token 401, and scoped dct1 control token boundaries in one flow"
)]
async fn a_sec_1_owner_auth_bearer_and_scoped_control_token_boundary() {
    let h = AuditHarness::start("sec1", HeadServiceExtras::default()).await;

    // 1. Owner token is high-entropy, never leaked in extras Debug output, and writes cleanly with owner-only perms.
    assert!(
        h.service.owner_token().len() >= 32,
        "auto-generated owner_token must be high-entropy"
    );
    let extras_dbg = format!(
        "{:?}",
        HeadServiceExtras {
            owner_token_override: Some(String::from("SUPER_SECRET_OWNER_TOKEN_XYZ")),
            ..HeadServiceExtras::default()
        }
    );
    assert!(
        !extras_dbg.contains("SUPER_SECRET_OWNER_TOKEN_XYZ"),
        "HeadServiceExtras Debug output must never leak owner_token"
    );
    let tok_file = h.temp_root.join("head.token");
    write_owner_only_file(&tok_file, h.service.owner_token().as_bytes())
        .expect("write owner-only head.token");
    assert_eq!(
        fs::read_to_string(&tok_file).expect("read head.token"),
        h.service.owner_token()
    );

    // 2. Unauthenticated & wrong-token requests MUST fail with 401 Unauthorized, while /healthz succeeds!
    let unauth = HeadClient::new(&h.head_url).expect("unauth client");
    let health = unauth.healthz(None).await.expect("/healthz is exempt");
    assert_eq!(health.schema_version, Some(11));

    assert!(matches!(
        unauth.list_projects(None).await,
        Err(DennettError::Unauthorized { .. })
    ));
    assert!(matches!(
        unauth.list_agents(None).await,
        Err(DennettError::Unauthorized { .. })
    ));
    assert!(matches!(
        unauth.get_preferences(None).await,
        Err(DennettError::Unauthorized { .. })
    ));
    assert!(matches!(
        unauth.emergency_stop_all(None).await,
        Err(DennettError::Unauthorized { .. })
    ));

    let wrong_tok = HeadClient::new(&h.head_url)
        .expect("wrong client")
        .with_owner_token("totally-wrong-owner-token");
    assert!(matches!(
        wrong_tok.list_projects(None).await,
        Err(DennettError::Unauthorized { .. })
    ));

    // 3. Authenticated owner client succeeds and creates project + agent + session + waiting task.
    let scope = unique_scope("sec1");
    let proj = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Auth Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("create project");
    let agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Auth Agent"),
                description: String::from("Auth verifier"),
                system_prompt: String::from("Verify auth."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: None,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("create agent");
    let sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Scoped Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("session");
    let other_sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Other Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("other session");
    let waiting_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Wait Signal Task")),
                goal: String::from("Wait for go"),
                schedule: None,
                wait_for_signal: Some(String::from("go.now")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("waiting task");

    // 4. Scoped `X-Dennett-Control-Token` (`dct1.`) works ONLY on allowed control routes within scope!
    let dct = h
        .service
        .control_token_signer()
        .mint_control_token(
            Some(proj.project_id),
            agent.agent_id,
            sess.session_id,
            SessionKind::UserChat,
        )
        .expect("mint dct1");
    let ctrl_client = HeadClient::new(&h.head_url)
        .expect("ctrl client")
        .with_control_token(dct);

    // Owner-only routes MUST reject dct1!
    assert!(matches!(
        ctrl_client.get_preferences(None).await,
        Err(DennettError::Unauthorized { .. } | DennettError::AccessDenied { .. })
    ));
    assert!(matches!(
        ctrl_client.emergency_stop_all(None).await,
        Err(DennettError::Unauthorized { .. } | DennettError::AccessDenied { .. })
    ));
    assert!(matches!(
        ctrl_client.list_projects(None).await,
        Err(DennettError::Unauthorized { .. } | DennettError::AccessDenied { .. })
    ));

    // Allowed control routes: GET /v1/agents, own session artifacts, same-project task signal!
    let agents = ctrl_client
        .list_agents(None)
        .await
        .expect("dct1 may list agents");
    assert!(agents.agents.iter().any(|a| a.agent_id == agent.agent_id));

    let art = ctrl_client
        .create_artifact(
            sess.session_id,
            &CreateArtifactRequest {
                run_id: None,
                name: String::from("report.md"),
                mime_type: String::from("text/markdown"),
                content: Some(String::from("# Report")),
                relative_path: None,
                completeness_status: ArtifactCompleteness::Final,
            },
            None,
        )
        .await
        .expect("dct1 creates own session artifact");
    let listed = ctrl_client
        .list_artifacts(sess.session_id, None)
        .await
        .expect("dct1 lists own session artifacts");
    assert!(
        listed
            .artifacts
            .iter()
            .any(|a| a.artifact_id == art.artifact_id)
    );

    // Cross-session artifact access with dct1 MUST fail closed!
    assert!(matches!(
        ctrl_client
            .list_artifacts(other_sess.session_id, None)
            .await,
        Err(DennettError::AccessDenied { .. })
    ));

    // Same-project task signal via dct1 succeeds!
    let sig_resp = ctrl_client
        .send_task_signal(
            waiting_task.task_id,
            &TaskSignalRequest {
                signal_name: String::from("go.now"),
                payload_text: String::from("proceed"),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await
        .expect("dct1 signals same-project task");
    assert!(!sig_resp.deduplicated);

    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies A-SEC-3 cross-project secret isolation, A11 standalone global MCP exclusion, built-in dennett-guide skill, Agent-scoped capability, and Plugin skills"
)]
async fn a_sec_3_secret_scope_isolation_capabilities_and_dennett_guide() {
    let h = AuditHarness::start("sec3", HeadServiceExtras::default()).await;

    let scope_a = unique_scope("sec3-a");
    let scope_b = unique_scope("sec3-b");
    let proj_a = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project A"),
                workspace_path: None,
                memory_scope: Some(scope_a.clone()),
            },
            None,
        )
        .await
        .expect("proj A");
    let proj_b = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project B"),
                workspace_path: None,
                memory_scope: Some(scope_b.clone()),
            },
            None,
        )
        .await
        .expect("proj B");

    // Store one global secret and one project-A-only secret.
    h.client
        .set_secret(
            "global_mcp_key",
            &SetSecretRequest {
                value: String::from("GLOBAL_MCP_VALUE_111"),
                scope: String::from("global"),
                description: String::from("Global MCP secret"),
            },
            None,
        )
        .await
        .expect("set global secret");
    h.client
        .set_secret(
            "proj_a_only_key",
            &SetSecretRequest {
                value: String::from("PROJ_A_SECRET_VALUE_222"),
                scope: format!("project:{}", proj_a.project_id),
                description: String::from("Project A secret"),
            },
            None,
        )
        .await
        .expect("set project A secret");

    // Create active trusted Global MCP using `global_mcp_key`, plus a Project-B MCP that tries to steal `proj_a_only_key`!
    let _global_mcp = h
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("global-helper-mcp"),
                display_name: String::from("Global Helper MCP"),
                description: String::from("Trusted global MCP"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                agent_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Active,
                source_path: None,
                spec: CapabilitySpec::McpServer {
                    command: String::from("echo"),
                    args: vec![String::from("global")],
                    env_vars: BTreeMap::new(),
                    secret_slots: BTreeMap::from([(
                        String::from("API_KEY"),
                        String::from("global_mcp_key"),
                    )]),
                },
            },
            None,
        )
        .await
        .expect("global mcp");

    let _steal_mcp = h
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("steal-proj-a-mcp"),
                display_name: String::from("Project B Thief MCP"),
                description: String::from("Attempts cross-project secret read"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Project,
                project_id: Some(proj_b.project_id),
                agent_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Active,
                source_path: None,
                spec: CapabilitySpec::McpServer {
                    command: String::from("echo"),
                    args: vec![String::from("steal")],
                    env_vars: BTreeMap::new(),
                    secret_slots: BTreeMap::from([(
                        String::from("STOLEN_KEY"),
                        String::from("proj_a_only_key"),
                    )]),
                },
            },
            None,
        )
        .await
        .expect("project B mcp");

    // Create an OnDemand skill attached via `agent.current_config.attached_skills` and an Agent-scoped skill,
    // plus a Plugin bundling a skill!
    let _attached_ondemand = h
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("attached-skill-x"),
                display_name: String::from("Attached Skill X"),
                description: String::from("Activated when listed in agent.attached_skills"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                agent_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::OnDemand,
                source_path: None,
                spec: CapabilitySpec::Skill {
                    skill_markdown: String::from("# Attached Skill X\nBody X."),
                    has_scripts: false,
                    entry_files: Vec::new(),
                },
            },
            None,
        )
        .await
        .expect("attached skill");

    let plugin_dir = h.temp_root.join("my-plugin");
    let bundled_skill_dir = plugin_dir.join("skills").join("bundled-plugin-skill");
    fs::create_dir_all(&bundled_skill_dir).expect("plugin skill dir");
    fs::write(
        bundled_skill_dir.join("SKILL.md"),
        "# Bundled Plugin Skill\nLoaded directly from plugin directory.",
    )
    .expect("write plugin SKILL.md");
    let _plugin = h
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("audit-plugin"),
                display_name: String::from("Audit Plugin"),
                description: String::from("Plugin with bundled skill"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Global,
                project_id: None,
                agent_id: None,
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Active,
                source_path: Some(plugin_dir.to_string_lossy().into_owned()),
                spec: CapabilitySpec::Plugin {
                    manifest_version: String::from("1.0"),
                    skills: vec![String::from("bundled-plugin-skill")],
                    mcp_servers: Vec::new(),
                    has_hooks: false,
                    enabled_components: vec![String::from("bundled-plugin-skill")],
                },
            },
            None,
        )
        .await
        .expect("plugin");

    let agent_1 = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Agent One"),
                description: String::from("Has attached_skills and agent-scoped skill"),
                system_prompt: String::from("Inspect capabilities."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a, scope_b]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: None,
                default_budget: None,
                attached_skills: vec![String::from("attached-skill-x")],
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent 1");
    let agent_2 = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Agent Two"),
                description: String::from("Must NOT see Agent One's agent-scoped skill"),
                system_prompt: String::from("Inspect capabilities."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::new(),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: None,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent 2");

    let _agent_only_skill = h
        .client
        .create_capability(
            &CreateCapabilityRequest {
                slug: String::from("agent-one-private-skill"),
                display_name: String::from("Agent One Private Skill"),
                description: String::from("Scoped solely to Agent One"),
                version: String::from("1.0.0"),
                origin_kind: CapabilityOrigin::UserManual,
                ownership: CapabilityOwnership::User,
                scope_kind: CapabilityScopeKind::Agent,
                project_id: None,
                agent_id: Some(agent_1.agent_id),
                trust_state: CapabilityTrust::Trusted,
                activation_state: CapabilityActivation::Active,
                source_path: None,
                spec: CapabilitySpec::Skill {
                    skill_markdown: String::from("# Agent One Only"),
                    has_scripts: false,
                    entry_files: Vec::new(),
                },
            },
            None,
        )
        .await
        .expect("agent-scoped skill");

    // A11: Project A session receives global MCP + Agent 1's agent-scoped skill + attached_skills + plugin skill!
    let sess_a = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_1.agent_id,
                project_id: Some(proj_a.project_id),
                title: String::from("Proj A Caps"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess A");
    let resp_a = h
        .client
        .send_message(
            sess_a.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect A");
    let text_a = resp_a.assistant_message.expect("reply A").content;
    assert!(
        text_a.contains("global-helper-mcp(API_KEY[set:true])"),
        "project session must receive trusted global MCP: {text_a}"
    );
    assert!(
        text_a.contains("agent-one-private-skill"),
        "agent 1 must receive its agent-scoped skill: {text_a}"
    );
    assert!(
        text_a.contains("attached-skill-x"),
        "agent 1 must receive its attached_skills on-demand skill: {text_a}"
    );
    assert!(
        text_a.contains("bundled-plugin-skill"),
        "plugin bundled skill must be materialized: {text_a}"
    );

    // Built-in `dennett-guide` skill is seeded and readable via scoped session-skills API!
    let dct_a = h
        .service
        .control_token_signer()
        .mint_control_token(
            Some(proj_a.project_id),
            agent_1.agent_id,
            sess_a.session_id,
            SessionKind::UserChat,
        )
        .expect("mint dct_a");
    let ctrl_a = HeadClient::new(&h.head_url)
        .expect("ctrl_a")
        .with_control_token(dct_a);
    let guide = ctrl_a
        .read_session_skill("dennett-guide", None)
        .await
        .expect("read built-in dennett-guide skill");
    assert_eq!(guide.skill.slug, "dennett-guide");
    assert_ne!(guide.skill_md_content.trim(), "");

    // A11: Standalone session with Agent 2 MUST NOT receive `global-helper-mcp` OR `agent-one-private-skill`!
    let sess_standalone = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_2.agent_id,
                project_id: None,
                title: String::from("Standalone Caps"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess standalone");
    let resp_st = h
        .client
        .send_message(
            sess_standalone.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("inspect standalone");
    let text_st = resp_st.assistant_message.expect("reply st").content;
    assert!(
        !text_st.contains("global-helper-mcp"),
        "A11: standalone chat must NOT inject global MCP servers + secrets: {text_st}"
    );
    assert!(
        !text_st.contains("agent-one-private-skill"),
        "agent 2 must NOT receive agent 1's agent-scoped skill: {text_st}"
    );

    // A-SEC-3: Project B session trying to mount MCP referencing Project A's secret MUST fail closed!
    let sess_b = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_1.agent_id,
                project_id: Some(proj_b.project_id),
                title: String::from("Proj B Secret Isolation"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess B");
    let resp_b = h
        .client
        .send_message(
            sess_b.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:INSPECT_CAPABILITIES]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await;
    match resp_b {
        Err(DennettError::AccessDenied { .. }) => {}
        Ok(ok_resp) => {
            let txt = ok_resp.assistant_message.map_or_default(|m| m.content);
            assert!(
                !txt.contains("steal-proj-a-mcp"),
                "Project B must NEVER receive an MCP server resolved with Project A's secret: {txt}"
            );
        }
        other => panic!("unexpected result for cross-project secret access: {other:?}"),
    }

    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies A-CORE-1 2-mode autonomy (Auto vs FullAccess), safe workspace shell, compound shell Ask + session grant, A8 web search, and Runtime Context injection"
)]
async fn a_core_1_two_mode_autonomy_shell_guard_and_runtime_context() {
    let h = AuditHarness::start("core1", HeadServiceExtras::default()).await;
    let ws_dir = h.temp_root.join("ws");
    fs::create_dir_all(&ws_dir).expect("ws dir");

    let _prefs = h
        .client
        .update_preferences(
            &UpdateUserPreferencesRequest {
                ui_language: Some(String::from("en")),
                agent_language: Some(String::from("ru")),
                timezone: Some(String::from("Asia/Yekaterinburg")),
                enter_to_send: Some(true),
            },
            None,
        )
        .await
        .expect("set prefs");

    let scope = unique_scope("core1");
    let proj = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Autonomy Workspace"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("proj");

    let agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Autonomy Agent"),
                description: String::from("Tests 2-mode autonomy"),
                system_prompt: String::from("Execute cleanly."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                autonomy_mode: None,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    assert_eq!(agent.current_config.autonomy_mode, AutonomyMode::Auto);

    let sess1 = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Auto Session 1"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess1");

    // 1. [Dennett Runtime Context] injected into system_prompt with session/agent/project/autonomy/prefs!
    let echo_resp = h
        .client
        .send_message(
            sess1.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:ECHO_SYSTEM_PROMPT]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("echo system prompt");
    let sys_prompt = echo_resp.assistant_message.expect("sys prompt").content;
    assert!(
        sys_prompt.contains("[Dennett Runtime Context]"),
        "{sys_prompt}"
    );
    assert!(
        sys_prompt.contains(&sess1.session_id.to_string()),
        "{sys_prompt}"
    );
    assert!(
        sys_prompt.contains(&agent.agent_id.to_string()),
        "{sys_prompt}"
    );
    assert!(sys_prompt.contains("autonomy_mode: auto"), "{sys_prompt}");
    assert!(sys_prompt.contains("Asia/Yekaterinburg"), "{sys_prompt}");

    // 2. Under Auto: routine workspace write_file + safe local shell + A8 web_search are ALLOWED without prompts!
    let write_resp = h
        .client
        .send_message(
            sess1.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:write_file:notes/ok.txt|auto_written]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("auto write");
    assert_eq!(write_resp.run.status, RunStatus::Succeeded);
    assert_eq!(
        fs::read_to_string(ws_dir.join("notes").join("ok.txt")).expect("written"),
        "auto_written"
    );

    let cargo_resp = h
        .client
        .send_message(
            sess1.session_id,
            &SendMessageRequest {
                content: String::from(
                    "[SIM:TOOL:run_command:cargo test --workspace && git status]",
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("safe compound shell");
    assert!(
        cargo_resp
            .assistant_message
            .expect("cargo reply")
            .content
            .contains("ALLOWED")
    );

    let web_resp = h
        .client
        .send_message(
            sess1.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:web_search:rust async traits]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("A8 web_search");
    assert!(
        web_resp
            .assistant_message
            .expect("web reply")
            .content
            .contains("ALLOWED")
    );

    // 3. Compound risky shell (`cargo test && curl ...`) MUST trigger interactive `Ask`, and
    //    `ApproveTask` grants session-scoped reuse for THIS chat without unlocking another chat!
    let c_risky = h.client.clone();
    let sid1 = sess1.session_id;
    let risky_handle = tokio::spawn(async move {
        c_risky
            .send_message(
                sid1,
                &SendMessageRequest {
                    content: String::from(
                        "[SIM:TOOL:run_command:cargo test && curl https://example.invalid]",
                    ),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let perm = {
        let deadline = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid1), None)
                .await
                .expect("poll pending");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < deadline,
                "expected interactive Ask on risky compound shell"
            );
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    assert_eq!(perm.status, PermissionRequestStatus::Pending);

    let resolved = h
        .client
        .resolve_permission_request(
            perm.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm.revision),
                decision: PermissionResolutionDecision::ApproveTask,
                reason: Some(String::from("Allow for this chat")),
                max_uses: Some(5),
                expires_in_secs: Some(300),
            },
            None,
        )
        .await
        .expect("approve_task session grant");
    assert!(resolved.issued_grant.is_some());
    let risky_done = risky_handle.await.expect("join").expect("risky done");
    assert!(
        risky_done
            .assistant_message
            .expect("risky msg")
            .content
            .contains("ALLOWED")
    );

    // Same session reuses session grant immediately for the SAME command signature without prompting!
    let reuse_resp = h
        .client
        .send_message(
            sid1,
            &SendMessageRequest {
                content: String::from(
                    "[SIM:TOOL:run_command:cargo test && curl https://example.invalid/v2]",
                ),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("session grant reuse for matching signature");
    assert!(
        reuse_resp
            .assistant_message
            .expect("reuse msg")
            .content
            .contains("ALLOWED")
    );

    // Unrelated command (`git push origin feature`) MUST NOT reuse the `cargo test && curl` grant!
    let c_unrelated = h.client.clone();
    let unrelated_handle = tokio::spawn(async move {
        c_unrelated
            .send_message(
                sid1,
                &SendMessageRequest {
                    content: String::from("[SIM:TOOL:run_command:git push origin feature]"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let unrelated_perm = {
        let deadline = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid1), None)
                .await
                .expect("poll unrelated pending");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < deadline,
                "expected unrelated `git push` to still ask for approval"
            );
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    let _ = h
        .client
        .resolve_permission_request(
            unrelated_perm.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(unrelated_perm.revision),
                decision: PermissionResolutionDecision::Deny,
                reason: Some(String::from("reject unrelated command")),
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await
        .expect("deny unrelated command");
    let _ = unrelated_handle.await.expect("join unrelated");

    // 4. Upgrade agent to `AutonomyMode::FullAccess` -> persists in get/list/history and allows risky shell without prompt!
    let upgraded = h
        .client
        .update_agent_config(
            agent.agent_id,
            &UpdateAgentConfigRequest {
                system_prompt: String::from("Full autonomy."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                autonomy_mode: Some(AutonomyMode::FullAccess),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("upgrade to full_access");
    assert_eq!(
        upgraded.current_config.autonomy_mode,
        AutonomyMode::FullAccess
    );
    let history = h
        .client
        .get_agent_config_history(agent.agent_id, None)
        .await
        .expect("history");
    assert_eq!(history.versions.len(), 2);
    assert_eq!(history.versions[0].autonomy_mode, AutonomyMode::Auto);
    assert_eq!(history.versions[1].autonomy_mode, AutonomyMode::FullAccess);

    let sess2 = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("FullAccess Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess2");
    let full_resp = h
        .client
        .send_message(
            sess2.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:git push origin main]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("full_access shell");
    assert!(
        full_resp
            .assistant_message
            .expect("full msg")
            .content
            .contains("ALLOWED")
    );

    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies preferences, workspace file preview + traversal guard, session fork/status/delete, policy rule deletion after decision match, and emergency-stop"
)]
async fn api_parity_preferences_file_preview_fork_delete_policy_and_emergency_stop() {
    let h = AuditHarness::start("parity", HeadServiceExtras::default()).await;
    let ws_dir = h.temp_root.join("ws");
    fs::create_dir_all(ws_dir.join("src")).expect("ws src");
    fs::write(
        ws_dir.join("src").join("hello.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .expect("write hello.rs");

    // 1. GET/PATCH /v1/preferences + invalid IANA timezone rejection.
    let prefs_default = h.client.get_preferences(None).await.expect("default prefs");
    assert_eq!(prefs_default.timezone, "UTC");
    let prefs_updated = h
        .client
        .update_preferences(
            &UpdateUserPreferencesRequest {
                ui_language: Some(String::from("en")),
                agent_language: Some(String::from("en")),
                timezone: Some(String::from("Europe/Berlin")),
                enter_to_send: Some(false),
            },
            None,
        )
        .await
        .expect("update prefs");
    assert_eq!(prefs_updated.timezone, "Europe/Berlin");
    assert!(!prefs_updated.enter_to_send);
    assert!(matches!(
        h.client
            .update_preferences(
                &UpdateUserPreferencesRequest {
                    ui_language: None,
                    agent_language: None,
                    timezone: Some(String::from("Mars/Olympus_Mons")),
                    enter_to_send: None,
                },
                None,
            )
            .await,
        Err(DennettError::InvalidInput { .. })
    ));

    let scope = unique_scope("parity");
    let proj = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Parity Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("proj");

    // 2. GET /v1/projects/{id}/file preview + traversal rejection.
    let preview = h
        .client
        .get_project_file_content(proj.project_id, "src/hello.rs", None)
        .await
        .expect("file preview");
    assert_eq!(preview.relative_path, "src/hello.rs");
    assert!(!preview.is_binary);
    assert!(!preview.truncated);
    assert!(
        preview
            .content
            .as_deref()
            .unwrap_or("")
            .contains("fn main()")
    );
    assert!(matches!(
        h.client
            .get_project_file_content(proj.project_id, "../outside.txt", None)
            .await,
        Err(DennettError::AccessDenied { .. } | DennettError::InvalidInput { .. })
    ));

    let agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Parity Agent"),
                description: String::from("API parity tester"),
                system_prompt: String::from("Be brief."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                autonomy_mode: None,
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent");
    let sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Parent Session"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess");

    // 3. Session Fork copies exact prefix up to from_message_id and isolates subsequent turns.
    let turn1 = h
        .client
        .send_message(
            sess.session_id,
            &SendMessageRequest {
                content: String::from("Turn one"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn1");
    let turn1_assistant_id = turn1.assistant_message.expect("turn1 assistant").message_id;
    let _turn2 = h
        .client
        .send_message(
            sess.session_id,
            &SendMessageRequest {
                content: String::from("Turn two only in parent"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn2");

    let forked = h
        .client
        .fork_session(
            sess.session_id,
            &ForkSessionRequest {
                from_message_id: turn1_assistant_id,
                title: Some(String::from("Forked Branch")),
            },
            None,
        )
        .await
        .expect("fork session");
    assert_ne!(forked.session_id, sess.session_id);
    assert_eq!(forked.title, "Forked Branch");
    let forked_detail = h
        .client
        .get_session_detail(forked.session_id, None)
        .await
        .expect("forked detail");
    assert_eq!(forked_detail.messages.len(), 2);
    assert_eq!(forked_detail.messages[0].content, "Turn one");

    // 4. PATCH /v1/sessions/{id} updates status; DELETE /v1/sessions/{id} works when idle and returns NotFound after.
    let archived = h
        .client
        .update_session(
            forked.session_id,
            &dennett_contracts::UpdateSessionRequest {
                title: None,
                pinned: None,
                status: Some(SessionStatus::Archived),
            },
            None,
        )
        .await
        .expect("archive session status");
    assert_eq!(archived.status, SessionStatus::Archived);

    h.client
        .delete_session(forked.session_id, None)
        .await
        .expect("delete idle forked session");
    assert!(matches!(
        h.client.get_session_detail(forked.session_id, None).await,
        Err(DennettError::NotFound { .. })
    ));

    // 5. DELETE /v1/policies/{rule_id} succeeds even after matching an append-only permission_decision!
    let rule = h
        .client
        .create_policy_rule(
            &CreatePolicyRuleRequest {
                scope: format!("project:{}", proj.project_id),
                tool_name: String::from("run_command"),
                target_pattern: Some(String::from("echo *")),
                effect: PolicyEffect::Deny,
                reason: String::from("temporary deny for delete test"),
            },
            None,
        )
        .await
        .expect("create policy rule");
    let denied_turn = h
        .client
        .send_message(
            sess.session_id,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:echo blocked_by_rule]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("turn matching policy rule");
    assert!(
        denied_turn
            .permission_decisions
            .iter()
            .any(|d| d.matched_rule_id == Some(rule.rule_id))
    );
    h.client
        .delete_policy_rule(rule.rule_id, None)
        .await
        .expect("delete matched policy rule cleanly");
    let rules_after = h.client.list_policy_rules(None).await.expect("list rules");
    assert!(!rules_after.rules.iter().any(|r| r.rule_id == rule.rule_id));

    // 6. DELETE /v1/sessions/{id} returns 409 Conflict during active run; POST /v1/runs/emergency-stop cancels active run & task!
    let slow_client = h.client.clone();
    let sid = sess.session_id;
    let slow_handle = tokio::spawn(async move {
        slow_client
            .send_message(
                sid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:8000] stop me"),
                    timeout_ms: Some(20_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let deadline = time::Instant::now() + Duration::from_secs(5);
    loop {
        let live = h.client.get_session_live(sid, None).await.expect("live");
        if live.active.is_some() {
            break;
        }
        assert!(time::Instant::now() < deadline, "slow run did not start");
        time::sleep(Duration::from_millis(25)).await;
    }
    assert!(matches!(
        h.client.delete_session(sid, None).await,
        Err(DennettError::Conflict { .. })
    ));

    let bg_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Active BG Task")),
                goal: String::from("[SIM:SLOW:8000] stop active bg task"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(true),
            },
            None,
        )
        .await
        .expect("bg task");

    let task_start_deadline = time::Instant::now() + Duration::from_secs(5);
    loop {
        let d = h
            .client
            .get_task_detail(bg_task.task_id, None)
            .await
            .expect("poll bg task");
        if d.task.status == TaskStatus::Running {
            break;
        }
        assert!(
            time::Instant::now() < task_start_deadline,
            "bg task did not enter running state: {:?}",
            d.task
        );
        time::sleep(Duration::from_millis(25)).await;
    }

    // Create both a queued task AND a recurring scheduled task to verify P1-3 non-destructive pause!
    let queued_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Queued Task To Pause")),
                goal: String::from("Wait in queue"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("queued task");
    assert_eq!(queued_task.status, TaskStatus::Queued);

    let sched_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Recurring Task To Pause")),
                goal: String::from("Recurring heartbeat"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::Interval { interval_secs: 60 },
                    timezone: String::from("UTC"),
                    original_expression: Some(String::from("every 1 minute")),
                    missed_run_policy: MissedRunPolicy::ExecuteOnce,
                    next_fire_at: None,
                }),
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("scheduled task");
    assert_eq!(sched_task.status, TaskStatus::WaitingTimer);

    let estop = h
        .client
        .emergency_stop_all(None)
        .await
        .expect("emergency stop all");
    assert!(estop.cancelled_session_ids.contains(&sid));
    assert!(estop.cancelled_task_ids.contains(&bg_task.task_id));
    assert!(estop.cancelled_task_ids.contains(&queued_task.task_id));
    assert!(estop.cancelled_task_ids.contains(&sched_task.task_id));

    let slow_res = slow_handle
        .await
        .expect("join slow")
        .expect("cancelled run");
    assert_eq!(slow_res.run.status, RunStatus::Cancelled);

    // Running task MUST be cancelled!
    let bg_detail = h
        .client
        .get_task_detail(bg_task.task_id, None)
        .await
        .expect("bg detail");
    assert_eq!(bg_detail.task.status, TaskStatus::Cancelled);

    // Queued & scheduled tasks MUST be non-destructively PAUSED (never cancelled!) with emergency-stop wait_detail!
    let queued_detail = h
        .client
        .get_task_detail(queued_task.task_id, None)
        .await
        .expect("queued detail");
    assert_eq!(queued_detail.task.status, TaskStatus::WaitingTimer);
    assert_eq!(
        queued_detail.task.wait_detail.as_deref(),
        Some("Остановлено аварийным стопом")
    );

    let sched_detail = h
        .client
        .get_task_detail(sched_task.task_id, None)
        .await
        .expect("sched detail");
    assert_eq!(sched_detail.task.status, TaskStatus::WaitingTimer);
    assert_eq!(
        sched_detail.task.wait_detail.as_deref(),
        Some("Остановлено аварийным стопом")
    );

    // Scheduler tick far in the future MUST NOT fire the paused recurring task!
    let future_now = TimestampMs::from_millis(TimestampMs::now().as_millis() + 3_600_000);
    let tick_while_paused = h
        .service
        .tick_scheduler_at(future_now, &CorrelationId::generate())
        .await
        .expect("scheduler tick while paused");
    assert_eq!(
        tick_while_paused.woken_tasks, 0,
        "emergency-stopped recurring task must stay paused and NOT fire on future ticks"
    );

    // Owner can cleanly resume the paused recurring task afterwards!
    let resumed_sched = h
        .client
        .resume_task(sched_task.task_id, None)
        .await
        .expect("resume emergency-stopped recurring task");
    assert_eq!(resumed_sched.status, TaskStatus::WaitingTimer);
    assert_ne!(
        resumed_sched.wait_detail.as_deref(),
        Some("Остановлено аварийным стопом")
    );

    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies YearlyWallTime PG16 leap-year math, task pause/resume/run-now/delete, OnRunCompleted trigger + notify_session_id delivery, and A10 delegation ceiling"
)]
async fn scheduling_yearly_on_run_completed_task_controls_and_a10_delegation_ceiling() {
    let h = AuditHarness::start("sched", HeadServiceExtras::default()).await;

    // 1. PG16 `YearlyWallTime` (Feb 29 leap-year safe computation).
    let pg_cfg: tokio_postgres::Config = h.iso_pg_url.parse().expect("parse PG URL");
    let mgr = Manager::from_config(
        pg_cfg,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    let pool = Pool::builder(mgr)
        .max_size(2)
        .build()
        .expect("build PG pool");
    let pg = pool.get().await.expect("checkout PG client");
    let yearly_leap = TaskSchedule {
        kind: ScheduleKind::YearlyWallTime {
            month: 2,
            day: 29,
            hour: 9,
            minute: 0,
        },
        timezone: String::from("UTC"),
        original_expression: Some(String::from("every Feb 29 at 09:00")),
        missed_run_policy: MissedRunPolicy::ExecuteOnce,
        next_fire_at: None,
    };
    // 2025-01-15T00:00:00Z = 1_736_899_200_000 ms (2025 is non-leap -> safely clamped to 2025-02-28T09:00:00Z)
    let jan_2025_ms = 1_736_899_200_000_i64;
    let next_2025 = compute_next_fire_at_pg(
        &pg,
        &yearly_leap,
        TimestampMs::from_millis(jan_2025_ms),
        &h.iso_target,
    )
    .await
    .expect("yearly leap compute 2025")
    .expect("next fire 2025");
    assert!(next_2025.as_millis() > jan_2025_ms);
    // 2028-01-15T00:00:00Z = 1_831_507_200_000 ms (2028 is leap -> exact 2028-02-29T09:00:00Z = 1_835_427_600_000 ms!)
    let jan_2028_ms = 1_831_507_200_000_i64;
    let expected_feb29_2028_ms = 1_835_427_600_000_i64;
    let next_2028 = compute_next_fire_at_pg(
        &pg,
        &yearly_leap,
        TimestampMs::from_millis(jan_2028_ms),
        &h.iso_target,
    )
    .await
    .expect("yearly leap compute 2028")
    .expect("next fire 2028");
    assert_eq!(next_2028.as_millis(), expected_feb29_2028_ms);
    drop(pg);

    let scope = unique_scope("sched");
    let proj = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Sched Project"),
                workspace_path: None,
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("proj");
    let parent_agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Parent Agent"),
                description: String::from("Narrow parent"),
                system_prompt: String::from("Parent"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowPaths {
                    allowed_prefixes: vec![String::from("docs/")],
                },
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("parent agent");
    let notify_sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: parent_agent.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Main Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("notify sess");

    // 2. Task controls: pause -> resume -> run-now -> delete.
    let ctrl_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: parent_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Controllable Periodic Task")),
                goal: String::from("Run periodic check"),
                schedule: Some(TaskSchedule {
                    kind: ScheduleKind::Interval {
                        interval_secs: 3600,
                    },
                    timezone: String::from("UTC"),
                    original_expression: None,
                    missed_run_policy: MissedRunPolicy::ExecuteOnce,
                    next_fire_at: None,
                }),
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("ctrl task");

    let paused = h
        .client
        .pause_task(ctrl_task.task_id, None)
        .await
        .expect("pause task");
    assert!(
        paused
            .wait_detail
            .as_deref()
            .is_some_and(|w| !w.trim().is_empty()),
        "paused task must carry explicit wait_detail: {paused:?}"
    );
    let resumed = h
        .client
        .resume_task(ctrl_task.task_id, None)
        .await
        .expect("resume task");
    assert_eq!(resumed.status, TaskStatus::WaitingTimer);
    let _ran = h
        .client
        .run_task_now(ctrl_task.task_id, None)
        .await
        .expect("run task now");
    // Wait briefly for immediate run to finish, then delete task cleanly.
    let done_deadline = time::Instant::now() + Duration::from_secs(10);
    loop {
        let d = h
            .client
            .get_task_detail(ctrl_task.task_id, None)
            .await
            .expect("poll ran task");
        if d.task.usage.turns_used >= 1 && d.task.status != TaskStatus::Running {
            break;
        }
        assert!(
            time::Instant::now() < done_deadline,
            "run-now did not finish"
        );
        time::sleep(Duration::from_millis(40)).await;
    }
    h.client
        .delete_task(ctrl_task.task_id, None)
        .await
        .expect("delete task");
    assert!(matches!(
        h.client.get_task_detail(ctrl_task.task_id, None).await,
        Err(DennettError::NotFound { .. })
    ));

    // 3. `TaskTrigger::OnRunCompleted { run_id }` wakes waiting task when run completes & posts summary to `notify_session_id`!
    let slow_client = h.client.clone();
    let nsid = notify_sess.session_id;
    let lead_handle = tokio::spawn(async move {
        slow_client
            .send_message(
                nsid,
                &SendMessageRequest {
                    content: String::from("[SIM:SLOW:800] lead build"),
                    timeout_ms: Some(15_000),
                    client_message_id: None,
                },
                None,
            )
            .await
    });
    let lead_run_id = {
        let dl = time::Instant::now() + Duration::from_secs(5);
        loop {
            let live = h.client.get_session_live(nsid, None).await.expect("live");
            if let Some(active) = live.active {
                break active.run_id;
            }
            assert!(time::Instant::now() < dl, "lead run did not become active");
            time::sleep(Duration::from_millis(25)).await;
        }
    };
    let on_done_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: parent_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("After Build Notify")),
                goal: String::from("Summarize lead run output"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: Some(nsid),
                model_override: Some(String::from("sim-model-beta")),
                thinking_override: Some(ThinkingLevel::High),
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: Some(TaskTrigger::OnRunCompleted {
                    run_id: lead_run_id,
                }),
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("on_run_completed task");
    assert_eq!(on_done_task.status, TaskStatus::WaitingSignal);

    let lead_done = lead_handle.await.expect("join lead").expect("lead done");
    assert_eq!(lead_done.run.status, RunStatus::Succeeded);

    let wake_deadline = time::Instant::now() + Duration::from_secs(10);
    loop {
        let d = h
            .client
            .get_task_detail(on_done_task.task_id, None)
            .await
            .expect("poll on_done_task");
        if d.task.status == TaskStatus::Succeeded {
            // Confirm model/thinking override took effect on task's run!
            let task_sess_detail = h
                .client
                .get_session_detail(d.task.session_id, None)
                .await
                .expect("task sess detail");
            assert!(
                task_sess_detail.messages.iter().any(|m| {
                    m.role == MessageRole::Assistant
                        && m.content.contains("model=sim-model-beta")
                        && m.content.contains("thinking=high")
                }),
                "model/thinking override did not propagate: {:?}",
                task_sess_detail.messages
            );
            break;
        }
        assert!(
            time::Instant::now() < wake_deadline,
            "OnRunCompleted task did not finish: {:?}",
            d.task
        );
        time::sleep(Duration::from_millis(50)).await;
    }

    // Concise system completion notice MUST be delivered to `notify_session_id`!
    let main_detail = h
        .client
        .get_session_detail(nsid, None)
        .await
        .expect("main sess detail");
    assert!(
        main_detail
            .messages
            .iter()
            .any(|m| { m.role == MessageRole::System && m.content.contains("After Build Notify") }),
        "expected system completion notice in notify_session_id: {:?}",
        main_detail.messages
    );

    // 4. A10 Delegation Ceiling (`child ⊆ parent`): FullAccess child & paid-connection child MUST fail closed!
    let parent_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: parent_agent.agent_id,
                session_id: Some(nsid),
                parent_task_id: None,
                title: Some(String::from("Parent Task")),
                goal: String::from("Coordinate"),
                schedule: None,
                wait_for_signal: Some(String::from("hold")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("parent task");

    let full_access_child = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Escalated Autonomy Child"),
                description: String::from("More autonomy than parent"),
                system_prompt: String::from("Child"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: Some(AutonomyMode::FullAccess),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("full_access child");

    let escalated_autonomy_err = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: full_access_child.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Illegal FullAccess Delegation")),
                goal: String::from("Should be denied"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await;
    assert!(
        matches!(
            escalated_autonomy_err,
            Err(DennettError::AccessDenied { .. } | DennettError::ForbiddenScope { .. })
        ),
        "A10: parent(Auto) must NOT delegate to child(FullAccess): {escalated_autonomy_err:?}"
    );

    // Paid connection escalation: child using `gemini-api-key` connection when parent has none!
    let paid_conn = h
        .client
        .create_provider_connection(
            &UpsertProviderConnectionRequest {
                provider_kind: ProviderKind::AntigravityAcp,
                display_name: String::from("Paid API Key Conn"),
                adapter_id: None,
                auth_profile: RuntimeAuthProfile::GeminiApiKey,
                endpoint_url: None,
                secret_slot_key: None,
                is_default_interactive: None,
                is_default_background: None,
                enabled: Some(true),
            },
            None,
        )
        .await
        .expect("paid conn");
    let paid_child = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Paid Child"),
                description: String::from("Uses paid connection"),
                system_prompt: String::from("Child"),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: Some(paid_conn.connection_id),
            },
            None,
        )
        .await
        .expect("paid child");

    let paid_escalation_err = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj.project_id,
                agent_id: paid_child.agent_id,
                session_id: None,
                parent_task_id: Some(parent_task.task_id),
                title: Some(String::from("Illegal Paid Conn Delegation")),
                goal: String::from("Should be denied"),
                schedule: None,
                wait_for_signal: None,
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await;
    assert!(
        matches!(
            paid_escalation_err,
            Err(DennettError::AccessDenied { .. } | DennettError::ForbiddenScope { .. })
        ),
        "A10: parent without paid connection must NOT delegate to paid-connection child: {paid_escalation_err:?}"
    );

    let _ = CorrelationId::generate();
    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies P0-1 dct1 cross-agent task mutation ceiling (PATCH/cancel/signal), task_run cross-task goal/execute guard, and project-scoped GET /v1/agents filtering"
)]
async fn p0_1_control_token_task_privilege_ceiling_and_project_agent_filtering() {
    let h = AuditHarness::start("p01", HeadServiceExtras::default()).await;

    let scope_a = unique_scope("p01-a");
    let scope_b = unique_scope("p01-b");
    let proj_a = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project A"),
                workspace_path: None,
                memory_scope: Some(scope_a.clone()),
            },
            None,
        )
        .await
        .expect("proj A");
    let proj_b = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Project B"),
                workspace_path: None,
                memory_scope: Some(scope_b.clone()),
            },
            None,
        )
        .await
        .expect("proj B");

    let low_agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Low Privilege Agent"),
                description: String::from("DenyAll + Auto"),
                system_prompt: String::from("Low."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("low agent");
    let high_agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("High Privilege Agent"),
                description: String::from("AllowWorkspace + FullAccess"),
                system_prompt: String::from("High."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_a.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AllowWorkspace,
                autonomy_mode: Some(AutonomyMode::FullAccess),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("high agent");
    let foreign_agent = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Foreign Project B Agent"),
                description: String::from("Scoped solely to Project B"),
                system_prompt: String::from("Foreign."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope_b]),
                workspace_write_policy: WorkspaceWritePolicy::DenyAll,
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("foreign agent");
    let _foreign_sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: foreign_agent.agent_id,
                project_id: Some(proj_b.project_id),
                title: String::from("Project B Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("foreign sess");

    let low_sess = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: low_agent.agent_id,
                project_id: Some(proj_a.project_id),
                title: String::from("Low Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("low sess");

    let high_task = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj_a.project_id,
                agent_id: high_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Privileged FullAccess Task")),
                goal: String::from("Privileged goal"),
                schedule: None,
                wait_for_signal: Some(String::from("deploy.go")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("high task");

    let low_dct = h
        .service
        .control_token_signer()
        .mint_control_token(
            Some(proj_a.project_id),
            low_agent.agent_id,
            low_sess.session_id,
            SessionKind::UserChat,
        )
        .expect("mint low dct");
    let low_ctrl = HeadClient::new(&h.head_url)
        .expect("low ctrl client")
        .with_control_token(low_dct);

    // 1. GET /v1/agents with dct1 MUST NOT leak foreign_agent belonging to Project B!
    let visible_agents = low_ctrl
        .list_agents(None)
        .await
        .expect("list project agents");
    assert!(
        visible_agents
            .agents
            .iter()
            .any(|a| a.agent_id == low_agent.agent_id),
        "caller agent must be visible"
    );
    assert!(
        !visible_agents
            .agents
            .iter()
            .any(|a| a.agent_id == foreign_agent.agent_id),
        "foreign project B agent MUST NOT leak through project A dct1 GET /v1/agents"
    );

    // 2. Low-privilege dct1 MUST get AccessDenied trying to PATCH, cancel, or signal high_agent's task!
    let patch_err = low_ctrl
        .update_task(
            high_task.task_id,
            &dennett_contracts::UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Hijacked privileged goal")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(true),
            },
            None,
        )
        .await;
    assert!(
        matches!(patch_err, Err(DennettError::AccessDenied { .. })),
        "low-priv dct1 PATCH on high-priv task must return AccessDenied, got: {patch_err:?}"
    );

    let cancel_err = low_ctrl.cancel_task(high_task.task_id, None).await;
    assert!(
        matches!(cancel_err, Err(DennettError::AccessDenied { .. })),
        "low-priv dct1 cancel on high-priv task must return AccessDenied, got: {cancel_err:?}"
    );

    let signal_err = low_ctrl
        .send_task_signal(
            high_task.task_id,
            &TaskSignalRequest {
                signal_name: String::from("deploy.go"),
                payload_text: String::from("hijack"),
                idempotency_key: IdempotencyKey::generate(),
            },
            None,
        )
        .await;
    assert!(
        matches!(signal_err, Err(DennettError::AccessDenied { .. })),
        "low-priv dct1 signal on high-priv task must return AccessDenied, got: {signal_err:?}"
    );

    // 3. Background `task_run` dct1 (with `task_id = Some(task_1)`) MUST NOT rewrite `goal` or
    //    set `execute_immediately = Some(true)` on a different task `task_2` (even for SAME agent)!
    let low_task_1 = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj_a.project_id,
                agent_id: low_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Task 1")),
                goal: String::from("Task 1 goal"),
                schedule: None,
                wait_for_signal: Some(String::from("sig1")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("low task 1");
    let low_task_2 = h
        .client
        .create_task(
            &CreateTaskRequest {
                project_id: proj_a.project_id,
                agent_id: low_agent.agent_id,
                session_id: None,
                parent_task_id: None,
                title: Some(String::from("Task 2")),
                goal: String::from("Task 2 goal"),
                schedule: None,
                wait_for_signal: Some(String::from("sig2")),
                budget: None,
                attached_skills: Vec::new(),
                auth_profile_override: None,
                target_session_id: None,
                notify_session_id: None,
                model_override: None,
                thinking_override: None,
                connection_override_id: None,
                skill_slugs_override: None,
                trigger: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("low task 2");

    let task_run_dct = h
        .service
        .control_token_signer()
        .mint_control_token_with_task(
            Some(proj_a.project_id),
            low_agent.agent_id,
            low_task_1.session_id,
            SessionKind::TaskRun,
            Some(low_task_1.task_id),
        )
        .expect("mint task_run dct");
    let task_run_ctrl = HeadClient::new(&h.head_url)
        .expect("task_run ctrl")
        .with_control_token(task_run_dct);

    let cross_goal_err = task_run_ctrl
        .update_task(
            low_task_2.task_id,
            &dennett_contracts::UpdateTaskRequest {
                title: None,
                goal: Some(String::from("Rewritten by another task_run")),
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await;
    assert!(
        matches!(cross_goal_err, Err(DennettError::AccessDenied { .. })),
        "task_run dct1 must not rewrite goal on another task: {cross_goal_err:?}"
    );
    let cross_exec_err = task_run_ctrl
        .update_task(
            low_task_2.task_id,
            &dennett_contracts::UpdateTaskRequest {
                title: Some(String::from("Renamed")),
                goal: None,
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(true),
            },
            None,
        )
        .await;
    assert!(
        matches!(cross_exec_err, Err(DennettError::AccessDenied { .. })),
        "task_run dct1 must not trigger execute_immediately on another task: {cross_exec_err:?}"
    );
    // Updating its OWN task_1 title/goal stays permitted.
    let self_upd = task_run_ctrl
        .update_task(
            low_task_1.task_id,
            &dennett_contracts::UpdateTaskRequest {
                title: Some(String::from("Task 1 Updated")),
                goal: None,
                schedule: None,
                clear_schedule: false,
                budget: None,
                execute_immediately: Some(false),
            },
            None,
        )
        .await
        .expect("task_run may update its own task");
    assert_eq!(self_upd.title, "Task 1 Updated");

    h.cleanup().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "verifies P1-1 shell grant signature + agent_id isolation and P0-2 .git/hooks write protection end-to-end against PostgreSQL 16"
)]
async fn p1_1_shell_grant_scopes_to_signature_and_agent_id_and_blocks_git_hooks_pg() {
    let h = AuditHarness::start("p11", HeadServiceExtras::default()).await;
    let ws_dir = h.temp_root.join("ws");
    fs::create_dir_all(ws_dir.join(".git").join("hooks")).expect("ws .git/hooks");

    let scope = unique_scope("p11");
    let proj = h
        .client
        .create_project(
            &CreateProjectRequest {
                name: String::from("Grant Scope Project"),
                workspace_path: Some(ws_dir.to_string_lossy().into_owned()),
                memory_scope: Some(scope.clone()),
            },
            None,
        )
        .await
        .expect("proj");

    let agent_a = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Agent A"),
                description: String::from("First agent"),
                system_prompt: String::from("Run commands."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope.clone()]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent A");
    let agent_b = h
        .client
        .create_agent(
            &CreateAgentRequest {
                name: String::from("Agent B"),
                description: String::from("Second agent"),
                system_prompt: String::from("Run commands."),
                model_preference: None,
                thinking_level: None,
                allowed_memory_scopes: BTreeSet::from([scope]),
                workspace_write_policy: WorkspaceWritePolicy::AskBeforeWrite,
                autonomy_mode: Some(AutonomyMode::Auto),
                default_budget: None,
                attached_skills: Vec::new(),
                preferred_connection_id: None,
            },
            None,
        )
        .await
        .expect("agent B");

    let sess_a = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_a.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Agent A Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess A");
    let sess_b = h
        .client
        .create_session(
            &CreateSessionRequest {
                agent_id: agent_b.agent_id,
                project_id: Some(proj.project_id),
                title: String::from("Agent B Chat"),
                session_kind: SessionKind::UserChat,
            },
            None,
        )
        .await
        .expect("sess B");

    // 1. Agent A runs `git push origin branch-a` -> owner approves via ApproveProject.
    let c1 = h.client.clone();
    let sid_a = sess_a.session_id;
    let h1 = tokio::spawn(async move {
        c1.send_message(
            sid_a,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:git push origin branch-a]"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
    });
    let perm1 = {
        let dl = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid_a), None)
                .await
                .expect("poll perm1");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(time::Instant::now() < dl, "expected pending perm1");
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    let resolved1 = h
        .client
        .resolve_permission_request(
            perm1.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm1.revision),
                decision: PermissionResolutionDecision::ApproveProject,
                reason: Some(String::from("Allow git push for Agent A")),
                max_uses: Some(5),
                expires_in_secs: Some(300),
            },
            None,
        )
        .await
        .expect("approve_project git push");
    let grant = resolved1.issued_grant.expect("issued grant");
    assert_eq!(
        grant.agent_id,
        Some(agent_a.agent_id),
        "capability_grants.agent_id MUST be pinned to approving agent_a"
    );
    assert_eq!(
        grant.target_prefixes,
        vec![String::from("git push")],
        "shell capability grant MUST store normalized command signature"
    );
    let r1 = h1.await.expect("join h1").expect("h1 ok");
    assert!(
        r1.assistant_message
            .expect("m1")
            .content
            .contains("ALLOWED")
    );

    // 2. Same Agent A runs `git push origin branch-b` -> allowed immediately by grant!
    let r2 = h
        .client
        .send_message(
            sid_a,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:git push origin branch-b]"),
                timeout_ms: Some(10_000),
                client_message_id: None,
            },
            None,
        )
        .await
        .expect("same agent git push branch-b");
    assert!(
        r2.assistant_message
            .expect("m2")
            .content
            .contains("ALLOWED")
    );

    // 3. Same Agent A running `curl https://evil.example` MUST STILL prompt for approval!
    let c3 = h.client.clone();
    let h3 = tokio::spawn(async move {
        c3.send_message(
            sid_a,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:curl https://evil.example]"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
    });
    let perm3 = {
        let dl = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid_a), None)
                .await
                .expect("poll perm3");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < dl,
                "expected curl to require fresh approval despite git push grant"
            );
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    let _ = h
        .client
        .resolve_permission_request(
            perm3.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm3.revision),
                decision: PermissionResolutionDecision::Deny,
                reason: Some(String::from("deny curl")),
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await
        .expect("deny perm3");
    let _ = h3.await.expect("join h3");

    // 4. Different Agent B running `git push origin branch-c` MUST STILL prompt for approval!
    let c4 = h.client.clone();
    let sid_b = sess_b.session_id;
    let h4 = tokio::spawn(async move {
        c4.send_message(
            sid_b,
            &SendMessageRequest {
                content: String::from("[SIM:TOOL:run_command:git push origin branch-c]"),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
    });
    let perm4 = {
        let dl = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid_b), None)
                .await
                .expect("poll perm4");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < dl,
                "expected Agent B git push to require approval (must not inherit Agent A's grant)"
            );
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    let _ = h
        .client
        .resolve_permission_request(
            perm4.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm4.revision),
                decision: PermissionResolutionDecision::Deny,
                reason: Some(String::from("deny agent B")),
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await
        .expect("deny perm4");
    let _ = h4.await.expect("join h4");

    // 5. P0-2: Writing `.git/hooks/pre-commit` under Auto MUST trigger interactive approval!
    let c5 = h.client.clone();
    let h5 = tokio::spawn(async move {
        c5.send_message(
            sid_a,
            &SendMessageRequest {
                content: String::from(
                    "[SIM:TOOL:write_file:.git/hooks/pre-commit|#!/bin/sh\necho pwned]",
                ),
                timeout_ms: Some(15_000),
                client_message_id: None,
            },
            None,
        )
        .await
    });
    let perm5 = {
        let dl = time::Instant::now() + Duration::from_secs(10);
        loop {
            let list = h
                .client
                .list_permission_requests(Some("pending"), Some(sid_a), None)
                .await
                .expect("poll perm5");
            if let Some(p) = list.requests.first().cloned() {
                break p;
            }
            assert!(
                time::Instant::now() < dl,
                "expected .git/hooks/pre-commit write under Auto to require interactive approval"
            );
            time::sleep(Duration::from_millis(40)).await;
        }
    };
    let _ = h
        .client
        .resolve_permission_request(
            perm5.request_id,
            &ResolvePermissionRequest {
                expected_revision: Some(perm5.revision),
                decision: PermissionResolutionDecision::Deny,
                reason: Some(String::from("block hook planting")),
                max_uses: None,
                expires_in_secs: None,
            },
            None,
        )
        .await
        .expect("deny perm5");
    let _ = h5.await.expect("join h5");
    assert!(
        !ws_dir
            .join(".git")
            .join("hooks")
            .join("pre-commit")
            .exists(),
        "denied .git/hooks/pre-commit write must NOT create file on disk"
    );

    h.cleanup().await;
}
