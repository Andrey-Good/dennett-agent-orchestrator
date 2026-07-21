use dennett_contracts::{CommandId, ProjectId, SessionEventId, SessionId, TurnId};
use dennett_memory_core::session::{
    CommittedSessionEvent, PendingSessionEvent, ProjectSessionSnapshot, SafeSessionError,
    SessionActivityStatus, SessionCommit, SessionEventBody, SessionJournal, SessionJournalError,
    SessionNativeExtension, SessionTurnOutcome, SessionTurnState,
};
use dennett_sync_core::watch::{ResyncReason, WatchCursor, WatchError, WatchFrame};
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

pub type SessionWatchFrame = WatchFrame<ProjectSessionSnapshot, CommittedSessionEvent>;

pub struct AgentActivityUpdate {
    pub event_id: SessionEventId,
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub activity_id: String,
    pub phase: String,
    pub message: Option<String>,
    pub status: SessionActivityStatus,
    pub native_extensions: Vec<SessionNativeExtension>,
    pub committed_at_unix_ms: u64,
}

pub struct UserSteerRequest {
    pub command_id: CommandId,
    pub project_id: Option<ProjectId>,
    pub session_id: SessionId,
    pub agent_turn_id: TurnId,
    pub text: String,
    pub expected_revision: Option<u64>,
    pub committed_at_unix_ms: u64,
}

pub struct UserSteerCompletion {
    pub event_id: SessionEventId,
    pub session_id: SessionId,
    pub user_turn_id: TurnId,
    pub agent_turn_id: TurnId,
    pub state: SessionTurnState,
    pub error: Option<SafeSessionError>,
    pub committed_at_unix_ms: u64,
}

#[derive(Clone)]
pub struct SessionCoordinator {
    journal: SessionJournal,
    authority_epoch: u64,
    updates: broadcast::Sender<CommittedSessionEvent>,
    append_gate: Arc<Mutex<()>>,
}

impl SessionCoordinator {
    #[must_use]
    pub fn new(journal: SessionJournal, authority_epoch: u64, watch_capacity: usize) -> Self {
        let (updates, _) = broadcast::channel(watch_capacity.max(1));
        Self {
            journal,
            authority_epoch,
            updates,
            // The M01 SQLite control store has one writer connection. Keep the
            // journal's load-validate-append transaction atomic with respect to
            // every in-process producer (runtime progress, steer, text, stop).
            append_gate: Arc::new(Mutex::new(())),
        }
    }

    pub async fn create_session(
        &self,
        command_id: CommandId,
        project_id: Option<ProjectId>,
        title: String,
        committed_at_unix_ms: u64,
    ) -> Result<SessionCommit, SessionJournalError> {
        let _append_guard = self.append_gate.lock().await;
        if let Some(existing) = self.journal.event_for_command(command_id).await? {
            return self
                .replay_created_session(existing, project_id, &title)
                .await;
        }
        let expected_title = title.clone();
        let pending = PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id: SessionId::new(),
            command_id: Some(command_id),
            body: SessionEventBody::SessionCreated { project_id, title },
            committed_at_unix_ms,
        };
        match self.append_and_publish_unlocked(pending).await {
            Ok(commit) => Ok(commit),
            Err(SessionJournalError::IdempotencyConflict) => {
                let existing = self
                    .journal
                    .event_for_command(command_id)
                    .await?
                    .ok_or(SessionJournalError::IdempotencyConflict)?;
                self.replay_created_session(existing, project_id, &expected_title)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    pub async fn accept_turn(
        &self,
        command_id: CommandId,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        text: String,
        expected_revision: Option<u64>,
        committed_at_unix_ms: u64,
    ) -> Result<AcceptedTurn, SessionJournalError> {
        let _append_guard = self.append_gate.lock().await;
        if let Some(existing) = self.journal.event_for_command(command_id).await? {
            return self
                .replay_accepted_turn(existing, project_id, session_id, &text)
                .await;
        }
        let snapshot = self.journal.restore(session_id).await?;
        if let Some(expected) = expected_revision
            && expected != snapshot.session.revision
        {
            return Err(SessionJournalError::RevisionConflict {
                expected,
                actual: snapshot.session.revision,
            });
        }
        if snapshot.session.project_id != project_id {
            return Err(SessionJournalError::InvalidTransition(
                "turn project does not own the session",
            ));
        }
        let user_turn_id = TurnId::new();
        let agent_turn_id = TurnId::new();
        let expected_text = text.clone();
        let pending = PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id,
            command_id: Some(command_id),
            body: SessionEventBody::TurnAccepted {
                user_turn_id,
                agent_turn_id,
                command_id,
                text,
            },
            committed_at_unix_ms,
        };
        let commit = match self.append_and_publish_unlocked(pending).await {
            Ok(commit) => commit,
            Err(SessionJournalError::IdempotencyConflict) => {
                let existing = self
                    .journal
                    .event_for_command(command_id)
                    .await?
                    .ok_or(SessionJournalError::IdempotencyConflict)?;
                return self
                    .replay_accepted_turn(existing, project_id, session_id, &expected_text)
                    .await;
            }
            Err(error) => return Err(error),
        };
        Ok(AcceptedTurn {
            user_turn_id,
            agent_turn_id,
            replayed: false,
            commit,
        })
    }

    pub async fn append_agent_text(
        &self,
        event_id: SessionEventId,
        session_id: SessionId,
        turn_id: TurnId,
        text: String,
        committed_at_unix_ms: u64,
    ) -> Result<SessionCommit, SessionJournalError> {
        self.append_and_publish(PendingSessionEvent {
            event_id,
            session_id,
            command_id: None,
            body: SessionEventBody::AgentTextAppended { turn_id, text },
            committed_at_unix_ms,
        })
        .await
    }

    pub async fn request_steer(
        &self,
        request: UserSteerRequest,
    ) -> Result<AcceptedTurn, SessionJournalError> {
        let UserSteerRequest {
            command_id,
            project_id,
            session_id,
            agent_turn_id,
            text,
            expected_revision,
            committed_at_unix_ms,
        } = request;
        let _append_guard = self.append_gate.lock().await;
        if let Some(existing) = self.journal.event_for_command(command_id).await? {
            return self
                .replay_requested_steer(existing, project_id, session_id, agent_turn_id, &text)
                .await;
        }
        let snapshot = self.journal.restore(session_id).await?;
        if let Some(expected) = expected_revision
            && expected != snapshot.session.revision
        {
            return Err(SessionJournalError::RevisionConflict {
                expected,
                actual: snapshot.session.revision,
            });
        }
        if snapshot.session.project_id != project_id
            || snapshot.session.active_turn_id != Some(agent_turn_id)
        {
            return Err(SessionJournalError::InvalidTransition(
                "steer scope does not own the active session turn",
            ));
        }
        let user_turn_id = TurnId::new();
        let expected_text = text.clone();
        let pending = PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id,
            command_id: Some(command_id),
            body: SessionEventBody::UserSteerRequested {
                user_turn_id,
                agent_turn_id,
                command_id,
                text,
            },
            committed_at_unix_ms,
        };
        let commit = match self.append_and_publish_unlocked(pending).await {
            Ok(commit) => commit,
            Err(SessionJournalError::IdempotencyConflict) => {
                let existing = self
                    .journal
                    .event_for_command(command_id)
                    .await?
                    .ok_or(SessionJournalError::IdempotencyConflict)?;
                return self
                    .replay_requested_steer(
                        existing,
                        project_id,
                        session_id,
                        agent_turn_id,
                        &expected_text,
                    )
                    .await;
            }
            Err(error) => return Err(error),
        };
        Ok(AcceptedTurn {
            user_turn_id,
            agent_turn_id,
            replayed: false,
            commit,
        })
    }

    pub async fn finish_steer(
        &self,
        completion: UserSteerCompletion,
    ) -> Result<SessionCommit, SessionJournalError> {
        self.append_and_publish(PendingSessionEvent {
            event_id: completion.event_id,
            session_id: completion.session_id,
            command_id: None,
            body: SessionEventBody::UserSteerFinished {
                user_turn_id: completion.user_turn_id,
                agent_turn_id: completion.agent_turn_id,
                state: completion.state,
                error: completion.error,
            },
            committed_at_unix_ms: completion.committed_at_unix_ms,
        })
        .await
    }

    pub async fn upsert_agent_activity(
        &self,
        update: AgentActivityUpdate,
    ) -> Result<SessionCommit, SessionJournalError> {
        self.append_and_publish(PendingSessionEvent {
            event_id: update.event_id,
            session_id: update.session_id,
            command_id: None,
            body: SessionEventBody::AgentActivityUpserted {
                turn_id: update.turn_id,
                activity_id: update.activity_id,
                phase: update.phase,
                message: update.message,
                status: update.status,
                native_extensions: update.native_extensions,
            },
            committed_at_unix_ms: update.committed_at_unix_ms,
        })
        .await
    }

    pub async fn finish_turn(
        &self,
        event_id: SessionEventId,
        session_id: SessionId,
        turn_id: TurnId,
        state: SessionTurnState,
        outcome: Option<SessionTurnOutcome>,
        committed_at_unix_ms: u64,
    ) -> Result<SessionCommit, SessionJournalError> {
        self.append_and_publish(PendingSessionEvent {
            event_id,
            session_id,
            command_id: None,
            body: SessionEventBody::TurnFinished {
                turn_id,
                state,
                outcome,
            },
            committed_at_unix_ms,
        })
        .await
    }

    pub async fn restore_all(&self) -> Result<Vec<ProjectSessionSnapshot>, SessionJournalError> {
        self.journal.restore_all().await
    }

    pub async fn restore(
        &self,
        session_id: SessionId,
    ) -> Result<ProjectSessionSnapshot, SessionJournalError> {
        self.journal.restore(session_id).await
    }

    pub async fn event_for_command(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommittedSessionEvent>, SessionJournalError> {
        self.journal.event_for_command(command_id).await
    }

    pub async fn subscribe(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSubscription, SessionJournalError> {
        // Subscribe before loading the snapshot. Commits racing with bootstrap are
        // retained by the receiver and skipped if the loaded snapshot already includes them.
        let receiver = self.updates.subscribe();
        let snapshot = self.journal.restore(session_id).await?;
        let stream_id = SessionEventId::new().0.to_string();
        let initial = WatchFrame::Snapshot {
            cursor: WatchCursor {
                stream_id: stream_id.clone(),
                sequence: 1,
                authority_epoch: self.authority_epoch,
            },
            revision: snapshot.session.revision,
            fingerprint: snapshot.fingerprint.to_vec(),
            value: snapshot.clone(),
        };
        Ok(SessionSubscription {
            journal: self.journal.clone(),
            receiver,
            session_id,
            stream_id,
            authority_epoch: self.authority_epoch,
            sequence: 1,
            revision: snapshot.session.revision,
            blocked: false,
            initial: Some(initial),
        })
    }

    async fn append_and_publish(
        &self,
        event: PendingSessionEvent,
    ) -> Result<SessionCommit, SessionJournalError> {
        let _append_guard = self.append_gate.lock().await;
        self.append_and_publish_unlocked(event).await
    }

    async fn append_and_publish_unlocked(
        &self,
        event: PendingSessionEvent,
    ) -> Result<SessionCommit, SessionJournalError> {
        let commit = self.journal.append(event).await?;
        let _ = self.updates.send(commit.event.clone());
        Ok(commit)
    }

    async fn replay_created_session(
        &self,
        existing: CommittedSessionEvent,
        project_id: Option<ProjectId>,
        title: &str,
    ) -> Result<SessionCommit, SessionJournalError> {
        match &existing.body {
            SessionEventBody::SessionCreated {
                project_id: existing_project_id,
                title: existing_title,
            } if *existing_project_id == project_id && existing_title == title => {
                Ok(SessionCommit {
                    snapshot: self.journal.restore(existing.session_id).await?,
                    event: existing,
                })
            }
            _ => Err(SessionJournalError::IdempotencyConflict),
        }
    }

    async fn replay_accepted_turn(
        &self,
        existing: CommittedSessionEvent,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        text: &str,
    ) -> Result<AcceptedTurn, SessionJournalError> {
        match existing.body.clone() {
            SessionEventBody::TurnAccepted {
                user_turn_id,
                agent_turn_id,
                text: existing_text,
                ..
            } if existing.session_id == session_id && existing_text == text => {
                let snapshot = self.journal.restore(session_id).await?;
                if snapshot.session.project_id != project_id {
                    return Err(SessionJournalError::IdempotencyConflict);
                }
                Ok(AcceptedTurn {
                    user_turn_id,
                    agent_turn_id,
                    replayed: true,
                    commit: SessionCommit {
                        snapshot,
                        event: existing,
                    },
                })
            }
            _ => Err(SessionJournalError::IdempotencyConflict),
        }
    }

    async fn replay_requested_steer(
        &self,
        existing: CommittedSessionEvent,
        project_id: Option<ProjectId>,
        session_id: SessionId,
        agent_turn_id: TurnId,
        text: &str,
    ) -> Result<AcceptedTurn, SessionJournalError> {
        match existing.body.clone() {
            SessionEventBody::UserSteerRequested {
                user_turn_id,
                agent_turn_id: existing_agent_turn_id,
                text: existing_text,
                ..
            } if existing.session_id == session_id
                && existing_agent_turn_id == agent_turn_id
                && existing_text == text =>
            {
                let snapshot = self.journal.restore(session_id).await?;
                if snapshot.session.project_id != project_id {
                    return Err(SessionJournalError::IdempotencyConflict);
                }
                Ok(AcceptedTurn {
                    user_turn_id,
                    agent_turn_id,
                    replayed: true,
                    commit: SessionCommit {
                        snapshot,
                        event: existing,
                    },
                })
            }
            _ => Err(SessionJournalError::IdempotencyConflict),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedTurn {
    pub user_turn_id: TurnId,
    pub agent_turn_id: TurnId,
    pub replayed: bool,
    pub commit: SessionCommit,
}

pub struct SessionSubscription {
    journal: SessionJournal,
    receiver: broadcast::Receiver<CommittedSessionEvent>,
    session_id: SessionId,
    stream_id: String,
    authority_epoch: u64,
    sequence: u64,
    revision: u64,
    blocked: bool,
    initial: Option<SessionWatchFrame>,
}

impl SessionSubscription {
    pub fn take_initial(&mut self) -> Option<SessionWatchFrame> {
        self.initial.take()
    }

    pub fn heartbeat(&mut self) -> Option<SessionWatchFrame> {
        if self.blocked {
            return None;
        }
        self.sequence += 1;
        Some(WatchFrame::Heartbeat {
            cursor: self.cursor(),
            current_revision: self.revision,
        })
    }

    pub async fn recv(&mut self) -> Result<Option<SessionWatchFrame>, SessionJournalError> {
        if self.blocked {
            return Ok(None);
        }
        loop {
            match self.receiver.recv().await {
                Ok(event) if event.session_id != self.session_id => continue,
                Ok(event) if event.revision <= self.revision => continue,
                Ok(event) => {
                    self.sequence += 1;
                    if event.revision != self.revision + 1 {
                        self.blocked = true;
                        return Ok(Some(
                            self.resync_frame(ResyncReason::RevisionGap, event.revision),
                        ));
                    }
                    let base_revision = self.revision;
                    self.revision = event.revision;
                    return Ok(Some(WatchFrame::Delta {
                        cursor: self.cursor(),
                        base_revision,
                        new_revision: event.revision,
                        delta: event,
                    }));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let current = self.journal.restore(self.session_id).await?;
                    self.sequence += 1;
                    self.blocked = true;
                    return Ok(Some(self.resync_frame(
                        ResyncReason::SequenceGap,
                        current.session.revision,
                    )));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.blocked = true;
                    return Ok(Some(WatchFrame::Unavailable {
                        error: WatchError {
                            code: "watch_closed".to_owned(),
                            message_key: "session.watch_closed".to_owned(),
                        },
                    }));
                }
            }
        }
    }

    fn cursor(&self) -> WatchCursor {
        WatchCursor {
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            authority_epoch: self.authority_epoch,
        }
    }

    fn resync_frame(&self, reason: ResyncReason, current_revision: u64) -> SessionWatchFrame {
        WatchFrame::ResyncRequired {
            cursor: self.cursor(),
            current_revision,
            reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dennett_memory_core::session::{
        InMemorySessionEventStore, SessionResult, SessionTurnOutcome,
    };
    fn coordinator(capacity: usize) -> SessionCoordinator {
        SessionCoordinator::new(
            SessionJournal::new(Arc::new(InMemorySessionEventStore::default())),
            9,
            capacity,
        )
    }

    #[tokio::test]
    async fn watch_bootstraps_snapshot_then_monotonic_delta() {
        let coordinator = coordinator(8);
        let project_id = ProjectId::new();
        let created = coordinator
            .create_session(CommandId::new(), Some(project_id), "Chat".to_owned(), 1)
            .await
            .expect("create session");
        let session_id = created.snapshot.session.session_id;
        let mut subscription = coordinator.subscribe(session_id).await.expect("subscribe");
        let initial = subscription.take_initial().expect("initial snapshot");
        assert!(matches!(
            initial,
            WatchFrame::Snapshot {
                revision: 1,
                cursor: WatchCursor { sequence: 1, .. },
                ..
            }
        ));
        assert!(matches!(
            subscription.heartbeat(),
            Some(WatchFrame::Heartbeat {
                current_revision: 1,
                cursor: WatchCursor { sequence: 2, .. },
            })
        ));

        coordinator
            .accept_turn(
                CommandId::new(),
                Some(project_id),
                session_id,
                "hello".to_owned(),
                None,
                2,
            )
            .await
            .expect("accept turn");
        let delta = subscription.recv().await.expect("receive").expect("delta");
        assert!(matches!(
            delta,
            WatchFrame::Delta {
                base_revision: 1,
                new_revision: 2,
                cursor: WatchCursor { sequence: 3, .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn parallel_create_retries_allocate_one_session() {
        let coordinator = coordinator(32);
        let project_id = ProjectId::new();
        let command_id = CommandId::new();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..16 {
            let coordinator = coordinator.clone();
            tasks.spawn(async move {
                coordinator
                    .create_session(command_id, Some(project_id), "One chat".to_owned(), 1)
                    .await
            });
        }
        let mut session_id = None;
        while let Some(result) = tasks.join_next().await {
            let commit = result.expect("join create").expect("create or replay");
            assert_eq!(
                session_id.get_or_insert(commit.event.session_id),
                &commit.event.session_id
            );
        }
        assert_eq!(
            coordinator.restore_all().await.expect("restore all").len(),
            1
        );
    }

    #[tokio::test]
    async fn parallel_send_retries_allocate_one_turn_pair() {
        let coordinator = coordinator(32);
        let project_id = ProjectId::new();
        let created = coordinator
            .create_session(CommandId::new(), Some(project_id), "Chat".to_owned(), 1)
            .await
            .expect("create");
        let session_id = created.snapshot.session.session_id;
        let command_id = CommandId::new();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..16 {
            let coordinator = coordinator.clone();
            tasks.spawn(async move {
                coordinator
                    .accept_turn(
                        command_id,
                        Some(project_id),
                        session_id,
                        "send once".to_owned(),
                        Some(1),
                        2,
                    )
                    .await
            });
        }
        let mut turn_ids = None;
        while let Some(result) = tasks.join_next().await {
            let accepted = result.expect("join send").expect("accept or replay");
            let expected = turn_ids.get_or_insert((accepted.user_turn_id, accepted.agent_turn_id));
            assert_eq!(*expected, (accepted.user_turn_id, accepted.agent_turn_id));
        }
        let snapshot = coordinator.restore(session_id).await.expect("restore");
        assert_eq!(snapshot.turns.len(), 2);
        assert_eq!(snapshot.session.revision, 2);
    }

    #[tokio::test]
    async fn lagged_head_stream_requires_new_snapshot() {
        let coordinator = coordinator(1);
        let project_id = ProjectId::new();
        let created = coordinator
            .create_session(CommandId::new(), Some(project_id), "Gap".to_owned(), 1)
            .await
            .expect("create session");
        let session_id = created.snapshot.session.session_id;
        let mut subscription = coordinator.subscribe(session_id).await.expect("subscribe");
        subscription.take_initial();
        let accepted = coordinator
            .accept_turn(
                CommandId::new(),
                Some(project_id),
                session_id,
                "stream".to_owned(),
                None,
                2,
            )
            .await
            .expect("accept turn");
        coordinator
            .append_agent_text(
                SessionEventId::new(),
                session_id,
                accepted.agent_turn_id,
                "partial".to_owned(),
                3,
            )
            .await
            .expect("append text");
        coordinator
            .finish_turn(
                SessionEventId::new(),
                session_id,
                accepted.agent_turn_id,
                SessionTurnState::Completed,
                Some(SessionTurnOutcome::Result(SessionResult {
                    summary: "done".to_owned(),
                    partial: false,
                    artifact_handles: Vec::new(),
                    evidence_handles: Vec::new(),
                })),
                4,
            )
            .await
            .expect("finish turn");

        let frame = subscription
            .recv()
            .await
            .expect("receive")
            .expect("resync frame");
        assert!(matches!(
            frame,
            WatchFrame::ResyncRequired {
                reason: ResyncReason::SequenceGap,
                current_revision: 4,
                ..
            }
        ));
        assert!(
            subscription
                .recv()
                .await
                .expect("blocked receiver")
                .is_none()
        );
    }

    #[tokio::test]
    async fn create_and_turn_admission_are_idempotent_by_command() {
        let coordinator = coordinator(8);
        let project_id = ProjectId::new();
        let create_command = CommandId::new();
        let first = coordinator
            .create_session(create_command, Some(project_id), "Stable".to_owned(), 1)
            .await
            .expect("create");
        let retry = coordinator
            .create_session(create_command, Some(project_id), "Stable".to_owned(), 1)
            .await
            .expect("retry create");
        assert_eq!(
            first.snapshot.session.session_id,
            retry.snapshot.session.session_id
        );
        assert_eq!(retry.snapshot.session.revision, 1);

        let turn_command = CommandId::new();
        let first_turn = coordinator
            .accept_turn(
                turn_command,
                Some(project_id),
                first.snapshot.session.session_id,
                "first".to_owned(),
                None,
                3,
            )
            .await
            .expect("accept");
        let retry_turn = coordinator
            .accept_turn(
                turn_command,
                Some(project_id),
                first.snapshot.session.session_id,
                "first".to_owned(),
                None,
                3,
            )
            .await
            .expect("retry accept");
        assert_eq!(first_turn.agent_turn_id, retry_turn.agent_turn_id);
        assert_eq!(retry_turn.commit.snapshot.session.revision, 2);
        assert_eq!(
            coordinator
                .accept_turn(
                    CommandId::new(),
                    Some(project_id),
                    first.snapshot.session.session_id,
                    "stale".to_owned(),
                    Some(1),
                    4,
                )
                .await,
            Err(SessionJournalError::RevisionConflict {
                expected: 1,
                actual: 2,
            })
        );
        assert_eq!(
            coordinator
                .accept_turn(
                    turn_command,
                    Some(ProjectId::new()),
                    first.snapshot.session.session_id,
                    "first".to_owned(),
                    None,
                    3,
                )
                .await,
            Err(SessionJournalError::IdempotencyConflict)
        );
    }
}
