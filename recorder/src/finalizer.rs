use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chronobreak_replay_time::{
    FFPROBE_SUMMARY_SHOW_ENTRIES, FinalizationExpectationsV2, FrameBoundary, MediaId,
    MediaTimelineV2, Rational, parse_ffprobe_media_timeline,
};
use queueback_media_runtime::{BoundedProcessErrorKind, run_bounded_process};
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::storage::{METADATA_JSON, VIDEO_MP4};

const FINALIZATION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const FINALIZATION_PROBE_OUTPUT_LIMIT: usize = 1024 * 1024;
const METADATA_PENDING_JSON: &str = "metadata.pending.json";

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A fully closed private media file plus recorder-owned cadence evidence.
///
/// This type deliberately does not imply publication or validated media facts.
#[derive(Debug)]
pub struct CompletedVideoCandidate {
    directory: PathBuf,
    partial_path: PathBuf,
    expected_frame_rate: Rational,
    expected_frame_count: FrameBoundary,
}

impl CompletedVideoCandidate {
    pub(crate) fn new(
        directory: PathBuf,
        partial_path: PathBuf,
        expected_frame_rate: Rational,
        expected_frame_count: FrameBoundary,
    ) -> Result<Self> {
        if expected_frame_count.get() == 0 {
            bail!("completed video candidate has no frames");
        }
        let parent = partial_path
            .parent()
            .context("completed video candidate has no parent directory")?;
        if parent != directory {
            bail!("completed video candidate is outside its recording directory");
        }
        let file_name = partial_path
            .file_name()
            .and_then(|value| value.to_str())
            .context("completed video candidate has no UTF-8 file name")?;
        if !file_name.ends_with(".partial.mp4") {
            bail!("completed video candidate does not use the private partial suffix");
        }
        Ok(Self {
            directory,
            partial_path,
            expected_frame_rate,
            expected_frame_count,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn partial_path(&self) -> &Path {
        &self.partial_path
    }

    pub fn expected_frame_rate(&self) -> Rational {
        self.expected_frame_rate
    }

    pub fn expected_frame_count(&self) -> FrameBoundary {
        self.expected_frame_count
    }
}

/// Candidate that has passed the bounded packaged-runtime media probe.
#[derive(Debug)]
pub(crate) struct ValidatedVideoCandidate {
    candidate: CompletedVideoCandidate,
    timeline: MediaTimelineV2,
    #[cfg(feature = "replay-time-fixture")]
    probe_stdout_bytes: usize,
    #[cfg(feature = "replay-time-fixture")]
    probe_stderr_bytes: usize,
}

impl ValidatedVideoCandidate {
    pub(crate) fn timeline(&self) -> &MediaTimelineV2 {
        &self.timeline
    }
}

/// Metadata accepted by the atomic v2 publication boundary.
pub(crate) trait FinalMetadata: Serialize {
    fn media_timeline(&self) -> &MediaTimelineV2;
}

#[derive(Debug)]
struct ProbeOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    exceeded_output_limit: bool,
}

pub(crate) async fn validate_candidate(
    ffprobe: &Path,
    candidate: CompletedVideoCandidate,
    mut expectations: FinalizationExpectationsV2,
) -> Result<ValidatedVideoCandidate> {
    if expectations.producer.expected_frame_rate != candidate.expected_frame_rate()
        || expectations.producer.expected_frame_count != candidate.expected_frame_count()
    {
        bail!("finalization expectations do not match the private candidate evidence");
    }
    expectations.require_zero_video_start = true;
    let output = run_packaged_probe(ffprobe, candidate.partial_path()).await?;
    #[cfg(feature = "replay-time-fixture")]
    let probe_stdout_bytes = output.stdout.len();
    #[cfg(feature = "replay-time-fixture")]
    let probe_stderr_bytes = output.stderr.len();
    let timeline = evaluate_probe_output(output, expectations)?;
    Ok(ValidatedVideoCandidate {
        candidate,
        timeline,
        #[cfg(feature = "replay-time-fixture")]
        probe_stdout_bytes,
        #[cfg(feature = "replay-time-fixture")]
        probe_stderr_bytes,
    })
}

/// Successful observation from the production finalized-media validator.
#[cfg(feature = "replay-time-fixture")]
#[derive(Debug)]
pub struct FinalizerFixtureValidation {
    /// Validated media timeline produced by the production parser.
    pub timeline: MediaTimelineV2,
    /// Bytes emitted on stdout by the bounded production ffprobe.
    pub probe_stdout_bytes: usize,
    /// Bytes emitted on stderr by the bounded production ffprobe.
    pub probe_stderr_bytes: usize,
}

/// Production ffprobe timeout used by the finalizer fixture wrapper.
#[cfg(feature = "replay-time-fixture")]
pub const FINALIZER_FIXTURE_PROBE_TIMEOUT: Duration = FINALIZATION_PROBE_TIMEOUT;

/// Production combined-output ceiling used by the finalizer fixture wrapper.
#[cfg(feature = "replay-time-fixture")]
pub const FINALIZER_FIXTURE_PROBE_OUTPUT_LIMIT: usize = FINALIZATION_PROBE_OUTPUT_LIMIT;

/// Runs production finalized-media validation without exposing publication.
#[cfg(feature = "replay-time-fixture")]
pub async fn validate_finalizer_fixture(
    ffprobe: &Path,
    directory: PathBuf,
    partial_path: PathBuf,
    expected_frame_rate: Rational,
    expected_frame_count: FrameBoundary,
    expectations: FinalizationExpectationsV2,
) -> Result<FinalizerFixtureValidation> {
    let candidate = CompletedVideoCandidate::new(
        directory,
        partial_path,
        expected_frame_rate,
        expected_frame_count,
    )?;
    let validated = validate_candidate(ffprobe, candidate, expectations).await?;
    Ok(FinalizerFixtureValidation {
        timeline: validated.timeline,
        probe_stdout_bytes: validated.probe_stdout_bytes,
        probe_stderr_bytes: validated.probe_stderr_bytes,
    })
}

fn evaluate_probe_output(
    output: ProbeOutput,
    expectations: FinalizationExpectationsV2,
) -> Result<MediaTimelineV2> {
    if output.timed_out {
        bail!("finalized-media ffprobe exceeded its five-second deadline");
    }
    if output.exceeded_output_limit {
        bail!("finalized-media ffprobe exceeded the 1 MiB combined output limit");
    }
    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("finalized-media ffprobe failed: {}", stderr.trim());
    }
    let stdout = std::str::from_utf8(&output.stdout)
        .context("finalized-media ffprobe output is not UTF-8")?;
    parse_ffprobe_media_timeline(stdout, expectations)
        .map_err(|error| anyhow!(error))
        .context("finalized media does not satisfy the replay-time contract")
}

async fn run_packaged_probe(ffprobe: &Path, media: &Path) -> Result<ProbeOutput> {
    let mut command = Command::new(ffprobe);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .arg("-v")
        .arg("error")
        .arg("-print_format")
        .arg("json")
        .arg("-show_entries")
        .arg(FFPROBE_SUMMARY_SHOW_ENTRIES)
        .arg(media);

    let output = match run_bounded_process(
        command,
        FINALIZATION_PROBE_TIMEOUT,
        FINALIZATION_PROBE_OUTPUT_LIMIT,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return match error.kind() {
                BoundedProcessErrorKind::Timeout => Ok(ProbeOutput {
                    success: false,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    timed_out: true,
                    exceeded_output_limit: false,
                }),
                BoundedProcessErrorKind::OutputLimit => Ok(ProbeOutput {
                    success: false,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    timed_out: false,
                    exceeded_output_limit: true,
                }),
                BoundedProcessErrorKind::Spawn | BoundedProcessErrorKind::Execution => {
                    Err(anyhow!(error)).with_context(|| {
                        format!("failed to supervise packaged ffprobe {}", ffprobe.display())
                    })
                }
            };
        }
    };
    Ok(ProbeOutput {
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
        timed_out: false,
        exceeded_output_limit: false,
    })
}

pub(crate) async fn publish_validated_candidate<T>(
    validated: ValidatedVideoCandidate,
    game_log_media_id: &MediaId,
    metadata: &T,
) -> Result<PathBuf>
where
    T: FinalMetadata + ?Sized,
{
    if &validated.timeline.media_id != game_log_media_id
        || metadata.media_timeline() != &validated.timeline
    {
        bail!("metadata, game log, and validated media identity/timeline do not match");
    }

    let directory = validated.candidate.directory();
    let canonical_video = directory.join(VIDEO_MP4);
    let canonical_metadata = directory.join(METADATA_JSON);
    let pending_metadata = directory.join(METADATA_PENDING_JSON);
    for path in [&canonical_video, &canonical_metadata, &pending_metadata] {
        if tokio::fs::try_exists(path)
            .await
            .with_context(|| format!("failed to inspect publication target {}", path.display()))?
        {
            bail!(
                "refusing to overwrite recording artifact {}",
                path.display()
            );
        }
    }

    let mut bytes =
        serde_json::to_vec_pretty(metadata).context("failed to serialize v2 metadata")?;
    bytes.push(b'\n');
    let mut pending = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending_metadata)
        .await
        .with_context(|| {
            format!(
                "failed to create pending metadata {}",
                pending_metadata.display()
            )
        })?;
    pending.write_all(&bytes).await.with_context(|| {
        format!(
            "failed to write pending metadata {}",
            pending_metadata.display()
        )
    })?;
    pending.sync_all().await.with_context(|| {
        format!(
            "failed to flush pending metadata {}",
            pending_metadata.display()
        )
    })?;
    drop(pending);

    if let Err(error) =
        tokio::fs::rename(validated.candidate.partial_path(), &canonical_video).await
    {
        return Err(error).with_context(|| {
            format!(
                "failed to publish validated video {} as {}",
                validated.candidate.partial_path().display(),
                canonical_video.display()
            )
        });
    }

    if let Err(error) = tokio::fs::rename(&pending_metadata, &canonical_metadata).await {
        let rollback =
            tokio::fs::rename(&canonical_video, validated.candidate.partial_path()).await;
        return match rollback {
            Ok(()) => Err(error).with_context(|| {
                format!(
                    "failed to publish metadata {}; validated video was returned to its partial path",
                    canonical_metadata.display()
                )
            }),
            Err(rollback_error) => Err(error).with_context(|| {
                format!(
                    "failed to publish metadata {} and could not return {} to its partial path: {rollback_error}",
                    canonical_metadata.display(),
                    canonical_video.display()
                )
            }),
        };
    }
    Ok(directory.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_replay_time::{
        AudioTimelineV2, ContainerTimelineV2, MediaPts, ProducerEvidenceV2,
        REPLAY_TICKS_PER_SECOND, ReplayTick, SignedReplayTick, VideoTimelineV2,
    };
    use serde::Serialize;

    fn media_id() -> MediaId {
        MediaId::parse("11111111-2222-4333-8444-555555555555").unwrap()
    }

    fn rational(numerator: i64, denominator: u64) -> Rational {
        Rational::new(numerator, denominator).unwrap()
    }

    fn timeline() -> MediaTimelineV2 {
        MediaTimelineV2 {
            schema_version: 2,
            replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
            media_id: media_id(),
            video: VideoTimelineV2 {
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                time_base: rational(1, 15_360),
                first_pts: MediaPts::new(0),
                frame_rate: rational(60, 1),
                frame_count: FrameBoundary::new(300).unwrap(),
                one_past_last_pts: MediaPts::new(76_800),
                replay_end: ReplayTick::new(240_000_000).unwrap(),
                exact_cfr: true,
            },
            audio: AudioTimelineV2 {
                present: true,
                codec: Some("aac".to_owned()),
                sample_rate: Some(48_000),
                time_base: Some(rational(1, 48_000)),
                first_pts: Some(MediaPts::new(0)),
                replay_start: Some(SignedReplayTick::new(0).unwrap()),
                replay_end: Some(SignedReplayTick::new(240_000_000).unwrap()),
            },
            container: ContainerTimelineV2 {
                start_seconds: rational(0, 1),
                duration_seconds: rational(5, 1),
            },
            producer: ProducerEvidenceV2 {
                backend: "test".to_owned(),
                expected_frame_rate: rational(60, 1),
                expected_frame_count: FrameBoundary::new(300).unwrap(),
                media_runtime_id: "test-runtime".to_owned(),
            },
            capture: None,
        }
    }

    #[derive(Serialize)]
    struct TestMetadata {
        schema_version: u32,
        media_timeline: MediaTimelineV2,
    }

    impl FinalMetadata for TestMetadata {
        fn media_timeline(&self) -> &MediaTimelineV2 {
            &self.media_timeline
        }
    }

    fn validated(directory: &Path, timeline: MediaTimelineV2) -> ValidatedVideoCandidate {
        let partial = directory.join("video.partial.mp4");
        std::fs::write(&partial, b"fixture").unwrap();
        ValidatedVideoCandidate {
            candidate: CompletedVideoCandidate::new(
                directory.to_path_buf(),
                partial,
                rational(60, 1),
                FrameBoundary::new(300).unwrap(),
            )
            .unwrap(),
            timeline,
            #[cfg(feature = "replay-time-fixture")]
            probe_stdout_bytes: 0,
            #[cfg(feature = "replay-time-fixture")]
            probe_stderr_bytes: 0,
        }
    }

    #[test]
    fn probe_result_rejects_timeout_oversize_failure_and_malformed_json() {
        let expectations = FinalizationExpectationsV2 {
            media_id: media_id(),
            expected_video_codec: "h264".to_owned(),
            expected_audio_codec: Some("aac".to_owned()),
            producer: timeline().producer,
            capture: None,
            require_zero_video_start: true,
        };
        let output = |stdout: &[u8], success, timed_out, exceeded_output_limit| ProbeOutput {
            success,
            stdout: stdout.to_vec(),
            stderr: b"fixture failure".to_vec(),
            timed_out,
            exceeded_output_limit,
        };
        assert!(
            evaluate_probe_output(output(b"{}", true, true, false), expectations.clone()).is_err()
        );
        assert!(
            evaluate_probe_output(output(b"{}", true, false, true), expectations.clone()).is_err()
        );
        assert!(
            evaluate_probe_output(output(b"{}", false, false, false), expectations.clone())
                .is_err()
        );
        assert!(
            evaluate_probe_output(output(b"not-json", true, false, false), expectations).is_err()
        );
    }

    #[tokio::test]
    async fn publication_requires_identity_and_exposes_video_then_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let timeline = timeline();
        let metadata = TestMetadata {
            schema_version: 2,
            media_timeline: timeline.clone(),
        };
        let result = publish_validated_candidate(
            validated(directory.path(), timeline),
            &media_id(),
            &metadata,
        )
        .await
        .unwrap();
        assert_eq!(result, directory.path());
        assert!(directory.path().join(VIDEO_MP4).exists());
        assert!(directory.path().join(METADATA_JSON).exists());
        assert!(!directory.path().join(METADATA_PENDING_JSON).exists());
    }

    #[tokio::test]
    async fn publication_rejects_stale_media_identity_without_renaming_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let timeline = timeline();
        let partial = directory.path().join("video.partial.mp4");
        let metadata = TestMetadata {
            schema_version: 2,
            media_timeline: timeline.clone(),
        };
        let stale = MediaId::parse("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee").unwrap();
        assert!(
            publish_validated_candidate(validated(directory.path(), timeline), &stale, &metadata)
                .await
                .is_err()
        );
        assert!(partial.exists());
        assert!(!directory.path().join(VIDEO_MP4).exists());
        assert!(!directory.path().join(METADATA_JSON).exists());
    }
}
