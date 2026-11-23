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
    AgentId, CommitMemoryNoteRequest, CorrelationId, CreateAgentRequest, CreateArtifactRequest,
    CreatePolicyRuleRequest, CreateProjectRequest, CreateSessionRequest, DennettError,
    HeadMemorySearchRequest, IdempotencyKey, MemoryScope, PolicyEffect, ProjectId,
    SendMessageRequest, SessionId, ThinkingLevel, UpdateAgentConfigRequest, WorkspaceWritePolicy,
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
    ProjectCreate {
        g: GlobalOpts,
        req: CreateProjectRequest,
    },
    ProjectList(GlobalOpts),
    SessionCreate {
        g: GlobalOpts,
        req: CreateSessionRequest,
    },
    SessionList(GlobalOpts),
    SessionShow {
        g: GlobalOpts,
        session_id: SessionId,
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
        | Action::AgentCreate { g, .. }
        | Action::AgentList(g)
        | Action::AgentUpdateConfig { g, .. }
        | Action::AgentHistory { g, .. }
        | Action::ProjectCreate { g, .. }
        | Action::ProjectList(g)
        | Action::SessionCreate { g, .. }
        | Action::SessionList(g)
        | Action::SessionShow { g, .. }
        | Action::Send { g, .. }
        | Action::Cancel { g, .. }
        | Action::ArtifactCreate { g, .. }
        | Action::ArtifactList { g, .. }
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
        Action::ProjectCreate { g, req } => {
            let client = HeadClient::new(&g.url)?;
            let resp = client
                .create_project(&req, g.correlation_id.as_ref())
                .await?;
            print_output(&resp, g.json)
        }
        Action::ProjectList(g) => {
            let client = HeadClient::new(&g.url)?;
            let resp = client.list_projects(g.correlation_id.as_ref()).await?;
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
        "agent" => parse_agent_subcommand(g, &mut pos),
        "project" => parse_project_subcommand(g, &mut pos),
        "session" => parse_session_subcommand(g, &mut pos),
        "send" => parse_send(g, &mut pos),
        "cancel" => parse_cancel(g, &mut pos),
        "artifact" => parse_artifact_subcommand(g, &mut pos),
        "policy" => parse_policy_subcommand(g, &mut pos),
        "memory" => parse_memory_subcommand(g, &mut pos),
        "diagnostics" => parse_diagnostics(g, &mut pos),
        other => Err(DennettError::InvalidInput {
            field: String::from("command"),
            reason: format!("unrecognized command `{}`", sanitize_cli_arg(other)),
        }),
    }
}

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
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateAgentRequest {
                name: req_opt(name, "--name")?,
                description,
                system_prompt: req_opt(system_prompt, "--system-prompt")?,
                model_preference,
                thinking_level,
                allowed_memory_scopes: scopes,
                workspace_write_policy: write_policy,
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
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = UpdateAgentConfigRequest {
                system_prompt: req_opt(system_prompt, "--system-prompt")?,
                model_preference,
                thinking_level,
                allowed_memory_scopes: scopes,
                workspace_write_policy: write_policy,
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
        other => Err(unknown_flag(other)),
    }
}

fn parse_project_subcommand(
    g: GlobalOpts,
    iter: &mut impl Iterator<Item = String>,
) -> Result<Action, DennettError> {
    let sub = next_val(iter, "project <subcommand>")?;
    match sub.as_str() {
        "list" => {
            ensure_no_extra(iter)?;
            Ok(Action::ProjectList(g))
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
                workspace_path: req_opt(workspace, "--workspace")?,
                memory_scope: req_opt(scope, "--scope")?,
            };
            req.validate()?;
            Ok(Action::ProjectCreate { g, req })
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
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--agent" => agent_id = Some(AgentId::from_str(&next_val(iter, "--agent")?)?),
                    "--project" => {
                        project_id = Some(ProjectId::from_str(&next_val(iter, "--project")?)?);
                    }
                    "--title" => title = Some(next_val(iter, "--title")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateSessionRequest {
                agent_id: req_opt(agent_id, "--agent")?,
                project_id: req_opt(project_id, "--project")?,
                title: req_opt(title, "--title")?,
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
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--session" => session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?),
            "--message" => message = Some(next_val(iter, "--message")?),
            "--timeout-ms" => {
                let raw = next_val(iter, "--timeout-ms")?;
                let parsed: u64 = raw.parse().map_err(|_| DennettError::InvalidInput {
                    field: String::from("--timeout-ms"),
                    reason: String::from("must be a positive integer"),
                })?;
                timeout_ms = Some(parsed);
            }
            other => return Err(unknown_flag(other)),
        }
    }
    let req = SendMessageRequest {
        content: req_opt(message, "--message")?,
        timeout_ms,
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
            let mut session_id = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    other => return Err(unknown_flag(other)),
                }
            }
            Ok(Action::ArtifactList {
                g,
                session_id: req_opt(session_id, "--session")?,
            })
        }
        "create" => {
            let mut session_id = None;
            let mut name = None;
            let mut mime_type = String::from("text/plain");
            let mut content = None;
            let mut relative_path = None;
            while let Some(flag) = iter.next() {
                match flag.as_str() {
                    "--session" => {
                        session_id = Some(SessionId::from_str(&next_val(iter, "--session")?)?);
                    }
                    "--name" => name = Some(next_val(iter, "--name")?),
                    "--mime-type" => mime_type = next_val(iter, "--mime-type")?,
                    "--content" => content = Some(next_val(iter, "--content")?),
                    "--path" => relative_path = Some(next_val(iter, "--path")?),
                    other => return Err(unknown_flag(other)),
                }
            }
            let req = CreateArtifactRequest {
                run_id: None,
                name: req_opt(name, "--name")?,
                mime_type,
                content,
                relative_path,
            };
            req.validate()?;
            Ok(Action::ArtifactCreate {
                g,
                session_id: req_opt(session_id, "--session")?,
                req,
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
           agent create --name <n> --system-prompt <p> [--model <m>] [--thinking <l>] [--scope <s>] [--write-policy <p>]\n\
           agent list\n\
           agent update-config --id <uuid> --system-prompt <p> [--model <m>] [--thinking <l>] [--scope <s>] [--write-policy <p>]\n\
           agent history --id <uuid>\n\
           project create --name <n> --workspace <dir> --scope <s>\n\
           project list\n\
           session create --agent <uuid> --project <uuid> --title <t>\n\
           session list\n\
           session show --id <uuid>\n\
           send --session <uuid> --message <text> [--timeout-ms <ms>]\n\
           cancel --session <uuid>\n\
           artifact create --session <uuid> --name <n> [--mime-type <m>] (--content <c> | --path <rel>)\n\
           artifact list --session <uuid>\n\
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

        assert!(parse_write_policy("paths:../escape").is_err());
    }
}
