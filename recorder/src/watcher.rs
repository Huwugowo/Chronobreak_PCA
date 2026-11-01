use std::ffi::OsStr;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[cfg(target_os = "windows")]
pub const DEFAULT_PROCESS_NAME: &str = "League of Legends.exe";
#[cfg(target_os = "macos")]
pub const DEFAULT_PROCESS_NAME: &str = "League of Legends";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub const DEFAULT_PROCESS_NAME: &str = "League of Legends";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeagueProcess {
    pub pid: u32,
    /// OS creation identity: exact FILETIME ticks on Windows, Unix seconds elsewhere.
    pub started_at: u64,
}

impl LeagueProcess {
    /// Query only this PID's identity; readiness must not rely on the watcher's
    /// previous two-second snapshot when admitting native resources.
    #[cfg(target_os = "windows")]
    pub(crate) fn is_current(self) -> bool {
        self.started_at != 0 && process_creation_time(self.pid) == Some(self.started_at)
    }
}

#[cfg(target_os = "windows")]
fn process_creation_time(pid: u32) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION).ok()?;
    creation_time_from_handle(HANDLE(process.as_raw_handle())).ok()
}

#[cfg(target_os = "windows")]
fn open_process(
    pid: u32,
    access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS,
) -> anyhow::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows::Win32::System::Threading::OpenProcess;

    // SAFETY: only query/synchronize rights are requested for an OS process ID.
    let handle = unsafe { OpenProcess(access, false, pid) }?;
    // SAFETY: successful OpenProcess transfers one uniquely owned, valid handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

#[cfg(target_os = "windows")]
fn creation_time_from_handle(handle: windows::Win32::Foundation::HANDLE) -> anyhow::Result<u64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetProcessTimes;

    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the owned process handle and all four distinct output pointers are live.
    unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }?;
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Pins the original kernel process object before capture, including after exit.
#[cfg(target_os = "windows")]
pub(crate) struct OriginalProcess {
    identity: LeagueProcess,
    handle: std::os::windows::io::OwnedHandle,
}

#[cfg(target_os = "windows")]
impl OriginalProcess {
    pub(crate) fn identity(&self) -> LeagueProcess {
        self.identity
    }

    pub(crate) fn open(identity: LeagueProcess) -> anyhow::Result<Self> {
        use windows::Win32::System::Threading::{
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };

        let process = Self {
            identity,
            handle: open_process(
                identity.pid,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            )?,
        };
        anyhow::ensure!(
            !process.has_ended()?,
            "League process ended before native startup"
        );
        Ok(process)
    }

    pub(crate) fn has_ended(&self) -> anyhow::Result<bool> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Threading::WaitForSingleObject;

        let handle = HANDLE(self.handle.as_raw_handle());
        let created = creation_time_from_handle(handle);
        // SAFETY: the retained process handle has synchronize rights. Zero only
        // queries its signal state; it never waits for or delays League exit.
        let status = unsafe { WaitForSingleObject(handle, 0) };
        process_exit_status(self.identity.started_at, created, status)
    }
}

#[cfg(target_os = "windows")]
fn process_exit_status(
    expected_creation: u64,
    observed_creation: anyhow::Result<u64>,
    status: windows::Win32::Foundation::WAIT_EVENT,
) -> anyhow::Result<bool> {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};

    anyhow::ensure!(
        expected_creation != 0 && observed_creation? == expected_creation,
        "original League process creation identity could not be confirmed"
    );
    match status {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => anyhow::bail!("original League process termination query failed: {status:?}"),
    }
}

pub struct ProcessWatcher {
    system: System,
    target_name: String,
    refreshes: u64,
    refreshed_process_records: u64,
    total_refresh_100ns: u64,
    maximum_refresh_100ns: u64,
    maximum_known_processes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessWatcherTelemetry {
    pub refreshes: u64,
    pub refreshed_process_records: u64,
    pub total_refresh_100ns: u64,
    pub maximum_refresh_100ns: u64,
    pub known_processes: usize,
    pub maximum_known_processes: usize,
}

impl ProcessWatcher {
    pub fn new(target_name: impl Into<String>) -> Self {
        Self {
            system: System::new(),
            target_name: target_name.into(),
            refreshes: 0,
            refreshed_process_records: 0,
            total_refresh_100ns: 0,
            maximum_refresh_100ns: 0,
            maximum_known_processes: 0,
        }
    }

    pub fn refresh(&mut self) -> Option<LeagueProcess> {
        let started_at = Instant::now();
        let refreshed = self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            minimal_process_refresh_kind(),
        );
        let elapsed_100ns = duration_100ns(started_at.elapsed());
        self.refreshes = self.refreshes.saturating_add(1);
        self.refreshed_process_records = self
            .refreshed_process_records
            .saturating_add(refreshed as u64);
        self.total_refresh_100ns = self.total_refresh_100ns.saturating_add(elapsed_100ns);
        self.maximum_refresh_100ns = self.maximum_refresh_100ns.max(elapsed_100ns);
        self.maximum_known_processes = self
            .maximum_known_processes
            .max(self.system.processes().len());
        self.system
            .processes()
            .iter()
            .filter(|(_, process)| process_name_matches(process.name(), &self.target_name))
            .min_by_key(|(pid, _)| pid.as_u32())
            .and_then(|(pid, _process)| {
                #[cfg(target_os = "windows")]
                let started_at = process_creation_time(pid.as_u32())?;
                #[cfg(not(target_os = "windows"))]
                let started_at = _process.start_time();
                Some(LeagueProcess {
                    pid: pid.as_u32(),
                    started_at,
                })
            })
    }

    /// Enumeration and watcher selection never authorize publication.
    #[cfg(target_os = "windows")]
    pub(crate) fn confirms_process_ended(&self, original: &OriginalProcess) -> bool {
        original.has_ended().unwrap_or_else(|error| {
            tracing::warn!(pid = original.identity.pid, error = %error, "cannot confirm original League process termination");
            false
        })
    }

    pub fn telemetry(&self) -> ProcessWatcherTelemetry {
        ProcessWatcherTelemetry {
            refreshes: self.refreshes,
            refreshed_process_records: self.refreshed_process_records,
            total_refresh_100ns: self.total_refresh_100ns,
            maximum_refresh_100ns: self.maximum_refresh_100ns,
            known_processes: self.system.processes().len(),
            maximum_known_processes: self.maximum_known_processes,
        }
    }

    #[cfg(test)]
    pub fn target_name(&self) -> &str {
        &self.target_name
    }
}

fn minimal_process_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing().without_tasks()
}

fn duration_100ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos() / 100).unwrap_or(u64::MAX)
}

pub fn process_name_matches(actual: &OsStr, expected: &str) -> bool {
    actual.to_string_lossy().eq_ignore_ascii_case(expected)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessTransition {
    Appeared(LeagueProcess),
    Replaced(LeagueProcess),
    Disappeared,
    Unchanged,
}

pub fn transition(
    previous: Option<LeagueProcess>,
    current: Option<LeagueProcess>,
) -> ProcessTransition {
    match (previous, current) {
        (None, Some(process)) => ProcessTransition::Appeared(process),
        (Some(_), None) => ProcessTransition::Disappeared,
        (Some(previous), Some(current)) if previous != current => {
            ProcessTransition::Replaced(current)
        }
        _ => ProcessTransition::Unchanged,
    }
}

#[allow(dead_code)]
fn _pid_is_send(_: Pid) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn termination_requires_exact_identity_and_an_authoritative_signaled_handle() {
        use windows::Win32::Foundation::{WAIT_EVENT, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};

        let creation = 134_000_000_000_000_000;
        assert!(process_exit_status(creation, Ok(creation), WAIT_OBJECT_0).unwrap());
        for (expected, observed, wait) in [
            (creation, Ok(creation), WAIT_TIMEOUT),
            (creation, Ok(creation), WAIT_FAILED),
            (creation, Ok(creation), WAIT_EVENT(42)),
            (
                creation,
                Err(anyhow::anyhow!("access denied")),
                WAIT_OBJECT_0,
            ),
            (
                creation,
                Err(anyhow::anyhow!("query failed")),
                WAIT_OBJECT_0,
            ),
            // PID reuse within one second must not match the old identity.
            (creation, Ok(creation + 1), WAIT_OBJECT_0),
            (0, Ok(0), WAIT_OBJECT_0),
        ] {
            assert!(!process_exit_status(expected, observed, wait).unwrap_or(false));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn process_exit_child_fixture() {
        if std::env::var_os("QUEUEBACK_PROCESS_EXIT_CHILD").is_some() {
            use std::io::Read;
            // Parent controls the exit boundary without a timer or arbitrary sleep.
            let _ = std::io::stdin().read_exact(&mut [0_u8]);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn original_process_handle_survives_exit_and_ignores_watcher_selection() {
        use std::io::Write;
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "watcher::tests::process_exit_child_fixture",
                "--nocapture",
            ])
            .env("QUEUEBACK_PROCESS_EXIT_CHILD", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        let identity = LeagueProcess {
            pid: child.id(),
            started_at: process_creation_time(child.id()).unwrap(),
        };
        let original = OriginalProcess::open(identity).unwrap();
        let mut watcher = ProcessWatcher::new("queueback-no-such-process-fixture");
        assert_eq!(watcher.refresh(), None);
        assert!(!watcher.confirms_process_ended(&original));

        // A newly selected generation is not evidence about the original owner.
        let replacement = LeagueProcess {
            pid: std::process::id(),
            started_at: process_creation_time(std::process::id()).unwrap(),
        };
        assert_eq!(
            transition(Some(identity), Some(replacement)),
            ProcessTransition::Replaced(replacement)
        );
        assert!(!watcher.confirms_process_ended(&original));
        child.stdin.take().unwrap().write_all(&[1]).unwrap();
        assert!(child.wait().unwrap().success());
        assert!(watcher.confirms_process_ended(&original));
    }

    #[test]
    fn process_names_match_case_insensitively() {
        assert!(process_name_matches(
            OsStr::new("league OF legends.EXE"),
            "League of Legends.exe"
        ));
        assert!(!process_name_matches(
            OsStr::new("LeagueClient.exe"),
            "League of Legends.exe"
        ));
    }

    #[test]
    fn detects_only_edges() {
        let process = LeagueProcess {
            pid: 7,
            started_at: 100,
        };
        assert_eq!(
            transition(None, Some(process)),
            ProcessTransition::Appeared(process)
        );
        assert_eq!(
            transition(Some(process), Some(process)),
            ProcessTransition::Unchanged
        );
        assert_eq!(
            transition(Some(process), None),
            ProcessTransition::Disappeared
        );
        assert_eq!(
            transition(
                Some(process),
                Some(LeagueProcess {
                    pid: 8,
                    started_at: 100
                })
            ),
            ProcessTransition::Replaced(LeagueProcess {
                pid: 8,
                started_at: 100
            })
        );
        assert_eq!(transition(None, None), ProcessTransition::Unchanged);
    }

    #[test]
    fn watcher_keeps_configured_name() {
        let watcher = ProcessWatcher::new("custom.exe");
        assert_eq!(watcher.target_name(), "custom.exe");
        assert_eq!(watcher.telemetry(), ProcessWatcherTelemetry::default());
    }

    #[test]
    fn a_new_process_reusing_a_pid_is_a_replacement() {
        let before = LeagueProcess {
            pid: 42,
            started_at: 100,
        };
        let after = LeagueProcess {
            pid: 42,
            started_at: 101,
        };
        assert_eq!(
            transition(Some(before), Some(after)),
            ProcessTransition::Replaced(after)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn readiness_identity_probe_rejects_a_different_process_creation_time() {
        let pid = std::process::id();
        let process = LeagueProcess {
            pid,
            started_at: process_creation_time(pid).unwrap(),
        };
        assert!(process.is_current());
        assert!(
            !LeagueProcess {
                started_at: process.started_at + 1,
                ..process
            }
            .is_current()
        );
        assert!(
            OriginalProcess::open(LeagueProcess {
                started_at: process.started_at + 1,
                ..process
            })
            .is_err()
        );
        assert!(
            OriginalProcess::open(LeagueProcess {
                pid: 0,
                started_at: 1
            })
            .is_err()
        );
    }

    #[test]
    fn watcher_refresh_requests_only_identity_fields() {
        let refresh = minimal_process_refresh_kind();
        assert!(!refresh.cpu());
        assert!(!refresh.memory());
        assert!(!refresh.disk_usage());
        assert!(!refresh.tasks());
    }

    #[test]
    fn watcher_refresh_is_accounted() {
        let mut watcher = ProcessWatcher::new("process-that-does-not-exist.exe");

        assert_eq!(watcher.refresh(), None);

        let telemetry = watcher.telemetry();
        assert_eq!(telemetry.refreshes, 1);
        assert!(telemetry.refreshed_process_records >= telemetry.known_processes as u64);
        assert!(telemetry.maximum_known_processes >= telemetry.known_processes);
        assert!(telemetry.total_refresh_100ns >= telemetry.maximum_refresh_100ns);
    }
}
