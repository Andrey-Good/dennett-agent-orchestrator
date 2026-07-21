use async_trait::async_trait;
use dennett_agent_core::RuntimeDescriptor;
use dennett_contracts::SessionEventId;
use dennett_memory_core::session::ProjectSessionState;
use dennett_sync_core::watch::{ResyncReason, WatchCursor, WatchError, WatchFrame};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, broadcast};

pub type SystemWatchFrame = WatchFrame<SystemSnapshot, SystemDelta>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSummary {
    pub project_id: String,
    pub display_name: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_id: String,
    pub title: String,
    pub state: ProjectSessionState,
    pub revision: u64,
    pub active_turn_id: Option<String>,
    pub last_activity_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemHealth {
    Starting,
    Ready,
    Degraded,
    RecoveryRequired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemSnapshot {
    pub revision: u64,
    pub authority_epoch: u64,
    pub observed_at_unix_ms: u64,
    pub projects: Vec<ProjectSummary>,
    pub recent_sessions: Vec<SessionSummary>,
    pub active_project_id: Option<String>,
    pub active_session_id: Option<String>,
    pub health: SystemHealth,
    pub runtime: Option<RuntimeDescriptor>,
}

impl SystemSnapshot {
    #[must_use]
    pub fn empty(authority_epoch: u64) -> Self {
        Self {
            revision: 1,
            authority_epoch,
            observed_at_unix_ms: unix_time_ms(),
            projects: Vec::new(),
            recent_sessions: Vec::new(),
            active_project_id: None,
            active_session_id: None,
            health: SystemHealth::Ready,
            runtime: None,
        }
    }

    #[must_use]
    pub fn fingerprint(&self) -> Vec<u8> {
        let mut hash = Sha256::new();
        hash.update(self.revision.to_le_bytes());
        hash.update(self.authority_epoch.to_le_bytes());
        hash.update([self.health as u8]);
        if let Some(runtime) = &self.runtime {
            update_text(&mut hash, &runtime.adapter_id);
            hash.update([runtime.runtime_kind as u8]);
            hash.update([
                runtime.capabilities.streaming as u8,
                runtime.capabilities.continuation as u8,
                runtime.capabilities.scoped_cancellation as u8,
                runtime.capabilities.deadlines as u8,
                runtime.capabilities.steering as u8,
            ]);
            for schema in &runtime.capabilities.native_extension_schemas {
                update_text(&mut hash, schema);
            }
        }
        update_optional(&mut hash, self.active_project_id.as_deref());
        update_optional(&mut hash, self.active_session_id.as_deref());
        for project in &self.projects {
            update_text(&mut hash, &project.project_id);
            update_text(&mut hash, &project.display_name);
            hash.update(project.revision.to_le_bytes());
        }
        for session in &self.recent_sessions {
            update_text(&mut hash, &session.session_id);
            update_text(&mut hash, &session.project_id);
            update_text(&mut hash, &session.title);
            hash.update([session.state as u8]);
            hash.update(session.revision.to_le_bytes());
            update_optional(&mut hash, session.active_turn_id.as_deref());
            hash.update(session.last_activity_unix_ms.to_le_bytes());
        }
        hash.finalize().to_vec()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SystemMutation {
    UpsertProject(ProjectSummary),
    RemoveProject(String),
    UpsertSession(SessionSummary),
    RemoveSession(String),
    Select {
        project_id: Option<String>,
        session_id: Option<String>,
    },
    SetHealth(SystemHealth),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDelta {
    pub mutations: Vec<SystemMutation>,
}

#[derive(Debug, thiserror::Error)]
pub enum SystemStateError {
    #[error("system watch is unavailable")]
    WatchUnavailable,
}

#[async_trait]
pub trait SystemWatchSubscription: Send {
    fn take_initial(&mut self) -> Option<SystemWatchFrame>;
    fn heartbeat(&mut self) -> Option<SystemWatchFrame>;
    async fn recv(&mut self) -> Result<Option<SystemWatchFrame>, SystemStateError>;
}

#[async_trait]
pub trait SystemStatePort: Send + Sync {
    async fn bootstrap(&self) -> Result<SystemSnapshot, SystemStateError>;
    async fn subscribe(&self) -> Result<Box<dyn SystemWatchSubscription>, SystemStateError>;
}

#[derive(Clone)]
pub struct SystemProjection {
    state: std::sync::Arc<RwLock<SystemSnapshot>>,
    updates: broadcast::Sender<CommittedSystemDelta>,
}

impl SystemProjection {
    #[must_use]
    pub fn new(snapshot: SystemSnapshot, watch_capacity: usize) -> Self {
        let (updates, _) = broadcast::channel(watch_capacity.max(1));
        Self {
            state: std::sync::Arc::new(RwLock::new(snapshot)),
            updates,
        }
    }

    pub async fn apply(&self, mutations: Vec<SystemMutation>) -> SystemSnapshot {
        let mut state = self.state.write().await;
        let base_revision = state.revision;
        for mutation in &mutations {
            apply_mutation(&mut state, mutation);
        }
        state.revision += 1;
        state.observed_at_unix_ms = unix_time_ms();
        let committed = CommittedSystemDelta {
            base_revision,
            new_revision: state.revision,
            delta: SystemDelta { mutations },
        };
        let snapshot = state.clone();
        // Publication is part of the commit's critical section. Releasing the
        // write lock first would allow revision N+1 to be broadcast before N.
        let _ = self.updates.send(committed);
        snapshot
    }
}

#[async_trait]
impl SystemStatePort for SystemProjection {
    async fn bootstrap(&self) -> Result<SystemSnapshot, SystemStateError> {
        Ok(self.state.read().await.clone())
    }

    async fn subscribe(&self) -> Result<Box<dyn SystemWatchSubscription>, SystemStateError> {
        // Subscribe before loading the snapshot so a racing commit is retained.
        let receiver = self.updates.subscribe();
        let snapshot = self.state.read().await.clone();
        let stream_id = format!("system-{}", SessionEventId::new().0);
        let initial = WatchFrame::Snapshot {
            cursor: WatchCursor {
                stream_id: stream_id.clone(),
                sequence: 1,
                authority_epoch: snapshot.authority_epoch,
            },
            revision: snapshot.revision,
            fingerprint: snapshot.fingerprint(),
            value: snapshot.clone(),
        };
        Ok(Box::new(ProjectionSubscription {
            receiver,
            stream_id,
            authority_epoch: snapshot.authority_epoch,
            sequence: 1,
            revision: snapshot.revision,
            blocked: false,
            initial: Some(initial),
        }))
    }
}

#[derive(Clone, Debug)]
struct CommittedSystemDelta {
    base_revision: u64,
    new_revision: u64,
    delta: SystemDelta,
}

struct ProjectionSubscription {
    receiver: broadcast::Receiver<CommittedSystemDelta>,
    stream_id: String,
    authority_epoch: u64,
    sequence: u64,
    revision: u64,
    blocked: bool,
    initial: Option<SystemWatchFrame>,
}

#[async_trait]
impl SystemWatchSubscription for ProjectionSubscription {
    fn take_initial(&mut self) -> Option<SystemWatchFrame> {
        self.initial.take()
    }

    fn heartbeat(&mut self) -> Option<SystemWatchFrame> {
        if self.blocked {
            return None;
        }
        self.sequence += 1;
        Some(WatchFrame::Heartbeat {
            cursor: self.cursor(),
            current_revision: self.revision,
        })
    }

    async fn recv(&mut self) -> Result<Option<SystemWatchFrame>, SystemStateError> {
        if self.blocked {
            return Ok(None);
        }
        loop {
            match self.receiver.recv().await {
                Ok(delta) if delta.new_revision <= self.revision => continue,
                Ok(delta) if delta.base_revision != self.revision => {
                    self.sequence += 1;
                    self.blocked = true;
                    return Ok(Some(
                        self.resync(ResyncReason::RevisionGap, delta.new_revision),
                    ));
                }
                Ok(delta) => {
                    self.sequence += 1;
                    self.revision = delta.new_revision;
                    return Ok(Some(WatchFrame::Delta {
                        cursor: self.cursor(),
                        base_revision: delta.base_revision,
                        new_revision: delta.new_revision,
                        delta: delta.delta,
                    }));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.sequence += 1;
                    self.blocked = true;
                    return Ok(Some(self.resync(ResyncReason::SequenceGap, self.revision)));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.blocked = true;
                    return Ok(Some(WatchFrame::Unavailable {
                        error: WatchError {
                            code: "watch_closed".to_owned(),
                            message_key: "system.watch_closed".to_owned(),
                        },
                    }));
                }
            }
        }
    }
}

impl ProjectionSubscription {
    fn cursor(&self) -> WatchCursor {
        WatchCursor {
            stream_id: self.stream_id.clone(),
            sequence: self.sequence,
            authority_epoch: self.authority_epoch,
        }
    }

    fn resync(&self, reason: ResyncReason, current_revision: u64) -> SystemWatchFrame {
        WatchFrame::ResyncRequired {
            cursor: self.cursor(),
            current_revision,
            reason,
        }
    }
}

fn apply_mutation(state: &mut SystemSnapshot, mutation: &SystemMutation) {
    match mutation {
        SystemMutation::UpsertProject(project) => {
            state
                .projects
                .retain(|item| item.project_id != project.project_id);
            state.projects.push(project.clone());
        }
        SystemMutation::RemoveProject(project_id) => {
            state.projects.retain(|item| item.project_id != *project_id);
        }
        SystemMutation::UpsertSession(session) => {
            state
                .recent_sessions
                .retain(|item| item.session_id != session.session_id);
            state.recent_sessions.push(session.clone());
        }
        SystemMutation::RemoveSession(session_id) => {
            state
                .recent_sessions
                .retain(|item| item.session_id != *session_id);
        }
        SystemMutation::Select {
            project_id,
            session_id,
        } => {
            state.active_project_id.clone_from(project_id);
            state.active_session_id.clone_from(session_id);
        }
        SystemMutation::SetHealth(health) => state.health = *health,
    }
}

fn update_text(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value.as_bytes());
}

fn update_optional(hash: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hash.update([1]);
            update_text(hash, value);
        }
        None => hash.update([0]),
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn system_watch_starts_with_snapshot_then_monotonic_delta() {
        let projection = SystemProjection::new(SystemSnapshot::empty(7), 8);
        let mut subscription = projection.subscribe().await.expect("subscribe");
        assert!(matches!(
            subscription.take_initial(),
            Some(WatchFrame::Snapshot {
                revision: 1,
                cursor: WatchCursor {
                    sequence: 1,
                    authority_epoch: 7,
                    ..
                },
                ..
            })
        ));
        assert!(matches!(
            subscription.heartbeat(),
            Some(WatchFrame::Heartbeat {
                current_revision: 1,
                cursor: WatchCursor { sequence: 2, .. },
            })
        ));

        projection
            .apply(vec![SystemMutation::Select {
                project_id: Some("project-1".to_owned()),
                session_id: Some("session-1".to_owned()),
            }])
            .await;

        assert!(matches!(
            subscription.recv().await.expect("receive"),
            Some(WatchFrame::Delta {
                base_revision: 1,
                new_revision: 2,
                cursor: WatchCursor { sequence: 3, .. },
                ..
            })
        ));
    }

    #[tokio::test]
    async fn lagged_system_watch_requires_resync_and_stops_deltas() {
        let projection = SystemProjection::new(SystemSnapshot::empty(7), 1);
        let mut subscription = projection.subscribe().await.expect("subscribe");
        subscription.take_initial();
        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Degraded)])
            .await;
        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Ready)])
            .await;

        assert!(matches!(
            subscription.recv().await.expect("receive"),
            Some(WatchFrame::ResyncRequired {
                reason: ResyncReason::SequenceGap,
                ..
            })
        ));
        assert!(subscription.recv().await.expect("blocked").is_none());
    }

    #[tokio::test]
    async fn rapid_subscriptions_receive_distinct_stream_identities() {
        let projection = SystemProjection::new(SystemSnapshot::empty(7), 8);
        let mut first = projection.subscribe().await.expect("first subscription");
        let mut second = projection.subscribe().await.expect("second subscription");
        let first_stream = match first.take_initial().expect("first snapshot") {
            WatchFrame::Snapshot { cursor, .. } => cursor.stream_id,
            frame => panic!("expected first snapshot, got {frame:?}"),
        };
        let second_stream = match second.take_initial().expect("second snapshot") {
            WatchFrame::Snapshot { cursor, .. } => cursor.stream_id,
            frame => panic!("expected second snapshot, got {frame:?}"),
        };

        assert_ne!(first_stream, second_stream);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_commits_publish_in_revision_order() {
        const COMMITS: usize = 32;
        let projection =
            std::sync::Arc::new(SystemProjection::new(SystemSnapshot::empty(7), COMMITS + 1));
        let mut subscription = projection.subscribe().await.expect("subscribe");
        subscription.take_initial();
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(COMMITS + 1));
        let mut tasks = Vec::with_capacity(COMMITS);

        for index in 0..COMMITS {
            let projection = projection.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                projection
                    .apply(vec![SystemMutation::Select {
                        project_id: Some(format!("project-{index}")),
                        session_id: None,
                    }])
                    .await
            }));
        }
        barrier.wait().await;
        for task in tasks {
            task.await.expect("concurrent commit task");
        }

        for expected_revision in 2..=(COMMITS as u64 + 1) {
            assert!(matches!(
                subscription.recv().await.expect("ordered delta"),
                Some(WatchFrame::Delta {
                    base_revision,
                    new_revision,
                    ..
                }) if base_revision + 1 == expected_revision && new_revision == expected_revision
            ));
        }
    }
}
