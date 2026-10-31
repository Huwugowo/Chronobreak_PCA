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
    /// OS process creation time, in Unix seconds; distinguishes observed PID reuse.
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
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct ProcessHandle(HANDLE);
    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            // SAFETY: this guard uniquely owns the successful OpenProcess handle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    // SAFETY: query-only access to an OS process identifier; no process mutation.
    let process =
        ProcessHandle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: the owned process handle and all four distinct output pointers are live.
    unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .ok()?;
    let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    // FILETIME uses 100 ns since 1601; sysinfo start_time uses Unix seconds.
    (ticks / 10_000_000).checked_sub(11_644_473_600)
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
            .map(|(pid, process)| LeagueProcess {
                pid: pid.as_u32(),
                started_at: process.start_time(),
            })
    }

    /// Confirm the original generation against the full refreshed snapshot, not
    /// just `refresh`'s lowest-PID selected League process.
    pub(crate) fn confirms_process_ended(&mut self, original: LeagueProcess) -> bool {
        let _ = self.refresh();
        original.started_at != 0
            && self
                .system
                .process(Pid::from_u32(original.pid))
                .is_none_or(|process| process.start_time() != original.started_at)
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

    #[test]
    fn target_loss_confirmation_checks_original_identity_not_selected_process() {
        let mut watcher = ProcessWatcher::new("queueback-no-such-process-fixture");
        assert_eq!(watcher.refresh(), None);
        let pid = std::process::id();
        let started_at = watcher
            .system
            .process(Pid::from_u32(pid))
            .unwrap()
            .start_time();
        let original = LeagueProcess { pid, started_at };

        // No selected League process does not imply this original owner ended.
        assert!(!watcher.confirms_process_ended(original));
        assert!(watcher.confirms_process_ended(LeagueProcess {
            started_at: started_at + 1,
            ..original
        }));
        assert!(!watcher.confirms_process_ended(LeagueProcess {
            started_at: 0,
            ..original
        }));
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
        let pid = Pid::from_u32(std::process::id());
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            minimal_process_refresh_kind(),
        );
        let process = LeagueProcess {
            pid: pid.as_u32(),
            started_at: system.process(pid).unwrap().start_time(),
        };
        assert!(process.is_current());
        assert!(
            !LeagueProcess {
                started_at: process.started_at + 1,
                ..process
            }
            .is_current()
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
