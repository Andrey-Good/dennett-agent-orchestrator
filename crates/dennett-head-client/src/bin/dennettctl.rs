//! Thin `dennettctl` CLI facade backed strictly by [`HeadClient`] -> `dennett-head` HTTP API.
//!
//! Architectural boundary (Architecture 80 §6, 82 §7, V06):
//! - Never connects to PostgreSQL directly and never links `dennett-head` or `dennett-memory-core`.
//! - Calls the exact same versioned Head HTTP API that future Desktop/Mobile UIs will call.

use std::collections::BTreeSet;
use std::env;
use std::process::ExitCode;
use std::str::FromStr;

use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg};
use dennett_contracts::{
    AgentId, ArtifactCompleteness, ArtifactId, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    BudgetExceedPolicy, CapabilityGrantId, CommitMemoryNoteRequest, CorrelationId,
    CreateAgentRequest, CreateArtifactRequest, CreatePolicyRuleRequest, CreateProjectRequest,
    CreateSessionRequest, CreateTaskRequest, DennettError, DispatchEffectRequest, EffectClaimId,
    HeadMemorySearchRequest, IdempotencyKey, InboxCardId, InboxResolutionAction,
    ManualEffectResolution, MemoryScope, MissedRunPolicy, PermissionRequestId,
    PermissionResolutionDecision, PolicyEffect, ProjectId, RebindProjectRequest,
    ReconcileEffectRequest, ReplyTaskRequest, ResolveInboxCardRequest, ResolvePermissionRequest,
    RevokeGrantRequest, RuntimeAuthProfile, ScheduleKind, SendAgentMessageRequest,
    SendMessageRequest, SessionId, SessionKind, SimEffectBehavior, TaskBudget, TaskId,
    TaskSchedule, TaskSignalRequest, ThinkingLevel, TimestampMs, UpdateAgentConfigRequest,
    UpdateProjectRequest, UpdateSessionRequest, UpdateTaskRequest, WorkspaceWritePolicy,
};
use dennett_head_client::HeadClient;
use serde::Serialize;

const DEFAULT_HEAD_URL: &str = "http://127.0.0.1:18081";
const ENV_HEAD_URL: &str = "DENNETT_HEAD_URL";

#[derive(Debug, Clone)]
struct GlobalOpts {
    url: String,
    json: bool,
    correlation_id: Option<CorrelationId>,
}

#[derive(Debug)]
enum Action {
    Help,
    Version,
    Health(GlobalOpts),
    RuntimeCatalog {
        g: GlobalOpts,
        refresh: bool,
    },
    AgentCreate {
        g: GlobalOpts,
        req: CreateAgentRequest,
    },
    AgentList(GlobalOpts),
    AgentUpdateConfig {
        g: GlobalOpts,
        agent_id: AgentId,
        req: UpdateAgentConfigRequest,
    },
    AgentHistory {
        g: GlobalOpts,
        agent_id: AgentId,
    },
    AgentSendMessage {
        g: GlobalOpts,
        recipient_agent_id: AgentId,
        req: SendAgentMessageRequest,
    },
    AgentInbox {
        g: GlobalOpts,
        agent_id: AgentId,
        undelivered_only: bool,
    },
    ProjectCreate {
        g: GlobalOpts,
        req: CreateProjectRequest,
    },
    ProjectList {
        g: GlobalOpts,
        include_archived: bool,
    },
    ProjectUpdate {
        g: GlobalOpts,
        project_id: ProjectId,
        req: UpdateProjectRequest,
    },
    ProjectFiles {
        g: GlobalOpts,
        project_id: ProjectId,
        subpath: Option<String>,
    },
    ProjectRebind {
        g: GlobalOpts,
        project_id: ProjectId,
        req: RebindProjectRequest,
    },
    ProjectDetach {
        g: GlobalOpts,
        project_id: ProjectId,
    },
    ProjectArchive {
        g: GlobalOpts,
        project_id: ProjectId,
    },
    ProjectUnarchive {
        g: GlobalOpts,
        project_id: ProjectId,
    },
    ProjectRemove {
        g: GlobalOpts,
        project_id: ProjectId,
    },
    ProjectDelete {
        g: GlobalOpts,
        project_id: ProjectId,
        confirm: bool,
    },
    SessionCreate {
        g: GlobalOpts,
        req: CreateSessionRequest,
    },
    SessionList(GlobalOpts),
    SessionShow {
        g: GlobalOpts,
        session_id: SessionId,
    },
    SessionUpdate {
        g: GlobalOpts,
        session_id: SessionId,
        req: UpdateSessionRequest,
    },
    Send {
        g: GlobalOpts,
        session_id: SessionId,
        req: SendMessageRequest,
    },
    Cancel {
        g: GlobalOpts,
        session_id: SessionId,
    },
    ArtifactCreate {
        g: GlobalOpts,
        session_id: SessionId,
        req: CreateArtifactRequest,
    },
    ArtifactList {
        g: GlobalOpts,
        session_id: SessionId,
    },
    ArtifactRevisions {
        g: GlobalOpts,
        session_id: SessionId,
        key: String,
    },
    ArtifactLifecycle {
        g: GlobalOpts,
        session_id: SessionId,
        artifact_id: ArtifactId,
        req: ArtifactLifecycleRequest,
    },
    TaskCreate {
        g: GlobalOpts,
        req: CreateTaskRequest,
    },
    TaskList {
        g: GlobalOpts,
        project_id: Option<ProjectId>,
        status: Option<String>,
    },
    TaskShow {
        g: GlobalOpts,
        task_id: TaskId,
    },
    TaskUpdate {
        g: GlobalOpts,
        task_id: TaskId,
        req: UpdateTaskRequest,
    },
    TaskSignal {
        g: GlobalOpts,
        task_id: TaskId,
        req: TaskSignalRequest,
    },
    TaskReply {
        g: GlobalOpts,
        task_id: TaskId,
        req: ReplyTaskRequest,
    },
    TaskCancel {
        g: GlobalOpts,
        task_id: TaskId,
    },
    InboxList {
        g: GlobalOpts,
        status: Option<String>,
        project_id: Option<ProjectId>,
    },
    InboxResolve {
        g: GlobalOpts,
        card_id: InboxCardId,
        req: ResolveInboxCardRequest,
    },
    PermissionList {
        g: GlobalOpts,
        status: Option<String>,
        session_id: Option<SessionId>,
    },
    PermissionResolve {
        g: GlobalOpts,
        request_id: PermissionRequestId,
        req: ResolvePermissionRequest,
    },
    GrantList {
        g: GlobalOpts,
        project_id: Option<ProjectId>,
        include_revoked: bool,
    },
    GrantRevoke {
        g: GlobalOpts,
        grant_id: CapabilityGrantId,
        req: RevokeGrantRequest,
    },
    EffectDispatch {
        g: GlobalOpts,
        req: DispatchEffectRequest,
    },
    EffectList {
        g: GlobalOpts,
        task_id: Option<TaskId>,
        session_id: Option<SessionId>,
        status: Option<String>,
    },
    EffectReconcile {
        g: GlobalOpts,
        effect_id: EffectClaimId,
        req: ReconcileEffectRequest,
    },
    EventsSnapshot {
        g: GlobalOpts,
        project_id: Option<ProjectId>,
        include_archived: bool,
    },
    EventsTail {
        g: GlobalOpts,
        after_cursor: i64,
        epoch: Option<i64>,
        project_id: Option<ProjectId>,
        limit: Option<u32>,
    },
    PolicyAdd {
        g: GlobalOpts,
        req: CreatePolicyRuleRequest,
    },
    PolicyList(GlobalOpts),
    MemoryCommit {
        g: GlobalOpts,
        req: CommitMemoryNoteRequest,
    },
    MemoryFlushOutbox(GlobalOpts),
    MemoryOutbox(GlobalOpts),
    MemorySearch {
        g: GlobalOpts,
        req: HeadMemorySearchRequest,
    },
    Diagnostics {
        g: GlobalOpts,
        limit: Option<i64>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let default_url = env::var(ENV_HEAD_URL)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from(DEFAULT_HEAD_URL));

    let action = match parse_cli(env::args().skip(1), &default_url) {
        Ok(a) => a,
        Err(err) => {
            emit_error(&err, false);
            return exit_code_for_error(&err);
        }
    };

    match action {
        Action::Help => {
            print_usage();
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("dennettctl {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        other => {
            let json_mode = action_json_mode(&other);
            match run_action(other).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(err) => {
                    emit_error(&err, json_mode);
                    exit_code_for_error(&err)
                }
            }
        }
    }
}

const fn action_json_mode(action: &Action) -> bool {
    match action {
        Action::Help | Action::Version => false,
        Action::Health(g)
        | Action::RuntimeCatalog { g, .. }
        | Action::AgentCreate { g, .. }
        | Action::AgentList(g)
        | Action::AgentUpdateConfig { g, .. }
        | Action::AgentHistory { g, .. }
        | Action::AgentSendMessage { g, .. }
        | Action::AgentInbox { g, .. }
        | Action::ProjectCreate { g, .. }
        | Action::ProjectList { g, .. }
        | Action::ProjectUpdate { g, .. }
        | Action::ProjectFiles { g, .. }
        | Action::ProjectRebind { g, .. }
        | Action::ProjectDetach { g, .. }
        | Action::ProjectArchive { g, .. }
        | Action::ProjectUnarchive { g, .. }
        | Action::ProjectRemove { g, .. }
        | Action::ProjectDelete { g, .. }
        | Action::SessionCreate { g, .. }
        | Action::SessionList(g)
        | Action::SessionShow { g, .. }
        | Action::SessionUpdate { g, .. }
        | Action::Send { g, .. }
        | Action::Cancel { g, .. }
        | Action::ArtifactCreate { g, .. }
        | Action::ArtifactList { g, .. }
        | Action::ArtifactRevisions { g, .. }
        | Action::ArtifactLifecycle { g, .. }
        | Action::TaskCreate { g, .. }
        | Action::TaskList { g, .. }
        | Action::TaskShow { g, .. }
        | Action::TaskUpdate { g, .. }
        | Action::TaskSignal { g, .. }
        | Action::TaskReply { g, .. }
        | Action::TaskCancel { g, .. }
        | Action::InboxList { g, .. }
        | Action::InboxResolve { g, .. }
        | Action::PermissionList { g, .. }
        | Action::PermissionResolve { g, .. }
        | Action::GrantList { g, .. }
        | Action::GrantRevoke { g, .. }
        | Action::EffectDispatch { g, .. }
        | Action::EffectList { g, .. }
        | Action::EffectReconcile { g, .. }
        | Action::EventsSnapshot { g, .. }
        | Action::EventsTail { g, .. }
        | Action::PolicyAdd { g, .. }
        | Action::PolicyList(g)
        | Action::MemoryCommit { g, .. }
        | Action::MemoryFlushOutbox(g)
        | Action::MemoryOutbox(g)
        | Action::MemorySearch { g, .. }
        | Action::Diagnostics { g, .. } => g.json,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "single flat command dispatcher maps every CLI subcommand to its HeadClient call"
)]
async fn run_action(action: Action) -> Result<(), DennettError> {
    match action {
        Action::Help | Action::Version => Ok(()),
        Action::Health(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.healthz(g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::RuntimeCatalog { g, refresh } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_runtime_catalog(refresh, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::AgentCreate { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.create_agent(&req, g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::AgentList(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.list_agents(g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::AgentUpdateConfig { g, agent_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .update_agent_config(agent_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::AgentHistory { g, agent_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_agent_config_history(agent_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::AgentSendMessage {
            g,
            recipient_agent_id,
            req,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .send_agent_message(recipient_agent_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::AgentInbox {
            g,
            agent_id,
            undelivered_only,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_agent_messages(agent_id, undelivered_only, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectCreate { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .create_project(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectList {
            g,
            include_archived,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_projects_with_options(include_archived, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectUpdate { g, project_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .update_project(project_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectFiles {
            g,
            project_id,
            subpath,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_project_files(project_id, subpath.as_deref(), g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectRebind { g, project_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .rebind_project(project_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectDetach { g, project_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .detach_project_workspace(project_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectArchive { g, project_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .archive_project(project_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectUnarchive { g, project_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .unarchive_project(project_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectRemove { g, project_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .remove_project(project_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectDelete {
            g,
            project_id,
            confirm,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .delete_project(project_id, confirm, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::SessionCreate { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .create_session(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::SessionList(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.list_sessions(g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::SessionShow { g, session_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_session_detail(session_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::SessionUpdate { g, session_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .update_session(session_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::Send { g, session_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .send_message(session_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::Cancel { g, session_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .cancel_run(session_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ArtifactCreate { g, session_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .create_artifact(session_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ArtifactList { g, session_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_artifacts(session_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ArtifactRevisions { g, session_id, key } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_artifact_revisions(session_id, &key, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ArtifactLifecycle {
            g,
            session_id,
            artifact_id,
            req,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .apply_artifact_lifecycle(session_id, artifact_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskCreate { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.create_task(&req, g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::TaskList {
            g,
            project_id,
            status,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_tasks(project_id, status.as_deref(), g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskShow { g, task_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_task_detail(task_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskUpdate { g, task_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .update_task(task_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskSignal { g, task_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .send_task_signal(task_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskReply { g, task_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .reply_to_task(task_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::TaskCancel { g, task_id } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .cancel_task(task_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::InboxList {
            g,
            status,
            project_id,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_inbox_cards(status.as_deref(), project_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::InboxResolve { g, card_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .resolve_inbox_card(card_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::PermissionList {
            g,
            status,
            session_id,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_permission_requests(status.as_deref(), session_id, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::PermissionResolve { g, request_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .resolve_permission_request(request_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::GrantList {
            g,
            project_id,
            include_revoked,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_capability_grants(project_id, include_revoked, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::GrantRevoke { g, grant_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .revoke_capability_grant(grant_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::EffectDispatch { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .dispatch_effect(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::EffectList {
            g,
            task_id,
            session_id,
            status,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .list_effects(
                    task_id,
                    session_id,
                    status.as_deref(),
                    g.correlation_id.as_ref(),
                )
                .await?;
            print_output(&resp, g.json)
        }
        Action::EffectReconcile { g, effect_id, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .reconcile_effect(effect_id, &req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::EventsSnapshot {
            g,
            project_id,
            include_archived,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_state_snapshot(project_id, include_archived, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::EventsTail {
            g,
            after_cursor,
            epoch,
            project_id,
            limit,
        } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_events(
                    after_cursor,
                    epoch,
                    project_id,
                    limit,
                    g.correlation_id.as_ref(),
                )
                .await?;
            print_output(&resp, g.json)
        }
        Action::PolicyAdd { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .create_policy_rule(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::PolicyList(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.list_policy_rules(g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::MemoryCommit { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .commit_memory_note(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::MemoryFlushOutbox(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .flush_memory_outbox(g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::MemoryOutbox(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.list_memory_outbox(g.correlation_id.as_ref()).await?;
            print_output(&resp, g.json)
        }
        Action::MemorySearch { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .search_memory(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::Diagnostics { g, limit } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .get_diagnostics(limit, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
    }
}

fn parse_cli<I>(args: I, default_url: &str) -> Result<Action, DennettError>
where
    I: IntoIterator<Item = String>,
{
    let mut g = GlobalOpts {
        url: default_url.to_owned(),
        json: false,
        correlation_id: None,
    };
    let mut rest: Vec<String> = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--json" => g.json = true,
            "--url" => g.url = next_val(&mut iter, "--url")?,
            "--correlation-id" => {
                let raw = next_val(&mut iter, "--correlation-id")?;
                g.correlation_id = Some(CorrelationId::try_new(raw)?);
            }
            _ => rest.push(arg),
        }
    }

    let mut pos = rest.into_iter();
    let Some(cmd) = pos.next() else {
        return Ok(Action::Help);
    };

    match cmd.as_str() {
        "health" => {
            ensure_no_extra(&mut pos)?;
            Ok(Action::Health(g))
        }
        "runtime" => parse_runtime_subcommand(g, &mut pos),
        "agent" => parse_agent_subcommand(g, &mut pos),
        "project" => parse_project_subcommand(g, &mut pos),
        "session" => parse_session_subcommand(g, &mut pos),
        "send" => parse_send(g, &mut pos),
        "cancel" => parse_cancel(g, &mut pos),
        "artifact" => parse_artifact_subcommand(g, &mut pos),
        "task" => parse_task_subcommand(g, &mut pos),
        "inbox" => parse_inbox_subcommand(g, &mut pos),
        "permission" | "approval" => parse_permission_subcommand(g, &mut pos),
        "grant" => parse_grant_subcommand(g, &mut pos),
        "effect" => parse_effect_subcommand(g, &mut pos),
        "events" => parse_events_subcommand(g, &mut pos),
        "policy" => parse_policy_subcommand(g, &mut pos),
        "memory" => parse_memory_subcommand(g, &mut pos),
        "diagnostics" => parse_diagnostics(g, &mut pos),
        other => Err(DennettError::InvalidInput {
            field: String::from("command"),
            reason: format!("unrecognized command `{}`", sanitize_cli_arg(other)),
        }),
    }
}

fn parse_runtime_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "runtime <subcommand>")?;
    match sub.as_str() {
        "catalog" => {
            let mut refresh = false;
            for flag in iter.by_ref() {
                match flag.as_str() {
                    "--refresh" => refresh = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::RuntimeCatalog { g, refresh })
        }
        other => Err(unknown_flag(other)),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses all agent subcommands including D2 config budget/skills and inter-agent messages"
)]
fn parse_agent_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "agent <subcommand>")?;
    match sub.as_str() {
        "list" => {
            ensure_no_extra(iter)?;
            Ok(Action::AgentList(g))
        }
        "create" => {
            let mut name = None;
            let mut description = String::new();
            let mut system_prompt = None;
            let mut model_preference = None;
            let mut thinking_level = None;
            let mut scopes = BTreeSet::new();
            let mut write_policy = WorkspaceWritePolicy::AskBeforeWrite;
            let mut attached_skills = Vec::new();
            let mut max_turns: Option<u32> = None;
            let mut max_tokens: Option<u64> = None;
            let mut max_wall_secs: Option<u64> = None;

            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--description" => description = next_val(iter, "--description")?,
                    "--system-prompt" => system_prompt = Some(next_val(iter, "--system-prompt")?),
                    "--model" => model_preference = Some(next_val(iter, "--model")?),
                    "--thinking" => {
                        thinking_level =
                            Some(ThinkingLevel::from_str(&next_val(iter, "--thinking")?)?);
                    }
                    "--scope" => {
                        scopes.insert(MemoryScope::try_new(next_val(iter, "--scope")?)?);
                    }
                    "--write-policy" => {
                        write_policy = parse_write_policy(&next_val(iter, "--write-policy")?)?;
                    }
                    "--skill" => attached_skills.push(next_val(iter, "--skill")?),
                    "--max-turns" => {
                        max_turns =
                            Some(parse_u32(&next_val(iter, "--max-turns")?, "--max-turns")?);
                    }
                    "--max-tokens" => {
                        max_tokens =
                            Some(parse_u64(&next_val(iter, "--max-tokens")?, "--max-tokens")?);
                    }
                    "--max-wall-secs" => {
                        max_wall_secs = Some(parse_u64(
                            &next_val(iter, "--max-wall-secs")?,
                            "--max-wall-secs",
                        )?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let default_budget =
                if max_turns.is_some() || max_tokens.is_some() || max_wall_secs.is_some() {
                    Some(TaskBudget {
                        max_turns,
                        max_tokens,
                        max_wall_secs,
                        exceed_policy: BudgetExceedPolicy::AskApproval,
                    })
                } else {
                    None
                };
            let req = CreateAgentRequest {
                name: req_opt(name, "--name")?,
                description,
                system_prompt: req_opt(system_prompt, "--system-prompt")?,
                model_preference,
                thinking_level,
                allowed_memory_scopes: scopes,
                workspace_write_policy: write_policy,
                default_budget,
                attached_skills,
            };
            req.validate()?;
            Ok(Action::AgentCreate { g, req })
        }
        "update-config" => {
            let mut id = None;
            let mut system_prompt = None;
            let mut model_preference = None;
            let mut thinking_level = None;
            let mut scopes = BTreeSet::new();
            let mut write_policy = WorkspaceWritePolicy::AskBeforeWrite;
            let mut attached_skills = Vec::new();
            let mut max_turns: Option<u32> = None;
            let mut max_tokens: Option<u64> = None;
            let mut max_wall_secs: Option<u64> = None;

            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(AgentId::from_str(&next_val(iter, "--id")?)?),
                    "--system-prompt" => system_prompt = Some(next_val(iter, "--system-prompt")?),
                    "--model" => model_preference = Some(next_val(iter, "--model")?),
                    "--thinking" => {
                        thinking_level =
                            Some(ThinkingLevel::from_str(&next_val(iter, "--thinking")?)?);
                    }
                    "--scope" => {
                        scopes.insert(MemoryScope::try_new(next_val(iter, "--scope")?)?);
                    }
                    "--write-policy" => {
                        write_policy = parse_write_policy(&next_val(iter, "--write-policy")?)?;
                    }
                    "--skill" => attached_skills.push(next_val(iter, "--skill")?),
                    "--max-turns" => {
                        max_turns =
                            Some(parse_u32(&next_val(iter, "--max-turns")?, "--max-turns")?);
                    }
                    "--max-tokens" => {
                        max_tokens =
                            Some(parse_u64(&next_val(iter, "--max-tokens")?, "--max-tokens")?);
                    }
                    "--max-wall-secs" => {
                        max_wall_secs = Some(parse_u64(
                            &next_val(iter, "--max-wall-secs")?,
                            "--max-wall-secs",
                        )?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let default_budget =
                if max_turns.is_some() || max_tokens.is_some() || max_wall_secs.is_some() {
                    Some(TaskBudget {
                        max_turns,
                        max_tokens,
                        max_wall_secs,
                        exceed_policy: BudgetExceedPolicy::AskApproval,
                    })
                } else {
                    None
                };
            let req = UpdateAgentConfigRequest {
                system_prompt: req_opt(system_prompt, "--system-prompt")?,
                model_preference,
                thinking_level,
                allowed_memory_scopes: scopes,
                workspace_write_policy: write_policy,
                default_budget,
                attached_skills,
            };
            req.validate()?;
            Ok(Action::AgentUpdateConfig {
                g,
                agent_id: req_opt(id, "--id")?,
                req,
            })
        }
        "history" => {
            let mut id = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(AgentId::from_str(&next_val(iter, "--id")?)?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::AgentHistory {
                g,
                agent_id: req_opt(id, "--id")?,
            })
        }
        "send-message" => {
            let mut recipient = None;
            let mut sender = None;
            let mut project = None;
            let mut related_task = None;
            let mut body = None;
            let mut idempotency_key = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--to" => recipient = Some(AgentId::from_str(&next_val(iter, "--to")?)?),
                    "--from" => sender = Some(AgentId::from_str(&next_val(iter, "--from")?)?),
                    "--project" => {
                        project = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--task" => related_task = Some(TaskId::from_str(&next_val(iter, "--task")?)?),
                    "--body" => body = Some(next_val(iter, "--body")?),
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = SendAgentMessageRequest {
                sender_agent_id: sender,
                project_id: req_opt(project, "--project")?,
                related_task_id: related_task,
                body_text: req_opt(body, "--body")?,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            };
            req.validate()?;
            Ok(Action::AgentSendMessage {
                g,
                recipient_agent_id: req_opt(recipient, "--to")?,
                req,
            })
        }
        "inbox" => {
            let mut id = None;
            let mut undelivered_only = false;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(AgentId::from_str(&next_val(iter, "--id")?)?),
                    "--undelivered" => undelivered_only = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::AgentInbox {
                g,
                agent_id: req_opt(id, "--id")?,
                undelivered_only,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses all Contract C project lifecycle subcommands cleanly"
)]
fn parse_project_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "project <subcommand>")?;
    match sub.as_str() {
        "list" => {
            let mut include_archived = false;
            for flag in iter.by_ref() {
                match flag.as_str() {
                    "--include-archived" => include_archived = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ProjectList {
                g,
                include_archived,
            })
        }
        "create" => {
            let mut name = None;
            let mut workspace = None;
            let mut scope = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--workspace" => workspace = Some(next_val(iter, "--workspace")?),
                    "--scope" => scope = Some(MemoryScope::try_new(next_val(iter, "--scope")?)?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateProjectRequest {
                name: req_opt(name, "--name")?,
                workspace_path: workspace,
                memory_scope: scope,
            };
            req.validate()?;
            Ok(Action::ProjectCreate { g, req })
        }
        "update" => {
            let mut id = None;
            let mut name = None;
            let mut workspace = None;
            let mut pinned: Option<bool> = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(ProjectId::from_str(&next_val(iter, "--id")?)?),
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--workspace" => workspace = Some(next_val(iter, "--workspace")?),
                    "--pin" => {
                        if pinned == Some(false) {
                            return Err(DennettError::InvalidInput {
                                field: String::from("pinned"),
                                reason: String::from("cannot specify both `--pin` and `--unpin`"),
                            });
                        }
                        pinned = Some(true);
                    }
                    "--unpin" => {
                        if pinned == Some(true) {
                            return Err(DennettError::InvalidInput {
                                field: String::from("pinned"),
                                reason: String::from("cannot specify both `--pin` and `--unpin`"),
                            });
                        }
                        pinned = Some(false);
                    }
                    other if id.is_none() && !other.starts_with("--") => {
                        id = Some(ProjectId::from_str(other)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = UpdateProjectRequest {
                name,
                workspace_path: workspace,
                pinned,
            };
            req.validate()?;
            Ok(Action::ProjectUpdate {
                g,
                project_id: req_opt(id, "--id")?,
                req,
            })
        }
        "files" => {
            let mut id = None;
            let mut subpath = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(ProjectId::from_str(&next_val(iter, "--id")?)?),
                    "--subpath" => subpath = Some(next_val(iter, "--subpath")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ProjectFiles {
                g,
                project_id: req_opt(id, "--id")?,
                subpath,
            })
        }
        "rebind" => {
            let mut id = None;
            let mut workspace = None;
            let mut reason = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(ProjectId::from_str(&next_val(iter, "--id")?)?),
                    "--workspace" => workspace = Some(next_val(iter, "--workspace")?),
                    "--reason" => reason = Some(next_val(iter, "--reason")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = RebindProjectRequest {
                workspace_path: req_opt(workspace, "--workspace")?,
                reason,
            };
            req.validate()?;
            Ok(Action::ProjectRebind {
                g,
                project_id: req_opt(id, "--id")?,
                req,
            })
        }
        "detach" => {
            let id = parse_only_id::<ProjectId>(iter, "--id")?;
            Ok(Action::ProjectDetach { g, project_id: id })
        }
        "archive" => {
            let id = parse_only_id::<ProjectId>(iter, "--id")?;
            Ok(Action::ProjectArchive { g, project_id: id })
        }
        "unarchive" => {
            let id = parse_only_id::<ProjectId>(iter, "--id")?;
            Ok(Action::ProjectUnarchive { g, project_id: id })
        }
        "remove" => {
            let id = parse_only_id::<ProjectId>(iter, "--id")?;
            Ok(Action::ProjectRemove { g, project_id: id })
        }
        "delete" => {
            let mut id = None;
            let mut confirm = false;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(ProjectId::from_str(&next_val(iter, "--id")?)?),
                    "--confirm" => confirm = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ProjectDelete {
                g,
                project_id: req_opt(id, "--id")?,
                confirm,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_session_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "session <subcommand>")?;
    match sub.as_str() {
        "list" => {
            ensure_no_extra(iter)?;
            Ok(Action::SessionList(g))
        }
        "create" => {
            let mut agent_id = None;
            let mut project_id = None;
            let mut title = None;
            let mut session_kind = SessionKind::UserChat;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--agent" => agent_id = Some(AgentId::from_str(&next_val(iter, "--agent")?)?),
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--title" => title = Some(next_val(iter, "--title")?),
                    "--kind" => {
                        session_kind = SessionKind::from_str(&next_val(iter, "--kind")?)?;
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateSessionRequest {
                agent_id: req_opt(agent_id, "--agent")?,
                project_id,
                title: title.unwrap_or_else(|| String::from("Новый чат")),
                session_kind,
            };
            req.validate()?;
            Ok(Action::SessionCreate { g, req })
        }
        "show" => {
            let mut id = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(SessionId::from_str(&next_val(iter, "--id")?)?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::SessionShow {
                g,
                session_id: req_opt(id, "--id")?,
            })
        }
        "update" => {
            let mut id = None;
            let mut title = None;
            let mut pinned: Option<bool> = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(SessionId::from_str(&next_val(iter, "--id")?)?),
                    "--title" => title = Some(next_val(iter, "--title")?),
                    "--pin" => {
                        if pinned == Some(false) {
                            return Err(DennettError::InvalidInput {
                                field: String::from("pinned"),
                                reason: String::from("cannot specify both `--pin` and `--unpin`"),
                            });
                        }
                        pinned = Some(true);
                    }
                    "--unpin" => {
                        if pinned == Some(true) {
                            return Err(DennettError::InvalidInput {
                                field: String::from("pinned"),
                                reason: String::from("cannot specify both `--pin` and `--unpin`"),
                            });
                        }
                        pinned = Some(false);
                    }
                    other if id.is_none() && !other.starts_with("--") => {
                        id = Some(SessionId::from_str(other)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = UpdateSessionRequest { title, pinned };
            req.validate()?;
            Ok(Action::SessionUpdate {
                g,
                session_id: req_opt(id, "--id")?,
                req,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_send(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let mut session_id = None;
    let mut message = None;
    let mut timeout_ms = None;
    let mut client_message_id = None;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--session" => session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?),
            "--message" => message = Some(next_val(iter, "--message")?),
            "--client-message-id" => {
                client_message_id = Some(next_val(iter, "--client-message-id")?);
            }
            "--timeout-ms" => {
                timeout_ms = Some(parse_u64(&next_val(iter, "--timeout-ms")?, "--timeout-ms")?);
            }
            other => return Err(unknown_flag(other)),
        }
    }
    let req = SendMessageRequest {
        content: req_opt(message, "--message")?,
        timeout_ms,
        client_message_id,
    };
    req.validate()?;
    Ok(Action::Send {
        g,
        session_id: req_opt(session_id, "--session")?,
        req,
    })
}

fn parse_cancel(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let mut session_id = None;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--session" => session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?),
            other => return Err(unknown_flag(other)),
        }
    }
    Ok(Action::Cancel {
        g,
        session_id: req_opt(session_id, "--session")?,
    })
}

fn parse_artifact_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "artifact <subcommand>")?;
    match sub.as_str() {
        "list" => {
            let session_id = parse_only_id::<SessionId>(iter, "--session")?;
            Ok(Action::ArtifactList { g, session_id })
        }
        "create" => {
            let mut session_id = None;
            let mut name = None;
            let mut mime_type = String::from("text/plain");
            let mut content = None;
            let mut relative_path = None;
            let mut completeness_status = ArtifactCompleteness::Final;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--mime-type" => mime_type = next_val(iter, "--mime-type")?,
                    "--content" => content = Some(next_val(iter, "--content")?),
                    "--path" => relative_path = Some(next_val(iter, "--path")?),
                    "--status" => {
                        completeness_status =
                            ArtifactCompleteness::from_str(&next_val(iter, "--status")?)?;
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateArtifactRequest {
                run_id: None,
                name: req_opt(name, "--name")?,
                mime_type,
                content,
                relative_path,
                completeness_status,
            };
            req.validate()?;
            Ok(Action::ArtifactCreate {
                g,
                session_id: req_opt(session_id, "--session")?,
                req,
            })
        }
        "revisions" => {
            let mut session_id = None;
            let mut key = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--id" | "--name" => key = Some(next_val(iter, &flag)?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ArtifactRevisions {
                g,
                session_id: req_opt(session_id, "--session")?,
                key: req_opt(key, "--id|--name")?,
            })
        }
        "finalize" | "archive" | "unarchive" | "remove" | "delete" => {
            let action = match sub.as_str() {
                "finalize" => ArtifactLifecycleAction::Finalize,
                "archive" => ArtifactLifecycleAction::Archive,
                "unarchive" => ArtifactLifecycleAction::Unarchive,
                "remove" => ArtifactLifecycleAction::Remove,
                _ => ArtifactLifecycleAction::Delete,
            };
            let mut session_id = None;
            let mut artifact_id = None;
            let mut reason = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--id" => {
                        artifact_id = Some(ArtifactId::from_str(&next_val(iter, "--id")?)?);
                    }
                    "--reason" => reason = Some(next_val(iter, "--reason")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ArtifactLifecycle {
                g,
                session_id: req_opt(session_id, "--session")?,
                artifact_id: req_opt(artifact_id, "--id")?,
                req: ArtifactLifecycleRequest { action, reason },
            })
        }
        other => Err(unknown_flag(other)),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses all durable task subcommands (create/list/show/update/signal/reply/cancel)"
)]
fn parse_task_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "task <subcommand>")?;
    match sub.as_str() {
        "create" => {
            let mut project_id = None;
            let mut agent_id = None;
            let mut session_id = None;
            let mut parent_task_id = None;
            let mut title = None;
            let mut goal = None;
            let mut fire_at_ms: Option<i64> = None;
            let mut interval_secs: Option<u64> = None;
            let mut daily_time: Option<(u8, u8)> = None;
            let mut weekly_spec: Option<(u8, u8, u8)> = None;
            let mut timezone = String::from("UTC");
            let mut misfire = MissedRunPolicy::ExecuteOnce;
            let mut wait_for_signal: Option<String> = None;
            let mut max_turns: Option<u32> = None;
            let mut max_tokens: Option<u64> = None;
            let mut max_wall_secs: Option<u64> = None;
            let mut exceed_policy = BudgetExceedPolicy::AskApproval;
            let mut attached_skills = Vec::new();
            let mut auth_profile_override = None;
            let mut execute_immediately = None;

            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--agent" => agent_id = Some(AgentId::from_str(&next_val(iter, "--agent")?)?),
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--parent-task" => {
                        parent_task_id = Some(TaskId::from_str(&next_val(iter, "--parent-task")?)?);
                    }
                    "--title" => title = Some(next_val(iter, "--title")?),
                    "--goal" => goal = Some(next_val(iter, "--goal")?),
                    "--fire-at-ms" => {
                        fire_at_ms =
                            Some(parse_i64(&next_val(iter, "--fire-at-ms")?, "--fire-at-ms")?);
                    }
                    "--interval-secs" => {
                        interval_secs = Some(parse_u64(
                            &next_val(iter, "--interval-secs")?,
                            "--interval-secs",
                        )?);
                    }
                    "--daily" => {
                        daily_time = Some(parse_hhmm(&next_val(iter, "--daily")?)?);
                    }
                    "--weekly" => {
                        weekly_spec = Some(parse_weekly_spec(&next_val(iter, "--weekly")?)?);
                    }
                    "--tz" => timezone = next_val(iter, "--tz")?,
                    "--misfire" => {
                        misfire = MissedRunPolicy::from_str(&next_val(iter, "--misfire")?)?;
                    }
                    "--wait-for-signal" => {
                        wait_for_signal = Some(next_val(iter, "--wait-for-signal")?);
                    }
                    "--max-turns" => {
                        max_turns =
                            Some(parse_u32(&next_val(iter, "--max-turns")?, "--max-turns")?);
                    }
                    "--max-tokens" => {
                        max_tokens =
                            Some(parse_u64(&next_val(iter, "--max-tokens")?, "--max-tokens")?);
                    }
                    "--max-wall-secs" => {
                        max_wall_secs = Some(parse_u64(
                            &next_val(iter, "--max-wall-secs")?,
                            "--max-wall-secs",
                        )?);
                    }
                    "--budget-policy" => {
                        exceed_policy =
                            parse_budget_exceed_policy(&next_val(iter, "--budget-policy")?)?;
                    }
                    "--skill" => attached_skills.push(next_val(iter, "--skill")?),
                    "--auth-profile" => {
                        auth_profile_override =
                            Some(parse_auth_profile(&next_val(iter, "--auth-profile")?)?);
                    }
                    "--no-auto-run" => execute_immediately = Some(false),
                    other => return Err(unknown_flag(other)),
                }
            }

            let schedule = build_optional_schedule(
                fire_at_ms,
                interval_secs,
                daily_time,
                weekly_spec,
                timezone,
                misfire,
            )?;
            let budget = if max_turns.is_some()
                || max_tokens.is_some()
                || max_wall_secs.is_some()
                || exceed_policy != BudgetExceedPolicy::AskApproval
            {
                Some(TaskBudget {
                    max_turns,
                    max_tokens,
                    max_wall_secs,
                    exceed_policy,
                })
            } else {
                None
            };

            let req = CreateTaskRequest {
                project_id: req_opt(project_id, "--project")?,
                agent_id: req_opt(agent_id, "--agent")?,
                session_id,
                parent_task_id,
                title,
                goal: req_opt(goal, "--goal")?,
                schedule,
                wait_for_signal,
                budget,
                attached_skills,
                auth_profile_override,
                execute_immediately,
            };
            req.validate()?;
            Ok(Action::TaskCreate { g, req })
        }
        "list" => {
            let mut project_id = None;
            let mut status = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--status" => status = Some(next_val(iter, "--status")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::TaskList {
                g,
                project_id,
                status,
            })
        }
        "show" => {
            let task_id = parse_only_id::<TaskId>(iter, "--id")?;
            Ok(Action::TaskShow { g, task_id })
        }
        "update" => {
            let mut id = None;
            let mut title = None;
            let mut goal = None;
            let mut clear_schedule = false;
            let mut fire_at_ms = None;
            let mut interval_secs = None;
            let mut daily_time = None;
            let mut weekly_spec = None;
            let mut timezone = String::from("UTC");
            let mut misfire = MissedRunPolicy::ExecuteOnce;
            let mut max_turns: Option<u32> = None;
            let mut max_tokens: Option<u64> = None;
            let mut max_wall_secs: Option<u64> = None;
            let mut exceed_policy: Option<BudgetExceedPolicy> = None;
            let mut execute_immediately = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(TaskId::from_str(&next_val(iter, "--id")?)?),
                    "--title" => title = Some(next_val(iter, "--title")?),
                    "--goal" => goal = Some(next_val(iter, "--goal")?),
                    "--clear-schedule" => clear_schedule = true,
                    "--fire-at-ms" => {
                        fire_at_ms =
                            Some(parse_i64(&next_val(iter, "--fire-at-ms")?, "--fire-at-ms")?);
                    }
                    "--interval-secs" => {
                        interval_secs = Some(parse_u64(
                            &next_val(iter, "--interval-secs")?,
                            "--interval-secs",
                        )?);
                    }
                    "--daily" => daily_time = Some(parse_hhmm(&next_val(iter, "--daily")?)?),
                    "--weekly" => {
                        weekly_spec = Some(parse_weekly_spec(&next_val(iter, "--weekly")?)?);
                    }
                    "--tz" => timezone = next_val(iter, "--tz")?,
                    "--misfire" => {
                        misfire = MissedRunPolicy::from_str(&next_val(iter, "--misfire")?)?;
                    }
                    "--max-turns" => {
                        max_turns =
                            Some(parse_u32(&next_val(iter, "--max-turns")?, "--max-turns")?);
                    }
                    "--max-tokens" => {
                        max_tokens =
                            Some(parse_u64(&next_val(iter, "--max-tokens")?, "--max-tokens")?);
                    }
                    "--max-wall-secs" => {
                        max_wall_secs = Some(parse_u64(
                            &next_val(iter, "--max-wall-secs")?,
                            "--max-wall-secs",
                        )?);
                    }
                    "--budget-policy" | "--exceed-policy" => {
                        exceed_policy =
                            Some(parse_budget_exceed_policy(&next_val(iter, flag.as_str())?)?);
                    }
                    "--no-auto-run" => execute_immediately = Some(false),
                    other => return Err(unknown_flag(other)),
                }
            }
            let schedule = build_optional_schedule(
                fire_at_ms,
                interval_secs,
                daily_time,
                weekly_spec,
                timezone,
                misfire,
            )?;
            let budget = if max_turns.is_some()
                || max_tokens.is_some()
                || max_wall_secs.is_some()
                || exceed_policy.is_some()
            {
                Some(TaskBudget {
                    max_turns,
                    max_tokens,
                    max_wall_secs,
                    exceed_policy: exceed_policy.unwrap_or(BudgetExceedPolicy::AskApproval),
                })
            } else {
                None
            };
            let req = UpdateTaskRequest {
                title,
                goal,
                schedule,
                clear_schedule,
                budget,
                execute_immediately,
            };
            req.validate()?;
            Ok(Action::TaskUpdate {
                g,
                task_id: req_opt(id, "--id")?,
                req,
            })
        }
        "signal" => {
            let mut id = None;
            let mut name = None;
            let mut payload = String::new();
            let mut idempotency_key = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(TaskId::from_str(&next_val(iter, "--id")?)?),
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--payload" => payload = next_val(iter, "--payload")?,
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = TaskSignalRequest {
                signal_name: req_opt(name, "--name")?,
                payload_text: payload,
                idempotency_key: req_opt(idempotency_key, "--idempotency-key")?,
            };
            req.validate()?;
            Ok(Action::TaskSignal {
                g,
                task_id: req_opt(id, "--id")?,
                req,
            })
        }
        "reply" => {
            let mut id = None;
            let mut answer = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(TaskId::from_str(&next_val(iter, "--id")?)?),
                    "--answer" => answer = Some(next_val(iter, "--answer")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = ReplyTaskRequest {
                answer_text: req_opt(answer, "--answer")?,
            };
            req.validate()?;
            Ok(Action::TaskReply {
                g,
                task_id: req_opt(id, "--id")?,
                req,
            })
        }
        "cancel" => {
            let task_id = parse_only_id::<TaskId>(iter, "--id")?;
            Ok(Action::TaskCancel { g, task_id })
        }
        other => Err(unknown_flag(other)),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses inbox list and resolve actions across all InboxResolutionAction variants"
)]
fn parse_inbox_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "inbox <subcommand>")?;
    match sub.as_str() {
        "list" => {
            let mut status = None;
            let mut project_id = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--status" => status = Some(next_val(iter, "--status")?),
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::InboxList {
                g,
                status,
                project_id,
            })
        }
        "resolve" => {
            let mut id = None;
            let mut expected_revision = None;
            let mut action_kind = None;
            let mut decision = None;
            let mut answer = None;
            let mut add_turns = None;
            let mut add_tokens = None;
            let mut add_wall_secs = None;
            let mut manual_effect = None;
            let mut note = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(InboxCardId::from_str(&next_val(iter, "--id")?)?),
                    "--revision" => {
                        expected_revision =
                            Some(parse_i64(&next_val(iter, "--revision")?, "--revision")?);
                    }
                    "--action" => action_kind = Some(next_val(iter, "--action")?),
                    "--decision" => {
                        decision = Some(PermissionResolutionDecision::from_str(&next_val(
                            iter,
                            "--decision",
                        )?)?);
                    }
                    "--answer" => answer = Some(next_val(iter, "--answer")?),
                    "--add-turns" => {
                        add_turns =
                            Some(parse_u32(&next_val(iter, "--add-turns")?, "--add-turns")?);
                    }
                    "--add-tokens" => {
                        add_tokens =
                            Some(parse_u64(&next_val(iter, "--add-tokens")?, "--add-tokens")?);
                    }
                    "--add-wall-secs" => {
                        add_wall_secs = Some(parse_u64(
                            &next_val(iter, "--add-wall-secs")?,
                            "--add-wall-secs",
                        )?);
                    }
                    "--manual-resolution" => {
                        manual_effect = Some(parse_manual_effect_resolution(&next_val(
                            iter,
                            "--manual-resolution",
                        )?)?);
                    }
                    "--note" => note = Some(next_val(iter, "--note")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let kind_str = req_opt(action_kind, "--action")?;
            let resolution = match kind_str.trim().to_ascii_lowercase().as_str() {
                "permission" => InboxResolutionAction::Permission {
                    decision: req_opt(decision, "--decision")?,
                    expires_in_secs: None,
                    max_uses: None,
                    reason: note,
                },
                "reply" | "clarification" | "reply_clarification" => {
                    InboxResolutionAction::Clarification {
                        answer_text: req_opt(answer, "--answer")?,
                    }
                }
                "increase_budget" => InboxResolutionAction::IncreaseBudget {
                    additional_turns: add_turns,
                    additional_tokens: add_tokens,
                    additional_wall_secs: add_wall_secs,
                },
                "run_now" | "run_missed_now" => {
                    InboxResolutionAction::MissedSchedule { run_now: true }
                }
                "skip_missed" => InboxResolutionAction::MissedSchedule { run_now: false },
                "reconcile_effect" => InboxResolutionAction::ReconcileEffect {
                    manual_resolution: manual_effect,
                    reason: note,
                },
                "stop_task" | "cancel_task" => InboxResolutionAction::StopTask { reason: note },
                other => {
                    return Err(DennettError::InvalidInput {
                        field: String::from("--action"),
                        reason: format!("unsupported inbox action `{other}`"),
                    });
                }
            };
            let req = ResolveInboxCardRequest {
                expected_revision: req_opt(expected_revision, "--revision")?,
                resolution,
            };
            Ok(Action::InboxResolve {
                g,
                card_id: req_opt(id, "--id")?,
                req,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_permission_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "permission <subcommand>")?;
    match sub.as_str() {
        "list" => {
            let mut status = None;
            let mut session_id = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--status" => status = Some(next_val(iter, "--status")?),
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::PermissionList {
                g,
                status,
                session_id,
            })
        }
        "resolve" => {
            let mut id = None;
            let mut decision = None;
            let mut ttl_secs = None;
            let mut max_uses = None;
            let mut reason = None;
            let mut expected_revision = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => {
                        id = Some(PermissionRequestId::from_str(&next_val(iter, "--id")?)?);
                    }
                    "--decision" => {
                        decision = Some(PermissionResolutionDecision::from_str(&next_val(
                            iter,
                            "--decision",
                        )?)?);
                    }
                    "--ttl-secs" => {
                        ttl_secs = Some(parse_u64(&next_val(iter, "--ttl-secs")?, "--ttl-secs")?);
                    }
                    "--max-uses" => {
                        max_uses = Some(parse_i32(&next_val(iter, "--max-uses")?, "--max-uses")?);
                    }
                    "--reason" => reason = Some(next_val(iter, "--reason")?),
                    "--revision" => {
                        expected_revision =
                            Some(parse_i64(&next_val(iter, "--revision")?, "--revision")?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = ResolvePermissionRequest {
                decision: req_opt(decision, "--decision")?,
                expires_in_secs: ttl_secs,
                max_uses,
                reason,
                expected_revision,
            };
            req.validate()?;
            Ok(Action::PermissionResolve {
                g,
                request_id: req_opt(id, "--id")?,
                req,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_grant_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "grant <subcommand>")?;
    match sub.as_str() {
        "list" => {
            let mut project_id = None;
            let mut include_revoked = false;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--include-revoked" => include_revoked = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::GrantList {
                g,
                project_id,
                include_revoked,
            })
        }
        "revoke" => {
            let mut id = None;
            let mut reason = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(CapabilityGrantId::from_str(&next_val(iter, "--id")?)?),
                    "--reason" => reason = Some(next_val(iter, "--reason")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::GrantRevoke {
                g,
                grant_id: req_opt(id, "--id")?,
                req: RevokeGrantRequest { reason },
            })
        }
        other => Err(unknown_flag(other)),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "parses effect dispatch, list, and reconcile subcommands cleanly"
)]
fn parse_effect_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "effect <subcommand>")?;
    match sub.as_str() {
        "dispatch" => {
            let mut project_id = None;
            let mut session_id = None;
            let mut task_id = None;
            let mut agent_id = None;
            let mut connector_id = None;
            let mut operation = None;
            let mut recipient = None;
            let mut params = String::from("{}");
            let mut idempotency_key = None;
            let mut behavior = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--task" => task_id = Some(TaskId::from_str(&next_val(iter, "--task")?)?),
                    "--agent" => agent_id = Some(AgentId::from_str(&next_val(iter, "--agent")?)?),
                    "--connector" => connector_id = Some(next_val(iter, "--connector")?),
                    "--operation" => operation = Some(next_val(iter, "--operation")?),
                    "--recipient" => recipient = Some(next_val(iter, "--recipient")?),
                    "--params" => params = next_val(iter, "--params")?,
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    "--behavior" => {
                        behavior =
                            Some(SimEffectBehavior::from_str(&next_val(iter, "--behavior")?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = DispatchEffectRequest {
                project_id: req_opt(project_id, "--project")?,
                session_id: req_opt(session_id, "--session")?,
                task_id,
                run_id: None,
                agent_id: req_opt(agent_id, "--agent")?,
                connector_id,
                operation: req_opt(operation, "--operation")?,
                target_recipient: req_opt(recipient, "--recipient")?,
                parameters_json: params,
                effect_idempotency_key: req_opt(idempotency_key, "--idempotency-key")?,
                simulated_behavior: behavior,
            };
            req.validate()?;
            Ok(Action::EffectDispatch { g, req })
        }
        "list" => {
            let mut task_id = None;
            let mut session_id = None;
            let mut status = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--task" => task_id = Some(TaskId::from_str(&next_val(iter, "--task")?)?),
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--status" => status = Some(next_val(iter, "--status")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::EffectList {
                g,
                task_id,
                session_id,
                status,
            })
        }
        "reconcile" => {
            let mut id = None;
            let mut manual_resolution = None;
            let mut note = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--id" => id = Some(EffectClaimId::from_str(&next_val(iter, "--id")?)?),
                    "--manual" => {
                        manual_resolution = Some(parse_manual_effect_resolution(&next_val(
                            iter, "--manual",
                        )?)?);
                    }
                    "--note" => note = Some(next_val(iter, "--note")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::EffectReconcile {
                g,
                effect_id: req_opt(id, "--id")?,
                req: ReconcileEffectRequest {
                    manual_resolution,
                    reason: note,
                },
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_events_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "events <subcommand>")?;
    match sub.as_str() {
        "snapshot" => {
            let mut project_id = None;
            let mut include_archived = false;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--include-archived" => include_archived = true,
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::EventsSnapshot {
                g,
                project_id,
                include_archived,
            })
        }
        "tail" => {
            let mut after_cursor: i64 = 0;
            let mut epoch = None;
            let mut project_id = None;
            let mut limit = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--after" => {
                        after_cursor = parse_i64(&next_val(iter, "--after")?, "--after")?;
                    }
                    "--epoch" => epoch = Some(parse_i64(&next_val(iter, "--epoch")?, "--epoch")?),
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--limit" => limit = Some(parse_u32(&next_val(iter, "--limit")?, "--limit")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::EventsTail {
                g,
                after_cursor,
                epoch,
                project_id,
                limit,
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_policy_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "policy <subcommand>")?;
    match sub.as_str() {
        "list" => {
            ensure_no_extra(iter)?;
            Ok(Action::PolicyList(g))
        }
        "add" => {
            let mut scope = String::from("global");
            let mut tool_name = None;
            let mut target_pattern = None;
            let mut effect = None;
            let mut reason = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--scope" => scope = next_val(iter, "--scope")?,
                    "--tool" => tool_name = Some(next_val(iter, "--tool")?),
                    "--target" => target_pattern = Some(next_val(iter, "--target")?),
                    "--effect" => {
                        effect = Some(PolicyEffect::from_str(&next_val(iter, "--effect")?)?);
                    }
                    "--reason" => reason = Some(next_val(iter, "--reason")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreatePolicyRuleRequest {
                scope,
                tool_name: req_opt(tool_name, "--tool")?,
                target_pattern,
                effect: req_opt(effect, "--effect")?,
                reason: req_opt(reason, "--reason")?,
            };
            req.validate()?;
            Ok(Action::PolicyAdd { g, req })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_memory_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "memory <subcommand>")?;
    match sub.as_str() {
        "flush-outbox" => {
            ensure_no_extra(iter)?;
            Ok(Action::MemoryFlushOutbox(g))
        }
        "outbox" => {
            ensure_no_extra(iter)?;
            Ok(Action::MemoryOutbox(g))
        }
        "commit" => {
            let mut scope = None;
            let mut title = None;
            let mut body = None;
            let mut session_id = None;
            let mut idempotency_key = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--scope" => scope = Some(MemoryScope::try_new(next_val(iter, "--scope")?)?),
                    "--title" => title = Some(next_val(iter, "--title")?),
                    "--body" => body = Some(next_val(iter, "--body")?),
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::MemoryCommit {
                g,
                req: CommitMemoryNoteRequest {
                    session_id,
                    run_id: None,
                    scope: req_opt(scope, "--scope")?,
                    title: req_opt(title, "--title")?,
                    body: req_opt(body, "--body")?,
                    tags: Vec::new(),
                    idempotency_key,
                },
            })
        }
        "search" => {
            let mut scope = None;
            let mut query = None;
            let mut limit = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--scope" => scope = Some(MemoryScope::try_new(next_val(iter, "--scope")?)?),
                    "--query" => query = Some(next_val(iter, "--query")?),
                    "--limit" => {
                        let raw = next_val(iter, "--limit")?;
                        limit =
                            Some(raw.parse::<u32>().map_err(|_| DennettError::InvalidInput {
                                field: String::from("--limit"),
                                reason: String::from("must be a positive integer"),
                            })?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::MemorySearch {
                g,
                req: HeadMemorySearchRequest {
                    scope,
                    query,
                    limit,
                },
            })
        }
        other => Err(unknown_flag(other)),
    }
}

fn parse_diagnostics(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let mut limit = None;
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--limit" => {
                let raw = next_val(iter, "--limit")?;
                limit = Some(raw.parse::<i64>().map_err(|_| DennettError::InvalidInput {
                    field: String::from("--limit"),
                    reason: String::from("must be a positive integer"),
                })?);
            }
            other => return Err(unknown_flag(other)),
        }
    }
    Ok(Action::Diagnostics { g, limit })
}

fn parse_write_policy(raw: &str) -> Result<WorkspaceWritePolicy, DennettError> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("deny") || trimmed.eq_ignore_ascii_case("deny_all") {
        return Ok(WorkspaceWritePolicy::DenyAll);
    }
    if trimmed.eq_ignore_ascii_case("ask") || trimmed.eq_ignore_ascii_case("ask_before_write") {
        return Ok(WorkspaceWritePolicy::AskBeforeWrite);
    }
    if trimmed.eq_ignore_ascii_case("workspace") || trimmed.eq_ignore_ascii_case("allow_workspace")
    {
        return Ok(WorkspaceWritePolicy::AllowWorkspace);
    }
    if let Some(rest) = trimmed.strip_prefix("paths:") {
        let prefixes: Vec<String> = rest
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        let policy = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: prefixes,
        };
        policy.validate()?;
        return Ok(policy);
    }
    Err(DennettError::InvalidInput {
        field: String::from("--write-policy"),
        reason: String::from("expected `deny`, `ask`, `workspace`, or `paths:dir1,dir2`"),
    })
}

fn parse_only_id<T: FromStr<Err = DennettError>>(
    iter: &mut impl Iterator<Item = String>,
    flag_name: &str,
) -> Result<T, DennettError> {
    let mut id = None;
    while let Some(flag) = iter.next() {
        if flag == flag_name {
            id = Some(T::from_str(&next_val(iter, flag_name)?)?);
        } else {
            return Err(unknown_flag(&flag));
        }
    }
    req_opt(id, flag_name)
}

fn parse_u32(raw: &str, flag: &str) -> Result<u32, DennettError> {
    raw.parse::<u32>().map_err(|_| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: String::from("must be a non-negative 32-bit integer"),
    })
}

fn parse_u64(raw: &str, flag: &str) -> Result<u64, DennettError> {
    raw.parse::<u64>().map_err(|_| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: String::from("must be a non-negative integer"),
    })
}

fn parse_i64(raw: &str, flag: &str) -> Result<i64, DennettError> {
    raw.parse::<i64>().map_err(|_| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: String::from("must be an integer"),
    })
}

fn parse_hhmm(raw: &str) -> Result<(u8, u8), DennettError> {
    let Some((h_raw, m_raw)) = raw.trim().split_once(':') else {
        return Err(DennettError::InvalidInput {
            field: String::from("--daily"),
            reason: String::from("expected HH:MM wall-clock time (e.g. 09:30)"),
        });
    };
    let hour = h_raw
        .parse::<u8>()
        .map_err(|_| DennettError::InvalidInput {
            field: String::from("--daily"),
            reason: String::from("invalid hour in HH:MM"),
        })?;
    let minute = m_raw
        .parse::<u8>()
        .map_err(|_| DennettError::InvalidInput {
            field: String::from("--daily"),
            reason: String::from("invalid minute in HH:MM"),
        })?;
    if hour > 23 || minute > 59 {
        return Err(DennettError::InvalidInput {
            field: String::from("--daily"),
            reason: String::from("hour must be 0..=23 and minute must be 0..=59"),
        });
    }
    Ok((hour, minute))
}

fn parse_auth_profile(raw: &str) -> Result<RuntimeAuthProfile, DennettError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "oauth-personal" => Ok(RuntimeAuthProfile::OAuthPersonal),
        "gemini-api-key" => Ok(RuntimeAuthProfile::GeminiApiKey),
        other => Err(DennettError::InvalidInput {
            field: String::from("--auth-profile"),
            reason: format!(
                "unsupported auth profile `{other}` (expected oauth-personal|gemini-api-key)"
            ),
        }),
    }
}

fn parse_i32(raw: &str, flag: &str) -> Result<i32, DennettError> {
    raw.parse::<i32>().map_err(|_| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: String::from("must be a 32-bit integer"),
    })
}

fn parse_budget_exceed_policy(raw: &str) -> Result<BudgetExceedPolicy, DennettError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ask" | "ask_approval" => Ok(BudgetExceedPolicy::AskApproval),
        "fail" => Ok(BudgetExceedPolicy::Fail),
        other => Err(DennettError::InvalidInput {
            field: String::from("--budget-policy"),
            reason: format!(
                "unsupported budget exceed policy `{other}` (expected ask_approval|fail)"
            ),
        }),
    }
}

fn parse_manual_effect_resolution(raw: &str) -> Result<ManualEffectResolution, DennettError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "confirmed" | "mark_confirmed" => Ok(ManualEffectResolution::MarkConfirmed),
        "not_executed" | "mark_not_executed" => Ok(ManualEffectResolution::MarkNotExecuted),
        other => Err(DennettError::InvalidInput {
            field: String::from("--manual"),
            reason: format!(
                "unsupported manual resolution `{other}` (expected mark_confirmed|mark_not_executed)"
            ),
        }),
    }
}

fn parse_weekly_spec(raw: &str) -> Result<(u8, u8, u8), DennettError> {
    let Some((days_raw, time_raw)) = raw.trim().split_once('@') else {
        return Err(DennettError::InvalidInput {
            field: String::from("--weekly"),
            reason: String::from("expected <mon,wed,fri>@HH:MM (e.g. mon,wed,fri@09:30)"),
        });
    };
    let mut mask: u8 = 0;
    for tok in days_raw.split(',') {
        let bit = match tok.trim().to_ascii_lowercase().as_str() {
            "mon" | "monday" => 1 << 0,
            "tue" | "tuesday" => 1 << 1,
            "wed" | "wednesday" => 1 << 2,
            "thu" | "thursday" => 1 << 3,
            "fri" | "friday" => 1 << 4,
            "sat" | "saturday" => 1 << 5,
            "sun" | "sunday" => 1 << 6,
            other => {
                return Err(DennettError::InvalidInput {
                    field: String::from("--weekly"),
                    reason: format!("unrecognized weekday `{other}` (expected mon..sun)"),
                });
            }
        };
        mask |= bit;
    }
    if mask == 0 {
        return Err(DennettError::InvalidInput {
            field: String::from("--weekly"),
            reason: String::from("at least one weekday (mon..sun) is required"),
        });
    }
    let (hour, minute) = parse_hhmm(time_raw)?;
    Ok((mask, hour, minute))
}

fn build_optional_schedule(
    fire_at_ms: Option<i64>,
    interval_secs: Option<u64>,
    daily_time: Option<(u8, u8)>,
    weekly_spec: Option<(u8, u8, u8)>,
    timezone: String,
    missed_run_policy: MissedRunPolicy,
) -> Result<Option<TaskSchedule>, DennettError> {
    let count = usize::from(fire_at_ms.is_some())
        + usize::from(interval_secs.is_some())
        + usize::from(daily_time.is_some())
        + usize::from(weekly_spec.is_some());
    if count > 1 {
        return Err(DennettError::InvalidInput {
            field: String::from("schedule"),
            reason: String::from(
                "specify at most one of --fire-at-ms, --interval-secs, --daily, or --weekly",
            ),
        });
    }
    let kind = if let Some(ms) = fire_at_ms {
        ScheduleKind::OnceAt {
            fire_at: TimestampMs::from_millis(ms),
        }
    } else if let Some(secs) = interval_secs {
        ScheduleKind::Interval {
            interval_secs: secs,
        }
    } else if let Some((hour, minute)) = daily_time {
        ScheduleKind::DailyWallTime { hour, minute }
    } else if let Some((weekdays_mask, hour, minute)) = weekly_spec {
        ScheduleKind::WeeklyWallTime {
            weekdays_mask,
            hour,
            minute,
        }
    } else {
        return Ok(None);
    };
    let sched = TaskSchedule {
        kind,
        timezone,
        original_expression: None,
        missed_run_policy,
        next_fire_at: None,
    };
    sched.validate()?;
    Ok(Some(sched))
}

fn next_val(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, DennettError> {
    iter.next().ok_or_else(|| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: format!("expected value after {flag}"),
    })
}

fn req_opt<T>(val: Option<T>, flag: &str) -> Result<T, DennettError> {
    val.ok_or_else(|| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: format!("required argument {flag} is missing"),
    })
}

fn ensure_no_extra(iter: &mut impl Iterator<Item = String>) -> Result<(), DennettError> {
    if let Some(extra) = iter.next() {
        return Err(unknown_flag(&extra));
    }
    Ok(())
}

fn unknown_flag(arg: &str) -> DennettError {
    DennettError::InvalidInput {
        field: String::from("cli"),
        reason: format!("unrecognized argument `{}`", sanitize_cli_arg(arg)),
    }
}

fn print_output<T: Serialize>(value: &T, json_mode: bool) -> Result<(), DennettError> {
    let text = if json_mode {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .map_err(|err| DennettError::Internal {
        detail: format!("failed serializing output: {err}"),
    })?;
    println!("{text}");
    Ok(())
}

fn emit_error(err: &DennettError, json_mode: bool) {
    let safe_msg = redact_secrets(&err.to_string(), &[]);
    if json_mode {
        let payload = serde_json::json!({
            "code": err.code(),
            "message": safe_msg,
        });
        eprintln!("{payload}");
    } else {
        eprintln!("error [{}]: {safe_msg}", err.code());
    }
}

fn exit_code_for_error(err: &DennettError) -> ExitCode {
    match err {
        DennettError::InvalidInput { .. } => ExitCode::from(2),
        DennettError::Unauthorized { .. } | DennettError::AuthRequired { .. } => ExitCode::from(3),
        DennettError::ForbiddenScope { .. }
        | DennettError::AccessDenied { .. }
        | DennettError::SafetyGuardRejected { .. } => ExitCode::from(4),
        DennettError::NotFound { .. } => ExitCode::from(5),
        DennettError::Conflict { .. } | DennettError::IdempotencyMismatch { .. } => {
            ExitCode::from(6)
        }
        DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. } => {
            ExitCode::from(7)
        }
        _ => ExitCode::from(1),
    }
}

fn print_usage() {
    println!(
        "dennettctl {} — thin CLI facade for dennett-head\n\
         \n\
         GLOBAL FLAGS:\n\
           --url <http://...>          Head base URL (default: $DENNETT_HEAD_URL or {DEFAULT_HEAD_URL})\n\
           --json                      Emit compact machine-readable JSON\n\
           --correlation-id <id>       Attach explicit correlation ID\n\
         \n\
         COMMANDS:\n\
           health\n\
           runtime catalog [--refresh]\n\
           agent create --name <n> --system-prompt <p> [--model <m>] [--thinking <l>] [--scope <s>] [--write-policy <p>] [--skill <s>] [--max-turns <n>] [--max-tokens <n>] [--max-wall-secs <n>]\n\
           agent list\n\
           agent update-config --id <uuid> --system-prompt <p> [--model <m>] [--thinking <l>] [--scope <s>] [--write-policy <p>] [--skill <s>]\n\
           agent history --id <uuid>\n\
           agent send-message --to <uuid> --project <uuid> --body <text> [--from <uuid>] [--task <uuid>] [--idempotency-key <k>]\n\
           agent inbox --id <uuid> [--undelivered]\n\
           project create --name <n> [--workspace <dir>] [--scope <s>]\n\
           project list [--include-archived]\n\
           project update --id <uuid> [--name <n>] [--workspace <dir>] [--pin | --unpin]\n\
           project files --id <uuid> [--subpath <rel>]\n\
           project rebind --id <uuid> --workspace <dir> [--reason <r>]\n\
           project detach --id <uuid>\n\
           project archive|unarchive|remove --id <uuid>\n\
           project delete --id <uuid> --confirm\n\
           session create --agent <uuid> [--project <uuid>] [--title <t>] [--kind user_chat|task_run]\n\
           session list\n\
           session show --id <uuid>\n\
           session update --id <uuid> [--title <t>] [--pin | --unpin]\n\
           send --session <uuid> --message <text> [--client-message-id <k>] [--timeout-ms <ms>]\n\
           cancel --session <uuid>\n\
           artifact create --session <uuid> --name <n> [--mime-type <m>] [--status partial|final] (--content <c> | --path <rel>)\n\
           artifact list --session <uuid>\n\
           artifact revisions --session <uuid> (--id <uuid> | --name <n>)\n\
           artifact finalize|archive|unarchive|remove|delete --session <uuid> --id <uuid> [--confirm]\n\
           task create --project <uuid> --agent <uuid> --title <t> --goal <g> [--fire-at-ms <ms> | --interval-secs <s> | --daily HH:MM | --weekly mon,wed@HH:MM] [--tz <iana>] [--misfire execute_once|skip|ask] [--wait-for-signal <name>] [--max-turns <n>] [--max-tokens <n>] [--max-wall-secs <n>] [--budget-policy ask_approval|fail] [--parent-task <uuid>] [--skill <s>] [--auth-profile <p>] [--no-auto-run]\n\
           task list [--project <uuid>] [--status <s>]\n\
           task show --id <uuid>\n\
           task update --id <uuid> [--title <t>] [--goal <g>] [--clear-schedule | --fire-at-ms <ms> | --interval-secs <s> | --daily HH:MM | --weekly mon,wed@HH:MM] [--tz <iana>] [--misfire execute_once|skip|ask] [--max-turns <n>] [--max-tokens <n>] [--max-wall-secs <n>] [--budget-policy ask_approval|fail] [--no-auto-run]\n\
           task signal --id <uuid> --name <n> --payload <text> --idempotency-key <k>\n\
           task reply --id <uuid> --answer <text>\n\
           task cancel --id <uuid>\n\
           inbox list [--status open|resolved|all] [--project <uuid>]\n\
           inbox resolve --id <uuid> --revision <n> --action <permission|reply|increase_budget|run_now|skip_missed|reconcile_effect|stop_task> [...]\n\
           permission list [--status pending|all] [--session <uuid>]\n\
           permission resolve --id <uuid> --decision <approve_once|allow_scope|deny> [--ttl-secs <s>] [--max-uses <n>] [--revision <n>]\n\
           grant list [--project <uuid>] [--include-revoked]\n\
           grant revoke --id <uuid> [--reason <r>]\n\
           effect dispatch --project <uuid> --session <uuid> --agent <uuid> --operation <op> --recipient <r> --idempotency-key <k> [--task <uuid>] [--behavior success|denied|failed_before_accept|lost_ack]\n\
           effect list [--task <uuid>] [--session <uuid>] [--status <s>]\n\
           effect reconcile --id <uuid> [--manual mark_confirmed|mark_not_executed] [--note <text>]\n\
           events snapshot [--project <uuid>] [--include-archived]\n\
           events tail [--after <seq>] [--epoch <n>] [--project <uuid>] [--limit <n>]\n\
           policy add --scope <scope> --tool <tool> [--target <pat>] --effect <allow|deny> --reason <r>\n\
           policy list\n\
           memory commit --scope <s> --title <t> --body <b> [--session <uuid>] [--idempotency-key <k>]\n\
           memory outbox\n\
           memory flush-outbox\n\
           memory search [--scope <s>] [--query <q>] [--limit <n>]\n\
           diagnostics [--limit <n>]",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_agent_write_policies_and_commands() {
        let action = parse_cli(
            [
                "--json",
                "agent",
                "create",
                "--name",
                "Researcher",
                "--system-prompt",
                "You are a scientist.",
                "--scope",
                "project-alpha",
                "--write-policy",
                "paths:docs/,reports/",
            ]
            .map(String::from),
            DEFAULT_HEAD_URL,
        )
        .expect("parse agent create");

        match action {
            Action::AgentCreate { g, req } => {
                assert!(g.json);
                assert_eq!(req.name, "Researcher");
                assert_eq!(
                    req.workspace_write_policy,
                    WorkspaceWritePolicy::AllowPaths {
                        allowed_prefixes: vec![String::from("docs/"), String::from("reports/")]
                    }
                );
            }
            other => panic!("unexpected action: {other:?}"),
        }

        let pid = ProjectId::new();
        let sid = SessionId::new();
        assert!(matches!(
            parse_cli(
                ["runtime", "catalog", "--refresh"].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse runtime catalog"),
            Action::RuntimeCatalog { refresh: true, .. }
        ));
        assert!(matches!(
            parse_cli(
                [
                    "project",
                    "update",
                    "--id",
                    &pid.to_string(),
                    "--name",
                    "Renamed",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse project update"),
            Action::ProjectUpdate { .. }
        ));
        assert!(matches!(
            parse_cli(
                [
                    "project",
                    "files",
                    "--id",
                    &pid.to_string(),
                    "--subpath",
                    "src",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse project files"),
            Action::ProjectFiles {
                subpath: Some(ref s),
                ..
            } if s == "src"
        ));
        assert!(matches!(
            parse_cli(
                [
                    "session",
                    "update",
                    "--id",
                    &sid.to_string(),
                    "--title",
                    "New Title",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse session update"),
            Action::SessionUpdate { .. }
        ));

        assert!(parse_write_policy("paths:../escape").is_err());
    }

    #[test]
    fn cli_parses_standalone_sessions_and_pinning() {
        let pid = ProjectId::new();
        let sid = SessionId::new();
        let aid = AgentId::new();
        assert!(matches!(
            parse_cli(
                ["session", "create", "--agent", &aid.to_string()].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse standalone session create"),
            Action::SessionCreate { ref req, .. }
                if req.project_id.is_none() && req.title == "Новый чат"
        ));
        assert!(matches!(
            parse_cli(
                ["session", "update", &sid.to_string(), "--pin"].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse session pin"),
            Action::SessionUpdate { ref req, .. }
                if req.title.is_none() && req.pinned == Some(true)
        ));
        assert!(matches!(
            parse_cli(
                ["project", "update", &pid.to_string(), "--unpin"].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse project unpin"),
            Action::ProjectUpdate { ref req, .. }
                if req.name.is_none() && req.workspace_path.is_none() && req.pinned == Some(false)
        ));
        assert!(
            parse_cli(
                [
                    "session",
                    "update",
                    "--id",
                    &sid.to_string(),
                    "--pin",
                    "--unpin"
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .is_err()
        );
        assert!(
            parse_cli(
                [
                    "project",
                    "update",
                    "--id",
                    &pid.to_string(),
                    "--pin",
                    "--unpin"
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .is_err()
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "exercises all D2 CLI subcommand parsers in one test"
    )]
    fn cli_parses_d2_commands() {
        let pid = ProjectId::new();
        let agent_id = AgentId::new();
        let task_id = TaskId::new();
        let perm_req_id = PermissionRequestId::new();
        let grant_id = CapabilityGrantId::new();
        assert!(matches!(
            parse_cli(
                [
                    "task",
                    "create",
                    "--project",
                    &pid.to_string(),
                    "--agent",
                    &agent_id.to_string(),
                    "--title",
                    "Morning digest",
                    "--goal",
                    "Summarize repo status",
                    "--daily",
                    "09:30",
                    "--tz",
                    "Asia/Yekaterinburg",
                    "--misfire",
                    "ask",
                    "--max-turns",
                    "5",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse task create"),
            Action::TaskCreate { ref req, .. } if req.schedule.is_some()
        ));
        assert!(matches!(
            parse_cli(
                [
                    "task",
                    "signal",
                    "--id",
                    &task_id.to_string(),
                    "--name",
                    "ci.green",
                    "--payload",
                    "ok",
                    "--idempotency-key",
                    "sig-001",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse task signal"),
            Action::TaskSignal { .. }
        ));
        assert!(matches!(
            parse_cli(
                [
                    "permission",
                    "resolve",
                    "--id",
                    &perm_req_id.to_string(),
                    "--decision",
                    "allow_scope",
                    "--ttl-secs",
                    "300",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse permission resolve"),
            Action::PermissionResolve { .. }
        ));
        assert!(matches!(
            parse_cli(
                [
                    "grant",
                    "revoke",
                    "--id",
                    &grant_id.to_string(),
                    "--reason",
                    "done",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse grant revoke"),
            Action::GrantRevoke { .. }
        ));
        assert!(matches!(
            parse_cli(
                ["project", "delete", "--id", &pid.to_string(), "--confirm"].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse project delete"),
            Action::ProjectDelete { confirm: true, .. }
        ));
        assert!(matches!(
            parse_cli(
                [
                    "task",
                    "update",
                    "--id",
                    &task_id.to_string(),
                    "--weekly",
                    "mon,wed,fri@09:30",
                    "--tz",
                    "UTC",
                    "--max-turns",
                    "8",
                    "--max-tokens",
                    "12000",
                    "--max-wall-secs",
                    "180",
                    "--budget-policy",
                    "fail",
                ]
                .map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse task update with budget and weekly schedule"),
            Action::TaskUpdate { ref req, .. }
                if req.budget.as_ref().is_some_and(|b| b.max_turns == Some(8)
                    && b.max_tokens == Some(12000)
                    && b.max_wall_secs == Some(180)
                    && b.exceed_policy == BudgetExceedPolicy::Fail)
                    && matches!(
                        req.schedule.as_ref().map(|s| &s.kind),
                        Some(ScheduleKind::WeeklyWallTime {
                            weekdays_mask: 0b001_0101,
                            hour: 9,
                            minute: 30,
                        })
                    )
        ));
        assert!(matches!(
            parse_cli(
                ["events", "tail", "--after", "42", "--epoch", "1"].map(String::from),
                DEFAULT_HEAD_URL,
            )
            .expect("parse events tail"),
            Action::EventsTail {
                after_cursor: 42,
                epoch: Some(1),
                ..
            }
        ));
    }
}
