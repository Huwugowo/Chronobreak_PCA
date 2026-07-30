#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::env;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use league_replay_recorder::config::{Config, default_config_path, log_directory};
use league_replay_recorder::service::{self, ServiceCommand, ServiceEvent};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("League Replay Recorder failed: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let _log_guard = initialize_logging()?;
    let config_path = default_config_path()?;
    let config = Config::load_or_create(&config_path)?;
    tracing::info!(config = %config_path.display(), "starting League Replay Recorder");

    let arguments: Vec<String> = env::args().collect();
    if arguments.iter().any(|argument| argument == "--diagnose") {
        return run_diagnostics(&config);
    }
    if arguments.iter().any(|argument| argument == "--headless") {
        return run_headless(config);
    }
    let smoke_test_timeout = arguments
        .iter()
        .any(|argument| argument == "--tray-smoke-test")
        .then_some(Duration::from_secs(3));

    let (command_sender, command_receiver) = mpsc::unbounded_channel();
    tray::run(config, command_sender, command_receiver, smoke_test_timeout)
}

fn run_diagnostics(config: &Config) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to create diagnostic runtime")?;
    let report = runtime.block_on(service::diagnose(config))?;
    println!("League Replay Recorder diagnostics");
    println!("  config:  {}", report.config_path.display());
    println!("  output:  {}", report.output_path.display());
    println!("  ffmpeg:  {}", report.ffmpeg_path.display());
    println!(
        "  encoder: {} ({})",
        report.encoder.label(),
        report.encoder.codec_name()
    );
    println!("  audio:   {}", report.audio.description());
    Ok(())
}

fn run_headless(config: Config) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to create recorder runtime")?;
    runtime.block_on(async move {
        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let sink = std::sync::Arc::new(|event| match event {
            ServiceEvent::Idle => tracing::info!("state: idle"),
            ServiceEvent::Recording { directory } => {
                tracing::info!(directory = %directory.display(), "state: recording")
            }
            ServiceEvent::Error { message } => tracing::error!(%message, "state: error"),
            ServiceEvent::ShutdownComplete => tracing::info!("state: stopped"),
        });
        let service = tokio::spawn(service::run(config, command_receiver, sink));
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl+C")?;
        let _ = command_sender.send(ServiceCommand::Shutdown);
        service.await.context("recorder service task panicked")??;
        Ok(())
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
