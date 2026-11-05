//! Native system playback acquisition and independent bounded PCM delivery.
mod capture;
pub(crate) mod flow;
mod pipe;

use std::collections::VecDeque;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};

use flow::{AudioFlow, Event, LeadEscape};
use pipe::PcmPipe;

const RATE: u64 = 48_000;
const BLOCK_BYTES: usize = 32768;
const LEAD: u64 = 24_000;
const DELIVERY: Duration = Duration::from_millis(250);
static AUDIO_DISABLED: AtomicBool = AtomicBool::new(false);
static AUDIO_IN_USE: AtomicBool = AtomicBool::new(false);

pub use crate::poller::AudioCaptureSummaryV1;

struct Packet {
    bytes: Vec<u8>,
    qpc: u64,
    device: u64,
    frames: u32,
    silent: bool,
    discontinuity: bool,
    generation: u64,
    offset: usize,
    slot: Option<i64>,
}

struct State {
    queue: VecDeque<Packet>,
    pool: Vec<Vec<u8>>,
    queued_frames: u64,
    epoch: Option<(i64, Instant)>,
    stop: Option<(u64, Instant)>,
    available: bool,
    handshake: Option<bool>,
    activations: u64,
    overwritten_end_qpc: Option<u64>,
    startup_loss_until: u64,
    summary: AudioCaptureSummaryV1,
}

struct Shared {
    state: Mutex<State>,
    flow: Arc<AudioFlow>,
    capture_stop: Event,
    handshake_ready: Event,
    cancelled: AtomicBool,
    reserved: bool,
    retained: AtomicBool,
    #[cfg(feature = "native-failure-injection")]
    inject_bad_timestamp: AtomicBool,
}

impl Drop for Shared {
    fn drop(&mut self) {
        if self.reserved {
            AUDIO_IN_USE.store(false, Ordering::Release);
        }
    }
}

impl Shared {
    fn new(flow: Arc<AudioFlow>, reserved: bool) -> Result<Self> {
        Ok(Self {
            state: Mutex::new(State {
                queue: VecDeque::with_capacity(64),
                pool: (0..64).map(|_| Vec::with_capacity(BLOCK_BYTES)).collect(),
                queued_frames: 0,
                epoch: None,
                stop: None,
                available: false,
                handshake: None,
                activations: 0,
                overwritten_end_qpc: None,
                startup_loss_until: 0,
                summary: AudioCaptureSummaryV1::default(),
            }),
            flow,
            capture_stop: Event::new(true)?,
            handshake_ready: Event::new(true)?,
            cancelled: AtomicBool::new(false),
            reserved,
            retained: AtomicBool::new(false),
            #[cfg(feature = "native-failure-injection")]
            inject_bad_timestamp: AtomicBool::new(false),
        })
    }
    fn error(&self, error: impl std::fmt::Display) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.summary.last_error.is_none() {
            tracing::warn!(error=%error, "system audio degraded; video continues");
        }
        state.summary.last_error = Some(error.to_string().chars().take(512).collect());
        state.summary.outcome = "degraded".into();
        drop(state);
        self.flow.wake.set();
    }

    fn capture_unavailable(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Keep the last endpoint state during the intentional final drain.
        if state.stop.is_none() {
            state.available = false;
        }
    }

    fn owner_returned(&self) {
        if self.retained.load(Ordering::Acquire)
            && self
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .stop
                .is_some_and(|(_, deadline)| Instant::now() >= deadline)
        {
            AUDIO_DISABLED.store(true, Ordering::Release);
        }
    }
}

/// Thread-affine capture is independent of the GPU worker and PCM pipe owner.
pub(crate) struct NativeAudio {
    shared: Arc<Shared>,
    capture: Option<JoinHandle<()>>,
    pump: Option<JoinHandle<()>>,
}

impl NativeAudio {
    pub(crate) fn start() -> Result<Self> {
        if AUDIO_DISABLED.load(Ordering::Acquire) {
            bail!("audio owner cleanup is pending; relaunch to re-enable audio");
        }
        // A detached Windows owner retains Shared, including its reservation.
        // Cleanup may finish later without ever retaining video/GPU resources.
        if AUDIO_IN_USE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            bail!("previous audio owner is still closing");
        }
        let flow = match AudioFlow::new() {
            Ok(flow) => flow,
            Err(e) => {
                AUDIO_IN_USE.store(false, Ordering::Release);
                return Err(e);
            }
        };
        let shared = match Shared::new(Arc::new(flow), true) {
            Ok(shared) => Arc::new(shared),
            Err(e) => {
                AUDIO_IN_USE.store(false, Ordering::Release);
                return Err(e);
            }
        };
        let owner = Arc::clone(&shared);
        let capture = thread::Builder::new()
            .name("chronobreak-wasapi".into())
            .spawn(move || {
                if let Err(e) = capture::run(&owner) {
                    owner.error(e);
                }
                owner.capture_unavailable();
                owner.owner_returned();
            })?;
        Ok(Self {
            shared,
            capture: Some(capture),
            pump: None,
        })
    }

    pub(crate) fn flow(&self) -> Arc<AudioFlow> {
        Arc::clone(&self.shared.flow)
    }

    #[cfg(feature = "native-failure-injection")]
    pub(crate) fn inject_timestamp_failure(&self) {
        self.shared
            .inject_bad_timestamp
            .store(true, Ordering::Release);
    }

    pub(crate) fn begin_transport(
        &mut self,
        epoch: i64,
        anchor: Instant,
        startup: Instant,
    ) -> Result<String> {
        let pipe = PcmPipe::create()?;
        let name = pipe.name.clone();
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            align_pre_roll(&mut state, epoch, anchor)?;
        }
        let shared = Arc::clone(&self.shared);
        self.pump = Some(
            thread::Builder::new()
                .name("chronobreak-pcm".into())
                .spawn(move || {
                    if let Err(e) = pump(&shared, pipe, startup) {
                        shared.error(e);
                        shared
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .handshake
                            .get_or_insert(false);
                        shared.handshake_ready.set();
                    }
                    shared.owner_returned();
                })?,
        );
        Ok(name)
    }

    pub(crate) fn await_handshake(&self, deadline: Instant, stop: &AtomicBool) -> bool {
        loop {
            if let Some(ready) = self
                .shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .handshake
            {
                return ready;
            }
            if Instant::now() >= deadline || stop.load(Ordering::Acquire) {
                if !stop.load(Ordering::Acquire) {
                    self.shared
                        .error("PCM bootstrap did not complete before its startup deadline");
                }
                return false;
            }
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50));
            // SAFETY: handshake event is retained by this owner throughout the wait.
            unsafe {
                WaitForSingleObject(
                    self.shared.handshake_ready.handle(),
                    wait.as_millis().saturating_add(1) as u32,
                );
            }
        }
    }

    pub(crate) fn stop(&self, video_frames: u64) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stop.get_or_insert((
            video_frames.saturating_mul(800),
            Instant::now() + Duration::from_millis(500),
        ));
        drop(state);
        self.shared.cancelled.store(true, Ordering::Release);
        self.shared.capture_stop.set();
        self.shared.flow.wake.set();
    }

    pub(crate) fn summary(&self) -> AudioCaptureSummaryV1 {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .summary
            .clone()
    }

    /// Called after video/encoder/mux drain; capture cleanup is never its gate.
    pub(crate) fn finish_summary(&mut self) -> AudioCaptureSummaryV1 {
        let deadline = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop
            .map(|(_, deadline)| deadline)
            .unwrap_or_else(Instant::now);
        for owner in [&mut self.capture, &mut self.pump] {
            if let Some(thread) = owner.as_ref() {
                let wait = deadline.saturating_duration_since(Instant::now());
                // SAFETY: JoinHandle owns the live native thread handle through this bounded wait.
                unsafe {
                    WaitForSingleObject(
                        windows::Win32::Foundation::HANDLE(thread.as_raw_handle()),
                        wait.as_millis().min(u128::from(u32::MAX - 1)) as u32,
                    );
                }
            }
            if owner.as_ref().is_some_and(JoinHandle::is_finished) {
                if owner.take().unwrap().join().is_err() {
                    self.shared.error("audio owner panicked during cleanup");
                }
            } else if owner.is_some() {
                AUDIO_DISABLED.store(true, Ordering::Release);
                self.shared
                    .error("audio owner cleanup deadline; audio disabled until relaunch");
            }
        }
        self.summary()
    }
}

impl Drop for NativeAudio {
    fn drop(&mut self) {
        self.stop(0);
        // Do not join audio-local Windows calls on the GPU/finalization path.
        // Finished owners are reaped; outstanding owners retain their own state.
        for owner in [&mut self.capture, &mut self.pump] {
            if owner.as_ref().is_some_and(JoinHandle::is_finished) {
                let _ = owner.take().map(JoinHandle::join);
            } else if owner.is_some() {
                self.shared.retained.store(true, Ordering::Release);
                if self
                    .shared
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .stop
                    .is_some_and(|(_, deadline)| Instant::now() >= deadline)
                {
                    AUDIO_DISABLED.store(true, Ordering::Release);
                }
            }
        }
    }
}

fn sample_slot(packet: u64, epoch: i64) -> Result<i64> {
    let numerator = (i128::from(packet) - i128::from(epoch))
        .checked_mul(i128::from(RATE))
        .context("audio timestamp overflow")?;
    let quotient = numerator.div_euclid(10_000_000);
    let rem = numerator.rem_euclid(10_000_000);
    i64::try_from(quotient + i128::from(rem > 5_000_000 || (rem == 5_000_000 && quotient % 2 != 0)))
        .context("audio sample range")
}

fn align_pre_roll(state: &mut State, epoch: i64, anchor: Instant) -> Result<()> {
    state.epoch = Some((epoch, anchor));
    if let Some(end) = state.overwritten_end_qpc {
        state.startup_loss_until = sample_slot(end, epoch)?.max(0) as u64;
        if state.startup_loss_until > 0 {
            state.summary.outcome = "degraded".into();
        }
    }
    // Drop ordinary negative pre-roll first. Ownership/offsets, never a
    // PCM clone, establish the post-epoch 500-ms queue bound immediately.
    while let Some(mut packet) = state.queue.pop_front() {
        let slot = sample_slot(packet.qpc, epoch)?;
        let trim = slot.min(0).unsigned_abs().min(u64::from(packet.frames));
        state.queued_frames -= trim;
        state.summary.pre_epoch_trimmed_frames += trim;
        packet.frames -= trim as u32;
        packet.device += trim;
        packet.offset += trim as usize;
        if packet.frames == 0 {
            packet.bytes.clear();
            state.pool.push(packet.bytes);
            continue;
        }
        if trim > 0 {
            packet.slot = Some(slot + trim as i64);
        }
        state.queue.push_front(packet);
        break;
    }
    while state.queued_frames > LEAD {
        let mut old = state.queue.pop_front().context("audio queue accounting")?;
        state.queued_frames -= u64::from(old.frames);
        state.summary.discarded_frames += u64::from(old.frames);
        state.startup_loss_until = state.startup_loss_until.max(
            (old.slot.unwrap_or(sample_slot(old.qpc, epoch)?) + i64::from(old.frames)).max(0)
                as u64,
        );
        old.bytes.clear();
        state.pool.push(old.bytes);
        state.summary.outcome = "degraded".into();
    }
    Ok(())
}

fn pump(shared: &Shared, pipe: PcmPipe, startup: Instant) -> Result<()> {
    let startup = startup.min(Instant::now() + Duration::from_secs(1));
    pipe.connect(shared.flow.wake.handle(), || {
        shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop
            .map_or(startup, |(_, deadline)| deadline.min(startup))
    })?;
    let (epoch, anchor) = shared
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .epoch
        .context("PCM epoch missing")?;
    let mut output = Vec::with_capacity(BLOCK_BYTES);
    let mut cursor = 0_u64;
    let mut escape = LeadEscape::default();
    let mut run: Option<(u64, u64, i64)> = None;
    loop {
        let now = Instant::now();
        let flow = shared.flow.snapshot();
        let (stop, ready) = {
            let s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            (s.stop, s.handshake == Some(true))
        };
        let cap = flow
            .frames
            .checked_mul(800)
            .and_then(|n| n.checked_add(LEAD))
            .context("audio lead overflow")?
            .max(LEAD);
        let due = if let Some((end, _)) = stop {
            end
        } else {
            u64::try_from(
                now.saturating_duration_since(anchor)
                    .saturating_sub(DELIVERY)
                    .as_nanos()
                    * u128::from(RATE)
                    / 1_000_000_000,
            )
            .context("audio due overflow")?
        };
        if stop.is_some_and(|(end, deadline)| cursor >= end || now >= deadline) {
            break;
        }
        if !ready && now >= startup {
            bail!("PCM initial prefix deadline expired");
        }
        let block = if ready { 2400 } else { 4096 };
        let target = (cursor + block).min(cap);
        let end = due.min(target);
        let capped = due > cursor && cap <= cursor;
        if escape
            .deadline(capped, flow, now)
            .is_some_and(|deadline| now >= deadline)
        {
            shared.error("audio_lead_stall_eof");
            break;
        }
        let enough = end > cursor
            && (end == target && (ready || end - cursor == 4096) || stop.is_some() && ready);
        if !enough {
            let due_time = anchor
                + DELIVERY
                + Duration::from_nanos(
                    (u128::from(target) * 1_000_000_000 / u128::from(RATE)) as u64,
                );
            let deadline = if capped {
                escape.deadline(true, flow, now)
            } else {
                Some(due_time)
            };
            let deadline = deadline
                .into_iter()
                .chain(stop.map(|(_, d)| d))
                .chain((!ready).then_some(startup))
                .min();
            if let Some(deadline) = deadline {
                shared.flow.wait(deadline.saturating_duration_since(now));
            } else {
                // SAFETY: shared flow retains its wake event for this wait.
                unsafe {
                    WaitForSingleObject(shared.flow.wake.handle(), INFINITE);
                }
            }
            continue;
        }
        output.clear();
        let (valid_frames, non_silent_frames) =
            assemble(shared, epoch, cursor, end, &mut output, &mut run)?;
        // Reserve the full write before dispatch; only this owner schedules PCM.
        {
            let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            s.summary.maximum_lead_frames = s
                .summary
                .maximum_lead_frames
                .max(end.saturating_sub(flow.frames.saturating_mul(800)));
        }
        let ordinary = if ready {
            now + Duration::from_secs(1)
        } else {
            startup
        };
        let written = pipe.write(&output, shared.flow.wake.handle(), || {
            shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .stop
                .map_or(ordinary, |(_, d)| d.min(ordinary))
        });
        if let Err(error) = written {
            let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((final_end, _)) = s.stop {
                s.summary.tail_overhang_frames = end.saturating_sub(final_end);
            }
            return Err(error);
        }
        cursor = end;
        let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        s.summary.written_frames = cursor;
        s.summary.valid_written_frames += valid_frames;
        s.summary.non_silent_written_frames += non_silent_frames;
        if s.summary.outcome != "degraded" {
            let outcome = if s.summary.non_silent_written_frames > 0 {
                "captured"
            } else if s.summary.observed_frames > 0 || s.summary.inferred_idle_frames > 0 {
                "idle_uncertain"
            } else {
                "unavailable"
            };
            if s.summary.outcome != outcome {
                s.summary.outcome = outcome.into();
            }
        }
        let output_kind = if s.summary.valid_written_frames > 0 {
            "captured_aac"
        } else {
            "fallback_only_aac"
        };
        if s.summary.output != output_kind {
            s.summary.output = output_kind.into();
        }
        s.handshake = Some(true);
        drop(s);
        shared.handshake_ready.set();
    }
    let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((end, _)) = s.stop {
        s.summary.tail_overhang_frames = cursor.saturating_sub(end);
    }
    Ok(())
}

fn assemble(
    shared: &Shared,
    epoch: i64,
    start: u64,
    end: u64,
    output: &mut Vec<u8>,
    run: &mut Option<(u64, u64, i64)>,
) -> Result<(u64, u64)> {
    let mut cursor = start;
    let mut valid_frames = 0;
    let mut non_silent_frames = 0;
    let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    while cursor < end {
        let Some(mut packet) = s.queue.pop_front() else {
            let missing = end - cursor;
            count_fill(&mut s, cursor, missing);
            output.resize(output.len() + missing as usize * 8, 0);
            break;
        };
        s.queued_frames = s.queued_frames.saturating_sub(u64::from(packet.frames));
        let qpc_slot = packet.slot.unwrap_or(sample_slot(packet.qpc, epoch)?);
        if run.is_some_and(|(generation, device_end, _)| {
            generation == packet.generation && device_end != packet.device
        }) && !packet.discontinuity
        {
            s.summary.discontinuities += 1;
            s.summary.outcome = "degraded".into();
        }
        let slot = match *run {
            Some((generation, device_end, slot_end))
                if !packet.discontinuity
                    && generation == packet.generation
                    && device_end == packet.device
                    && slot_end >= start as i64 =>
            {
                slot_end
            }
            _ => qpc_slot,
        };
        let residual = qpc_slot.saturating_sub(slot);
        s.summary.last_residual_frames = residual;
        s.summary.maximum_residual_frames = s
            .summary
            .maximum_residual_frames
            .max(residual.unsigned_abs());
        let packet_end = slot.saturating_add(i64::from(packet.frames));
        let negative = slot.min(0).unsigned_abs().min(u64::from(packet.frames));
        s.summary.pre_epoch_trimmed_frames += negative;
        let lost = (cursor as i64).saturating_sub(slot.max(0)).max(0) as u64;
        let lost = lost.min(u64::from(packet.frames).saturating_sub(negative));
        if lost > 0 {
            s.summary.late_valid_frames += lost;
            if !packet.silent {
                s.summary.late_non_silent_frames += lost;
            }
            s.summary.late_packets += 1;
            s.summary.maximum_lateness_frames = s
                .summary
                .maximum_lateness_frames
                .max((cursor as i64).saturating_sub(slot).max(0) as u64);
            s.summary.outcome = "degraded".into();
        }
        if packet_end <= cursor as i64 {
            packet.bytes.clear();
            s.pool.push(packet.bytes);
            continue;
        }
        if slot > cursor as i64 {
            let gap = (slot as u64).min(end) - cursor;
            count_fill(&mut s, cursor, gap);
            output.resize(output.len() + gap as usize * 8, 0);
            cursor += gap;
        }
        let offset = (cursor as i64).saturating_sub(slot).max(0) as usize;
        let count = ((packet_end.max(0) as u64).min(end).saturating_sub(cursor)) as usize;
        if count > 0 {
            valid_frames += count as u64;
            if !packet.silent {
                non_silent_frames += count as u64;
            }
            if packet.silent {
                output.resize(output.len() + count * 8, 0);
            } else {
                output.extend_from_slice(
                    &packet.bytes
                        [(packet.offset + offset) * 8..(packet.offset + offset + count) * 8],
                );
            }
            cursor += count as u64;
        }
        if packet_end > cursor as i64 {
            // Preserve borrowed slice offsets by advancing the small packet view.
            // This rare split currently retains its block; no sample-vector clone.
            let consumed = offset + count;
            packet.slot = Some(slot.saturating_add(consumed as i64));
            packet.device += consumed as u64;
            packet.frames -= consumed as u32;
            packet.offset += consumed;
            s.queued_frames += u64::from(packet.frames);
            s.queue.push_front(packet);
            *run = None;
            break;
        }
        *run = Some((
            packet.generation,
            packet.device + u64::from(packet.frames),
            packet_end,
        ));
        packet.bytes.clear();
        s.pool.push(packet.bytes);
    }
    Ok((valid_frames, non_silent_frames))
}

fn count_fill(state: &mut State, cursor: u64, frames: u64) {
    let startup_loss = frames.min(state.startup_loss_until.saturating_sub(cursor));
    state.summary.startup_missing_frames += startup_loss;
    let other = frames - startup_loss;
    if !state.available {
        state.summary.unavailable_fill_frames += other;
    } else {
        state.summary.inferred_idle_frames += other;
    }
}

#[cfg(test)]
pub(crate) fn pump_without_endpoint(
    flow: Arc<AudioFlow>,
) -> (
    String,
    JoinHandle<Result<AudioCaptureSummaryV1>>,
    impl FnOnce(),
) {
    let shared = Arc::new(Shared::new(flow, false).expect("test audio state"));
    shared.state.lock().unwrap().epoch = Some((1, Instant::now()));
    let pipe = PcmPipe::create().expect("real PCM pipe");
    let name = pipe.name.clone();
    let cancelled = Arc::clone(&shared);
    let thread = thread::spawn(move || {
        pump(&shared, pipe, Instant::now() + Duration::from_secs(1))?;
        Ok(shared.state.lock().unwrap().summary.clone())
    });
    (name, thread, move || {
        cancelled.state.lock().unwrap().stop = Some((0, Instant::now()));
        cancelled.flow.wake.set();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn state() -> Shared {
        Shared::new(Arc::new(AudioFlow::new().unwrap()), false).unwrap()
    }
    fn packet(state: &Shared, slot: i64, device: u64, frames: u32, silent: bool) {
        let mut s = state.state.lock().unwrap();
        let mut bytes = s.pool.pop().unwrap();
        bytes.resize(frames as usize * 8, 42);
        s.queue.push_back(Packet {
            bytes,
            qpc: 1_000_000_u64.saturating_add_signed(slot * 10_000_000 / 48_000),
            device,
            frames,
            silent,
            discontinuity: false,
            generation: 1,
            offset: 0,
            slot: Some(slot),
        });
        s.queued_frames += u64::from(frames);
        s.available = true;
    }
    #[test]
    fn qpc_alignment_preserves_signed_rounding_and_contiguous_sample_sequence() {
        assert_eq!(sample_slot(1_000_000 - 208, 1_000_000).unwrap(), -1);
        assert_eq!(sample_slot(1_000_000 + 208, 1_000_000).unwrap(), 1);
        let s = state();
        packet(&s, -10, 0, 490, false);
        packet(&s, 500, 490, 480, true);
        let mut bytes = Vec::new();
        let mut run = None;
        assert_eq!(
            assemble(&s, 1_000_000, 0, 960, &mut bytes, &mut run).unwrap(),
            (960, 480)
        );
        assert_eq!(bytes.len(), 960 * 8);
        let state = s.state.lock().unwrap();
        assert_eq!(state.summary.pre_epoch_trimmed_frames, 10);
        assert_eq!(state.summary.late_valid_frames, 0);
        assert_eq!(state.summary.maximum_residual_frames, 20);
    }
    #[test]
    fn late_valid_audio_and_device_jumps_are_accountable_after_fill() {
        let s = state();
        let mut bytes = Vec::new();
        let mut run = None;
        assemble(&s, 1_000_000, 0, 480, &mut bytes, &mut run).unwrap();
        packet(&s, 0, 0, 600, false);
        bytes.clear();
        assert_eq!(
            assemble(&s, 1_000_000, 480, 600, &mut bytes, &mut run).unwrap(),
            (120, 120)
        );
        packet(&s, 600, 700, 120, false);
        bytes.clear();
        assemble(&s, 1_000_000, 600, 720, &mut bytes, &mut run).unwrap();
        let state = s.state.lock().unwrap();
        assert_eq!(state.summary.unavailable_fill_frames, 480);
        assert_eq!(state.summary.late_valid_frames, 480);
        assert_eq!(state.summary.late_non_silent_frames, 480);
        assert_eq!(state.summary.late_packets, 1);
        assert_eq!(state.summary.discontinuities, 1);
    }

    #[test]
    fn quiet_activated_capture_is_inferred_idle_even_during_final_drain() {
        let shared = state();
        shared.state.lock().unwrap().available = true;
        let mut bytes = Vec::new();
        let mut run = None;
        assert_eq!(
            assemble(&shared, 1_000_000, 0, 4096, &mut bytes, &mut run).unwrap(),
            (0, 0)
        );
        shared.state.lock().unwrap().stop = Some((4800, Instant::now()));
        shared.capture_unavailable();
        assemble(&shared, 1_000_000, 4096, 4800, &mut bytes, &mut run).unwrap();
        assert_eq!(bytes, vec![0; 4800 * 8]);
        let s = shared.state.lock().unwrap();
        assert_eq!(s.summary.inferred_idle_frames, 4800);
        assert_eq!(s.summary.startup_missing_frames, 0);
        assert_eq!(s.summary.unavailable_fill_frames, 0);
    }
    #[test]
    fn silent_packet_never_dereferences_samples_and_split_keeps_ownership() {
        let s = state();
        packet(&s, 0, 0, 480, true);
        s.state
            .lock()
            .unwrap()
            .queue
            .front_mut()
            .unwrap()
            .bytes
            .clear();
        let mut bytes = Vec::new();
        let mut run = None;
        assemble(&s, 1_000_000, 0, 200, &mut bytes, &mut run).unwrap();
        assert!(bytes.iter().all(|b| *b == 0));
        assert_eq!(s.state.lock().unwrap().queue.front().unwrap().offset, 200);
        bytes.clear();
        assemble(&s, 1_000_000, 200, 480, &mut bytes, &mut run).unwrap();
        assert_eq!(s.state.lock().unwrap().pool.len(), 64);
    }

    #[test]
    fn pre_roll_overwrite_and_epoch_queue_loss_are_bounded_and_accounted() {
        let shared = state();
        let samples = [0_u8; 960 * 8];
        for index in 0..120 {
            capture::accept(
                &shared,
                samples.as_ptr().cast_mut(),
                960,
                0,
                index * 960,
                10_000_000 + index * 200_000,
                0,
            )
            .unwrap();
        }
        {
            let mut s = shared.state.lock().unwrap();
            assert_eq!(s.queued_frames, RATE);
            assert_eq!(s.summary.pre_roll_overwritten_frames, 70 * 960);
            assert!(
                s.pool.iter().map(Vec::capacity).sum::<usize>()
                    + s.queue.iter().map(|p| p.bytes.capacity()).sum::<usize>()
                    + BLOCK_BYTES
                    < 8 * 1024 * 1024
            );
            // Epoch lies inside overwritten coverage. Missing coverage is a
            // timestamp boundary, not a fabricated count of discarded samples.
            align_pre_roll(&mut s, 10_000_000 + 68 * 200_000 + 100_000, Instant::now()).unwrap();
            assert!(s.queued_frames <= LEAD);
            assert_eq!(s.summary.discarded_frames, 24_000);
            assert_eq!(s.summary.outcome, "degraded");
        }
        let mut output = Vec::new();
        assemble(&shared, 23_700_000, 0, 2400, &mut output, &mut None).unwrap();
        assert_eq!(
            shared.state.lock().unwrap().summary.startup_missing_frames,
            2400
        );
        assert!(output.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn empty_and_bad_timestamp_packets_never_corrupt_pcm_delivery() {
        use windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR;
        let shared = state();
        capture::accept(&shared, std::ptr::null_mut(), 0, 0, 0, 0, 0).unwrap();
        assert!(
            capture::accept(
                &shared,
                std::ptr::null_mut(),
                480,
                AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32,
                0,
                1_000_000,
                0
            )
            .is_err()
        );
        let mut output = Vec::new();
        assemble(&shared, 1_000_000, 0, 480, &mut output, &mut None).unwrap();
        assert_eq!(output, vec![0; 480 * 8]);
        packet(&shared, 480, 480, 480, false);
        output.clear();
        assert_eq!(
            assemble(&shared, 1_000_000, 480, 960, &mut output, &mut None).unwrap(),
            (480, 480)
        );
        let s = shared.state.lock().unwrap();
        assert_eq!(s.summary.discarded_frames, 480);
        assert_eq!(s.summary.unavailable_fill_frames, 480);
        assert_eq!(s.summary.late_valid_frames, 0);
    }

    #[test]
    #[ignore = "requires interactive Windows playback; writes dedicated raw PCM evidence"]
    fn capture_real_endpoint() {
        let output = std::path::PathBuf::from(
            std::env::var_os("CHRONOBREAK_AUDIO_TEST_OUTPUT").expect("dedicated output"),
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .expect("new fixture output");
        let mut audio = NativeAudio::start().expect("capture owner");
        let (mut ticks, mut frequency) = (0_i64, 0_i64);
        // SAFETY: test outputs are writable counters; QPC calls borrow nothing beyond the call.
        unsafe {
            windows::Win32::System::Performance::QueryPerformanceFrequency(&mut frequency).unwrap();
            windows::Win32::System::Performance::QueryPerformanceCounter(&mut ticks).unwrap();
        }
        let epoch = i64::try_from(i128::from(ticks) * 10_000_000 / i128::from(frequency)).unwrap();
        let anchor = crate::platform::instant_from_qpc_100ns(epoch).unwrap();
        let name = audio
            .begin_transport(epoch, anchor, Instant::now() + Duration::from_secs(1))
            .unwrap();
        let flow = audio.flow();
        let reader = thread::spawn(move || {
            let mut input = std::fs::File::open(name).expect("PCM client");
            let mut block = [0_u8; BLOCK_BYTES];
            let mut total = 0_u64;
            loop {
                let count = input.read(&mut block).expect("PCM read");
                if count == 0 {
                    break;
                }
                file.write_all(&block[..count]).unwrap();
                total += count as u64;
                // Audio-only fixture: simulated downstream video progress is
                // explicit and is not WGC/NVENC synchronization evidence.
                flow.progress(total / 8 / 800);
            }
            total
        });
        assert!(audio.await_handshake(
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false)
        ));
        thread::sleep(Duration::from_secs(4));
        audio.stop(240);
        let bytes = reader.join().unwrap();
        audio.pump.take().unwrap().join().unwrap();
        audio.capture.take().unwrap().join().unwrap();
        let summary = audio.summary();
        std::fs::write(
            output.with_extension("json"),
            serde_json::to_vec_pretty(&summary).unwrap(),
        )
        .unwrap();
        println!("{}", serde_json::to_string(&summary).unwrap());
        assert!(summary.observed_frames > 0, "no endpoint packet evidence");
        assert!(bytes > 0);
    }
}
