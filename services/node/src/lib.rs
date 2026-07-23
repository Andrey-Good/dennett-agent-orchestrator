mod project_fs;
mod project_location;
mod runtime_host;
mod workspace_fs;

use dennett_agent_core::{AgentRuntimePort, FakeAgentRuntime, RuntimeError};
use dennett_contracts::{CommandId, ProjectId, WorkspaceBindingId};
use dennett_head::conversation::{ConversationApplication, LocalProject};
use dennett_head::draft::{ComposerDraftApplication, SessionOperationLocks};
use dennett_head::project::{ProjectApplication, ProjectApplicationError, ProjectLocationError};
use dennett_head::session::SessionCoordinator;
use dennett_head::system::{SystemProjection, SystemSnapshot};
use dennett_head::workspace::{WorkspaceApplication, WorkspaceApplicationError};
use dennett_local_ipc::{
    HandshakePolicy, LocalEndpoint, ProjectServiceAdapter, SessionRegistry, SessionServiceAdapter,
    SystemServiceAdapter, TransportError, run_local_server,
};
use dennett_memory_core::session::{SessionJournal, SessionJournalError};
use dennett_storage_sqlite::SqliteControlStore;
use dennett_trust_core::project_registry::{ProjectRegistryError, ProjectRegistryPort};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

pub use runtime_host::{RUNTIME_HOST_SCRIPT_ENV, RUNTIME_NODE_EXECUTABLE_ENV};

pub const INSTALLATION_ID_ENV: &str = "DENNETT_INSTALLATION_ID";
pub const AUTHORITY_EPOCH_ENV: &str = "DENNETT_AUTHORITY_EPOCH";
pub const DATA_DIR_ENV: &str = "DENNETT_DATA_DIR";
pub const PROJECT_ROOT_ENV: &str = "DENNETT_PROJECT_ROOT";
pub const AGENT_RUNTIME_ENV: &str = "DENNETT_AGENT_RUNTIME";
const SYSTEM_WATCH_CAPACITY: usize = 128;
const SESSION_WATCH_CAPACITY: usize = 128;
const NODE_INSTANCE_LOCK_FILE: &str = "dennett-node.lock";
const CONTROL_DATABASE_FILE: &str = "control.sqlite3";
const CONTROL_DATABASE_TRANSIENT_FILES: [&str; 3] = [
    "control.sqlite3-journal",
    "control.sqlite3-shm",
    "control.sqlite3-wal",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeConfig {
    pub installation_id: String,
    pub authority_epoch: u64,
    pub node_version: String,
    pub data_dir: PathBuf,
    pub project_root: PathBuf,
    pub agent_runtime: AgentRuntimeSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRuntimeSelection {
    Fake,
    Codex,
}

impl NodeConfig {
    pub fn from_environment() -> Result<Self, NodeConfigError> {
        let installation_id = std::env::var(INSTALLATION_ID_ENV)
            .map_err(|_| NodeConfigError::MissingInstallationIdentity)?;
        let authority_epoch = std::env::var(AUTHORITY_EPOCH_ENV)
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|_| NodeConfigError::InvalidAuthorityEpoch)?
            .unwrap_or(1);
        let mut config = Self::new(
            installation_id.clone(),
            authority_epoch,
            env!("CARGO_PKG_VERSION"),
        )?;
        config.data_dir = diagnostic_data_dir_from_environment();
        config.project_root = std::env::var_os(PROJECT_ROOT_ENV)
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| config.data_dir.clone());
        config.agent_runtime = match std::env::var(AGENT_RUNTIME_ENV).as_deref() {
            Ok("fake") => AgentRuntimeSelection::Fake,
            Ok("codex") | Err(_) => AgentRuntimeSelection::Codex,
            Ok(_) => return Err(NodeConfigError::InvalidAgentRuntime),
        };
        Ok(config)
    }

    pub fn new(
        installation_id: impl Into<String>,
        authority_epoch: u64,
        node_version: impl Into<String>,
    ) -> Result<Self, NodeConfigError> {
        let installation_id = installation_id.into();
        LocalEndpoint::for_installation(installation_id.clone())
            .map_err(|_| NodeConfigError::InvalidInstallationIdentity)?;
        let node_version = node_version.into();
        if authority_epoch == 0 {
            return Err(NodeConfigError::InvalidAuthorityEpoch);
        }
        if node_version.is_empty() {
            return Err(NodeConfigError::InvalidNodeVersion);
        }
        let data_dir = std::env::temp_dir()
            .join("dennett-node-tests")
            .join(&installation_id);
        Ok(Self {
            installation_id,
            authority_epoch,
            node_version,
            data_dir,
            project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            agent_runtime: AgentRuntimeSelection::Fake,
        })
    }

    #[must_use]
    pub fn with_paths(mut self, data_dir: PathBuf, project_root: PathBuf) -> Self {
        self.data_dir = data_dir;
        self.project_root = project_root;
        self
    }

    #[must_use]
    pub fn with_agent_runtime(mut self, agent_runtime: AgentRuntimeSelection) -> Self {
        self.agent_runtime = agent_runtime;
        self
    }
}

#[must_use]
pub fn diagnostic_data_dir_from_environment() -> PathBuf {
    std::env::var_os(DATA_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let installation = std::env::var(INSTALLATION_ID_ENV)
                .ok()
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= 128
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
                })
                .unwrap_or_else(|| "unconfigured".to_owned());
            dennett_observability::default_user_data_root()
                .join("installations")
                .join(installation)
        })
}

pub async fn run<F>(config: NodeConfig, shutdown: F) -> Result<(), NodeRunError>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let data_root_guard = dennett_observability::guard_local_data_root(&config.data_dir)?;
    data_root_guard.ensure_unchanged()?;
    dennett_observability::record(
        dennett_observability::DiagnosticEvent::new(
            dennett_observability::DiagnosticEventKind::NodeConfigurationValidated,
        )
        .status("ready"),
    );
    tracing::info!(
        phase = "local_ipc_configuration",
        authority_epoch = config.authority_epoch,
        "validated Node local IPC configuration"
    );
    let standalone_workspace = config.data_dir.join("standalone-workspace");
    data_root_guard.ensure_child_directory("standalone-workspace")?;
    let _instance_lock = NodeInstanceLock::acquire(&data_root_guard)?;
    dennett_observability::record(
        dennett_observability::DiagnosticEvent::new(
            dennett_observability::DiagnosticEventKind::NodeInstanceLockAcquired,
        )
        .status("ready"),
    );
    let endpoint = LocalEndpoint::for_installation(config.installation_id.clone())?;
    let _control_file_guard = data_root_guard.open_or_create_regular_file(CONTROL_DATABASE_FILE)?;
    for name in CONTROL_DATABASE_TRANSIENT_FILES {
        data_root_guard.validate_optional_regular_file(name)?;
    }
    data_root_guard.ensure_unchanged()?;
    let control_database_path = data_root_guard.stable_child_path(CONTROL_DATABASE_FILE)?;
    let store = SqliteControlStore::open(control_database_path).await?;
    data_root_guard.ensure_unchanged()?;
    let store = Arc::new(store);
    let coordinator = SessionCoordinator::new(
        SessionJournal::new(store.clone()),
        config.authority_epoch,
        SESSION_WATCH_CAPACITY,
    );
    // M01 derived its sole project identity from the configured path. Keep
    // that value only as a migration key; the M02 registry persists it and
    // every newly registered project receives a path-independent UUID.
    let project_root = std::path::absolute(&config.project_root)
        .map_err(|_| NodeRunError::ProjectLocation(ProjectLocationError::InvalidRequest))?;
    let project_id = legacy_m01_project_id_for(&project_root);
    let display_name = config
        .project_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Dennett project")
        .to_owned();
    let locks = SessionOperationLocks::default();
    let drafts = ComposerDraftApplication::new(coordinator.clone(), store.clone(), locks);
    let runtime: Arc<dyn AgentRuntimePort> = match config.agent_runtime {
        AgentRuntimeSelection::Fake => Arc::new(FakeAgentRuntime),
        AgentRuntimeSelection::Codex => Arc::new(runtime_host::HostedAgentRuntime::start().await?),
    };
    let runtime_descriptor = runtime.describe().await?;
    dennett_observability::record(
        dennett_observability::DiagnosticEvent::new(
            dennett_observability::DiagnosticEventKind::NodeRuntimeReady,
        )
        .provider(dennett_observability::DiagnosticProvider::from_adapter_id(
            &runtime_descriptor.adapter_id,
        ))
        .status("ready"),
    );
    let mut system_snapshot = SystemSnapshot::empty(config.authority_epoch);
    system_snapshot.runtime = Some(runtime_descriptor);
    let projection = Arc::new(SystemProjection::new(
        system_snapshot,
        SYSTEM_WATCH_CAPACITY,
    ));
    let project_locations = Arc::new(project_location::NodeProjectLocationAdapter::default());
    if store.get_project(project_id).await?.is_none() {
        let legacy_import = project_locations
            .inspect_legacy_import(
                project_id,
                legacy_m01_binding_id_for(project_id),
                display_name.clone(),
                project_root.clone(),
                unix_time_ms(),
            )
            .await?;
        store.import_legacy_project(legacy_import).await?;
    }
    let project_application = Arc::new(ProjectApplication::new(
        store.clone(),
        project_locations,
        coordinator.clone(),
        projection.clone(),
    ));
    for result in project_application.reconcile_registrations().await {
        if let Err(error) = result {
            tracing::warn!(
                code = project_application_error_code(&error),
                "project registration remains recoverable after startup reconciliation"
            );
        }
    }
    for (reconciled_project_id, result) in project_application
        .reconcile_project_locations("m02.startup_reconcile")
        .await?
    {
        if let Err(error) = result {
            tracing::info!(
                dennett.project.id = %reconciled_project_id.0,
                code = project_application_error_code(&error),
                "project location is not ready after startup reconciliation"
            );
        }
    }
    let workspace_application = Arc::new(WorkspaceApplication::new(
        project_application.clone(),
        store.clone(),
        Arc::new(workspace_fs::NodeWorkspaceFilesystemAdapter),
    ));
    let reconciled_workspace_effects = workspace_application.reconcile_unfinished().await?;
    if !reconciled_workspace_effects.is_empty() {
        tracing::info!(
            count = reconciled_workspace_effects.len(),
            "reconciled durable workspace effects before accepting local commands"
        );
    }
    let application = Arc::new(
        ConversationApplication::new(
            coordinator,
            projection.clone(),
            runtime,
            LocalProject {
                project_id,
                display_name,
                workspace_path: project_root.to_string_lossy().into_owned(),
                standalone_workspace_path: standalone_workspace.to_string_lossy().into_owned(),
            },
        )
        .with_projects(project_application.clone())
        .with_continuations(store.clone())
        .with_drafts(drafts.clone()),
    );
    application
        .initialize(CommandId::new(), "Untitled chat".to_owned())
        .await?;
    let sessions = SessionRegistry::new(HandshakePolicy::m01(
        config.installation_id,
        config.node_version,
        config.authority_epoch,
    ));
    let system_service = SystemServiceAdapter::new(projection, sessions.clone());
    let session_service =
        SessionServiceAdapter::new(application, drafts, sessions.clone(), store.clone());
    let project_service = ProjectServiceAdapter::new(project_application, sessions, store);
    tracing::info!(
        phase = "local_ipc_listen",
        "starting authenticated Node IPC"
    );
    dennett_observability::record(
        dennett_observability::DiagnosticEvent::new(
            dennett_observability::DiagnosticEventKind::NodeIpcStartRequested,
        )
        .status("starting"),
    );
    let result = run_local_server(
        endpoint,
        system_service,
        session_service,
        project_service,
        shutdown,
    )
    .await
    .map_err(Into::into);
    if result.is_ok() {
        dennett_observability::record(
            dennett_observability::DiagnosticEvent::new(
                dennett_observability::DiagnosticEventKind::NodeIpcStopped,
            )
            .status("stopped"),
        );
    }
    result
}

struct NodeInstanceLock {
    _file: File,
}

impl NodeInstanceLock {
    fn acquire(
        data_root: &dennett_observability::LocalDataRootGuard,
    ) -> Result<Self, NodeRunError> {
        let file = data_root.open_or_create_regular_file(NODE_INSTANCE_LOCK_FILE)?;
        file.try_lock_exclusive()
            .map_err(|_| NodeRunError::AlreadyRunning)?;
        Ok(Self { _file: file })
    }
}

fn legacy_m01_project_id_for(project_root: &std::path::Path) -> ProjectId {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let mut identity = canonical.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        identity.make_ascii_lowercase();
    }
    let digest = Sha256::digest(identity.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    ProjectId(uuid::Uuid::from_bytes(bytes))
}

fn legacy_m01_binding_id_for(project_id: ProjectId) -> WorkspaceBindingId {
    let digest = Sha256::digest(format!("dennett:m01-binding:{}", project_id.0).as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    WorkspaceBindingId(uuid::Uuid::from_bytes(bytes))
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn project_application_error_code(error: &ProjectApplicationError) -> &'static str {
    match error {
        ProjectApplicationError::InvalidRequest => "project.request.invalid",
        ProjectApplicationError::ProjectNotFound => "project.not_found",
        ProjectApplicationError::BindingNotFound => "project.binding.not_found",
        ProjectApplicationError::TrustDecisionMissing => "project.trust.decision_missing",
        ProjectApplicationError::RecoveryRequired => "project.registration.recovery_required",
        ProjectApplicationError::ProjectRestricted => "project.trust.restricted",
        ProjectApplicationError::ProjectRevoked => "project.trust.revoked",
        ProjectApplicationError::ProjectMissing => "project.location.missing",
        ProjectApplicationError::ProjectDetached => "project.location.detached",
        ProjectApplicationError::ProjectInaccessible => "project.location.inaccessible",
        ProjectApplicationError::ConcurrentChange => "project.concurrent_change",
        ProjectApplicationError::Location(error) => error.safe_code(),
        ProjectApplicationError::Registry(_) => "project.registry.unavailable",
        ProjectApplicationError::TrustDecision(_) => "project.trust.decision_rejected",
        ProjectApplicationError::Session(_) => "project.session.unavailable",
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeRunError {
    #[error("the local data root is unsafe or unavailable")]
    DataRoot(#[from] dennett_observability::DiagnosticsError),
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Session(#[from] SessionJournalError),
    #[error(transparent)]
    Conversation(#[from] dennett_head::conversation::ConversationError),
    #[error("another Dennett Node already owns this installation")]
    AlreadyRunning,
    #[error("the Dennett Node instance lock is unavailable")]
    InstanceLockUnavailable,
    #[error(transparent)]
    RuntimeHost(#[from] runtime_host::RuntimeHostStartError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    ProjectLocation(#[from] ProjectLocationError),
    #[error(transparent)]
    ProjectRegistry(#[from] ProjectRegistryError),
    #[error(transparent)]
    ProjectApplication(#[from] ProjectApplicationError),
    #[error(transparent)]
    WorkspaceApplication(#[from] WorkspaceApplicationError),
}

impl NodeRunError {
    #[must_use]
    pub fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::DataRoot(_) => "node.data_root_unavailable",
            Self::Transport(_) => "node.transport_failure",
            Self::Session(_) => "node.session_failure",
            Self::Conversation(_) => "node.conversation_failure",
            Self::AlreadyRunning => "node.already_running",
            Self::InstanceLockUnavailable => "node.instance_lock_unavailable",
            Self::RuntimeHost(error) => error.diagnostic_code(),
            Self::Runtime(error) => error.code.as_str(),
            Self::ProjectLocation(error) => error.safe_code(),
            Self::ProjectRegistry(_) => "node.project_registry_failure",
            Self::ProjectApplication(error) => project_application_error_code(error),
            Self::WorkspaceApplication(_) => "node.workspace_recovery_failure",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NodeConfigError {
    #[error("DENNETT_INSTALLATION_ID is required")]
    MissingInstallationIdentity,
    #[error("installation identity is invalid")]
    InvalidInstallationIdentity,
    #[error("authority epoch must be a positive integer")]
    InvalidAuthorityEpoch,
    #[error("Node version is invalid")]
    InvalidNodeVersion,
    #[error("DENNETT_AGENT_RUNTIME must be fake or codex")]
    InvalidAgentRuntime,
}

impl NodeConfigError {
    #[must_use]
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::MissingInstallationIdentity => "node.config.installation_missing",
            Self::InvalidInstallationIdentity => "node.config.installation_invalid",
            Self::InvalidAuthorityEpoch => "node.config.authority_epoch_invalid",
            Self::InvalidNodeVersion => "node.config.version_invalid",
            Self::InvalidAgentRuntime => "node.config.runtime_invalid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_invalid_identity_epoch_and_version() {
        assert!(matches!(
            NodeConfig::new("", 1, "0.1.0"),
            Err(NodeConfigError::InvalidInstallationIdentity)
        ));
        assert!(matches!(
            NodeConfig::new("install", 0, "0.1.0"),
            Err(NodeConfigError::InvalidAuthorityEpoch)
        ));
        assert!(matches!(
            NodeConfig::new("install", 1, ""),
            Err(NodeConfigError::InvalidNodeVersion)
        ));
    }

    #[tokio::test]
    async fn run_rejects_a_relative_data_root_before_opening_canonical_state() {
        let relative = PathBuf::from(format!(
            "relative-dennett-data-{}",
            uuid::Uuid::now_v7().simple()
        ));
        let config = NodeConfig::new("unsafe-relative-root", 1, "0.1.0")
            .expect("Node config")
            .with_paths(relative.clone(), std::env::temp_dir());
        let error = run(config, async {})
            .await
            .expect_err("relative data root must fail closed");
        assert!(matches!(
            error,
            NodeRunError::DataRoot(dennett_observability::DiagnosticsError::InvalidProfileRoot)
        ));
        assert!(!relative.exists());
    }

    #[tokio::test]
    async fn run_rejects_preplanted_database_and_sidecar_links() {
        for name in std::iter::once(CONTROL_DATABASE_FILE).chain(CONTROL_DATABASE_TRANSIENT_FILES) {
            let directory = tempfile::tempdir().expect("temporary profile parent");
            let profile = directory.path().join("profile");
            std::fs::create_dir(&profile).expect("profile");
            let outside = directory.path().join("outside.sqlite3");
            std::fs::write(&outside, b"outside").expect("outside file");
            let link = profile.join(name);
            if !create_file_link(&outside, &link) {
                return;
            }
            let config = NodeConfig::new("unsafe-database-link", 1, "0.1.0")
                .expect("Node config")
                .with_paths(profile, std::env::temp_dir());

            let error = run(config, async {})
                .await
                .expect_err("linked database entry must fail closed");
            assert!(matches!(error, NodeRunError::DataRoot(_)), "entry: {name}");
            assert_eq!(
                std::fs::read(&outside).expect("outside file"),
                b"outside",
                "entry: {name}"
            );
        }
    }

    #[test]
    fn one_process_owns_a_profile_lock_at_a_time() {
        let directory = tempfile::tempdir().expect("temporary profile");
        let root = dennett_observability::guard_local_data_root(directory.path())
            .expect("guarded profile");
        let first = NodeInstanceLock::acquire(&root).expect("first lock");
        assert!(matches!(
            NodeInstanceLock::acquire(&root),
            Err(NodeRunError::AlreadyRunning)
        ));
        drop(first);
        NodeInstanceLock::acquire(&root).expect("lock released");
    }

    #[cfg(unix)]
    fn create_file_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        std::os::unix::fs::symlink(target, link).expect("file symlink");
        true
    }

    #[cfg(windows)]
    fn create_file_link(target: &std::path::Path, link: &std::path::Path) -> bool {
        if let Err(error) = std::os::windows::fs::symlink_file(target, link) {
            eprintln!("skipping symlink assertion: {error}");
            return false;
        }
        true
    }
}
