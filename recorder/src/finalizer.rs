use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use chronobreak_replay_time::{
    FFPROBE_SUMMARY_SHOW_ENTRIES, FinalizationExpectationsV2, FrameBoundary, MediaId,
    MediaTimelineV2, Rational, parse_ffprobe_media_timeline,
};
use queueback_media_runtime::{BoundedProcessErrorKind, run_bounded_process};
use serde::Serialize;
#[cfg(not(windows))]
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::storage::{METADATA_JSON, VIDEO_MP4};

mod fragments;
#[cfg(windows)]
mod publication;
#[cfg(windows)]
pub(crate) mod recovery;

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
    #[cfg(windows)]
    receipt: publication::Receipt,
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
    #[cfg(windows)]
    let _directory = publication::DirectoryPins::open(candidate.directory())?;
    #[cfg(windows)]
    let _media = publication::open_file(candidate.partial_path(), false)?;
    #[cfg(windows)]
    let receipt = publication::Receipt::capture(&_directory, &_media, &expectations.media_id)?;
    let timeline = validate_media(ffprobe, candidate.partial_path(), expectations).await?;
    Ok(ValidatedVideoCandidate {
        candidate,
        timeline: timeline.0,
        #[cfg(windows)]
        receipt,
        #[cfg(feature = "replay-time-fixture")]
        probe_stdout_bytes: timeline.1,
        #[cfg(feature = "replay-time-fixture")]
        probe_stderr_bytes: timeline.2,
    })
}

async fn validate_media(
    ffprobe: &Path,
    media: &Path,
    mut expectations: FinalizationExpectationsV2,
) -> Result<(MediaTimelineV2, usize, usize)> {
    expectations.require_zero_video_start = true;
    let deadline = Instant::now() + FINALIZATION_PROBE_TIMEOUT;
    let mut output = run_packaged_probe(ffprobe, media).await?;
    let probe_stdout_bytes = output.stdout.len();
    let probe_stderr_bytes = output.stderr.len();
    let timing = if output.success && !output.timed_out && !output.exceeded_output_limit {
        Some(
            complete_fragmented_media_facts(
                &mut output.stdout,
                media,
                deadline,
                expectations.producer.expected_frame_rate,
            )
            .await?,
        )
    } else {
        None
    };
    let timeline = evaluate_probe_output(output, expectations)?;
    let timing = timing.context("independent MP4 timing evidence missing")?;
    ensure!(
        timeline.video.time_base == Rational::new(1, u64::from(timing.timescale))?
            && timeline.video.frame_count.get() == timing.samples
            && timeline.video.one_past_last_pts.get() == i64::try_from(timing.end_pts)?,
        "MP4 sample timing disagrees with ffprobe timeline"
    );
    Ok((timeline, probe_stdout_bytes, probe_stderr_bytes))
}

async fn complete_fragmented_media_facts(
    stdout: &mut Vec<u8>,
    media: &Path,
    deadline: Instant,
    rate: Rational,
) -> Result<fragments::VideoTiming> {
    let mut summary: serde_json::Value = serde_json::from_slice(stdout)?;
    let streams = summary["streams"]
        .as_array_mut()
        .context("ffprobe streams missing")?;
    let mut videos = streams
        .iter_mut()
        .filter(|stream| stream["codec_type"] == "video");
    let video = videos.next().context("ffprobe video stream missing")?;
    ensure!(
        videos.next().is_none(),
        "exactly one ffprobe video stream is required"
    );
    ensure!(
        video["width"] == 1920 && video["height"] == 1080,
        "native recording dimensions must be 1920x1080"
    );
    ensure!(
        video["codec_name"] == "h264" && video["profile"] == "High",
        "native recording must be H.264 High profile"
    );
    // Dimensions are observed here, without widening persisted replay-time schema
    // or the generic export-summary contract. All remaining strict fields still
    // pass through the unchanged shared replay-time parser.
    let object = video
        .as_object_mut()
        .context("ffprobe video stream is not an object")?;
    object.remove("width");
    object.remove("height");
    let path = media.to_path_buf();
    // Keep ownership until the deadline-aware reader exits: timing out and
    // dropping a spawn_blocking handle would leave filesystem work detached.
    let timing =
        tokio::task::spawn_blocking(move || fragments::video_sample_timing(&path, deadline, rate))
            .await
            .context("fragment scan worker failed")??;
    if video["nb_frames"].is_null() {
        video["nb_frames"] = timing.samples.to_string().into();
    } else {
        ensure!(
            video["nb_frames"].as_str() == Some(timing.samples.to_string().as_str()),
            "ffprobe frame count disagrees with MP4 samples"
        );
    }
    *stdout = serde_json::to_vec(&summary)?;
    Ok(timing)
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
        .arg(FFPROBE_SUMMARY_SHOW_ENTRIES.replacen("stream=", "stream=width,height,", 1))
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

    #[cfg(windows)]
    {
        publish_windows(validated, metadata)
    }
    #[cfg(not(windows))]
    {
        let directory = validated.candidate.directory();
        let canonical_video = directory.join(VIDEO_MP4);
        let canonical_metadata = directory.join(METADATA_JSON);
        let pending_metadata = directory.join(METADATA_PENDING_JSON);
        for path in [&canonical_video, &canonical_metadata, &pending_metadata] {
            if tokio::fs::try_exists(path).await.with_context(|| {
                format!("failed to inspect publication target {}", path.display())
            })? {
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
}

#[cfg(windows)]
fn publish_windows<T: FinalMetadata + ?Sized>(
    validated: ValidatedVideoCandidate,
    metadata: &T,
) -> Result<PathBuf> {
    let directory = validated.candidate.directory();
    let pins = publication::DirectoryPins::open(directory)?;
    for leaf in [VIDEO_MP4, METADATA_JSON, METADATA_PENDING_JSON] {
        if publication::exists(&directory.join(leaf))? {
            bail!("refusing to overwrite recording artifact {leaf}");
        }
    }
    let media = publication::open_file(validated.candidate.partial_path(), true)?;
    validated
        .receipt
        .verify(&pins, &media, &validated.timeline.media_id)?;
    let mut value = serde_json::to_value(metadata)?;
    let object = value
        .as_object_mut()
        .context("metadata must be an object")?;
    object.insert(
        "publication".to_owned(),
        serde_json::to_value(&validated.receipt)?,
    );
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    let pending = publication::create_pending(&directory.join(METADATA_PENDING_JSON), &bytes)?;
    #[cfg(test)]
    recovery::tests::publication_checkpoint("pending");
    pins.rename(&media, VIDEO_MP4)?;
    #[cfg(test)]
    recovery::tests::publication_checkpoint("video");
    if let Err(error) = pins.rename(&pending, METADATA_JSON) {
        return match pins.rename(&media, crate::storage::VIDEO_PARTIAL_MP4) {
            Ok(()) => Err(error).context("metadata publication failed; video returned to partial"),
            Err(rollback) => Err(error).context(format!(
                "metadata publication failed; video retained at canonical path: {rollback:#}"
            )),
        };
    }
    #[cfg(test)]
    recovery::tests::publication_checkpoint("metadata");
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

    pub(super) fn timeline() -> MediaTimelineV2 {
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
            #[cfg(windows)]
            receipt: publication::Receipt::capture(
                &publication::DirectoryPins::open(directory).unwrap(),
                &publication::open_file(&partial, false).unwrap(),
                &timeline.media_id,
            )
            .unwrap(),
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

    #[tokio::test]
    #[ignore = "requires QUEUEBACK_FINALIZER_MEDIA, QUEUEBACK_FINALIZER_FFPROBE and QUEUEBACK_FINALIZER_FRAMES; reads source unchanged and publishes only a temporary copy"]
    async fn real_fragmented_media_validates_and_publishes_a_copy() {
        let source = PathBuf::from(std::env::var_os("QUEUEBACK_FINALIZER_MEDIA").unwrap());
        let ffprobe = PathBuf::from(std::env::var_os("QUEUEBACK_FINALIZER_FFPROBE").unwrap());
        let frames = std::env::var("QUEUEBACK_FINALIZER_FRAMES")
            .unwrap()
            .parse()
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let partial = directory.path().join("video.partial.mp4");
        std::fs::copy(&source, &partial).unwrap();
        let expectations = FinalizationExpectationsV2 {
            media_id: media_id(),
            expected_video_codec: "h264".to_owned(),
            expected_audio_codec: Some("aac".to_owned()),
            producer: ProducerEvidenceV2 {
                expected_frame_count: FrameBoundary::new(frames).unwrap(),
                ..timeline().producer
            },
            capture: None,
            require_zero_video_start: true,
        };
        let probe = run_packaged_probe(&ffprobe, &partial).await.unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert!(
            raw["streams"][0]["nb_frames"].is_null(),
            "fixture must exercise fragmented MP4 with no nb_frames"
        );
        let make = |count| {
            CompletedVideoCandidate::new(
                directory.path().to_path_buf(),
                partial.clone(),
                rational(60, 1),
                FrameBoundary::new(count).unwrap(),
            )
            .unwrap()
        };
        let mut wrong = expectations.clone();
        wrong.producer.expected_frame_count = FrameBoundary::new(frames + 1).unwrap();
        let error = validate_candidate(&ffprobe, make(frames + 1), wrong)
            .await
            .unwrap_err();
        assert!(format!("{error:#}").contains("video frame count does not match"));
        assert!(partial.is_file());
        let started = Instant::now();
        let validated = validate_candidate(&ffprobe, make(frames), expectations)
            .await
            .unwrap();
        assert_eq!(validated.timeline.video.frame_count.get(), frames);
        println!(
            "fragmented finalization: {} samples in {:?}",
            frames,
            started.elapsed()
        );
        let metadata = TestMetadata {
            schema_version: 2,
            media_timeline: validated.timeline.clone(),
        };
        publish_validated_candidate(validated, &media_id(), &metadata)
            .await
            .unwrap();
        assert!(directory.path().join(VIDEO_MP4).is_file());
        assert!(directory.path().join(METADATA_JSON).is_file());
        assert!(!partial.exists());
        assert!(source.is_file());
    }

    #[tokio::test]
    #[ignore = "requires QUEUEBACK_TEST_MEDIA_RUNTIME; generated temporary media only"]
    async fn packaged_native_contract_rejects_wrong_format_and_sample_cadence() {
        let root = PathBuf::from(std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").unwrap());
        let tools = queueback_media_runtime::resolve_root(&root).await.unwrap();
        for (name, dimensions, profile, skew, expected_error) in [
            ("valid", "1920x1080", "high", false, None),
            ("dimensions", "1280x720", "high", false, Some("dimensions")),
            (
                "profile",
                "1920x1080",
                "baseline",
                false,
                Some("High profile"),
            ),
            (
                "cadence",
                "1920x1080",
                "high",
                true,
                Some("sample duration"),
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let partial = directory.path().join("video.partial.mp4");
            let mut command = std::process::Command::new(tools.ffmpeg());
            command.args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=size={dimensions}:rate=60:duration=0.5"),
                "-f",
                "lavfi",
                "-i",
                "sine=sample_rate=48000:duration=0.5",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-preset",
                "fast",
                "-profile:v",
                profile,
                "-bf",
                "0",
                "-g",
                "30",
                "-c:a",
                "aac",
                "-movflags",
                crate::encoder::FRAGMENTED_MP4_FLAGS,
                "-avoid_negative_ts",
                "disabled",
            ]);
            if skew {
                command.args(["-bsf:v", "setts=pts='N*256+if(eq(N,1),1,0)':dts='N*256+if(eq(N,1),1,0)':duration=256:time_base=1/15360:prescale=1"]);
            }
            let generated = command.arg(&partial).output().unwrap();
            assert!(
                generated.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&generated.stderr)
            );
            let original = std::fs::read(&partial).unwrap();
            let expectations = FinalizationExpectationsV2 {
                media_id: media_id(),
                expected_video_codec: "h264".into(),
                expected_audio_codec: Some("aac".into()),
                producer: ProducerEvidenceV2 {
                    expected_frame_count: FrameBoundary::new(30).unwrap(),
                    ..timeline().producer
                },
                capture: None,
                require_zero_video_start: true,
            };
            if skew {
                // Establish the adversary actually satisfies the old aggregate
                // validator, using the independently known generated count.
                let output = run_packaged_probe(tools.ffprobe(), &partial).await.unwrap();
                let mut summary: serde_json::Value =
                    serde_json::from_slice(&output.stdout).unwrap();
                let video = summary["streams"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|s| s["codec_type"] == "video")
                    .unwrap();
                video["nb_frames"] = "30".into();
                video.as_object_mut().unwrap().remove("width");
                video.as_object_mut().unwrap().remove("height");
                parse_ffprobe_media_timeline(&summary.to_string(), expectations.clone())
                    .expect("skew fixture must retain valid aggregate statistics");
            }
            let candidate = CompletedVideoCandidate::new(
                directory.path().to_path_buf(),
                partial.clone(),
                rational(60, 1),
                FrameBoundary::new(30).unwrap(),
            )
            .unwrap();
            let result = validate_candidate(tools.ffprobe(), candidate, expectations).await;
            if let Some(message) = expected_error {
                let error = result.unwrap_err();
                assert!(format!("{error:#}").contains(message), "{name}: {error:#}");
                assert_eq!(std::fs::read(&partial).unwrap(), original);
                assert!(!directory.path().join(VIDEO_MP4).exists());
                assert!(!directory.path().join(METADATA_JSON).exists());
                assert!(!directory.path().join(METADATA_PENDING_JSON).exists());
            } else {
                let validated = result.unwrap();
                let metadata = TestMetadata {
                    schema_version: 2,
                    media_timeline: validated.timeline.clone(),
                };
                publish_validated_candidate(validated, &media_id(), &metadata)
                    .await
                    .unwrap();
                assert_eq!(
                    std::fs::read(directory.path().join(VIDEO_MP4)).unwrap(),
                    original
                );
            }
        }
    }
}
