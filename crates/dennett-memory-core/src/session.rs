use async_trait::async_trait;
use dennett_contracts::{CommandId, ProjectId, SessionEventId, SessionId, TurnId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSessionState {
    Idle,
    Running,
    Waiting,
    Failed,
    Archived,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnRole {
    User,
    Agent,
    System,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnState {
    Accepted,
    Streaming,
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

impl SessionTurnState {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::TimedOut | Self::Failed
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionResult {
    pub summary: String,
    pub partial: bool,
    pub artifact_handles: Vec<String>,
    pub evidence_handles: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SafeSessionError {
    pub code: String,
    pub message_key: String,
    pub details_handle: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SessionTurnOutcome {
    Result(SessionResult),
    Error(SafeSessionError),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectSession {
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub title: String,
    pub state: ProjectSessionState,
    pub revision: u64,
    pub active_turn_id: Option<TurnId>,
    pub last_activity_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SessionTurn {
    pub turn_id: TurnId,
    pub command_id: CommandId,
    pub role: SessionTurnRole,
    pub state: SessionTurnState,
    pub text: String,
    pub outcome: Option<SessionTurnOutcome>,
    pub created_at_unix_ms: u64,
    pub completed_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectSessionSnapshot {
    pub session: ProjectSession,
    pub turns: Vec<SessionTurn>,
    pub fingerprint: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventBody {
    SessionCreated {
        project_id: ProjectId,
        title: String,
    },
    TurnAccepted {
        user_turn_id: TurnId,
        agent_turn_id: TurnId,
        command_id: CommandId,
        text: String,
    },
    AgentTextAppended {
        turn_id: TurnId,
        text: String,
    },
    TurnFinished {
        turn_id: TurnId,
        state: SessionTurnState,
        outcome: Option<SessionTurnOutcome>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingSessionEvent {
    pub event_id: SessionEventId,
    pub session_id: SessionId,
    pub command_id: Option<CommandId>,
    pub body: SessionEventBody,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommittedSessionEvent {
    pub event_id: SessionEventId,
    pub session_id: SessionId,
    pub revision: u64,
    pub command_id: Option<CommandId>,
    pub body: SessionEventBody,
    pub committed_at_unix_ms: u64,
}

impl CommittedSessionEvent {
    #[must_use]
    pub fn matches_pending(&self, pending: &PendingSessionEvent) -> bool {
        self.event_id == pending.event_id
            && self.session_id == pending.session_id
            && self.command_id == pending.command_id
            && self.body == pending.body
            && self.committed_at_unix_ms == pending.committed_at_unix_ms
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SessionJournalError {
    #[error("project session was not found")]
    NotFound,
    #[error("session revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("session event violates an invariant: {0}")]
    InvalidTransition(&'static str),
    #[error("an idempotency key was reused for different content")]
    IdempotencyConflict,
    #[error("the session journal failed an integrity check: {0}")]
    IntegrityFailure(&'static str),
    #[error("unsupported session schema version {found}; supported version is {supported}")]
    UnsupportedSchemaVersion { found: u32, supported: u32 },
    #[error("the session journal migration could not be applied safely")]
    MigrationFailure,
    #[error("the session journal storage is unavailable")]
    StorageUnavailable,
}

#[async_trait]
pub trait SessionEventStore: Send + Sync {
    async fn append(
        &self,
        expected_revision: u64,
        event: PendingSessionEvent,
    ) -> Result<CommittedSessionEvent, SessionJournalError>;

    async fn load_session(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<CommittedSessionEvent>, SessionJournalError>;

    async fn event_for_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommittedSessionEvent>, SessionJournalError>;

    async fn list_session_ids(&self) -> Result<Vec<SessionId>, SessionJournalError>;
}

#[derive(Clone)]
pub struct SessionJournal {
    store: Arc<dyn SessionEventStore>,
}

impl SessionJournal {
    #[must_use]
    pub fn new(store: Arc<dyn SessionEventStore>) -> Self {
        Self { store }
    }

    pub async fn append(
        &self,
        event: PendingSessionEvent,
    ) -> Result<SessionCommit, SessionJournalError> {
        if let Some(command_id) = event.command_id
            && let Some(existing) = self.store.event_for_command(command_id).await?
        {
            if !existing.matches_pending(&event) {
                return Err(SessionJournalError::IdempotencyConflict);
            }
            return Ok(SessionCommit {
                snapshot: self.restore(existing.session_id).await?,
                event: existing,
            });
        }

        let mut history = self.store.load_session(event.session_id).await?;
        let expected_revision = history.last().map_or(0, |item| item.revision);
        let candidate = CommittedSessionEvent {
            event_id: event.event_id,
            session_id: event.session_id,
            revision: expected_revision + 1,
            command_id: event.command_id,
            body: event.body.clone(),
            committed_at_unix_ms: event.committed_at_unix_ms,
        };
        history.push(candidate);
        fold_session(&history)?;

        let committed = self.store.append(expected_revision, event).await?;
        Ok(SessionCommit {
            snapshot: self.restore(committed.session_id).await?,
            event: committed,
        })
    }

    pub async fn restore(
        &self,
        session_id: SessionId,
    ) -> Result<ProjectSessionSnapshot, SessionJournalError> {
        let history = self.store.load_session(session_id).await?;
        if history.is_empty() {
            return Err(SessionJournalError::NotFound);
        }
        fold_session(&history)
    }

    pub async fn restore_all(&self) -> Result<Vec<ProjectSessionSnapshot>, SessionJournalError> {
        let mut snapshots = Vec::new();
        for session_id in self.store.list_session_ids().await? {
            snapshots.push(self.restore(session_id).await?);
        }
        Ok(snapshots)
    }

    pub async fn event_for_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommittedSessionEvent>, SessionJournalError> {
        self.store.event_for_command(command_id).await
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionCommit {
    pub event: CommittedSessionEvent,
    pub snapshot: ProjectSessionSnapshot,
}

pub fn fold_session(
    history: &[CommittedSessionEvent],
) -> Result<ProjectSessionSnapshot, SessionJournalError> {
    let session_id = history
        .first()
        .ok_or(SessionJournalError::NotFound)?
        .session_id;
    let mut snapshot: Option<ProjectSessionSnapshot> = None;

    for (index, event) in history.iter().enumerate() {
        if event.session_id != session_id {
            return Err(SessionJournalError::IntegrityFailure(
                "mixed session identities",
            ));
        }
        let expected_revision = u64::try_from(index)
            .map_err(|_| SessionJournalError::IntegrityFailure("revision overflow"))?
            + 1;
        if event.revision != expected_revision {
            return Err(SessionJournalError::IntegrityFailure(
                "non-contiguous session revision",
            ));
        }
        apply_event(&mut snapshot, event)?;
    }

    let mut snapshot = snapshot.ok_or(SessionJournalError::NotFound)?;
    snapshot.fingerprint = snapshot_fingerprint(&snapshot)?;
    Ok(snapshot)
}

fn apply_event(
    snapshot: &mut Option<ProjectSessionSnapshot>,
    event: &CommittedSessionEvent,
) -> Result<(), SessionJournalError> {
    match &event.body {
        SessionEventBody::SessionCreated { project_id, title } => {
            if snapshot.is_some() || event.revision != 1 {
                return Err(SessionJournalError::InvalidTransition(
                    "session can only be created once at revision one",
                ));
            }
            if event.command_id.is_none() {
                return Err(SessionJournalError::InvalidTransition(
                    "session creation requires a stable command identity",
                ));
            }
            *snapshot = Some(ProjectSessionSnapshot {
                session: ProjectSession {
                    session_id: event.session_id,
                    project_id: *project_id,
                    title: title.clone(),
                    state: ProjectSessionState::Idle,
                    revision: event.revision,
                    active_turn_id: None,
                    last_activity_unix_ms: event.committed_at_unix_ms,
                },
                turns: Vec::new(),
                fingerprint: [0; 32],
            });
            return Ok(());
        }
        _ if snapshot.is_none() => {
            return Err(SessionJournalError::InvalidTransition(
                "session must exist before turn events",
            ));
        }
        _ => {}
    }

    let snapshot = snapshot
        .as_mut()
        .ok_or(SessionJournalError::InvalidTransition("session is absent"))?;
    match &event.body {
        SessionEventBody::SessionCreated { .. } => unreachable!(),
        SessionEventBody::TurnAccepted {
            user_turn_id,
            agent_turn_id,
            command_id,
            text,
        } => {
            if event.command_id != Some(*command_id) {
                return Err(SessionJournalError::InvalidTransition(
                    "turn command identity does not match its event envelope",
                ));
            }
            if snapshot.session.active_turn_id.is_some() {
                return Err(SessionJournalError::InvalidTransition(
                    "another turn is already active",
                ));
            }
            if user_turn_id == agent_turn_id
                || snapshot
                    .turns
                    .iter()
                    .any(|turn| turn.turn_id == *user_turn_id || turn.turn_id == *agent_turn_id)
            {
                return Err(SessionJournalError::InvalidTransition(
                    "turn identity was reused",
                ));
            }
            snapshot.turns.push(SessionTurn {
                turn_id: *user_turn_id,
                command_id: *command_id,
                role: SessionTurnRole::User,
                state: SessionTurnState::Completed,
                text: text.clone(),
                outcome: None,
                created_at_unix_ms: event.committed_at_unix_ms,
                completed_at_unix_ms: Some(event.committed_at_unix_ms),
            });
            snapshot.turns.push(SessionTurn {
                turn_id: *agent_turn_id,
                command_id: *command_id,
                role: SessionTurnRole::Agent,
                state: SessionTurnState::Accepted,
                text: String::new(),
                outcome: None,
                created_at_unix_ms: event.committed_at_unix_ms,
                completed_at_unix_ms: None,
            });
            snapshot.session.active_turn_id = Some(*agent_turn_id);
            snapshot.session.state = ProjectSessionState::Running;
        }
        SessionEventBody::AgentTextAppended { turn_id, text } => {
            if snapshot.session.active_turn_id != Some(*turn_id) {
                return Err(SessionJournalError::InvalidTransition(
                    "text can only append to the active agent turn",
                ));
            }
            let turn = snapshot
                .turns
                .iter_mut()
                .find(|turn| turn.turn_id == *turn_id)
                .ok_or(SessionJournalError::InvalidTransition(
                    "active turn is missing",
                ))?;
            if turn.role != SessionTurnRole::Agent
                || !matches!(
                    turn.state,
                    SessionTurnState::Accepted | SessionTurnState::Streaming
                )
            {
                return Err(SessionJournalError::InvalidTransition(
                    "only a non-terminal agent turn can stream text",
                ));
            }
            turn.text.push_str(text);
            turn.state = SessionTurnState::Streaming;
        }
        SessionEventBody::TurnFinished {
            turn_id,
            state,
            outcome,
        } => {
            if !state.is_terminal() {
                return Err(SessionJournalError::InvalidTransition(
                    "turn terminal event requires a terminal state",
                ));
            }
            if snapshot.session.active_turn_id != Some(*turn_id) {
                return Err(SessionJournalError::InvalidTransition(
                    "only the active agent turn can finish",
                ));
            }
            if (*state == SessionTurnState::Completed
                && !matches!(outcome, Some(SessionTurnOutcome::Result(_))))
                || (*state == SessionTurnState::Failed
                    && !matches!(outcome, Some(SessionTurnOutcome::Error(_))))
            {
                return Err(SessionJournalError::InvalidTransition(
                    "terminal outcome does not match terminal state",
                ));
            }
            let turn = snapshot
                .turns
                .iter_mut()
                .find(|turn| turn.turn_id == *turn_id)
                .ok_or(SessionJournalError::InvalidTransition(
                    "active turn is missing",
                ))?;
            if turn.role != SessionTurnRole::Agent || turn.state.is_terminal() {
                return Err(SessionJournalError::InvalidTransition(
                    "agent turn is already terminal",
                ));
            }
            turn.state = *state;
            turn.outcome = outcome.clone();
            turn.completed_at_unix_ms = Some(event.committed_at_unix_ms);
            snapshot.session.active_turn_id = None;
            snapshot.session.state = if *state == SessionTurnState::Failed {
                ProjectSessionState::Failed
            } else {
                ProjectSessionState::Idle
            };
        }
    }
    snapshot.session.revision = event.revision;
    snapshot.session.last_activity_unix_ms = event.committed_at_unix_ms;
    Ok(())
}

fn snapshot_fingerprint(
    snapshot: &ProjectSessionSnapshot,
) -> Result<[u8; 32], SessionJournalError> {
    let bytes = serde_json::to_vec(&(&snapshot.session, &snapshot.turns))
        .map_err(|_| SessionJournalError::IntegrityFailure("snapshot serialization failed"))?;
    Ok(Sha256::digest(bytes).into())
}

#[must_use]
pub fn event_integrity_hash(event: &CommittedSessionEvent) -> [u8; 32] {
    let bytes = serde_json::to_vec(event).expect("session event serialization is infallible");
    Sha256::digest(bytes).into()
}

#[derive(Clone, Default)]
pub struct InMemorySessionEventStore {
    events: Arc<RwLock<Vec<CommittedSessionEvent>>>,
}

#[async_trait]
impl SessionEventStore for InMemorySessionEventStore {
    async fn append(
        &self,
        expected_revision: u64,
        event: PendingSessionEvent,
    ) -> Result<CommittedSessionEvent, SessionJournalError> {
        let mut events = self.events.write().await;
        if let Some(existing) = events.iter().find(|item| item.event_id == event.event_id) {
            return if existing.matches_pending(&event) {
                Ok(existing.clone())
            } else {
                Err(SessionJournalError::IdempotencyConflict)
            };
        }
        if let Some(command_id) = event.command_id
            && let Some(existing) = events
                .iter()
                .find(|item| item.command_id == Some(command_id))
        {
            return if existing.matches_pending(&event) {
                Ok(existing.clone())
            } else {
                Err(SessionJournalError::IdempotencyConflict)
            };
        }

        let actual = events
            .iter()
            .rev()
            .find(|item| item.session_id == event.session_id)
            .map_or(0, |item| item.revision);
        if actual != expected_revision {
            return Err(SessionJournalError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        let committed = CommittedSessionEvent {
            event_id: event.event_id,
            session_id: event.session_id,
            revision: actual + 1,
            command_id: event.command_id,
            body: event.body,
            committed_at_unix_ms: event.committed_at_unix_ms,
        };
        events.push(committed.clone());
        Ok(committed)
    }

    async fn load_session(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<CommittedSessionEvent>, SessionJournalError> {
        Ok(self
            .events
            .read()
            .await
            .iter()
            .filter(|item| item.session_id == session_id)
            .cloned()
            .collect())
    }

    async fn event_for_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommittedSessionEvent>, SessionJournalError> {
        Ok(self
            .events
            .read()
            .await
            .iter()
            .find(|item| item.command_id == Some(command_id))
            .cloned())
    }

    async fn list_session_ids(&self) -> Result<Vec<SessionId>, SessionJournalError> {
        let events = self.events.read().await;
        let mut seen = HashSet::new();
        Ok(events
            .iter()
            .filter_map(|event| seen.insert(event.session_id).then_some(event.session_id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(
        session_id: SessionId,
        command_id: Option<CommandId>,
        body: SessionEventBody,
        committed_at_unix_ms: u64,
    ) -> PendingSessionEvent {
        PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id,
            command_id,
            body,
            committed_at_unix_ms,
        }
    }

    #[tokio::test]
    async fn journal_folds_streaming_and_terminal_state_deterministically() {
        let journal = SessionJournal::new(Arc::new(InMemorySessionEventStore::default()));
        let project_id = ProjectId::new();
        let session_id = SessionId::new();
        let create_command = CommandId::new();
        journal
            .append(pending(
                session_id,
                Some(create_command),
                SessionEventBody::SessionCreated {
                    project_id,
                    title: "Recovery".to_owned(),
                },
                1,
            ))
            .await
            .expect("create session");

        let turn_command = CommandId::new();
        let user_turn_id = TurnId::new();
        let agent_turn_id = TurnId::new();
        journal
            .append(pending(
                session_id,
                Some(turn_command),
                SessionEventBody::TurnAccepted {
                    user_turn_id,
                    agent_turn_id,
                    command_id: turn_command,
                    text: "Continue".to_owned(),
                },
                2,
            ))
            .await
            .expect("accept turn");
        journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::AgentTextAppended {
                    turn_id: agent_turn_id,
                    text: "Done".to_owned(),
                },
                3,
            ))
            .await
            .expect("append text");
        let result = SessionResult {
            summary: "Done".to_owned(),
            partial: false,
            artifact_handles: Vec::new(),
            evidence_handles: Vec::new(),
        };
        journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::TurnFinished {
                    turn_id: agent_turn_id,
                    state: SessionTurnState::Completed,
                    outcome: Some(SessionTurnOutcome::Result(result.clone())),
                },
                4,
            ))
            .await
            .expect("finish turn");

        let first = journal.restore(session_id).await.expect("restore session");
        let second = journal.restore(session_id).await.expect("restore again");
        assert_eq!(first, second);
        assert_eq!(first.session.revision, 4);
        assert_eq!(first.session.state, ProjectSessionState::Idle);
        assert_eq!(first.session.active_turn_id, None);
        assert_eq!(first.turns.len(), 2);
        assert_eq!(first.turns[1].text, "Done");
        assert_eq!(
            first.turns[1].outcome,
            Some(SessionTurnOutcome::Result(result))
        );
    }

    #[tokio::test]
    async fn command_idempotency_rejects_changed_content() {
        let journal = SessionJournal::new(Arc::new(InMemorySessionEventStore::default()));
        let session_id = SessionId::new();
        let command_id = CommandId::new();
        let original = pending(
            session_id,
            Some(command_id),
            SessionEventBody::SessionCreated {
                project_id: ProjectId::new(),
                title: "Original".to_owned(),
            },
            1,
        );
        journal
            .append(original.clone())
            .await
            .expect("first append");
        assert_eq!(
            journal
                .append(original)
                .await
                .expect("idempotent retry")
                .snapshot
                .session
                .revision,
            1
        );
        let changed = pending(
            session_id,
            Some(command_id),
            SessionEventBody::SessionCreated {
                project_id: ProjectId::new(),
                title: "Changed".to_owned(),
            },
            1,
        );
        assert_eq!(
            journal.append(changed).await,
            Err(SessionJournalError::IdempotencyConflict)
        );
    }

    #[tokio::test]
    async fn terminal_turn_rejects_late_text_without_mutating_history() {
        let journal = SessionJournal::new(Arc::new(InMemorySessionEventStore::default()));
        let session_id = SessionId::new();
        journal
            .append(pending(
                session_id,
                Some(CommandId::new()),
                SessionEventBody::SessionCreated {
                    project_id: ProjectId::new(),
                    title: "Cancel".to_owned(),
                },
                1,
            ))
            .await
            .expect("create");
        let command_id = CommandId::new();
        let agent_turn_id = TurnId::new();
        journal
            .append(pending(
                session_id,
                Some(command_id),
                SessionEventBody::TurnAccepted {
                    user_turn_id: TurnId::new(),
                    agent_turn_id,
                    command_id,
                    text: "Stop".to_owned(),
                },
                2,
            ))
            .await
            .expect("accept");
        journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::TurnFinished {
                    turn_id: agent_turn_id,
                    state: SessionTurnState::Cancelled,
                    outcome: None,
                },
                3,
            ))
            .await
            .expect("cancel");
        let late = journal
            .append(pending(
                session_id,
                None,
                SessionEventBody::AgentTextAppended {
                    turn_id: agent_turn_id,
                    text: "late".to_owned(),
                },
                4,
            ))
            .await;
        assert!(matches!(
            late,
            Err(SessionJournalError::InvalidTransition(_))
        ));
        assert_eq!(
            journal
                .restore(session_id)
                .await
                .expect("restore")
                .session
                .revision,
            3
        );
    }
}
