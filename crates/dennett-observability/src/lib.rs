//! Privacy-safe process diagnostics for local Dennett profiles.
//!
//! Personal Quiet persistence accepts only the fixed [`DiagnosticEvent`]
//! schema. Correlation uses UUID values and a bounded provider category rather
//! than arbitrary strings, so prompts, responses, paths and credentials cannot
//! be smuggled into an identifier field.

mod lifecycle;
mod secure_fs;
mod writer;

use lifecycle::{LifecycleProgress, LifecycleSession};
use secure_fs::SecureDir;
use std::{
    path::PathBuf,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{Layer, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use writer::{PreparedWriter, WriterHealth, prepare_writer};

pub use lifecycle::{
    ActiveRunSummary, DiagnosticExit, DiagnosticFlushStatus, DiagnosticStorageStatus,
    DiagnosticSummary, ExitStatus, MarkerState, inspect_local,
};

const DIAGNOSTIC_TARGET: &str = "dennett.private_safe_diagnostic";
const DIAGNOSTIC_MODULE_PATH: &str = module_path!();
const DEFAULT_MAX_LOG_FILES: usize = 14;
const DEFAULT_MAX_LIFECYCLE_RECORDS: usize = 64;
const DEFAULT_MAX_LOG_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
const DEFAULT_MAX_LOG_BYTES: u64 = 32 * 1024 * 1024;
const DIAGNOSTIC_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const CHECKPOINT_IDLE_POLL: Duration = Duration::from_millis(250);
static PROCESS_CONTEXT: OnceLock<ProcessContext> = OnceLock::new();

struct ProcessContext {
    component: String,
    run_id: String,
    checkpoint_publisher: CheckpointPublisher,
    writer_health: WriterHealth,
}

struct CheckpointState {
    enabled: AtomicBool,
    phase: AtomicU8,
    dropped_records: AtomicUsize,
    failures: AtomicUsize,
}

#[derive(Clone)]
pub(crate) struct CheckpointPublisher {
    state: Arc<CheckpointState>,
    wake: Option<SyncSender<()>>,
}

impl CheckpointPublisher {
    fn start(progress: LifecycleProgress) -> Result<CheckpointWorker, DiagnosticsError> {
        let state = Arc::new(CheckpointState {
            enabled: AtomicBool::new(true),
            phase: AtomicU8::new(phase_id("startup")),
            dropped_records: AtomicUsize::new(0),
            failures: AtomicUsize::new(0),
        });
        let (wake, receiver) = mpsc::sync_channel(1);
        let publisher = Self {
            state: Arc::clone(&state),
            wake: Some(wake),
        };
        let handle = thread::Builder::new()
            .name("dennett-diagnostic-checkpoint".to_owned())
            .spawn(move || run_checkpoint_worker(progress, state, receiver))
            .map_err(|source| DiagnosticsError::io("spawn_checkpoint_worker", source))?;
        Ok(CheckpointWorker {
            publisher,
            handle: Some(handle),
        })
    }

    fn publish(&self, phase: &'static str, dropped_records: usize) {
        if !self.state.enabled.load(Ordering::Acquire) {
            return;
        }
        self.state.phase.store(phase_id(phase), Ordering::Release);
        self.state
            .dropped_records
            .fetch_max(dropped_records, Ordering::AcqRel);
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }

    pub(crate) fn publish_dropped(&self, dropped_records: usize) {
        if !self.state.enabled.load(Ordering::Acquire) {
            return;
        }
        self.state
            .dropped_records
            .fetch_max(dropped_records, Ordering::AcqRel);
        if let Some(wake) = &self.wake {
            let _ = wake.try_send(());
        }
    }

    fn failures(&self) -> usize {
        self.state.failures.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn disabled_for_test() -> Self {
        Self {
            state: Arc::new(CheckpointState {
                enabled: AtomicBool::new(false),
                phase: AtomicU8::new(phase_id("test")),
                dropped_records: AtomicUsize::new(0),
                failures: AtomicUsize::new(0),
            }),
            wake: None,
        }
    }
}

struct CheckpointWorker {
    publisher: CheckpointPublisher,
    handle: Option<JoinHandle<()>>,
}

impl CheckpointWorker {
    fn publisher(&self) -> CheckpointPublisher {
        self.publisher.clone()
    }

    fn stop_and_join(mut self) -> usize {
        self.publisher.state.enabled.store(false, Ordering::Release);
        if let Some(wake) = &self.publisher.wake {
            let _ = wake.try_send(());
        }
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            self.publisher.state.failures.fetch_add(1, Ordering::AcqRel);
        }
        self.publisher.failures()
    }
}

impl Drop for CheckpointWorker {
    fn drop(&mut self) {
        self.publisher.state.enabled.store(false, Ordering::Release);
        if let Some(wake) = &self.publisher.wake {
            let _ = wake.try_send(());
        }
    }
}

fn run_checkpoint_worker(
    progress: LifecycleProgress,
    state: Arc<CheckpointState>,
    receiver: Receiver<()>,
) {
    loop {
        let wake = receiver.recv_timeout(CHECKPOINT_IDLE_POLL);
        if !state.enabled.load(Ordering::Acquire) {
            break;
        }
        if wake.is_err() && !matches!(wake, Err(mpsc::RecvTimeoutError::Timeout)) {
            break;
        }
        while !matches!(
            receiver.try_recv(),
            Err(TryRecvError::Empty | TryRecvError::Disconnected)
        ) {}
        let phase = phase_name(state.phase.load(Ordering::Acquire));
        let dropped = state.dropped_records.load(Ordering::Acquire);
        if progress.checkpoint(phase, dropped).is_err() {
            state.failures.fetch_add(1, Ordering::AcqRel);
        }
    }
}

const fn phase_id(phase: &str) -> u8 {
    match phase.as_bytes() {
        b"startup" => 1,
        b"shutdown" => 2,
        b"test" => 3,
        b"runtime" => 4,
        b"runtime_startup" => 5,
        b"local_ipc" => 6,
        b"runtime_control" => 7,
        b"runtime_recovery" => 8,
        b"runtime_host" => 9,
        b"conversation" => 10,
        _ => 0,
    }
}

const fn phase_name(id: u8) -> &'static str {
    match id {
        1 => "startup",
        2 => "shutdown",
        3 => "test",
        4 => "runtime",
        5 => "runtime_startup",
        6 => "local_ipc",
        7 => "runtime_control",
        8 => "runtime_recovery",
        9 => "runtime_host",
        10 => "conversation",
        _ => "unknown",
    }
}

/// Typed configuration for the local Personal Quiet diagnostic profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalDiagnosticsConfig {
    pub component: String,
    pub data_dir: PathBuf,
    pub max_log_files: usize,
    pub max_lifecycle_records: usize,
    pub max_log_age: Duration,
    pub max_log_bytes: u64,
}

impl LocalDiagnosticsConfig {
    #[must_use]
    pub fn personal_quiet(component: impl Into<String>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            component: component.into(),
            data_dir: data_dir.into(),
            max_log_files: DEFAULT_MAX_LOG_FILES,
            max_lifecycle_records: DEFAULT_MAX_LIFECYCLE_RECORDS,
            max_log_age: DEFAULT_MAX_LOG_AGE,
            max_log_bytes: DEFAULT_MAX_LOG_BYTES,
        }
    }

    pub fn validate(&self) -> Result<(), DiagnosticsError> {
        if !valid_component(&self.component) {
            return Err(DiagnosticsError::InvalidComponent);
        }
        if self.max_log_files == 0
            || self.max_lifecycle_records == 0
            || self.max_log_age.is_zero()
            || self.max_log_bytes == 0
        {
            return Err(DiagnosticsError::InvalidRetention);
        }
        Ok(())
    }
}

/// Keeps the writer and active-run lock alive for the process lifetime.
///
/// Call [`LocalDiagnostics::shutdown`] on every handled exit. Dropping this
/// value without shutdown intentionally leaves the readable marker behind, so
/// the next process can report an unclean previous exit.
#[must_use]
pub struct LocalDiagnostics {
    component: String,
    data_dir: PathBuf,
    lifecycle: Option<LifecycleSession>,
    writer_health: WriterHealth,
    worker_guard: Option<WorkerGuard>,
    checkpoint_worker: Option<CheckpointWorker>,
}

impl LocalDiagnostics {
    pub fn shutdown(mut self, exit: DiagnosticExit) -> Result<(), DiagnosticsError> {
        let (status, error_code) = match exit {
            DiagnosticExit::Clean => ("clean", ""),
            DiagnosticExit::Failed { error_code } => ("failed", error_code),
        };
        record(
            DiagnosticEvent::new(DiagnosticEventKind::DiagnosticsProcessExit)
                .status(status)
                .error_code(error_code),
        );
        let worker_guard = self
            .worker_guard
            .take()
            .expect("diagnostic writer guard is present until shutdown");
        let checkpoint_worker = self
            .checkpoint_worker
            .take()
            .expect("checkpoint worker is present until shutdown");
        let lifecycle = self
            .lifecycle
            .take()
            .expect("local diagnostics lifecycle is present until shutdown");
        let health = self.writer_health.clone();
        let (completed, result) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("dennett-diagnostic-shutdown".to_owned())
            .spawn(move || {
                let checkpoint_failures = checkpoint_worker.stop_and_join();
                drop(worker_guard);
                let dropped_log_records = health.dropped_records();
                let flush_confirmed = health.flush_confirmed() && checkpoint_failures == 0;
                let outcome = lifecycle.complete(exit, dropped_log_records, flush_confirmed, true);
                let _ = completed.send(outcome);
            })
            .map_err(|source| DiagnosticsError::io("spawn_diagnostic_shutdown", source))?;
        result
            .recv_timeout(DIAGNOSTIC_SHUTDOWN_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => DiagnosticsError::ShutdownTimeout,
                mpsc::RecvTimeoutError::Disconnected => DiagnosticsError::ShutdownWorkerFailed,
            })?
    }

    pub fn summary(&self) -> Result<DiagnosticSummary, DiagnosticsError> {
        let dropped = self.writer_health.dropped_records();
        let lifecycle = self
            .lifecycle
            .as_ref()
            .expect("local diagnostics lifecycle is present until shutdown");
        self.checkpoint_worker
            .as_ref()
            .expect("checkpoint worker is present until shutdown")
            .publisher
            .publish_dropped(dropped);
        let run_id = lifecycle.run_id();
        let mut summary = inspect_local(&self.data_dir, &self.component)?;
        if let Some(active) = summary.active_runs.iter().find(|run| run.run_id == run_id) {
            summary.dropped_log_records = summary
                .dropped_log_records
                .saturating_sub(active.dropped_log_records)
                .saturating_add(dropped);
        } else {
            summary.dropped_log_records = summary.dropped_log_records.saturating_add(dropped);
        }
        Ok(summary)
    }
}

/// Initializes bounded JSONL diagnostics plus a metadata-only console layer.
///
/// Initialization is transactional with respect to the lifecycle marker: a
/// failure after marker creation removes only the marker owned by this attempt.
pub fn init_local(config: LocalDiagnosticsConfig) -> Result<LocalDiagnostics, DiagnosticsError> {
    config.validate()?;
    let profile_dir = SecureDir::open_or_create_profile(&config.data_dir)?;
    let diagnostics_dir =
        profile_dir.open_or_create_child("diagnostics", "create_diagnostics_directory")?;
    let lifecycle = LifecycleSession::start(
        &diagnostics_dir,
        &config.component,
        config.max_lifecycle_records,
    )?;
    let checkpoint_worker = match CheckpointPublisher::start(lifecycle.progress()) {
        Ok(worker) => worker,
        Err(error) => {
            lifecycle.cancel_startup()?;
            return Err(error);
        }
    };
    let checkpoint_publisher = checkpoint_worker.publisher();
    let PreparedWriter {
        writer,
        guard,
        health,
    } = match prepare_writer(
        &diagnostics_dir,
        &config.component,
        config.max_log_files,
        config.max_log_age,
        config.max_log_bytes,
        checkpoint_publisher.clone(),
    ) {
        Ok(writer) => writer,
        Err(error) => {
            checkpoint_worker.stop_and_join();
            lifecycle.cancel_startup()?;
            return Err(error);
        }
    };
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_filter(filter_fn(is_registered_diagnostic));
    let diagnostic_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .with_writer(writer)
        .with_filter(filter_fn(is_registered_diagnostic));
    if tracing_subscriber::registry()
        .with(console_layer)
        .with(diagnostic_layer)
        .try_init()
        .is_err()
    {
        checkpoint_worker.stop_and_join();
        drop(guard);
        lifecycle.cancel_startup()?;
        return Err(DiagnosticsError::SubscriberInitialization);
    }
    let context = ProcessContext {
        component: config.component.clone(),
        run_id: lifecycle.run_id().to_owned(),
        checkpoint_publisher,
        writer_health: health.clone(),
    };
    if PROCESS_CONTEXT.set(context).is_err() {
        checkpoint_worker.stop_and_join();
        drop(guard);
        lifecycle.cancel_startup()?;
        return Err(DiagnosticsError::SubscriberInitialization);
    }

    let previous_status = lifecycle.previous_status().as_str();
    record(
        DiagnosticEvent::new(DiagnosticEventKind::DiagnosticsInitialized).status(previous_status),
    );
    Ok(LocalDiagnostics {
        component: config.component,
        data_dir: config.data_dir,
        lifecycle: Some(lifecycle),
        writer_health: health,
        worker_guard: Some(guard),
        checkpoint_worker: Some(checkpoint_worker),
    })
}

/// Console-only bootstrap for components without a durable diagnostic profile.
pub fn init(service_name: &str) {
    let _ = service_name;
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_filter(filter_fn(is_registered_diagnostic));
    let _ = tracing_subscriber::registry()
        .with(console_layer)
        .try_init();
}

/// Returns the platform's per-user Dennett data root.
///
/// Explicit `DENNETT_DATA_DIR` values remain the responsibility of the caller;
/// this helper is only the safe default when no override was configured.
#[must_use]
pub fn default_user_data_root() -> PathBuf {
    platform_user_data_home()
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".dennett-local-data")
        })
        .join("Dennett")
        .join("data")
}

#[cfg(windows)]
fn platform_user_data_home() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join("AppData").join("Local"))
        })
}

#[cfg(target_os = "macos")]
fn platform_user_data_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library").join("Application Support"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn platform_user_data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local").join("share"))
        })
}

#[cfg(not(any(unix, windows)))]
fn platform_user_data_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A provider category safe to persist. Unknown adapter identifiers collapse
/// to `other`; the original provider-supplied text is never logged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticProvider {
    Codex,
    Fake,
    TestFixture,
    Other,
}

impl DiagnosticProvider {
    #[must_use]
    pub fn from_adapter_id(adapter_id: &str) -> Self {
        match adapter_id {
            "openai.codex.sdk" => Self::Codex,
            "dennett.fake" => Self::Fake,
            value if value.starts_with("dennett.test.") || value.starts_with("fixture.") => {
                Self::TestFixture
            }
            _ => Self::Other,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "openai.codex.sdk",
            Self::Fake => "dennett.fake",
            Self::TestFixture => "test_fixture",
            Self::Other => "other",
        }
    }
}

/// Registered metadata-only event kinds.
///
/// The persisted code, phase, level and message are owned by this crate. A
/// caller can select a kind and attach typed identifiers, but cannot provide
/// text that may contain prompts, responses, paths or credentials.
#[derive(Clone, Copy, Debug)]
pub enum DiagnosticEventKind {
    DiagnosticsProcessExit,
    DiagnosticsInitialized,
    DiagnosticsCapacityProbe,
    DiagnosticsTestCheckpoint,
    RuntimeProviderFailure,
    NodeConfigurationValidated,
    NodeInstanceLockAcquired,
    NodeRuntimeReady,
    NodeIpcStartRequested,
    NodeIpcStopped,
    NodeConfigurationFailed,
    NodeShutdownSignalFailed,
    NodeRunFailed,
    RuntimeHostSpawnFailed,
    RuntimeHostHandshakeFailed,
    RuntimeHostProtocolMismatch,
    RuntimeHostReady,
    RuntimeHostWriteFailed,
    RuntimeHostResponseChannelClosed,
    RuntimeHostControlTimeout,
    RuntimeHostFenced,
    RuntimeHostFrameTooLarge,
    RuntimeHostReadFailed,
    RuntimeHostInvalidUtf8,
    RuntimeHostStdoutEof,
    RuntimeHostStderrFrameTooLarge,
    RuntimeHostStderrReadFailed,
    RuntimeHostUnhandledFailure,
    RuntimeHostStderrUnclassified,
    RuntimeHostInvalidJson,
    RuntimeHostProtocolVersionInvalid,
    RuntimeHostUnknownResponse,
    RuntimeHostInvalidResponse,
    RuntimeHostInvalidEvent,
    RuntimeHostInvalidScope,
    RuntimeHostInvalidError,
    RuntimeHostUnknownEvent,
    HeadTurnAccepted,
    HeadTurnTerminalCommitted,
    HeadDemoStarted,
    HeadDemoCompleted,
}

impl DiagnosticEventKind {
    const fn spec(self) -> DiagnosticEventSpec {
        use DiagnosticEventKind as Kind;
        match self {
            Kind::DiagnosticsProcessExit => DiagnosticEventSpec::info(
                "diagnostics.process_exit",
                "shutdown",
                "process diagnostic session reached a handled exit",
            ),
            Kind::DiagnosticsInitialized => DiagnosticEventSpec::info(
                "diagnostics.initialized",
                "startup",
                "privacy-safe local diagnostics initialized",
            ),
            Kind::DiagnosticsCapacityProbe => DiagnosticEventSpec::info(
                "diagnostics.capacity_probe",
                "test",
                "bounded writer capacity probe",
            ),
            Kind::DiagnosticsTestCheckpoint => DiagnosticEventSpec::info(
                "diagnostics.test_checkpoint",
                "test",
                "diagnostic persistence checkpoint",
            ),
            Kind::RuntimeProviderFailure => DiagnosticEventSpec::error(
                "runtime.provider_failure",
                "runtime",
                "provider operation failed",
            ),
            Kind::NodeConfigurationValidated => DiagnosticEventSpec::info(
                "node.configuration_validated",
                "startup",
                "Node configuration passed validation",
            ),
            Kind::NodeInstanceLockAcquired => DiagnosticEventSpec::info(
                "node.instance_lock_acquired",
                "startup",
                "Node owns the local installation lock",
            ),
            Kind::NodeRuntimeReady => DiagnosticEventSpec::info(
                "node.runtime_ready",
                "runtime_startup",
                "agent runtime descriptor is available",
            ),
            Kind::NodeIpcStartRequested => DiagnosticEventSpec::info(
                "node.ipc_start_requested",
                "local_ipc",
                "authenticated local IPC startup was requested",
            ),
            Kind::NodeIpcStopped => DiagnosticEventSpec::info(
                "node.ipc_stopped",
                "local_ipc",
                "local IPC stopped after an explicit shutdown",
            ),
            Kind::NodeConfigurationFailed => DiagnosticEventSpec::error(
                "node.configuration_failed",
                "startup",
                "Node configuration validation failed",
            ),
            Kind::NodeShutdownSignalFailed => DiagnosticEventSpec::error(
                "node.shutdown_signal_failed",
                "shutdown",
                "Node could not wait for the explicit shutdown signal",
            ),
            Kind::NodeRunFailed => DiagnosticEventSpec::error(
                "node.run_failed",
                "runtime",
                "Node stopped after a handled runtime failure",
            ),
            Kind::RuntimeHostSpawnFailed => DiagnosticEventSpec::error(
                "runtime.host_spawn_failed",
                "runtime_startup",
                "Node adapter host process could not be started",
            ),
            Kind::RuntimeHostHandshakeFailed => DiagnosticEventSpec::error(
                "runtime.host_handshake_failed",
                "runtime_startup",
                "Node adapter host did not complete its protocol handshake",
            ),
            Kind::RuntimeHostProtocolMismatch => DiagnosticEventSpec::error(
                "runtime.host_protocol_mismatch",
                "runtime_startup",
                "Node adapter host reported an incompatible health contract",
            ),
            Kind::RuntimeHostReady => DiagnosticEventSpec::info(
                "runtime.host_ready",
                "runtime_startup",
                "Node adapter host protocol handshake completed",
            ),
            Kind::RuntimeHostWriteFailed => DiagnosticEventSpec::error(
                "runtime.host_write_failed",
                "runtime_control",
                "runtime host control request could not be written",
            ),
            Kind::RuntimeHostResponseChannelClosed => DiagnosticEventSpec::error(
                "runtime.host_response_channel_closed",
                "runtime_control",
                "runtime host closed before returning a control result",
            ),
            Kind::RuntimeHostControlTimeout => DiagnosticEventSpec::error(
                "runtime.host_control_timeout",
                "runtime_control",
                "runtime host control request exceeded its deadline",
            ),
            Kind::RuntimeHostFenced => DiagnosticEventSpec::warning(
                "runtime.host_fenced",
                "runtime_recovery",
                "runtime host was fenced after communication became uncertain",
            ),
            Kind::RuntimeHostFrameTooLarge => DiagnosticEventSpec::error(
                "runtime.host_frame_too_large",
                "runtime_host",
                "runtime host exceeded the stdout protocol frame limit",
            ),
            Kind::RuntimeHostReadFailed => DiagnosticEventSpec::error(
                "runtime.host_read_failed",
                "runtime_host",
                "runtime host stdout could not be read",
            ),
            Kind::RuntimeHostInvalidUtf8 => DiagnosticEventSpec::error(
                "runtime.host_invalid_utf8",
                "runtime_host",
                "runtime host emitted non-UTF-8 protocol data",
            ),
            Kind::RuntimeHostStdoutEof => DiagnosticEventSpec::error(
                "runtime.host_stdout_eof",
                "runtime_host",
                "runtime host stdout closed unexpectedly",
            ),
            Kind::RuntimeHostStderrFrameTooLarge => DiagnosticEventSpec::error(
                "runtime.host_stderr_frame_too_large",
                "runtime_host",
                "runtime host exceeded the stderr diagnostic frame limit",
            ),
            Kind::RuntimeHostStderrReadFailed => DiagnosticEventSpec::error(
                "runtime.host_stderr_read_failed",
                "runtime_host",
                "runtime host stderr could not be read",
            ),
            Kind::RuntimeHostUnhandledFailure => DiagnosticEventSpec::error(
                "runtime.host_unhandled_failure",
                "runtime_host",
                "runtime host reported an unhandled internal failure",
            ),
            Kind::RuntimeHostStderrUnclassified => DiagnosticEventSpec::error(
                "runtime.host_stderr_unclassified",
                "runtime_host",
                "runtime host wrote unclassified private data to stderr",
            ),
            Kind::RuntimeHostInvalidJson => DiagnosticEventSpec::error(
                "runtime.host_invalid_json",
                "runtime_host",
                "runtime host emitted invalid JSON",
            ),
            Kind::RuntimeHostProtocolVersionInvalid => DiagnosticEventSpec::error(
                "runtime.host_protocol_version_invalid",
                "runtime_host",
                "runtime host emitted an incompatible protocol version",
            ),
            Kind::RuntimeHostUnknownResponse => DiagnosticEventSpec::error(
                "runtime.host_unknown_response",
                "runtime_host",
                "runtime host returned an unknown control response",
            ),
            Kind::RuntimeHostInvalidResponse => DiagnosticEventSpec::error(
                "runtime.host_invalid_response",
                "runtime_host",
                "runtime host returned an invalid control response",
            ),
            Kind::RuntimeHostInvalidEvent => DiagnosticEventSpec::error(
                "runtime.host_invalid_event",
                "runtime_host",
                "runtime host emitted an invalid runtime event",
            ),
            Kind::RuntimeHostInvalidScope => DiagnosticEventSpec::error(
                "runtime.host_invalid_scope",
                "runtime_host",
                "runtime host event omitted its session or turn scope",
            ),
            Kind::RuntimeHostInvalidError => DiagnosticEventSpec::error(
                "runtime.host_invalid_error",
                "runtime_host",
                "runtime host emitted an invalid scoped error",
            ),
            Kind::RuntimeHostUnknownEvent => DiagnosticEventSpec::error(
                "runtime.host_unknown_event",
                "runtime_host",
                "runtime host emitted an unknown notification",
            ),
            Kind::HeadTurnAccepted => DiagnosticEventSpec::info(
                "head.turn_accepted",
                "conversation",
                "project conversation turn was durably accepted",
            ),
            Kind::HeadTurnTerminalCommitted => DiagnosticEventSpec::info(
                "head.turn_terminal_committed",
                "conversation",
                "project conversation terminal state was durably committed",
            ),
            Kind::HeadDemoStarted => DiagnosticEventSpec::info(
                "head.demo_started",
                "startup",
                "credential-free Head demo started",
            ),
            Kind::HeadDemoCompleted => DiagnosticEventSpec::info(
                "head.demo_completed",
                "runtime",
                "credential-free Head demo completed",
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DiagnosticEventSpec {
    level: DiagnosticLevel,
    code: &'static str,
    phase: &'static str,
    message: &'static str,
}

impl DiagnosticEventSpec {
    const fn info(code: &'static str, phase: &'static str, message: &'static str) -> Self {
        Self::new(DiagnosticLevel::Info, code, phase, message)
    }

    const fn warning(code: &'static str, phase: &'static str, message: &'static str) -> Self {
        Self::new(DiagnosticLevel::Warning, code, phase, message)
    }

    const fn error(code: &'static str, phase: &'static str, message: &'static str) -> Self {
        Self::new(DiagnosticLevel::Error, code, phase, message)
    }

    const fn new(
        level: DiagnosticLevel,
        code: &'static str,
        phase: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            level,
            code,
            phase,
            message,
        }
    }
}

/// A fixed-schema, metadata-only diagnostic event.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticEvent {
    kind: DiagnosticEventKind,
    status: &'static str,
    error_code: &'static str,
    project_id: Option<Uuid>,
    session_id: Option<Uuid>,
    command_id: Option<Uuid>,
    runtime_turn_id: Option<Uuid>,
    provider: Option<DiagnosticProvider>,
    retryable: Option<bool>,
}

impl DiagnosticEvent {
    #[must_use]
    pub const fn new(kind: DiagnosticEventKind) -> Self {
        Self {
            kind,
            status: "",
            error_code: "",
            project_id: None,
            session_id: None,
            command_id: None,
            runtime_turn_id: None,
            provider: None,
            retryable: None,
        }
    }

    #[must_use]
    pub const fn status(mut self, value: &'static str) -> Self {
        self.status = value;
        self
    }

    #[must_use]
    pub const fn error_code(mut self, value: &'static str) -> Self {
        self.error_code = value;
        self
    }

    #[must_use]
    pub const fn project_id(mut self, value: Uuid) -> Self {
        self.project_id = Some(value);
        self
    }

    #[must_use]
    pub const fn session_id(mut self, value: Uuid) -> Self {
        self.session_id = Some(value);
        self
    }

    #[must_use]
    pub const fn command_id(mut self, value: Uuid) -> Self {
        self.command_id = Some(value);
        self
    }

    #[must_use]
    pub const fn runtime_turn_id(mut self, value: Uuid) -> Self {
        self.runtime_turn_id = Some(value);
        self
    }

    #[must_use]
    pub const fn provider(mut self, value: DiagnosticProvider) -> Self {
        self.provider = Some(value);
        self
    }

    #[must_use]
    pub const fn retryable(mut self, value: bool) -> Self {
        self.retryable = Some(value);
        self
    }
}

#[derive(Clone, Copy, Debug)]
enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

pub fn record(event: DiagnosticEvent) {
    let Some(context) = PROCESS_CONTEXT.get() else {
        return;
    };
    let project_id = optional_uuid(event.project_id);
    let session_id = optional_uuid(event.session_id);
    let command_id = optional_uuid(event.command_id);
    let runtime_turn_id = optional_uuid(event.runtime_turn_id);
    let provider_id = event.provider.map_or("", DiagnosticProvider::as_str);
    let spec = event.kind.spec();
    let status = optional_registered_code(event.status, REGISTERED_STATUSES);
    let error_code = optional_registered_code(event.error_code, REGISTERED_ERROR_CODES);
    let retryable = event
        .retryable
        .map_or("", |value| if value { "true" } else { "false" });
    match spec.level {
        DiagnosticLevel::Info => tracing::info!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = spec.code,
            phase = spec.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = spec.message,
        ),
        DiagnosticLevel::Warning => tracing::warn!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = spec.code,
            phase = spec.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = spec.message,
        ),
        DiagnosticLevel::Error => tracing::error!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = spec.code,
            phase = spec.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = spec.message,
        ),
    }
    context
        .checkpoint_publisher
        .publish(spec.phase, context.writer_health.dropped_records());
}

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("diagnostic component name is invalid")]
    InvalidComponent,
    #[error("diagnostic retention must be non-zero")]
    InvalidRetention,
    #[error("local diagnostic file appender could not be initialized")]
    AppenderInitialization,
    #[error("the process tracing subscriber is already initialized")]
    SubscriberInitialization,
    #[error("diagnostic lifecycle data is invalid")]
    InvalidLifecycleData,
    #[error("diagnostic directory escapes the configured data root")]
    DiagnosticRootEscape,
    #[error("diagnostic profile root must be an absolute ordinary directory")]
    InvalidProfileRoot,
    #[error("diagnostic entry must be one ordinary relative name")]
    InvalidDiagnosticEntry,
    #[error("diagnostic entry exceeds its byte limit")]
    DiagnosticEntryTooLarge,
    #[error("diagnostic directory exceeds its entry limit")]
    DiagnosticEntryLimit,
    #[error("diagnostic shutdown exceeded its bounded deadline")]
    ShutdownTimeout,
    #[error("diagnostic shutdown worker ended without a result")]
    ShutdownWorkerFailed,
    #[error("diagnostic I/O failed during {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("diagnostic JSON encoding failed")]
    Json(#[from] serde_json::Error),
}

impl DiagnosticsError {
    pub(crate) fn io(operation: &'static str, source: std::io::Error) -> Self {
        Self::Io { operation, source }
    }

    #[must_use]
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::InvalidComponent => "diagnostics.invalid_component",
            Self::InvalidRetention => "diagnostics.invalid_retention",
            Self::AppenderInitialization => "diagnostics.appender_unavailable",
            Self::SubscriberInitialization => "diagnostics.subscriber_unavailable",
            Self::InvalidLifecycleData => "diagnostics.lifecycle_invalid",
            Self::DiagnosticRootEscape => "diagnostics.root_escape",
            Self::InvalidProfileRoot => "diagnostics.profile_root_invalid",
            Self::InvalidDiagnosticEntry => "diagnostics.entry_invalid",
            Self::DiagnosticEntryTooLarge => "diagnostics.entry_too_large",
            Self::DiagnosticEntryLimit => "diagnostics.entry_limit",
            Self::ShutdownTimeout => "diagnostics.shutdown_timeout",
            Self::ShutdownWorkerFailed => "diagnostics.shutdown_worker_failed",
            Self::Io { .. } => "diagnostics.io_failure",
            Self::Json(_) => "diagnostics.json_failure",
        }
    }
}

pub(crate) fn valid_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 64
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 96
        && code.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn optional_uuid(value: Option<Uuid>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

const REGISTERED_STATUSES: &[&str] = &[
    "accepted",
    "cancelled",
    "clean",
    "completed",
    "failed",
    "non_terminal",
    "ready",
    "running",
    "starting",
    "stopped",
    "timed_out",
    "unclean",
    "unknown",
];

const REGISTERED_ERROR_CODES: &[&str] = &[
    "continuation_unavailable",
    "diagnostics.active_marker_invalid",
    "diagnostics.invalid_exit_code",
    "diagnostics.lifecycle_invalid",
    "diagnostics.previous_process_unclean",
    "head.demo_failure",
    "invalid_request",
    "node.already_running",
    "node.config.authority_epoch_invalid",
    "node.config.installation_invalid",
    "node.config.installation_missing",
    "node.config.runtime_invalid",
    "node.config.version_invalid",
    "node.conversation_failure",
    "node.instance_lock_unavailable",
    "node.session_failure",
    "node.shutdown_signal_failure",
    "node.transport_failure",
    "protocol_violation",
    "provider_failure",
    "provider_unavailable",
    "runtime_failure",
    "runtime_host.frame_too_large",
    "runtime_host.handshake_failed",
    "runtime_host.invalid_error",
    "runtime_host.invalid_event",
    "runtime_host.invalid_json",
    "runtime_host.invalid_response",
    "runtime_host.invalid_scope",
    "runtime_host.invalid_utf8",
    "runtime_host.missing",
    "runtime_host.protocol_mismatch",
    "runtime_host.protocol_version_invalid",
    "runtime_host.read_failed",
    "runtime_host.response_channel_closed",
    "runtime_host.spawn_failed",
    "runtime_host.stderr_frame_too_large",
    "runtime_host.stderr_read_failed",
    "runtime_host.stderr_unclassified",
    "runtime_host.stdout_eof",
    "runtime_host.unhandled_failure",
    "runtime_host.unknown_event",
    "runtime_host.unknown_response",
    "runtime_host.write_failed",
    "scope_mismatch",
    "unsupported",
];

fn optional_registered_code(value: &'static str, registered: &[&str]) -> &'static str {
    if value.is_empty() || registered.contains(&value) {
        value
    } else {
        "[invalid]"
    }
}

fn is_registered_diagnostic(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target() == DIAGNOSTIC_TARGET && metadata.module_path() == Some(DIAGNOSTIC_MODULE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_categories_never_persist_provider_supplied_text() {
        assert_eq!(
            DiagnosticProvider::from_adapter_id("openai.codex.sdk"),
            DiagnosticProvider::Codex
        );
        assert_eq!(
            DiagnosticProvider::from_adapter_id("sk-proj-secret-token"),
            DiagnosticProvider::Other
        );
        assert_eq!(DiagnosticProvider::Other.as_str(), "other");
    }

    #[test]
    fn component_and_code_names_are_safe() {
        assert!(valid_component("dennett-node"));
        assert!(!valid_component("../dennett-node"));
        assert!(valid_code("node.runtime_failure"));
        assert!(!valid_code("token\nleak"));
    }
}
