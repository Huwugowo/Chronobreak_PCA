use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

use crate::encoder::AudioSource;
use crate::platform::{CaptureTarget, instant_from_qpc_100ns};

use super::{
    NativeCfrClock, NativeCfrTelemetrySnapshot, NativeMuxPlan, NativeMuxProcess,
    NativeMuxTelemetrySnapshot, NativeNv12Converter, NativeNv12TelemetrySnapshot,
    NativeNvencEncoder, NativeNvencTelemetrySnapshot, NativeWgcSource, NativeWgcTelemetrySnapshot,
};

const NATIVE_FPS: u32 = 60;
const NATIVE_WIDTH: u32 = 1920;
const NATIVE_HEIGHT: u32 = 1080;
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_SOURCE_WAIT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSessionTelemetrySnapshot {
    pub capture: NativeWgcTelemetrySnapshot,
    pub cfr: NativeCfrTelemetrySnapshot,
    pub conversion: NativeNv12TelemetrySnapshot,
    pub encode: NativeNvencTelemetrySnapshot,
    pub mux: NativeMuxTelemetrySnapshot,
    pub slot_tick_drops: u64,
    pub unstaged_tick_drops: u64,
    pub target_closed: bool,
}

/// Standalone M5 native recorder session. All WGC, D3D11 conversion and NVENC
/// submission calls remain on the constructing GPU worker. FFmpeg receives
/// only completed Annex-B bytes on its stdin and performs audio + MP4 muxing.
///
/// Field order is deliberate for abnormal Drop: NVENC closes its writer before
/// the mux child is killed, then converter/source GPU resources are released.
pub struct NativeRecorderSession {
    encoder: Option<NativeNvencEncoder>,
    mux: Option<NativeMuxProcess>,
    converter: NativeNv12Converter,
    source: Option<NativeWgcSource>,
    clock: Option<NativeCfrClock>,
    output: PathBuf,
    snapshot_ready: bool,
    pending_resize: Option<(u32, u32)>,
    slot_tick_drops: u64,
    unstaged_tick_drops: u64,
    target_closed: bool,
}

impl NativeRecorderSession {
    pub fn start(
        target: &CaptureTarget,
        ffmpeg: &Path,
        audio: &AudioSource,
        output: &Path,
    ) -> Result<Self> {
        let source = NativeWgcSource::start(target)?;
        let converter = source.create_nv12_converter(NATIVE_WIDTH, NATIVE_HEIGHT, NATIVE_FPS)?;
        let plan = NativeMuxPlan::h264(NATIVE_FPS, audio, output)?;
        let mut mux = NativeMuxProcess::start(ffmpeg, &plan)?;
        let writer = mux.take_video_writer()?;
        let encoder = NativeNvencEncoder::new(&source, &converter, writer)?;
        Ok(Self {
            encoder: Some(encoder),
            mux: Some(mux),
            converter,
            source: Some(source),
            clock: None,
            output: output.to_path_buf(),
            snapshot_ready: false,
            pending_resize: None,
            slot_tick_drops: 0,
            unstaged_tick_drops: 0,
            target_closed: false,
        })
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    /// Record exactly `duration * 60` scheduled output ticks after the first
    /// source-frame anchor. Source polling never waits beyond the next tick.
    pub fn run_for(&mut self, duration: Duration) -> Result<()> {
        ensure!(!duration.is_zero(), "native recording duration is zero");
        self.wait_for_first_source()?;
        let recording_deadline = self
            .clock
            .as_ref()
            .context("native CFR clock was not anchored")?
            .deadline_after(duration)?;

        loop {
            let next_deadline = self
                .clock
                .as_ref()
                .context("native CFR clock disappeared")?
                .next_deadline()?;
            if next_deadline >= recording_deadline || self.target_closed {
                break;
            }

            let now = Instant::now();
            if now >= next_deadline {
                let tick = self
                    .clock
                    .as_mut()
                    .context("native CFR clock disappeared")?
                    .emit_due(now)?
                    .context("due native CFR deadline did not emit a tick")?;
                self.submit_tick(tick.qpc_100ns)?;
                continue;
            }

            let timeout = next_deadline
                .saturating_duration_since(now)
                .min(MAX_SOURCE_WAIT);
            self.receive_source(timeout)?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<NativeSessionTelemetrySnapshot> {
        let capture = self
            .source
            .take()
            .context("native WGC source was already closed")?
            .close();
        let encode = self
            .encoder
            .take()
            .context("native NVENC encoder was already closed")?
            .finish();
        let mux = self
            .mux
            .take()
            .context("native mux process was already closed")?
            .finish();
        let conversion = self.converter.telemetry();

        match (capture, encode, mux) {
            (Ok(capture), Ok(encode), Ok(mux)) => {
                let cfr = self
                    .clock
                    .as_ref()
                    .context("native session finished without a CFR clock")?
                    .telemetry();
                ensure!(
                    encode.completed_frames == mux.encoded_frames,
                    "native encoder/mux frame accounting differs: {} completed vs {} muxed",
                    encode.completed_frames,
                    mux.encoded_frames
                );
                Ok(NativeSessionTelemetrySnapshot {
                    capture,
                    cfr,
                    conversion,
                    encode,
                    mux,
                    slot_tick_drops: self.slot_tick_drops,
                    unstaged_tick_drops: self.unstaged_tick_drops,
                    target_closed: self.target_closed,
                })
            }
            (capture, encode, mux) => bail!(
                "native session shutdown failed: capture={}; encode={}; mux={} (partial fragmented MP4 preserved at {})",
                format_result_error(capture),
                format_result_error(encode),
                format_result_error(mux),
                self.output.display()
            ),
        }
    }

    fn wait_for_first_source(&mut self) -> Result<()> {
        let deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        while self.clock.is_none() {
            if Instant::now() >= deadline {
                bail!(
                    "native WGC produced no first frame within {} seconds",
                    FIRST_FRAME_TIMEOUT.as_secs_f64()
                );
            }
            self.receive_source(MAX_SOURCE_WAIT)?;
            if self.target_closed {
                bail!("native WGC target closed before the first CFR frame");
            }
        }
        Ok(())
    }

    fn receive_source(&mut self, timeout: Duration) -> Result<()> {
        let source = self
            .source
            .as_ref()
            .context("native WGC source is closed")?;
        let Some(frame) = source.recv_timeout(timeout)? else {
            self.target_closed = source.telemetry().closed;
            return Ok(());
        };
        let dimensions = frame.dimensions();
        let qpc_100ns = frame.qpc_100ns();
        let pool_dimensions = source.pool_dimensions();
        if dimensions != pool_dimensions {
            frame.close()?;
            self.source
                .as_mut()
                .context("native WGC source is closed")?
                .recreate_for_content_size(dimensions.0, dimensions.1)?;
            // Keep converting the last valid GPU snapshot while the recreated
            // WGC pool produces its first new-size surface. Replacing the
            // snapshot here would create a CFR hole during the resize gap.
            self.pending_resize = Some(dimensions);
            return Ok(());
        }

        if let Some(pending_resize) = self.pending_resize {
            ensure!(
                dimensions == pending_resize,
                "native WGC recreated {:?} but delivered {:?}",
                pending_resize,
                dimensions
            );
            self.encoder
                .as_mut()
                .context("native NVENC encoder is closed")?
                .drain()?;
            self.converter
                .reconfigure_input(dimensions.0, dimensions.1)?;
            self.pending_resize = None;
        }

        if let Some(clock) = self.clock.as_mut() {
            ensure!(
                qpc_100ns > clock.telemetry().latest_source_qpc_100ns,
                "native WGC source timestamp did not advance monotonically"
            );
        }
        self.converter.stage_latest_source(&frame)?;
        frame.close()?;
        if let Some(clock) = self.clock.as_mut() {
            clock.observe_source(qpc_100ns)?;
        } else {
            let anchor = instant_from_qpc_100ns(qpc_100ns)?;
            self.clock = Some(NativeCfrClock::start(NATIVE_FPS, qpc_100ns, anchor)?);
        }
        self.snapshot_ready = true;
        Ok(())
    }

    fn submit_tick(&mut self, qpc_100ns: i64) -> Result<()> {
        if !self.snapshot_ready {
            self.unstaged_tick_drops = self.unstaged_tick_drops.saturating_add(1);
            return Ok(());
        }
        let Some(converted) = self.converter.convert_staged(qpc_100ns)? else {
            self.slot_tick_drops = self.slot_tick_drops.saturating_add(1);
            return Ok(());
        };
        self.encoder
            .as_mut()
            .context("native NVENC encoder is closed")?
            .submit(converted)
    }
}

fn format_result_error<T>(result: Result<T>) -> String {
    result.map_or_else(|error| format!("{error:#}"), |_| "none".to_owned())
}
