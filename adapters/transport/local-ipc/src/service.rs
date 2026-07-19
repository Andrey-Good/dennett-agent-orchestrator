use crate::auth::AuthError;
use crate::protocol::dennett::common::v1::ErrorEnvelope;
use crate::protocol::dennett::control::v1::bootstrap_response;
use crate::protocol::dennett::control::v1::get_health_response;
use crate::protocol::dennett::control::v1::handshake_response;
use crate::protocol::dennett::control::v1::system_mutation;
use crate::protocol::dennett::control::v1::system_service_server::{
    SystemService, SystemServiceServer,
};
use crate::protocol::dennett::control::v1::system_watch_frame;
use crate::protocol::dennett::control::v1::{
    BootstrapRequest, BootstrapResponse, BootstrapSnapshot, GetHealthRequest, GetHealthResponse,
    GetHealthResult, HandshakeRequest, HandshakeResponse, HealthState, ProjectState,
    ProjectSummary as WireProjectSummary, SessionState, SessionSummary as WireSessionSummary,
    SystemDelta as WireSystemDelta, SystemHealthUpdate, SystemMutation as WireSystemMutation,
    SystemSelectionUpdate, SystemSnapshot as WireSystemSnapshot, SystemWatchFrame, WatchRequest,
    WatchResponse,
};
use crate::protocol::dennett::sync::v1::{
    ResyncReason as WireResyncReason, ResyncRequired, WatchCursor as WireWatchCursor,
    WatchHeartbeat,
};
use crate::{LocalEndpoint, PeerIdentity, SessionRegistry, TransportError};
use dennett_head::system::{
    ProjectSummary, SessionSummary, SystemDelta, SystemHealth, SystemMutation, SystemSnapshot,
    SystemStateError, SystemStatePort, SystemWatchFrame as DomainWatchFrame,
};
use dennett_sync_core::watch::{ResyncReason, WatchCursor, WatchFrame};
use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct SystemServiceAdapter<S: SystemStatePort> {
    state: Arc<S>,
    sessions: SessionRegistry,
}

impl<S: SystemStatePort> SystemServiceAdapter<S> {
    #[must_use]
    pub fn new(state: Arc<S>, sessions: SessionRegistry) -> Self {
        Self { state, sessions }
    }
}

#[tonic::async_trait]
impl<S: SystemStatePort + 'static> SystemService for SystemServiceAdapter<S> {
    async fn handshake(
        &self,
        request: Request<HandshakeRequest>,
    ) -> Result<Response<HandshakeResponse>, Status> {
        let peer = peer(&request);
        let hello = request.into_inner().hello;
        let outcome = match (peer, hello) {
            (Ok(peer), Some(hello)) => self
                .sessions
                .issue(&peer, hello)
                .map(handshake_response::Outcome::Welcome)
                .unwrap_or_else(|error| handshake_response::Outcome::Error(auth_error(error))),
            (Err(error), _) => handshake_response::Outcome::Error(auth_error(error)),
            (_, None) => {
                handshake_response::Outcome::Error(auth_error(AuthError::ClientIdentityInvalid))
            }
        };
        Ok(Response::new(HandshakeResponse {
            outcome: Some(outcome),
        }))
    }

    async fn bootstrap(
        &self,
        request: Request<BootstrapRequest>,
    ) -> Result<Response<BootstrapResponse>, Status> {
        let peer = peer(&request);
        let request = request.into_inner();
        let outcome = match peer.and_then(|peer| {
            self.sessions.consume_bootstrap(
                &peer,
                &request.client_session_id,
                &request.session_proof,
            )
        }) {
            Ok(authenticated) => match self.state.bootstrap().await {
                Ok(snapshot) if snapshot.authority_epoch == authenticated.authority_epoch => {
                    bootstrap_response::Outcome::Snapshot(snapshot_to_wire(&snapshot))
                }
                Ok(_) => {
                    bootstrap_response::Outcome::Error(auth_error(AuthError::AuthorityEpochChanged))
                }
                Err(error) => bootstrap_response::Outcome::Error(state_error(error)),
            },
            Err(error) => bootstrap_response::Outcome::Error(auth_error(error)),
        };
        Ok(Response::new(BootstrapResponse {
            outcome: Some(outcome),
        }))
    }

    type WatchStream = Pin<Box<dyn Stream<Item = Result<WatchResponse, Status>> + Send>>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let peer = peer(&request);
        let request = request.into_inner();
        let authentication = peer.and_then(|peer| {
            self.sessions
                .authorize_active(&peer, &request.client_session_id)
        });
        let state = self.state.clone();
        let stream = async_stream::stream! {
            if let Err(error) = authentication {
                yield Ok(error_watch_response(auth_error(error)));
                return;
            }
            let mut subscription = match state.subscribe().await {
                Ok(subscription) => subscription,
                Err(error) => {
                    yield Ok(error_watch_response(state_error(error)));
                    return;
                }
            };
            let Some(initial) = subscription.take_initial() else {
                yield Ok(error_watch_response(internal_error(
                    "system_watch_missing_snapshot",
                    "system.watch_missing_snapshot",
                )));
                return;
            };
            yield Ok(watch_to_wire(initial));
            loop {
                match subscription.recv().await {
                    Ok(Some(frame)) => yield Ok(watch_to_wire(frame)),
                    Ok(None) => return,
                    Err(error) => {
                        yield Ok(error_watch_response(state_error(error)));
                        return;
                    }
                }
            }
        };
        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_health(
        &self,
        _request: Request<GetHealthRequest>,
    ) -> Result<Response<GetHealthResponse>, Status> {
        let outcome = match self.state.bootstrap().await {
            Ok(snapshot) => get_health_response::Outcome::Health(GetHealthResult {
                state: health_to_wire(snapshot.health),
                node_version: self.sessions.policy().node_version.clone(),
                observed_at: Some(timestamp(snapshot.observed_at_unix_ms)),
                status_code: "node_ready".to_owned(),
            }),
            Err(error) => get_health_response::Outcome::Error(state_error(error)),
        };
        Ok(Response::new(GetHealthResponse {
            outcome: Some(outcome),
        }))
    }
}

#[cfg(windows)]
pub async fn run_system_server<S, F>(
    endpoint: LocalEndpoint,
    service: SystemServiceAdapter<S>,
    shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let incoming = crate::transport::secure_incoming(endpoint)?;
    tonic::transport::Server::builder()
        .add_service(SystemServiceServer::new(service))
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await
        .map_err(TransportError::from)
}

#[cfg(not(windows))]
pub async fn run_system_server<S, F>(
    _endpoint: LocalEndpoint,
    _service: SystemServiceAdapter<S>,
    _shutdown: F,
) -> Result<(), TransportError>
where
    S: SystemStatePort + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    Err(TransportError::UnsupportedPlatform)
}

fn peer<T>(request: &Request<T>) -> Result<PeerIdentity, AuthError> {
    request
        .extensions()
        .get::<PeerIdentity>()
        .cloned()
        .ok_or(AuthError::ConnectionMismatch)
}

fn snapshot_to_wire(snapshot: &SystemSnapshot) -> BootstrapSnapshot {
    BootstrapSnapshot {
        revision: snapshot.revision,
        authority_epoch: snapshot.authority_epoch,
        observed_at: Some(timestamp(snapshot.observed_at_unix_ms)),
        projects: snapshot.projects.iter().map(project_to_wire).collect(),
        recent_sessions: snapshot
            .recent_sessions
            .iter()
            .map(session_to_wire)
            .collect(),
        active_project_id: snapshot.active_project_id.clone().unwrap_or_default(),
        active_session_id: snapshot.active_session_id.clone().unwrap_or_default(),
        node_state: health_to_wire(snapshot.health),
    }
}

fn project_to_wire(project: &ProjectSummary) -> WireProjectSummary {
    WireProjectSummary {
        project_id: project.project_id.clone(),
        display_name: project.display_name.clone(),
        state: ProjectState::Ready as i32,
        revision: project.revision,
        last_activity_at: None,
    }
}

fn session_to_wire(session: &SessionSummary) -> WireSessionSummary {
    WireSessionSummary {
        session_id: session.session_id.clone(),
        project_id: session.project_id.clone(),
        title: session.title.clone(),
        state: SessionState::Idle as i32,
        revision: session.revision,
        active_turn_id: String::new(),
        last_activity_at: None,
    }
}

fn delta_to_wire(base_revision: u64, new_revision: u64, delta: SystemDelta) -> WireSystemDelta {
    WireSystemDelta {
        base_revision,
        new_revision,
        mutations: delta
            .mutations
            .into_iter()
            .map(|mutation| WireSystemMutation {
                mutation: Some(match mutation {
                    SystemMutation::UpsertProject(project) => {
                        system_mutation::Mutation::UpsertProject(project_to_wire(&project))
                    }
                    SystemMutation::RemoveProject(project_id) => {
                        system_mutation::Mutation::RemoveProjectId(project_id)
                    }
                    SystemMutation::UpsertSession(session) => {
                        system_mutation::Mutation::UpsertSession(session_to_wire(&session))
                    }
                    SystemMutation::RemoveSession(session_id) => {
                        system_mutation::Mutation::RemoveSessionId(session_id)
                    }
                    SystemMutation::Select {
                        project_id,
                        session_id,
                    } => system_mutation::Mutation::UpdateSelection(SystemSelectionUpdate {
                        active_project_id: Some(project_id.unwrap_or_default()),
                        active_session_id: Some(session_id.unwrap_or_default()),
                    }),
                    SystemMutation::SetHealth(health) => {
                        system_mutation::Mutation::UpdateHealth(SystemHealthUpdate {
                            node_state: health_to_wire(health),
                            status_code: "node_health_changed".to_owned(),
                            observed_at: None,
                        })
                    }
                }),
            })
            .collect(),
    }
}

fn watch_to_wire(frame: DomainWatchFrame) -> WatchResponse {
    let frame = match frame {
        WatchFrame::Snapshot {
            cursor,
            fingerprint,
            value,
            ..
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Snapshot(WireSystemSnapshot {
                bootstrap: Some(snapshot_to_wire(&value)),
                snapshot_fingerprint: fingerprint,
            })),
        },
        WatchFrame::Delta {
            cursor,
            base_revision,
            new_revision,
            delta,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Delta(delta_to_wire(
                base_revision,
                new_revision,
                delta,
            ))),
        },
        WatchFrame::Heartbeat {
            cursor,
            current_revision,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::Heartbeat(WatchHeartbeat {
                observed_at: None,
                current_revision,
            })),
        },
        WatchFrame::ResyncRequired {
            cursor,
            current_revision,
            reason,
        } => SystemWatchFrame {
            cursor: Some(cursor_to_wire(cursor)),
            frame: Some(system_watch_frame::Frame::ResyncRequired(ResyncRequired {
                reason: resync_reason_to_wire(reason),
                current_revision,
                snapshot_required: true,
            })),
        },
        WatchFrame::Unavailable { error } => SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(internal_error(
                &error.code,
                &error.message_key,
            ))),
        },
        WatchFrame::AccessRevoked => SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(internal_error(
                "access_revoked",
                "system.access_revoked",
            ))),
        },
    };
    WatchResponse { frame: Some(frame) }
}

fn error_watch_response(error: ErrorEnvelope) -> WatchResponse {
    WatchResponse {
        frame: Some(SystemWatchFrame {
            cursor: None,
            frame: Some(system_watch_frame::Frame::Error(error)),
        }),
    }
}

fn cursor_to_wire(cursor: WatchCursor) -> WireWatchCursor {
    WireWatchCursor {
        stream_id: cursor.stream_id,
        sequence: cursor.sequence,
        authority_epoch: cursor.authority_epoch,
    }
}

fn health_to_wire(health: SystemHealth) -> i32 {
    (match health {
        SystemHealth::Starting => HealthState::Starting,
        SystemHealth::Ready => HealthState::Ready,
        SystemHealth::Degraded => HealthState::Degraded,
        SystemHealth::RecoveryRequired => HealthState::RecoveryRequired,
    }) as i32
}

fn resync_reason_to_wire(reason: ResyncReason) -> i32 {
    (match reason {
        ResyncReason::SequenceGap => WireResyncReason::SequenceGap,
        ResyncReason::RevisionGap => WireResyncReason::RevisionGap,
        ResyncReason::AuthorityEpochChanged => WireResyncReason::AuthorityEpochChanged,
        ResyncReason::StreamReplaced => WireResyncReason::StreamReplaced,
        ResyncReason::SnapshotInvalid => WireResyncReason::SnapshotInvalid,
    }) as i32
}

fn timestamp(unix_ms: u64) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: (unix_ms / 1_000).try_into().unwrap_or(i64::MAX),
        nanos: ((unix_ms % 1_000) * 1_000_000) as i32,
    }
}

fn auth_error(error: AuthError) -> ErrorEnvelope {
    ErrorEnvelope {
        code: error.code().to_owned(),
        message_key: format!("local_ipc.{}", error.code()),
        correlation_id: String::new(),
        retryable: error.retryable(),
        user_action_required: matches!(
            error,
            AuthError::InstallationMismatch | AuthError::ProtocolIncompatible
        ),
        details_handle: String::new(),
        current_revision: None,
    }
}

fn state_error(_error: SystemStateError) -> ErrorEnvelope {
    internal_error("system_unavailable", "system.unavailable")
}

fn internal_error(code: &str, message_key: &str) -> ErrorEnvelope {
    ErrorEnvelope {
        code: code.to_owned(),
        message_key: message_key.to_owned(),
        correlation_id: String::new(),
        retryable: true,
        user_action_required: false,
        details_handle: String::new(),
        current_revision: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::dennett::control::v1::system_service_server::SystemService;
    use crate::{HandshakePolicy, SessionRegistry};
    use dennett_head::system::{SystemMutation, SystemProjection};
    use tonic::codegen::tokio_stream::StreamExt;

    #[cfg(windows)]
    use crate::{AuthenticatedSystemClient, ClientConfig};
    #[cfg(windows)]
    use std::time::Duration;
    #[cfg(windows)]
    use tokio::sync::oneshot;

    fn peer() -> PeerIdentity {
        PeerIdentity {
            process_id: 7,
            user_sid: "S-1-test".to_owned(),
            connection_id: "connection-1".to_owned(),
        }
    }

    fn request<T>(value: T) -> Request<T> {
        let mut request = Request::new(value);
        request.extensions_mut().insert(peer());
        request
    }

    async fn authenticated(service: &SystemServiceAdapter<SystemProjection>) -> (String, Vec<u8>) {
        let response = service
            .handshake(request(HandshakeRequest {
                hello: Some(crate::protocol::dennett::control::v1::ClientHello {
                    client_component: "dennett-desktop-shell".to_owned(),
                    component_version: "0.1.0".to_owned(),
                    protocol_versions: vec![1],
                    installation_id: "install".to_owned(),
                    device_id: "device".to_owned(),
                    session_challenge: vec![9; 32],
                    requested_features: vec!["system-watch".to_owned()],
                }),
            }))
            .await
            .expect("handshake")
            .into_inner();
        match response.outcome.expect("outcome") {
            handshake_response::Outcome::Welcome(welcome) => {
                (welcome.client_session_id, welcome.session_proof)
            }
            handshake_response::Outcome::Error(error) => panic!("unexpected {error:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_consumes_proof_and_watch_starts_with_snapshot_then_delta() {
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(7), 8));
        let service = SystemServiceAdapter::new(
            projection.clone(),
            SessionRegistry::new(HandshakePolicy::m01("install", "node", 7)),
        );
        let (session_id, proof) = authenticated(&service).await;
        let bootstrap = service
            .bootstrap(request(BootstrapRequest {
                session_proof: proof.clone(),
                known_revision: None,
                client_session_id: session_id.clone(),
            }))
            .await
            .expect("bootstrap")
            .into_inner();
        assert!(matches!(
            bootstrap.outcome,
            Some(bootstrap_response::Outcome::Snapshot(BootstrapSnapshot {
                revision: 1,
                authority_epoch: 7,
                ..
            }))
        ));
        let replay = service
            .bootstrap(request(BootstrapRequest {
                session_proof: proof,
                known_revision: None,
                client_session_id: session_id.clone(),
            }))
            .await
            .expect("replay response")
            .into_inner();
        assert!(matches!(
            replay.outcome,
            Some(bootstrap_response::Outcome::Error(ErrorEnvelope { ref code, .. }))
                if code == "ipc_proof_consumed"
        ));

        let mut stream = service
            .watch(request(WatchRequest {
                client_session_id: session_id,
                known_revision: Some(1),
            }))
            .await
            .expect("watch")
            .into_inner();
        let first = stream.as_mut().next().await.expect("snapshot").expect("ok");
        assert!(matches!(
            first.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Snapshot(_))
        ));
        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Degraded)])
            .await;
        let second = stream.as_mut().next().await.expect("delta").expect("ok");
        assert!(matches!(
            second.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Delta(WireSystemDelta {
                base_revision: 1,
                new_revision: 2,
                ..
            }))
        ));
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authenticated_named_pipe_round_trip_bootstraps_and_streams_a_delta() {
        let installation_id = format!("ipc-integration-{}", uuid::Uuid::now_v7());
        let endpoint = LocalEndpoint::for_installation(installation_id.clone()).expect("endpoint");
        let projection = Arc::new(SystemProjection::new(SystemSnapshot::empty(11), 8));
        let service = SystemServiceAdapter::new(
            projection.clone(),
            SessionRegistry::new(HandshakePolicy::m01(
                installation_id.clone(),
                "node-test",
                11,
            )),
        );
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(async move {
            run_system_server(endpoint, service, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        let mut client = tokio::time::timeout(
            Duration::from_secs(5),
            AuthenticatedSystemClient::connect(ClientConfig::m01(
                installation_id,
                "desktop-test-device",
                "0.1.0-test",
            )),
        )
        .await
        .expect("client connection timed out")
        .expect("authenticated client");
        assert_eq!(client.bootstrap().revision, 1);
        assert_eq!(client.bootstrap().authority_epoch, 11);

        let mut watch = client.watch().await.expect("watch");
        let initial = tokio::time::timeout(Duration::from_secs(5), watch.message())
            .await
            .expect("initial watch frame timed out")
            .expect("initial watch status")
            .expect("initial watch frame");
        assert!(matches!(
            initial.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Snapshot(WireSystemSnapshot {
                bootstrap: Some(BootstrapSnapshot {
                    revision: 1,
                    authority_epoch: 11,
                    ..
                }),
                ..
            }))
        ));

        projection
            .apply(vec![SystemMutation::SetHealth(SystemHealth::Degraded)])
            .await;
        let update = tokio::time::timeout(Duration::from_secs(5), watch.message())
            .await
            .expect("delta watch frame timed out")
            .expect("delta watch status")
            .expect("delta watch frame");
        assert!(matches!(
            update.frame.and_then(|frame| frame.frame),
            Some(system_watch_frame::Frame::Delta(WireSystemDelta {
                base_revision: 1,
                new_revision: 2,
                ..
            }))
        ));

        drop(watch);
        drop(client);
        shutdown_tx.send(()).expect("shutdown receiver");
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("server shutdown timed out")
            .expect("server task")
            .expect("server result");
    }
}
