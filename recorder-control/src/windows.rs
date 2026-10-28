//! Windows ownership and restricted, bounded local status transport.
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use tokio::sync::{oneshot, watch};
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{PCWSTR, PWSTR};

use crate::{
    AvailabilityEvent, PROTOCOL_VERSION, REQUEST_TIMEOUT, Status, StatusRequest, read_frame,
    write_frame,
};

const REPARSE: u32 = 0x400;
const OPEN_REPARSE: u32 = 0x0020_0000;
const BACKUP_SEMANTICS: u32 = 0x0200_0000;

/// Process-owned lease. Neither the lock file nor its pinned parent is deleted.
pub struct FileLease {
    _file: File,
    _parent: File,
}

impl FileLease {
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        let parent = path.parent().context("lease has no parent")?;
        std::fs::create_dir_all(parent)?;
        reject_reparse_ancestors(parent)?;
        let parent_handle = OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(OPEN_REPARSE | BACKUP_SEMANTICS)
            .open(parent)?;
        if parent_handle.metadata()?.file_attributes() & REPARSE != 0 {
            bail!("lease parent is a reparse point");
        }
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(1)
            .custom_flags(OPEN_REPARSE)
            .open(path)
        {
            Ok(file) => file,
            Err(e) if e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33) => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & REPARSE != 0 {
            bail!("lease is not a regular file");
        }
        Ok(Some(Self {
            _file: file,
            _parent: parent_handle,
        }))
    }

    /// Bounded blocking acquisition; invoke on a filesystem worker, never an async worker.
    pub fn acquire(path: &Path, deadline: Duration) -> Result<Self> {
        let started = Instant::now();
        loop {
            if let Some(lease) = Self::try_acquire(path)? {
                return Ok(lease);
            }
            if started.elapsed() >= deadline {
                bail!("configuration is busy");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn reject_reparse_ancestors(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        if std::fs::symlink_metadata(ancestor)?.file_attributes() & REPARSE != 0 {
            bail!("reparse path is not permitted");
        }
    }
    Ok(())
}

pub fn regular_executable(path: &Path) -> Result<PathBuf> {
    reject_reparse_ancestors(path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(OPEN_REPARSE)
        .open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.file_attributes() & REPARSE != 0 {
        bail!("recorder is not a regular executable");
    }
    Ok(path.canonicalize()?)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn owned_handle(handle: HANDLE) -> OwnedHandle {
    // SAFETY: caller transfers a newly created valid, uniquely owned Win32 handle.
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}

pub fn current_session() -> Result<u32> {
    process_session(std::process::id())
}

fn process_session(pid: u32) -> Result<u32> {
    let mut session = 0;
    // SAFETY: session is a writable u32; no borrowed memory escapes the call.
    unsafe {
        ProcessIdToSessionId(pid, &mut session)?;
    }
    Ok(session)
}

fn process_sid(process: HANDLE) -> Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: process is valid for querying; token is a writable output handle.
    unsafe {
        OpenProcessToken(process, TOKEN_QUERY, &mut token)?;
    }
    let token = owned_handle(token);
    let mut length = 0;
    // SAFETY: zero-length query only obtains required buffer length.
    let _ = unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenUser,
            None,
            0,
            &mut length,
        )
    };
    if length == 0 || length > 65536 {
        bail!("invalid user token length");
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    // SAFETY: buffer is sufficiently large and aligned for TOKEN_USER and owned during use.
    unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &mut length,
        )?;
    }
    // SAFETY: successful TokenUser query initialized TOKEN_USER at this aligned address.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = PWSTR::null();
    // SAFETY: user SID remains in the owned buffer; text receives LocalAlloc memory.
    unsafe {
        ConvertSidToStringSidW(user.User.Sid, &mut text)?;
    }
    // SAFETY: conversion returns a NUL-terminated string; copy it before freeing once.
    let result = unsafe { text.to_string() };
    // SAFETY: this is the allocation returned by ConvertSidToStringSidW.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    Ok(result?)
}

pub fn current_sid() -> Result<String> {
    // SAFETY: GetCurrentProcess returns a valid pseudo-handle which is not closed here.
    process_sid(unsafe { GetCurrentProcess() })
}

/// Namespace cannot be changed through media/config path selection.
#[derive(Clone, Debug)]
pub struct Namespace {
    pub lease: PathBuf,
    pub pipe: String,
}

impl Namespace {
    pub fn production() -> Result<Self> {
        let root = directories::BaseDirs::new()
            .context("user profile unavailable")?
            .data_local_dir()
            .join("LeagueReplay");
        Ok(Self {
            lease: root.join("recorder-owner.lock"),
            pipe: format!(r"\\.\pipe\Chronobreak.Recorder.{}", current_sid()?),
        })
    }
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: exactly one release of the LocalAlloc descriptor created below.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}

fn create_server(pipe: &str, first: bool) -> Result<NamedPipeServer> {
    let sddl = wide(&format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", current_sid()?));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: sddl is terminated; descriptor is a writable output initialized by API.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )?;
    }
    let descriptor = SecurityDescriptor(descriptor);
    let mut attributes = windows::Win32::Security::SECURITY_ATTRIBUTES {
        nLength: size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0.0,
        bInheritHandle: false.into(),
    };
    // One active request plus one waiting instance; no unbounded client tasks.
    // SAFETY: security attributes and descriptor remain valid during synchronous creation.
    Ok(unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(2)
            .in_buffer_size(4096)
            .out_buffer_size(4096)
            .create_with_security_attributes_raw(
                pipe,
                (&mut attributes as *mut windows::Win32::Security::SECURITY_ATTRIBUTES)
                    .cast::<c_void>(),
            )?
    })
}

async fn serve(
    mut server: NamedPipeServer,
    name: &str,
    status: watch::Receiver<Status>,
) -> Result<()> {
    loop {
        server.connect().await?;
        // Retain a listening instance before finishing the connected one. Windows
        // disconnect discards unread response bytes; closing after the response
        // preserves them and the waiting instance keeps the namespace occupied.
        let next = create_server(name, false)?;
        let _ = tokio::time::timeout(REQUEST_TIMEOUT, async {
            let request: StatusRequest = read_frame(&mut server).await?;
            if request.protocol_version != PROTOCOL_VERSION {
                bail!("incompatible status request");
            }
            let reply = status.borrow().clone();
            write_frame(&mut server, &reply).await
        })
        .await;
        drop(server);
        server = next;
    }
}

/// Holds the lease until the status listener has been cancelled and joined.
pub struct ControlOwner {
    _lease: FileLease,
    status: watch::Sender<Status>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ControlOwner {
    pub fn start(namespace: &Namespace) -> Result<Option<Self>> {
        let Some(lease) = FileLease::try_acquire(&namespace.lease)? else {
            return Ok(None);
        };
        let mut random = [0; 16];
        getrandom::fill(&mut random)?;
        let instance = random.iter().map(|b| format!("{b:02x}")).collect();
        let (status, rx) = watch::channel(Status::starting(
            std::process::id(),
            current_session()?,
            instance,
        ));
        let (stop, stopped) = oneshot::channel();
        let (ready, readiness) = mpsc::sync_channel(1);
        let pipe = namespace.pipe.clone();
        let failures = status.clone();
        let thread = thread::Builder::new()
            .name("recorder-status".into())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async {
                        let server = create_server(&pipe, true)?;
                        let _ = ready.send(Ok(()));
                        tokio::select! {
                            _ = stopped => Ok(()),
                            result = serve(server, &pipe, rx) => result,
                        }
                    })
                })();
                if let Err(error) = result {
                    failures.send_modify(|s| {
                        s.apply(AvailabilityEvent::ServiceError(
                            crate::ServiceFailure::ControlUnavailable,
                        ))
                    });
                    let _ = ready.send(Err(format!("{error:#}")));
                }
            })?;
        match readiness.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Ok(())) => Ok(Some(Self {
                _lease: lease,
                status,
                stop: Some(stop),
                thread: Some(thread),
            })),
            other => {
                let _ = stop.send(());
                let _ = thread.join();
                bail!("recorder control initialization failed: {other:?}");
            }
        }
    }

    pub fn status_sender(&self) -> watch::Sender<Status> {
        self.status.clone()
    }
    pub fn apply(&self, event: AvailabilityEvent) {
        self.status.send_modify(|s| s.apply(event));
    }
}

impl Drop for ControlOwner {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub async fn query(namespace: &Namespace, expected_executable: &Path) -> Result<Status> {
    tokio::time::timeout(REQUEST_TIMEOUT, async {
        let mut pipe = ClientOptions::new().open(&namespace.pipe)?;
        let mut pid = 0;
        // SAFETY: live pipe handle and writable output PID.
        unsafe {
            GetNamedPipeServerProcessId(HANDLE(pipe.as_raw_handle()), &mut pid)?;
        }
        // SAFETY: bounded query access only; no mutation or termination rights requested.
        let process =
            owned_handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)? });
        if process_sid(HANDLE(process.as_raw_handle()))? != current_sid()? {
            bail!("recorder user mismatch");
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        // SAFETY: path is a writable UTF-16 buffer of the stated length.
        unsafe {
            QueryFullProcessImageNameW(
                HANDLE(process.as_raw_handle()),
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut length,
            )?;
        }
        let actual = PathBuf::from(String::from_utf16(&path[..length as usize])?);
        if !regular_executable(&actual)?
            .as_os_str()
            .eq_ignore_ascii_case(regular_executable(expected_executable)?.as_os_str())
        {
            bail!("recorder executable mismatch");
        }
        write_frame(
            &mut pipe,
            &StatusRequest {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await?;
        let reply: Status = read_frame(&mut pipe).await?;
        reply.validate()?;
        if reply.pid != pid || reply.session_id != process_session(pid)? {
            bail!("recorder process identity mismatch");
        }
        Ok(reply)
    })
    .await
    .context("recorder status request timed out")?
}
