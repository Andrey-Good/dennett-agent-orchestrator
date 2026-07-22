use crate::{
    CheckpointPublisher, DiagnosticsError,
    secure_fs::{SecureDir, lock_exclusive_bounded},
};
use fs2::FileExt;
use std::{
    fs::File,
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

const MAX_LOG_DIRECTORY_ENTRIES: usize = 512;

pub(crate) struct PreparedWriter {
    pub(crate) writer: NonBlocking,
    pub(crate) guard: WorkerGuard,
    pub(crate) health: WriterHealth,
}

#[derive(Clone)]
pub(crate) struct WriterHealth {
    queue_drops: ErrorCounter,
    storage_drops: Arc<AtomicUsize>,
    durability_failures: Arc<AtomicUsize>,
}

impl WriterHealth {
    pub(crate) fn dropped_records(&self) -> usize {
        self.queue_drops
            .dropped_lines()
            .saturating_add(self.storage_drops.load(Ordering::Acquire))
    }

    pub(crate) fn flush_confirmed(&self) -> bool {
        self.durability_failures.load(Ordering::Acquire) == 0
    }
}

pub(crate) fn prepare_writer(
    diagnostics_dir: &SecureDir,
    component: &str,
    max_log_files: usize,
    max_log_age: Duration,
    max_log_bytes: u64,
    checkpoint_publisher: CheckpointPublisher,
) -> Result<PreparedWriter, DiagnosticsError> {
    let logs_dir = diagnostics_dir.open_or_create_child("logs", "create_log_directory")?;
    let maintenance_lock = logs_dir.open_lock_file(
        &log_maintenance_name(component),
        true,
        "open_log_maintenance_lock",
    )?;
    lock_exclusive_bounded(&maintenance_lock, "lock_log_maintenance")?;
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
    let durability_failures = Arc::new(AtomicUsize::new(0));
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
        durability_failures: Arc::clone(&durability_failures),
        checkpoint_publisher,
    };
    let (writer, guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(1024)
        .lossy(true)
        .finish(bounded);
    let health = WriterHealth {
        queue_drops: writer.error_counter(),
        storage_drops,
        durability_failures,
    };
    Ok(PreparedWriter {
        writer,
        guard,
        health,
    })
}

struct BoundedRollingWriter {
    directory: SecureDir,
    component: String,
    max_files: usize,
    max_age: Duration,
    max_bytes: u64,
    max_file_bytes: u64,
    current: Option<OpenLog>,
    maintenance_lock: File,
    storage_drops: Arc<AtomicUsize>,
    durability_failures: Arc<AtomicUsize>,
    checkpoint_publisher: CheckpointPublisher,
}

impl BoundedRollingWriter {
    fn write_locked(&mut self, buffer: &[u8]) -> io::Result<()> {
        let bytes = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if bytes > self.max_bytes {
            self.note_drop(false);
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
                    self.note_drop(true);
                    return Ok(());
                }
            };
            if inventory.files >= self.max_files
                || inventory.bytes.saturating_add(bytes) > self.max_bytes
            {
                self.note_drop(false);
                return Ok(());
            }
            match OpenLog::create(&self.directory, &self.component, day) {
                Ok(log) => self.current = Some(log),
                Err(_) => {
                    self.note_drop(true);
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
                    self.note_drop(true);
                    return Ok(());
                }
            };
            if inventory.bytes.saturating_add(bytes) > self.max_bytes {
                self.close_current();
                self.note_drop(false);
                return Ok(());
            }
        }

        let Some(current) = self.current.as_mut() else {
            self.note_drop(true);
            return Ok(());
        };
        if current.file.write_all(buffer).is_err() {
            drop(self.current.take());
            self.note_drop(true);
            return Ok(());
        }
        current.bytes = current.bytes.saturating_add(bytes);
        Ok(())
    }

    fn close_current(&mut self) {
        if let Some(mut current) = self.current.take()
            && (current.file.flush().is_err() || current.file.sync_data().is_err())
        {
            self.note_drop(true);
        }
    }

    fn note_drop(&self, durability_failure: bool) {
        let dropped = self
            .storage_drops
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        if durability_failure {
            self.durability_failures.fetch_add(1, Ordering::AcqRel);
        }
        self.checkpoint_publisher.publish_dropped(dropped);
    }
}

impl Write for BoundedRollingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if FileExt::try_lock_exclusive(&self.maintenance_lock).is_err() {
            self.note_drop(true);
            return Ok(buffer.len());
        }
        let result = self.write_locked(buffer);
        if FileExt::unlock(&self.maintenance_lock).is_err() {
            self.note_drop(true);
        }
        result.map(|()| buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if FileExt::try_lock_exclusive(&self.maintenance_lock).is_err() {
            self.note_drop(true);
            return Ok(());
        }
        if let Some(current) = self.current.as_mut()
            && (current.file.flush().is_err() || current.file.sync_data().is_err())
        {
            self.note_drop(true);
        }
        if FileExt::unlock(&self.maintenance_lock).is_err() {
            self.note_drop(true);
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
    directory: SecureDir,
    file: File,
    lock_file: File,
    path: PathBuf,
    lock_path: PathBuf,
    bytes: u64,
    day: u64,
}

impl OpenLog {
    fn create(directory: &SecureDir, component: &str, day: u64) -> Result<Self, DiagnosticsError> {
        let nonce = Uuid::now_v7();
        let path = PathBuf::from(format!(
            "{component}-diagnostic.{day:010}.{}.{nonce}.jsonl",
            std::process::id()
        ));
        let lock_path = log_lock_path(&path);
        let lock_file =
            directory.create_new_file(path_name(&lock_path)?, true, "create_log_lock")?;
        FileExt::try_lock_exclusive(&lock_file)
            .map_err(|source| DiagnosticsError::io("lock_log_file", source))?;
        let file = match directory.create_new_file(path_name(&path)?, false, "create_log_file") {
            Ok(file) => file,
            Err(error) => {
                let _ = FileExt::unlock(&lock_file);
                drop(lock_file);
                let _ = directory.remove_file(path_name(&lock_path)?, "remove_log_lock");
                return Err(error);
            }
        };
        Ok(Self {
            directory: directory.clone(),
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
        let _ = self.directory.remove_file(
            path_name(&self.lock_path).unwrap_or_default(),
            "remove_log_lock",
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LogInventory {
    files: usize,
    bytes: u64,
}

fn prune_logs(
    directory: &SecureDir,
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
            remove_bounded_file(directory, &file.path)?;
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
        if remove_bounded_file(directory, &file.path)? {
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

fn log_files(directory: &SecureDir, component: &str) -> Result<Vec<LogFile>, DiagnosticsError> {
    let prefix = format!("{component}-diagnostic.");
    let mut files = Vec::new();
    for entry in directory.entries_bounded(MAX_LOG_DIRECTORY_ENTRIES, "read_log_directory")? {
        let name = entry.name.to_string_lossy();
        if !entry.metadata.is_file()
            || entry.metadata.file_type().is_symlink()
            || !name.starts_with(&prefix)
            || !name.ends_with(".jsonl")
        {
            continue;
        }
        files.push(LogFile {
            path: PathBuf::from(name.as_ref()),
            modified: entry.metadata.modified().ok().map(|value| value.into_std()),
            bytes: entry.metadata.len(),
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

fn remove_bounded_file(directory: &SecureDir, path: &Path) -> Result<bool, DiagnosticsError> {
    let lock_path = log_lock_path(path);
    let lock_file =
        match directory.open_existing_file(path_name(&lock_path)?, true, "open_log_lock") {
            Ok(file) => match FileExt::try_lock_exclusive(&file) {
                Ok(()) => Some(file),
                Err(_) => return Ok(false),
            },
            Err(error) if is_not_found(&error) => None,
            Err(error) => return Err(error),
        };
    let existed = directory.try_exists(path_name(path)?)?;
    directory.remove_file(path_name(path)?, "prune_log_file")?;
    let removed = existed && !directory.try_exists(path_name(path)?)?;
    if let Some(file) = lock_file {
        let _ = FileExt::unlock(&file);
        drop(file);
        if removed {
            let _ = directory.remove_file(path_name(&lock_path)?, "remove_log_lock");
        }
    }
    Ok(removed)
}

fn cleanup_orphan_log_locks(
    directory: &SecureDir,
    component: &str,
) -> Result<(), DiagnosticsError> {
    let prefix = format!("{component}-diagnostic.");
    for entry in directory.entries_bounded(MAX_LOG_DIRECTORY_ENTRIES, "read_log_directory")? {
        let name = entry.name.to_string_lossy();
        if !name.starts_with(&prefix) || !name.ends_with(".jsonl.lock") {
            continue;
        }
        let lock_path = PathBuf::from(name.as_ref());
        let log_path = lock_path.with_extension("");
        if directory.try_exists(path_name(&log_path)?)? {
            continue;
        }
        let file = match directory.open_existing_file(
            path_name(&lock_path)?,
            true,
            "open_orphan_log_lock",
        ) {
            Ok(file) => file,
            Err(error) if is_not_found(&error) => continue,
            Err(error) => return Err(error),
        };
        if FileExt::try_lock_exclusive(&file).is_ok() {
            let _ = FileExt::unlock(&file);
            drop(file);
            directory.remove_file(path_name(&lock_path)?, "remove_orphan_log_lock")?;
        }
    }
    Ok(())
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

fn is_not_found(error: &DiagnosticsError) -> bool {
    matches!(
        error,
        DiagnosticsError::Io { source, .. }
            if source.kind() == io::ErrorKind::NotFound
    )
}

fn log_lock_path(path: &Path) -> PathBuf {
    path.with_extension("jsonl.lock")
}

fn log_maintenance_name(component: &str) -> String {
    format!("{component}-diagnostic.maintenance.lock")
}

fn unix_day(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() / 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logs_dir(temp: &tempfile::TempDir) -> SecureDir {
        SecureDir::open_or_create_profile(temp.path())
            .expect("profile")
            .open_or_create_child("logs", "logs")
            .expect("logs")
    }

    #[test]
    fn age_pruning_removes_expired_logs() {
        let temp = tempfile::tempdir().expect("temporary logs");
        let directory = logs_dir(&temp);
        let old = temp.path().join("logs/dennett-node-diagnostic.old.jsonl");
        std::fs::write(&old, vec![b'a'; 16]).expect("old log");
        std::thread::sleep(Duration::from_millis(10));
        let result = prune_logs(
            &directory,
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
        let directory = logs_dir(&temp);
        let old = temp.path().join("logs/dennett-node-diagnostic.old.jsonl");
        let new = temp.path().join("logs/dennett-node-diagnostic.new.jsonl");
        std::fs::write(&old, vec![b'a'; 16]).expect("old log");
        std::thread::sleep(Duration::from_millis(10));
        std::fs::write(&new, vec![b'b'; 16]).expect("new log");
        let result = prune_logs(
            &directory,
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
    fn writer_rotates_under_a_lifetime_quota() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let directory = logs_dir(&temp);
        let maintenance_lock = directory
            .open_lock_file(
                &log_maintenance_name("dennett-node"),
                true,
                "maintenance lock",
            )
            .expect("maintenance lock");
        let drops = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(AtomicUsize::new(0));
        let mut writer = BoundedRollingWriter {
            directory: directory.clone(),
            component: "dennett-node".to_owned(),
            max_files: 2,
            max_age: Duration::from_secs(60),
            max_bytes: 16,
            max_file_bytes: 8,
            current: None,
            maintenance_lock,
            storage_drops: Arc::clone(&drops),
            durability_failures: Arc::clone(&failures),
            checkpoint_publisher: CheckpointPublisher::disabled_for_test(),
        };
        for value in [b"12345678".as_slice(), b"abcdefgh", b"ABCDEFGH"] {
            writer.write_all(value).expect("bounded write");
        }
        writer.flush().expect("flush");
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        assert_eq!(failures.load(Ordering::Relaxed), 0);
        assert_eq!(
            log_files(&directory, "dennett-node").expect("logs").len(),
            2
        );
    }

    #[test]
    fn physical_write_failure_is_counted() {
        let temp = tempfile::tempdir().expect("temporary diagnostics");
        let directory = logs_dir(&temp);
        let day = unix_day(SystemTime::now());
        let path = PathBuf::from(format!(
            "dennett-node-diagnostic.{day:010}.{}.jsonl",
            Uuid::now_v7()
        ));
        let display_path = temp.path().join("logs").join(&path);
        std::fs::write(&display_path, b"").expect("read-only test log");
        let lock_path = log_lock_path(&path);
        let lock_file = directory
            .create_new_file(path_name(&lock_path).expect("lock name"), true, "log lock")
            .expect("log lock");
        FileExt::try_lock_exclusive(&lock_file).expect("lock test log");
        let read_only = directory
            .open_existing_file(path_name(&path).expect("log name"), false, "read-only log")
            .expect("read-only log handle");
        let maintenance_lock = directory
            .open_lock_file(
                &log_maintenance_name("dennett-node"),
                true,
                "maintenance lock",
            )
            .expect("maintenance lock");
        let drops = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(AtomicUsize::new(0));
        let mut writer = BoundedRollingWriter {
            directory: directory.clone(),
            component: "dennett-node".to_owned(),
            max_files: 2,
            max_age: Duration::from_secs(60),
            max_bytes: 16,
            max_file_bytes: 8,
            current: Some(OpenLog {
                directory,
                file: read_only,
                lock_file,
                path,
                lock_path,
                bytes: 0,
                day,
            }),
            maintenance_lock,
            storage_drops: Arc::clone(&drops),
            durability_failures: Arc::clone(&failures),
            checkpoint_publisher: CheckpointPublisher::disabled_for_test(),
        };

        writer
            .write_all(b"x")
            .expect("lossy writer stays available");
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(failures.load(Ordering::Relaxed), 1);
    }
}
