use crate::DiagnosticsError;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt, OpenOptionsMaybeDirExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};
use fs2::FileExt;
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

const LOCK_RETRY_ATTEMPTS: usize = 200;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(5);

/// An open directory capability. All children are opened relative to this
/// handle, so a renamed or replaced display path cannot redirect later I/O.
#[derive(Clone)]
pub(crate) struct SecureDir {
    inner: Arc<Dir>,
    display_path: Arc<PathBuf>,
}

pub(crate) fn lock_exclusive_bounded(
    file: &File,
    operation: &'static str,
) -> Result<(), DiagnosticsError> {
    for attempt in 0..LOCK_RETRY_ATTEMPTS {
        match FileExt::try_lock_exclusive(file) {
            Ok(()) => return Ok(()),
            Err(source) if lock_is_contended(&source) => {
                if attempt + 1 == LOCK_RETRY_ATTEMPTS {
                    return Err(DiagnosticsError::io(operation, source));
                }
                thread::sleep(LOCK_RETRY_DELAY);
            }
            Err(source) => return Err(DiagnosticsError::io(operation, source)),
        }
    }
    unreachable!("bounded lock loop always returns")
}

fn lock_is_contended(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::WouldBlock || matches!(error.raw_os_error(), Some(32 | 33))
}

pub(crate) struct SecureEntry {
    pub(crate) name: OsString,
    pub(crate) metadata: Metadata,
}

impl SecureDir {
    pub(crate) fn open_existing_profile(path: &Path) -> Result<Self, DiagnosticsError> {
        if !path.is_absolute() {
            return Err(DiagnosticsError::InvalidProfileRoot);
        }
        let parent = path.parent().ok_or(DiagnosticsError::InvalidProfileRoot)?;
        let name = path
            .file_name()
            .ok_or(DiagnosticsError::InvalidProfileRoot)?;
        let parent = Dir::open_ambient_dir(parent, ambient_authority())
            .map_err(|source| DiagnosticsError::io("open_profile_parent", source))?;
        let dir = open_child_dir_nofollow(&parent, name)
            .map_err(|source| DiagnosticsError::io("open_profile_root", source))?;
        Ok(Self {
            inner: Arc::new(dir),
            display_path: Arc::new(path.to_path_buf()),
        })
    }

    pub(crate) fn open_or_create_profile(path: &Path) -> Result<Self, DiagnosticsError> {
        if !path.is_absolute() {
            return Err(DiagnosticsError::InvalidProfileRoot);
        }
        let mut ancestor = path;
        loop {
            match std::fs::symlink_metadata(ancestor) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => break,
                Ok(_) | Err(_) => {
                    ancestor = ancestor
                        .parent()
                        .ok_or(DiagnosticsError::InvalidProfileRoot)?;
                }
            }
        }
        let mut current = Dir::open_ambient_dir(ancestor, ambient_authority())
            .map_err(|source| DiagnosticsError::io("open_profile_ancestor", source))?;
        let mut display = ancestor.to_path_buf();
        for component in path
            .strip_prefix(ancestor)
            .map_err(|_| DiagnosticsError::InvalidProfileRoot)?
            .components()
        {
            let Component::Normal(name) = component else {
                return Err(DiagnosticsError::InvalidProfileRoot);
            };
            let next_display = display.join(name);
            current = open_or_create_child_dir(&current, name, &next_display)?;
            display = next_display;
        }
        Ok(Self {
            inner: Arc::new(current),
            display_path: Arc::new(path.to_path_buf()),
        })
    }

    pub(crate) fn open_or_create_child(
        &self,
        name: &str,
        operation: &'static str,
    ) -> Result<Self, DiagnosticsError> {
        validate_name(name)?;
        let display = self.display_path.join(name);
        let dir = open_or_create_child_dir(&self.inner, OsStr::new(name), &display)
            .map_err(|error| remap_io_operation(error, operation))?;
        Ok(Self {
            inner: Arc::new(dir),
            display_path: Arc::new(display),
        })
    }

    pub(crate) fn open_child(
        &self,
        name: &str,
        operation: &'static str,
    ) -> Result<Self, DiagnosticsError> {
        validate_name(name)?;
        let dir = open_child_dir_nofollow(&self.inner, OsStr::new(name))
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        Ok(Self {
            inner: Arc::new(dir),
            display_path: Arc::new(self.display_path.join(name)),
        })
    }

    pub(crate) fn create_new_file(
        &self,
        name: &str,
        read: bool,
        operation: &'static str,
    ) -> Result<File, DiagnosticsError> {
        validate_name(name)?;
        let mut options = OpenOptions::new();
        options
            .read(read)
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        configure_new_file_mode(&mut options);
        let file = self
            .inner
            .open_with(name, &options)
            .map_err(|source| DiagnosticsError::io(operation, source))?
            .into_std();
        secure_file(&file)?;
        Ok(file)
    }

    pub(crate) fn open_lock_file(
        &self,
        name: &str,
        create: bool,
        operation: &'static str,
    ) -> Result<File, DiagnosticsError> {
        validate_name(name)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(create)
            .follow(FollowSymlinks::No);
        configure_new_file_mode(&mut options);
        let file = self
            .inner
            .open_with(name, &options)
            .map_err(|source| DiagnosticsError::io(operation, source))?
            .into_std();
        secure_file(&file)?;
        Ok(file)
    }

    pub(crate) fn open_existing_file(
        &self,
        name: &str,
        write: bool,
        operation: &'static str,
    ) -> Result<File, DiagnosticsError> {
        validate_name(name)?;
        let mut options = OpenOptions::new();
        options.read(true).write(write).follow(FollowSymlinks::No);
        let file = self
            .inner
            .open_with(name, &options)
            .map_err(|source| DiagnosticsError::io(operation, source))?
            .into_std();
        let metadata = file
            .metadata()
            .map_err(|source| DiagnosticsError::io("read_open_file_metadata", source))?;
        if !metadata.is_file() {
            return Err(DiagnosticsError::InvalidDiagnosticEntry);
        }
        Ok(file)
    }

    pub(crate) fn read_bounded(
        &self,
        name: &str,
        max_bytes: u64,
        operation: &'static str,
    ) -> Result<Vec<u8>, DiagnosticsError> {
        let mut file = self.open_existing_file(name, false, operation)?;
        let metadata = file
            .metadata()
            .map_err(|source| DiagnosticsError::io("read_bounded_metadata", source))?;
        if metadata.len() > max_bytes {
            return Err(DiagnosticsError::DiagnosticEntryTooLarge);
        }
        let capacity = usize::try_from(metadata.len())
            .map_err(|_| DiagnosticsError::DiagnosticEntryTooLarge)?;
        let mut bytes = Vec::with_capacity(capacity);
        Read::by_ref(&mut file)
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
            return Err(DiagnosticsError::DiagnosticEntryTooLarge);
        }
        Ok(bytes)
    }

    pub(crate) fn entries_bounded(
        &self,
        max_entries: usize,
        operation: &'static str,
    ) -> Result<Vec<SecureEntry>, DiagnosticsError> {
        let entries = self
            .inner
            .entries()
            .map_err(|source| DiagnosticsError::io(operation, source))?;
        let mut output = Vec::new();
        for entry in entries {
            if output.len() == max_entries {
                return Err(DiagnosticsError::DiagnosticEntryLimit);
            }
            let entry = entry.map_err(|source| DiagnosticsError::io(operation, source))?;
            let name = entry.file_name();
            let metadata = self
                .inner
                .symlink_metadata(&name)
                .map_err(|source| DiagnosticsError::io(operation, source))?;
            output.push(SecureEntry { name, metadata });
        }
        Ok(output)
    }

    pub(crate) fn remove_file(
        &self,
        name: &str,
        operation: &'static str,
    ) -> Result<(), DiagnosticsError> {
        validate_name(name)?;
        match self.inner.remove_file(name) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(DiagnosticsError::io(operation, source)),
        }
    }

    pub(crate) fn rename(
        &self,
        from: &str,
        to: &str,
        operation: &'static str,
    ) -> Result<(), DiagnosticsError> {
        validate_name(from)?;
        validate_name(to)?;
        self.inner
            .rename(from, &self.inner, to)
            .map_err(|source| DiagnosticsError::io(operation, source))
    }

    pub(crate) fn metadata(
        &self,
        name: &str,
        operation: &'static str,
    ) -> Result<Metadata, DiagnosticsError> {
        validate_name(name)?;
        self.inner
            .symlink_metadata(name)
            .map_err(|source| DiagnosticsError::io(operation, source))
    }

    pub(crate) fn try_exists(&self, name: &str) -> Result<bool, DiagnosticsError> {
        validate_name(name)?;
        self.inner
            .try_exists(name)
            .map_err(|source| DiagnosticsError::io("inspect_diagnostic_entry", source))
    }
}

fn open_or_create_child_dir(
    parent: &Dir,
    name: &OsStr,
    display: &Path,
) -> Result<Dir, DiagnosticsError> {
    match open_child_dir_nofollow(parent, name) {
        Ok(dir) => {
            secure_directory(&dir)?;
            Ok(dir)
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            match parent.create_dir(name) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(source) => {
                    return Err(DiagnosticsError::io("create_secure_directory", source));
                }
            }
            let dir = open_child_dir_nofollow(parent, name)
                .map_err(|source| DiagnosticsError::io("open_secure_directory", source))?;
            secure_directory(&dir)?;
            Ok(dir)
        }
        Err(source) => Err(DiagnosticsError::io(
            if display.exists() {
                "reject_reparse_directory"
            } else {
                "open_secure_directory"
            },
            source,
        )),
    }
}

fn open_child_dir_nofollow(parent: &Dir, name: &OsStr) -> std::io::Result<Dir> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .maybe_dir(true);
    configure_directory_access(&mut options);
    let file = parent.open_with(name, &options)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "diagnostic entry is not a directory",
        ));
    }
    #[cfg(windows)]
    {
        let file = file.into_std();
        windows_acl::secure_current_user_only(&file)?;
        Dir::reopen_dir(&file)
    }
    #[cfg(not(windows))]
    {
        Dir::reopen_dir(&file)
    }
}

fn validate_name(name: &str) -> Result<(), DiagnosticsError> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err(DiagnosticsError::InvalidDiagnosticEntry);
    }
    Ok(())
}

fn remap_io_operation(error: DiagnosticsError, operation: &'static str) -> DiagnosticsError {
    match error {
        DiagnosticsError::Io { source, .. } => DiagnosticsError::io(operation, source),
        other => other,
    }
}

#[cfg(unix)]
fn configure_new_file_mode(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_new_file_mode(options: &mut OpenOptions) {
    configure_private_file_access(options);
}

#[cfg(windows)]
fn configure_directory_access(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, WRITE_DAC,
    };
    options.access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC);
}

#[cfg(not(windows))]
fn configure_directory_access(_options: &mut OpenOptions) {}

#[cfg(windows)]
fn configure_private_file_access(options: &mut OpenOptions) {
    use cap_std::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, WRITE_DAC,
    };
    options.access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC);
}

#[cfg(not(windows))]
fn configure_private_file_access(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn secure_directory(dir: &Dir) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::PermissionsExt;
    let file = dir
        .try_clone()
        .map_err(|source| DiagnosticsError::io("clone_secure_directory", source))?
        .into_std_file();
    file.set_permissions(std::fs::Permissions::from_mode(0o700))
        .map_err(|source| DiagnosticsError::io("secure_diagnostic_directory", source))
}

#[cfg(unix)]
fn secure_file(file: &File) -> Result<(), DiagnosticsError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|source| DiagnosticsError::io("secure_diagnostic_file", source))
}

#[cfg(windows)]
fn secure_directory(_dir: &Dir) -> Result<(), DiagnosticsError> {
    // The directory is secured on the original no-follow handle before it is
    // reopened as a capability. Reopened directory handles need not retain
    // WRITE_DAC on Windows.
    Ok(())
}

#[cfg(windows)]
fn secure_file(file: &File) -> Result<(), DiagnosticsError> {
    windows_acl::secure_current_user_only(file)
        .map_err(|source| DiagnosticsError::io("secure_diagnostic_file", source))
}

#[cfg(not(any(unix, windows)))]
fn secure_directory(_dir: &Dir) -> Result<(), DiagnosticsError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn secure_file(_file: &File) -> Result<(), DiagnosticsError> {
    Ok(())
}

#[cfg(windows)]
mod windows_acl {
    use std::{ffi::c_void, fs::File, io, os::windows::io::AsRawHandle, ptr, sync::OnceLock};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            DACL_SECURITY_INFORMATION, GetLengthSid, GetTokenInformation,
            PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SetKernelObjectSecurity,
            TOKEN_QUERY, TOKEN_USER, TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    static CURRENT_USER_SID: OnceLock<String> = OnceLock::new();

    pub(super) fn secure_current_user_only(file: &File) -> io::Result<()> {
        let sid = CURRENT_USER_SID.get_or_init(|| current_user_sid().unwrap_or_default());
        if sid.is_empty() {
            return Err(io::Error::other("current user SID unavailable"));
        }
        let sddl = format!("D:P(A;;GA;;;{sid})");
        let encoded = sddl
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: encoded is NUL-terminated and descriptor is writable output storage.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                encoded.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                ptr::null_mut(),
            )
        } == 0
            || descriptor.is_null()
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the file handle is live and descriptor is a valid allocation.
        let applied = unsafe {
            SetKernelObjectSecurity(
                file.as_raw_handle().cast(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        };
        // SAFETY: the converter allocates with LocalAlloc.
        unsafe { LocalFree(descriptor.cast::<c_void>()) };
        if applied == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn current_user_sid() -> io::Result<String> {
        let mut token = ptr::null_mut();
        // SAFETY: token is writable and GetCurrentProcess returns a valid pseudo-handle.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = token_user_sid(token);
        // SAFETY: token is owned by this function.
        unsafe { CloseHandle(token) };
        result
    }

    fn token_user_sid(token: *mut c_void) -> io::Result<String> {
        let mut required = 0_u32;
        // SAFETY: first call requests the required buffer size.
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &raw mut required);
        }
        if required == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0_u8; required as usize];
        // SAFETY: buffer has the size requested by Windows.
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &raw mut required,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: TokenUser initialized TOKEN_USER at the start of the buffer.
        let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
        let sid = user.User.Sid;
        if sid.is_null() || unsafe { GetLengthSid(sid) } == 0 {
            return Err(io::Error::other("invalid current user SID"));
        }
        let mut raw = ptr::null_mut();
        // SAFETY: sid is valid and raw is writable output storage.
        if unsafe { ConvertSidToStringSidW(sid, &raw mut raw) } == 0 || raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        // SAFETY: converter returns a NUL-terminated allocation.
        while unsafe { *raw.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: the loop found the terminator inside the allocation.
        let value = String::from_utf16(unsafe { std::slice::from_raw_parts(raw, len) })
            .map_err(|_| io::Error::other("invalid SID encoding"));
        // SAFETY: converter allocates with LocalAlloc.
        unsafe { LocalFree(raw.cast::<c_void>()) };
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_child_rejects_a_preplanted_directory_link() {
        let temp = tempfile::tempdir().expect("temporary profile parent");
        let profile = temp.path().join("profile");
        std::fs::create_dir(&profile).expect("profile");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        let link = profile.join("diagnostics");
        if !create_directory_link(&outside, &link) {
            return;
        }
        let root = SecureDir::open_or_create_profile(&profile).expect("secure profile");
        assert!(
            root.open_or_create_child("diagnostics", "open_diagnostics")
                .is_err()
        );
        assert!(!outside.join("lifecycle").exists());
    }

    #[test]
    fn profile_root_rejects_a_preplanted_directory_link() {
        let temp = tempfile::tempdir().expect("temporary profile parent");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        let link = temp.path().join("profile-link");
        if !create_directory_link(&outside, &link) {
            return;
        }

        assert!(SecureDir::open_or_create_profile(&link).is_err());
        assert!(!outside.join("control.sqlite3").exists());
    }

    #[test]
    fn bounded_reader_rejects_links_and_large_files() {
        let temp = tempfile::tempdir().expect("temporary profile");
        let root = SecureDir::open_or_create_profile(temp.path()).expect("secure profile");
        let diagnostics = root
            .open_or_create_child("diagnostics", "diagnostics")
            .expect("diagnostics");
        std::fs::write(
            temp.path().join("diagnostics").join("large"),
            vec![0_u8; 128],
        )
        .expect("large file");
        assert!(matches!(
            diagnostics.read_bounded("large", 64, "read"),
            Err(DiagnosticsError::DiagnosticEntryTooLarge)
        ));
    }

    #[test]
    fn open_capability_is_not_redirected_by_a_directory_swap() {
        let temp = tempfile::tempdir().expect("temporary profile");
        let root = SecureDir::open_or_create_profile(temp.path()).expect("secure profile");
        let diagnostics = root
            .open_or_create_child("diagnostics", "diagnostics")
            .expect("diagnostics");
        let original = temp.path().join("diagnostics");
        let moved = temp.path().join("diagnostics-open-handle");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        if let Err(error) = std::fs::rename(&original, &moved) {
            assert!(
                matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Other
                ) || matches!(error.raw_os_error(), Some(32 | 33)),
                "unexpected directory swap error: {error}"
            );
            return;
        }
        if !create_directory_link(&outside, &original) {
            return;
        }

        diagnostics
            .open_or_create_child("lifecycle", "lifecycle")
            .expect("handle-relative lifecycle");
        assert!(moved.join("lifecycle").is_dir());
        assert!(!outside.join("lifecycle").exists());
    }

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        std::os::unix::fs::symlink(target, link).expect("directory symlink");
        true
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) -> bool {
        if let Err(error) = std::os::windows::fs::symlink_dir(target, link) {
            eprintln!("skipping symlink assertion: {error}");
            return false;
        }
        true
    }
}
