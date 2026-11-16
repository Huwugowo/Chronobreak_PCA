//! Opt-in generated packets for measuring the real PCM pump and mux transport.

use std::path::Path;

use super::*;
use crate::encoder::AudioSource;
use crate::native::{NativeMuxPlan, NativeMuxProcess};

/// Generated 48-kHz packets replace endpoint acquisition only. The production
/// packet admission, PCM assembly, named pipe, flow control and cleanup run intact.
pub struct MuxResourceAudioFixture(NativeAudio);

impl MuxResourceAudioFixture {
    /// Starts generated audio and the production A/V mux on one fixture epoch.
    pub fn start(
        ffmpeg: &Path,
        output: &Path,
        anchor: Instant,
    ) -> Result<(Self, NativeMuxProcess, NativeMuxPlan)> {
        const EPOCH: i64 = 1_000_000_000;
        let shared = Arc::new(Shared::new(Arc::new(AudioFlow::new()?), false)?);
        shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .available = true;
        let owner = Arc::clone(&shared);
        let capture = thread::Builder::new()
            .name("mux-fixture-packets".into())
            .spawn(move || {
                let mut samples = Vec::with_capacity(480 * 8);
                for index in 0..480 {
                    let value = (index as f32 * std::f32::consts::TAU / 48.0).sin() * 0.1;
                    samples.extend_from_slice(&value.to_le_bytes());
                    samples.extend_from_slice(&value.to_le_bytes());
                }
                let mut position = 0_u64;
                while !owner.cancelled.load(Ordering::Acquire) {
                    let target = anchor + Duration::from_nanos(position * 1_000_000_000 / RATE);
                    if let Some(wait) = target.checked_duration_since(Instant::now()) {
                        thread::sleep(wait.min(Duration::from_millis(10)));
                        continue;
                    }
                    if let Err(error) = capture::accept(
                        &owner,
                        samples.as_mut_ptr(),
                        480,
                        0,
                        position,
                        EPOCH as u64 + position * 10_000_000 / RATE,
                        1,
                    ) {
                        owner.error(error);
                        break;
                    }
                    position += 480;
                }
                owner.capture_unavailable();
                owner.owner_returned();
            })?;
        let mut audio = NativeAudio {
            shared,
            capture: Some(capture),
            pump: None,
        };
        let endpoint = audio.begin_transport(EPOCH, anchor, anchor + Duration::from_secs(15))?;
        let plan = NativeMuxPlan::h264(60, &AudioSource::NativePcm(endpoint), output)?;
        let mux = NativeMuxProcess::start_with_audio_flow(ffmpeg, &plan, audio.flow())?;
        Ok((Self(audio), mux, plan))
    }

    /// Waits for the production bootstrap, with the ordinary startup deadline.
    pub fn await_ready(&self, deadline: Instant) -> bool {
        self.0.await_handshake(deadline, &AtomicBool::new(false))
    }

    /// Freezes the same video-authoritative boundary used by the native recorder.
    pub fn stop(&self, video_frames: u64) {
        self.0.stop(video_frames);
    }

    /// Joins production audio cleanup and returns its accounted summary.
    pub fn finish(&mut self) -> AudioCaptureSummaryV1 {
        self.0.finish_summary()
    }
}
