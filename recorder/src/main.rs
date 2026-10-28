#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![deny(clippy::undocumented_unsafe_blocks)]

mod tray;

use std::env;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use league_replay_recorder::config::{Config, default_config_path, log_directory};
use league_replay_recorder::control::{self, StatusSender};
use league_replay_recorder::service::{self, ServiceCommand, ServiceEvent};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const SERVICE_RUNTIME_WORKER_THREADS: usize = 2;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("League Replay Recorder failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn build_service_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(SERVICE_RUNTIME_WORKER_THREADS)
        .thread_name("queueback-control")
        .enable_all()
        .build()
        .context("failed to create recorder runtime")
}

fn build_diagnostic_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create diagnostic runtime")
}

fn run() -> Result<()> {
    let arguments: Vec<String> = env::args().collect();
    let isolated = arguments
        .iter()
        .any(|a| matches!(a.as_str(), "--diagnose" | "--tray-smoke-test"));
    #[cfg(windows)]
    let owner = if isolated {
        None
    } else {
        use chronobreak_recorder_control::windows::{ControlOwner, Namespace, query};
        let namespace = Namespace::production()?;
        let Some(owner) = ControlOwner::start(&namespace)? else {
            let result =
                build_diagnostic_runtime()?.block_on(query(&namespace, &env::current_exe()?));
            match result {
                Ok(status) => {
                    tracing::info!(?status.service_state, session = status.session_id, "existing recorder owns this user profile")
                }
                Err(error) => anyhow::bail!(
                    "another recorder owns this profile but its status is unavailable: {error:#}"
                ),
            }
            return Ok(());
        };
        Some(owner)
    };
    #[cfg(windows)]
    let status = owner.as_ref().map(|owner| owner.status_sender());
    #[cfg(not(windows))]
    let status = {
        let _ = isolated;
        None
    };
    let result = run_owned(arguments, status.clone());
    if result.is_err()
        && let Some(sender) = &status
    {
        use chronobreak_recorder_control::{AvailabilityEvent, ServiceFailure, ServiceState};
        sender.send_modify(|s| {
            if s.service_state != ServiceState::Failed {
                s.apply(AvailabilityEvent::ServiceError(
                    if s.service_state == ServiceState::Starting {
                        ServiceFailure::Initialization
                    } else {
                        ServiceFailure::UnexpectedExit
                    },
                ));
            }
        });
    }
    result
}

fn run_owned(arguments: Vec<String>, status: StatusSender) -> Result<()> {
    let _log_guard = initialize_logging()?;
    let config_path = default_config_path()?;
    let config = Config::load_or_create(&config_path)?;
    tracing::info!(config = %config_path.display(), "starting League Replay Recorder");

    if arguments.iter().any(|argument| argument == "--diagnose") {
        return run_diagnostics(&config);
    }
    if arguments.iter().any(|argument| argument == "--headless") {
        return run_headless(config, status);
    }
    let smoke_test_timeout = arguments
        .iter()
        .any(|argument| argument == "--tray-smoke-test")
        .then_some(Duration::from_secs(3));

    let (command_sender, command_receiver) = mpsc::unbounded_channel();
    tray::run(
        config,
        command_sender,
        command_receiver,
        smoke_test_timeout,
        status,
    )
}

fn run_diagnostics(config: &Config) -> Result<()> {
    let runtime = build_diagnostic_runtime()?;
    let report = runtime.block_on(service::diagnose(config))?;
    println!("League Replay Recorder diagnostics");
    println!("  config:  {}", report.config_path.display());
    println!("  output:  {}", report.output_path.display());
    println!("  ffmpeg:  {}", report.ffmpeg_path.display());
    println!("  runtime: {}", report.media_runtime_id);
    println!(
        "  encoder: {} ({})",
        report.plan.encoder().label(),
        report.plan.encoder().codec_name(report.plan.codec())
    );
    println!("  codec:   {}", report.plan.codec().label());
    println!("  profile: {}", report.plan.profile().label());
    println!("  audio:   {}", report.audio.description());
    Ok(())
}

fn run_headless(config: Config, status: StatusSender) -> Result<()> {
    let runtime = build_service_runtime()?;
    runtime.block_on(async move {
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let sink = std::sync::Arc::new(|event| match event {
            ServiceEvent::Idle => tracing::info!("state: idle"),
            ServiceEvent::Recording { directory } => {
                tracing::info!(directory = %directory.display(), "state: recording")
            }
            ServiceEvent::Error { message } => tracing::error!(%message, "state: error"),
            ServiceEvent::ShutdownComplete => tracing::info!("state: stopped"),
            ServiceEvent::Availability(event) => {
                tracing::debug!(?event, "capture/service lifecycle")
            }
        });
        let mut service = tokio::spawn(control::run(config, command_receiver, sink, status));
        tokio::select! {
            result = &mut service => result.context("recorder supervisor task panicked")?,
            signal = tokio::signal::ctrl_c() => {
                // Even a signal-listener failure must join the accepted shutdown.
                let _ = command_sender.send(ServiceCommand::Shutdown);
                let result = service.await.context("recorder supervisor task panicked")?;
                signal.context("failed to listen for Ctrl+C")?;
                result
            }
        }
    })
}

fn initialize_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let directory = log_directory()?;
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create log directory {}", directory.display()))?;
    let file = tracing_appender::rolling::daily(directory, "recorder.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .try_init()
        .context("failed to initialize logging")?;
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_runtime_has_two_control_workers() {
        let runtime = build_service_runtime().unwrap();
        assert_eq!(
            runtime.metrics().num_workers(),
            SERVICE_RUNTIME_WORKER_THREADS
        );
    }

    #[test]
    fn diagnostic_runtime_is_current_thread() {
        let runtime = build_diagnostic_runtime().unwrap();
        assert_eq!(runtime.metrics().num_workers(), 1);
    }

    #[test]
    fn headless_returns_initialization_failure_without_ctrl_c() {
        let root = tempfile::tempdir().unwrap();
        let blocked = root.path().join("blocked");
        std::fs::write(&blocked, "fixture").unwrap();
        let mut config = Config::default();
        config.storage.output_path = blocked.to_str().unwrap().into();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(run_headless(config, None));
        });
        assert!(
            receiver
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .is_err()
        );
    }
}
