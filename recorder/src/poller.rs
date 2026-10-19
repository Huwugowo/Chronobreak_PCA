use anyhow::{Result, bail};
use chronobreak_replay_time::{
    CalibrationStatus, GameCalibrationV2, GameTick, MediaId, MediaTimelineV2,
    REPLAY_TICKS_PER_GAME_TICK, SignedReplayTick,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
pub mod acquisition;
pub mod persistence;
mod session;
pub(crate) use session::{IdentityProbeSource, PollerSession};
#[cfg(test)]
mod legacy_decode_baseline;
const LIVE_CLIENT_BASE_URL: &str = "https://127.0.0.1:2999/liveclientdata";
const CALIBRATION_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const CALIBRATION_PROBE_INTERVAL: Duration = Duration::from_millis(250);
const CALIBRATION_MIN_WINDOW_SPAN: Duration = Duration::from_millis(750);
const CALIBRATION_CLOCK_TOLERANCE: Duration = Duration::from_millis(250);
const CALIBRATION_SAMPLE_COUNT: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollerSummary {
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingMetadata {
    pub schema_version: u32,
    pub media_id: MediaId,
    pub media_timeline: MediaTimelineV2,
    pub recorded_at: String,
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
    pub encoder_used: String,
    pub recording_codec: String,
    pub recording_profile: String,
    pub recording_resolution: String,
    pub capture_backend: String,
    pub capture_adapter_luid: Option<String>,
    pub capture_adapter_name: Option<String>,
    pub capture_output: Option<String>,
    pub encoder_interop: Option<String>,
    pub media_runtime_id: String,
    pub capture_support_label: String,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<CaptureMetadata>,
    pub saved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureMetadata {
    pub schema_version: u32,
    pub backend: String,
    pub diagnostics_abi: u32,
    pub support_label: String,
    pub capture_adapter_luid: String,
    pub encoder_adapter_luid: String,
    pub capture_adapter_name: String,
    pub capture_output: String,
    pub encoder_backend: String,
    pub encoder_interop: String,
    pub media_runtime_id: String,
    pub source_format: String,
    pub converted_format: String,
    pub host_readback: bool,
    pub gpu_stages: Vec<String>,
    pub frame_pool_capacity: u32,
    pub capture_output_pool_capacity: u32,
    pub filter_buffered_frame_limit: u32,
    pub encoder_depth: u32,
    pub progress_stall_timeout_seconds: u32,
    pub maximum_texture_bytes: u64,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub encoded_frames: u64,
    pub muxed_bytes: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    pub first_qpc_100ns: i64,
    pub latest_qpc_100ns: i64,
    pub terminal_progress: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingDetails {
    pub encoder_used: String,
    pub codec: String,
    pub profile: String,
    pub resolution: String,
    pub fps: u32,
    pub capture_backend: String,
    pub capture_adapter_luid: Option<String>,
    pub capture_adapter_name: Option<String>,
    pub capture_output: Option<String>,
    pub encoder_interop: Option<String>,
    pub media_runtime_id: String,
    pub capture_support_label: String,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub pool_recreations: u64,
    pub capture: Option<CaptureMetadata>,
}

impl RecordingMetadata {
    pub fn new(
        recorded_at: String,
        media_timeline: MediaTimelineV2,
        summary: PollerSummary,
        recording: RecordingDetails,
    ) -> Self {
        let media_id = media_timeline.media_id.clone();

        Self {
            schema_version: 2,
            media_id,
            media_timeline,
            recorded_at,
            game_mode: summary.game_mode,
            local_player_summoner_name: summary.local_player_summoner_name,
            local_player_champion: summary.local_player_champion,
            local_player_team: summary.local_player_team,
            encoder_used: recording.encoder_used,
            recording_codec: recording.codec,
            recording_profile: recording.profile,
            recording_resolution: recording.resolution,
            capture_backend: recording.capture_backend,
            capture_adapter_luid: recording.capture_adapter_luid,
            capture_adapter_name: recording.capture_adapter_name,
            capture_output: recording.capture_output,
            encoder_interop: recording.encoder_interop,
            media_runtime_id: recording.media_runtime_id,
            capture_support_label: recording.capture_support_label,
            source_frames_surfaced: recording.source_frames_surfaced,
            source_frames_superseded: recording.source_frames_superseded,
            cfr_duplicates: recording.cfr_duplicates,
            cfr_discards: recording.cfr_discards,
            pool_recreations: recording.pool_recreations,
            capture: recording.capture,
            saved: false,
        }
    }
}

impl crate::finalizer::FinalMetadata for RecordingMetadata {
    fn media_timeline(&self) -> &MediaTimelineV2 {
        &self.media_timeline
    }
}

#[derive(Debug, Clone, Copy)]
struct ClockSample {
    midpoint: Instant,
    game_tick: GameTick,
    round_trip: Duration,
}

#[derive(Debug, Default)]
struct CalibrationWindow {
    samples: VecDeque<ClockSample>,
}

impl CalibrationWindow {
    fn push(
        &mut self,
        sample: ClockSample,
        video_started_at: Instant,
    ) -> Option<GameCalibrationV2> {
        if self
            .samples
            .back()
            .is_some_and(|previous| sample.game_tick <= previous.game_tick)
        {
            self.samples.clear();
        }
        self.samples.push_back(sample);
        while self.samples.len() > CALIBRATION_SAMPLE_COUNT {
            self.samples.pop_front();
        }
        if self.samples.len() < CALIBRATION_SAMPLE_COUNT {
            return None;
        }

        let evidence = calibration_evidence(&self.samples, video_started_at);
        if evidence.is_none() {
            self.samples.clear();
        }
        evidence
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

fn calibration_evidence(
    samples: &VecDeque<ClockSample>,
    video_started_at: Instant,
) -> Option<GameCalibrationV2> {
    if samples.len() != CALIBRATION_SAMPLE_COUNT {
        return None;
    }
    let first = samples.front()?;
    let last = samples.back()?;
    let window_span = last.midpoint.checked_duration_since(first.midpoint)?;
    if window_span < CALIBRATION_MIN_WINDOW_SPAN {
        return None;
    }

    let tolerance_ticks = duration_game_ticks(CALIBRATION_CLOCK_TOLERANCE);
    for (previous, current) in samples.iter().zip(samples.iter().skip(1)) {
        let monotonic_delta =
            duration_game_ticks(current.midpoint.checked_duration_since(previous.midpoint)?);
        let game_delta = current
            .game_tick
            .get()
            .checked_sub(previous.game_tick.get())?;
        if game_delta <= 0 || game_delta.unsigned_abs().abs_diff(monotonic_delta) > tolerance_ticks
        {
            return None;
        }
    }

    let mut candidates = samples
        .iter()
        .map(|sample| {
            let elapsed_ticks = i128::from(duration_game_ticks(
                sample.midpoint.checked_duration_since(video_started_at)?,
            ));
            let game_ticks = i128::from(sample.game_tick.get());
            let intercept = elapsed_ticks
                .checked_sub(game_ticks)?
                .checked_mul(i128::from(REPLAY_TICKS_PER_GAME_TICK))?;
            i64::try_from(intercept).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    let minimum = *candidates.iter().min()?;
    let maximum = *candidates.iter().max()?;
    if maximum.saturating_sub(minimum).unsigned_abs() / REPLAY_TICKS_PER_GAME_TICK as u64
        > tolerance_ticks
    {
        return None;
    }
    let intercept = SignedReplayTick::new(median(&mut candidates).ok()?).ok()?;
    let maximum_rtt_game_ticks = samples
        .iter()
        .map(|sample| duration_game_ticks(sample.round_trip))
        .max()?;
    let maximum_residual_game_ticks =
        maximum.saturating_sub(minimum).unsigned_abs() / REPLAY_TICKS_PER_GAME_TICK as u64;
    Some(GameCalibrationV2 {
        status: CalibrationStatus::Available,
        sample_count: u32::try_from(samples.len()).ok()?,
        first_game_tick: Some(first.game_tick),
        last_game_tick: Some(last.game_tick),
        replay_tick_at_game_zero: Some(intercept),
        maximum_rtt_game_ticks,
        maximum_residual_game_ticks,
        uncertainty_game_ticks: maximum_rtt_game_ticks / 2 + maximum_residual_game_ticks,
    })
}

fn duration_game_ticks(duration: Duration) -> u64 {
    let ticks = u128::from(duration.as_secs())
        .saturating_mul(1_000_000)
        .saturating_add(u128::from(duration.subsec_micros()));
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

fn request_midpoint(started_at: Instant, finished_at: Instant) -> Instant {
    started_at
        .checked_add(finished_at.saturating_duration_since(started_at) / 2)
        .unwrap_or(finished_at)
}

fn median(values: &mut [i64]) -> Result<i64> {
    if values.is_empty() {
        bail!("cannot calculate a median from zero samples");
    }
    values.sort_unstable();
    Ok(values[values.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_replay_time::{
        AudioTimelineV2, ContainerTimelineV2, FrameBoundary, MediaPts, ProducerEvidenceV2,
        REPLAY_TICKS_PER_SECOND, Rational, ReplayTick, VideoTimelineV2,
    };

    fn test_media_timeline(media_id: MediaId) -> MediaTimelineV2 {
        let rational = |numerator, denominator| Rational::new(numerator, denominator).unwrap();
        MediaTimelineV2 {
            schema_version: 2,
            replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
            media_id,
            video: VideoTimelineV2 {
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                time_base: rational(1, 15_360),
                first_pts: MediaPts::new(0),
                frame_rate: rational(60, 1),
                frame_count: FrameBoundary::new(120).unwrap(),
                one_past_last_pts: MediaPts::new(30_720),
                replay_end: ReplayTick::new(96_000_000).unwrap(),
                exact_cfr: true,
            },
            audio: AudioTimelineV2 {
                present: false,
                codec: None,
                sample_rate: None,
                time_base: None,
                first_pts: None,
                replay_start: None,
                replay_end: None,
            },
            container: ContainerTimelineV2 {
                start_seconds: rational(0, 1),
                duration_seconds: rational(2, 1),
            },
            producer: ProducerEvidenceV2 {
                backend: "test".to_owned(),
                expected_frame_rate: rational(60, 1),
                expected_frame_count: FrameBoundary::new(120).unwrap(),
                media_runtime_id: "test-runtime".to_owned(),
            },
            capture: None,
        }
    }

    #[test]
    fn calibration_uses_the_middle_sample() {
        let mut candidates = [142_303, 142_301, 180_000, 142_302, 142_300];
        assert_eq!(median(&mut candidates).unwrap(), 142_302);
    }

    #[test]
    fn frozen_loading_clock_is_rejected_then_advancing_clock_is_calibrated() {
        let video_started_at = Instant::now();
        let frozen = (0..CALIBRATION_SAMPLE_COUNT)
            .map(|index| ClockSample {
                midpoint: video_started_at
                    + Duration::from_millis(u64::try_from(index).unwrap() * 250),
                game_tick: GameTick::new(18_000).unwrap(),
                round_trip: Duration::from_millis(10),
            })
            .collect();
        assert_eq!(calibration_evidence(&frozen, video_started_at), None);

        let moving = (0..CALIBRATION_SAMPLE_COUNT)
            .map(|index| ClockSample {
                midpoint: video_started_at
                    + Duration::from_secs(40)
                    + Duration::from_millis(u64::try_from(index).unwrap() * 250),
                game_tick: GameTick::new(50_000 + i64::try_from(index).unwrap() * 250_000).unwrap(),
                round_trip: Duration::from_millis(10),
            })
            .collect();
        assert!(calibration_evidence(&moving, video_started_at).is_some());
    }

    #[test]
    fn calibration_window_resets_on_frozen_backward_and_outlier_samples() {
        let video_started_at = Instant::now();
        let mut window = CalibrationWindow::default();
        let sample = |elapsed_ms, game_time_seconds| ClockSample {
            midpoint: video_started_at + Duration::from_millis(elapsed_ms),
            game_tick: GameTick::from_seconds(game_time_seconds).unwrap(),
            round_trip: Duration::from_millis(10),
        };

        assert_eq!(window.push(sample(10_000, 0.018), video_started_at), None);
        assert_eq!(window.push(sample(10_250, 0.018), video_started_at), None);
        assert_eq!(window.samples.len(), 1);
        assert_eq!(window.push(sample(10_500, 0.010), video_started_at), None);
        assert_eq!(window.samples.len(), 1);

        window.clear();
        for index in 0..4 {
            assert_eq!(
                window.push(
                    sample(20_000 + index * 250, index as f64 * 0.250),
                    video_started_at,
                ),
                None
            );
        }
        assert_eq!(window.push(sample(21_000, 2.0), video_started_at), None);
        assert!(window.samples.is_empty());

        let mut calibration = None;
        for index in 0..CALIBRATION_SAMPLE_COUNT as u64 {
            calibration = window.push(
                sample(22_000 + index * 250, 3.0 + index as f64 * 0.250),
                video_started_at,
            );
        }
        assert!(calibration.is_some());
    }
    #[test]
    fn metadata_keeps_unavailable_live_client_fields_null() {
        let media_id = MediaId::new_v4();
        let metadata = RecordingMetadata::new(
            "1970-01-01T00:00:00Z".to_owned(),
            test_media_timeline(media_id.clone()),
            PollerSummary::default(),
            RecordingDetails {
                encoder_used: "nvenc".to_owned(),
                codec: "hevc".to_owned(),
                profile: "high".to_owned(),
                resolution: "1920x1080".to_owned(),
                fps: 60,
                ..RecordingDetails::default()
            },
        );
        let json = serde_json::to_value(metadata).unwrap();

        assert_eq!(json["recorded_at"], "1970-01-01T00:00:00Z");
        assert!(json["game_mode"].is_null());
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["media_id"], media_id.to_string());
        assert_eq!(json["media_timeline"]["video"]["replay_end"], "96000000");
        assert_eq!(json["recording_codec"], "hevc");
        assert_eq!(json["recording_profile"], "high");
    }
}
