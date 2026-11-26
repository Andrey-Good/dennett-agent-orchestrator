//! OS-native process-tree containment for spawned `agy acp` subprocesses.
//!
//! Ensures that terminating or dropping an `AcpProcess` atomically kills the entire
//! descendant process tree (e.g. `cargo`, `rustc`, test runners) without leaving orphan
//! grandchild processes holding NTFS file locks (`os error 32`) or background CPU/ports:
//! - On Windows: creates an anonymous Win32 Job Object with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, attaches the spawned child handle immediately,
//!   and invokes `TerminateJobObject` + `CloseHandle` on explicit termination or `Drop`.
//! - On Unix: places the child into its own process group (`process_group(0)`) before
//!   spawn and delivers `SIGKILL` to `-pgid` on explicit termination or `Drop`.

use std::time::Duration;

use tokio::process::{Child, Command};

#[cfg(windows)]
mod platform {
    #![expect(
        unsafe_code,
        reason = "direct Win32 kernel32 Job Object FFI for zero-dependency OS process-tree containment"
    )]

    use core::ffi::c_void;
    use core::mem::{size_of, zeroed};
    use core::ptr;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::time::{Duration, Instant};

    use tokio::process::{Child, Command};

    const JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION_CLASS: u32 = 1;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: u32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[expect(
        clippy::struct_field_names,
        reason = "exact Win32 IO_COUNTERS ABI field names"
    )]
    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    pub(super) struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[repr(C)]
    struct JobObjectBasicAccountingInformation {
        total_user_time: i64,
        total_kernel_time: i64,
        this_period_total_user_time: i64,
        this_period_total_kernel_time: i64,
        total_page_fault_count: u32,
        total_processes: u32,
        active_processes: u32,
        total_terminated_processes: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(lp_job_attributes: *mut c_void, lp_name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            h_job: *mut c_void,
            job_object_information_class: u32,
            lp_job_object_information: *const c_void,
            cb_job_object_information_length: u32,
        ) -> i32;
        fn QueryInformationJobObject(
            h_job: *mut c_void,
            job_object_information_class: u32,
            lp_job_object_information: *mut c_void,
            cb_job_object_information_length: u32,
            lp_return_length: *mut u32,
        ) -> i32;
        fn AssignProcessToJobObject(h_job: *mut c_void, h_process: *mut c_void) -> i32;
        fn TerminateJobObject(h_job: *mut c_void, u_exit_code: u32) -> i32;
        fn CloseHandle(h_object: *mut c_void) -> i32;
    }

    struct OwnedJobHandle(*mut c_void);

    // SAFETY: Win32 Job Object handles are process-wide kernel handles valid from any thread.
    unsafe impl Send for OwnedJobHandle {}

    impl Drop for OwnedJobHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: `OwnedJobHandle` exclusively owns this Win32 Job Object handle.
                unsafe {
                    let _ = CloseHandle(self.0);
                }
                self.0 = ptr::null_mut();
            }
        }
    }

    pub(super) struct InnerGuard {
        job: AtomicPtr<c_void>,
    }

    impl InnerGuard {
        pub(super) const fn configure_command(_cmd: &mut Command) {
            // Windows assigns the spawned process to a Job Object immediately after `spawn()`.
        }

        pub(super) fn attach(child: &Child) -> Self {
            let Some(raw_handle) = child.raw_handle() else {
                return Self {
                    job: AtomicPtr::new(ptr::null_mut()),
                };
            };
            if raw_handle.is_null() {
                return Self {
                    job: AtomicPtr::new(ptr::null_mut()),
                };
            }

            // SAFETY: Anonymous job creation with null security attributes and null name.
            let job = unsafe { CreateJobObjectW(ptr::null_mut(), ptr::null()) };
            if job.is_null() {
                return Self {
                    job: AtomicPtr::new(ptr::null_mut()),
                };
            }

            // SAFETY: `JobObjectExtendedLimitInformation` is a plain C POD struct with valid all-zero layout.
            let mut info: JobObjectExtendedLimitInformation = unsafe { zeroed() };
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let info_len =
                u32::try_from(size_of::<JobObjectExtendedLimitInformation>()).unwrap_or(0);
            // SAFETY: `job` is a live kernel Job Object handle and `info` has exact Win32 ABI size.
            let set_ok = unsafe {
                SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    (&raw const info).cast::<c_void>(),
                    info_len,
                )
            };
            if set_ok == 0 {
                // SAFETY: `job` is owned solely by this function and has not been closed.
                unsafe {
                    let _ = CloseHandle(job);
                }
                return Self {
                    job: AtomicPtr::new(ptr::null_mut()),
                };
            }

            // SAFETY: `job` is valid and `raw_handle` is the live Win32 process handle borrowed from `child`.
            // Nested jobs succeed by default on Windows 8/10/11; if an ancient host disallows nested jobs,
            // fall back gracefully by closing the job handle without failing process spawn.
            let assigned = unsafe { AssignProcessToJobObject(job, raw_handle) };
            if assigned == 0 {
                // SAFETY: `job` is owned solely by this function and has not been closed.
                unsafe {
                    let _ = CloseHandle(job);
                }
                return Self {
                    job: AtomicPtr::new(ptr::null_mut()),
                };
            }

            Self {
                job: AtomicPtr::new(job),
            }
        }

        pub(super) fn is_attached(&self) -> bool {
            !self.job.load(Ordering::Acquire).is_null()
        }

        pub(super) fn terminate(&self) {
            let job = self.job.swap(ptr::null_mut(), Ordering::AcqRel);
            if job.is_null() {
                return;
            }
            // SAFETY: `job` was uniquely claimed via `swap` and is closed exactly once here.
            unsafe {
                let _ = TerminateJobObject(job, 1);
                let _ = CloseHandle(job);
            }
        }

        pub(super) async fn terminate_and_drain(&self, budget: Duration) {
            let raw_job = self.job.swap(ptr::null_mut(), Ordering::AcqRel);
            if raw_job.is_null() {
                return;
            }
            let job = OwnedJobHandle(raw_job);
            // SAFETY: `job.0` was uniquely claimed via `swap` and stays live until `job` drops.
            unsafe {
                let _ = TerminateJobObject(job.0, 1);
            }

            // Wait asynchronously until kernel teardown closes all member process handles so callers
            // can immediately delete workspace/tempdir files without encountering `os error 32`.
            let deadline = Instant::now() + budget;
            let acct_len =
                u32::try_from(size_of::<JobObjectBasicAccountingInformation>()).unwrap_or(0);
            loop {
                // SAFETY: `JobObjectBasicAccountingInformation` is a plain C POD struct.
                let mut acct: JobObjectBasicAccountingInformation = unsafe { zeroed() };
                let mut ret_len: u32 = 0;
                // SAFETY: `job.0` is live and exclusively owned by `OwnedJobHandle`.
                let ok = unsafe {
                    QueryInformationJobObject(
                        job.0,
                        JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION_CLASS,
                        (&raw mut acct).cast::<c_void>(),
                        acct_len,
                        &raw mut ret_len,
                    )
                };
                if ok == 0 || acct.active_processes == 0 || Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            drop(job);
        }
    }

    impl Drop for InnerGuard {
        fn drop(&mut self) {
            self.terminate();
        }
    }
}

#[cfg(unix)]
mod platform {
    #![expect(
        unsafe_code,
        reason = "direct POSIX kill(2) FFI for zero-dependency process-group termination"
    )]

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};

    use tokio::process::{Child, Command};

    const SIGKILL: i32 = 9;

    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    pub(super) struct InnerGuard {
        pgid: AtomicU32,
    }

    impl InnerGuard {
        pub(super) fn configure_command(cmd: &mut Command) {
            // Put the child into its own process group (`pgid == child.pid`) so `kill(-pgid, SIGKILL)`
            // terminates the entire descendant process tree atomically.
            cmd.process_group(0);
        }

        pub(super) fn attach(child: &Child) -> Self {
            Self {
                pgid: AtomicU32::new(child.id().unwrap_or(0)),
            }
        }

        pub(super) fn is_attached(&self) -> bool {
            self.pgid.load(Ordering::Acquire) > 0
        }

        pub(super) fn terminate(&self) {
            let pgid = self.pgid.swap(0, Ordering::AcqRel);
            if pgid == 0 {
                return;
            }
            if let Ok(pgid_i32) = i32::try_from(pgid)
                && pgid_i32 > 0
            {
                // SAFETY: Negative PID targets the child's dedicated process group created via `process_group(0)`.
                unsafe {
                    let _ = kill(-pgid_i32, SIGKILL);
                }
            }
        }

        pub(super) async fn terminate_and_drain(&self, budget: Duration) {
            let pgid = self.pgid.swap(0, Ordering::AcqRel);
            if pgid == 0 {
                return;
            }
            let Ok(pgid_i32) = i32::try_from(pgid) else {
                return;
            };
            if pgid_i32 <= 0 {
                return;
            }

            // SAFETY: Negative PID targets the child's dedicated process group created via `process_group(0)`.
            unsafe {
                let _ = kill(-pgid_i32, SIGKILL);
            }

            let deadline = Instant::now() + budget;
            loop {
                // SAFETY: Signal `0` probes whether any process remains alive in the group without sending a signal.
                let still_alive = unsafe { kill(-pgid_i32, 0) } == 0;
                if !still_alive || Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }

    impl Drop for InnerGuard {
        fn drop(&mut self) {
            self.terminate();
        }
    }
}

#[cfg(not(any(windows, unix)))]
mod platform {
    use std::time::Duration;

    use tokio::process::{Child, Command};

    pub(super) struct InnerGuard;

    impl InnerGuard {
        pub(super) const fn configure_command(_cmd: &mut Command) {}
        pub(super) const fn attach(_child: &Child) -> Self {
            Self
        }
        pub(super) const fn is_attached(&self) -> bool {
            false
        }
        pub(super) const fn terminate(&self) {}
        pub(super) async fn terminate_and_drain(&self, _budget: Duration) {}
    }
}

/// RAII handle binding a spawned child process and all of its descendants to an OS-native
/// containment boundary (Win32 Job Object on Windows, dedicated process group on Unix).
pub struct ProcessTreeGuard {
    inner: platform::InnerGuard,
}

impl ProcessTreeGuard {
    /// Configures `cmd` prior to spawning so the child starts inside its own process-tree boundary.
    pub fn configure_command(cmd: &mut Command) {
        platform::InnerGuard::configure_command(cmd);
    }

    /// Attaches an immediately spawned `child` process to a fresh OS-native containment boundary.
    #[must_use]
    pub fn attach(child: &Child) -> Self {
        Self {
            inner: platform::InnerGuard::attach(child),
        }
    }

    /// Returns `true` if the child was successfully attached to an OS-native process-tree guard.
    #[must_use]
    pub fn is_attached(&self) -> bool {
        self.inner.is_attached()
    }

    /// Non-blocking immediate termination of the entire contained process tree.
    ///
    /// Safe and idempotent to invoke from synchronous contexts including [`Drop`].
    pub fn terminate(&self) {
        self.inner.terminate();
    }

    /// Terminates the entire contained process tree and asynchronously waits up to `budget`
    /// for all member processes to complete kernel exit teardown (releasing file locks/handles).
    pub async fn terminate_and_drain(&self, budget: Duration) {
        self.inner.terminate_and_drain(budget).await;
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ProcessTreeGuard>();
};

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use tokio::process::Command;

    use super::ProcessTreeGuard;

    #[cfg(all(windows, target_pointer_width = "64"))]
    #[test]
    fn win32_job_extended_limit_struct_matches_msvc_abi_size() {
        assert_eq!(
            core::mem::size_of::<super::platform::JobObjectExtendedLimitInformation>(),
            144
        );
    }

    /// Real OS-level verification: spawn a shell parent that spawns a grandchild process holding
    /// an open writable file in a temporary directory for 30s; verify that `ProcessTreeGuard`
    /// kills BOTH parent and grandchild and releases all NTFS/OS locks so the locked file and
    /// directory can be removed immediately with zero `os error 32`.
    #[tokio::test]
    async fn process_tree_guard_kills_grandchild_and_releases_file_locks_immediately() {
        let base_dir = std::env::temp_dir().join(format!(
            "dennett-proc-tree-test-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&base_dir).expect("create temp dir");
        let lock_file = base_dir.join("held-by-grandchild.lock");
        let pid_file = base_dir.join("grandchild.pid");

        let mut cmd = build_grandchild_lock_command(&lock_file, &pid_file);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        ProcessTreeGuard::configure_command(&mut cmd);

        let mut child = cmd.spawn().expect("spawn parent shell");
        let guard = ProcessTreeGuard::attach(&child);
        assert!(
            guard.is_attached(),
            "ProcessTreeGuard must attach on supported Windows/Unix host"
        );

        // Wait until the grandchild has opened the lock file and written its PID.
        let wait_deadline = Instant::now() + Duration::from_secs(10);
        let grandchild_pid: u32 = loop {
            if let Ok(raw) = fs::read_to_string(&pid_file)
                && let Ok(pid) = raw.trim().parse::<u32>()
                && lock_file.exists()
            {
                break pid;
            }
            assert!(
                Instant::now() < wait_deadline,
                "timed out waiting for grandchild process to create lock file and pid file"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };

        let parent_pid = child.id().expect("parent pid");
        assert_ne!(
            grandchild_pid, parent_pid,
            "test must spawn a distinct grandchild PID"
        );

        // On Windows, verify the grandchild actually holds an active open handle right now!
        #[cfg(windows)]
        {
            let remove_while_alive = fs::remove_file(&lock_file);
            assert!(
                remove_while_alive.is_err(),
                "lock file must be held open by grandchild before termination"
            );
        }

        // Terminate the entire process tree via `ProcessTreeGuard` and reap the parent.
        guard.terminate_and_drain(Duration::from_secs(5)).await;
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
        drop(guard);

        // Both parent and grandchild MUST be dead now.
        assert!(
            matches!(child.try_wait(), Ok(Some(_))),
            "parent shell process must have exited after ProcessTreeGuard termination"
        );
        assert!(
            !is_pid_alive(grandchild_pid),
            "grandchild process {grandchild_pid} must be dead after ProcessTreeGuard termination"
        );

        // Prove the locked file AND its parent directory can be deleted immediately with zero os error 32!
        // Also verify we can open the file exclusively/delete it right away.
        assert!(
            OpenOptions::new().write(true).open(&lock_file).is_ok(),
            "lock file must be writable immediately after grandchild termination"
        );
        fs::remove_file(&lock_file)
            .expect("locked file must be removable immediately with zero os error 32");
        fs::remove_dir_all(&base_dir)
            .expect("temp directory must be removable immediately with zero os error 32");
    }

    /// Proves that simply dropping `ProcessTreeGuard` (without calling `terminate_and_drain`)
    /// atomically kills both parent and grandchild via `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` /
    /// `Drop` and releases the locked file/directory.
    #[tokio::test]
    async fn process_tree_guard_drop_kills_grandchild_and_releases_file_locks() {
        let base_dir = std::env::temp_dir().join(format!(
            "dennett-proc-tree-drop-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&base_dir).expect("create temp dir");
        let lock_file = base_dir.join("held-by-grandchild-drop.lock");
        let pid_file = base_dir.join("grandchild-drop.pid");

        let mut cmd = build_grandchild_lock_command(&lock_file, &pid_file);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        ProcessTreeGuard::configure_command(&mut cmd);

        let mut child = cmd.spawn().expect("spawn parent shell");
        let guard = ProcessTreeGuard::attach(&child);
        assert!(guard.is_attached());

        let wait_deadline = Instant::now() + Duration::from_secs(10);
        let grandchild_pid: u32 = loop {
            if let Ok(raw) = fs::read_to_string(&pid_file)
                && let Ok(pid) = raw.trim().parse::<u32>()
                && lock_file.exists()
            {
                break pid;
            }
            assert!(
                Instant::now() < wait_deadline,
                "timed out waiting for grandchild lock holder"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        };

        // Trigger purely via `Drop` (simulating abrupt handle drop / cancellation).
        drop(guard);
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;

        // Wait briefly for OS kernel async handle teardown after non-blocking Drop.
        let remove_deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if !is_pid_alive(grandchild_pid) && fs::remove_dir_all(&base_dir).is_ok() {
                break;
            }
            assert!(
                Instant::now() < remove_deadline,
                "grandchild {grandchild_pid} or its file lock survived ProcessTreeGuard Drop"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[cfg(windows)]
    fn build_grandchild_lock_command(
        lock_file: &std::path::Path,
        pid_file: &std::path::Path,
    ) -> Command {
        // Parent: `cmd.exe /c powershell ...` -> Grandchild: `powershell.exe` holding an exclusive
        // writable FileStream (`FileShare::None`) on `lock_file` for 30s.
        let lock_escaped = lock_file.to_string_lossy().replace('\'', "''");
        let pid_escaped = pid_file.to_string_lossy().replace('\'', "''");
        let ps_script = format!(
            "$fs = [System.IO.File]::Open('{lock_escaped}', [System.IO.FileMode]::Create, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None); \
             [System.IO.File]::WriteAllText('{pid_escaped}', [string]$PID); \
             Start-Sleep -Seconds 30; \
             $fs.Dispose()"
        );
        let mut cmd = Command::new("cmd.exe");
        cmd.args([
            "/C",
            "powershell.exe",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &ps_script,
        ]);
        cmd
    }

    #[cfg(unix)]
    fn build_grandchild_lock_command(
        lock_file: &std::path::Path,
        pid_file: &std::path::Path,
    ) -> Command {
        // Parent: `sh -c` -> Grandchild: backgrounded subshell/sleep holding open writable fd.
        let script = format!("(exec 3>\"$1\"; echo $$ > \"$2\"; sleep 30) & wait");
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &script, "sh"]).arg(lock_file).arg(pid_file);
        cmd
    }

    #[cfg(windows)]
    fn is_pid_alive(pid: u32) -> bool {
        // Check via `tasklist /FI "PID eq <pid>" /NH` — zero extra unsafe code in tests.
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .is_ok_and(|out| {
                let text = String::from_utf8_lossy(&out.stdout);
                text.contains(&format!("\"{pid}\""))
            })
    }

    #[cfg(unix)]
    fn is_pid_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|s| s.success())
    }
}
