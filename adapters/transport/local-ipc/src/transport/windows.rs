use super::{LocalEndpoint, PeerIdentity, TransportError};
use async_stream::try_stream;
use futures_core::Stream;
use hyper_util::rt::TokioIo;
use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tonic::transport::server::Connected;
use tower::service_fn;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

const CONNECT_RETRIES: usize = 80;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(25);

pub(crate) async fn connect_channel(
    endpoint: &LocalEndpoint,
) -> Result<tonic::transport::Channel, TransportError> {
    let pipe_name = endpoint.pipe_name().to_owned();
    let expected_sid = current_process_user_sid()?;
    let connector = service_fn(move |_| {
        let pipe_name = pipe_name.clone();
        let expected_sid = expected_sid.clone();
        async move {
            let pipe = open_pipe_with_retry(&pipe_name).await?;
            validate_server_peer(&pipe, &expected_sid)
                .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
            Ok::<_, io::Error>(TokioIo::new(pipe))
        }
    });
    tonic::transport::Endpoint::from_static("http://dennett.local")
        .connect_timeout(Duration::from_secs(3))
        .connect_with_connector(connector)
        .await
        .map_err(TransportError::from)
}

pub(crate) fn secure_incoming(
    endpoint: LocalEndpoint,
) -> Result<impl Stream<Item = Result<AuthenticatedPipe, io::Error>>, TransportError> {
    let expected_sid = current_process_user_sid()?;
    let pipe_name = endpoint.pipe_name().to_owned();
    Ok(try_stream! {
        let mut first_instance = true;
        let mut listener = create_secure_pipe(&pipe_name, &expected_sid, first_instance)?;
        first_instance = false;
        loop {
            listener.connect().await?;
            let next = create_secure_pipe(&pipe_name, &expected_sid, first_instance)?;
            match validate_client_peer(&listener, &expected_sid) {
                Ok(peer) => yield AuthenticatedPipe { inner: listener, peer },
                Err(error) => {
                    tracing::warn!(error = %error, "rejected local IPC client before gRPC");
                }
            }
            listener = next;
        }
    })
}

#[derive(Debug)]
pub(crate) struct AuthenticatedPipe {
    inner: NamedPipeServer,
    peer: PeerIdentity,
}

impl Connected for AuthenticatedPipe {
    type ConnectInfo = PeerIdentity;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.peer.clone()
    }
}

impl AsyncRead for AuthenticatedPipe {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buffer)
    }
}

impl AsyncWrite for AuthenticatedPipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

async fn open_pipe_with_retry(pipe_name: &str) -> io::Result<NamedPipeClient> {
    for attempt in 0..CONNECT_RETRIES {
        match ClientOptions::new().open(pipe_name) {
            Ok(pipe) => return Ok(pipe),
            Err(error)
                if attempt + 1 < CONNECT_RETRIES
                    && (error.kind() == io::ErrorKind::NotFound
                        || error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)) =>
            {
                tokio::time::sleep(CONNECT_RETRY_DELAY).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("bounded connection loop returns on its last attempt")
}

fn create_secure_pipe(
    pipe_name: &str,
    current_sid: &str,
    first_instance: bool,
) -> io::Result<NamedPipeServer> {
    let descriptor = SecurityDescriptor::current_user_only(current_sid)?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.raw.cast::<c_void>(),
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first_instance)
        .reject_remote_clients(true)
        .max_instances(16);
    // SAFETY: `attributes` and its descriptor remain alive for the call. Windows
    // copies the descriptor into the pipe object; Tokio takes sole ownership of
    // the returned handle.
    unsafe {
        options
            .create_with_security_attributes_raw(pipe_name, (&raw mut attributes).cast::<c_void>())
    }
}

fn validate_client_peer(
    pipe: &NamedPipeServer,
    expected_sid: &str,
) -> Result<PeerIdentity, TransportError> {
    let process_id = named_pipe_peer_pid(pipe.as_raw_handle(), true)?;
    peer_identity(process_id, expected_sid)
}

fn validate_server_peer(
    pipe: &NamedPipeClient,
    expected_sid: &str,
) -> Result<PeerIdentity, TransportError> {
    let process_id = named_pipe_peer_pid(pipe.as_raw_handle(), false)?;
    peer_identity(process_id, expected_sid)
}

fn named_pipe_peer_pid(handle: RawHandle, client: bool) -> Result<u32, TransportError> {
    let mut process_id = 0_u32;
    // SAFETY: handle is a live named-pipe handle and process_id is writable.
    let result = unsafe {
        if client {
            GetNamedPipeClientProcessId(handle as HANDLE, &raw mut process_id)
        } else {
            GetNamedPipeServerProcessId(handle as HANDLE, &raw mut process_id)
        }
    };
    if result == 0 || process_id == 0 {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    Ok(process_id)
}

fn peer_identity(process_id: u32, expected_sid: &str) -> Result<PeerIdentity, TransportError> {
    let actual_sid = process_user_sid(process_id)?;
    if actual_sid != expected_sid {
        return Err(TransportError::PeerUserMismatch);
    }
    Ok(PeerIdentity {
        process_id,
        user_sid: actual_sid,
        connection_id: random_hex(16)?,
    })
}

fn current_process_user_sid() -> Result<String, TransportError> {
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle for this process.
    process_handle_user_sid(unsafe { GetCurrentProcess() }, false)
}

fn process_user_sid(process_id: u32) -> Result<String, TransportError> {
    // SAFETY: no pointer arguments; the returned handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    let result = process_handle_user_sid(process, true);
    // SAFETY: process is an owned handle returned by OpenProcess.
    unsafe { CloseHandle(process) };
    result
}

fn process_handle_user_sid(handle: HANDLE, close_process: bool) -> Result<String, TransportError> {
    let mut token = ptr::null_mut();
    // SAFETY: token points to writable handle storage and handle is a process handle.
    if unsafe { OpenProcessToken(handle, TOKEN_QUERY, &raw mut token) } == 0 {
        if close_process {
            tracing::debug!("could not open peer process token");
        }
        return Err(TransportError::PeerIdentityUnavailable);
    }
    let result = token_user_sid(token);
    // SAFETY: token is an owned handle returned by OpenProcessToken.
    unsafe { CloseHandle(token) };
    result
}

fn token_user_sid(token: HANDLE) -> Result<String, TransportError> {
    let mut required = 0_u32;
    // SAFETY: first call intentionally asks Windows for the required buffer size.
    unsafe {
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &raw mut required);
    }
    if required == 0 {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    let mut buffer = vec![0_u8; required as usize];
    // SAFETY: buffer has the required size and remains live while TOKEN_USER is read.
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            required,
            &raw mut required,
        )
    } == 0
    {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    // SAFETY: GetTokenInformation(TokenUser) initialized a TOKEN_USER in buffer.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_to_string(user.User.Sid)
}

fn sid_to_string(sid: PSID) -> Result<String, TransportError> {
    // GetLengthSid also rejects malformed pointers through a zero result.
    if sid.is_null() || unsafe { GetLengthSid(sid) } == 0 {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    let mut raw = ptr::null_mut();
    // SAFETY: sid points into a live token buffer and raw is writable output storage.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut raw) } == 0 || raw.is_null() {
        return Err(TransportError::PeerIdentityUnavailable);
    }
    let mut len = 0;
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated allocation.
    while unsafe { *raw.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the previous loop found the allocation's NUL terminator.
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(raw, len) })
        .map_err(|_| TransportError::PeerIdentityUnavailable);
    // SAFETY: ConvertSidToStringSidW allocates with LocalAlloc.
    unsafe { LocalFree(raw.cast::<c_void>()) };
    value
}

fn random_hex(bytes: usize) -> Result<String, TransportError> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(super::hex_prefix(&random, bytes))
}

struct SecurityDescriptor {
    raw: PSECURITY_DESCRIPTOR,
}

impl SecurityDescriptor {
    fn current_user_only(current_sid: &str) -> io::Result<Self> {
        let sddl = format!("D:P(A;;GA;;;{current_sid})");
        let encoded: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut raw = ptr::null_mut();
        // SAFETY: encoded is a valid NUL-terminated SDDL string; raw is writable.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                encoded.as_ptr(),
                SDDL_REVISION_1,
                &raw mut raw,
                ptr::null_mut(),
            )
        } == 0
            || raw.is_null()
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { raw })
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: descriptor was allocated by LocalAlloc through the converter.
            unsafe { LocalFree(self.raw.cast::<c_void>()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;

    #[test]
    fn current_user_security_descriptor_is_constructible() {
        let sid = current_process_user_sid().expect("current SID");
        let descriptor = SecurityDescriptor::current_user_only(&sid).expect("descriptor");
        assert!(!descriptor.raw.is_null());
    }

    #[tokio::test]
    async fn created_pipe_applies_only_the_current_user_dacl() {
        let sid = current_process_user_sid().expect("current SID");
        let endpoint = LocalEndpoint::for_installation(format!(
            "acl-test-{}",
            random_hex(8).expect("random endpoint suffix")
        ))
        .expect("endpoint");
        let pipe = create_secure_pipe(endpoint.pipe_name(), &sid, true).expect("secure pipe");
        let actual = pipe_dacl_sddl(pipe.as_raw_handle()).expect("pipe DACL");
        assert_eq!(actual, format!("D:P(A;;FA;;;{sid})"));
    }

    fn pipe_dacl_sddl(handle: RawHandle) -> io::Result<String> {
        let mut descriptor = ptr::null_mut();
        // SAFETY: handle is a live pipe object and descriptor is writable output storage.
        let status = unsafe {
            GetSecurityInfo(
                handle as HANDLE,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &raw mut descriptor,
            )
        };
        if status != 0 || descriptor.is_null() {
            return Err(io::Error::from_raw_os_error(status as i32));
        }

        let mut encoded = ptr::null_mut();
        let mut encoded_len = 0_u32;
        // SAFETY: descriptor came from GetSecurityInfo; output pointers are writable.
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &raw mut encoded,
                &raw mut encoded_len,
            )
        };
        if converted == 0 || encoded.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: descriptor was allocated by the local allocator.
            unsafe { LocalFree(descriptor.cast::<c_void>()) };
            return Err(error);
        }

        // SAFETY: the converter returned encoded_len initialized UTF-16 code units.
        let value = String::from_utf16(unsafe {
            std::slice::from_raw_parts(encoded, encoded_len as usize)
        })
        .map(|value| value.trim_end_matches('\0').to_owned())
        .map_err(io::Error::other);
        // SAFETY: both allocations are documented LocalAlloc results.
        unsafe {
            LocalFree(encoded.cast::<c_void>());
            LocalFree(descriptor.cast::<c_void>());
        }
        value
    }
}
