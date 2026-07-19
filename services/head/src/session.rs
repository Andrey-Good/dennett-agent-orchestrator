use dennett_contracts::{CommandId, ProjectId, SessionEventId, SessionId, TurnId};
use dennett_memory_core::session::{
    CommittedSessionEvent, PendingSessionEvent, ProjectSessionSnapshot, SessionCommit,
    SessionEventBody, SessionJournal, SessionJournalError, SessionTurnOutcome, SessionTurnState,
};
use dennett_sync_core::watch::{ResyncReason, WatchCursor, WatchError, WatchFrame};
use tokio::sync::broadcast;

pub type SessionWatchFrame = WatchFrame<ProjectSessionSnapshot, SessionEventBody>;

#[derive(Clone)]
pub struct SessionCoordinator {
    journal: SessionJournal,
    authority_epoch: u64,
    updates: broadcast::Sender<CommittedSessionEvent>,
}

impl SessionCoordinator {
    #[must_use]
    pub fn new(journal: SessionJournal, authority_epoch: u64, watch_capacity: usize) -> Self {
        let (updates, _) = broadcast::channel(watch_capacity.max(1));
        Self {
            journal,
            authority_epoch,
            updates,
        }
    }

    pub async fn create_session(
        &self,
        command_id: CommandId,
        project_id: ProjectId,
        title: String,
        committed_at_unix_ms: u64,
    ) -> Result<SessionCommit, SessionJournalError> {
        if let Some(existing) = self.journal.event_for_command(command_id).await? {
            return match &existing.body {
                SessionEventBody::SessionCreated {
                    project_id: existing_project_id,
                    title: existing_title,
                } if *existing_project_id == project_id && *existing_title == title => {
                    Ok(SessionCommit {
                        snapshot: self.journal.restore(existing.session_id).await?,
                        event: existing,
                    })
                }
                _ => Err(SessionJournalError::IdempotencyConflict),
            };
        }
        self.append_and_publish(PendingSessionEvent {
            event_id: SessionEventId::new(),
            session_id: SessionId::new(),
            command_id: Some(command_id),
            body: SessionEventBody::SessionCreated { project_id, title },
            committed_at_unix_ms,
        })
        .await
    }

    pub async fn accept_turn(
        &self,
        command_id: CommandId,
        project_id: ProjectId,
        session_id: SessionId,
        text: String,
        committed_at_unix_ms: u64,
    ) -> Result<AcceptedTurn, SessionJournalError> {
        if let Some(existing) = self.journal.event_for_command(command_id).await? {
            return match existing.body.clone() {
                SessionEventBody::TurnAccepted {
                    user_turn_id,
                    agent_turn_id,
                    text: existing_text,
                    ..
                } if existing.session_id == session_id && existing_text == text => {
                    Ok(AcceptedTurn {
                        user_turn_id,
                        agent_turn_id,
                        commit: SessionCommit {
                            snapshot: self.journal.restore(session_id).await?,
                            event: existing,
                        },
                    })
                }
                _ => Err(SessionJournalError::IdempotencyConflict),
            };
        }
        let snapshot = self.journal.restore(session_id).await?;
        if snapshot.session.project_id != project_id {
            return Err(SessionJournalError::InvalidTransition(
                "turn project does not own the session",
            ));
        }
        let user_turn_id = TurnId::new();
        let agent_turn_id = TurnId::new();
        let commit = self
            .append_and_publish(PendingSessionEvent {
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
            })
            .await?;
        Ok(AcceptedTurn {
            user_turn_id,
            agent_turn_id,
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
        let commit = self.journal.append(event).await?;
        let _ = self.updates.send(commit.event.clone());
        Ok(commit)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedTurn {
    pub user_turn_id: TurnId,
    pub agent_turn_id: TurnId,
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
                        delta: event.body,
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
    use std::sync::Arc;

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
            .create_session(CommandId::new(), project_id, "Chat".to_owned(), 1)
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

        coordinator
            .accept_turn(
                CommandId::new(),
                project_id,
                session_id,
                "hello".to_owned(),
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
                cursor: WatchCursor { sequence: 2, .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn lagged_head_stream_requires_new_snapshot() {
        let coordinator = coordinator(1);
        let project_id = ProjectId::new();
        let created = coordinator
            .create_session(CommandId::new(), project_id, "Gap".to_owned(), 1)
            .await
            .expect("create session");
        let session_id = created.snapshot.session.session_id;
        let mut subscription = coordinator.subscribe(session_id).await.expect("subscribe");
        subscription.take_initial();
        let accepted = coordinator
            .accept_turn(
                CommandId::new(),
                project_id,
                session_id,
                "stream".to_owned(),
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
            .create_session(create_command, project_id, "Stable".to_owned(), 1)
            .await
            .expect("create");
        let retry = coordinator
            .create_session(create_command, project_id, "Stable".to_owned(), 1)
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
                project_id,
                first.snapshot.session.session_id,
                "first".to_owned(),
                3,
            )
            .await
            .expect("accept");
        let retry_turn = coordinator
            .accept_turn(
                turn_command,
                project_id,
                first.snapshot.session.session_id,
                "first".to_owned(),
                3,
            )
            .await
            .expect("retry accept");
        assert_eq!(first_turn.agent_turn_id, retry_turn.agent_turn_id);
        assert_eq!(retry_turn.commit.snapshot.session.revision, 2);
    }
}
