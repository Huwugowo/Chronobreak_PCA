use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use tokio::sync::watch;

use crate::encoder::{AudioSource, RecordingEvidence};
use crate::platform::{CaptureTarget, instant_from_qpc_100ns, validate_capture_target};
use crate::storage::{VIDEO_MP4, VIDEO_PARTIAL_MP4};

use super::{NativeRecorderSession, NativeSessionTelemetrySnapshot};

const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(50);

struct NativeWorker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<NativeSessionTelemetrySnapshot>>>,
}

impl NativeWorker {
    fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    async fn stop_and_join(mut self) -> Result<NativeSessionTelemetrySnapshot> {
        self.request_stop();
        let thread = self
            .thread
            .take()
            .context("native recorder worker was already joined")?;
        tokio::task::spawn_blocking(move || thread.join())
            .await
            .context("native recorder join task panicked")?
            .map_err(|_| anyhow!("native recorder worker panicked"))?
    }
}

impl Drop for NativeWorker {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Automatic-recorder owner for the thread-affine native WGC/D3D11/NVENC
/// graph. The service sees the same latest-value evidence and publication
/// contract as the existing FFmpeg/WGC session.
pub(crate) struct NativeRecordingSession {
    directory: PathBuf,
    output: PathBuf,
    worker: NativeWorker,
    evidence: watch::Receiver<RecordingEvidence>,
    video_started_at: Instant,
    recorded_at: SystemTime,
}

impl NativeRecordingSession {
    pub(crate) async fn start(
        directory: PathBuf,
        target: CaptureTarget,
        ffmpeg: PathBuf,
        audio: AudioSource,
        mut cancellation: watch::Receiver<bool>,
        startup_timeout: Duration,
    ) -> Result<Self> {
        validate_capture_target(&target)?;
        let output = directory.join(VIDEO_PARTIAL_MP4);
        let worker_output = output.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (evidence_sender, mut evidence) = watch::channel(RecordingEvidence::default());
        let thread = thread::Builder::new()
            .name("queueback-native-gpu".to_owned())
            .spawn(move || {
                run_native_worker(
                    &target,
                    &ffmpeg,
                    &audio,
                    &worker_output,
                    &worker_stop,
                    &evidence_sender,
                )
            })
            .context("failed to start native recorder GPU worker")?;
        let worker = NativeWorker {
            stop,
            thread: Some(thread),
        };
        let deadline = Instant::now() + startup_timeout;

        loop {
            if *cancellation.borrow() {
                let cleanup = worker.stop_and_join().await.err();
                return Err(cancellation_error(cleanup));
            }
            let snapshot = evidence.borrow().clone();
            if let Some(error) = snapshot.protocol_error.as_deref() {
                let worker_error = worker.stop_and_join().await.err();
                return Err(anyhow!(
                    "native recorder startup failed: {error}{}",
                    format_cleanup_error(worker_error)
                ));
            }
            if snapshot.startup_ready() {
                let first_qpc = snapshot
                    .first_qpc
                    .context("native recorder became ready without a first-frame QPC timestamp")?;
                let video_started_at = instant_from_qpc_100ns(first_qpc)?;
                let now = Instant::now();
                let recorded_at = SystemTime::now()
                    .checked_sub(now.saturating_duration_since(video_started_at))
                    .context("native first-frame wall-clock anchor underflowed")?;
                return Ok(Self {
                    directory,
                    output,
                    worker,
                    evidence,
                    video_started_at,
                    recorded_at,
                });
            }
            if worker.is_finished() {
                let error = worker.stop_and_join().await.err().map_or_else(
                    || "native recorder stopped before startup became ready".to_owned(),
                    |error| format!("native recorder stopped during startup: {error:#}"),
                );
                bail!(error);
            }
            if Instant::now() >= deadline {
                let snapshot = evidence.borrow().clone();
                let worker_error = worker.stop_and_join().await.err();
                bail!(
                    "native recorder did not produce source, encode and mux progress within {} seconds (source={}, encoded={}, muxed_bytes={}, output_time_us={:?}){}",
                    startup_timeout.as_secs_f64(),
                    snapshot.source_frames_received(),
                    snapshot.encoded_frames,
                    snapshot.muxed_bytes,
                    snapshot.output_time_us,
                    format_cleanup_error(worker_error)
                );
            }

            tokio::select! {
                changed = evidence.changed() => {
                    if changed.is_err() && !worker.is_finished() {
                        bail!("native recorder evidence channel closed during startup");
                    }
                }
                changed = cancellation.changed() => {
                    if changed.is_err() || *cancellation.borrow() {
                        let cleanup = worker.stop_and_join().await.err();
                        return Err(cancellation_error(cleanup));
                    }
                }
                _ = tokio::time::sleep(STARTUP_POLL_INTERVAL) => {}
            }
        }
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn video_started_at(&self) -> Instant {
        self.video_started_at
    }

    pub(crate) fn recorded_at(&self) -> SystemTime {
        self.recorded_at
    }

    pub(crate) fn evidence_receiver(&self) -> watch::Receiver<RecordingEvidence> {
        self.evidence.clone()
    }

    pub(crate) fn has_exited(&self) -> bool {
        self.worker.is_finished()
    }

    pub(crate) async fn stop(self) -> Result<PathBuf> {
        self.stop_inner(None).await
    }

    pub(crate) async fn stop_with_failure(self, reason: String) -> Result<PathBuf> {
        self.stop_inner(Some(reason)).await
    }

    async fn stop_inner(self, failure_reason: Option<String>) -> Result<PathBuf> {
        let Self {
            directory,
            output,
            worker,
            evidence,
            video_started_at: _,
            recorded_at: _,
        } = self;
        let worker_result = worker.stop_and_join().await;
        if let Some(reason) = failure_reason {
            let worker_error = worker_result.err();
            bail!(
                "{reason}{}; partial fragmented MP4 preserved at {}",
                format_cleanup_error(worker_error),
                output.display()
            );
        }
        worker_result?;

        let final_evidence = evidence.borrow().clone();
        if let Some(error) = final_evidence.protocol_error {
            bail!(
                "native recorder observability failed: {error}; partial fragmented MP4 preserved at {}",
                output.display()
            );
        }
        if !final_evidence.capture_terminal || !final_evidence.progress_end {
            bail!(
                "native recorder did not report terminal capture and mux progress; partial fragmented MP4 preserved at {}",
                output.display()
            );
        }
        let output_metadata = tokio::fs::metadata(&output)
            .await
            .with_context(|| format!("native recording is missing {}", output.display()))?;
        if output_metadata.len() == 0 {
            bail!(
                "native recording {} is empty; partial output was not published",
                output.display()
            );
        }

        let canonical_output = directory.join(VIDEO_MP4);
        if tokio::fs::try_exists(&canonical_output).await? {
            bail!(
                "refusing to overwrite an existing recording at {}; validated partial remains at {}",
                canonical_output.display(),
                output.display()
            );
        }
        tokio::fs::rename(&output, &canonical_output)
            .await
            .with_context(|| {
                format!(
                    "failed to publish validated native output {} as {}",
                    output.display(),
                    canonical_output.display()
                )
            })?;
        Ok(directory)
    }
}

fn run_native_worker(
    target: &CaptureTarget,
    ffmpeg: &Path,
    audio: &AudioSource,
    output: &Path,
    stop: &AtomicBool,
    evidence: &watch::Sender<RecordingEvidence>,
) -> Result<NativeSessionTelemetrySnapshot> {
    let mut session = NativeRecorderSession::start(target, ffmpeg, audio, output)?;
    let run_result = session.run_until_stopped(stop, evidence);
    let finish_result = session.finish();
    let result = match (run_result, finish_result) {
        (Ok(()), Ok(snapshot)) => Ok(snapshot),
        (Err(run_error), Ok(snapshot)) => {
            evidence.send_replace(snapshot.recording_evidence());
            Err(run_error)
        }
        (Ok(()), Err(finish_error)) => Err(finish_error),
        (Err(run_error), Err(finish_error)) => Err(anyhow!(
            "{run_error:#}; native shutdown also failed: {finish_error:#}"
        )),
    };

    match &result {
        Ok(snapshot) => {
            evidence.send_replace(snapshot.recording_evidence());
        }
        Err(error) => {
            evidence.send_modify(|state| {
                if state.protocol_error.is_none() {
                    state.protocol_error = Some(format!("{error:#}"));
                }
            });
        }
    }
    result
}

fn cancellation_error(cleanup: Option<anyhow::Error>) -> anyhow::Error {
    anyhow!(
        "native recording startup was cancelled{}",
        format_cleanup_error(cleanup)
    )
}

fn format_cleanup_error(error: Option<anyhow::Error>) -> String {
    error.map_or_else(String::new, |error| {
        format!("; native cleanup also failed: {error:#}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{
        NativeCfrTelemetrySnapshot, NativeMuxTelemetrySnapshot, NativeNv12TelemetrySnapshot,
        NativeNvencTelemetrySnapshot, NativeWgcTelemetrySnapshot,
    };

    fn telemetry() -> NativeSessionTelemetrySnapshot {
        NativeSessionTelemetrySnapshot {
            capture: NativeWgcTelemetrySnapshot {
                arrivals: 120,
                admitted: 120,
                handoff_drops: 0,
                callback_errors: 0,
                first_arrival_qpc_100ns: Some(100),
                latest_arrival_qpc_100ns: Some(200),
                first_accepted_qpc_100ns: Some(100),
                latest_accepted_qpc_100ns: Some(200),
                first_callback_error: None,
                recreations: 0,
                closed: true,
            },
            cfr: NativeCfrTelemetrySnapshot {
                frames_per_second: 60,
                media_time_base_numerator: 1,
                media_time_base_denominator: 60,
                first_source_qpc_100ns: 100,
                latest_source_qpc_100ns: 200,
                scheduled_ticks: 120,
                source_discards: 0,
                duplicate_ticks: 0,
                late_ticks: 0,
                catch_up_ticks: 0,
                maximum_lateness_100ns: 0,
                latest_source_age_100ns: 0,
                maximum_source_age_100ns: 0,
            },
            conversion: NativeNv12TelemetrySnapshot {
                converted_frames: 120,
                no_free_slot_drops: 0,
                processor_state_configurations: 120,
                slot_texture_allocations: 4,
                input_view_creations: 2,
                input_view_replacements: 0,
                input_view_cache_resets: 0,
                processor_recreations: 0,
                output_view_recreations: 0,
                source_snapshot_allocations: 1,
                source_snapshot_copies: 120,
            },
            encode: NativeNvencTelemetrySnapshot {
                submitted_frames: 120,
                completed_frames: 120,
                output_bytes: 4_096,
                max_in_flight: 3,
                submission_queue_failures: 0,
                completion_errors: 0,
            },
            mux: NativeMuxTelemetrySnapshot {
                encoded_frames: 120,
                muxed_bytes: 8_192,
                output_time_us: Some(2_000_000),
                output_file_bytes: 8_192,
                progress_end: true,
                reader_errors: 0,
            },
            slot_tick_drops: 0,
            unstaged_tick_drops: 0,
            maximum_catch_up_batch: 0,
            injected_worker_stalls: 0,
            injected_worker_stall_100ns: 0,
            target_closed: false,
        }
    }

    fn fake_session(
        directory: &Path,
        snapshot: NativeSessionTelemetrySnapshot,
    ) -> NativeRecordingSession {
        let output = directory.join(VIDEO_PARTIAL_MP4);
        std::fs::write(&output, b"fixture-fragmented-mp4").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }
            Ok(snapshot)
        });
        let (_sender, evidence) = watch::channel(snapshot.recording_evidence());
        NativeRecordingSession {
            directory: directory.to_path_buf(),
            output,
            worker: NativeWorker {
                stop,
                thread: Some(thread),
            },
            evidence,
            video_started_at: Instant::now(),
            recorded_at: SystemTime::now(),
        }
    }

    #[tokio::test]
    async fn successful_native_lifecycle_publishes_only_after_worker_join() {
        let directory = tempfile::tempdir().unwrap();
        let session = fake_session(directory.path(), telemetry());
        assert!(directory.path().join(VIDEO_PARTIAL_MP4).is_file());
        assert!(!directory.path().join(VIDEO_MP4).exists());

        let published = session.stop().await.unwrap();
        assert_eq!(published, directory.path());
        assert!(directory.path().join(VIDEO_MP4).is_file());
        assert!(!directory.path().join(VIDEO_PARTIAL_MP4).exists());
    }

    #[tokio::test]
    async fn failed_native_lifecycle_preserves_explicit_partial_output() {
        let directory = tempfile::tempdir().unwrap();
        let session = fake_session(directory.path(), telemetry());

        let error = session
            .stop_with_failure("fixture watchdog failure".to_owned())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("fixture watchdog failure"));
        assert!(directory.path().join(VIDEO_PARTIAL_MP4).is_file());
        assert!(!directory.path().join(VIDEO_MP4).exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    #[ignore = "requires QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID and QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG"]
    async fn real_native_startup_cancellation_reaps_the_worker() {
        let pid = std::env::var("QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let ffmpeg =
            PathBuf::from(std::env::var("QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG").unwrap());
        let target = crate::platform::capture_target_for_process(pid).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let (cancel, cancellation) = watch::channel(false);
        cancel.send(true).unwrap();
        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            NativeRecordingSession::start(
                directory.path().to_path_buf(),
                target,
                ffmpeg,
                AudioSource::Silent,
                cancellation,
                Duration::from_secs(15),
            ),
        )
        .await
        .expect("native startup cancellation exceeded ten seconds");
        let error = match result {
            Ok(session) => {
                let _ = session
                    .stop_with_failure("fixture expected cancellation".to_owned())
                    .await;
                panic!("native session became ready after pre-start cancellation");
            }
            Err(error) => error,
        };
        assert!(error.to_string().contains("startup was cancelled"));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!directory.path().join(VIDEO_MP4).exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    #[ignore = "requires QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID and QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG; optional duration/output environment variables preserve long-run evidence"]
    async fn real_native_lifecycle_records_a_non_league_window() {
        let pid = std::env::var("QUEUEBACK_NATIVE_LIFECYCLE_TEST_PID")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let ffmpeg =
            PathBuf::from(std::env::var("QUEUEBACK_NATIVE_LIFECYCLE_TEST_FFMPEG").unwrap());
        let decode_ffmpeg = ffmpeg.clone();
        let target = crate::platform::capture_target_for_process(pid).unwrap();
        let persistent_directory =
            std::env::var_os("QUEUEBACK_NATIVE_LIFECYCLE_TEST_OUTPUT").map(PathBuf::from);
        let temporary_directory = persistent_directory
            .is_none()
            .then(|| tempfile::tempdir().unwrap());
        let directory = persistent_directory
            .clone()
            .unwrap_or_else(|| temporary_directory.as_ref().unwrap().path().to_path_buf());
        if persistent_directory.is_some() {
            assert!(
                !directory.exists(),
                "persistent native lifecycle fixture directory must be new: {}",
                directory.display()
            );
            std::fs::create_dir_all(&directory).unwrap();
        }
        let duration_seconds = std::env::var("QUEUEBACK_NATIVE_LIFECYCLE_TEST_SECONDS")
            .map_or(Ok(5), |value| value.parse::<u64>())
            .expect("QUEUEBACK_NATIVE_LIFECYCLE_TEST_SECONDS must be an integer");
        assert!(
            (1..=3_600).contains(&duration_seconds),
            "native lifecycle fixture duration must be between 1 and 3600 seconds"
        );
        let (_cancel, cancellation) = watch::channel(false);
        let session = NativeRecordingSession::start(
            directory.clone(),
            target,
            ffmpeg,
            AudioSource::Silent,
            cancellation,
            Duration::from_secs(15),
        )
        .await
        .unwrap();
        assert!(session.evidence.borrow().startup_ready());
        let final_evidence = session.evidence_receiver();
        tokio::time::sleep(Duration::from_secs(duration_seconds)).await;

        let published = session.stop().await.unwrap();
        let video = published.join(VIDEO_MP4);
        assert!(video.is_file());
        assert!(std::fs::metadata(&video).unwrap().len() > 0);
        let final_evidence = final_evidence.borrow().clone();
        assert!(final_evidence.capture_terminal);
        assert!(final_evidence.progress_end);
        assert_eq!(final_evidence.frame_pool_capacity, Some(2));
        assert_eq!(final_evidence.output_pool_capacity, Some(1));
        assert!(final_evidence.protocol_error.is_none());
        assert!(
            final_evidence.encoded_frames >= duration_seconds.saturating_mul(60),
            "native lifecycle encoded {} frames during a {}-second fixture",
            final_evidence.encoded_frames,
            duration_seconds
        );
        assert!(final_evidence.muxed_bytes > 0);
        assert!(final_evidence.output_time_us.is_some_and(|value| value > 0));
        assert!(
            final_evidence
                .latest_qpc
                .zip(final_evidence.first_qpc)
                .is_some_and(|(latest, first)| latest > first)
        );
        if persistent_directory.is_some() {
            std::fs::write(
                published.join("recording-evidence.txt"),
                format!("duration_seconds={duration_seconds}\n{final_evidence:#?}\n"),
            )
            .unwrap();
        }
        assert!(
            std::process::Command::new(decode_ffmpeg)
                .args(["-v", "error", "-i"])
                .arg(video)
                .args(["-f", "null", "-"])
                .status()
                .unwrap()
                .success()
        );
    }
}
