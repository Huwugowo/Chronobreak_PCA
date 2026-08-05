use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::{Config, parse_resolution};
use crate::encoder::{AudioSource, EncoderKind, Ffmpeg, RecordingSession};
use crate::platform::{capture_target_for_process, fallback_capture_target};
use crate::poller::{PollerSession, RecordingMetadata};
use crate::storage::{METADATA_JSON, create_game_directory, unix_timestamp_now, write_json_atomic};
use crate::watcher::{
    DEFAULT_PROCESS_NAME, LeagueProcess, POLL_INTERVAL, ProcessTransition, ProcessWatcher,
    transition,
};

#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Idle,
    Recording { directory: PathBuf },
    Error { message: String },
    ShutdownComplete,
}

#[derive(Debug, Clone, Copy)]
pub enum ServiceCommand {
    Shutdown,
}

pub type EventSink = Arc<dyn Fn(ServiceEvent) + Send + Sync>;

struct ActiveRecording {
    session: RecordingSession,
    poller: PollerSession,
    encoder_used: String,
    recording_resolution: String,
    recording_fps: u32,
}

pub async fn run(
    config: Config,
    mut commands: mpsc::UnboundedReceiver<ServiceCommand>,
    events: EventSink,
) -> Result<()> {
    let output_path = config.output_path()?;
    fs::create_dir_all(output_path.join("games")).with_context(|| {
        format!(
            "failed to create recorder output at {}",
            output_path.display()
        )
    })?;

    let ffmpeg = Ffmpeg::resolve().await?;
    let encoder = ffmpeg.select_hardware_encoder().await?;
    let audio = ffmpeg.detect_audio_source().await;
    info!(
        ffmpeg = %ffmpeg.path().display(),
        encoder = encoder.label(),
        audio = %audio.description(),
        output = %output_path.display(),
        "recorder initialized"
    );

    events(ServiceEvent::Idle);

    let process_name =
        env::var("LEAGUE_REPLAY_PROCESS_NAME").unwrap_or_else(|_| DEFAULT_PROCESS_NAME.to_owned());
    let mut watcher = ProcessWatcher::new(&process_name);
    let mut previous_process: Option<LeagueProcess> = None;
    let mut active: Option<ActiveRecording> = None;
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            command = commands.recv() => {
                if matches!(command, Some(ServiceCommand::Shutdown) | None) {
                    if let Some(recording) = active.take() {
                        stop_recording(recording, &events).await;
                    }
                    events(ServiceEvent::ShutdownComplete);
                    return Ok(());
                }
            }
            _ = interval.tick() => {
                let current_process = watcher.refresh();
                match transition(previous_process, current_process) {
                    ProcessTransition::Appeared(process) => {
                        match start_recording(
                            &ffmpeg,
                            encoder,
                            &audio,
                            &config,
                            &output_path,
                            process,
                        ).await {
                            Ok(recording) => {
                                events(ServiceEvent::Recording {
                                    directory: recording.session.directory().to_path_buf(),
                                });
                                active = Some(recording);
                            }
                            Err(start_error) => {
                                error!(error = %start_error, "recording could not start");
                                events(ServiceEvent::Error {
                                    message: format!("Recording failed to start: {start_error:#}"),
                                });
                            }
                        }
                    }
                    ProcessTransition::Replaced(process) => {
                        warn!(pid = process.pid, "League process was replaced between watcher polls");
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                        }
                        match start_recording(
                            &ffmpeg,
                            encoder,
                            &audio,
                            &config,
                            &output_path,
                            process,
                        ).await {
                            Ok(recording) => {
                                events(ServiceEvent::Recording {
                                    directory: recording.session.directory().to_path_buf(),
                                });
                                active = Some(recording);
                            }
                            Err(start_error) => {
                                error!(error = %start_error, "replacement recording could not start");
                                events(ServiceEvent::Error {
                                    message: format!("Recording failed to start: {start_error:#}"),
                                });
                            }
                        }
                    }
                    ProcessTransition::Disappeared => {
                        if let Some(recording) = active.take() {
                            stop_recording(recording, &events).await;
                            events(ServiceEvent::Idle);
                        }
                    }
                    ProcessTransition::Unchanged => {}
                }

                if let Some(recording) = active.as_mut() {
                    match recording.session.has_exited() {
                        Ok(true) => {
                            let recording = active.take().expect("active recording exists");
                            warn!("ffmpeg exited while League was still running");
                            events(ServiceEvent::Error {
                                message: "ffmpeg exited unexpectedly; preserving the partial recording".to_owned(),
                            });
                            stop_recording(recording, &events).await;
                        }
                        Ok(false) => {}
                        Err(inspect_error) => {
                            error!(error = %inspect_error, "could not inspect ffmpeg");
                        }
                    }
                }

                previous_process = current_process;
            }
        }
    }
}

pub async fn diagnose(config: &Config) -> Result<DiagnosticReport> {
    let ffmpeg = Ffmpeg::resolve().await?;
    let encoder = ffmpeg.select_hardware_encoder().await?;
    let audio = ffmpeg.detect_audio_source().await;
    Ok(DiagnosticReport {
        config_path: crate::config::default_config_path()?,
        output_path: config.output_path()?,
        ffmpeg_path: ffmpeg.path().to_path_buf(),
        encoder,
        audio,
    })
}

pub struct DiagnosticReport {
    pub config_path: PathBuf,
    pub output_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub encoder: EncoderKind,
    pub audio: AudioSource,
}

async fn start_recording(
    ffmpeg: &Ffmpeg,
    encoder: EncoderKind,
    audio: &AudioSource,
    config: &Config,
    output_path: &Path,
    process: LeagueProcess,
) -> Result<ActiveRecording> {
    let directory = create_game_directory(output_path, unix_timestamp_now()?)?;
    let mut target = capture_target_for_process(process.pid)
        .with_context(|| format!("failed to select capture target for PID {}", process.pid))?;
    let first_attempt = ffmpeg
        .start_recording(
            directory.clone(),
            &target,
            &config.recording,
            encoder,
            audio,
        )
        .await;
    let session = match first_attempt {
        Ok(session) => session,
        Err(error) if target.is_window_region() => {
            warn!(
                %error,
                "window-region capture failed; retrying the primary-display fallback"
            );
            target = fallback_capture_target()?;
            ffmpeg
                .start_recording(
                    directory.clone(),
                    &target,
                    &config.recording,
                    encoder,
                    audio,
                )
                .await
                .context("primary-display capture fallback also failed")?
        }
        Err(error) => return Err(error),
    };

    let recording_resolution = parse_resolution(&config.recording.resolution)?
        .or_else(|| target.dimensions())
        .map(|(width, height)| format!("{width}x{height}"))
        .unwrap_or_else(|| config.recording.resolution.clone());
    let poller = match PollerSession::start(&directory, session.video_started_at()).await {
        Ok(poller) => poller,
        Err(error) => {
            let _ = session.stop().await;
            return Err(error).context("failed to start the Live Client poller");
        }
    };

    info!(
        pid = process.pid,
        directory = %directory.display(),
        "recording started"
    );
    Ok(ActiveRecording {
        session,
        poller,
        encoder_used: encoder.label().to_owned(),
        recording_resolution,
        recording_fps: config.recording.fps,
    })
}

async fn stop_recording(recording: ActiveRecording, events: &EventSink) {
    let ActiveRecording {
        session,
        poller,
        encoder_used,
        recording_resolution,
        recording_fps,
    } = recording;
    let directory = session.directory().to_path_buf();
    let recorded_at = session.recorded_at();
    let duration = session.video_started_at().elapsed();
    let (summary, video_result) = tokio::join!(poller.stop(), session.stop());

    let metadata_result = RecordingMetadata::new(
        recorded_at,
        duration,
        summary,
        encoder_used,
        recording_resolution,
        recording_fps,
    );
    match metadata_result {
        Ok(metadata) => {
            if let Err(metadata_error) =
                write_json_atomic(&directory.join(METADATA_JSON), &metadata).await
            {
                error!(error = %metadata_error, "could not write recording metadata");
                events(ServiceEvent::Error {
                    message: format!("Recording metadata could not be saved: {metadata_error:#}"),
                });
            }
        }
        Err(metadata_error) => {
            error!(error = %metadata_error, "could not build recording metadata");
            events(ServiceEvent::Error {
                message: format!("Recording metadata could not be created: {metadata_error:#}"),
            });
        }
    }

    match video_result {
        Ok(directory) => {
            info!(directory = %directory.display(), "video closed");
        }
        Err(stop_error) => {
            error!(error = %stop_error, "recording stopped with an error");
            events(ServiceEvent::Error {
                message: format!(
                    "Recording stopped unexpectedly; the fragmented MP4 was preserved: {stop_error:#}"
                ),
            });
        }
    }
}
