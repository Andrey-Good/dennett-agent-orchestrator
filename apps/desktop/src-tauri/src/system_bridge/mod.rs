mod conversation;
mod dto;
mod installation;
mod node_process;

pub use conversation::*;
pub use dto::{
    CloseSystemWatchRequest, DesktopSystemEvent, OpenSystemWatchRequest, OpenSystemWatchResponse,
    UiSafeError,
};

use dennett_local_ipc::{
    AuthenticatedSystemClient, AuthenticatedSystemWatch, ClientConfig, ClientError,
};
use dto::{BridgePhase, DesktopSystemSnapshot, frame_to_event};
use installation::{InstallationError, InstallationMetadata};
use node_process::NodeStartError;
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::sync::watch;

const COMPONENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(5);
const INITIAL_WATCH_SNAPSHOT_DEADLINE: Duration = Duration::from_secs(5);
const NODE_READY_DEADLINE: Duration = Duration::from_secs(5);
const NODE_READY_RETRY_DELAY: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct DesktopBridge {
    inner: Arc<DesktopBridgeInner>,
}

struct DesktopBridgeInner {
    data_dir: Option<PathBuf>,
    node_starter: Arc<dyn NodeStarter>,
    node_start_gate: Arc<tokio::sync::Mutex<()>>,
    subscriptions: Mutex<HashMap<String, ActiveSubscription>>,
}

struct ActiveSubscription {
    subscription_id: String,
    cancel: watch::Sender<bool>,
}

impl DesktopBridge {
    #[must_use]
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self::with_node_starter(data_dir, Arc::new(OsNodeStarter))
    }

    fn with_node_starter(data_dir: Option<PathBuf>, node_starter: Arc<dyn NodeStarter>) -> Self {
        Self {
            inner: Arc::new(DesktopBridgeInner {
                data_dir,
                node_starter,
                node_start_gate: Arc::new(tokio::sync::Mutex::new(())),
                subscriptions: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn open_system_watch(
        &self,
        window_label: String,
        request: OpenSystemWatchRequest,
        channel: Channel<DesktopSystemEvent>,
    ) -> Result<OpenSystemWatchResponse, UiSafeError> {
        request.validate()?;
        let correlation_id = request.correlation_id;
        let data_dir = self.inner.data_dir.clone().ok_or_else(|| {
            UiSafeError::new(
                "desktop_data_directory_unavailable",
                "desktop.data_directory_unavailable",
                false,
                true,
                &correlation_id,
            )
        })?;
        let metadata = installation::load_or_create(data_dir.clone())
            .await
            .map_err(|error| installation_error(error, &correlation_id))?;
        let subscription_id = uuid::Uuid::now_v7().to_string();
        send(
            &channel,
            DesktopSystemEvent::phase(&subscription_id, BridgePhase::DiscoveringNode, 0),
            &correlation_id,
        )?;
        let mut client = connect_or_start(
            &metadata,
            &data_dir,
            &self.inner.node_starter,
            &self.inner.node_start_gate,
            &channel,
            &subscription_id,
            &correlation_id,
            0,
        )
        .await?;
        send(
            &channel,
            DesktopSystemEvent::phase(&subscription_id, BridgePhase::Subscribing, 0),
            &correlation_id,
        )?;
        let watch = client
            .watch()
            .await
            .map_err(|error| UiSafeError::from_client(&error, &correlation_id))?;
        let mut watch = watch;
        // Consume the watch's authoritative initial snapshot before returning.
        // Sending it through the Channel as well could let a newer snapshot race
        // ahead of the older command response and then be overwritten by it.
        let snapshot = take_initial_snapshot(&mut watch, &subscription_id, &correlation_id).await?;
        send(
            &channel,
            DesktopSystemEvent::phase(&subscription_id, BridgePhase::Watching, 0),
            &correlation_id,
        )?;

        let (cancel, cancel_rx) = watch::channel(false);
        self.replace_subscription(
            window_label.clone(),
            ActiveSubscription {
                subscription_id: subscription_id.clone(),
                cancel,
            },
        );
        let bridge = self.clone();
        let task_subscription_id = subscription_id.clone();
        let task_correlation_id = correlation_id.clone();
        let node_starter = self.inner.node_starter.clone();
        let node_start_gate = self.inner.node_start_gate.clone();
        tauri::async_runtime::spawn(async move {
            supervise_watch(
                watch,
                WatchContext {
                    metadata,
                    data_dir,
                    node_starter,
                    node_start_gate,
                    channel,
                    cancel: cancel_rx,
                    subscription_id: task_subscription_id.clone(),
                    correlation_id: task_correlation_id,
                },
            )
            .await;
            bridge.remove_subscription(&window_label, &task_subscription_id);
        });

        tracing::info!(
            phase = "desktop_watch_opened",
            correlation_id,
            subscription_id,
            "desktop bridge subscribed to authenticated Node watch"
        );
        Ok(OpenSystemWatchResponse {
            correlation_id,
            subscription_id,
            snapshot,
        })
    }

    pub fn close_system_watch(
        &self,
        window_label: &str,
        request: &CloseSystemWatchRequest,
    ) -> bool {
        let active = {
            let mut subscriptions = self
                .inner
                .subscriptions
                .lock()
                .expect("desktop subscription registry poisoned");
            if subscriptions
                .get(window_label)
                .is_some_and(|active| active.subscription_id == request.subscription_id)
            {
                subscriptions.remove(window_label)
            } else {
                None
            }
        };
        let Some(active) = active else {
            return false;
        };
        let _ = active.cancel.send(true);
        tracing::info!(
            phase = "desktop_watch_closed",
            subscription_id = request.subscription_id,
            "desktop watch closed without stopping Node"
        );
        true
    }

    fn replace_subscription(&self, window_label: String, next: ActiveSubscription) {
        let previous = self
            .inner
            .subscriptions
            .lock()
            .expect("desktop subscription registry poisoned")
            .insert(window_label, next);
        if let Some(previous) = previous {
            let _ = previous.cancel.send(true);
        }
    }

    fn remove_subscription(&self, window_label: &str, subscription_id: &str) {
        let mut subscriptions = self
            .inner
            .subscriptions
            .lock()
            .expect("desktop subscription registry poisoned");
        if subscriptions
            .get(window_label)
            .is_some_and(|active| active.subscription_id == subscription_id)
        {
            subscriptions.remove(window_label);
        }
    }
}

async fn connect_or_start(
    metadata: &InstallationMetadata,
    data_dir: &Path,
    node_starter: &Arc<dyn NodeStarter>,
    node_start_gate: &tokio::sync::Mutex<()>,
    channel: &Channel<DesktopSystemEvent>,
    subscription_id: &str,
    correlation_id: &str,
    attempt: u32,
) -> Result<AuthenticatedSystemClient, UiSafeError> {
    send(
        channel,
        DesktopSystemEvent::phase(subscription_id, BridgePhase::Handshaking, attempt),
        correlation_id,
    )?;
    match connect(metadata).await {
        Ok(client) => Ok(client),
        Err(error) if error.node_start_candidate() => {
            send(
                channel,
                DesktopSystemEvent::phase(subscription_id, BridgePhase::StartingNode, attempt),
                correlation_id,
            )?;
            let _start_guard = node_start_gate.lock().await;
            if let Ok(client) = connect(metadata).await {
                return Ok(client);
            }
            node_starter
                .start(metadata.clone(), data_dir.to_path_buf())
                .await
                .map_err(|error| node_start_error(error, correlation_id))?;
            send(
                channel,
                DesktopSystemEvent::phase(subscription_id, BridgePhase::Handshaking, attempt),
                correlation_id,
            )?;
            wait_for_node(metadata)
                .await
                .map_err(|error| UiSafeError::from_client(&error, correlation_id))
        }
        Err(error) => Err(UiSafeError::from_client(&error, correlation_id)),
    }
}

async fn wait_for_node(
    metadata: &InstallationMetadata,
) -> Result<AuthenticatedSystemClient, ClientError> {
    let deadline = tokio::time::Instant::now() + NODE_READY_DEADLINE;
    loop {
        match connect(metadata).await {
            Ok(client) => return Ok(client),
            Err(error)
                if error.node_start_candidate() && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(NODE_READY_RETRY_DELAY).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn connect(
    metadata: &InstallationMetadata,
) -> Result<AuthenticatedSystemClient, ClientError> {
    AuthenticatedSystemClient::connect(ClientConfig::m01(
        &metadata.installation_id,
        &metadata.device_id,
        COMPONENT_VERSION,
    ))
    .await
}

async fn take_initial_snapshot(
    watch: &mut AuthenticatedSystemWatch,
    subscription_id: &str,
    correlation_id: &str,
) -> Result<DesktopSystemSnapshot, UiSafeError> {
    let response = tokio::time::timeout(INITIAL_WATCH_SNAPSHOT_DEADLINE, watch.message())
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
    let frame = response.frame.as_ref().ok_or_else(|| {
        UiSafeError::new(
            "ipc_watch_frame_missing",
            "desktop.ipc_watch_frame_missing",
            true,
            false,
            correlation_id,
        )
    })?;
    match frame_to_event(subscription_id, frame)? {
        DesktopSystemEvent::Snapshot { snapshot, .. } => Ok(snapshot),
        DesktopSystemEvent::Error { error, .. } => Err(error),
        _ => Err(UiSafeError::new(
            "ipc_watch_first_frame_not_snapshot",
            "desktop.ipc_watch_first_frame_not_snapshot",
            true,
            false,
            correlation_id,
        )),
    }
}

struct WatchContext {
    metadata: InstallationMetadata,
    data_dir: PathBuf,
    node_starter: Arc<dyn NodeStarter>,
    node_start_gate: Arc<tokio::sync::Mutex<()>>,
    channel: Channel<DesktopSystemEvent>,
    cancel: watch::Receiver<bool>,
    subscription_id: String,
    correlation_id: String,
}

async fn supervise_watch(mut stream: AuthenticatedSystemWatch, context: WatchContext) {
    let WatchContext {
        metadata,
        data_dir,
        node_starter,
        node_start_gate,
        channel,
        mut cancel,
        subscription_id,
        correlation_id,
    } = context;
    let mut reconnect_attempt = 0_u32;
    loop {
        let next = tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return;
                }
                continue;
            }
            next = stream.message() => next,
        };

        let reconnect = match next {
            Ok(Some(response)) => match response.frame.as_ref() {
                Some(frame) => match frame_to_event(&subscription_id, frame) {
                    Ok(event) => {
                        let healthy_frame = matches!(
                            event,
                            DesktopSystemEvent::Snapshot { .. }
                                | DesktopSystemEvent::Delta { .. }
                                | DesktopSystemEvent::Heartbeat { .. }
                        );
                        let requires_resync = matches!(
                            event,
                            DesktopSystemEvent::ResyncRequired { .. }
                                | DesktopSystemEvent::Error {
                                    error: UiSafeError {
                                        retryable: true,
                                        ..
                                    },
                                    ..
                                }
                        );
                        let terminal_error = matches!(
                            event,
                            DesktopSystemEvent::Error {
                                error: UiSafeError {
                                    retryable: false,
                                    ..
                                },
                                ..
                            }
                        );
                        if channel.send(event).is_err() {
                            return;
                        }
                        if terminal_error {
                            return;
                        }
                        if healthy_frame {
                            reconnect_attempt = 0;
                        }
                        requires_resync
                    }
                    Err(error) => {
                        if channel
                            .send(DesktopSystemEvent::error(&subscription_id, error))
                            .is_err()
                        {
                            return;
                        }
                        true
                    }
                },
                None => true,
            },
            Ok(None) => {
                if channel
                    .send(DesktopSystemEvent::error(
                        &subscription_id,
                        UiSafeError::new(
                            "ipc_watch_closed",
                            "desktop.ipc_watch_closed",
                            true,
                            false,
                            &correlation_id,
                        ),
                    ))
                    .is_err()
                {
                    return;
                }
                true
            }
            Err(error) => {
                let retryable = error.retryable();
                if channel
                    .send(DesktopSystemEvent::error(
                        &subscription_id,
                        UiSafeError::from_client(&error, &correlation_id),
                    ))
                    .is_err()
                {
                    return;
                }
                if !retryable {
                    return;
                }
                retryable
            }
        };
        if !reconnect {
            continue;
        }

        reconnect_attempt = reconnect_attempt.saturating_add(1);
        if channel
            .send(DesktopSystemEvent::phase(
                &subscription_id,
                BridgePhase::Reconnecting,
                reconnect_attempt,
            ))
            .is_err()
        {
            return;
        }
        let delay = reconnect_delay(reconnect_attempt);
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return;
                }
            }
            () = tokio::time::sleep(delay) => {}
        }

        let Some(reopened) = reconnect_or_cancel(
            &mut cancel,
            reconnect_watch(
                &metadata,
                &data_dir,
                &node_starter,
                &node_start_gate,
                &channel,
                &subscription_id,
                &correlation_id,
                reconnect_attempt,
            ),
        )
        .await
        else {
            return;
        };
        match reopened {
            Ok(next_stream) => {
                stream = next_stream;
                if channel
                    .send(DesktopSystemEvent::phase(
                        &subscription_id,
                        BridgePhase::Watching,
                        0,
                    ))
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                let retryable = error.retryable;
                if channel
                    .send(DesktopSystemEvent::error(&subscription_id, error))
                    .is_err()
                {
                    return;
                }
                if !retryable {
                    return;
                }
            }
        }
    }
}

async fn reconnect_or_cancel<F>(
    cancel: &mut watch::Receiver<bool>,
    reconnect: F,
) -> Option<F::Output>
where
    F: Future,
{
    tokio::pin!(reconnect);
    loop {
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return None;
                }
            }
            result = &mut reconnect => return Some(result),
        }
    }
}

async fn reconnect_watch(
    metadata: &InstallationMetadata,
    data_dir: &Path,
    node_starter: &Arc<dyn NodeStarter>,
    node_start_gate: &Arc<tokio::sync::Mutex<()>>,
    channel: &Channel<DesktopSystemEvent>,
    subscription_id: &str,
    correlation_id: &str,
    attempt: u32,
) -> Result<AuthenticatedSystemWatch, UiSafeError> {
    let mut client = connect_or_start(
        metadata,
        data_dir,
        node_starter,
        node_start_gate,
        channel,
        subscription_id,
        correlation_id,
        attempt,
    )
    .await?;
    client
        .watch()
        .await
        .map_err(|error| UiSafeError::from_client(&error, correlation_id))
}

type NodeStartFuture = Pin<Box<dyn Future<Output = Result<(), NodeStartError>> + Send>>;

trait NodeStarter: Send + Sync {
    fn start(&self, metadata: InstallationMetadata, data_dir: PathBuf) -> NodeStartFuture;
}

struct OsNodeStarter;

impl NodeStarter for OsNodeStarter {
    fn start(&self, metadata: InstallationMetadata, data_dir: PathBuf) -> NodeStartFuture {
        Box::pin(node_process::start(metadata, data_dir))
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_millis(250_u64.saturating_mul(1_u64 << exponent)).min(MAX_RECONNECT_DELAY)
}

fn send(
    channel: &Channel<DesktopSystemEvent>,
    event: DesktopSystemEvent,
    correlation_id: &str,
) -> Result<(), UiSafeError> {
    channel.send(event).map_err(|_| {
        UiSafeError::new(
            "desktop_channel_closed",
            "desktop.channel_closed",
            true,
            false,
            correlation_id,
        )
    })
}

fn installation_error(error: InstallationError, correlation_id: &str) -> UiSafeError {
    match error {
        InstallationError::InvalidMetadata => UiSafeError::new(
            "installation_metadata_invalid",
            "desktop.installation_metadata_invalid",
            false,
            true,
            correlation_id,
        ),
        InstallationError::NotFound | InstallationError::StorageUnavailable => UiSafeError::new(
            "installation_storage_unavailable",
            "desktop.installation_storage_unavailable",
            true,
            true,
            correlation_id,
        ),
    }
}

fn node_start_error(error: NodeStartError, correlation_id: &str) -> UiSafeError {
    match error {
        NodeStartError::ExecutableMissing => UiSafeError::new(
            "node_executable_missing",
            "desktop.node_executable_missing",
            false,
            true,
            correlation_id,
        ),
        NodeStartError::StartFailed => UiSafeError::new(
            "node_start_failed",
            "desktop.node_start_failed",
            true,
            true,
            correlation_id,
        ),
        #[cfg(not(windows))]
        NodeStartError::UnsupportedPlatform => UiSafeError::new(
            "ipc_platform_unsupported",
            "desktop.ipc_platform_unsupported",
            false,
            true,
            correlation_id,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    #[cfg(windows)]
    use tauri::ipc::InvokeResponseBody;
    #[cfg(windows)]
    use tokio::sync::{mpsc, oneshot};

    #[cfg(windows)]
    type InProcessNodeHandle = (
        oneshot::Sender<()>,
        tokio::task::JoinHandle<Result<(), dennett_node::NodeRunError>>,
    );

    #[cfg(windows)]
    #[derive(Clone, Default)]
    struct InProcessNodeStarter {
        starts: Arc<AtomicUsize>,
        node: Arc<Mutex<Option<InProcessNodeHandle>>>,
    }

    #[cfg(windows)]
    struct PendingFutureGuard(Arc<AtomicBool>);

    #[cfg(windows)]
    impl Drop for PendingFutureGuard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[cfg(windows)]
    impl NodeStarter for InProcessNodeStarter {
        fn start(&self, metadata: InstallationMetadata, _data_dir: PathBuf) -> NodeStartFuture {
            let starts = self.starts.clone();
            let node = self.node.clone();
            Box::pin(async move {
                starts.fetch_add(1, Ordering::SeqCst);
                let config = dennett_node::NodeConfig::new(
                    &metadata.installation_id,
                    metadata.authority_epoch,
                    "bridge-test-node",
                )
                .map_err(|_| NodeStartError::StartFailed)?;
                let (shutdown_tx, shutdown_rx) = oneshot::channel();
                let task = tokio::spawn(dennett_node::run(config, async move {
                    let _ = shutdown_rx.await;
                }));
                node.lock()
                    .expect("test Node registry poisoned")
                    .replace((shutdown_tx, task));
                Ok(())
            })
        }
    }

    #[cfg(windows)]
    impl InProcessNodeStarter {
        async fn shutdown(&self) {
            let Some((shutdown, task)) = self
                .node
                .lock()
                .expect("test Node registry poisoned")
                .take()
            else {
                return;
            };
            shutdown.send(()).expect("shutdown Node");
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .expect("Node shutdown timeout")
                .expect("Node task")
                .expect("Node result");
        }
    }

    #[test]
    fn reconnect_backoff_is_bounded() {
        assert_eq!(reconnect_delay(1), Duration::from_millis(250));
        assert_eq!(reconnect_delay(2), Duration::from_millis(500));
        assert_eq!(reconnect_delay(6), MAX_RECONNECT_DELAY);
        assert_eq!(reconnect_delay(u32::MAX), MAX_RECONNECT_DELAY);
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tauri_channel_gets_authenticated_snapshot_and_close_leaves_node_alive() {
        let directory = tempfile::tempdir().expect("tempdir");
        let metadata = installation::load_or_create(directory.path().to_owned())
            .await
            .expect("installation metadata");
        let node_starter = Arc::new(InProcessNodeStarter::default());
        let bridge = DesktopBridge::with_node_starter(
            Some(directory.path().to_owned()),
            node_starter.clone(),
        );
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let channel = Channel::new(move |body| {
            let _ = event_tx.send(body);
            Ok(())
        });
        let opened = bridge
            .open_system_watch(
                "main".to_owned(),
                OpenSystemWatchRequest {
                    correlation_id: "bridge-test-1".to_owned(),
                },
                channel,
            )
            .await
            .expect("open system watch");
        let initial_revision = opened.snapshot.revision.clone();
        assert!(
            initial_revision
                .parse::<u64>()
                .is_ok_and(|revision| revision > 0),
            "the initial snapshot must carry an authoritative revision"
        );
        assert_eq!(
            opened.snapshot.authority_epoch,
            metadata.authority_epoch.to_string()
        );

        let mut saw_watching = false;
        while let Ok(body) = event_rx.try_recv() {
            let InvokeResponseBody::Json(json) = body else {
                panic!("expected JSON channel event");
            };
            assert!(!json.contains(&metadata.installation_id));
            assert!(!json.contains(&metadata.device_id));
            assert!(!json.contains("session_proof"));
            let event: DesktopSystemEvent = serde_json::from_str(&json).expect("event JSON");
            assert!(
                !matches!(event, DesktopSystemEvent::Snapshot { .. }),
                "the initial snapshot must have one ordered delivery path"
            );
            saw_watching |= matches!(
                event,
                DesktopSystemEvent::Phase {
                    phase: BridgePhase::Watching,
                    ..
                }
            );
        }
        assert!(saw_watching);

        assert!(bridge.close_system_watch(
            "main",
            &CloseSystemWatchRequest {
                subscription_id: opened.subscription_id,
            },
        ));
        let reconnected = bridge
            .open_system_watch(
                "main".to_owned(),
                OpenSystemWatchRequest {
                    correlation_id: "bridge-test-2".to_owned(),
                },
                Channel::new(|_| Ok(())),
            )
            .await
            .expect("Desktop reconnects while Node stays alive");
        assert_eq!(reconnected.snapshot.revision, initial_revision);
        assert!(bridge.close_system_watch(
            "main",
            &CloseSystemWatchRequest {
                subscription_id: reconnected.subscription_id,
            },
        ));
        assert_eq!(node_starter.starts.load(Ordering::SeqCst), 1);
        node_starter.shutdown().await;
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_windows_share_one_node_start() {
        let directory = tempfile::tempdir().expect("tempdir");
        let node_starter = Arc::new(InProcessNodeStarter::default());
        let bridge = DesktopBridge::with_node_starter(
            Some(directory.path().to_owned()),
            node_starter.clone(),
        );
        let first = bridge.open_system_watch(
            "window-one".to_owned(),
            OpenSystemWatchRequest {
                correlation_id: "parallel-one".to_owned(),
            },
            Channel::new(|_| Ok(())),
        );
        let second = bridge.open_system_watch(
            "window-two".to_owned(),
            OpenSystemWatchRequest {
                correlation_id: "parallel-two".to_owned(),
            },
            Channel::new(|_| Ok(())),
        );
        let (first, second) = tokio::join!(first, second);
        let first = first.expect("first watch");
        let second = second.expect("second watch");
        assert_eq!(node_starter.starts.load(Ordering::SeqCst), 1);
        assert!(bridge.close_system_watch(
            "window-one",
            &CloseSystemWatchRequest {
                subscription_id: first.subscription_id,
            },
        ));
        assert!(bridge.close_system_watch(
            "window-two",
            &CloseSystemWatchRequest {
                subscription_id: second.subscription_id,
            },
        ));
        node_starter.shutdown().await;
    }

    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_watch_cancels_the_entire_in_flight_reconnect() {
        let started = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (close, mut close_rx) = watch::channel(false);
        let future_started = started.clone();
        let future_cancelled = cancelled.clone();
        let reconnect = async move {
            future_started.store(true, Ordering::SeqCst);
            let _guard = PendingFutureGuard(future_cancelled);
            std::future::pending::<()>().await;
        };
        let task = tokio::spawn(async move { reconnect_or_cancel(&mut close_rx, reconnect).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending reconnect did not start");

        close.send(true).expect("close reconnect");
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("closing the watch must cancel a hung reconnect promptly")
            .expect("reconnect task");
        assert!(result.is_none());
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
