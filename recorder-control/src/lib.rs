//! Local recorder ownership and availability. Capture errors never imply service death.
#![deny(clippy::undocumented_unsafe_blocks)]

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
pub mod config_lock;
pub mod login;

#[cfg(windows)]
pub mod windows;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 4096;
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    Starting,
    Available,
    Stopping,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureActivity {
    Idle,
    Starting,
    Recording,
    Finalizing,
}

/// Safe codes only: neither paths nor arbitrary backend exception text cross IPC.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureProblem {
    Startup,
    Interrupted,
    Finalization,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceFailure {
    Initialization,
    UnexpectedExit,
    WorkerPanicked,
    ControlUnavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub protocol_version: u32,
    pub pid: u32,
    pub session_id: u32,
    pub instance: String,
    pub service_state: ServiceState,
    pub capture_activity: CaptureActivity,
    pub capture_problem: Option<CaptureProblem>,
    pub service_failure: Option<ServiceFailure>,
}

impl Status {
    pub fn starting(pid: u32, session_id: u32, instance: String) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            pid,
            session_id,
            instance,
            service_state: ServiceState::Starting,
            capture_activity: CaptureActivity::Idle,
            capture_problem: None,
            service_failure: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.protocol_version != PROTOCOL_VERSION || self.pid == 0 {
            bail!("incompatible recorder protocol");
        }
        if self.instance.len() != 32 || !self.instance.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("invalid recorder instance");
        }
        if (self.service_state == ServiceState::Failed) != self.service_failure.is_some() {
            bail!("incoherent service failure");
        }
        if self.service_state != ServiceState::Available
            && !(self.service_state == ServiceState::Stopping
                && self.capture_activity == CaptureActivity::Finalizing)
            && self.capture_activity != CaptureActivity::Idle
        {
            bail!("capture activity without available service");
        }
        Ok(())
    }

    pub fn apply(&mut self, event: AvailabilityEvent) {
        match event {
            AvailabilityEvent::Initialized => {
                if self.service_state == ServiceState::Starting {
                    self.service_state = ServiceState::Available;
                    self.capture_activity = CaptureActivity::Idle;
                }
            }
            AvailabilityEvent::Capture(activity) => {
                if self.service_state == ServiceState::Available
                    || (self.service_state == ServiceState::Stopping
                        && matches!(
                            activity,
                            CaptureActivity::Finalizing | CaptureActivity::Idle
                        ))
                {
                    self.capture_activity = activity;
                    if matches!(
                        activity,
                        CaptureActivity::Starting | CaptureActivity::Recording
                    ) {
                        self.capture_problem = None;
                    }
                }
            }
            AvailabilityEvent::CaptureError(problem) => {
                // Capture retry/suppression remains owned by the recording service.
                self.capture_problem = Some(problem);
                self.capture_activity = CaptureActivity::Idle;
            }
            AvailabilityEvent::Stopping => {
                if self.service_state != ServiceState::Failed {
                    self.service_state = ServiceState::Stopping;
                    self.capture_activity = CaptureActivity::Idle;
                }
            }
            AvailabilityEvent::Stopped => {
                if self.service_state != ServiceState::Failed {
                    self.service_state = ServiceState::Stopped;
                    self.capture_activity = CaptureActivity::Idle;
                    self.service_failure = None;
                }
            }
            AvailabilityEvent::ServiceError(reason) => {
                self.service_state = ServiceState::Failed;
                self.capture_activity = CaptureActivity::Idle;
                self.service_failure = Some(reason);
            }
        }
    }

    /// Per-user exclusion does not make another session's HWNDs recordable here.
    pub fn projection(&self, current_session: u32) -> SessionAvailability {
        if self.session_id != current_session {
            SessionAvailability::OwnerInOtherSession
        } else if self.service_state == ServiceState::Available {
            SessionAvailability::AvailableHere
        } else {
            SessionAvailability::NotAvailableHere
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum AvailabilityEvent {
    Initialized,
    Capture(CaptureActivity),
    CaptureError(CaptureProblem),
    Stopping,
    Stopped,
    ServiceError(ServiceFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionAvailability {
    AvailableHere,
    NotAvailableHere,
    OwnerInOtherSession,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub protocol_version: u32,
}

pub async fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut (impl tokio::io::AsyncRead + Unpin),
) -> Result<T> {
    use tokio::io::AsyncReadExt;
    let length = stream.read_u32_le().await? as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        bail!("recorder frame size exceeds bound");
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn write_frame(
    stream: &mut (impl tokio::io::AsyncWrite + Unpin),
    value: &impl Serialize,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        bail!("recorder frame size exceeds bound");
    }
    stream.write_u32_le(u32::try_from(bytes.len())?).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_errors_never_fail_the_service_and_later_attempt_recovers() {
        let mut s = Status::starting(1, 2, "a".repeat(32));
        s.apply(AvailabilityEvent::Initialized);
        for problem in [
            CaptureProblem::Startup,
            CaptureProblem::Interrupted,
            CaptureProblem::Finalization,
        ] {
            s.apply(AvailabilityEvent::CaptureError(problem));
            assert_eq!(s.service_state, ServiceState::Available);
            assert_eq!(s.capture_activity, CaptureActivity::Idle);
            s.validate().unwrap();
            s.apply(AvailabilityEvent::Capture(CaptureActivity::Starting));
            s.apply(AvailabilityEvent::Capture(CaptureActivity::Recording));
            assert_eq!(s.capture_problem, None);
        }
        s.apply(AvailabilityEvent::ServiceError(
            ServiceFailure::UnexpectedExit,
        ));
        assert_eq!(s.projection(2), SessionAvailability::NotAvailableHere);
        s.validate().unwrap();
    }

    #[test]
    fn other_session_never_claims_current_session_capability() {
        let mut s = Status::starting(1, 2, "a".repeat(32));
        s.apply(AvailabilityEvent::Initialized);
        for activity in [
            CaptureActivity::Idle,
            CaptureActivity::Starting,
            CaptureActivity::Recording,
        ] {
            s.apply(AvailabilityEvent::Capture(activity));
            assert_eq!(s.projection(3), SessionAvailability::OwnerInOtherSession);
            assert_eq!(s.projection(2), SessionAvailability::AvailableHere);
        }
    }

    #[tokio::test]
    async fn oversized_frame_rejected_before_body_and_missing_body_times_out() {
        use tokio::io::AsyncWriteExt;
        let (mut tx, mut rx) = tokio::io::duplex(16);
        tx.write_u32_le(4097).await.unwrap();
        assert!(read_frame::<StatusRequest>(&mut rx).await.is_err());
        tx.write_u32_le(12).await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(10),
                read_frame::<StatusRequest>(&mut rx)
            )
            .await
            .is_err()
        );
    }
}
