//! Thin stdio JSON-RPC 2.0 MCP server (`dennett-control-mcp`) allowing an agent in chat to
//! create, list, update, and cancel project-scoped durable events/schedules via [`HeadClient`].
//!
//! Architectural boundary (`AGENTS.md` / Blueprint §5):
//! - Calls `dennett-head` strictly over HTTP using [`HeadClient`] with a short-lived project-scoped
//!   `X-Dennett-Control-Token` (`dct1.`) HMAC token.
//! - NEVER links `dennett-head`, `dennett-memory-*`, or any PostgreSQL driver.
//! - Every `dennett_event_create` creates a dedicated `SessionKind::TaskRun` session so background
//!   trigger runs never pollute the user's interactive chat sidebar or lock the live chat session.

use std::io::{self, BufRead, Write};
use std::str::FromStr;

use dennett_contracts::redact::redact_secrets;
use dennett_contracts::{
    AgentId, BudgetExceedPolicy, CorrelationId, CreateSessionRequest, CreateTaskRequest,
    DennettError, IdempotencyKey, MissedRunPolicy, ProjectId, ScheduleKind,
    SendAgentMessageRequest, SessionId, SessionKind, TaskBudget, TaskId, TaskSchedule, TaskStatus,
    TimestampMs, UpdateTaskRequest,
};
use dennett_head_client::HeadClient;
use serde_json::{Value, json};

/// Environment variable pointing to `dennett-head` loopback HTTP base URL.
pub const ENV_HEAD_URL: &str = "DENNETT_HEAD_URL";
/// Environment variable binding this MCP adapter to one exact [`ProjectId`].
pub const ENV_CONTROL_PROJECT_ID: &str = "DENNETT_CONTROL_PROJECT_ID";
/// Environment variable specifying the owning [`AgentId`].
pub const ENV_CONTROL_AGENT_ID: &str = "DENNETT_CONTROL_AGENT_ID";
/// Environment variable specifying the originating [`SessionId`].
pub const ENV_CONTROL_SESSION_ID: &str = "DENNETT_CONTROL_SESSION_ID";
/// Environment variable carrying the short-lived `dct1.` HMAC control token.
pub const ENV_CONTROL_TOKEN: &str = "DENNETT_CONTROL_TOKEN";
/// Upper bound on a single incoming stdio MCP JSON-RPC frame (2 MiB).
pub const MAX_MCP_FRAME_BYTES: usize = 2 * 1024 * 1024;
/// Supported MCP protocol version advertised on `initialize`.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Framing mode observed on stdin so responses match the client's transport framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMode {
    /// Newline-delimited JSON (`{...}\n`).
    Ndjson,
    /// LSP / MCP `Content-Length: <N>\r\n\r\n<payload>` framing.
    ContentLength,
}

/// Outcome of reading one bounded stdio JSON-RPC frame.
#[derive(Debug, PartialEq, Eq)]
pub enum BoundedFrame {
    /// End of stream.
    Eof,
    /// Frame exceeded [`MAX_MCP_FRAME_BYTES`] (remaining bytes drained safely).
    TooLarge(FrameMode),
    /// Successfully read UTF-8 JSON payload together with its framing mode.
    Message(String, FrameMode),
}

/// Reads one bounded MCP JSON-RPC message supporting BOTH `Content-Length` and NDJSON framing.
///
/// # Errors
///
/// Returns [`io::Error`] if the underlying stream fails or contains invalid UTF-8 / malformed headers.
pub fn read_mcp_frame<R: BufRead>(reader: &mut R, max_bytes: usize) -> io::Result<BoundedFrame> {
    loop {
        let first_line = match read_bounded_line_raw(reader, max_bytes)? {
            RawLine::Eof => return Ok(BoundedFrame::Eof),
            RawLine::TooLarge => return Ok(BoundedFrame::TooLarge(FrameMode::Ndjson)),
            RawLine::Line(s) => s,
        };
        let trimmed = first_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for LSP/MCP `Content-Length: <n>` header block.
        if let Some((key, val)) = trimmed.split_once(':')
            && key.trim().eq_ignore_ascii_case("content-length")
        {
            let content_len: usize = val.trim().parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Content-Length header integer",
                )
            })?;
            // Drain remaining header lines until the empty CRLF/LF separator.
            loop {
                match read_bounded_line_raw(reader, 16 * 1024)? {
                    RawLine::Eof => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "EOF before Content-Length header terminator",
                        ));
                    }
                    RawLine::TooLarge => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "oversized MCP header line",
                        ));
                    }
                    RawLine::Line(hdr) if hdr.trim().is_empty() => break,
                    RawLine::Line(_) => {}
                }
            }
            if content_len > max_bytes {
                drain_exact_bytes(reader, content_len)?;
                return Ok(BoundedFrame::TooLarge(FrameMode::ContentLength));
            }
            let mut body = vec![0_u8; content_len];
            reader.read_exact(&mut body)?;
            let text = String::from_utf8(body)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 MCP body"))?;
            return Ok(BoundedFrame::Message(text, FrameMode::ContentLength));
        }

        return Ok(BoundedFrame::Message(trimmed.to_owned(), FrameMode::Ndjson));
    }
}

/// Writes one JSON-RPC response using the matching [`FrameMode`].
///
/// # Errors
///
/// Returns [`io::Error`] on serialization or write failure.
pub fn write_mcp_frame(out: &mut impl Write, response: &Value, mode: FrameMode) -> io::Result<()> {
    let serialized = serde_json::to_string(response)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    match mode {
        FrameMode::Ndjson => {
            writeln!(out, "{serialized}")?;
        }
        FrameMode::ContentLength => {
            write!(
                out,
                "Content-Length: {}\r\n\r\n{}",
                serialized.len(),
                serialized
            )?;
        }
    }
    out.flush()
}

enum RawLine {
    Eof,
    TooLarge,
    Line(String),
}

fn read_bounded_line_raw<R: BufRead>(reader: &mut R, max_bytes: usize) -> io::Result<RawLine> {
    let mut buf = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if buf.is_empty() && !oversized {
                return Ok(RawLine::Eof);
            }
            break;
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let take = pos + 1;
            if !oversized && buf.len().saturating_add(take) <= max_bytes {
                buf.extend_from_slice(&available[..take]);
            } else {
                oversized = true;
            }
            reader.consume(take);
            break;
        }
        let len = available.len();
        if !oversized && buf.len().saturating_add(len) <= max_bytes {
            buf.extend_from_slice(available);
        } else {
            oversized = true;
        }
        reader.consume(len);
    }
    if oversized {
        return Ok(RawLine::TooLarge);
    }
    let s = String::from_utf8(buf)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8 line"))?;
    Ok(RawLine::Line(s))
}

fn drain_exact_bytes<R: BufRead>(reader: &mut R, mut remaining: usize) -> io::Result<()> {
    while remaining > 0 {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "EOF while draining oversized Content-Length frame",
            ));
        }
        let consume = available.len().min(remaining);
        reader.consume(consume);
        remaining -= consume;
    }
    Ok(())
}

/// Session/project-scoped MCP server translating event, agent, and session-skill tools into [`HeadClient`] HTTP requests.
pub struct ControlMcpServer {
    client: HeadClient,
    project_id: Option<ProjectId>,
    agent_id: AgentId,
    session_id: SessionId,
    secrets: Vec<String>,
}

impl ControlMcpServer {
    /// Creates a new [`ControlMcpServer`] bound strictly to `(project_id, agent_id, session_id)`.
    ///
    /// When `project_id` is `None` (standalone chat), only global session-skill read tools
    /// (`dennett_skill_list`, `dennett_skill_read`) are permitted; project event/delegation tools
    /// fail closed with `ACCESS_DENIED`.
    #[must_use]
    pub fn new(
        client: HeadClient,
        project_id: Option<ProjectId>,
        agent_id: AgentId,
        session_id: SessionId,
        secrets: Vec<String>,
    ) -> Self {
        Self {
            client,
            project_id,
            agent_id,
            session_id,
            secrets,
        }
    }

    /// Redacts any configured secrets and `dct1.`/`dnt1.` tokens from text before returning errors.
    #[must_use]
    pub fn redact_text(&self, input: &str) -> String {
        let refs: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        redact_secrets(input, &refs)
    }

    /// Handles one decoded JSON-RPC 2.0 request/notification message.
    ///
    /// Returns `None` for notifications (messages without an `"id"` field or `notifications/*`).
    pub async fn handle_message(&self, msg: &Value) -> Option<Value> {
        let method = msg.get("method").and_then(Value::as_str)?;
        let id = msg.get("id").cloned();

        // JSON-RPC 2.0 notifications carry no id and expect no reply.
        if id.is_none() || method.starts_with("notifications/") {
            return None;
        }
        let id = id.unwrap_or(Value::Null);

        match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "dennett-control-mcp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            })),
            "ping" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            })),
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tool_definitions_for_scope(self.project_id.is_some())
                }
            })),
            "tools/call" => {
                let params = msg.get("params").unwrap_or(&Value::Null);
                let tool_name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let args = params.get("arguments").unwrap_or(&Value::Null);
                let result = self.dispatch_tool(tool_name, args).await;
                Some(match result {
                    Ok(payload) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
                            }],
                            "isError": false
                        }
                    }),
                    Err(err) => {
                        let safe_msg = self.redact_text(&err.to_string());
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": json!({
                                        "code": err.code(),
                                        "message": safe_msg
                                    }).to_string()
                                }],
                                "isError": true
                            }
                        })
                    }
                })
            }
            _ => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("unsupported MCP method `{}`", self.redact_text(method))
                }
            })),
        }
    }

    async fn dispatch_tool(&self, tool_name: &str, args: &Value) -> Result<Value, DennettError> {
        let cid = CorrelationId::generate();
        match tool_name {
            "dennett_event_create" => self.tool_event_create(args, &cid).await,
            "dennett_event_list" => self.tool_event_list(args, &cid).await,
            "dennett_event_update" => self.tool_event_update(args, &cid).await,
            "dennett_event_cancel" => self.tool_event_cancel(args, &cid).await,
            "dennett_agent_send_message" => self.tool_agent_send_message(args, &cid).await,
            "dennett_agent_list_messages" => self.tool_agent_list_messages(args, &cid).await,
            "dennett_delegate_task" => self.tool_delegate_task(args, &cid).await,
            "dennett_skill_list" => self.tool_skill_list(&cid).await,
            "dennett_skill_read" => self.tool_skill_read(args, &cid).await,
            other => Err(DennettError::InvalidInput {
                field: String::from("name"),
                reason: format!("unknown control MCP tool `{other}`"),
            }),
        }
    }

    fn require_bound_project(&self, tool_name: &str) -> Result<ProjectId, DennettError> {
        self.project_id.ok_or_else(|| DennettError::AccessDenied {
            scope: format!("session:{}", self.session_id),
            reason: format!(
                "control MCP tool `{tool_name}` is not permitted in standalone sessions without a bound project"
            ),
        })
    }

    fn enforce_optional_project_arg(
        &self,
        args: &Value,
        tool_name: &str,
    ) -> Result<ProjectId, DennettError> {
        let project_id = self.require_bound_project(tool_name)?;
        if let Some(raw_pid) = args.get("project_id").and_then(Value::as_str) {
            let trimmed = raw_pid.trim();
            if !trimmed.is_empty() {
                let parsed = ProjectId::from_str(trimmed)?;
                if parsed != project_id {
                    return Err(DennettError::AccessDenied {
                        scope: format!("project:{project_id}"),
                        reason: format!(
                            "control MCP is strictly bound to project `{project_id}` and cannot access `{parsed}`"
                        ),
                    });
                }
            }
        }
        Ok(project_id)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "parses all trigger kinds, creates isolated task_run session, and registers durable task atomically"
    )]
    async fn tool_event_create(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_event_create")?;

        let title = required_trimmed_str(args, "title")?;
        let goal = required_trimmed_str(args, "goal")?;
        let trigger_kind = args
            .get("trigger_kind")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("immediate")
            .to_ascii_lowercase();

        let timezone = args
            .get("timezone")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("UTC")
            .to_owned();
        let misfire_policy =
            parse_misfire_policy(args.get("misfire_policy").and_then(Value::as_str))?;

        let (schedule, wait_for_signal, schedule_summary) = match trigger_kind.as_str() {
            "once_at" => {
                let fire_val = args
                    .get("fire_at_iso_or_ms")
                    .or_else(|| args.get("fire_at"))
                    .ok_or_else(|| DennettError::InvalidInput {
                        field: String::from("fire_at_iso_or_ms"),
                        reason: String::from(
                            "once_at requires `fire_at_iso_or_ms` (ISO-8601 or epoch ms)",
                        ),
                    })?;
                let fire_at = parse_iso8601_or_epoch_ms(fire_val)?;
                let sched = TaskSchedule {
                    kind: ScheduleKind::OnceAt { fire_at },
                    timezone: timezone.clone(),
                    original_expression: fire_val.as_str().map(ToOwned::to_owned),
                    missed_run_policy: misfire_policy,
                    next_fire_at: None,
                };
                sched.validate()?;
                (
                    Some(sched),
                    None,
                    format!("once_at {} ({timezone})", fire_at.as_millis()),
                )
            }
            "interval" => {
                let interval_secs = args
                    .get("interval_secs")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| DennettError::InvalidInput {
                        field: String::from("interval_secs"),
                        reason: String::from("interval requires integer `interval_secs` >= 10"),
                    })?;
                let sched = TaskSchedule {
                    kind: ScheduleKind::Interval { interval_secs },
                    timezone: timezone.clone(),
                    original_expression: Some(format!("every {interval_secs}s")),
                    missed_run_policy: misfire_policy,
                    next_fire_at: None,
                };
                sched.validate()?;
                (
                    Some(sched),
                    None,
                    format!("interval every {interval_secs}s ({timezone})"),
                )
            }
            "daily_local_time" | "daily_wall_time" => {
                let (hour, minute) = extract_hour_minute(args)?;
                let sched = TaskSchedule {
                    kind: ScheduleKind::DailyWallTime { hour, minute },
                    timezone: timezone.clone(),
                    original_expression: Some(format!("daily {hour:02}:{minute:02} {timezone}")),
                    missed_run_policy: misfire_policy,
                    next_fire_at: None,
                };
                sched.validate()?;
                (
                    Some(sched),
                    None,
                    format!("daily at {hour:02}:{minute:02} ({timezone})"),
                )
            }
            "weekly_local_time" | "weekly_wall_time" => {
                let (hour, minute) = extract_hour_minute(args)?;
                let weekdays_val =
                    args.get("weekdays")
                        .ok_or_else(|| DennettError::InvalidInput {
                            field: String::from("weekdays"),
                            reason: String::from(
                                "weekly_local_time requires non-empty `weekdays` array (mon..sun)",
                            ),
                        })?;
                let weekdays_mask = parse_weekdays_json(weekdays_val)?;
                let sched = TaskSchedule {
                    kind: ScheduleKind::WeeklyWallTime {
                        weekdays_mask,
                        hour,
                        minute,
                    },
                    timezone: timezone.clone(),
                    original_expression: Some(format!(
                        "weekly mask={weekdays_mask} {hour:02}:{minute:02} {timezone}"
                    )),
                    missed_run_policy: misfire_policy,
                    next_fire_at: None,
                };
                sched.validate()?;
                (
                    Some(sched),
                    None,
                    format!("weekly mask={weekdays_mask} at {hour:02}:{minute:02} ({timezone})"),
                )
            }
            "wait_for_signal" => {
                let sig = required_trimmed_str(args, "expected_signal_name")?;
                (None, Some(sig.clone()), format!("wait_for_signal `{sig}`"))
            }
            "immediate" => (None, None, String::from("immediate")),
            other => {
                return Err(DennettError::InvalidInput {
                    field: String::from("trigger_kind"),
                    reason: format!(
                        "unsupported trigger_kind `{other}` (expected once_at|interval|daily_local_time|weekly_local_time|wait_for_signal|immediate)"
                    ),
                });
            }
        };

        let budget = parse_optional_budget(args)?;

        // Step 1: Create dedicated clean `SessionKind::TaskRun` session so background runs never
        // pollute the interactive user chat list or block the user's active conversation!
        let task_session = self
            .client
            .create_session(
                &CreateSessionRequest {
                    agent_id: self.agent_id,
                    project_id: Some(project_id),
                    title: title.clone(),
                    session_kind: SessionKind::TaskRun,
                },
                Some(cid),
            )
            .await?;

        // Step 2: Create the durable task bound to the new `task_run` session.
        let created_task = self
            .client
            .create_task(
                &CreateTaskRequest {
                    project_id,
                    agent_id: self.agent_id,
                    session_id: Some(task_session.session_id),
                    parent_task_id: None,
                    title: Some(title),
                    goal,
                    schedule,
                    wait_for_signal,
                    budget,
                    attached_skills: Vec::new(),
                    auth_profile_override: None,
                    execute_immediately: Some(trigger_kind == "immediate"),
                },
                Some(cid),
            )
            .await?;

        let next_fire_at = created_task
            .schedule
            .as_ref()
            .and_then(|s| s.next_fire_at)
            .map(TimestampMs::as_millis);

        Ok(json!({
            "task_id": created_task.task_id,
            "session_id": created_task.session_id,
            "origin_session_id": self.session_id,
            "title": created_task.title,
            "status": created_task.status,
            "next_fire_at": next_fire_at,
            "schedule_summary": schedule_summary,
        }))
    }

    async fn tool_event_list(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_event_list")?;
        let status_filter = args
            .get("status")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let include_completed = args
            .get("include_completed")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let resp = self
            .client
            .list_tasks(Some(project_id), status_filter, Some(cid))
            .await?;
        let events: Vec<Value> = resp
            .tasks
            .into_iter()
            .filter(|t| {
                include_completed
                    || status_filter.is_some()
                    || !matches!(t.status, TaskStatus::Succeeded | TaskStatus::Cancelled)
            })
            .map(|t| {
                json!({
                    "task_id": t.task_id,
                    "session_id": t.session_id,
                    "title": t.title,
                    "goal": t.goal,
                    "status": t.status,
                    "wait_detail": t.wait_detail,
                    "generation": t.generation,
                    "next_fire_at": t.schedule.as_ref().and_then(|s| s.next_fire_at).map(TimestampMs::as_millis),
                })
            })
            .collect();

        Ok(json!({
            "project_id": project_id,
            "count": events.len(),
            "events": events,
        }))
    }

    async fn tool_event_update(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_event_update")?;
        let raw_task_id = required_trimmed_str(args, "task_id")?;
        let task_id = TaskId::from_str(&raw_task_id)?;

        // Defense-in-depth project ownership check (enforced on Head HTTP boundary too).
        let detail = self.client.get_task_detail(task_id, Some(cid)).await?;
        if detail.task.project_id != project_id {
            return Err(DennettError::AccessDenied {
                scope: format!("project:{project_id}"),
                reason: format!("task `{task_id}` does not belong to bound project"),
            });
        }

        let title = args
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let goal = args
            .get("goal")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);
        let clear_schedule = args
            .get("clear_schedule")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let budget = parse_optional_budget(args)?;

        let updated = self
            .client
            .update_task(
                task_id,
                &UpdateTaskRequest {
                    title,
                    goal,
                    schedule: None,
                    clear_schedule,
                    budget,
                    execute_immediately: Some(false),
                },
                Some(cid),
            )
            .await?;

        Ok(json!({
            "task_id": updated.task_id,
            "title": updated.title,
            "goal": updated.goal,
            "status": updated.status,
            "generation": updated.generation,
        }))
    }

    async fn tool_event_cancel(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_event_cancel")?;
        let raw_task_id = required_trimmed_str(args, "task_id")?;
        let task_id = TaskId::from_str(&raw_task_id)?;

        let detail = self.client.get_task_detail(task_id, Some(cid)).await?;
        if detail.task.project_id != project_id {
            return Err(DennettError::AccessDenied {
                scope: format!("project:{project_id}"),
                reason: format!("task `{task_id}` does not belong to bound project"),
            });
        }

        let resp = self.client.cancel_task(task_id, Some(cid)).await?;
        Ok(json!({
            "task_id": resp.task.task_id,
            "status": resp.task.status,
            "external_outcome_unknown": resp.external_outcome_unknown,
        }))
    }

    async fn tool_agent_send_message(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_agent_send_message")?;
        let recipient_raw = required_trimmed_str(args, "recipient_agent_id")?;
        let recipient_agent_id = AgentId::from_str(&recipient_raw)?;
        let body_text = args
            .get("body_text")
            .or_else(|| args.get("message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| DennettError::InvalidInput {
                field: String::from("body_text"),
                reason: String::from("missing or empty required plain-text `body_text`"),
            })?;
        let related_task_id = args
            .get("related_task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(TaskId::from_str)
            .transpose()?;
        let idempotency_key = match args
            .get("idempotency_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(k) => IdempotencyKey::try_new(k)?,
            None => IdempotencyKey::generate(),
        };

        let resp = self
            .client
            .send_agent_message(
                recipient_agent_id,
                &SendAgentMessageRequest {
                    sender_agent_id: Some(self.agent_id),
                    project_id,
                    related_task_id,
                    idempotency_key,
                    body_text,
                },
                Some(cid),
            )
            .await?;

        Ok(json!({
            "agent_message_id": resp.message.agent_message_id,
            "sender_agent_id": resp.message.sender_agent_id,
            "recipient_agent_id": resp.message.recipient_agent_id,
            "project_id": resp.message.project_id,
            "related_task_id": resp.message.related_task_id,
            "deduplicated": resp.deduplicated,
        }))
    }

    async fn tool_agent_list_messages(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_agent_list_messages")?;
        let undelivered_only = args
            .get("undelivered_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let resp = self
            .client
            .list_agent_messages(self.agent_id, undelivered_only, Some(cid))
            .await?;
        let messages: Vec<Value> = resp
            .messages
            .into_iter()
            .filter(|m| m.project_id == project_id)
            .map(|m| {
                json!({
                    "agent_message_id": m.agent_message_id,
                    "sender_agent_id": m.sender_agent_id,
                    "recipient_agent_id": m.recipient_agent_id,
                    "related_task_id": m.related_task_id,
                    "body_text": m.body_text,
                    "delivered_in_run_id": m.delivered_in_run_id,
                    "created_at": m.created_at,
                })
            })
            .collect();
        Ok(json!({
            "agent_id": self.agent_id,
            "project_id": project_id,
            "count": messages.len(),
            "messages": messages,
        }))
    }

    async fn tool_delegate_task(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let project_id = self.enforce_optional_project_arg(args, "dennett_delegate_task")?;
        let target_raw = args
            .get("recipient_agent_id")
            .or_else(|| args.get("agent_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DennettError::InvalidInput {
                field: String::from("recipient_agent_id"),
                reason: String::from("missing or empty `recipient_agent_id`"),
            })?;
        let target_agent_id = AgentId::from_str(target_raw)?;
        let goal = required_trimmed_str(args, "goal")?;
        let title = args
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(
                || goal.chars().take(80).collect::<String>(),
                ToOwned::to_owned,
            );
        let parent_task_id = args
            .get("parent_task_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(TaskId::from_str)
            .transpose()?;
        let budget = parse_optional_budget(args)?;
        let execute_immediately = args
            .get("execute_immediately")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        let task_session = self
            .client
            .create_session(
                &CreateSessionRequest {
                    agent_id: target_agent_id,
                    project_id: Some(project_id),
                    title: title.clone(),
                    session_kind: SessionKind::TaskRun,
                },
                Some(cid),
            )
            .await?;

        let created_task = self
            .client
            .create_task(
                &CreateTaskRequest {
                    project_id,
                    agent_id: target_agent_id,
                    session_id: Some(task_session.session_id),
                    parent_task_id,
                    title: Some(title),
                    goal,
                    schedule: None,
                    wait_for_signal: None,
                    budget,
                    attached_skills: Vec::new(),
                    auth_profile_override: None,
                    execute_immediately: Some(execute_immediately),
                },
                Some(cid),
            )
            .await?;

        Ok(json!({
            "task_id": created_task.task_id,
            "session_id": created_task.session_id,
            "agent_id": created_task.agent_id,
            "parent_task_id": created_task.parent_task_id,
            "title": created_task.title,
            "status": created_task.status,
        }))
    }

    async fn tool_skill_list(&self, cid: &CorrelationId) -> Result<Value, DennettError> {
        let skills = self.client.list_session_skills(Some(cid)).await?;
        Ok(json!({
            "count": skills.len(),
            "skills": skills,
        }))
    }

    async fn tool_skill_read(
        &self,
        args: &Value,
        cid: &CorrelationId,
    ) -> Result<Value, DennettError> {
        let slug = required_trimmed_str(args, "slug")?;
        let detail = self.client.read_session_skill(&slug, Some(cid)).await?;
        Ok(json!({
            "skill": detail.skill,
            "skill_md_content": detail.skill_md_content,
        }))
    }
}

/// Returns the JSON Schema definitions for all 9 control MCP tools (project + session skills).
#[must_use]
pub fn tool_definitions() -> Value {
    tool_definitions_for_scope(true)
}

/// Returns the JSON Schema definitions visible for this session scope:
/// - Project sessions (`has_project = true`): all 9 tools (events, agent messages, delegation, skills).
/// - Standalone sessions (`has_project = false`): read-only session skill tools only.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "declares static MCP tool JSON schemas in one declarative builder"
)]
pub fn tool_definitions_for_scope(has_project: bool) -> Value {
    let mut defs = Vec::new();
    if has_project {
        defs.extend([
            json!({
                "name": "dennett_event_create",
                "description": "Create a durable scheduled event, recurring check, or signal-triggered task in the current Dennett project (uses an isolated background task_run session).",
                "inputSchema": {
                    "type": "object",
                    "required": ["title", "goal", "trigger_kind"],
                    "properties": {
                        "title": { "type": "string", "description": "Short human-readable title shown in Tasks (<= 120 chars)" },
                        "goal": { "type": "string", "description": "Full instructions for the agent when the event triggers" },
                        "trigger_kind": {
                            "type": "string",
                            "enum": ["once_at", "interval", "daily_local_time", "weekly_local_time", "wait_for_signal", "immediate"]
                        },
                        "fire_at_iso_or_ms": {
                            "description": "For once_at: RFC3339/ISO-8601 timestamp (e.g. 2026-11-25T09:30:00+05:00 or ...Z) or Unix epoch milliseconds"
                        },
                        "interval_secs": { "type": "integer", "minimum": 10 },
                        "hour": { "type": "integer", "minimum": 0, "maximum": 23 },
                        "minute": { "type": "integer", "minimum": 0, "maximum": 59 },
                        "weekdays": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["mon", "tue", "wed", "thu", "fri", "sat", "sun"] }
                        },
                        "timezone": { "type": "string", "description": "IANA timezone name, e.g. Asia/Yekaterinburg or UTC" },
                        "misfire_policy": { "type": "string", "enum": ["catch_up_once", "execute_once", "skip", "ask"] },
                        "expected_signal_name": { "type": "string" },
                        "max_turns": { "type": "integer", "minimum": 1 },
                        "max_tokens": { "type": "integer", "minimum": 1 },
                        "max_wall_secs": { "type": "integer", "minimum": 1 }
                    }
                }
            }),
            json!({
                "name": "dennett_event_list",
                "description": "List durable scheduled events and tasks belonging to the current Dennett project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "status": { "type": "string" },
                        "include_completed": { "type": "boolean" }
                    }
                }
            }),
            json!({
                "name": "dennett_event_update",
                "description": "Update title, goal, or budget of an existing durable event/task in the current Dennett project.",
                "inputSchema": {
                    "type": "object",
                    "required": ["task_id"],
                    "properties": {
                        "task_id": { "type": "string" },
                        "title": { "type": "string" },
                        "goal": { "type": "string" },
                        "clear_schedule": { "type": "boolean" },
                        "max_turns": { "type": "integer", "minimum": 1 },
                        "max_tokens": { "type": "integer", "minimum": 1 },
                        "max_wall_secs": { "type": "integer", "minimum": 1 }
                    }
                }
            }),
            json!({
                "name": "dennett_event_cancel",
                "description": "Cancel/stop an existing durable event or scheduled task in the current Dennett project.",
                "inputSchema": {
                    "type": "object",
                    "required": ["task_id"],
                    "properties": {
                        "task_id": { "type": "string" }
                    }
                }
            }),
            json!({
                "name": "dennett_agent_send_message",
                "description": "Send a free-form plain-text coordination message to another agent inside the current Dennett project (A07, no rigid delegation forms).",
                "inputSchema": {
                    "type": "object",
                    "required": ["recipient_agent_id", "body_text"],
                    "properties": {
                        "recipient_agent_id": { "type": "string", "description": "Target agent UUID" },
                        "body_text": { "type": "string", "description": "Free-form plain-text message body" },
                        "related_task_id": { "type": "string", "description": "Optional related task UUID" },
                        "idempotency_key": { "type": "string", "description": "Optional idempotency key" }
                    }
                }
            }),
            json!({
                "name": "dennett_agent_list_messages",
                "description": "List plain-text coordination messages addressed to the current agent inside the current Dennett project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "undelivered_only": { "type": "boolean" }
                    }
                }
            }),
            json!({
                "name": "dennett_delegate_task",
                "description": "Delegate a free-form plain-text task to another agent within the current project; Head strictly enforces child_scope subset of parent_scope (A07).",
                "inputSchema": {
                    "type": "object",
                    "required": ["recipient_agent_id", "goal"],
                    "properties": {
                        "recipient_agent_id": { "type": "string", "description": "Helper agent UUID" },
                        "goal": { "type": "string", "description": "Free-form plain-text instructions for the helper agent" },
                        "title": { "type": "string" },
                        "parent_task_id": { "type": "string" },
                        "max_turns": { "type": "integer", "minimum": 1 },
                        "max_tokens": { "type": "integer", "minimum": 1 },
                        "max_wall_secs": { "type": "integer", "minimum": 1 },
                        "execute_immediately": { "type": "boolean" }
                    }
                }
            }),
        ]);
    }
    defs.extend([
        json!({
            "name": "dennett_skill_list",
            "description": "List Dennett skills enabled for the current session (read-only, scoped by session).",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "dennett_skill_read",
            "description": "Read the full SKILL.md instructions for one enabled Dennett skill in the current session by slug.",
            "inputSchema": {
                "type": "object",
                "required": ["slug"],
                "properties": {
                    "slug": { "type": "string", "description": "Normalized skill slug from the active skills index or dennett_skill_list" }
                }
            }
        }),
    ]);
    Value::Array(defs)
}

fn required_trimmed_str(args: &Value, field: &str) -> Result<String, DennettError> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("missing or empty required string field `{field}`"),
        })
}

fn parse_misfire_policy(raw: Option<&str>) -> Result<MissedRunPolicy, DennettError> {
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(MissedRunPolicy::ExecuteOnce);
    };
    match s.to_ascii_lowercase().as_str() {
        "catch_up_once" | "execute_once" => Ok(MissedRunPolicy::ExecuteOnce),
        "skip" => Ok(MissedRunPolicy::Skip),
        "ask" => Ok(MissedRunPolicy::Ask),
        other => Err(DennettError::InvalidInput {
            field: String::from("misfire_policy"),
            reason: format!("unsupported misfire_policy `{other}`"),
        }),
    }
}

fn extract_hour_minute(args: &Value) -> Result<(u8, u8), DennettError> {
    let hour = args
        .get("hour")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| DennettError::InvalidInput {
            field: String::from("hour"),
            reason: String::from("expected integer `hour` in 0..=23"),
        })?;
    let minute = args
        .get("minute")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| DennettError::InvalidInput {
            field: String::from("minute"),
            reason: String::from("expected integer `minute` in 0..=59"),
        })?;
    if hour > 23 || minute > 59 {
        return Err(DennettError::InvalidInput {
            field: String::from("hour"),
            reason: String::from("hour must be 0..=23 and minute must be 0..=59"),
        });
    }
    Ok((hour, minute))
}

fn parse_optional_budget(args: &Value) -> Result<Option<TaskBudget>, DennettError> {
    let max_turns = args
        .get("max_turns")
        .and_then(Value::as_u64)
        .map(|v| {
            u32::try_from(v).map_err(|_| DennettError::InvalidInput {
                field: String::from("max_turns"),
                reason: String::from("max_turns out of u32 range"),
            })
        })
        .transpose()?;
    let max_tokens = args.get("max_tokens").and_then(Value::as_u64);
    let max_wall_secs = args.get("max_wall_secs").and_then(Value::as_u64);
    if max_turns.is_none() && max_tokens.is_none() && max_wall_secs.is_none() {
        return Ok(None);
    }
    let budget = TaskBudget {
        max_turns,
        max_tokens,
        max_wall_secs,
        exceed_policy: BudgetExceedPolicy::AskApproval,
    };
    budget.validate()?;
    Ok(Some(budget))
}

/// Parses a JSON array of weekday names (`["mon", "wed", "fri"]`) into an ISO 7-bit mask (`1..=127`).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if the array is empty or contains invalid day names.
pub fn parse_weekdays_json(val: &Value) -> Result<u8, DennettError> {
    let arr = val.as_array().ok_or_else(|| DennettError::InvalidInput {
        field: String::from("weekdays"),
        reason: String::from("weekdays must be an array of strings (mon..sun)"),
    })?;
    let mut mask: u8 = 0;
    for entry in arr {
        let Some(day) = entry.as_str() else {
            return Err(DennettError::InvalidInput {
                field: String::from("weekdays"),
                reason: String::from("each weekday must be a string (mon..sun)"),
            });
        };
        let bit = match day.trim().to_ascii_lowercase().as_str() {
            "mon" | "monday" => 1 << 0,
            "tue" | "tuesday" => 1 << 1,
            "wed" | "wednesday" => 1 << 2,
            "thu" | "thursday" => 1 << 3,
            "fri" | "friday" => 1 << 4,
            "sat" | "saturday" => 1 << 5,
            "sun" | "sunday" => 1 << 6,
            other => {
                return Err(DennettError::InvalidInput {
                    field: String::from("weekdays"),
                    reason: format!("unrecognized weekday `{other}`"),
                });
            }
        };
        mask |= bit;
    }
    if mask == 0 {
        return Err(DennettError::InvalidInput {
            field: String::from("weekdays"),
            reason: String::from("weekdays array cannot be empty"),
        });
    }
    Ok(mask)
}

/// Pure-std parser converting either Unix epoch milliseconds or an RFC3339 / ISO-8601 timestamp
/// (`YYYY-MM-DDTHH:MM:SS[.fff](Z|+HH:MM|-HH:MM)`) into a validated [`TimestampMs`].
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on malformed timestamps or out-of-range calendar dates.
pub fn parse_iso8601_or_epoch_ms(val: &Value) -> Result<TimestampMs, DennettError> {
    if let Some(ms) = val.as_i64() {
        let ts = TimestampMs::from_millis(ms);
        ts.validate("fire_at_iso_or_ms")?;
        return Ok(ts);
    }
    let Some(raw) = val.as_str() else {
        return Err(DennettError::InvalidInput {
            field: String::from("fire_at_iso_or_ms"),
            reason: String::from("expected integer epoch ms or ISO-8601 string"),
        });
    };
    let s = raw.trim();
    if let Ok(ms) = s.parse::<i64>() {
        let ts = TimestampMs::from_millis(ms);
        ts.validate("fire_at_iso_or_ms")?;
        return Ok(ts);
    }
    parse_rfc3339_str(s)
}

/// Parses an RFC3339 timestamp string (`YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)`) in pure `std`.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on invalid syntax or impossible date/time.
pub fn parse_rfc3339_str(s: &str) -> Result<TimestampMs, DennettError> {
    let invalid = |reason: &str| DennettError::InvalidInput {
        field: String::from("fire_at_iso_or_ms"),
        reason: reason.to_owned(),
    };
    if s.len() < 20 {
        return Err(invalid(
            "ISO-8601 timestamp too short (expected YYYY-MM-DDTHH:MM:SSZ)",
        ));
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return Err(invalid("invalid ISO-8601 date/time separator layout"));
    }
    let year: i64 = s[0..4].parse().map_err(|_| invalid("invalid year"))?;
    let month: u32 = s[5..7].parse().map_err(|_| invalid("invalid month"))?;
    let day: u32 = s[8..10].parse().map_err(|_| invalid("invalid day"))?;
    let hour: u32 = s[11..13].parse().map_err(|_| invalid("invalid hour"))?;
    let minute: u32 = s[14..16].parse().map_err(|_| invalid("invalid minute"))?;
    let second: u32 = s[17..19].parse().map_err(|_| invalid("invalid second"))?;

    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid("date/time component out of range"));
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return Err(invalid("day of month is out of range for given year/month"));
    }

    let mut rest = &s[19..];
    let mut millis: i64 = 0;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let digit_len = after_dot.bytes().take_while(u8::is_ascii_digit).count();
        if digit_len == 0 {
            return Err(invalid("empty fractional seconds in ISO-8601 timestamp"));
        }
        let frac = &after_dot[..digit_len];
        let mut padded = [b'0'; 3];
        for (i, b) in frac.bytes().take(3).enumerate() {
            padded[i] = b;
        }
        let frac_str = std::str::from_utf8(&padded).map_err(|_| invalid("invalid fraction"))?;
        millis = frac_str.parse().map_err(|_| invalid("invalid millis"))?;
        rest = &after_dot[digit_len..];
    }

    let offset_secs: i64 = if rest.eq_ignore_ascii_case("z") {
        0
    } else if rest.len() == 6
        && matches!(rest.as_bytes()[0], b'+' | b'-')
        && rest.as_bytes()[3] == b':'
    {
        let sign: i64 = if rest.as_bytes()[0] == b'+' { 1 } else { -1 };
        let oh: i64 = rest[1..3]
            .parse()
            .map_err(|_| invalid("invalid offset hour"))?;
        let om: i64 = rest[4..6]
            .parse()
            .map_err(|_| invalid("invalid offset minute"))?;
        if oh > 23 || om > 59 {
            return Err(invalid("timezone offset out of range"));
        }
        sign * (oh * 3600 + om * 60)
    } else {
        return Err(invalid(
            "expected `Z` or `±HH:MM` timezone suffix in ISO-8601 timestamp",
        ));
    };

    let days = days_from_civil(year, month, day);
    let secs_local = days
        .checked_mul(86_400)
        .and_then(|v| {
            v.checked_add(i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second))
        })
        .ok_or_else(|| invalid("timestamp seconds overflow"))?;
    let secs_utc = secs_local
        .checked_sub(offset_secs)
        .ok_or_else(|| invalid("timestamp UTC adjustment overflow"))?;
    let epoch_ms = secs_utc
        .checked_mul(1_000)
        .and_then(|v| v.checked_add(millis))
        .ok_or_else(|| invalid("timestamp milliseconds overflow"))?;

    let ts = TimestampMs::from_millis(epoch_ms);
    ts.validate("fire_at_iso_or_ms")?;
    Ok(ts)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Howard Hinnant's civil-date to Unix epoch days algorithm (1970-01-01 -> 0).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let m = i64::from(month);
    let d = i64::from(day);
    let y = if m <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn rfc3339_parser_handles_utc_offsets_leap_years_and_rejects_invalid_dates() {
        let unix_epoch = parse_rfc3339_str("1970-01-01T00:00:00Z").expect("unix epoch");
        assert_eq!(unix_epoch.as_millis(), 0);

        let yekat = parse_rfc3339_str("2026-11-25T14:30:00+05:00").expect("+05:00 offset");
        let utc = parse_rfc3339_str("2026-11-25T09:30:00Z").expect("utc");
        assert_eq!(yekat, utc);

        let with_ms = parse_rfc3339_str("2024-02-29T23:59:59.250Z").expect("leap day");
        let base = parse_rfc3339_str("2024-02-29T23:59:59Z").expect("leap day base");
        assert_eq!(with_ms.as_millis() - base.as_millis(), 250);

        assert!(parse_rfc3339_str("2025-02-29T10:00:00Z").is_err());
        assert!(parse_rfc3339_str("2026-13-01T10:00:00Z").is_err());
        assert!(parse_rfc3339_str("2026-11-25T10:00:00").is_err());
    }

    #[test]
    fn weekdays_json_parser_builds_iso_mask_and_rejects_empty() {
        let mask = parse_weekdays_json(&json!(["mon", "wed", "sun"])).expect("mask");
        assert_eq!(mask, (1 << 0) | (1 << 2) | (1 << 6));
        assert!(parse_weekdays_json(&json!([])).is_err());
        assert!(parse_weekdays_json(&json!(["funday"])).is_err());
    }

    #[test]
    fn stdio_frame_reader_supports_both_ndjson_and_content_length_and_enforces_limit() {
        let ndjson_in = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
        let mut cursor = Cursor::new(&ndjson_in[..]);
        let frame = read_mcp_frame(&mut cursor, 1024).expect("ndjson frame");
        assert_eq!(
            frame,
            BoundedFrame::Message(
                String::from("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}"),
                FrameMode::Ndjson
            )
        );

        let body = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}";
        let lsp_in = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let mut cursor2 = Cursor::new(lsp_in.as_bytes());
        let frame2 = read_mcp_frame(&mut cursor2, 1024).expect("content-length frame");
        assert_eq!(
            frame2,
            BoundedFrame::Message(body.to_owned(), FrameMode::ContentLength)
        );

        let oversized = format!("Content-Length: 5000\r\n\r\n{}", "x".repeat(5000));
        let mut cursor3 = Cursor::new(oversized.as_bytes());
        let frame3 = read_mcp_frame(&mut cursor3, 64).expect("oversized frame");
        assert_eq!(frame3, BoundedFrame::TooLarge(FrameMode::ContentLength));
    }

    #[tokio::test]
    async fn control_mcp_lists_tools_for_project_and_standalone_and_blocks_foreign_project_id() {
        let client = HeadClient::new("http://127.0.0.1:18081").expect("client");
        let bound_pid = ProjectId::new();
        let foreign_pid = ProjectId::new();
        let server = ControlMcpServer::new(
            client.clone(),
            Some(bound_pid),
            AgentId::new(),
            SessionId::new(),
            vec![String::from("dct1.abc.def")],
        );

        let list_resp = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list"
            }))
            .await
            .expect("list response");
        let tools = list_resp["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 9);
        assert!(
            tools
                .iter()
                .any(|t| t["name"] == "dennett_agent_send_message")
        );
        assert!(
            tools
                .iter()
                .any(|t| t["name"] == "dennett_agent_list_messages")
        );
        assert!(tools.iter().any(|t| t["name"] == "dennett_delegate_task"));
        assert!(tools.iter().any(|t| t["name"] == "dennett_skill_list"));
        assert!(tools.iter().any(|t| t["name"] == "dennett_skill_read"));

        let foreign_call = server
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "dennett_event_list",
                    "arguments": {
                        "project_id": foreign_pid.to_string()
                    }
                }
            }))
            .await
            .expect("foreign call response");
        assert_eq!(foreign_call["result"]["isError"], Value::Bool(true));
        let err_text = foreign_call["result"]["content"][0]["text"]
            .as_str()
            .expect("text");
        assert!(err_text.contains("ACCESS_DENIED"));

        // Standalone server exposes only read-only session skill tools and denies project tools.
        let standalone = ControlMcpServer::new(
            client,
            None,
            AgentId::new(),
            SessionId::new(),
            vec![String::from("dct1.standalone.tok")],
        );
        let standalone_list = standalone
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/list"
            }))
            .await
            .expect("standalone list");
        let standalone_tools = standalone_list["result"]["tools"]
            .as_array()
            .expect("standalone tools");
        assert_eq!(standalone_tools.len(), 2);
        assert!(
            standalone_tools
                .iter()
                .any(|t| t["name"] == "dennett_skill_list")
        );
        assert!(
            standalone_tools
                .iter()
                .any(|t| t["name"] == "dennett_skill_read")
        );

        let standalone_event_deny = standalone
            .handle_message(&json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "dennett_event_list",
                    "arguments": {}
                }
            }))
            .await
            .expect("standalone event deny");
        assert_eq!(
            standalone_event_deny["result"]["isError"],
            Value::Bool(true)
        );
        assert!(
            standalone_event_deny["result"]["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("ACCESS_DENIED")
        );
    }
}
