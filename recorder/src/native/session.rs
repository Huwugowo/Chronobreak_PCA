use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use tokio::sync::watch;

use crate::encoder::capabilities::{NVENC_SURFACE_LIMIT, WGC_FRAME_POOL_CAPACITY};
use crate::encoder::{AudioSource, RecordingEvidence};
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
const NATIVE_EVIDENCE_INTERVAL: Duration = Duration::from_millis(250);
const NATIVE_SOURCE_SNAPSHOT_CAPACITY: u32 = 1;
const MAX_CATCH_UP_SUBMISSIONS_PER_PASS: u64 = 2;
const NO_SLOT_RETRY_WAIT: Duration = Duration::from_millis(1);
const FINAL_CATCH_UP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSessionTelemetrySnapshot {
    pub capture: NativeWgcTelemetrySnapshot,
    pub cfr: NativeCfrTelemetrySnapshot,
    pub conversion: NativeNv12TelemetrySnapshot,
    pub encode: NativeNvencTelemetrySnapshot,
    pub mux: NativeMuxTelemetrySnapshot,
    pub no_slot_admission_failures: u64,
    pub unstaged_tick_admission_failures: u64,
    pub maximum_catch_up_batch: u64,
    pub injected_worker_stalls: u64,
    pub injected_worker_stall_100ns: u64,
    pub target_closed: bool,
}

#[cfg(feature = "native-failure-injection")]
#[derive(Debug, Clone, Copy)]
struct InjectedWorkerStall {
    after_ticks: u64,
    duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickAdmission {
    Submitted,
    NoSlot,
}

/// Native recorder session. All WGC, D3D11 conversion and NVENC
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
    no_slot_admission_failures: u64,
    unstaged_tick_admission_failures: u64,
    maximum_catch_up_batch: u64,
    injected_worker_stalls: u64,
    injected_worker_stall_100ns: u64,
    target_closed: bool,
    #[cfg(feature = "native-failure-injection")]
    injected_nvenc_failure_after_ticks: Option<u64>,
    #[cfg(feature = "native-failure-injection")]
    injected_worker_stall: Option<InjectedWorkerStall>,
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
            no_slot_admission_failures: 0,
            unstaged_tick_admission_failures: 0,
            maximum_catch_up_batch: 0,
            injected_worker_stalls: 0,
            injected_worker_stall_100ns: 0,
            target_closed: false,
            #[cfg(feature = "native-failure-injection")]
            injected_nvenc_failure_after_ticks: None,
            #[cfg(feature = "native-failure-injection")]
            injected_worker_stall: None,
        })
    }

    pub fn output(&self) -> &Path {
        &self.output
    }

    #[cfg(feature = "native-failure-injection")]
    pub fn inject_nvenc_failure_after_ticks(&mut self, scheduled_ticks: u64) -> Result<()> {
        ensure!(
            scheduled_ticks > 0,
            "injected NVENC failure tick must be positive"
        );
        self.injected_nvenc_failure_after_ticks = Some(scheduled_ticks);
        Ok(())
    }

    #[cfg(feature = "native-failure-injection")]
    pub fn inject_worker_stall_after_ticks(
        &mut self,
        scheduled_ticks: u64,
        duration: Duration,
    ) -> Result<()> {
        ensure!(
            scheduled_ticks > 0,
            "injected worker stall tick must be positive"
        );
        ensure!(
            !duration.is_zero() && duration <= Duration::from_secs(5),
            "injected worker stall duration must be in 1 ns..=5 s"
        );
        self.injected_worker_stall = Some(InjectedWorkerStall {
            after_ticks: scheduled_ticks,
            duration,
        });
        Ok(())
    }

    #[cfg(feature = "native-failure-injection")]
    pub fn inject_mux_writer_stall_after_writes(
        &self,
        write_index: u64,
        duration: Duration,
    ) -> Result<()> {
        self.mux
            .as_ref()
            .context("native mux was not available for writer stall injection")?
            .inject_writer_stall_after_writes(write_index, duration)
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
        let catch_up_deadline = recording_deadline
            .checked_add(FINAL_CATCH_UP_TIMEOUT)
            .context("native final catch-up deadline exceeded the monotonic clock range")?;

        loop {
            let next_deadline = self
                .clock
                .as_ref()
                .context("native CFR clock disappeared")?
                .next_deadline()?;
            if next_deadline >= recording_deadline {
                break;
            }
            if self.target_closed {
                bail!("native WGC target closed while recording");
            }

            let now = Instant::now();
            if now >= catch_up_deadline {
                bail!(
                    "native CFR could not commit every tick before the bounded final catch-up deadline (next_tick={} target_duration={:.3}s)",
                    self.clock
                        .as_ref()
                        .context("native CFR clock disappeared")?
                        .telemetry()
                        .scheduled_ticks,
                    duration.as_secs_f64()
                );
            }
            if now >= next_deadline {
                let submitted = self.submit_due_batch(Some(recording_deadline))?;
                self.receive_source(if submitted == 0 {
                    NO_SLOT_RETRY_WAIT
                } else {
                    Duration::ZERO
                })?;
                continue;
            }

            let timeout = next_deadline
                .saturating_duration_since(now)
                .min(MAX_SOURCE_WAIT);
            self.receive_source(timeout)?;
        }
        Ok(())
    }

    /// Drive the native graph on its dedicated GPU worker until the lifecycle
    /// owner requests stop. Progress publication is bounded to four updates per
    /// second so service observability does not add per-frame synchronization.
    pub(crate) fn run_until_stopped(
        &mut self,
        stop: &AtomicBool,
        evidence: &watch::Sender<RecordingEvidence>,
    ) -> Result<()> {
        let first_frame_deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
        while self.clock.is_none() {
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            if Instant::now() >= first_frame_deadline {
                bail!(
                    "native WGC produced no first frame within {} seconds",
                    FIRST_FRAME_TIMEOUT.as_secs_f64()
                );
            }
            self.receive_source(MAX_SOURCE_WAIT)?;
            self.publish_evidence(evidence)?;
            if self.target_closed {
                bail!("native WGC target closed before the first CFR frame");
            }
        }

        let mut evidence_due = Instant::now();
        let mut stop_at = None;
        let mut catch_up_deadline = None;
        loop {
            if stop_at.is_none() && stop.load(Ordering::Acquire) {
                let observed_stop = Instant::now();
                stop_at = Some(observed_stop);
                catch_up_deadline =
                    Some(observed_stop.checked_add(FINAL_CATCH_UP_TIMEOUT).context(
                        "native stop catch-up deadline exceeded the monotonic clock range",
                    )?);
            }
            if self.target_closed && stop_at.is_none() {
                bail!("native WGC target closed while recording");
            }
            let next_deadline = self
                .clock
                .as_ref()
                .context("native CFR clock disappeared")?
                .next_deadline()?;
            if stop_at.is_some_and(|deadline| next_deadline >= deadline) {
                break;
            }
            let now = Instant::now();
            if catch_up_deadline.is_some_and(|deadline| now >= deadline) {
                bail!(
                    "native CFR could not commit every tick through the requested stop time before the bounded final catch-up deadline"
                );
            }
            if now >= next_deadline {
                let submitted = self.submit_due_batch(stop_at)?;
                self.receive_source(if submitted == 0 {
                    NO_SLOT_RETRY_WAIT
                } else {
                    Duration::ZERO
                })?;
            } else {
                self.receive_source(
                    next_deadline
                        .saturating_duration_since(now)
                        .min(MAX_SOURCE_WAIT),
                )?;
            }

            let now = Instant::now();
            if now >= evidence_due {
                self.publish_evidence(evidence)?;
                evidence_due = now + NATIVE_EVIDENCE_INTERVAL;
            }
        }
        self.publish_evidence(evidence)
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
                ensure!(
                    cfr.scheduled_ticks == encode.submitted_frames
                        && encode.submitted_frames == encode.completed_frames,
                    "native transactional tick accounting differs: committed={} submitted={} completed={}",
                    cfr.scheduled_ticks,
                    encode.submitted_frames,
                    encode.completed_frames
                );
                ensure!(
                    self.unstaged_tick_admission_failures == 0,
                    "native CFR encountered {} unstaged admission failures",
                    self.unstaged_tick_admission_failures
                );
                ensure!(
                    conversion.no_free_slot_admission_failures <= self.no_slot_admission_failures,
                    "native no-slot accounting differs: converter={} session={}",
                    conversion.no_free_slot_admission_failures,
                    self.no_slot_admission_failures
                );
                ensure!(
                    self.maximum_catch_up_batch <= MAX_CATCH_UP_SUBMISSIONS_PER_PASS,
                    "native CFR catch-up batch exceeded its bound: {} > {}",
                    self.maximum_catch_up_batch,
                    MAX_CATCH_UP_SUBMISSIONS_PER_PASS
                );
                ensure!(
                    capture.pending_frame_high_water_mark <= 1,
                    "native WGC pending-frame bound exceeded: {} > 1",
                    capture.pending_frame_high_water_mark
                );
                ensure!(
                    conversion.source_snapshot_copies <= cfr.scheduled_ticks.saturating_add(1),
                    "native WGC copied {} source snapshots for {} scheduled ticks",
                    conversion.source_snapshot_copies,
                    cfr.scheduled_ticks
                );
                ensure!(
                    capture.admitted
                        == conversion
                            .source_snapshot_copies
                            .saturating_add(capture.pending_frame_replacements)
                            .saturating_add(capture.worker_frame_discards),
                    "native WGC admitted-source accounting mismatch: admitted={} copies={} pending_replacements={} worker_discards={}",
                    capture.admitted,
                    conversion.source_snapshot_copies,
                    capture.pending_frame_replacements,
                    capture.worker_frame_discards
                );
                ensure!(
                    cfr.source_discards == capture.pending_frame_replacements,
                    "native CFR discard accounting mismatch: cfr={} pending_replacements={}",
                    cfr.source_discards,
                    capture.pending_frame_replacements
                );
                Ok(NativeSessionTelemetrySnapshot {
                    capture,
                    cfr,
                    conversion,
                    encode,
                    mux,
                    no_slot_admission_failures: self.no_slot_admission_failures,
                    unstaged_tick_admission_failures: self.unstaged_tick_admission_failures,
                    maximum_catch_up_batch: self.maximum_catch_up_batch,
                    injected_worker_stalls: self.injected_worker_stalls,
                    injected_worker_stall_100ns: self.injected_worker_stall_100ns,
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
        if self.clock.is_some() {
            let source = self
                .source
                .as_mut()
                .context("native WGC source is closed")?;
            let (received, replacements) = source.receive_pending_timeout(timeout)?;
            if !received {
                self.target_closed = source.telemetry().closed;
            }
            self.clock
                .as_mut()
                .context("native CFR clock disappeared")?
                .record_source_discards(replacements);
            return Ok(());
        }

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
            let close_result = frame.close();
            self.source
                .as_mut()
                .context("native WGC source is closed")?
                .record_worker_frame_discard();
            close_result?;
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
            let texture_ready = source_texture_contains_content(&frame);
            if !matches!(&texture_ready, Ok(true)) {
                let close_result = frame.close();
                self.source
                    .as_mut()
                    .context("native WGC source is closed")?
                    .record_worker_frame_discard();
                texture_ready?;
                close_result?;
                return Ok(());
            }
            self.encoder
                .as_mut()
                .context("native NVENC encoder is closed")?
                .drain()?;
            self.converter
                .reconfigure_input(dimensions.0, dimensions.1)?;
            self.pending_resize = None;
        }

        let stage_result = self.converter.stage_latest_source(&frame);
        let close_result = frame.close();
        if !matches!(&stage_result, Ok(true)) {
            self.source
                .as_mut()
                .context("native WGC source is closed")?
                .record_worker_frame_discard();
        }
        let staged = stage_result?;
        close_result?;
        if !staged {
            return Ok(());
        }
        let anchor = instant_from_qpc_100ns(qpc_100ns)?;
        self.clock = Some(NativeCfrClock::start(NATIVE_FPS, qpc_100ns, anchor)?);
        self.snapshot_ready = true;
        Ok(())
    }

    fn stage_pending_source_for_tick(&mut self) -> Result<()> {
        if self
            .clock
            .as_ref()
            .context("native CFR clock disappeared")?
            .telemetry()
            .scheduled_ticks
            == 0
        {
            // Tick zero belongs to the first frame that established the CFR
            // anchor. Coalescing starts only after that frame is presented.
            return Ok(());
        }
        let Self {
            encoder,
            converter,
            source,
            clock,
            snapshot_ready,
            pending_resize,
            target_closed,
            ..
        } = self;
        let source = source.as_mut().context("native WGC source is closed")?;
        let replacements = source.drain_handoff_to_pending()?;
        clock
            .as_mut()
            .context("native CFR clock disappeared")?
            .record_source_discards(replacements);
        let pool_dimensions = source.pool_dimensions();
        let Some(frame) = source.take_pending() else {
            *target_closed = source.telemetry().closed;
            return Ok(());
        };
        let dimensions = frame.dimensions();
        let qpc_100ns = frame.qpc_100ns();
        if dimensions != pool_dimensions {
            let close_result = frame.close();
            source.record_worker_frame_discard();
            close_result?;
            source.recreate_for_content_size(dimensions.0, dimensions.1)?;
            // Keep converting the last valid GPU snapshot while the recreated
            // WGC pool produces its first new-size surface.
            *pending_resize = Some(dimensions);
            return Ok(());
        }

        if let Some(expected_dimensions) = *pending_resize {
            ensure!(
                dimensions == expected_dimensions,
                "native WGC recreated {:?} but delivered {:?}",
                expected_dimensions,
                dimensions
            );
            let texture_ready = source_texture_contains_content(&frame);
            if !matches!(&texture_ready, Ok(true)) {
                let close_result = frame.close();
                source.record_worker_frame_discard();
                texture_ready?;
                close_result?;
                return Ok(());
            }
            encoder
                .as_mut()
                .context("native NVENC encoder is closed")?
                .drain()?;
            converter.reconfigure_input(dimensions.0, dimensions.1)?;
            *pending_resize = None;
        }

        let clock = clock.as_mut().context("native CFR clock disappeared")?;
        ensure!(
            qpc_100ns > clock.telemetry().latest_source_qpc_100ns,
            "native WGC source timestamp did not advance monotonically"
        );
        let stage_result = converter.stage_latest_source(&frame);
        let close_result = frame.close();
        if !matches!(&stage_result, Ok(true)) {
            source.record_worker_frame_discard();
        }
        let staged = stage_result?;
        close_result?;
        if !staged {
            return Ok(());
        }
        clock.observe_source(qpc_100ns)?;
        *snapshot_ready = true;
        Ok(())
    }

    fn submit_due_batch(&mut self, stop_before: Option<Instant>) -> Result<u64> {
        self.maybe_inject_worker_stall()?;
        let mut submissions = 0_u64;
        let mut catch_up = false;

        while submissions < MAX_CATCH_UP_SUBMISSIONS_PER_PASS {
            let next_deadline = self
                .clock
                .as_ref()
                .context("native CFR clock disappeared")?
                .next_deadline()?;
            if stop_before.is_some_and(|deadline| next_deadline >= deadline)
                || Instant::now() < next_deadline
            {
                break;
            }
            if !self.converter.has_free_slot() {
                self.no_slot_admission_failures = self.no_slot_admission_failures.saturating_add(1);
                break;
            }

            self.stage_pending_source_for_tick()?;
            let now = Instant::now();
            let tick = self
                .clock
                .as_ref()
                .context("native CFR clock disappeared")?
                .peek_due(now)?
                .context("due native CFR deadline did not expose a tick")?;
            match self.submit_tick(tick.qpc_100ns)? {
                TickAdmission::Submitted => {
                    let committed_at = Instant::now();
                    self.clock
                        .as_mut()
                        .context("native CFR clock disappeared")?
                        .commit(tick, committed_at)?;
                    submissions = submissions.saturating_add(1);
                    let another_deadline = self
                        .clock
                        .as_ref()
                        .context("native CFR clock disappeared")?
                        .next_deadline()?;
                    let another_target_tick =
                        stop_before.is_none_or(|deadline| another_deadline < deadline);
                    if another_target_tick && another_deadline <= Instant::now() {
                        catch_up = true;
                    }
                }
                TickAdmission::NoSlot => break,
            }
        }

        ensure!(
            submissions <= MAX_CATCH_UP_SUBMISSIONS_PER_PASS,
            "native CFR exceeded the bounded catch-up batch"
        );
        if catch_up {
            self.maximum_catch_up_batch = self.maximum_catch_up_batch.max(submissions);
        }
        Ok(submissions)
    }

    fn submit_tick(&mut self, qpc_100ns: i64) -> Result<TickAdmission> {
        #[cfg(feature = "native-failure-injection")]
        if self
            .injected_nvenc_failure_after_ticks
            .is_some_and(|threshold| {
                self.clock
                    .as_ref()
                    .is_some_and(|clock| clock.telemetry().scheduled_ticks >= threshold)
            })
        {
            self.injected_nvenc_failure_after_ticks = None;
            self.encoder
                .as_mut()
                .context("native NVENC encoder is closed")?
                .inject_terminal_failure_for_fixture()?;
            unreachable!("native NVENC failure fixture unexpectedly returned success");
        }
        if !self.snapshot_ready {
            self.unstaged_tick_admission_failures =
                self.unstaged_tick_admission_failures.saturating_add(1);
            bail!("native CFR tick reached admission without a staged source snapshot");
        }
        let Some(converted) = self.converter.convert_staged(qpc_100ns)? else {
            self.no_slot_admission_failures = self.no_slot_admission_failures.saturating_add(1);
            return Ok(TickAdmission::NoSlot);
        };
        self.encoder
            .as_mut()
            .context("native NVENC encoder is closed")?
            .submit(converted)?;
        Ok(TickAdmission::Submitted)
    }

    #[cfg(feature = "native-failure-injection")]
    fn maybe_inject_worker_stall(&mut self) -> Result<()> {
        let scheduled_ticks = self
            .clock
            .as_ref()
            .context("native CFR clock disappeared")?
            .telemetry()
            .scheduled_ticks;
        let Some(injection) = self
            .injected_worker_stall
            .filter(|injection| scheduled_ticks >= injection.after_ticks)
        else {
            return Ok(());
        };
        self.injected_worker_stall = None;
        std::thread::sleep(injection.duration);
        self.injected_worker_stalls = self.injected_worker_stalls.saturating_add(1);
        self.injected_worker_stall_100ns = self
            .injected_worker_stall_100ns
            .saturating_add(duration_100ns(injection.duration));
        Ok(())
    }

    #[cfg(not(feature = "native-failure-injection"))]
    fn maybe_inject_worker_stall(&mut self) -> Result<()> {
        Ok(())
    }

    fn publish_evidence(&self, sender: &watch::Sender<RecordingEvidence>) -> Result<()> {
        sender.send_replace(self.recording_evidence(false)?);
        Ok(())
    }

    fn recording_evidence(&self, terminal: bool) -> Result<RecordingEvidence> {
        let capture = self
            .source
            .as_ref()
            .context("native WGC source is closed")?
            .telemetry();
        let encode = self
            .encoder
            .as_ref()
            .context("native NVENC encoder is closed")?
            .telemetry();
        let mux = self
            .mux
            .as_ref()
            .context("native mux process is closed")?
            .telemetry();
        let cfr = self.clock.as_ref().map(NativeCfrClock::telemetry);
        let protocol_error = native_protocol_error(capture, encode, mux);
        Ok(RecordingEvidence {
            capture_ready: true,
            capture_terminal: terminal,
            frame_pool_capacity: Some(WGC_FRAME_POOL_CAPACITY),
            output_pool_capacity: Some(NATIVE_SOURCE_SNAPSHOT_CAPACITY),
            source_frames_surfaced: capture.admitted,
            source_frames_superseded: capture
                .handoff_drops
                .saturating_add(capture.pending_frame_replacements),
            pool_recreations: capture.recreations,
            first_qpc: capture.first_accepted_qpc_100ns,
            latest_qpc: capture.latest_accepted_qpc_100ns,
            encoded_frames: encode.completed_frames,
            muxed_bytes: mux.muxed_bytes.max(mux.output_file_bytes),
            output_time_us: mux.output_time_us,
            cfr_duplicates: cfr.map_or(0, |snapshot| snapshot.duplicate_ticks),
            cfr_discards: cfr.map_or(0, |snapshot| snapshot.source_discards),
            progress_end: terminal && mux.progress_end,
            protocol_error,
        })
    }
}

fn source_texture_contains_content(frame: &super::CapturedWgcFrame<'_>) -> Result<bool> {
    let desc = frame.texture_desc()?;
    let (width, height) = frame.dimensions();
    Ok(desc.Width >= width && desc.Height >= height)
}

#[cfg(feature = "native-failure-injection")]
fn duration_100ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos() / 100).unwrap_or(u64::MAX)
}

impl NativeSessionTelemetrySnapshot {
    pub(crate) fn recording_evidence(self) -> RecordingEvidence {
        RecordingEvidence {
            capture_ready: true,
            capture_terminal: true,
            frame_pool_capacity: Some(WGC_FRAME_POOL_CAPACITY),
            output_pool_capacity: Some(NATIVE_SOURCE_SNAPSHOT_CAPACITY),
            source_frames_surfaced: self.capture.admitted,
            source_frames_superseded: self
                .capture
                .handoff_drops
                .saturating_add(self.capture.pending_frame_replacements),
            pool_recreations: self.capture.recreations,
            first_qpc: self.capture.first_accepted_qpc_100ns,
            latest_qpc: self.capture.latest_accepted_qpc_100ns,
            encoded_frames: self.encode.completed_frames,
            muxed_bytes: self.mux.muxed_bytes.max(self.mux.output_file_bytes),
            output_time_us: self.mux.output_time_us,
            cfr_duplicates: self.cfr.duplicate_ticks,
            cfr_discards: self.cfr.source_discards,
            progress_end: self.mux.progress_end,
            protocol_error: native_protocol_error(self.capture, self.encode, self.mux),
        }
    }
}

fn native_protocol_error(
    capture: NativeWgcTelemetrySnapshot,
    encode: NativeNvencTelemetrySnapshot,
    mux: NativeMuxTelemetrySnapshot,
) -> Option<String> {
    if let Some(error) = capture.first_callback_error {
        return Some(format!("native WGC callback failed: {error:?}"));
    }
    if encode.completion_errors > 0 || encode.submission_queue_failures > 0 {
        return Some(format!(
            "native NVENC reported {} completion and {} submission-queue errors",
            encode.completion_errors, encode.submission_queue_failures
        ));
    }
    if mux.reader_errors > 0 {
        return Some(format!(
            "native mux evidence readers reported {} errors",
            mux.reader_errors
        ));
    }
    if encode.max_in_flight > u64::from(NVENC_SURFACE_LIMIT) {
        return Some(format!(
            "native NVENC exceeded the fixed surface bound: {} > {}",
            encode.max_in_flight, NVENC_SURFACE_LIMIT
        ));
    }
    if capture.pending_frame_high_water_mark > 1 {
        return Some(format!(
            "native WGC exceeded the worker pending-frame bound: {} > 1",
            capture.pending_frame_high_water_mark
        ));
    }
    None
}

fn format_result_error<T>(result: Result<T>) -> String {
    result.map_or_else(|error| format!("{error:#}"), |_| "none".to_owned())
}
