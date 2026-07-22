use crate::{DiagnosticsError, lifecycle::LifecycleProgress};
use fs2::FileExt;
use std::{
    fs::{File, FileType, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing_appender::non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard};
use uuid::Uuid;

pub(crate) struct PreparedWriter {
    pub(crate) writer: NonBlocking,
    pub(crate) guard: WorkerGuard,
    pub(crate) health: WriterHealth,
}

#[derive(Clone)]
pub(crate) struct WriterHealth {
    queue_drops: ErrorCounter,
    storage_drops: Arc<AtomicUsize>,
}

impl WriterHealth {
    pub(crate) fn dropped_records(&self) -> usize {
        self.queue_drops
            .dropped_lines()
            .saturating_add(self.storage_drops.load(Ordering::Relaxed))
    }
}

pub(crate) fn prepare_writer(
    data_dir: &Path,
    diagnostics_dir: &Path,
    component: &str,
    max_log_files: usize,
    max_log_age: Duration,
    max_log_bytes: u64,
    progress: LifecycleProgress,
) -> Result<PreparedWriter, DiagnosticsError> {
    let logs_dir = diagnostics_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .map_err(|source| DiagnosticsError::io("create_diagnostics_directory", source))?;
    ensure_within_root(data_dir, diagnostics_dir)?;
    ensure_within_root(data_dir, &logs_dir)?;
    let maintenance_lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(log_maintenance_path(&logs_dir, component))
        .map_err(|source| DiagnosticsError::io("open_log_maintenance_lock", source))?;
    FileExt::lock_exclusive(&maintenance_lock)
        .map_err(|source| DiagnosticsError::io("lock_log_maintenance", source))?;
    let prune_result = prune_logs(
        &logs_dir,
        component,
        max_log_age,
        max_log_files,
        max_log_bytes,
        SystemTime::now(),
        None,
    );
    let unlock_result = FileExt::unlock(&maintenance_lock)
        .map_err(|source| DiagnosticsError::io("unlock_log_maintenance", source));
    prune_result?;
    unlock_result?;

    let storage_drops = Arc::new(AtomicUsize::new(0));
    let bounded = BoundedRollingWriter {
        directory: logs_dir,
        component: component.to_owned(),
        max_files: max_log_files,
        max_age: max_log_age,
        max_bytes: max_log_bytes,
        max_file_bytes: (max_log_bytes / u64::try_from(max_log_files).unwrap_or(u64::MAX)).max(1),
        current: None,
        maintenance_lock,
        storage_drops: Arc::clone(&storage_drops),
        progress,
    };
    let (writer, guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(1024)
        .lossy(true)
        .finish(bounded);
    let health = WriterHealth {
        queue_drops: writer.error_counter(),
        storage_drops,
    };
    Ok(PreparedWriter {
        writer,
        guard,
        health,
    })
}

pub(crate) fn ensure_within_root(root: &Path, candidate: &Path) -> Result<(), DiagnosticsError> {
    let canonical_root = root
        .canonicalize()
        .map_err(|source| DiagnosticsError::io("canonicalize_diagnostic_root", source))?;
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|source| DiagnosticsError::io("canonicalize_diagnostic_directory", source))?;
    if canonical_candidate.starts_with(&canonical_root) {
        Ok(())
    } else {
        Err(DiagnosticsError::DiagnosticRootEscape)
    }
}

struct BoundedRollingWriter {
    directory: PathBuf,
    component: String,
    max_files: usize,
    max_age: Duration,
    max_bytes: u64,
    max_file_bytes: u64,
    current: Option<OpenLog>,
    maintenance_lock: File,
    storage_drops: Arc<AtomicUsize>,
    progress: LifecycleProgress,
}

impl BoundedRollingWriter {
    fn write_locked(&mut self, buffer: &[u8]) -> io::Result<()> {
        let bytes = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if bytes > self.max_bytes {
            self.note_drop();
            return Ok(());
        }
        let day = unix_day(SystemTime::now());
        let requires_rotation = self.current.as_ref().is_none_or(|current| {
            current.day != day
                || current
                    .bytes
                    .checked_add(bytes)
                    .is_none_or(|next| next > self.max_file_bytes)
        });
        if requires_rotation {
            self.close_current();
            let inventory = match prune_logs(
                &self.directory,
                &self.component,
                self.max_age,
                self.max_files.saturating_sub(1),
                self.max_bytes.saturating_sub(bytes),
                SystemTime::now(),
                None,
            ) {
                Ok(inventory) => inventory,
                Err(_) => {
                    self.note_drop();
                    return Ok(());
                }
            };
            if inventory.files >= self.max_files
                || inventory.bytes.saturating_add(bytes) > self.max_bytes
            {
                self.note_drop();
                return Ok(());
            }
            match OpenLog::create(&self.directory, &self.component, day) {
                Ok(log) => self.current = Some(log),
                Err(_) => {
                    self.note_drop();
                    return Ok(());
                }
            }
        } else {
            let protected = self.current.as_ref().map(|current| current.path.as_path());
            let inventory = match prune_logs(
                &self.directory,
                &self.component,
                self.max_age,
                self.max_files,
                self.max_bytes.saturating_sub(bytes),
                SystemTime::now(),
                protected,
            ) {
                Ok(inventory) => inventory,
                Err(_) => {
                    self.note_drop();
                    return Ok(());
                }
            };
            if inventory.bytes.saturating_add(bytes) > self.max_bytes {
                self.close_current();
                self.note_drop();
                return Ok(());
            }
        }

        let Some(current) = self.current.as_mut() else {
            self.note_drop();
            return Ok(());
        };
        if current.file.write_all(buffer).is_err() {
            drop(self.current.take());
            self.note_drop();
            return Ok(());
        }
        current.bytes = current.bytes.saturating_add(bytes);
        Ok(())
    }

    fn close_current(&mut self) {
        if let Some(mut current) = self.current.take()
            && (current.file.flush().is_err() || current.file.sync_data().is_err())
        {
            self.note_drop();
        }
    }

    fn note_drop(&self) {
        let dropped = self
            .storage_drops
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let _ = self.progress.note_dropped_records(dropped);
    }
}

impl Write for BoundedRollingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if FileExt::lock_exclusive(&self.maintenance_lock).is_err() {
            self.note_drop();
            return Ok(buffer.len());
        }
        let result = self.write_locked(buffer);
        if FileExt::unlock(&self.maintenance_lock).is_err() {
            self.note_drop();
        }
        result.map(|()| buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if FileExt::lock_exclusive(&self.maintenance_lock).is_err() {
            self.note_drop();
            return Ok(());
        }
        if let Some(current) = self.current.as_mut()
            && (current.file.flush().is_err() || current.file.sync_data().is_err())
        {
            self.note_drop();
        }
        if FileExt::unlock(&self.maintenance_lock).is_err() {
            self.note_drop();
        }
        Ok(())
    }
}

impl Drop for BoundedRollingWriter {
    fn drop(&mut self) {
        self.close_current();
    }
}

struct OpenLog {
    file: File,
    lock_file: File,
    path: PathBuf,
    lock_path: PathBuf,
    bytes: u64,
    day: u64,
}

impl OpenLog {
    fn create(directory: &Path, component: &str, day: u64) -> io::Result<Self> {
        let nonce = Uuid::now_v7();
        let path = directory.join(format!(
            "{component}-diagnostic.{day:010}.{}.{nonce}.jsonl",
            std::process::id()
        ));
        let lock_path = log_lock_path(&path);
        let lock_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        FileExt::try_lock_exclusive(&lock_file)?;
        let file = match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(file) => file,
            Err(error) => {
                let _ = FileExt::unlock(&lock_file);
                drop(lock_file);
                let _ = std::fs::remove_file(&lock_path);
                return Err(error);
            }
        };
        Ok(Self {
            file,
            lock_file,
            path,
            lock_path,
            bytes: 0,
            day,
        })
    }
}

impl Drop for OpenLog {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.lock_file);
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LogInventory {
    files: usize,
    bytes: u64,
}

fn prune_logs(
    directory: &Path,
    component: &str,
    max_age: Duration,
    max_files: usize,
    max_bytes: u64,
    now: SystemTime,
    protected: Option<&Path>,
) -> Result<LogInventory, DiagnosticsError> {
    cleanup_orphan_log_locks(directory, component)?;
    let mut files = log_files(directory, component)?;
    files.sort_by_key(|file| (file.modified, file.path.clone()));
    for file in &files {
        let expired = file
            .modified
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if expired && protected != Some(file.path.as_path()) {
            remove_bounded_file(&file.path)?;
        }
    }
    files = log_files(directory, component)?;
    files.sort_by_key(|file| (file.modified, file.path.clone()));
    let mut inventory = inventory(&files);
    for file in files {
        if inventory.files <= max_files && inventory.bytes <= max_bytes {
            break;
        }
        if protected == Some(file.path.as_path()) {
            continue;
        }
        if remove_bounded_file(&file.path)? {
            inventory.files = inventory.files.saturating_sub(1);
            inventory.bytes = inventory.bytes.saturating_sub(file.bytes);
        }
    }
    Ok(inventory)
}

struct LogFile {
    path: PathBuf,
    modified: Option<SystemTime>,
    bytes: u64,
}

fn log_files(directory: &Path, component: &str) -> Result<Vec<LogFile>, DiagnosticsError> {
    let prefix = format!("{component}-diagnostic.");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)
        .map_err(|source| DiagnosticsError::io("read_log_directory", source))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("read_log_entry", source)),
        };
        let file_type: FileType = entry
            .file_type()
            .map_err(|source| DiagnosticsError::io("read_log_file_type", source))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !file_type.is_file() || !name.starts_with(&prefix) || !name.ends_with(".jsonl") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|source| DiagnosticsError::io("read_log_metadata", source))?;
        files.push(LogFile {
            path: entry.path(),
            modified: metadata.modified().ok(),
            bytes: metadata.len(),
        });
    }
    Ok(files)
}

fn inventory(files: &[LogFile]) -> LogInventory {
    LogInventory {
        files: files.len(),
        bytes: files
            .iter()
            .fold(0_u64, |total, file| total.saturating_add(file.bytes)),
    }
}

fn remove_bounded_file(path: &Path) -> Result<bool, DiagnosticsError> {
    let lock_path = log_lock_path(path);
    let lock_file = match OpenOptions::new().read(true).write(true).open(&lock_path) {
        Ok(file) => match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Some(file),
            Err(_) => return Ok(false),
        },
        Err(source) if source.kind() == io::ErrorKind::NotFound => None,
        Err(source) => return Err(DiagnosticsError::io("open_log_lock", source)),
    };
    let removed = match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(source)
            if matches!(
                source.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) =>
        {
            false
        }
        Err(source) => return Err(DiagnosticsError::io("prune_log_file", source)),
    };
    if let Some(file) = lock_file {
        let _ = FileExt::unlock(&file);
        drop(file);
        if removed {
            let _ = std::fs::remove_file(lock_path);
        }
    }
    Ok(removed)
}

fn cleanup_orphan_log_locks(directory: &Path, component: &str) -> Result<(), DiagnosticsError> {
    let prefix = format!("{component}-diagnostic.");
    for entry in std::fs::read_dir(directory)
        .map_err(|source| DiagnosticsError::io("read_log_directory", source))?
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("read_log_entry", source)),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(&prefix) || !name.ends_with(".jsonl.lock") {
            continue;
        }
        let lock_path = entry.path();
        let log_path = lock_path.with_extension("");
        if log_path.exists() {
            continue;
        }
        let file = match OpenOptions::new().read(true).write(true).open(&lock_path) {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::NotFound => continue,
            Err(source) => return Err(DiagnosticsError::io("open_orphan_log_lock", source)),
        };
        if FileExt::try_lock_exclusive(&file).is_ok() {
            let _ = FileExt::unlock(&file);
            drop(file);
            let _ = std::fs::remove_file(lock_path);
        }
    }
    Ok(())
}

fn log_lock_path(path: &Path) -> PathBuf {
    path.with_extension("jsonl.lock")
}

fn log_maintenance_path(directory: &Path, component: &str) -> PathBuf {
    directory.join(format!("{component}-diagnostic.maintenance.lock"))
}

fn unix_day(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::LifecycleSession;

    #[test]
    fn containment_rejects_a_resolved_directory_outside_the_profile() {
        let temp = tempfile::tempdir().expect("temporary root");
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        assert!(matches!(
            ensure_within_root(&root, &outside),
            Err(DiagnosticsError::DiagnosticRootEscape)
        ));
    }

    #[test]
    fn age_pruning_removes_expired_logs() {
        let temp = tempfile::tempdir().expect("temporary logs");
        let old = temp.path().join("dennett-node-diagnostic.old.jsonl");
        std::fs::write(&old, vec![b'a'; 16]).expect("old log");
        std::thread::sleep(Duration::from_millis(10));
        let result = prune_logs(
            temp.path(),
            "dennett-node",
            Duration::from_millis(1),
            8,
            1_024,
            SystemTime::now(),
            None,
        )
        .expect("prune logs");
        assert_eq!(result.bytes, 0);
        assert!(!old.exists());
    }

    #[test]
    fn size_pruning_removes_the_oldest_log_first() {
        let temp = tempfile::tempdir().expect("temporary logs");
        let old = temp.path().join("dennett-node-diagnostic.old.jsonl");
        let new = temp.path().join("dennett-node-diagnostic.new.jsonl");
        std::fs::write(&old, vec![b'a'; 16]).expect("old log");
        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&new, vec![b'b'; 16]).expect("new log");
        let result = prune_logs(
            temp.path(),
            "dennett-node",
            Duration::from_secs(60),
            8,
            16,
            SystemTime::now(),
            None,
        )
        .expect("prune logs");
        assert_eq!(result.bytes, 16);
        assert!(!old.exists());
        assert!(new.exists());
    }

    #[test]
    fn writer_rotates_under_a_lifetime_quota_instead_of_becoming_permanently_blind() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let lifecycle =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("lifecycle");
        let progress = lifecycle.progress();
        let logs = diagnostics.join("logs");
        std::fs::create_dir_all(&logs).expect("logs");
        let maintenance_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(log_maintenance_path(&logs, "dennett-node"))
            .expect("maintenance lock");
        let drops = Arc::new(AtomicUsize::new(0));
        let mut writer = BoundedRollingWriter {
            directory: logs.clone(),
            component: "dennett-node".to_owned(),
            max_files: 2,
            max_age: Duration::from_secs(60),
            max_bytes: 16,
            max_file_bytes: 8,
            current: None,
            maintenance_lock,
            storage_drops: Arc::clone(&drops),
            progress,
        };
        for value in [b"12345678".as_slice(), b"abcdefgh", b"ABCDEFGH"] {
            writer.write_all(value).expect("bounded write");
        }
        writer.flush().expect("flush");
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert_eq!(log_files(&logs, "dennett-node").expect("logs").len(), 2);
        drop(writer);
        lifecycle
            .complete(crate::DiagnosticExit::Clean, 0)
            .expect("complete lifecycle");
    }

    #[test]
    fn physical_write_failure_is_counted_and_persisted_for_doctor() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let diagnostics = temp.path().join("diagnostics");
        let lifecycle =
            LifecycleSession::start(&diagnostics, "dennett-node", 8).expect("lifecycle");
        let logs = diagnostics.join("logs");
        std::fs::create_dir_all(&logs).expect("logs");
        let day = unix_day(SystemTime::now());
        let path = logs.join(format!(
            "dennett-node-diagnostic.{day:010}.{}.jsonl",
            Uuid::now_v7()
        ));
        std::fs::write(&path, b"").expect("read-only test log");
        let lock_path = log_lock_path(&path);
        let lock_file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .expect("log lock");
        FileExt::try_lock_exclusive(&lock_file).expect("lock test log");
        let read_only = OpenOptions::new()
            .read(true)
            .open(&path)
            .expect("read-only log handle");
        let maintenance_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(log_maintenance_path(&logs, "dennett-node"))
            .expect("maintenance lock");
        let drops = Arc::new(AtomicUsize::new(0));
        let mut writer = BoundedRollingWriter {
            directory: logs,
            component: "dennett-node".to_owned(),
            max_files: 2,
            max_age: Duration::from_secs(60),
            max_bytes: 16,
            max_file_bytes: 8,
            current: Some(OpenLog {
                file: read_only,
                lock_file,
                path,
                lock_path,
                bytes: 0,
                day,
            }),
            maintenance_lock,
            storage_drops: Arc::clone(&drops),
            progress: lifecycle.progress(),
        };

        writer
            .write_all(b"x")
            .expect("lossy writer stays available");
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        let summary = crate::inspect_local(temp.path(), "dennett-node").expect("doctor summary");
        assert_eq!(summary.dropped_log_records, 1);
        drop(writer);
        lifecycle
            .complete(crate::DiagnosticExit::Clean, 1)
            .expect("complete lifecycle");
    }

    #[test]
    fn resolved_directory_links_cannot_escape_the_profile() {
        let temp = tempfile::tempdir().expect("temporary root");
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        let link = root.join("diagnostics");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::create_dir_all(&outside).expect("outside");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).expect("directory symlink");
        #[cfg(windows)]
        if let Err(error) = std::os::windows::fs::symlink_dir(&outside, &link) {
            if error.kind() == io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(1314)
            {
                return;
            }
            panic!("directory link failed: {error}");
        }
        assert!(matches!(
            ensure_within_root(&root, &link),
            Err(DiagnosticsError::DiagnosticRootEscape)
        ));
    }
}
