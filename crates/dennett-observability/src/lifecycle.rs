use crate::{
    DiagnosticsError,
    secure_fs::{SecureDir, lock_exclusive_bounded},
    valid_code, valid_component,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fmt,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 2;
const MAX_LIFECYCLE_RECORD_BYTES: u64 = 64 * 1024;
const MAX_LIFECYCLE_DIRECTORY_ENTRIES: usize = 512;
const MAX_CLOCK_SKEW_MS: u64 = 24 * 60 * 60 * 1000;
const UNCLEAN_EXIT_CODE: &str = "diagnostics.previous_process_unclean";
const INVALID_MARKER_CODE: &str = "diagnostics.active_marker_invalid";
const INVALID_EXIT_CODE: &str = "diagnostics.invalid_exit_code";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticExit {
    Clean,
    Failed { error_code: &'static str },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitStatus {
    Unknown,
    Clean,
    Failed,
    Unclean,
}

impl ExitStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Clean => "clean",
            Self::Failed => "failed",
            Self::Unclean => "unclean",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LifecycleRecord {
    schema_version: u32,
    component: String,
    run_id: String,
    process_id: u32,
    run_sequence: u64,
    started_unix_ms: u64,
    completed_unix_ms: Option<u64>,
    status: LifecycleStatus,
    error_code: Option<String>,
    #[serde(default)]
    dropped_log_records: usize,
    #[serde(default = "unknown_phase")]
    last_durable_phase: String,
    #[serde(default)]
    checkpoint_sequence: u64,
    #[serde(default)]
    clock_anomaly: bool,
    #[serde(default)]
    flush_status: FlushStatus,
    #[serde(default)]
    drop_count_complete: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LifecycleStatus {
    Running,
    Clean,
    Failed,
    Unclean,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FlushStatus {
    #[default]
    Pending,
    Confirmed,
    Incomplete,
}

pub(crate) struct LifecycleSession {
    directory: SecureDir,
    active_path: PathBuf,
    active_lock_path: PathBuf,
    active_lock_file: Option<File>,
    run_id: String,
    state: Arc<Mutex<LifecycleProgressState>>,
    previous_status: ExitStatus,
    max_records: usize,
}

struct LifecycleProgressState {
    record: LifecycleRecord,
    latest_checkpoint_path: Option<PathBuf>,
    completed: bool,
}

#[derive(Clone)]
pub(crate) struct LifecycleProgress {
    directory: SecureDir,
    state: Arc<Mutex<LifecycleProgressState>>,
}

impl LifecycleSession {
    pub(crate) fn start(
        diagnostics_dir: &SecureDir,
        component: &str,
        max_records: usize,
    ) -> Result<Self, DiagnosticsError> {
        let directory =
            diagnostics_dir.open_or_create_child("lifecycle", "create_lifecycle_directory")?;
        let _maintenance = acquire_maintenance_lock(&directory, component)?;
        cleanup_auxiliary_files(&directory, component)?;
        reconcile_stale_markers(&directory, component)?;
        trim_terminal_records(&directory, component, max_records)?;
        let previous_status = latest_terminal_record(&directory, component)?
            .map_or(ExitStatus::Unknown, |record| record.exit_status());
        let process_id = std::process::id();
        let started_unix_ms = now_unix_ms()?;
        let run_sequence = allocate_run_sequence(&directory, component)?;
        let run_id = Uuid::now_v7().to_string();
        let record = LifecycleRecord {
            schema_version: SCHEMA_VERSION,
            component: component.to_owned(),
            run_id: run_id.clone(),
            process_id,
            run_sequence,
            started_unix_ms,
            completed_unix_ms: None,
            status: LifecycleStatus::Running,
            error_code: None,
            dropped_log_records: 0,
            last_durable_phase: "startup".to_owned(),
            checkpoint_sequence: 0,
            clock_anomaly: false,
            flush_status: FlushStatus::Pending,
            drop_count_complete: false,
        };
        let active_path = PathBuf::from(format!(
            "{component}.{run_id}.{run_sequence:020}.active.json"
        ));
        let active_lock_path = lock_path_for(&active_path);
        let active_lock_file =
            directory.create_new_file(path_name(&active_lock_path)?, true, "create_active_lock")?;
        FileExt::try_lock_exclusive(&active_lock_file)
            .map_err(|source| DiagnosticsError::io("lock_active_marker", source))?;
        if let Err(error) = atomic_write_new(
            &directory,
            &active_path,
            &record,
            "write_active_marker",
            false,
        ) {
            let _ = FileExt::unlock(&active_lock_file);
            drop(active_lock_file);
            let _ = directory.remove_file(
                path_name(&active_lock_path).unwrap_or_default(),
                "remove_active_lock",
            );
            return Err(error);
        }
        let state = Arc::new(Mutex::new(LifecycleProgressState {
            record,
            latest_checkpoint_path: None,
            completed: false,
        }));
        Ok(Self {
            directory,
            active_path,
            active_lock_path,
            active_lock_file: Some(active_lock_file),
            run_id,
            state,
            previous_status,
            max_records,
        })
    }

    pub(crate) const fn previous_status(&self) -> ExitStatus {
        self.previous_status
    }

    pub(crate) fn run_id(&self) -> &str {
        &self.run_id
    }

    pub(crate) fn progress(&self) -> LifecycleProgress {
        LifecycleProgress {
            directory: self.directory.clone(),
            state: Arc::clone(&self.state),
        }
    }

    pub(crate) fn cancel_startup(mut self) -> Result<(), DiagnosticsError> {
        let component = {
            let mut state = self.lock_state()?;
            state.completed = true;
            state.record.component.clone()
        };
        let _maintenance = acquire_maintenance_lock(&self.directory, &component)?;
        remove_checkpoints_for_run(&self.directory, &component, &self.run_id)?;
        self.remove_active()
    }

    pub(crate) fn complete(
        mut self,
        exit: DiagnosticExit,
        dropped_log_records: usize,
        writer_flush_confirmed: bool,
        drop_count_complete: bool,
    ) -> Result<(), DiagnosticsError> {
        let mut invalid_exit = false;
        let (status, error_code) = match exit {
            DiagnosticExit::Clean => (LifecycleStatus::Clean, None),
            DiagnosticExit::Failed { error_code } if valid_code(error_code) => {
                (LifecycleStatus::Failed, Some(error_code.to_owned()))
            }
            DiagnosticExit::Failed { .. } => {
                invalid_exit = true;
                (LifecycleStatus::Failed, Some(INVALID_EXIT_CODE.to_owned()))
            }
        };
        let mut state = self.lock_state()?;
        let _maintenance = acquire_maintenance_lock(&self.directory, &state.record.component)?;
        let completed = now_unix_ms()?;
        state.record.clock_anomaly = completed < state.record.started_unix_ms;
        state.record.completed_unix_ms = Some(completed.max(state.record.started_unix_ms));
        state.record.status = status;
        state.record.error_code = error_code;
        state.record.dropped_log_records = dropped_log_records;
        state.record.last_durable_phase = "shutdown".to_owned();
        state.record.flush_status = if writer_flush_confirmed {
            FlushStatus::Confirmed
        } else {
            FlushStatus::Incomplete
        };
        state.record.drop_count_complete = drop_count_complete;
        write_terminal_record(&self.directory, &state.record)?;
        state.completed = true;
        let component = state.record.component.clone();
        let run_id = state.record.run_id.clone();
        drop(state);
        remove_checkpoints_for_run(&self.directory, &component, &run_id)?;
        self.remove_active()?;
        trim_terminal_records(&self.directory, &component, self.max_records)?;
        if invalid_exit {
            Err(DiagnosticsError::InvalidLifecycleData)
        } else {
            Ok(())
        }
    }

    fn remove_active(&mut self) -> Result<(), DiagnosticsError> {
        remove_active_files(
            &self.directory,
            &self.active_path,
            &self.active_lock_path,
            &mut self.active_lock_file,
        )
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, LifecycleProgressState>, DiagnosticsError> {
        self.state
            .lock()
            .map_err(|_| DiagnosticsError::InvalidLifecycleData)
    }
}

impl LifecycleProgress {
    pub(crate) fn checkpoint(
        &self,
        phase: &'static str,
        dropped_log_records: usize,
    ) -> Result<(), DiagnosticsError> {
        self.checkpoint_owned_phase(phase.to_owned(), dropped_log_records)
    }

    fn checkpoint_owned_phase(
        &self,
        phase: String,
        dropped_log_records: usize,
    ) -> Result<(), DiagnosticsError> {
        if !valid_code(&phase) {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| DiagnosticsError::InvalidLifecycleData)?;
        if state.completed
            || (state.record.last_durable_phase == phase
                && state.record.dropped_log_records == dropped_log_records)
        {
            return Ok(());
        }
        persist_checkpoint(&self.directory, &mut state, phase, dropped_log_records)
    }
}

fn persist_checkpoint(
    directory: &SecureDir,
    state: &mut LifecycleProgressState,
    phase: String,
    dropped_log_records: usize,
) -> Result<(), DiagnosticsError> {
    let _maintenance = acquire_maintenance_lock(directory, &state.record.component)?;
    let mut checkpoint = state.record.clone();
    checkpoint.last_durable_phase = phase;
    checkpoint.dropped_log_records = dropped_log_records;
    checkpoint.checkpoint_sequence = checkpoint
        .checkpoint_sequence
        .checked_add(1)
        .ok_or(DiagnosticsError::InvalidLifecycleData)?;
    let path = checkpoint_path(&checkpoint);
    atomic_write_new(
        directory,
        &path,
        &checkpoint,
        "write_lifecycle_checkpoint",
        false,
    )?;
    let previous = state.latest_checkpoint_path.replace(path);
    state.record = checkpoint;
    if let Some(previous) = previous {
        remove_if_present(directory, &previous, "remove_lifecycle_checkpoint")?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActiveRunSummary {
    pub run_id: String,
    pub process_id: u32,
    pub started_unix_ms: u64,
    pub last_durable_phase: String,
    pub dropped_log_records: usize,
    pub marker_state: MarkerState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerState {
    Live,
    Stale,
    Unreadable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticStorageStatus {
    NotInitialized,
    Available,
    Degraded,
    InvalidLayout,
    Unreadable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticFlushStatus {
    Unknown,
    Confirmed,
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticSummary {
    pub component: String,
    pub diagnostics_dir: PathBuf,
    pub storage_status: DiagnosticStorageStatus,
    pub log_file_count: usize,
    pub log_bytes: u64,
    pub dropped_log_records: usize,
    pub active_runs: Vec<ActiveRunSummary>,
    pub unreadable_active_runs: usize,
    pub previous_exit: ExitStatus,
    pub previous_error_code: Option<String>,
    pub previous_last_durable_phase: Option<String>,
    pub previous_flush_status: DiagnosticFlushStatus,
    pub previous_drop_count_complete: bool,
    pub previous_clock_anomaly: bool,
    pub unreadable_log_entries: usize,
    pub unreadable_lifecycle_records: usize,
}

impl fmt::Display for DiagnosticSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let live = self
            .active_runs
            .iter()
            .filter(|run| run.marker_state == MarkerState::Live)
            .count();
        let stale = self
            .active_runs
            .iter()
            .filter(|run| run.marker_state == MarkerState::Stale)
            .count();
        let unreadable = self.unreadable_active_runs
            + self
                .active_runs
                .iter()
                .filter(|run| run.marker_state == MarkerState::Unreadable)
                .count();
        writeln!(formatter, "Dennett diagnostics: {}", self.component)?;
        writeln!(formatter, "Location: local profile diagnostics")?;
        writeln!(formatter, "Storage: {:?}", self.storage_status)?;
        writeln!(
            formatter,
            "Logs: {} file(s), {} byte(s), {} dropped record(s)",
            self.log_file_count, self.log_bytes, self.dropped_log_records
        )?;
        writeln!(
            formatter,
            "Previous exit: {}{}",
            self.previous_exit.as_str(),
            self.previous_error_code
                .as_deref()
                .map_or_else(String::new, |code| format!(" ({code})"))
        )?;
        writeln!(
            formatter,
            "Previous last durable phase: {}",
            self.previous_last_durable_phase
                .as_deref()
                .unwrap_or("unknown")
        )?;
        writeln!(
            formatter,
            "Previous diagnostic flush: {:?} (drop count complete: {})",
            self.previous_flush_status, self.previous_drop_count_complete
        )?;
        writeln!(
            formatter,
            "Active runs: {} (live {live}, stale {stale}, unreadable {unreadable})",
            self.active_runs.len() + self.unreadable_active_runs
        )?;
        if self.unreadable_lifecycle_records > 0 {
            writeln!(
                formatter,
                "Unreadable lifecycle records: {}",
                self.unreadable_lifecycle_records
            )?;
        }
        if self.unreadable_log_entries > 0 {
            writeln!(
                formatter,
                "Unreadable log entries: {}",
                self.unreadable_log_entries
            )?;
        }
        Ok(())
    }
}

pub fn inspect_local(
    data_dir: impl AsRef<Path>,
    component: &str,
) -> Result<DiagnosticSummary, DiagnosticsError> {
    if !valid_component(component) {
        return Err(DiagnosticsError::InvalidComponent);
    }
    let data_dir = data_dir.as_ref();
    let diagnostics_path = data_dir.join("diagnostics");
    if !data_dir.exists() {
        return Ok(empty_summary(
            component,
            diagnostics_path,
            DiagnosticStorageStatus::NotInitialized,
        ));
    }
    let profile = match SecureDir::open_existing_profile(data_dir) {
        Ok(profile) => profile,
        Err(_) => {
            return Ok(empty_summary(
                component,
                diagnostics_path,
                DiagnosticStorageStatus::Unreadable,
            ));
        }
    };
    let diagnostics_dir = match profile.open_child("diagnostics", "open_diagnostics_directory") {
        Ok(directory) => directory,
        Err(_) => {
            let status = match profile.metadata("diagnostics", "inspect_diagnostics_layout") {
                Err(DiagnosticsError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    DiagnosticStorageStatus::NotInitialized
                }
                _ => DiagnosticStorageStatus::InvalidLayout,
            };
            return Ok(empty_summary(component, diagnostics_path, status));
        }
    };
    let lifecycle_dir = match diagnostics_dir.open_child("lifecycle", "open_lifecycle_directory") {
        Ok(directory) => directory,
        Err(_) => {
            return Ok(empty_summary(
                component,
                diagnostics_path,
                DiagnosticStorageStatus::InvalidLayout,
            ));
        }
    };
    let logs_dir = match diagnostics_dir.open_child("logs", "open_log_directory") {
        Ok(directory) => directory,
        Err(_) => {
            return Ok(empty_summary(
                component,
                diagnostics_path,
                DiagnosticStorageStatus::InvalidLayout,
            ));
        }
    };
    let (active_runs, unreadable_active_runs) = inspect_active_markers(&lifecycle_dir, component)?;
    let mut unreadable_lifecycle_records = 0;
    let mut dropped_log_records = 0_usize;
    let mut terminal_run_ids = HashSet::new();
    let mut terminal = None;
    for path in terminal_paths(&lifecycle_dir, component)? {
        match read_record(&lifecycle_dir, &path) {
            Ok(record) if record.status != LifecycleStatus::Running => {
                terminal_run_ids.insert(record.run_id.clone());
                dropped_log_records =
                    dropped_log_records.saturating_add(record.dropped_log_records);
                if terminal
                    .as_ref()
                    .is_none_or(|current| terminal_is_newer(&record, current))
                {
                    terminal = Some(record);
                }
            }
            Ok(_) => unreadable_lifecycle_records += 1,
            Err(error) if is_not_found(&error) => {}
            Err(_) => unreadable_lifecycle_records += 1,
        }
    }
    for run in &active_runs {
        if !terminal_run_ids.contains(&run.run_id) {
            dropped_log_records = dropped_log_records.saturating_add(run.dropped_log_records);
        }
    }
    let (log_file_count, log_bytes, unreadable_log_entries) = inspect_logs(&logs_dir, component)?;
    let newest_terminal_is_unreadable =
        newest_terminal_file_is_unreadable(&lifecycle_dir, component, terminal.as_ref())?;
    let previous_exit = if newest_terminal_is_unreadable {
        ExitStatus::Unknown
    } else {
        terminal
            .as_ref()
            .map_or(ExitStatus::Unknown, LifecycleRecord::exit_status)
    };
    let previous_error_code = if newest_terminal_is_unreadable {
        Some("diagnostics.lifecycle_invalid".to_owned())
    } else {
        terminal
            .as_ref()
            .and_then(|record| record.error_code.clone())
    };
    let previous_last_durable_phase = if newest_terminal_is_unreadable {
        None
    } else {
        terminal
            .as_ref()
            .map(|record| record.last_durable_phase.clone())
    };
    let previous_flush_status = if newest_terminal_is_unreadable {
        DiagnosticFlushStatus::Unknown
    } else {
        terminal
            .as_ref()
            .map_or(DiagnosticFlushStatus::Unknown, |record| {
                match record.flush_status {
                    FlushStatus::Pending => DiagnosticFlushStatus::Unknown,
                    FlushStatus::Confirmed => DiagnosticFlushStatus::Confirmed,
                    FlushStatus::Incomplete => DiagnosticFlushStatus::Incomplete,
                }
            })
    };
    Ok(DiagnosticSummary {
        component: component.to_owned(),
        diagnostics_dir: diagnostics_path,
        storage_status: if unreadable_lifecycle_records > 0
            || unreadable_active_runs > 0
            || unreadable_log_entries > 0
            || newest_terminal_is_unreadable
        {
            DiagnosticStorageStatus::Degraded
        } else {
            DiagnosticStorageStatus::Available
        },
        log_file_count,
        log_bytes,
        dropped_log_records,
        active_runs,
        unreadable_active_runs,
        previous_exit,
        previous_error_code,
        previous_last_durable_phase,
        previous_flush_status,
        previous_drop_count_complete: terminal
            .as_ref()
            .is_some_and(|record| record.drop_count_complete),
        previous_clock_anomaly: terminal.as_ref().is_some_and(|record| record.clock_anomaly),
        unreadable_log_entries,
        unreadable_lifecycle_records,
    })
}

fn empty_summary(
    component: &str,
    diagnostics_dir: PathBuf,
    storage_status: DiagnosticStorageStatus,
) -> DiagnosticSummary {
    DiagnosticSummary {
        component: component.to_owned(),
        diagnostics_dir,
        storage_status,
        log_file_count: 0,
        log_bytes: 0,
        dropped_log_records: 0,
        active_runs: Vec::new(),
        unreadable_active_runs: 0,
        previous_exit: ExitStatus::Unknown,
        previous_error_code: None,
        previous_last_durable_phase: None,
        previous_flush_status: DiagnosticFlushStatus::Unknown,
        previous_drop_count_complete: false,
        previous_clock_anomaly: false,
        unreadable_log_entries: 0,
        unreadable_lifecycle_records: 0,
    }
}

impl LifecycleRecord {
    const fn exit_status(&self) -> ExitStatus {
        match self.status {
            LifecycleStatus::Running => ExitStatus::Unknown,
            LifecycleStatus::Clean => ExitStatus::Clean,
            LifecycleStatus::Failed => ExitStatus::Failed,
            LifecycleStatus::Unclean => ExitStatus::Unclean,
        }
    }

    fn validate(&self) -> Result<(), DiagnosticsError> {
        let latest_plausible_time = now_unix_ms()?.saturating_add(MAX_CLOCK_SKEW_MS);
        if self.schema_version != SCHEMA_VERSION
            || !valid_component(&self.component)
            || !valid_run_id(&self.run_id)
            || !valid_code(&self.last_durable_phase)
            || self.run_sequence == 0
            || self.started_unix_ms > latest_plausible_time
            || self
                .completed_unix_ms
                .is_some_and(|value| value > latest_plausible_time)
        {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        let terminal_time_is_valid = self
            .completed_unix_ms
            .is_some_and(|completed| completed >= self.started_unix_ms);
        let state_is_valid = match self.status {
            LifecycleStatus::Running => {
                self.completed_unix_ms.is_none()
                    && self.error_code.is_none()
                    && self.flush_status == FlushStatus::Pending
                    && !self.drop_count_complete
            }
            LifecycleStatus::Clean => {
                terminal_time_is_valid
                    && self.error_code.is_none()
                    && self.flush_status != FlushStatus::Pending
            }
            LifecycleStatus::Failed | LifecycleStatus::Unclean => {
                terminal_time_is_valid
                    && self.error_code.as_deref().is_some_and(valid_code)
                    && self.flush_status != FlushStatus::Pending
            }
        };
        if state_is_valid {
            Ok(())
        } else {
            Err(DiagnosticsError::InvalidLifecycleData)
        }
    }
}

fn reconcile_stale_markers(directory: &SecureDir, component: &str) -> Result<(), DiagnosticsError> {
    for path in matching_paths(directory, component, ".active.json")? {
        let lock_path = lock_path_for(&path);
        let Some(file) = acquire_stale_marker_lock(directory, &path, &lock_path)? else {
            continue;
        };
        let mut lock_file = Some(file);
        let (record, checkpoint_invalid) = match read_record(directory, &path) {
            Ok(record) if record.component == component => {
                effective_active_record(directory, record)?
            }
            Err(error) if is_not_found(&error) => {
                remove_active_files(directory, &path, &lock_path, &mut lock_file)?;
                continue;
            }
            Ok(_) | Err(DiagnosticsError::InvalidLifecycleData | DiagnosticsError::Json(_)) => {
                let terminal = invalid_marker_record(
                    component,
                    run_id_from_active_path(component, &path),
                    allocate_run_sequence(directory, component)?,
                )?;
                write_terminal_record(directory, &terminal)?;
                remove_checkpoints_for_run(directory, component, &terminal.run_id)?;
                remove_active_files(directory, &path, &lock_path, &mut lock_file)?;
                continue;
            }
            Err(error) => {
                let _ = unlock_only(&mut lock_file);
                return Err(error);
            }
        };
        if !terminal_record_exists(directory, &record)? {
            let mut terminal = record.clone();
            let completed = now_unix_ms()?;
            terminal.clock_anomaly = completed < terminal.started_unix_ms;
            terminal.completed_unix_ms = Some(completed.max(terminal.started_unix_ms));
            terminal.status = LifecycleStatus::Unclean;
            terminal.flush_status = FlushStatus::Incomplete;
            terminal.drop_count_complete = false;
            terminal.error_code = Some(
                if checkpoint_invalid {
                    INVALID_MARKER_CODE
                } else {
                    UNCLEAN_EXIT_CODE
                }
                .to_owned(),
            );
            write_terminal_record(directory, &terminal)?;
        }
        remove_checkpoints_for_run(directory, component, &record.run_id)?;
        remove_active_files(directory, &path, &lock_path, &mut lock_file)?;
    }
    Ok(())
}

fn acquire_stale_marker_lock(
    directory: &SecureDir,
    active_path: &Path,
    lock_path: &Path,
) -> Result<Option<File>, DiagnosticsError> {
    for _ in 0..3 {
        let lock_name = path_name(lock_path)?;
        let file = match directory.open_existing_file(lock_name, true, "open_active_lock") {
            Ok(file) => file,
            Err(error) if is_not_found(&error) => {
                if !directory.try_exists(path_name(active_path)?)? {
                    return Ok(None);
                }
                match directory.create_new_file(lock_name, true, "create_orphan_marker_lock") {
                    Ok(file) => file,
                    Err(DiagnosticsError::Io { source, .. })
                        if source.kind() == std::io::ErrorKind::AlreadyExists =>
                    {
                        continue;
                    }
                    Err(error) if is_not_found(&error) => return Ok(None),
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        return match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Some(file)),
            Err(_) => Ok(None),
        };
    }
    Ok(None)
}

fn invalid_marker_record(
    component: &str,
    recovered_run_id: Option<String>,
    run_sequence: u64,
) -> Result<LifecycleRecord, DiagnosticsError> {
    let timestamp = now_unix_ms()?;
    Ok(LifecycleRecord {
        schema_version: SCHEMA_VERSION,
        component: component.to_owned(),
        run_id: recovered_run_id.unwrap_or_else(|| Uuid::now_v7().to_string()),
        process_id: 0,
        run_sequence,
        started_unix_ms: timestamp,
        completed_unix_ms: Some(timestamp),
        status: LifecycleStatus::Unclean,
        error_code: Some(INVALID_MARKER_CODE.to_owned()),
        dropped_log_records: 0,
        last_durable_phase: "unknown".to_owned(),
        checkpoint_sequence: 0,
        clock_anomaly: false,
        flush_status: FlushStatus::Incomplete,
        drop_count_complete: false,
    })
}

fn terminal_record_exists(
    directory: &SecureDir,
    record: &LifecycleRecord,
) -> Result<bool, DiagnosticsError> {
    let prefix = format!("{}.{}.", record.component, record.run_id);
    Ok(terminal_paths(directory, &record.component)?
        .into_iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .any(|name| name.starts_with(&prefix) && !name.ends_with(".active.json")))
}

fn write_terminal_record(
    directory: &SecureDir,
    record: &LifecycleRecord,
) -> Result<(), DiagnosticsError> {
    record.validate()?;
    let path = terminal_path(record);
    atomic_write_new(directory, &path, record, "write_terminal_record", true)
}

fn atomic_write_new(
    directory: &SecureDir,
    path: &Path,
    value: &impl Serialize,
    operation: &'static str,
    existing_is_success: bool,
) -> Result<(), DiagnosticsError> {
    let target_name = path_name(path)?;
    let temp_name = format!(".{target_name}.{}.tmp", Uuid::now_v7());
    let result = (|| {
        let mut file = directory.create_new_file(&temp_name, false, operation)?;
        let mut encoded = serde_json::to_vec(value)?;
        encoded.push(b'\n');
        file.write_all(&encoded)
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        file.sync_data()
            .map_err(|source| DiagnosticsError::io("sync_lifecycle_record", source))?;
        drop(file);
        match directory.rename(&temp_name, target_name, "publish_lifecycle_record") {
            Ok(()) => Ok(()),
            Err(_) if existing_is_success && directory.try_exists(target_name)? => Ok(()),
            Err(error) => Err(error),
        }
    })();
    if result.is_err() || directory.try_exists(&temp_name).unwrap_or(false) {
        let _ = directory.remove_file(&temp_name, "remove_lifecycle_temp");
    }
    result
}

fn read_record(directory: &SecureDir, path: &Path) -> Result<LifecycleRecord, DiagnosticsError> {
    let bytes = directory.read_bounded(
        path_name(path)?,
        MAX_LIFECYCLE_RECORD_BYTES,
        "read_lifecycle_record",
    )?;
    let record: LifecycleRecord = serde_json::from_slice(&bytes)?;
    record.validate()?;
    validate_record_filename(&record, path)?;
    Ok(record)
}

fn validate_record_filename(record: &LifecycleRecord, path: &Path) -> Result<(), DiagnosticsError> {
    let name = path_name(path)?;
    let expected = if name.ends_with(".active.json") {
        if record.status != LifecycleStatus::Running || record.checkpoint_sequence != 0 {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        format!(
            "{}.{}.{:020}.active.json",
            record.component, record.run_id, record.run_sequence
        )
    } else if name.ends_with(".checkpoint.json") {
        if record.status != LifecycleStatus::Running || record.checkpoint_sequence == 0 {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        format!(
            "{}.{}.{:020}.{:020}.checkpoint.json",
            record.component, record.run_id, record.run_sequence, record.checkpoint_sequence
        )
    } else {
        if record.status == LifecycleStatus::Running {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        format!(
            "{}.{}.{:020}.{}.json",
            record.component,
            record.run_id,
            record.run_sequence,
            record.exit_status().as_str()
        )
    };
    if name == expected {
        Ok(())
    } else {
        Err(DiagnosticsError::InvalidLifecycleData)
    }
}

fn effective_active_record(
    directory: &SecureDir,
    active: LifecycleRecord,
) -> Result<(LifecycleRecord, bool), DiagnosticsError> {
    let mut effective = active;
    let mut unreadable_checkpoint = false;
    for path in checkpoint_paths_for_run(directory, &effective.component, &effective.run_id)? {
        match read_record(directory, &path) {
            Ok(checkpoint)
                if checkpoint.component == effective.component
                    && checkpoint.run_id == effective.run_id
                    && checkpoint.status == LifecycleStatus::Running =>
            {
                if checkpoint.checkpoint_sequence > effective.checkpoint_sequence {
                    effective = checkpoint;
                }
            }
            Ok(_) => unreadable_checkpoint = true,
            Err(error) if is_not_found(&error) => {}
            Err(_) => unreadable_checkpoint = true,
        }
    }
    Ok((effective, unreadable_checkpoint))
}

fn checkpoint_path(record: &LifecycleRecord) -> PathBuf {
    PathBuf::from(format!(
        "{}.{}.{:020}.{:020}.checkpoint.json",
        record.component, record.run_id, record.run_sequence, record.checkpoint_sequence
    ))
}

fn checkpoint_paths_for_run(
    directory: &SecureDir,
    component: &str,
    run_id: &str,
) -> Result<Vec<PathBuf>, DiagnosticsError> {
    let prefix = format!("{component}.{run_id}.");
    Ok(matching_paths(directory, component, ".checkpoint.json")?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect())
}

fn remove_checkpoints_for_run(
    directory: &SecureDir,
    component: &str,
    run_id: &str,
) -> Result<(), DiagnosticsError> {
    for path in checkpoint_paths_for_run(directory, component, run_id)? {
        remove_if_present(directory, &path, "remove_lifecycle_checkpoint")?;
    }
    Ok(())
}

fn latest_terminal_record(
    directory: &SecureDir,
    component: &str,
) -> Result<Option<LifecycleRecord>, DiagnosticsError> {
    let mut latest = None;
    for path in terminal_paths(directory, component)? {
        let Ok(record) = read_record(directory, &path) else {
            continue;
        };
        if latest
            .as_ref()
            .is_none_or(|current| terminal_is_newer(&record, current))
        {
            latest = Some(record);
        }
    }
    if newest_terminal_file_is_unreadable(directory, component, latest.as_ref())? {
        Ok(None)
    } else {
        Ok(latest)
    }
}

fn inspect_active_markers(
    directory: &SecureDir,
    component: &str,
) -> Result<(Vec<ActiveRunSummary>, usize), DiagnosticsError> {
    let mut summaries = Vec::new();
    let mut unreadable = 0;
    for path in matching_paths(directory, component, ".active.json")? {
        let (record, checkpoint_invalid) = match read_record(directory, &path) {
            Ok(record) if record.component == component => {
                effective_active_record(directory, record)?
            }
            Err(error) if is_not_found(&error) => continue,
            Ok(_) | Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let marker_state = if checkpoint_invalid {
            MarkerState::Unreadable
        } else {
            match directory.open_existing_file(
                path_name(&lock_path_for(&path))?,
                true,
                "inspect_active_lock",
            ) {
                Ok(file) => match FileExt::try_lock_exclusive(&file) {
                    Ok(()) => {
                        let _ = FileExt::unlock(&file);
                        MarkerState::Stale
                    }
                    Err(_) => MarkerState::Live,
                },
                Err(error) if is_not_found(&error) => MarkerState::Stale,
                Err(_) => MarkerState::Unreadable,
            }
        };
        summaries.push(ActiveRunSummary {
            run_id: record.run_id,
            process_id: record.process_id,
            started_unix_ms: record.started_unix_ms,
            last_durable_phase: record.last_durable_phase,
            dropped_log_records: record.dropped_log_records,
            marker_state,
        });
    }
    summaries.sort_by_key(|record| record.started_unix_ms);
    Ok((summaries, unreadable))
}

fn inspect_logs(
    directory: &SecureDir,
    component: &str,
) -> Result<(usize, u64, usize), DiagnosticsError> {
    let prefix = format!("{component}-diagnostic");
    let mut count = 0;
    let mut bytes = 0_u64;
    let mut unreadable = 0;
    for entry in directory.entries_bounded(MAX_LIFECYCLE_DIRECTORY_ENTRIES, "read_log_directory")? {
        let name = entry.name;
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".jsonl") {
            if entry.metadata.is_file() && !entry.metadata.file_type().is_symlink() {
                count += 1;
                bytes = bytes.saturating_add(entry.metadata.len());
            } else {
                unreadable += 1;
            }
        }
    }
    Ok((count, bytes, unreadable))
}

fn trim_terminal_records(
    directory: &SecureDir,
    component: &str,
    max_records: usize,
) -> Result<(), DiagnosticsError> {
    let mut records = terminal_paths(directory, component)?
        .into_iter()
        .map(|path| {
            let ordering = read_record(directory, &path).ok().map_or_else(
                || terminal_observation_key(&path).unwrap_or((0, String::new())),
                |record| (record.run_sequence, record.run_id),
            );
            (ordering, path)
        })
        .collect::<Vec<_>>();
    records.sort_by(|(left, _), (right, _)| left.cmp(right));
    let remove_count = records.len().saturating_sub(max_records);
    for (_, path) in records.into_iter().take(remove_count) {
        remove_if_present(directory, &path, "trim_lifecycle_record")?;
    }
    Ok(())
}

fn terminal_paths(
    directory: &SecureDir,
    component: &str,
) -> Result<Vec<PathBuf>, DiagnosticsError> {
    Ok(matching_paths(directory, component, ".json")?
        .into_iter()
        .filter(|path| is_terminal_path(path))
        .collect())
}

fn is_terminal_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.ends_with(".clean.json")
                || name.ends_with(".failed.json")
                || name.ends_with(".unclean.json")
        })
}

fn newest_terminal_file_is_unreadable(
    directory: &SecureDir,
    component: &str,
    selected: Option<&LifecycleRecord>,
) -> Result<bool, DiagnosticsError> {
    let selected_key = selected.map(|record| (record.run_sequence, record.run_id.clone()));
    let mut newest_unreadable = None;
    for path in terminal_paths(directory, component)? {
        if read_record(directory, &path).is_ok() {
            continue;
        }
        let key = match terminal_observation_key(&path) {
            Ok(key) => key,
            Err(_) => continue,
        };
        if newest_unreadable
            .as_ref()
            .is_none_or(|current| key > *current)
        {
            newest_unreadable = Some(key);
        }
    }
    Ok(match (newest_unreadable, selected_key) {
        (Some(_), None) => true,
        (Some(unreadable), Some(valid)) => unreadable >= valid,
        (None, _) => false,
    })
}

fn terminal_observation_key(path: &Path) -> Result<(u64, String), DiagnosticsError> {
    let name = path_name(path)?;
    let parts = name.split('.').collect::<Vec<_>>();
    if parts.len() != 5
        || !valid_component(parts[0])
        || !valid_run_id(parts[1])
        || parts[2].len() != 20
        || !matches!(parts[3], "clean" | "failed" | "unclean")
        || parts[4] != "json"
    {
        return Err(DiagnosticsError::InvalidLifecycleData);
    }
    let sequence = parts[2]
        .parse::<u64>()
        .map_err(|_| DiagnosticsError::InvalidLifecycleData)?;
    if sequence == 0 {
        return Err(DiagnosticsError::InvalidLifecycleData);
    }
    Ok((sequence, parts[1].to_owned()))
}

fn terminal_path(record: &LifecycleRecord) -> PathBuf {
    PathBuf::from(format!(
        "{}.{}.{:020}.{}.json",
        record.component,
        record.run_id,
        record.run_sequence,
        record.exit_status().as_str()
    ))
}

fn acquire_maintenance_lock(
    directory: &SecureDir,
    component: &str,
) -> Result<File, DiagnosticsError> {
    let name = format!("{component}.maintenance.lock");
    let file = directory.open_lock_file(&name, true, "open_lifecycle_maintenance_lock")?;
    lock_exclusive_bounded(&file, "lock_lifecycle_maintenance")?;
    Ok(file)
}

fn cleanup_auxiliary_files(directory: &SecureDir, component: &str) -> Result<(), DiagnosticsError> {
    let dotted_prefix = format!(".{component}.");
    for entry in
        directory.entries_bounded(MAX_LIFECYCLE_DIRECTORY_ENTRIES, "read_lifecycle_directory")?
    {
        let name = entry.name;
        let name = name.to_string_lossy();
        let path = PathBuf::from(name.as_ref());
        if name.starts_with(&dotted_prefix) && name.ends_with(".tmp") {
            remove_if_present(directory, &path, "remove_orphan_lifecycle_temp")?;
            continue;
        }
        if name.starts_with(&format!("{component}.")) && name.ends_with(".active.lock") {
            let active_path = path.with_extension("json");
            if directory.try_exists(path_name(&active_path)?)? {
                continue;
            }
            let file = match directory.open_existing_file(
                path_name(&path)?,
                true,
                "open_orphan_active_lock",
            ) {
                Ok(file) => file,
                Err(error) if is_not_found(&error) => continue,
                Err(error) => return Err(error),
            };
            if FileExt::try_lock_exclusive(&file).is_ok() {
                let _ = FileExt::unlock(&file);
                drop(file);
                remove_if_present(directory, &path, "remove_orphan_active_lock")?;
            }
        }
    }

    for checkpoint in matching_paths(directory, component, ".checkpoint.json")? {
        let Some(run_id) = run_id_from_checkpoint_path(component, &checkpoint) else {
            remove_if_present(
                directory,
                &checkpoint,
                "remove_invalid_lifecycle_checkpoint",
            )?;
            continue;
        };
        let active_prefix = format!("{component}.{run_id}.");
        let active_exists = matching_paths(directory, component, ".active.json")?
            .iter()
            .filter_map(|path| path.file_name()?.to_str())
            .any(|name| name.starts_with(&active_prefix));
        if !active_exists {
            remove_if_present(directory, &checkpoint, "remove_orphan_lifecycle_checkpoint")?;
        }
    }
    Ok(())
}

fn matching_paths(
    directory: &SecureDir,
    component: &str,
    suffix: &str,
) -> Result<Vec<PathBuf>, DiagnosticsError> {
    let prefix = format!("{component}.");
    let mut paths = Vec::new();
    for entry in
        directory.entries_bounded(MAX_LIFECYCLE_DIRECTORY_ENTRIES, "read_lifecycle_directory")?
    {
        let name = entry.name;
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(suffix) {
            paths.push(PathBuf::from(name.as_ref()));
        }
    }
    Ok(paths)
}

fn remove_active_files(
    directory: &SecureDir,
    active_path: &Path,
    lock_path: &Path,
    lock_file: &mut Option<File>,
) -> Result<(), DiagnosticsError> {
    unlock_only(lock_file)?;
    remove_if_present(directory, active_path, "remove_active_marker")?;
    match remove_if_present(directory, lock_path, "remove_active_lock") {
        Ok(()) => Ok(()),
        // Another startup can acquire the coordination file between our unlock and cleanup.
        // The durable terminal record and removal of the active marker already make this run
        // complete; the competing reconciler will remove the now-orphaned lock after it exits.
        Err(DiagnosticsError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::PermissionDenied
                && !directory.try_exists(path_name(active_path)?)? =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn unlock_only(lock_file: &mut Option<File>) -> Result<(), DiagnosticsError> {
    if let Some(file) = lock_file.take() {
        FileExt::unlock(&file)
            .map_err(|source| DiagnosticsError::io("unlock_active_marker", source))?;
        drop(file);
    }
    Ok(())
}

fn remove_if_present(
    directory: &SecureDir,
    path: &Path,
    operation: &'static str,
) -> Result<(), DiagnosticsError> {
    directory.remove_file(path_name(path)?, operation)
}

fn lock_path_for(active_path: &Path) -> PathBuf {
    active_path.with_extension("lock")
}

fn path_name(path: &Path) -> Result<&str, DiagnosticsError> {
    if path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Err(DiagnosticsError::InvalidDiagnosticEntry);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .ok_or(DiagnosticsError::InvalidDiagnosticEntry)
}

fn run_id_from_active_path(component: &str, path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let remainder = name
        .strip_prefix(&format!("{component}."))?
        .strip_suffix(".active.json")?;
    let run_id = remainder.split('.').next()?;
    valid_run_id(run_id).then(|| run_id.to_owned())
}

fn run_id_from_checkpoint_path(component: &str, path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let remainder = name.strip_prefix(&format!("{component}."))?;
    let run_id = remainder.split('.').next()?;
    valid_run_id(run_id).then(|| run_id.to_owned())
}

fn unknown_phase() -> String {
    "unknown".to_owned()
}

fn valid_run_id(run_id: &str) -> bool {
    Uuid::parse_str(run_id).is_ok()
}

fn allocate_run_sequence(directory: &SecureDir, component: &str) -> Result<u64, DiagnosticsError> {
    let mut highest = 0_u64;
    for path in matching_paths(directory, component, ".json")? {
        let Ok(record) = read_record(directory, &path) else {
            continue;
        };
        if record.component == component {
            highest = highest.max(record.run_sequence);
        }
    }
    highest
        .checked_add(1)
        .ok_or(DiagnosticsError::InvalidLifecycleData)
}

fn terminal_is_newer(candidate: &LifecycleRecord, current: &LifecycleRecord) -> bool {
    (candidate.run_sequence, candidate.run_id.as_str())
        > (current.run_sequence, current.run_id.as_str())
}

fn is_not_found(error: &DiagnosticsError) -> bool {
    matches!(
        error,
        DiagnosticsError::Io { source, .. }
            if source.kind() == std::io::ErrorKind::NotFound
    )
}

fn now_unix_ms() -> Result<u64, DiagnosticsError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DiagnosticsError::InvalidLifecycleData)?;
    u64::try_from(duration.as_millis()).map_err(|_| DiagnosticsError::InvalidLifecycleData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn diagnostics(temp: &tempfile::TempDir) -> (SecureDir, PathBuf) {
        let display = temp.path().join("diagnostics");
        let directory = SecureDir::open_or_create_profile(temp.path())
            .expect("profile")
            .open_or_create_child("diagnostics", "diagnostics")
            .expect("diagnostics");
        directory
            .open_or_create_child("logs", "logs")
            .expect("logs");
        (directory, display)
    }

    fn lifecycle_dir(diagnostics: &SecureDir) -> SecureDir {
        diagnostics
            .open_or_create_child("lifecycle", "lifecycle")
            .expect("lifecycle")
    }

    fn complete_clean(session: LifecycleSession, drops: usize) {
        session
            .complete(DiagnosticExit::Clean, drops, true, true)
            .expect("complete lifecycle");
    }

    #[test]
    fn dropped_session_is_reconciled_as_unclean() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        let first =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("first lifecycle");
        let first_run = first.run_id().to_owned();
        drop(first);

        let second =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("second lifecycle");
        assert_eq!(second.previous_status(), ExitStatus::Unclean);
        let lifecycle = lifecycle_dir(&diagnostics);
        assert!(
            matching_paths(&lifecycle, "dennett-node", ".unclean.json")
                .expect("unclean records")
                .iter()
                .any(|path| path_name(path).expect("record name").contains(&first_run))
        );
        complete_clean(second, 3);
        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.previous_exit, ExitStatus::Clean);
        assert_eq!(summary.dropped_log_records, 3);
        assert_eq!(summary.diagnostics_dir, display);
        assert_eq!(
            summary.previous_flush_status,
            DiagnosticFlushStatus::Confirmed
        );
        assert!(summary.previous_drop_count_complete);
    }

    #[test]
    fn crash_reconciliation_preserves_the_last_durable_phase_and_drop_count() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, _) = diagnostics(&temp);
        let first =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("first lifecycle");
        first
            .progress()
            .checkpoint("runtime_control", 7)
            .expect("durable progress");
        drop(first);

        let second =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("second lifecycle");
        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.previous_exit, ExitStatus::Unclean);
        assert_eq!(
            summary.previous_last_durable_phase.as_deref(),
            Some("runtime_control")
        );
        assert_eq!(summary.dropped_log_records, 7);
        assert_eq!(
            summary.previous_flush_status,
            DiagnosticFlushStatus::Incomplete
        );
        assert!(!summary.previous_drop_count_complete);
        complete_clean(second, 0);
    }

    #[test]
    fn corrupt_active_marker_is_recovered_without_disabling_diagnostics() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        let lifecycle = lifecycle_dir(&diagnostics);
        let marker = display
            .join("lifecycle")
            .join("dennett-node.corrupt.00000000000000000001.active.json");
        std::fs::write(&marker, b"{truncated").expect("corrupt marker");
        std::fs::write(lock_path_for(&marker), b"").expect("orphan lock");

        let session =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("recover lifecycle");
        assert_eq!(session.previous_status(), ExitStatus::Unclean);
        assert!(!marker.exists());
        complete_clean(session, 0);
        assert!(
            !matching_paths(&lifecycle, "dennett-node", ".active.json")
                .expect("active records")
                .iter()
                .any(|path| path_name(path).expect("name").contains("corrupt"))
        );
    }

    #[test]
    fn corrupt_active_marker_reuses_the_safe_run_id_from_its_filename() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        let lifecycle = lifecycle_dir(&diagnostics);
        let run_id = Uuid::now_v7().to_string();
        let marker = display.join("lifecycle").join(format!(
            "dennett-node.{run_id}.00000000000000000001.active.json"
        ));
        std::fs::write(&marker, b"{truncated").expect("corrupt marker");
        std::fs::write(lock_path_for(&marker), b"").expect("orphan lock");

        let session =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("recover lifecycle");
        assert!(
            matching_paths(&lifecycle, "dennett-node", ".unclean.json")
                .expect("unclean records")
                .iter()
                .any(|path| path_name(path).expect("record name").contains(&run_id))
        );
        complete_clean(session, 0);
    }

    #[test]
    fn startup_removes_orphan_locks_temps_and_checkpoints() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        let lifecycle = display.join("lifecycle");
        lifecycle_dir(&diagnostics);
        let run_id = Uuid::now_v7();
        let lock = lifecycle.join(format!(
            "dennett-node.{run_id}.00000000000000000001.active.lock"
        ));
        let checkpoint = lifecycle.join(format!(
            "dennett-node.{run_id}.00000000000000000001.00000000000000000001.checkpoint.json"
        ));
        let temporary = lifecycle.join(format!(
            ".dennett-node.{run_id}.clean.json.{}.tmp",
            Uuid::now_v7()
        ));
        std::fs::write(&lock, b"").expect("orphan lock");
        std::fs::write(&checkpoint, b"{}").expect("orphan checkpoint");
        std::fs::write(&temporary, b"{}").expect("orphan temporary file");

        let session =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("start lifecycle");
        assert!(!lock.exists());
        assert!(!checkpoint.exists());
        assert!(!temporary.exists());
        complete_clean(session, 0);
    }

    #[test]
    fn newest_corrupt_terminal_record_makes_doctor_report_unknown() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        LifecycleSession::start(&diagnostics, "dennett-node", 8)
            .expect("lifecycle")
            .complete(DiagnosticExit::Clean, 0, true, true)
            .expect("clean lifecycle");
        let lifecycle = display.join("lifecycle");
        std::fs::write(
            lifecycle.join(format!(
                "dennett-node.{}.00000000000000000002.failed.json",
                Uuid::now_v7()
            )),
            b"not-json",
        )
        .expect("corrupt terminal");

        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.previous_exit, ExitStatus::Unknown);
        assert_eq!(
            summary.previous_error_code.as_deref(),
            Some("diagnostics.lifecycle_invalid")
        );
        assert_eq!(summary.unreadable_lifecycle_records, 1);
    }

    #[test]
    fn cancelled_startup_leaves_no_crash_marker_or_terminal_record() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, _) = diagnostics(&temp);
        LifecycleSession::start(&diagnostics, "dennett-node", 8)
            .expect("lifecycle")
            .cancel_startup()
            .expect("cancel startup");
        let records = matching_paths(&lifecycle_dir(&diagnostics), "dennett-node", ".json")
            .expect("lifecycle records");
        assert!(records.is_empty());
    }

    #[test]
    fn concurrent_lifecycle_sessions_do_not_lose_records() {
        const RUNS: usize = 16;
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, _) = diagnostics(&temp);
        let diagnostics = Arc::new(diagnostics);
        let barrier = Arc::new(Barrier::new(RUNS));
        let handles = (0..RUNS)
            .map(|_| {
                let diagnostics = Arc::clone(&diagnostics);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    let session = LifecycleSession::start(&diagnostics, "dennett-node", RUNS + 8)?;
                    session.complete(DiagnosticExit::Clean, 0, true, true)
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle
                .join()
                .expect("lifecycle thread")
                .expect("concurrent lifecycle");
        }
        let records = matching_paths(&lifecycle_dir(&diagnostics), "dennett-node", ".clean.json")
            .expect("terminal records");
        assert_eq!(records.len(), RUNS);
    }

    #[test]
    fn terminal_retention_bounds_valid_and_corrupt_records() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        lifecycle_dir(&diagnostics);
        std::fs::write(
            display
                .join("lifecycle")
                .join("dennett-node.corrupt.00000000000000000000.failed.json"),
            b"not-json",
        )
        .expect("corrupt historical record");
        for _ in 0..5 {
            LifecycleSession::start(&diagnostics, "dennett-node", 3)
                .expect("lifecycle")
                .complete(DiagnosticExit::Clean, 0, true, true)
                .expect("complete lifecycle");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let records = matching_paths(&lifecycle_dir(&diagnostics), "dennett-node", ".json")
            .expect("lifecycle files");
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn lifecycle_validation_rejects_control_characters_and_invalid_state() {
        let mut record = LifecycleRecord {
            schema_version: SCHEMA_VERSION,
            component: "dennett-node".to_owned(),
            run_id: Uuid::now_v7().to_string(),
            process_id: 1,
            run_sequence: 1,
            started_unix_ms: 1,
            completed_unix_ms: Some(2),
            status: LifecycleStatus::Failed,
            error_code: Some("provider\nsecret".to_owned()),
            dropped_log_records: 0,
            last_durable_phase: "runtime".to_owned(),
            checkpoint_sequence: 0,
            clock_anomaly: false,
            flush_status: FlushStatus::Confirmed,
            drop_count_complete: true,
        };
        assert!(record.validate().is_err());
        record.error_code = Some("provider.unavailable".to_owned());
        record.completed_unix_ms = None;
        assert!(record.validate().is_err());
    }

    #[test]
    fn latest_terminal_record_has_a_deterministic_tie_break() {
        let mut older = LifecycleRecord {
            schema_version: SCHEMA_VERSION,
            component: "dennett-node".to_owned(),
            run_id: "00000000-0000-7000-8000-000000000001".to_owned(),
            process_id: 1,
            run_sequence: 1,
            started_unix_ms: 1,
            completed_unix_ms: Some(3),
            status: LifecycleStatus::Clean,
            error_code: None,
            dropped_log_records: 0,
            last_durable_phase: "shutdown".to_owned(),
            checkpoint_sequence: 0,
            clock_anomaly: false,
            flush_status: FlushStatus::Confirmed,
            drop_count_complete: true,
        };
        let mut newer = older.clone();
        newer.run_sequence = 2;
        assert!(terminal_is_newer(&newer, &older));

        older.run_sequence = 2;
        newer.run_id = "00000000-0000-7000-8000-000000000002".to_owned();
        assert!(terminal_is_newer(&newer, &older));
        assert!(!terminal_is_newer(&older, &newer));
    }

    #[test]
    fn doctor_reports_wrong_type_storage_roots_explicitly() {
        let diagnostics_file = tempfile::tempdir().expect("diagnostics file root");
        std::fs::write(
            diagnostics_file.path().join("diagnostics"),
            b"not a directory",
        )
        .expect("diagnostics file");
        assert_eq!(
            inspect_local(diagnostics_file.path(), "dennett-node")
                .expect("diagnostics status")
                .storage_status,
            DiagnosticStorageStatus::InvalidLayout
        );

        let lifecycle_file = tempfile::tempdir().expect("lifecycle file root");
        std::fs::create_dir(lifecycle_file.path().join("diagnostics"))
            .expect("diagnostics directory");
        std::fs::write(
            lifecycle_file.path().join("diagnostics/lifecycle"),
            b"not a directory",
        )
        .expect("lifecycle file");
        std::fs::create_dir(lifecycle_file.path().join("diagnostics/logs"))
            .expect("logs directory");
        assert_eq!(
            inspect_local(lifecycle_file.path(), "dennett-node")
                .expect("lifecycle status")
                .storage_status,
            DiagnosticStorageStatus::InvalidLayout
        );

        let logs_file = tempfile::tempdir().expect("logs file root");
        std::fs::create_dir_all(logs_file.path().join("diagnostics/lifecycle"))
            .expect("lifecycle directory");
        std::fs::write(
            logs_file.path().join("diagnostics/logs"),
            b"not a directory",
        )
        .expect("logs file");
        assert_eq!(
            inspect_local(logs_file.path(), "dennett-node")
                .expect("logs status")
                .storage_status,
            DiagnosticStorageStatus::InvalidLayout
        );
    }

    #[test]
    fn doctor_degrades_when_a_matching_log_entry_is_not_a_file() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        lifecycle_dir(&diagnostics);
        std::fs::create_dir(display.join("logs/dennett-node-diagnostic.fake.jsonl"))
            .expect("wrong-type log entry");

        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.storage_status, DiagnosticStorageStatus::Degraded);
        assert_eq!(summary.unreadable_log_entries, 1);
        assert_eq!(summary.log_file_count, 0);
    }

    #[test]
    fn filename_binding_rejects_a_crafted_terminal_record() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        let session = LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("lifecycle");
        complete_clean(session, 0);

        let valid_path =
            matching_paths(&lifecycle_dir(&diagnostics), "dennett-node", ".clean.json")
                .expect("terminal records")
                .into_iter()
                .next()
                .expect("valid terminal");
        let bytes =
            std::fs::read(display.join("lifecycle").join(&valid_path)).expect("terminal bytes");
        std::fs::write(
            display.join("lifecycle").join(format!(
                "dennett-node.{}.18446744073709551615.clean.json",
                Uuid::now_v7()
            )),
            bytes,
        )
        .expect("crafted terminal");

        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.storage_status, DiagnosticStorageStatus::Degraded);
        assert_eq!(summary.previous_exit, ExitStatus::Unknown);
        assert_eq!(summary.unreadable_lifecycle_records, 1);
    }

    #[test]
    fn lifecycle_directory_enumeration_is_bounded() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let (diagnostics, display) = diagnostics(&temp);
        lifecycle_dir(&diagnostics);
        for index in 0..=MAX_LIFECYCLE_DIRECTORY_ENTRIES {
            std::fs::write(
                display.join("lifecycle").join(format!("noise-{index:04}")),
                b"x",
            )
            .expect("bounded fixture");
        }
        assert!(matches!(
            inspect_local(temp.path(), "dennett-node"),
            Err(DiagnosticsError::DiagnosticEntryLimit)
        ));
    }

    #[test]
    fn terminal_ordering_rejects_noncanonical_sequence_width() {
        let run_id = Uuid::now_v7();
        assert!(matches!(
            terminal_observation_key(Path::new(&format!("dennett-node.{run_id}.1.clean.json"))),
            Err(DiagnosticsError::InvalidLifecycleData)
        ));
    }
}
