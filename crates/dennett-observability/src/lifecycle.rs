use crate::{DiagnosticsError, valid_code, valid_component, writer::ensure_within_root};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fmt,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
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
struct LifecycleRecord {
    schema_version: u32,
    component: String,
    run_id: String,
    process_id: u32,
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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LifecycleStatus {
    Running,
    Clean,
    Failed,
    Unclean,
}

pub(crate) struct LifecycleSession {
    directory: PathBuf,
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
    directory: PathBuf,
    state: Arc<Mutex<LifecycleProgressState>>,
}

impl LifecycleSession {
    pub(crate) fn start(
        diagnostics_dir: &Path,
        component: &str,
        max_records: usize,
    ) -> Result<Self, DiagnosticsError> {
        let directory = diagnostics_dir.join("lifecycle");
        std::fs::create_dir_all(&directory)
            .map_err(|source| DiagnosticsError::io("create_lifecycle_directory", source))?;
        ensure_within_root(diagnostics_dir, &directory)?;
        let _maintenance = acquire_maintenance_lock(&directory, component)?;
        cleanup_auxiliary_files(&directory, component)?;
        reconcile_stale_markers(&directory, component)?;
        trim_terminal_records(&directory, component, max_records)?;
        let previous_status = latest_terminal_record(&directory, component)?
            .map_or(ExitStatus::Unknown, |record| record.exit_status());
        let process_id = std::process::id();
        let started_unix_ms = now_unix_ms()?;
        let run_id = Uuid::now_v7().to_string();
        let record = LifecycleRecord {
            schema_version: SCHEMA_VERSION,
            component: component.to_owned(),
            run_id: run_id.clone(),
            process_id,
            started_unix_ms,
            completed_unix_ms: None,
            status: LifecycleStatus::Running,
            error_code: None,
            dropped_log_records: 0,
            last_durable_phase: "startup".to_owned(),
            checkpoint_sequence: 0,
        };
        let active_path = directory.join(format!("{component}.{run_id}.active.json"));
        let active_lock_path = lock_path_for(&active_path);
        let active_lock_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&active_lock_path)
            .map_err(|source| DiagnosticsError::io("create_active_lock", source))?;
        FileExt::try_lock_exclusive(&active_lock_file)
            .map_err(|source| DiagnosticsError::io("lock_active_marker", source))?;
        if let Err(error) = atomic_write_new(&active_path, &record, "write_active_marker", false) {
            let _ = FileExt::unlock(&active_lock_file);
            drop(active_lock_file);
            let _ = std::fs::remove_file(&active_lock_path);
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
        state.record.completed_unix_ms = Some(now_unix_ms()?);
        state.record.status = status;
        state.record.error_code = error_code;
        state.record.dropped_log_records = dropped_log_records;
        state.record.last_durable_phase = "shutdown".to_owned();
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

    pub(crate) fn note_dropped_records(
        &self,
        dropped_log_records: usize,
    ) -> Result<(), DiagnosticsError> {
        let phase = {
            let state = self
                .state
                .lock()
                .map_err(|_| DiagnosticsError::InvalidLifecycleData)?;
            if state.completed || dropped_log_records <= state.record.dropped_log_records {
                return Ok(());
            }
            state.record.last_durable_phase.clone()
        };
        self.checkpoint_owned_phase(phase, dropped_log_records)
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
    directory: &Path,
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
    let path = checkpoint_path(directory, &checkpoint);
    atomic_write_new(&path, &checkpoint, "write_lifecycle_checkpoint", false)?;
    let previous = state.latest_checkpoint_path.replace(path);
    state.record = checkpoint;
    if let Some(previous) = previous {
        remove_if_present(&previous, "remove_lifecycle_checkpoint")?;
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiagnosticSummary {
    pub component: String,
    pub diagnostics_dir: PathBuf,
    pub log_file_count: usize,
    pub log_bytes: u64,
    pub dropped_log_records: usize,
    pub active_runs: Vec<ActiveRunSummary>,
    pub unreadable_active_runs: usize,
    pub previous_exit: ExitStatus,
    pub previous_error_code: Option<String>,
    pub previous_last_durable_phase: Option<String>,
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
    let diagnostics_dir = data_dir.join("diagnostics");
    let lifecycle_dir = diagnostics_dir.join("lifecycle");
    let logs_dir = diagnostics_dir.join("logs");
    if diagnostics_dir.exists() {
        ensure_within_root(data_dir, &diagnostics_dir)?;
    }
    if lifecycle_dir.exists() {
        ensure_within_root(&diagnostics_dir, &lifecycle_dir)?;
    }
    if logs_dir.exists() {
        ensure_within_root(&diagnostics_dir, &logs_dir)?;
    }
    let (active_runs, unreadable_active_runs) = inspect_active_markers(&lifecycle_dir, component)?;
    let mut unreadable_lifecycle_records = 0;
    let mut dropped_log_records = 0_usize;
    let mut terminal_run_ids = HashSet::new();
    let mut terminal = None;
    if lifecycle_dir.is_dir() {
        for path in terminal_paths(&lifecycle_dir, component)? {
            match read_record(&path) {
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
    }
    for run in &active_runs {
        if !terminal_run_ids.contains(&run.run_id) {
            dropped_log_records = dropped_log_records.saturating_add(run.dropped_log_records);
        }
    }
    let (log_file_count, log_bytes) = inspect_logs(&logs_dir, component)?;
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
        terminal.map(|record| record.last_durable_phase)
    };
    Ok(DiagnosticSummary {
        component: component.to_owned(),
        diagnostics_dir,
        log_file_count,
        log_bytes,
        dropped_log_records,
        active_runs,
        unreadable_active_runs,
        previous_exit,
        previous_error_code,
        previous_last_durable_phase,
        unreadable_lifecycle_records,
    })
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
        if self.schema_version != SCHEMA_VERSION
            || !valid_component(&self.component)
            || !valid_run_id(&self.run_id)
            || !valid_code(&self.last_durable_phase)
        {
            return Err(DiagnosticsError::InvalidLifecycleData);
        }
        let terminal_time_is_valid = self
            .completed_unix_ms
            .is_some_and(|completed| completed >= self.started_unix_ms);
        let state_is_valid = match self.status {
            LifecycleStatus::Running => {
                self.completed_unix_ms.is_none() && self.error_code.is_none()
            }
            LifecycleStatus::Clean => terminal_time_is_valid && self.error_code.is_none(),
            LifecycleStatus::Failed | LifecycleStatus::Unclean => {
                terminal_time_is_valid && self.error_code.as_deref().is_some_and(valid_code)
            }
        };
        if state_is_valid {
            Ok(())
        } else {
            Err(DiagnosticsError::InvalidLifecycleData)
        }
    }
}

fn reconcile_stale_markers(directory: &Path, component: &str) -> Result<(), DiagnosticsError> {
    for path in matching_paths(directory, component, ".active.json")? {
        let lock_path = lock_path_for(&path);
        let Some(file) = acquire_stale_marker_lock(&path, &lock_path)? else {
            continue;
        };
        let mut lock_file = Some(file);
        let (record, checkpoint_invalid) = match read_record(&path) {
            Ok(record) if record.component == component => {
                effective_active_record(directory, record)?
            }
            Err(error) if is_not_found(&error) => {
                remove_active_files(&path, &lock_path, &mut lock_file)?;
                continue;
            }
            Ok(_) | Err(DiagnosticsError::InvalidLifecycleData | DiagnosticsError::Json(_)) => {
                let terminal =
                    invalid_marker_record(component, run_id_from_active_path(component, &path))?;
                write_terminal_record(directory, &terminal)?;
                remove_checkpoints_for_run(directory, component, &terminal.run_id)?;
                remove_active_files(&path, &lock_path, &mut lock_file)?;
                continue;
            }
            Err(error) => {
                let _ = unlock_only(&mut lock_file);
                return Err(error);
            }
        };
        if !terminal_record_exists(directory, &record)? {
            let mut terminal = record.clone();
            terminal.completed_unix_ms = Some(now_unix_ms()?);
            terminal.status = LifecycleStatus::Unclean;
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
        remove_active_files(&path, &lock_path, &mut lock_file)?;
    }
    Ok(())
}

fn acquire_stale_marker_lock(
    active_path: &Path,
    lock_path: &Path,
) -> Result<Option<File>, DiagnosticsError> {
    for _ in 0..3 {
        let file = match OpenOptions::new().read(true).write(true).open(lock_path) {
            Ok(file) => file,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                if !active_path.exists() {
                    return Ok(None);
                }
                match OpenOptions::new()
                    .create_new(true)
                    .read(true)
                    .write(true)
                    .open(lock_path)
                {
                    Ok(file) => file,
                    Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(None);
                    }
                    Err(source) => {
                        return Err(DiagnosticsError::io("create_orphan_marker_lock", source));
                    }
                }
            }
            Err(source) => return Err(DiagnosticsError::io("open_active_lock", source)),
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
) -> Result<LifecycleRecord, DiagnosticsError> {
    let timestamp = now_unix_ms()?;
    Ok(LifecycleRecord {
        schema_version: SCHEMA_VERSION,
        component: component.to_owned(),
        run_id: recovered_run_id.unwrap_or_else(|| Uuid::now_v7().to_string()),
        process_id: 0,
        started_unix_ms: timestamp,
        completed_unix_ms: Some(timestamp),
        status: LifecycleStatus::Unclean,
        error_code: Some(INVALID_MARKER_CODE.to_owned()),
        dropped_log_records: 0,
        last_durable_phase: "unknown".to_owned(),
        checkpoint_sequence: 0,
    })
}

fn terminal_record_exists(
    directory: &Path,
    record: &LifecycleRecord,
) -> Result<bool, DiagnosticsError> {
    let prefix = format!("{}.{}.", record.component, record.run_id);
    Ok(terminal_paths(directory, &record.component)?
        .into_iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .any(|name| name.starts_with(&prefix) && !name.ends_with(".active.json")))
}

fn write_terminal_record(
    directory: &Path,
    record: &LifecycleRecord,
) -> Result<(), DiagnosticsError> {
    record.validate()?;
    let path = terminal_path(directory, record);
    atomic_write_new(&path, record, "write_terminal_record", true)
}

fn atomic_write_new(
    path: &Path,
    value: &impl Serialize,
    operation: &'static str,
    existing_is_success: bool,
) -> Result<(), DiagnosticsError> {
    let parent = path
        .parent()
        .ok_or(DiagnosticsError::InvalidLifecycleData)?;
    let target_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(DiagnosticsError::InvalidLifecycleData)?;
    let temp_path = parent.join(format!(".{target_name}.{}.tmp", Uuid::now_v7()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        let mut encoded = serde_json::to_vec(value)?;
        encoded.push(b'\n');
        file.write_all(&encoded)
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        file.sync_data()
            .map_err(|source| DiagnosticsError::io("sync_lifecycle_record", source))?;
        drop(file);
        match std::fs::rename(&temp_path, path) {
            Ok(()) => Ok(()),
            Err(_) if existing_is_success && path.is_file() => Ok(()),
            Err(source) => Err(DiagnosticsError::io("publish_lifecycle_record", source)),
        }
    })();
    if result.is_err() || temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn read_record(path: &Path) -> Result<LifecycleRecord, DiagnosticsError> {
    let bytes = std::fs::read(path)
        .map_err(|source| DiagnosticsError::io("read_lifecycle_record", source))?;
    let record: LifecycleRecord = serde_json::from_slice(&bytes)?;
    record.validate()?;
    Ok(record)
}

fn effective_active_record(
    directory: &Path,
    active: LifecycleRecord,
) -> Result<(LifecycleRecord, bool), DiagnosticsError> {
    let mut effective = active;
    let mut unreadable_checkpoint = false;
    for path in checkpoint_paths_for_run(directory, &effective.component, &effective.run_id)? {
        match read_record(&path) {
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

fn checkpoint_path(directory: &Path, record: &LifecycleRecord) -> PathBuf {
    directory.join(format!(
        "{}.{}.{:020}.checkpoint.json",
        record.component, record.run_id, record.checkpoint_sequence
    ))
}

fn checkpoint_paths_for_run(
    directory: &Path,
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
    directory: &Path,
    component: &str,
    run_id: &str,
) -> Result<(), DiagnosticsError> {
    for path in checkpoint_paths_for_run(directory, component, run_id)? {
        remove_if_present(&path, "remove_lifecycle_checkpoint")?;
    }
    Ok(())
}

fn latest_terminal_record(
    directory: &Path,
    component: &str,
) -> Result<Option<LifecycleRecord>, DiagnosticsError> {
    let mut latest = None;
    for path in terminal_paths(directory, component)? {
        let Ok(record) = read_record(&path) else {
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
    directory: &Path,
    component: &str,
) -> Result<(Vec<ActiveRunSummary>, usize), DiagnosticsError> {
    if !directory.is_dir() {
        return Ok((Vec::new(), 0));
    }
    let mut summaries = Vec::new();
    let mut unreadable = 0;
    for path in matching_paths(directory, component, ".active.json")? {
        let (record, checkpoint_invalid) = match read_record(&path) {
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
            match OpenOptions::new()
                .read(true)
                .write(true)
                .open(lock_path_for(&path))
            {
                Ok(file) => match FileExt::try_lock_exclusive(&file) {
                    Ok(()) => {
                        let _ = FileExt::unlock(&file);
                        MarkerState::Stale
                    }
                    Err(_) => MarkerState::Live,
                },
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => MarkerState::Stale,
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

fn inspect_logs(directory: &Path, component: &str) -> Result<(usize, u64), DiagnosticsError> {
    if !directory.is_dir() {
        return Ok((0, 0));
    }
    let prefix = format!("{component}-diagnostic");
    let mut count = 0;
    let mut bytes = 0_u64;
    for entry in std::fs::read_dir(directory)
        .map_err(|source| DiagnosticsError::io("read_log_directory", source))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("read_log_entry", source)),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".jsonl") {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => return Err(DiagnosticsError::io("read_log_metadata", source)),
            };
            if metadata.is_file() {
                count += 1;
                bytes = bytes.saturating_add(metadata.len());
            }
        }
    }
    Ok((count, bytes))
}

fn trim_terminal_records(
    directory: &Path,
    component: &str,
    max_records: usize,
) -> Result<(), DiagnosticsError> {
    let mut records = terminal_paths(directory, component)?
        .into_iter()
        .map(|path| {
            let completed = read_record(&path)
                .ok()
                .and_then(|record| record.completed_unix_ms)
                .or_else(|| {
                    path.metadata()
                        .ok()?
                        .modified()
                        .ok()?
                        .duration_since(UNIX_EPOCH)
                        .ok()
                        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
                })
                .unwrap_or(0);
            (completed, path)
        })
        .collect::<Vec<_>>();
    records.sort_by_key(|(completed, _)| *completed);
    let remove_count = records.len().saturating_sub(max_records);
    for (_, path) in records.into_iter().take(remove_count) {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(DiagnosticsError::io("trim_lifecycle_record", source)),
        }
    }
    Ok(())
}

fn terminal_paths(directory: &Path, component: &str) -> Result<Vec<PathBuf>, DiagnosticsError> {
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
    directory: &Path,
    component: &str,
    selected: Option<&LifecycleRecord>,
) -> Result<bool, DiagnosticsError> {
    let selected_key =
        selected.and_then(|record| file_observation_key(&terminal_path(directory, record)).ok());
    let mut newest_unreadable = None;
    for path in terminal_paths(directory, component)? {
        if read_record(&path).is_ok() {
            continue;
        }
        let key = match file_observation_key(&path) {
            Ok(key) => key,
            Err(error) if is_not_found(&error) => continue,
            Err(error) => return Err(error),
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

fn file_observation_key(path: &Path) -> Result<(u128, String), DiagnosticsError> {
    let metadata = path
        .metadata()
        .map_err(|source| DiagnosticsError::io("read_lifecycle_metadata", source))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis());
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(DiagnosticsError::InvalidLifecycleData)?
        .to_owned();
    Ok((modified, name))
}

fn terminal_path(directory: &Path, record: &LifecycleRecord) -> PathBuf {
    directory.join(format!(
        "{}.{}.{}.json",
        record.component,
        record.run_id,
        record.exit_status().as_str()
    ))
}

fn acquire_maintenance_lock(directory: &Path, component: &str) -> Result<File, DiagnosticsError> {
    let path = directory.join(format!("{component}.maintenance.lock"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| DiagnosticsError::io("open_lifecycle_maintenance_lock", source))?;
    FileExt::lock_exclusive(&file)
        .map_err(|source| DiagnosticsError::io("lock_lifecycle_maintenance", source))?;
    Ok(file)
}

fn cleanup_auxiliary_files(directory: &Path, component: &str) -> Result<(), DiagnosticsError> {
    let dotted_prefix = format!(".{component}.");
    for entry in std::fs::read_dir(directory)
        .map_err(|source| DiagnosticsError::io("read_lifecycle_directory", source))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("read_lifecycle_entry", source)),
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&dotted_prefix) && name.ends_with(".tmp") {
            remove_if_present(&path, "remove_orphan_lifecycle_temp")?;
            continue;
        }
        if name.starts_with(&format!("{component}.")) && name.ends_with(".active.lock") {
            let active_path = path.with_extension("json");
            if active_path.exists() {
                continue;
            }
            let file = match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(file) => file,
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(DiagnosticsError::io("open_orphan_active_lock", source));
                }
            };
            if FileExt::try_lock_exclusive(&file).is_ok() {
                let _ = FileExt::unlock(&file);
                drop(file);
                remove_if_present(&path, "remove_orphan_active_lock")?;
            }
        }
    }

    for checkpoint in matching_paths(directory, component, ".checkpoint.json")? {
        let Some(run_id) = run_id_from_checkpoint_path(component, &checkpoint) else {
            remove_if_present(&checkpoint, "remove_invalid_lifecycle_checkpoint")?;
            continue;
        };
        let active = directory.join(format!("{component}.{run_id}.active.json"));
        if !active.exists() {
            remove_if_present(&checkpoint, "remove_orphan_lifecycle_checkpoint")?;
        }
    }
    Ok(())
}

fn matching_paths(
    directory: &Path,
    component: &str,
    suffix: &str,
) -> Result<Vec<PathBuf>, DiagnosticsError> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let prefix = format!("{component}.");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)
        .map_err(|source| DiagnosticsError::io("read_lifecycle_directory", source))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("read_lifecycle_entry", source)),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(suffix) {
            paths.push(entry.path());
        }
    }
    Ok(paths)
}

fn remove_active_files(
    active_path: &Path,
    lock_path: &Path,
    lock_file: &mut Option<File>,
) -> Result<(), DiagnosticsError> {
    unlock_only(lock_file)?;
    remove_if_present(active_path, "remove_active_marker")?;
    match std::fs::remove_file(lock_path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        // Another startup can acquire the coordination file between our unlock and cleanup.
        // The durable terminal record and removal of the active marker already make this run
        // complete; the competing reconciler will remove the now-orphaned lock after it exits.
        Err(source)
            if source.kind() == std::io::ErrorKind::PermissionDenied && !active_path.exists() =>
        {
            Ok(())
        }
        Err(source) => Err(DiagnosticsError::io("remove_active_lock", source)),
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

fn remove_if_present(path: &Path, operation: &'static str) -> Result<(), DiagnosticsError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(DiagnosticsError::io(operation, source)),
    }
}

fn lock_path_for(active_path: &Path) -> PathBuf {
    active_path.with_extension("lock")
}

fn run_id_from_active_path(component: &str, path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let run_id = name
        .strip_prefix(&format!("{component}."))?
        .strip_suffix(".active.json")?;
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

fn terminal_is_newer(candidate: &LifecycleRecord, current: &LifecycleRecord) -> bool {
    (
        candidate.completed_unix_ms,
        candidate.started_unix_ms,
        candidate.run_id.as_str(),
    ) > (
        current.completed_unix_ms,
        current.started_unix_ms,
        current.run_id.as_str(),
    )
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
    use std::{
        sync::{Arc, Barrier},
        time::Duration,
    };

    #[test]
    fn dropped_session_is_reconciled_as_unclean() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let first =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("first lifecycle");
        let first_run = first.run_id().to_owned();
        drop(first);

        let second =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("second lifecycle");
        assert_eq!(second.previous_status(), ExitStatus::Unclean);
        let lifecycle = diagnostics.join("lifecycle");
        assert!(
            lifecycle
                .join(format!("dennett-node.{first_run}.unclean.json"))
                .is_file()
        );
        second
            .complete(DiagnosticExit::Clean, 3)
            .expect("clean second lifecycle");
        let summary = inspect_local(temp.path(), "dennett-node").expect("diagnostic summary");
        assert_eq!(summary.previous_exit, ExitStatus::Clean);
        assert_eq!(summary.dropped_log_records, 3);
    }

    #[test]
    fn crash_reconciliation_preserves_the_last_durable_phase_and_drop_count() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
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
        second
            .complete(DiagnosticExit::Clean, 0)
            .expect("complete lifecycle");
    }

    #[test]
    fn corrupt_active_marker_is_recovered_without_disabling_diagnostics() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let lifecycle = diagnostics.join("lifecycle");
        std::fs::create_dir_all(&lifecycle).expect("lifecycle directory");
        let marker = lifecycle.join("dennett-node.corrupt.active.json");
        std::fs::write(&marker, b"{truncated").expect("corrupt marker");
        std::fs::write(lock_path_for(&marker), b"").expect("orphan lock");

        let session =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("recover lifecycle");
        assert_eq!(session.previous_status(), ExitStatus::Unclean);
        assert!(!marker.exists());
        session
            .complete(DiagnosticExit::Clean, 0)
            .expect("complete recovered lifecycle");
    }

    #[test]
    fn corrupt_active_marker_reuses_the_safe_run_id_from_its_filename() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let lifecycle = diagnostics.join("lifecycle");
        std::fs::create_dir_all(&lifecycle).expect("lifecycle directory");
        let run_id = Uuid::now_v7().to_string();
        let marker = lifecycle.join(format!("dennett-node.{run_id}.active.json"));
        std::fs::write(&marker, b"{truncated").expect("corrupt marker");
        std::fs::write(lock_path_for(&marker), b"").expect("orphan lock");

        let session =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("recover lifecycle");
        assert!(
            lifecycle
                .join(format!("dennett-node.{run_id}.unclean.json"))
                .is_file()
        );
        session
            .complete(DiagnosticExit::Clean, 0)
            .expect("complete recovered lifecycle");
    }

    #[test]
    fn startup_removes_orphan_locks_temps_and_checkpoints() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let lifecycle = diagnostics.join("lifecycle");
        std::fs::create_dir_all(&lifecycle).expect("lifecycle directory");
        let run_id = Uuid::now_v7();
        let lock = lifecycle.join(format!("dennett-node.{run_id}.active.lock"));
        let checkpoint = lifecycle.join(format!(
            "dennett-node.{run_id}.00000000000000000001.checkpoint.json"
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
        session
            .complete(DiagnosticExit::Clean, 0)
            .expect("complete lifecycle");
    }

    #[test]
    fn newest_corrupt_terminal_record_makes_doctor_report_unknown() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        LifecycleSession::start(&diagnostics, "dennett-node", 8)
            .expect("lifecycle")
            .complete(DiagnosticExit::Clean, 0)
            .expect("clean lifecycle");
        std::thread::sleep(Duration::from_millis(2));
        let lifecycle = diagnostics.join("lifecycle");
        std::fs::write(
            lifecycle.join(format!("dennett-node.{}.failed.json", Uuid::now_v7())),
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
        let diagnostics = temp.path().join("diagnostics");
        LifecycleSession::start(&diagnostics, "dennett-node", 8)
            .expect("lifecycle")
            .cancel_startup()
            .expect("cancel startup");
        let records = matching_paths(&diagnostics.join("lifecycle"), "dennett-node", ".json")
            .expect("lifecycle records");
        assert!(records.is_empty());
    }

    #[test]
    fn concurrent_lifecycle_sessions_do_not_lose_records() {
        const RUNS: usize = 48;
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = Arc::new(temp.path().join("diagnostics"));
        let barrier = Arc::new(Barrier::new(RUNS));
        let handles = (0..RUNS)
            .map(|_| {
                let diagnostics = Arc::clone(&diagnostics);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let session = LifecycleSession::start(&diagnostics, "dennett-node", RUNS + 8)?;
                    barrier.wait();
                    session.complete(DiagnosticExit::Clean, 0)
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle
                .join()
                .expect("lifecycle thread")
                .expect("concurrent lifecycle");
        }
        let records = matching_paths(
            &diagnostics.join("lifecycle"),
            "dennett-node",
            ".clean.json",
        )
        .expect("terminal records");
        assert_eq!(records.len(), RUNS);
    }

    #[test]
    fn terminal_retention_bounds_valid_and_corrupt_records() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        std::fs::create_dir_all(diagnostics.join("lifecycle")).expect("lifecycle directory");
        std::fs::write(
            diagnostics
                .join("lifecycle")
                .join("dennett-node.corrupt.failed.json"),
            b"not-json",
        )
        .expect("corrupt historical record");
        for _ in 0..5 {
            LifecycleSession::start(&diagnostics, "dennett-node", 3)
                .expect("lifecycle")
                .complete(DiagnosticExit::Clean, 0)
                .expect("complete lifecycle");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let records = matching_paths(&diagnostics.join("lifecycle"), "dennett-node", ".json")
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
            started_unix_ms: 1,
            completed_unix_ms: Some(2),
            status: LifecycleStatus::Failed,
            error_code: Some("provider\nsecret".to_owned()),
            dropped_log_records: 0,
            last_durable_phase: "runtime".to_owned(),
            checkpoint_sequence: 0,
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
            started_unix_ms: 1,
            completed_unix_ms: Some(3),
            status: LifecycleStatus::Clean,
            error_code: None,
            dropped_log_records: 0,
            last_durable_phase: "shutdown".to_owned(),
            checkpoint_sequence: 0,
        };
        let mut newer = older.clone();
        newer.started_unix_ms = 2;
        assert!(terminal_is_newer(&newer, &older));

        older.started_unix_ms = 2;
        newer.run_id = "00000000-0000-7000-8000-000000000002".to_owned();
        assert!(terminal_is_newer(&newer, &older));
        assert!(!terminal_is_newer(&older, &newer));
    }
}
