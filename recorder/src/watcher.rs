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
            .map(|(pid, _)| LeagueProcess { pid: pid.as_u32() })
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
        let process = LeagueProcess { pid: 7 };
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
            transition(Some(process), Some(LeagueProcess { pid: 8 })),
            ProcessTransition::Replaced(LeagueProcess { pid: 8 })
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
