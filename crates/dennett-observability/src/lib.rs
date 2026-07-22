//! Privacy-safe process diagnostics for local Dennett profiles.
//!
//! Personal Quiet persistence accepts only the fixed [`DiagnosticEvent`]
//! schema. Correlation uses UUID values and a bounded provider category rather
//! than arbitrary strings, so prompts, responses, paths and credentials cannot
//! be smuggled into an identifier field.

mod lifecycle;
mod writer;

use lifecycle::LifecycleSession;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{Layer, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;
use writer::{PreparedWriter, WriterHealth, prepare_writer};

pub use lifecycle::{
    ActiveRunSummary, DiagnosticExit, DiagnosticSummary, ExitStatus, MarkerState, inspect_local,
};

const DIAGNOSTIC_TARGET: &str = "dennett.private_safe_diagnostic";
const DIAGNOSTIC_MODULE_PATH: &str = module_path!();
const DEFAULT_MAX_LOG_FILES: usize = 14;
const DEFAULT_MAX_LIFECYCLE_RECORDS: usize = 64;
const DEFAULT_MAX_LOG_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
const DEFAULT_MAX_LOG_BYTES: u64 = 32 * 1024 * 1024;
static PROCESS_CONTEXT: OnceLock<ProcessContext> = OnceLock::new();

struct ProcessContext {
    component: String,
    run_id: String,
    progress: lifecycle::LifecycleProgress,
    writer_health: WriterHealth,
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
}

impl LocalDiagnostics {
    pub fn shutdown(mut self, exit: DiagnosticExit) -> Result<(), DiagnosticsError> {
        let (status, error_code) = match exit {
            DiagnosticExit::Clean => ("clean", ""),
            DiagnosticExit::Failed { error_code } => ("failed", error_code),
        };
        record(
            DiagnosticEvent::info(
                "diagnostics.process_exit",
                "shutdown",
                "process diagnostic session reached a handled exit",
            )
            .status(status)
            .error_code(error_code),
        );
        drop(self.worker_guard.take());
        let dropped_log_records = self.writer_health.dropped_records();
        self.lifecycle
            .take()
            .expect("local diagnostics lifecycle is present until shutdown")
            .complete(exit, dropped_log_records)
    }

    pub fn summary(&self) -> Result<DiagnosticSummary, DiagnosticsError> {
        self.lifecycle
            .as_ref()
            .expect("local diagnostics lifecycle is present until shutdown")
            .progress()
            .note_dropped_records(self.writer_health.dropped_records())?;
        inspect_local(&self.data_dir, &self.component)
    }
}

/// Initializes bounded JSONL diagnostics plus a metadata-only console layer.
///
/// Initialization is transactional with respect to the lifecycle marker: a
/// failure after marker creation removes only the marker owned by this attempt.
pub fn init_local(config: LocalDiagnosticsConfig) -> Result<LocalDiagnostics, DiagnosticsError> {
    config.validate()?;
    let diagnostics_dir = diagnostics_dir(&config.data_dir);
    let lifecycle = LifecycleSession::start(
        &diagnostics_dir,
        &config.component,
        config.max_lifecycle_records,
    )?;
    let progress = lifecycle.progress();
    let PreparedWriter {
        writer,
        guard,
        health,
    } = match prepare_writer(
        &config.data_dir,
        &diagnostics_dir,
        &config.component,
        config.max_log_files,
        config.max_log_age,
        config.max_log_bytes,
        progress.clone(),
    ) {
        Ok(writer) => writer,
        Err(error) => {
            lifecycle.cancel_startup()?;
            return Err(error);
        }
    };
    let context = ProcessContext {
        component: config.component.clone(),
        run_id: lifecycle.run_id().to_owned(),
        progress,
        writer_health: health.clone(),
    };
    if PROCESS_CONTEXT.set(context).is_err() {
        lifecycle.cancel_startup()?;
        return Err(DiagnosticsError::SubscriberInitialization);
    }

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
        lifecycle.cancel_startup()?;
        return Err(DiagnosticsError::SubscriberInitialization);
    }

    let previous_status = lifecycle.previous_status().as_str();
    record(
        DiagnosticEvent::info(
            "diagnostics.initialized",
            "startup",
            "privacy-safe local diagnostics initialized",
        )
        .status(previous_status),
    );
    Ok(LocalDiagnostics {
        component: config.component,
        data_dir: config.data_dir,
        lifecycle: Some(lifecycle),
        writer_health: health,
        worker_guard: Some(guard),
    })
}

/// Console-only bootstrap for components without a durable diagnostic profile.
pub fn init(service_name: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .try_init();
    tracing::info!(service = service_name, "observability initialized");
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

/// A fixed-schema, metadata-only diagnostic event.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticEvent {
    level: DiagnosticLevel,
    code: &'static str,
    phase: &'static str,
    message: &'static str,
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
    pub const fn info(code: &'static str, phase: &'static str, message: &'static str) -> Self {
        Self::new(DiagnosticLevel::Info, code, phase, message)
    }

    #[must_use]
    pub const fn warning(code: &'static str, phase: &'static str, message: &'static str) -> Self {
        Self::new(DiagnosticLevel::Warning, code, phase, message)
    }

    #[must_use]
    pub const fn error(code: &'static str, phase: &'static str, message: &'static str) -> Self {
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
    let status = optional_code(event.status);
    let error_code = optional_code(event.error_code);
    let retryable = event
        .retryable
        .map_or("", |value| if value { "true" } else { "false" });
    match event.level {
        DiagnosticLevel::Info => tracing::info!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = event.code,
            phase = event.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = event.message,
        ),
        DiagnosticLevel::Warning => tracing::warn!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = event.code,
            phase = event.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = event.message,
        ),
        DiagnosticLevel::Error => tracing::error!(
            target: DIAGNOSTIC_TARGET,
            component = context.component.as_str(),
            run_id = context.run_id.as_str(),
            event_code = event.code,
            phase = event.phase,
            status,
            error_code,
            project_id,
            session_id,
            command_id,
            runtime_turn_id,
            provider_id,
            retryable,
            redaction = "metadata_only",
            message = event.message,
        ),
    }
    let _ = context
        .progress
        .checkpoint(event.phase, context.writer_health.dropped_records());
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
            Self::Io { .. } => "diagnostics.io_failure",
            Self::Json(_) => "diagnostics.json_failure",
        }
    }
}

fn diagnostics_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("diagnostics")
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

fn optional_code(value: &'static str) -> &'static str {
    if value.is_empty() || valid_code(value) {
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
