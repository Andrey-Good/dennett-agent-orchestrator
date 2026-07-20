use dennett_local_ipc::ClientError;
use dennett_local_ipc::protocol::dennett::common::v1::{ErrorEnvelope, command_terminal};
use dennett_local_ipc::protocol::dennett::control::v1::{
    BootstrapSnapshot, HealthState, ProjectState, ProjectSummary, SessionState, SessionSummary,
    SystemDelta, SystemWatchFrame, system_mutation, system_watch_frame,
};
use dennett_local_ipc::protocol::dennett::sync::v1::{ResyncReason, WatchCursor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSystemWatchRequest {
    pub correlation_id: String,
}

impl OpenSystemWatchRequest {
    pub fn validate(&self) -> Result<(), UiSafeError> {
        let valid = !self.correlation_id.is_empty()
            && self.correlation_id.len() <= 128
            && self
                .correlation_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if valid {
            Ok(())
        } else {
            Err(UiSafeError::new(
                "desktop_request_invalid",
                "desktop.request_invalid",
                false,
                true,
                "",
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseSystemWatchRequest {
    pub subscription_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSystemWatchResponse {
    pub correlation_id: String,
    pub subscription_id: String,
    pub snapshot: DesktopSystemSnapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgePhase {
    DiscoveringNode,
    StartingNode,
    Handshaking,
    Subscribing,
    Watching,
    Reconnecting,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopSystemEvent {
    Phase {
        subscription_id: String,
        phase: BridgePhase,
        attempt: u32,
    },
    Snapshot {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        snapshot: DesktopSystemSnapshot,
        fingerprint: String,
    },
    Delta {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        base_revision: String,
        new_revision: String,
        mutations: Vec<DesktopSystemMutation>,
    },
    Heartbeat {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        current_revision: String,
        observed_at_unix_ms: Option<i64>,
    },
    ResyncRequired {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        reason: String,
        current_revision: String,
        snapshot_required: bool,
    },
    Error {
        subscription_id: String,
        error: UiSafeError,
    },
}

impl DesktopSystemEvent {
    pub fn phase(subscription_id: &str, phase: BridgePhase, attempt: u32) -> Self {
        Self::Phase {
            subscription_id: subscription_id.to_owned(),
            phase,
            attempt,
        }
    }

    pub fn error(subscription_id: &str, error: UiSafeError) -> Self {
        Self::Error {
            subscription_id: subscription_id.to_owned(),
            error,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSystemSnapshot {
    pub revision: String,
    pub authority_epoch: String,
    pub observed_at_unix_ms: Option<i64>,
    pub projects: Vec<DesktopProjectSummary>,
    pub recent_sessions: Vec<DesktopSessionSummary>,
    pub active_project_id: Option<String>,
    pub active_session_id: Option<String>,
    pub node_state: String,
    pub runtime: Option<DesktopRuntimeSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeSummary {
    pub adapter_id: String,
    pub runtime_kind: String,
    pub streaming: bool,
    pub continuation: bool,
    pub scoped_cancellation: bool,
    pub deadlines: bool,
    pub native_extension_schemas: Vec<String>,
}

impl From<&BootstrapSnapshot> for DesktopSystemSnapshot {
    fn from(snapshot: &BootstrapSnapshot) -> Self {
        Self {
            revision: snapshot.revision.to_string(),
            authority_epoch: snapshot.authority_epoch.to_string(),
            observed_at_unix_ms: timestamp_ms(snapshot.observed_at.as_ref()),
            projects: snapshot.projects.iter().map(Into::into).collect(),
            recent_sessions: snapshot.recent_sessions.iter().map(Into::into).collect(),
            active_project_id: non_empty(&snapshot.active_project_id),
            active_session_id: non_empty(&snapshot.active_session_id),
            node_state: health_name(snapshot.node_state).to_owned(),
            runtime: snapshot.runtime.as_ref().map(|runtime| DesktopRuntimeSummary {
                adapter_id: runtime.adapter_id.clone(),
                runtime_kind: runtime.runtime_kind.clone(),
                streaming: runtime.streaming,
                continuation: runtime.continuation,
                scoped_cancellation: runtime.scoped_cancellation,
                deadlines: runtime.deadlines,
                native_extension_schemas: runtime.native_extension_schemas.clone(),
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectSummary {
    pub project_id: String,
    pub display_name: String,
    pub state: String,
    pub revision: String,
    pub last_activity_at_unix_ms: Option<i64>,
}

impl From<&ProjectSummary> for DesktopProjectSummary {
    fn from(project: &ProjectSummary) -> Self {
        Self {
            project_id: project.project_id.clone(),
            display_name: project.display_name.clone(),
            state: enum_name(
                ProjectState::try_from(project.state).ok(),
                "project_state_unknown",
            ),
            revision: project.revision.to_string(),
            last_activity_at_unix_ms: timestamp_ms(project.last_activity_at.as_ref()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSessionSummary {
    pub session_id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub state: String,
    pub revision: String,
    pub active_turn_id: Option<String>,
    pub last_activity_at_unix_ms: Option<i64>,
}

impl From<&SessionSummary> for DesktopSessionSummary {
    fn from(session: &SessionSummary) -> Self {
        Self {
            session_id: session.session_id.clone(),
            project_id: non_empty(&session.project_id),
            title: session.title.clone(),
            state: enum_name(
                SessionState::try_from(session.state).ok(),
                "session_state_unknown",
            ),
            revision: session.revision.to_string(),
            active_turn_id: non_empty(&session.active_turn_id),
            last_activity_at_unix_ms: timestamp_ms(session.last_activity_at.as_ref()),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWatchCursor {
    pub stream_id: String,
    pub sequence: String,
    pub authority_epoch: String,
}

impl From<&WatchCursor> for DesktopWatchCursor {
    fn from(cursor: &WatchCursor) -> Self {
        Self {
            stream_id: cursor.stream_id.clone(),
            sequence: cursor.sequence.to_string(),
            authority_epoch: cursor.authority_epoch.to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopSystemMutation {
    UpsertProject {
        project: DesktopProjectSummary,
    },
    RemoveProject {
        project_id: String,
    },
    UpsertSession {
        session: DesktopSessionSummary,
    },
    RemoveSession {
        session_id: String,
    },
    UpdateSelection {
        active_project: DesktopSelectionValue,
        active_session: DesktopSelectionValue,
    },
    UpdateHealth {
        node_state: String,
        status_code: String,
        observed_at_unix_ms: Option<i64>,
    },
    FinishCommand {
        command_id: String,
        operation_id: String,
        outcome: DesktopCommandOutcome,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSelectionValue {
    pub changed: bool,
    pub value: Option<String>,
}

impl DesktopSelectionValue {
    fn from_wire(value: &Option<String>) -> Self {
        Self {
            changed: value.is_some(),
            value: value.as_ref().and_then(|value| non_empty(value)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopCommandOutcome {
    Completed {
        completed_revision: String,
        message_key: String,
        partial: bool,
    },
    Failed {
        error: UiSafeError,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSafeError {
    pub code: Box<str>,
    pub message_key: Box<str>,
    pub correlation_id: Box<str>,
    pub retryable: bool,
    pub user_action_required: bool,
    pub details_handle: Option<Box<str>>,
    pub current_revision: Option<Box<str>>,
}

impl UiSafeError {
    pub fn new(
        code: impl Into<String>,
        message_key: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        correlation_id: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into().into_boxed_str(),
            message_key: message_key.into().into_boxed_str(),
            correlation_id: correlation_id.into().into_boxed_str(),
            retryable,
            user_action_required,
            details_handle: None,
            current_revision: None,
        }
    }

    pub fn from_client(error: &ClientError, correlation_id: &str) -> Self {
        if let ClientError::Remote(remote) = error {
            let mut mapped = Self::from(remote);
            if mapped.correlation_id.is_empty() {
                mapped.correlation_id = correlation_id.to_owned().into_boxed_str();
            }
            return mapped;
        }
        Self::new(
            error.code(),
            format!("desktop.{}", error.code()),
            error.retryable(),
            error.user_action_required(),
            correlation_id,
        )
    }
}

impl From<&ErrorEnvelope> for UiSafeError {
    fn from(error: &ErrorEnvelope) -> Self {
        Self {
            code: error.code.clone().into_boxed_str(),
            message_key: error.message_key.clone().into_boxed_str(),
            correlation_id: error.correlation_id.clone().into_boxed_str(),
            retryable: error.retryable,
            user_action_required: error.user_action_required,
            details_handle: non_empty(&error.details_handle).map(String::into_boxed_str),
            current_revision: error
                .current_revision
                .map(|revision| revision.to_string().into_boxed_str()),
        }
    }
}

pub fn frame_to_event(
    subscription_id: &str,
    frame: &SystemWatchFrame,
) -> Result<DesktopSystemEvent, UiSafeError> {
    let payload = frame
        .frame
        .as_ref()
        .ok_or_else(|| malformed("missing_payload"))?;
    if let system_watch_frame::Frame::Error(error) = payload {
        return Ok(DesktopSystemEvent::error(
            subscription_id,
            UiSafeError::from(error),
        ));
    }
    let cursor = frame
        .cursor
        .as_ref()
        .ok_or_else(|| malformed("missing_cursor"))?;
    let cursor = DesktopWatchCursor::from(cursor);
    match payload {
        system_watch_frame::Frame::Snapshot(snapshot) => {
            let bootstrap = snapshot
                .bootstrap
                .as_ref()
                .ok_or_else(|| malformed("missing_snapshot"))?;
            Ok(DesktopSystemEvent::Snapshot {
                subscription_id: subscription_id.to_owned(),
                cursor,
                snapshot: bootstrap.into(),
                fingerprint: hex(&snapshot.snapshot_fingerprint),
            })
        }
        system_watch_frame::Frame::Delta(delta) => delta_event(subscription_id, cursor, delta),
        system_watch_frame::Frame::Heartbeat(heartbeat) => Ok(DesktopSystemEvent::Heartbeat {
            subscription_id: subscription_id.to_owned(),
            cursor,
            current_revision: heartbeat.current_revision.to_string(),
            observed_at_unix_ms: timestamp_ms(heartbeat.observed_at.as_ref()),
        }),
        system_watch_frame::Frame::ResyncRequired(resync) => {
            Ok(DesktopSystemEvent::ResyncRequired {
                subscription_id: subscription_id.to_owned(),
                cursor,
                reason: enum_name(
                    ResyncReason::try_from(resync.reason).ok(),
                    "resync_reason_unknown",
                ),
                current_revision: resync.current_revision.to_string(),
                snapshot_required: resync.snapshot_required,
            })
        }
        system_watch_frame::Frame::Error(_) => unreachable!("handled before cursor validation"),
    }
}

fn delta_event(
    subscription_id: &str,
    cursor: DesktopWatchCursor,
    delta: &SystemDelta,
) -> Result<DesktopSystemEvent, UiSafeError> {
    let mutations = delta
        .mutations
        .iter()
        .map(mutation_to_desktop)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DesktopSystemEvent::Delta {
        subscription_id: subscription_id.to_owned(),
        cursor,
        base_revision: delta.base_revision.to_string(),
        new_revision: delta.new_revision.to_string(),
        mutations,
    })
}

fn mutation_to_desktop(
    mutation: &dennett_local_ipc::protocol::dennett::control::v1::SystemMutation,
) -> Result<DesktopSystemMutation, UiSafeError> {
    Ok(match mutation.mutation.as_ref() {
        Some(system_mutation::Mutation::UpsertProject(project)) => {
            DesktopSystemMutation::UpsertProject {
                project: project.into(),
            }
        }
        Some(system_mutation::Mutation::RemoveProjectId(project_id)) => {
            DesktopSystemMutation::RemoveProject {
                project_id: project_id.clone(),
            }
        }
        Some(system_mutation::Mutation::UpsertSession(session)) => {
            DesktopSystemMutation::UpsertSession {
                session: session.into(),
            }
        }
        Some(system_mutation::Mutation::RemoveSessionId(session_id)) => {
            DesktopSystemMutation::RemoveSession {
                session_id: session_id.clone(),
            }
        }
        Some(system_mutation::Mutation::UpdateSelection(selection)) => {
            DesktopSystemMutation::UpdateSelection {
                active_project: DesktopSelectionValue::from_wire(&selection.active_project_id),
                active_session: DesktopSelectionValue::from_wire(&selection.active_session_id),
            }
        }
        Some(system_mutation::Mutation::UpdateHealth(health)) => {
            DesktopSystemMutation::UpdateHealth {
                node_state: health_name(health.node_state).to_owned(),
                status_code: health.status_code.clone(),
                observed_at_unix_ms: timestamp_ms(health.observed_at.as_ref()),
            }
        }
        Some(system_mutation::Mutation::FinishCommand(command)) => {
            let outcome = match command.outcome.as_ref() {
                Some(command_terminal::Outcome::Result(result)) => {
                    DesktopCommandOutcome::Completed {
                        completed_revision: result.completed_revision.to_string(),
                        message_key: result.message_key.clone(),
                        partial: result.partial,
                    }
                }
                Some(command_terminal::Outcome::Error(error)) => DesktopCommandOutcome::Failed {
                    error: error.into(),
                },
                None => return Err(malformed("command_outcome_missing")),
            };
            DesktopSystemMutation::FinishCommand {
                command_id: command.command_id.clone(),
                operation_id: command.operation_id.clone(),
                outcome,
            }
        }
        None => return Err(malformed("mutation_missing")),
    })
}

fn malformed(code: &str) -> UiSafeError {
    UiSafeError::new(
        format!("ipc_{code}"),
        "desktop.ipc_response_malformed",
        true,
        false,
        "",
    )
}

fn timestamp_ms(timestamp: Option<&prost_types::Timestamp>) -> Option<i64> {
    let timestamp = timestamp?;
    timestamp
        .seconds
        .checked_mul(1_000)
        .and_then(|seconds| seconds.checked_add(i64::from(timestamp.nanos) / 1_000_000))
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

trait WireEnumName {
    fn wire_name(self) -> &'static str;
}

impl WireEnumName for ProjectState {
    fn wire_name(self) -> &'static str {
        self.as_str_name()
    }
}

impl WireEnumName for SessionState {
    fn wire_name(self) -> &'static str {
        self.as_str_name()
    }
}

impl WireEnumName for ResyncReason {
    fn wire_name(self) -> &'static str {
        self.as_str_name()
    }
}

fn enum_name<T: WireEnumName>(value: Option<T>, unknown: &str) -> String {
    value
        .map(|value| value.wire_name().to_ascii_lowercase())
        .unwrap_or_else(|| unknown.to_owned())
}

fn health_name(value: i32) -> &'static str {
    match HealthState::try_from(value).ok() {
        Some(HealthState::Starting) => "health_state_starting",
        Some(HealthState::Ready) => "health_state_ready",
        Some(HealthState::Degraded) => "health_state_degraded",
        Some(HealthState::RecoveryRequired) => "health_state_recovery_required",
        _ => "health_state_unknown",
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_local_ipc::protocol::dennett::control::v1::SystemSnapshot;

    #[test]
    fn snapshot_event_is_renderer_safe_and_preserves_cursor_contract() {
        let frame = SystemWatchFrame {
            cursor: Some(WatchCursor {
                stream_id: "stream-1".to_owned(),
                sequence: 1,
                authority_epoch: 4,
            }),
            frame: Some(system_watch_frame::Frame::Snapshot(SystemSnapshot {
                bootstrap: Some(BootstrapSnapshot {
                    revision: 9,
                    authority_epoch: 4,
                    active_project_id: "project-1".to_owned(),
                    node_state: HealthState::Ready as i32,
                    ..Default::default()
                }),
                snapshot_fingerprint: vec![0xab, 0xcd],
            })),
        };

        let event = frame_to_event("subscription-1", &frame).expect("event");
        let json = serde_json::to_string(&event).expect("json");
        assert!(json.contains("\"fingerprint\":\"abcd\""));
        assert!(json.contains("\"authorityEpoch\":\"4\""));
        assert!(!json.contains("session_proof"));
        assert!(!json.contains("installation_id"));
        assert!(!json.contains("pipe"));
    }

    #[test]
    fn request_validation_rejects_missing_or_unsafe_correlation_ids() {
        for correlation_id in ["", "contains a space", &"x".repeat(129)] {
            assert!(
                OpenSystemWatchRequest {
                    correlation_id: correlation_id.to_owned(),
                }
                .validate()
                .is_err()
            );
        }
        OpenSystemWatchRequest {
            correlation_id: "019f-correlation_1".to_owned(),
        }
        .validate()
        .expect("valid correlation id");
    }

    #[test]
    fn terminal_error_does_not_require_a_watch_cursor() {
        let event = frame_to_event(
            "subscription-1",
            &SystemWatchFrame {
                cursor: None,
                frame: Some(system_watch_frame::Frame::Error(ErrorEnvelope {
                    code: "watch_unavailable".to_owned(),
                    message_key: "system.watch_unavailable".to_owned(),
                    retryable: false,
                    ..Default::default()
                })),
            },
        )
        .expect("terminal error event");
        assert!(matches!(
            event,
            DesktopSystemEvent::Error {
                error: UiSafeError {
                    retryable: false,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn delta_and_resync_frames_preserve_revision_and_recovery_semantics() {
        let delta = frame_to_event(
            "subscription-1",
            &SystemWatchFrame {
                cursor: Some(WatchCursor {
                    stream_id: "stream-1".to_owned(),
                    sequence: 2,
                    authority_epoch: 4,
                }),
                frame: Some(system_watch_frame::Frame::Delta(SystemDelta {
                    base_revision: 9,
                    new_revision: 10,
                    mutations:
                        vec![dennett_local_ipc::protocol::dennett::control::v1::SystemMutation {
                        mutation: Some(system_mutation::Mutation::UpdateHealth(
                            dennett_local_ipc::protocol::dennett::control::v1::SystemHealthUpdate {
                                node_state: HealthState::Degraded as i32,
                                status_code: "node_health_changed".to_owned(),
                                observed_at: None,
                            },
                        )),
                    }],
                })),
            },
        )
        .expect("delta event");
        assert!(matches!(
            delta,
            DesktopSystemEvent::Delta {
                base_revision,
                new_revision,
                mutations,
                ..
            } if base_revision == "9"
                && new_revision == "10"
                && matches!(mutations.as_slice(), [DesktopSystemMutation::UpdateHealth { .. }])
        ));

        let resync = frame_to_event(
            "subscription-1",
            &SystemWatchFrame {
                cursor: Some(WatchCursor {
                    stream_id: "stream-1".to_owned(),
                    sequence: 3,
                    authority_epoch: 4,
                }),
                frame: Some(system_watch_frame::Frame::ResyncRequired(
                    dennett_local_ipc::protocol::dennett::sync::v1::ResyncRequired {
                        current_revision: 12,
                        reason: ResyncReason::RevisionGap as i32,
                        snapshot_required: true,
                    },
                )),
            },
        )
        .expect("resync event");
        assert!(matches!(
            resync,
            DesktopSystemEvent::ResyncRequired {
                current_revision,
                reason,
                ..
            } if current_revision == "12" && reason == "resync_reason_revision_gap"
        ));
    }
}
