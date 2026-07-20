use super::dto::{DesktopSessionSummary, DesktopSystemSnapshot, DesktopWatchCursor, UiSafeError};
use super::{
    ActiveSubscription, DesktopBridge, NodeStarter, connect, installation, installation_error,
    node_start_error,
};
use dennett_local_ipc::protocol::dennett::control::v1::{
    ComposerDraft, ComposerDraftWriteState, SessionMetadataUpdate, SessionMutation,
    SessionSnapshot, SessionState, SessionWatchFrame, TurnActivitySnapshot, TurnActivityStatus,
    TurnRole, TurnSnapshot, TurnState, session_mutation, session_watch_frame, turn_snapshot,
    turn_terminal,
};
use dennett_local_ipc::protocol::dennett::sync::v1::ResyncReason;
use dennett_local_ipc::{AuthenticatedSessionWatch, ClientCommand};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::ipc::Channel;
use tokio::sync::watch;

const INITIAL_SESSION_SNAPSHOT_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectChatRequest {
    pub correlation_id: String,
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectChatResponse {
    pub correlation_id: String,
    pub subscription_id: String,
    pub system: DesktopSystemSnapshot,
    pub session: DesktopProjectChatSnapshot,
    pub draft: Option<DesktopComposerDraft>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseProjectChatRequest {
    pub subscription_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChatRequest {
    pub correlation_id: String,
    pub command_id: String,
    pub project_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChatResponse {
    pub command_id: String,
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendProjectTurnRequest {
    pub correlation_id: String,
    pub command_id: String,
    pub project_id: String,
    pub session_id: String,
    pub expected_revision: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendProjectTurnResponse {
    pub command_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelProjectTurnRequest {
    pub correlation_id: String,
    pub command_id: String,
    pub project_id: String,
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopComposerDraft {
    pub command_id: String,
    pub text: String,
    pub revision: String,
    pub updated_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveComposerDraftRequest {
    pub correlation_id: String,
    pub operation_id: String,
    pub project_id: String,
    pub session_id: String,
    pub command_id: String,
    pub text: String,
    pub revision: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveComposerDraftResponse {
    pub command_id: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardComposerDraftRequest {
    pub correlation_id: String,
    pub operation_id: String,
    pub project_id: String,
    pub session_id: String,
    pub command_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopProjectChatEvent {
    Snapshot {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        snapshot: DesktopProjectChatSnapshot,
    },
    Delta {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        base_revision: String,
        new_revision: String,
        committed_at_unix_ms: Option<i64>,
        mutations: Vec<DesktopSessionMutation>,
    },
    Heartbeat {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        current_revision: String,
    },
    ResyncRequired {
        subscription_id: String,
        cursor: DesktopWatchCursor,
        reason: String,
        current_revision: String,
    },
    Error {
        subscription_id: String,
        error: UiSafeError,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProjectChatSnapshot {
    pub session: DesktopSessionSummary,
    pub fingerprint: String,
    pub turns: Vec<DesktopTurn>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurn {
    pub turn_id: String,
    pub command_id: String,
    pub role: String,
    pub state: String,
    pub text: String,
    pub activities: Vec<DesktopTurnActivity>,
    pub outcome: Option<DesktopTurnOutcome>,
    pub created_at_unix_ms: Option<i64>,
    pub completed_at_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTurnActivity {
    pub activity_id: String,
    pub phase: String,
    pub message: Option<String>,
    pub status: String,
    pub updated_at_unix_ms: Option<i64>,
    pub native_extensions: Vec<DesktopNativeExtension>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopNativeExtension {
    pub namespace: String,
    pub schema_version: String,
    pub json_value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopTurnOutcome {
    Result { summary: String, partial: bool },
    Error { error: UiSafeError },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    tag = "kind",
    rename_all_fields = "camelCase"
)]
pub enum DesktopSessionMutation {
    UpsertTurn {
        turn: DesktopTurn,
    },
    AppendTurnText {
        turn_id: String,
        text: String,
    },
    UpsertTurnActivity {
        turn_id: String,
        activity: DesktopTurnActivity,
    },
    FinishTurn {
        turn_id: String,
        state: String,
        outcome: Option<DesktopTurnOutcome>,
        completed_at_unix_ms: Option<i64>,
    },
    UpdateSession {
        title: Option<String>,
        state: Option<String>,
        active_turn_id: Option<String>,
    },
}

impl DesktopBridge {
    pub async fn open_project_chat(
        &self,
        window_label: String,
        request: OpenProjectChatRequest,
        channel: Channel<DesktopProjectChatEvent>,
    ) -> Result<OpenProjectChatResponse, UiSafeError> {
        validate_identity(&request.correlation_id, "", false)?;
        if let Some(session_id) = request.session_id.as_deref() {
            validate_identity(session_id, &request.correlation_id, true)?;
        }
        let data_dir = self.inner.data_dir.clone().ok_or_else(|| {
            UiSafeError::new(
                "desktop_data_directory_unavailable",
                "desktop.data_directory_unavailable",
                false,
                true,
                &request.correlation_id,
            )
        })?;
        let metadata = installation::load_or_create(data_dir.clone())
            .await
            .map_err(|error| installation_error(error, &request.correlation_id))?;
        let mut client = connect_or_start(
            &metadata,
            &data_dir,
            &self.inner.node_starter,
            &self.inner.node_start_gate,
            &request.correlation_id,
        )
        .await?;
        let system = DesktopSystemSnapshot::from(client.bootstrap());
        let session_id = request
            .session_id
            .or_else(|| system.active_session_id.clone())
            .ok_or_else(|| {
                UiSafeError::new(
                    "session_not_found",
                    "desktop.session_not_found",
                    false,
                    true,
                    &request.correlation_id,
                )
            })?;
        let mut stream = client
            .watch_session(session_id, None)
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?;
        let subscription_id = uuid::Uuid::now_v7().to_string();
        let session = take_initial(&mut stream, &subscription_id, &request.correlation_id).await?;
        let project_id = session
            .session
            .project_id
            .clone()
            .ok_or_else(|| malformed("session_project_missing", &request.correlation_id))?;
        let draft = client
            .get_composer_draft(project_id, session.session.session_id.clone())
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?
            .as_ref()
            .map(draft_to_desktop);
        let (cancel, mut cancel_rx) = watch::channel(false);
        let key = format!("conversation:{window_label}");
        self.replace_subscription(
            key.clone(),
            ActiveSubscription {
                subscription_id: subscription_id.clone(),
                cancel,
            },
        );
        let bridge = self.clone();
        let task_subscription_id = subscription_id.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let next = tokio::select! {
                    changed = cancel_rx.changed() => {
                        if changed.is_err() || *cancel_rx.borrow() { break; }
                        continue;
                    }
                    next = stream.message() => next,
                };
                let event = match next {
                    Ok(Some(response)) => response
                        .frame
                        .as_ref()
                        .map(|frame| frame_to_event(&task_subscription_id, frame))
                        .unwrap_or_else(|| Err(malformed("missing_frame", ""))),
                    Ok(None) => Err(UiSafeError::new(
                        "ipc_watch_closed",
                        "desktop.ipc_watch_closed",
                        true,
                        false,
                        "",
                    )),
                    Err(error) => Err(UiSafeError::from_client(&error, "")),
                };
                let terminal = matches!(
                    event,
                    Ok(DesktopProjectChatEvent::ResyncRequired { .. }) | Err(_)
                );
                let event = event.unwrap_or_else(|error| DesktopProjectChatEvent::Error {
                    subscription_id: task_subscription_id.clone(),
                    error,
                });
                if channel.send(event).is_err() || terminal {
                    break;
                }
            }
            bridge.remove_subscription(&key, &task_subscription_id);
        });
        Ok(OpenProjectChatResponse {
            correlation_id: request.correlation_id,
            subscription_id,
            system,
            session,
            draft,
        })
    }

    pub fn close_project_chat(
        &self,
        window_label: &str,
        request: &CloseProjectChatRequest,
    ) -> bool {
        self.close_system_watch(
            &format!("conversation:{window_label}"),
            &super::dto::CloseSystemWatchRequest {
                subscription_id: request.subscription_id.clone(),
            },
        )
    }

    pub async fn create_chat(
        &self,
        request: CreateChatRequest,
    ) -> Result<CreateChatResponse, UiSafeError> {
        validate_mutation(&request.correlation_id, &request.command_id)?;
        let mut client = self.command_client(&request.correlation_id).await?;
        let accepted = client
            .create_session(
                command(&request.command_id, &request.correlation_id, None)?,
                request.project_id,
                request.title,
            )
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?;
        Ok(CreateChatResponse {
            command_id: request.command_id,
            session_id: accepted.session_id,
        })
    }

    #[tracing::instrument(
        name = "desktop_send_project_turn",
        skip_all,
        fields(
            dennett.component = "dennett-desktop-shell",
            dennett.protocol.version = 1_u64,
            dennett.project.id = %request.project_id,
            dennett.session.id = %request.session_id,
            dennett.command.id = %request.command_id,
            correlation_id = %request.correlation_id,
        )
    )]
    pub async fn send_project_turn(
        &self,
        request: SendProjectTurnRequest,
    ) -> Result<SendProjectTurnResponse, UiSafeError> {
        validate_mutation(&request.correlation_id, &request.command_id)?;
        if request.text.trim().is_empty() {
            return Err(malformed("message_empty", &request.correlation_id));
        }
        let expected = request
            .expected_revision
            .as_deref()
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|_| malformed("revision_invalid", &request.correlation_id))?;
        let mut client = self.command_client(&request.correlation_id).await?;
        let accepted = client
            .send_turn(
                command(&request.command_id, &request.correlation_id, expected)?,
                request.project_id,
                request.session_id,
                request.text,
                Vec::new(),
            )
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?;
        Ok(SendProjectTurnResponse {
            command_id: request.command_id,
            turn_id: accepted.turn_id,
        })
    }

    pub async fn cancel_project_turn(
        &self,
        request: CancelProjectTurnRequest,
    ) -> Result<(), UiSafeError> {
        validate_mutation(&request.correlation_id, &request.command_id)?;
        let mut client = self.command_client(&request.correlation_id).await?;
        client
            .cancel_turn(
                command(&request.command_id, &request.correlation_id, None)?,
                request.project_id,
                request.session_id,
                request.turn_id,
            )
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))
    }

    pub async fn save_composer_draft(
        &self,
        request: SaveComposerDraftRequest,
    ) -> Result<SaveComposerDraftResponse, UiSafeError> {
        validate_mutation(&request.correlation_id, &request.operation_id)?;
        validate_identity(&request.command_id, &request.correlation_id, true)?;
        let mut client = self.command_client(&request.correlation_id).await?;
        let receipt = client
            .save_composer_draft(
                command(&request.operation_id, &request.correlation_id, None)?,
                ComposerDraft {
                    project_id: request.project_id,
                    session_id: request.session_id,
                    command_id: request.command_id,
                    text: request.text,
                    updated_at: Some(timestamp(request.updated_at_unix_ms)),
                    revision: request.revision,
                },
            )
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?;
        Ok(SaveComposerDraftResponse {
            command_id: receipt.command_id,
            state: ComposerDraftWriteState::try_from(receipt.state)
                .map_or("composer_draft_write_state_unknown", |state| {
                    state.as_str_name()
                })
                .to_ascii_lowercase(),
        })
    }

    pub async fn discard_composer_draft(
        &self,
        request: DiscardComposerDraftRequest,
    ) -> Result<bool, UiSafeError> {
        validate_mutation(&request.correlation_id, &request.operation_id)?;
        validate_identity(&request.command_id, &request.correlation_id, true)?;
        let mut client = self.command_client(&request.correlation_id).await?;
        let discarded = client
            .discard_composer_draft(
                command(&request.operation_id, &request.correlation_id, None)?,
                request.project_id,
                request.session_id,
                request.command_id,
            )
            .await
            .map_err(|error| UiSafeError::from_client(&error, &request.correlation_id))?;
        Ok(discarded.existed)
    }

    async fn command_client(
        &self,
        correlation_id: &str,
    ) -> Result<dennett_local_ipc::AuthenticatedSystemClient, UiSafeError> {
        let data_dir = self.inner.data_dir.clone().ok_or_else(|| {
            UiSafeError::new(
                "desktop_data_directory_unavailable",
                "desktop.data_directory_unavailable",
                false,
                true,
                correlation_id,
            )
        })?;
        let metadata = installation::load_or_create(data_dir.clone())
            .await
            .map_err(|error| installation_error(error, correlation_id))?;
        connect_or_start(
            &metadata,
            &data_dir,
            &self.inner.node_starter,
            &self.inner.node_start_gate,
            correlation_id,
        )
        .await
    }
}

async fn connect_or_start(
    metadata: &super::installation::InstallationMetadata,
    data_dir: &Path,
    starter: &Arc<dyn NodeStarter>,
    start_gate: &Arc<tokio::sync::Mutex<()>>,
    correlation_id: &str,
) -> Result<dennett_local_ipc::AuthenticatedSystemClient, UiSafeError> {
    match connect(metadata).await {
        Ok(client) => Ok(client),
        Err(error) if error.node_start_candidate() => {
            let _start_guard = start_gate.lock().await;
            if let Ok(client) = connect(metadata).await {
                return Ok(client);
            }
            starter
                .start(metadata.clone(), data_dir.to_path_buf())
                .await
                .map_err(|error| node_start_error(error, correlation_id))?;
            super::wait_for_node(metadata)
                .await
                .map_err(|error| UiSafeError::from_client(&error, correlation_id))
        }
        Err(error) => Err(UiSafeError::from_client(&error, correlation_id)),
    }
}

async fn take_initial(
    stream: &mut AuthenticatedSessionWatch,
    subscription_id: &str,
    correlation_id: &str,
) -> Result<DesktopProjectChatSnapshot, UiSafeError> {
    let response = tokio::time::timeout(INITIAL_SESSION_SNAPSHOT_DEADLINE, stream.message())
        .await
        .map_err(|_| {
            UiSafeError::new(
                "ipc_watch_snapshot_deadline_exceeded",
                "desktop.ipc_watch_snapshot_deadline_exceeded",
                true,
                false,
                correlation_id,
            )
        })?
        .map_err(|error| UiSafeError::from_client(&error, correlation_id))?
        .ok_or_else(|| {
            UiSafeError::new(
                "ipc_watch_closed",
                "desktop.ipc_watch_closed",
                true,
                false,
                correlation_id,
            )
        })?;
    match response
        .frame
        .as_ref()
        .map(|frame| frame_to_event(subscription_id, frame))
        .transpose()?
    {
        Some(DesktopProjectChatEvent::Snapshot { snapshot, .. }) => Ok(snapshot),
        Some(DesktopProjectChatEvent::Error { error, .. }) => Err(error),
        _ => Err(malformed("first_frame_not_snapshot", correlation_id)),
    }
}

fn frame_to_event(
    subscription_id: &str,
    frame: &SessionWatchFrame,
) -> Result<DesktopProjectChatEvent, UiSafeError> {
    let payload = frame
        .frame
        .as_ref()
        .ok_or_else(|| malformed("missing_payload", ""))?;
    if let session_watch_frame::Frame::Error(error) = payload {
        return Ok(DesktopProjectChatEvent::Error {
            subscription_id: subscription_id.to_owned(),
            error: UiSafeError::from(error),
        });
    }
    let cursor = frame
        .cursor
        .as_ref()
        .ok_or_else(|| malformed("missing_cursor", ""))?;
    let cursor = DesktopWatchCursor {
        stream_id: cursor.stream_id.clone(),
        sequence: cursor.sequence.to_string(),
        authority_epoch: cursor.authority_epoch.to_string(),
    };
    Ok(match payload {
        session_watch_frame::Frame::Snapshot(snapshot) => DesktopProjectChatEvent::Snapshot {
            subscription_id: subscription_id.to_owned(),
            cursor,
            snapshot: snapshot_to_desktop(snapshot)?,
        },
        session_watch_frame::Frame::Delta(delta) => DesktopProjectChatEvent::Delta {
            subscription_id: subscription_id.to_owned(),
            cursor,
            base_revision: delta.base_revision.to_string(),
            new_revision: delta.new_revision.to_string(),
            committed_at_unix_ms: timestamp_ms(delta.committed_at.as_ref()),
            mutations: delta
                .mutations
                .iter()
                .map(|mutation| {
                    mutation_to_desktop(
                        mutation,
                        timestamp_ms(delta.committed_at.as_ref()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        session_watch_frame::Frame::Heartbeat(heartbeat) => DesktopProjectChatEvent::Heartbeat {
            subscription_id: subscription_id.to_owned(),
            cursor,
            current_revision: heartbeat.current_revision.to_string(),
        },
        session_watch_frame::Frame::ResyncRequired(resync) => {
            DesktopProjectChatEvent::ResyncRequired {
                subscription_id: subscription_id.to_owned(),
                cursor,
                reason: ResyncReason::try_from(resync.reason)
                    .map_or("resync_reason_unknown", |value| value.as_str_name())
                    .to_ascii_lowercase(),
                current_revision: resync.current_revision.to_string(),
            }
        }
        session_watch_frame::Frame::Error(_) => unreachable!("handled above"),
    })
}

fn snapshot_to_desktop(
    snapshot: &SessionSnapshot,
) -> Result<DesktopProjectChatSnapshot, UiSafeError> {
    let session = snapshot
        .session
        .as_ref()
        .ok_or_else(|| malformed("missing_session", ""))?;
    Ok(DesktopProjectChatSnapshot {
        session: DesktopSessionSummary {
            session_id: session.session_id.clone(),
            project_id: non_empty(&session.project_id),
            title: session.title.clone(),
            state: session_state(session.state),
            revision: session.revision.to_string(),
            active_turn_id: non_empty(&session.active_turn_id),
            last_activity_at_unix_ms: timestamp_ms(session.last_activity_at.as_ref()),
        },
        fingerprint: hex(&snapshot.snapshot_fingerprint),
        turns: snapshot.turns.iter().map(turn_to_desktop).collect(),
    })
}

fn turn_to_desktop(turn: &TurnSnapshot) -> DesktopTurn {
    DesktopTurn {
        turn_id: turn.turn_id.clone(),
        command_id: turn.command_id.clone(),
        role: TurnRole::try_from(turn.role)
            .map_or("turn_role_unknown", |value| value.as_str_name())
            .to_ascii_lowercase(),
        state: turn_state(turn.state),
        text: turn.text.clone(),
        activities: turn.activities.iter().map(activity_to_desktop).collect(),
        outcome: turn.outcome.as_ref().map(snapshot_outcome),
        created_at_unix_ms: timestamp_ms(turn.created_at.as_ref()),
        completed_at_unix_ms: timestamp_ms(turn.completed_at.as_ref()),
    }
}

fn activity_to_desktop(activity: &TurnActivitySnapshot) -> DesktopTurnActivity {
    DesktopTurnActivity {
        activity_id: activity.activity_id.clone(),
        phase: activity.phase.clone(),
        message: activity.message.clone(),
        status: TurnActivityStatus::try_from(activity.status)
            .map_or("turn_activity_status_unknown", |value| value.as_str_name())
            .to_ascii_lowercase(),
        updated_at_unix_ms: timestamp_ms(activity.updated_at.as_ref()),
        native_extensions: activity
            .native_extensions
            .iter()
            .map(|extension| DesktopNativeExtension {
                namespace: extension.namespace.clone(),
                schema_version: extension.schema_version.clone(),
                json_value: extension.json_value.clone(),
            })
            .collect(),
    }
}

fn draft_to_desktop(draft: &ComposerDraft) -> DesktopComposerDraft {
    DesktopComposerDraft {
        command_id: draft.command_id.clone(),
        text: draft.text.clone(),
        revision: draft.revision.to_string(),
        updated_at_unix_ms: timestamp_ms(draft.updated_at.as_ref()),
    }
}

fn mutation_to_desktop(
    mutation: &SessionMutation,
    committed_at_unix_ms: Option<i64>,
) -> Result<DesktopSessionMutation, UiSafeError> {
    Ok(
        match mutation
            .mutation
            .as_ref()
            .ok_or_else(|| malformed("missing_mutation", ""))?
        {
            session_mutation::Mutation::UpsertTurn(turn) => DesktopSessionMutation::UpsertTurn {
                turn: turn_to_desktop(turn),
            },
            session_mutation::Mutation::AppendTurnText(append) => {
                DesktopSessionMutation::AppendTurnText {
                    turn_id: append.turn_id.clone(),
                    text: append.text.clone(),
                }
            }
            session_mutation::Mutation::UpsertTurnActivity(update) => {
                DesktopSessionMutation::UpsertTurnActivity {
                    turn_id: update.turn_id.clone(),
                    activity: activity_to_desktop(
                        update
                            .activity
                            .as_ref()
                            .ok_or_else(|| malformed("missing_activity", ""))?,
                    ),
                }
            }
            session_mutation::Mutation::FinishTurn(terminal) => {
                DesktopSessionMutation::FinishTurn {
                    turn_id: terminal.turn_id.clone(),
                    state: turn_state(terminal.state),
                    outcome: terminal.outcome.as_ref().map(terminal_outcome),
                    completed_at_unix_ms: committed_at_unix_ms,
                }
            }
            session_mutation::Mutation::UpdateSession(update) => update_to_desktop(update),
        },
    )
}

fn update_to_desktop(update: &SessionMetadataUpdate) -> DesktopSessionMutation {
    DesktopSessionMutation::UpdateSession {
        title: update.title.clone(),
        state: update.state.map(session_state),
        active_turn_id: update.active_turn_id.as_deref().and_then(non_empty),
    }
}

fn snapshot_outcome(outcome: &turn_snapshot::Outcome) -> DesktopTurnOutcome {
    match outcome {
        turn_snapshot::Outcome::Result(result) => DesktopTurnOutcome::Result {
            summary: result.summary.clone(),
            partial: result.partial,
        },
        turn_snapshot::Outcome::Error(error) => DesktopTurnOutcome::Error {
            error: UiSafeError::from(error),
        },
    }
}

fn terminal_outcome(outcome: &turn_terminal::Outcome) -> DesktopTurnOutcome {
    match outcome {
        turn_terminal::Outcome::Result(result) => DesktopTurnOutcome::Result {
            summary: result.summary.clone(),
            partial: result.partial,
        },
        turn_terminal::Outcome::Error(error) => DesktopTurnOutcome::Error {
            error: UiSafeError::from(error),
        },
    }
}

fn command(
    command_id: &str,
    correlation_id: &str,
    expected_revision: Option<u64>,
) -> Result<ClientCommand, UiSafeError> {
    validate_identity(command_id, correlation_id, true)?;
    Ok(ClientCommand {
        command_id: command_id.to_owned(),
        correlation_id: correlation_id.to_owned(),
        created_at_unix_ms: unix_time_ms(),
        expected_revision,
    })
}

fn validate_mutation(correlation_id: &str, command_id: &str) -> Result<(), UiSafeError> {
    validate_identity(correlation_id, "", false)?;
    validate_identity(command_id, correlation_id, true)
}

fn validate_identity(value: &str, correlation_id: &str, uuid: bool) -> Result<(), UiSafeError> {
    let valid = if uuid {
        uuid::Uuid::parse_str(value).is_ok()
    } else {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    };
    valid
        .then_some(())
        .ok_or_else(|| malformed("request_invalid", correlation_id))
}

fn malformed(code: &str, correlation_id: &str) -> UiSafeError {
    UiSafeError::new(
        format!("desktop_{code}"),
        format!("desktop.{code}"),
        false,
        true,
        correlation_id,
    )
}

fn session_state(value: i32) -> String {
    SessionState::try_from(value)
        .map_or("session_state_unknown", |state| state.as_str_name())
        .to_ascii_lowercase()
}

fn turn_state(value: i32) -> String {
    TurnState::try_from(value)
        .map_or("turn_state_unknown", |state| state.as_str_name())
        .to_ascii_lowercase()
}

fn timestamp_ms(timestamp: Option<&prost_types::Timestamp>) -> Option<i64> {
    timestamp.and_then(|value| {
        value
            .seconds
            .checked_mul(1_000)?
            .checked_add(i64::from(value.nanos) / 1_000_000)
    })
}

fn timestamp(unix_ms: u64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: (unix_ms / 1_000).try_into().unwrap_or(i64::MAX),
        nanos: ((unix_ms % 1_000) * 1_000_000) as i32,
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        },
    )
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
