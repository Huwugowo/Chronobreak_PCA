use std::ffi::OsStr;
use std::time::Duration;

use sysinfo::{Pid, ProcessesToUpdate, System};

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
}

impl ProcessWatcher {
    pub fn new(target_name: impl Into<String>) -> Self {
        Self {
            system: System::new(),
            target_name: target_name.into(),
        }
    }

    pub fn refresh(&mut self) -> Option<LeagueProcess> {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.system
            .processes()
            .iter()
            .filter(|(_, process)| process_name_matches(process.name(), &self.target_name))
            .min_by_key(|(pid, _)| pid.as_u32())
            .map(|(pid, _)| LeagueProcess { pid: pid.as_u32() })
    }

    #[cfg(test)]
    pub fn target_name(&self) -> &str {
        &self.target_name
    }
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
    }
}
