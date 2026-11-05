use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_OUTBOUND, WriteFile,
};
use windows::Win32::System::Com::CoCreateGuid;
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::WaitForMultipleObjects;
use windows::core::PCWSTR;

use super::flow::Event;

pub(super) struct PcmPipe {
    handle: OwnedHandle,
    pub(super) name: String,
    event: Event,
}

impl PcmPipe {
    pub(super) fn create() -> Result<Self> {
        // SAFETY: COM GUID generation has no borrowed inputs or apartment requirement.
        let name = format!(r"\\.\pipe\Chronobreak.Audio.{:?}", unsafe {
            CoCreateGuid()?
        });
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // OWNER RIGHTS grants the creating token's owner only; no inherited/world ACL.
        let sddl: Vec<u16> = "D:P(A;;GA;;;OW)".encode_utf16().chain(Some(0)).collect();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: terminated SDDL and writable descriptor output.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?
        };
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        // SAFETY: security descriptor and terminated name live through creation.
        let raw = unsafe {
            CreateNamedPipeW(
                PCWSTR(wide.as_ptr()),
                PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                32768,
                32768,
                0,
                Some(&attributes),
            )
        };
        // SAFETY: descriptor came from LocalAlloc; pipe creation copied its ACL.
        let _ = unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
        if raw.is_invalid() {
            return Err(std::io::Error::last_os_error()).context("create private PCM pipe");
        }
        // SAFETY: unique valid ownership from CreateNamedPipeW.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
        Ok(Self {
            handle,
            name,
            event: Event::new(true)?,
        })
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.handle.as_raw_handle())
    }

    fn complete(
        &self,
        operation: &OVERLAPPED,
        wake: HANDLE,
        deadline: impl Fn() -> Instant,
    ) -> Result<u32> {
        loop {
            let remaining = deadline().saturating_duration_since(Instant::now());
            let millis =
                u32::try_from(remaining.as_millis().saturating_add(1)).unwrap_or(u32::MAX - 1);
            // SAFETY: both handles/operation stay owned and stable for the entire wait.
            let wait =
                unsafe { WaitForMultipleObjects(&[self.event.handle(), wake], false, millis) };
            if wait == WAIT_OBJECT_0 {
                let mut count = 0;
                // SAFETY: completed operation and its buffer are still alive.
                unsafe {
                    GetOverlappedResult(self.handle(), operation, &mut count, false)?;
                }
                return Ok(count);
            }
            if wait == WAIT_TIMEOUT && Instant::now() >= deadline() {
                // Cancellation is not completion. Retain the operation, buffer and event
                // until the OS acknowledges; this owner has no GPU/video ownership.
                let count = self.cancel_and_complete(operation);
                bail!("PCM pipe deadline expired (completed {count} bytes)");
            }
            if wait != WAIT_TIMEOUT && wait.0 != WAIT_OBJECT_0.0 + 1 {
                let error = std::io::Error::last_os_error();
                self.cancel_and_complete(operation);
                return Err(error).context("PCM pipe wait failed");
            }
        }
    }

    fn cancel_and_complete(&self, operation: &OVERLAPPED) -> u32 {
        let mut count = 0;
        // SAFETY: the caller retains the operation and borrowed buffer until
        // cancellation/completion is acknowledged, including failed wait paths.
        unsafe {
            let _ = CancelIoEx(self.handle(), Some(operation));
            let _ = GetOverlappedResult(self.handle(), operation, &mut count, true);
        }
        count
    }

    pub(super) fn connect(&self, wake: HANDLE, deadline: impl Fn() -> Instant) -> Result<()> {
        self.event.reset()?;
        let mut operation = OVERLAPPED {
            hEvent: self.event.handle(),
            ..Default::default()
        };
        // SAFETY: operation/event remain stable through completion or acknowledged cancellation.
        match unsafe { ConnectNamedPipe(self.handle(), Some(&mut operation)) } {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => Ok(()),
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                self.complete(&operation, wake, deadline).map(|_| ())
            }
            Err(e) => Err(e.into()),
        }
    }

    pub(super) fn write(
        &self,
        bytes: &[u8],
        wake: HANDLE,
        deadline: impl Fn() -> Instant,
    ) -> Result<()> {
        let mut offset = 0;
        while offset < bytes.len() {
            self.event.reset()?;
            let mut operation = OVERLAPPED {
                hEvent: self.event.handle(),
                ..Default::default()
            };
            // SAFETY: byte slice, overlapped and event live until completion is observed.
            let count = match unsafe {
                WriteFile(
                    self.handle(),
                    Some(&bytes[offset..]),
                    None,
                    Some(&mut operation),
                )
            } {
                Ok(()) => {
                    let mut count = 0;
                    // SAFETY: synchronous completion still retains the operation and borrowed buffer.
                    unsafe {
                        GetOverlappedResult(self.handle(), &operation, &mut count, true)?;
                    }
                    count
                }
                Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                    self.complete(&operation, wake, &deadline)?
                }
                Err(e) => return Err(e.into()),
            };
            if count == 0 {
                bail!("PCM pipe made no write progress");
            }
            offset += count as usize;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::Duration;

    #[test]
    fn repeated_overlapped_writes_and_blocked_write_cancellation_release_resources() {
        let pipe = PcmPipe::create().unwrap();
        let name = pipe.name.clone();
        let (release, released) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut input = std::fs::File::open(name).unwrap();
            let mut bytes = vec![0; 2 * 65_536];
            input.read_exact(&mut bytes).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            bytes
        });
        let wake = Event::new(false).unwrap();
        pipe.connect(wake.handle(), || Instant::now() + Duration::from_secs(1))
            .unwrap();
        let block = vec![0x55; 65_536];
        let deadline = Instant::now() + Duration::from_secs(2);
        pipe.write(&block, wake.handle(), || deadline).unwrap();
        pipe.write(&block, wake.handle(), || deadline).unwrap();
        // The actual reader now waits without consuming the pipe. This third
        // write exceeds its 32-KiB capacity and must be cancelled/acknowledged.
        let stop = Instant::now() + Duration::from_millis(100);
        let result = pipe.write(&block, wake.handle(), || stop);
        release.send(()).unwrap();
        let bytes = reader.join().unwrap();
        assert!(bytes.iter().all(|byte| *byte == 0x55));
        assert!(result.unwrap_err().to_string().contains("deadline expired"));
        assert!(Instant::now().duration_since(stop) < Duration::from_secs(1));
    }
}
