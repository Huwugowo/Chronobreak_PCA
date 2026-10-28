//! Availability projection shared by tray and headless entry points.
use std::sync::Arc;

use anyhow::{Result, bail};
use chronobreak_recorder_control::{
    AvailabilityEvent, CaptureActivity, ServiceFailure, ServiceState, Status,
};
use tokio::sync::{mpsc, watch};

use crate::config::Config;
use crate::service::{self, EventSink, ServiceCommand, ServiceEvent};

pub type StatusSender = Option<watch::Sender<Status>>;

pub fn publish(status: &StatusSender, event: AvailabilityEvent) {
    if let Some(status) = status {
        status.send_modify(|s| s.apply(event));
    }
}

pub fn event_sink(status: StatusSender, downstream: EventSink) -> EventSink {
    Arc::new(move |event| {
        let update = match &event {
            ServiceEvent::Idle => Some(AvailabilityEvent::Initialized),
            ServiceEvent::Recording { .. } => {
                Some(AvailabilityEvent::Capture(CaptureActivity::Recording))
            }
            ServiceEvent::Availability(event) => Some(*event),
            ServiceEvent::ShutdownComplete => Some(AvailabilityEvent::Stopped),
            // Text errors describe individual capture attempts, never service death.
            ServiceEvent::Error { .. } => None,
        };
        if let Some(event) = update {
            publish(&status, event);
        }
        downstream(event);
    })
}

/// Join the service task so early failure and panic cannot strand a live wrapper.
pub async fn run(
    config: Config,
    commands: mpsc::UnboundedReceiver<ServiceCommand>,
    downstream: EventSink,
    status: StatusSender,
) -> Result<()> {
    let sink = event_sink(status.clone(), downstream.clone());
    let result = tokio::spawn(service::run(config, commands, sink)).await;
    let failure = match result {
        Ok(Ok(())) => {
            if status
                .as_ref()
                .is_some_and(|s| s.borrow().service_state != ServiceState::Stopped)
            {
                Some((
                    ServiceFailure::UnexpectedExit,
                    anyhow::anyhow!("recorder service exited without shutdown"),
                ))
            } else {
                None
            }
        }
        Ok(Err(error)) => {
            let reason = if status
                .as_ref()
                .is_none_or(|s| s.borrow().service_state == ServiceState::Starting)
            {
                ServiceFailure::Initialization
            } else {
                ServiceFailure::UnexpectedExit
            };
            Some((reason, error))
        }
        Err(error) => Some((ServiceFailure::WorkerPanicked, error.into())),
    };
    if let Some((reason, error)) = failure {
        publish(&status, AvailabilityEvent::ServiceError(reason));
        downstream(ServiceEvent::Error {
            message: format!("Recorder service stopped: {error:#}"),
        });
        downstream(ServiceEvent::ShutdownComplete);
        bail!(error.context("recorder service failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_recorder_control::CaptureProblem;

    #[test]
    fn capture_error_text_and_typed_failures_leave_initialized_service_available() {
        let (sender, _) = watch::channel(Status::starting(1, 1, "a".repeat(32)));
        let sink = event_sink(Some(sender.clone()), Arc::new(|_| {}));
        sink(ServiceEvent::Idle);
        for _generation in 0..2 {
            sink(ServiceEvent::Availability(AvailabilityEvent::Capture(
                CaptureActivity::Starting,
            )));
            sink(ServiceEvent::Error {
                message: "retryable or terminal capture attempt".into(),
            });
            sink(ServiceEvent::Availability(AvailabilityEvent::CaptureError(
                CaptureProblem::Startup,
            )));
            assert_eq!(sender.borrow().service_state, ServiceState::Available);
            assert_eq!(sender.borrow().capture_activity, CaptureActivity::Idle);
        }
        sink(ServiceEvent::Recording {
            directory: "fixture".into(),
        });
        assert_eq!(sender.borrow().capture_activity, CaptureActivity::Recording);
        assert_eq!(sender.borrow().capture_problem, None);
    }

    #[tokio::test]
    async fn actual_initialization_failure_returns_without_shutdown_signal() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("file-not-directory");
        std::fs::write(&blocked, "fixture").unwrap();
        let mut config = Config::default();
        config.storage.output_path = blocked.to_str().unwrap().into();
        let (_commands, receiver) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(Status::starting(1, 1, "a".repeat(32)));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            run(config, receiver, Arc::new(|_| {}), Some(status.clone())),
        )
        .await;
        assert!(result.unwrap().is_err());
        assert_eq!(status.borrow().service_state, ServiceState::Failed);
        assert_eq!(
            status.borrow().service_failure,
            Some(ServiceFailure::Initialization)
        );
    }
}
