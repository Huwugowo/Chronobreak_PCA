//! One serialized observer; closing the library never terminates the recorder.
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chronobreak_recorder_control::{CaptureActivity, ServiceState, SessionAvailability, Status};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecorderState {
    DisabledForTest,
    Starting,
    Available,
    Recording,
    CaptureProblem,
    Stopping,
    Unavailable,
    ServiceFailed,
    OwnerInOtherSession,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecorderStatus {
    pub state: RecorderState,
    pub recording_capable_here: bool,
    pub can_retry: bool,
    pub message: String,
    pub owner: Option<Status>,
}

impl RecorderStatus {
    fn simple(state: RecorderState, message: &str, can_retry: bool) -> Self {
        Self {
            state,
            recording_capable_here: false,
            can_retry,
            message: message.into(),
            owner: None,
        }
    }

    #[cfg(windows)]
    fn from_launch(result: anyhow::Result<bool>) -> Self {
        match result {
            Ok(true) => Self::simple(RecorderState::Starting, "Recorder is starting…", false),
            Ok(false) => Self::simple(
                RecorderState::Unavailable,
                "A recorder owns this profile, but its status is unavailable. Retry rechecks ownership; it cannot replace a running recorder.",
                true,
            ),
            Err(_) => Self::simple(
                RecorderState::Unavailable,
                "Recorder could not start. Check that recorder.exe is installed beside Chronobreak and can be run, then retry.",
                true,
            ),
        }
    }

    fn from_owner(status: Status, session: u32) -> Self {
        let (state, message, can_retry) = if status.projection(session)
            == SessionAvailability::OwnerInOtherSession
        {
            (
                RecorderState::OwnerInOtherSession,
                "The recorder is open in another Windows session. Quit it from that session's tray, then reopen Chronobreak here.",
                false,
            )
        } else {
            match status.service_state {
                ServiceState::Starting => (RecorderState::Starting, "Recorder is starting…", false),
                ServiceState::Stopping => (
                    RecorderState::Stopping,
                    "Recorder is finishing and stopping…",
                    false,
                ),
                ServiceState::Failed => (
                    RecorderState::ServiceFailed,
                    "Recorder service failed. Quit its tray and wait for it to exit before retrying.",
                    false,
                ),
                ServiceState::Stopped => (RecorderState::Unavailable, "Recorder stopped.", true),
                ServiceState::Available if status.capture_problem.is_some() => (
                    RecorderState::CaptureProblem,
                    "Recorder is available, but the last capture attempt failed. Capture retries follow the recorder's policy.",
                    false,
                ),
                ServiceState::Available => match status.capture_activity {
                    CaptureActivity::Recording => (RecorderState::Recording, "Recording", false),
                    CaptureActivity::Starting => (
                        RecorderState::Available,
                        "Recorder available — starting capture…",
                        false,
                    ),
                    CaptureActivity::Finalizing => (
                        RecorderState::Available,
                        "Recorder available — finishing capture…",
                        false,
                    ),
                    CaptureActivity::Idle => {
                        (RecorderState::Available, "Recorder available", false)
                    }
                },
            }
        };
        Self {
            state,
            message: message.into(),
            can_retry,
            recording_capable_here: status.projection(session)
                == SessionAvailability::AvailableHere,
            owner: Some(status),
        }
    }
}

pub struct RecorderSupervisor {
    cache: Arc<RwLock<RecorderStatus>>,
    retry: mpsc::Sender<()>,
    stop: Option<oneshot::Sender<()>>,
    pub disabled: bool,
}

pub fn supervision_disabled() -> bool {
    cfg!(feature = "replay-benchmark")
        || std::env::var_os("CHRONOBREAK_DISABLE_RECORDER_SUPERVISION").is_some_and(|v| v == "1")
        || std::env::args().any(|v| {
            v == "--replay-benchmark-manifest" || v.starts_with("--replay-benchmark-manifest=")
        })
}

impl RecorderSupervisor {
    pub fn start(disabled: bool) -> Self {
        let initial = if disabled {
            RecorderStatus::simple(
                RecorderState::DisabledForTest,
                "Recorder startup is disabled for this test session.",
                false,
            )
        } else {
            RecorderStatus::simple(RecorderState::Starting, "Recorder is starting…", false)
        };
        let cache = Arc::new(RwLock::new(initial));
        let (retry, requests) = mpsc::channel(1);
        let (stop, stopped) = oneshot::channel();
        // This gate precedes path resolution, profile leases, pipe and registry access.
        if !disabled {
            tauri::async_runtime::spawn(observe(Arc::clone(&cache), requests, stopped));
        }
        Self {
            cache,
            retry,
            stop: Some(stop),
            disabled,
        }
    }

    pub fn status(&self) -> RecorderStatus {
        self.cache.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn retry(&self) -> Result<(), String> {
        if !self.status().can_retry {
            return Err("Recorder cannot be restarted in its current state.".into());
        }
        self.retry
            .try_send(())
            .map_err(|_| "Recorder retry is already pending.".into())
    }
}

impl Drop for RecorderSupervisor {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

fn store(cache: &RwLock<RecorderStatus>, status: RecorderStatus) {
    *cache.write().unwrap_or_else(|p| p.into_inner()) = status;
}

#[cfg(windows)]
pub fn sibling_recorder() -> anyhow::Result<std::path::PathBuf> {
    let executable = std::env::current_exe()?;
    let path = executable
        .parent()
        .ok_or_else(|| anyhow::anyhow!("application has no install directory"))?
        .join("recorder.exe");
    chronobreak_recorder_control::windows::regular_executable(&path)
}

#[cfg(windows)]
async fn observe(
    cache: Arc<RwLock<RecorderStatus>>,
    mut requests: mpsc::Receiver<()>,
    mut stopped: oneshot::Receiver<()>,
) {
    use chronobreak_recorder_control::windows::{Namespace, current_session, query};
    let (namespace, executable, session) = loop {
        let setup = tokio::task::spawn_blocking(|| {
            Ok::<_, anyhow::Error>((
                Namespace::production()?,
                sibling_recorder()?,
                current_session()?,
            ))
        })
        .await;
        match setup {
            Ok(Ok(value)) => break value,
            _ => {
                store(
                    &cache,
                    RecorderStatus::simple(
                        RecorderState::Unavailable,
                        "Recorder is missing or its installed path is invalid. Install the matching recorder beside Chronobreak.",
                        true,
                    ),
                );
                // Explicit retry can re-resolve a subsequently repaired installation.
                tokio::select! { _ = &mut stopped => return, request = requests.recv() => {
                    if request.is_none() { return; }
                }};
            }
        }
    };
    let mut ensure = true;
    let mut starting_since = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = &mut stopped => return,
            result = query(&namespace, &executable) => {
                match result {
                    Ok(status) => {
                        let mut projected = RecorderStatus::from_owner(status, session);
                        if projected.state == RecorderState::Starting && starting_since.elapsed() >= Duration::from_secs(30) {
                            projected.message = "Recorder startup has not completed. Chronobreak will keep observing it; no replacement will be started.".into();
                        }
                        store(&cache, projected);
                    }
                    Err(_) => {
                        if ensure {
                            let ns = namespace.clone(); let exe = executable.clone();
                            let launched = tokio::task::spawn_blocking(move || launch_if_unowned(&ns, &exe))
                                .await.unwrap_or_else(|error| Err(error.into()));
                            store(&cache, RecorderStatus::from_launch(launched));
                        } else if starting_since.elapsed() >= Duration::from_secs(30) || cache.read().unwrap_or_else(|p| p.into_inner()).owner.is_some() {
                            store(&cache, RecorderStatus::simple(RecorderState::Unavailable, "Recorder status is unavailable. Retry checks ownership before starting anything.", true));
                        }
                    }
                }
                ensure = false;
            }
        }
        tokio::select! {
            _ = &mut stopped => return,
            request = requests.recv() => {
                if request.is_none() { return; }
                ensure = true;
                starting_since = tokio::time::Instant::now();
            }
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
}

#[cfg(windows)]
fn launch_if_unowned(
    namespace: &chronobreak_recorder_control::windows::Namespace,
    executable: &std::path::Path,
) -> anyhow::Result<bool> {
    use chronobreak_recorder_control::windows::{FileLease, regular_executable};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let Some(lease) = FileLease::try_acquire(&namespace.lease)? else {
        return Ok(false);
    };
    let executable = regular_executable(executable)?;
    // The recorder arbitrates racing launches itself. Never hold its lease while spawning.
    drop(lease);
    Command::new(executable)
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("CHRONOBREAK_DISABLE_RECORDER_SUPERVISION")
        .spawn()?;
    Ok(true)
}

#[cfg(not(windows))]
async fn observe(
    cache: Arc<RwLock<RecorderStatus>>,
    _requests: mpsc::Receiver<()>,
    _stopped: oneshot::Receiver<()>,
) {
    store(
        &cache,
        RecorderStatus::simple(
            RecorderState::Unavailable,
            "Automatic recording requires Windows.",
            false,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_recorder_control::{AvailabilityEvent, CaptureProblem, ServiceFailure};

    #[cfg(windows)]
    #[test]
    fn successful_launch_request_waits_for_service_readiness() {
        let projected = RecorderStatus::from_launch(Ok(true));
        assert_eq!(projected.state, RecorderState::Starting);
        assert!(!projected.recording_capable_here && !projected.can_retry);
        assert!(projected.owner.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn unavailable_owner_blocks_launch_even_when_executable_is_missing() {
        use chronobreak_recorder_control::windows::{FileLease, Namespace};
        let root = tempfile::tempdir().unwrap();
        let namespace = Namespace {
            lease: root.path().join("owner.lock"),
            pipe: String::new(), // No status server; the held lease alone must exclude launch.
        };
        let owner = FileLease::try_acquire(&namespace.lease).unwrap().unwrap();
        let projected = RecorderStatus::from_launch(launch_if_unowned(
            &namespace,
            &root.path().join("missing-recorder.exe"),
        ));
        assert_eq!(projected.state, RecorderState::Unavailable);
        assert!(projected.can_retry && !projected.recording_capable_here);
        assert!(projected.message.contains("Retry rechecks ownership"));
        assert!(projected.owner.is_none());
        assert!(FileLease::try_acquire(&namespace.lease).unwrap().is_none());
        drop(owner);
        assert!(FileLease::try_acquire(&namespace.lease).unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    fn explicit_spawn_failure_is_unavailable_and_retryable() {
        use chronobreak_recorder_control::windows::{FileLease, Namespace, regular_executable};
        let root = tempfile::tempdir().unwrap();
        let namespace = Namespace {
            lease: root.path().join("owner.lock"),
            pipe: String::new(),
        };
        let executable = root.path().join("recorder.exe");
        std::fs::write(&executable, b"not a Windows executable").unwrap();
        regular_executable(&executable).unwrap(); // Pass path validation; fail CreateProcess.
        let error = launch_if_unowned(&namespace, &executable).unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        let projected = RecorderStatus::from_launch(Err(error));
        assert_eq!(projected.state, RecorderState::Unavailable);
        assert!(projected.can_retry && !projected.recording_capable_here);
        assert!(
            projected
                .message
                .contains("recorder.exe is installed beside Chronobreak")
        );
        assert!(projected.owner.is_none());
        assert!(FileLease::try_acquire(&namespace.lease).unwrap().is_some());
    }

    #[test]
    fn failed_owner_must_exit_before_retry_is_offered() {
        let mut owner = Status::starting(1, 5, "a".repeat(32));
        owner.apply(AvailabilityEvent::ServiceError(
            ServiceFailure::Initialization,
        ));
        let projected = RecorderStatus::from_owner(owner, 5);
        assert_eq!(projected.state, RecorderState::ServiceFailed);
        assert!(!projected.recording_capable_here && !projected.can_retry);
        assert!(projected.message.contains("wait for it to exit"));
        let (retry, mut requests) = mpsc::channel(1);
        let supervisor = RecorderSupervisor {
            cache: Arc::new(RwLock::new(projected)),
            retry,
            stop: None,
            disabled: false,
        };
        assert!(supervisor.retry().is_err());
        assert_eq!(requests.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    }

    #[test]
    fn other_session_and_capture_failure_never_offer_service_restart() {
        let mut status = Status::starting(1, 5, "a".repeat(32));
        status.apply(AvailabilityEvent::Initialized);
        for activity in [
            CaptureActivity::Idle,
            CaptureActivity::Starting,
            CaptureActivity::Recording,
        ] {
            status.apply(AvailabilityEvent::Capture(activity));
            let projected = RecorderStatus::from_owner(status.clone(), 6);
            assert_eq!(projected.state, RecorderState::OwnerInOtherSession);
            assert!(!projected.recording_capable_here && !projected.can_retry);
        }
        status.apply(AvailabilityEvent::CaptureError(CaptureProblem::Startup));
        let projected = RecorderStatus::from_owner(status, 5);
        assert_eq!(projected.state, RecorderState::CaptureProblem);
        assert!(projected.recording_capable_here && !projected.can_retry);
    }

    #[test]
    fn disabled_constructs_no_task_or_runtime() {
        let supervisor = RecorderSupervisor::start(true);
        assert_eq!(supervisor.status().state, RecorderState::DisabledForTest);
        assert!(supervisor.retry().is_err());
    }
}
