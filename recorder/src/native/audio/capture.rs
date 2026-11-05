use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::WAIT_FAILED;
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::WaitForMultipleObjects;
use windows::core::{PCWSTR, implement};

use super::{BLOCK_BYTES, Event, Packet, Shared};

#[implement(IMMNotificationClient)]
struct Notification {
    event: Arc<Event>,
    generation: Arc<AtomicU64>,
    active: Arc<Mutex<Option<String>>>,
}

impl Notification {
    fn changed(&self) -> windows::core::Result<()> {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.event.set();
        Ok(())
    }
    fn relevant(&self, id: &PCWSTR) -> windows::core::Result<()> {
        // Never block inside a device callback; a concurrent ID change requests a recheck.
        let Ok(active) = self.active.try_lock() else {
            return self.changed();
        };
        // SAFETY: notification IDs are valid, terminated strings during callback.
        if active.as_ref().is_none_or(|active| {
            // SAFETY: Windows owns this terminated endpoint ID during callback.
            unsafe { id.as_wide() }
                .iter()
                .copied()
                .eq(active.encode_utf16())
        }) {
            self.changed()?;
        }
        Ok(())
    }
}

impl IMMNotificationClient_Impl for Notification_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.relevant(id)
    }
    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.relevant(id)
    }
    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.relevant(id)
    }
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            self.changed()?;
        }
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        id: &PCWSTR,
        key: &windows::Win32::Foundation::PROPERTYKEY,
    ) -> windows::core::Result<()> {
        if *key == PKEY_AudioEngine_DeviceFormat {
            self.relevant(id)?;
        }
        Ok(())
    }
}

struct Mta;
impl Drop for Mta {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after successful owner-thread COM initialization.
        unsafe {
            CoUninitialize();
        }
    }
}

pub(super) fn run(shared: &Arc<Shared>) -> Result<()> {
    // SAFETY: dedicated capture owner enters/leaves its own MTA exactly once.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    let _mta = Mta;
    // SAFETY: default in-process enumerator created in this initialized MTA.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let changed = Arc::new(Event::new(false)?);
    let generation = Arc::new(AtomicU64::new(0));
    let active = Arc::new(Mutex::new(None));
    let callback: IMMNotificationClient = Notification {
        event: Arc::clone(&changed),
        generation: Arc::clone(&generation),
        active: Arc::clone(&active),
    }
    .into();
    // SAFETY: callback and its independent state outlive registration.
    unsafe {
        enumerator.RegisterEndpointNotificationCallback(&callback)?;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_recover(shared, &enumerator, &changed, &generation, &active)
    }))
    .unwrap_or_else(|_| Err(anyhow::anyhow!("WASAPI capture owner panicked")));
    // Callback is strongly owned through unregister. If release is unproven,
    // retain the registration alone and disable another audio owner this process.
    // SAFETY: same live enumerator/callback, outside notification callbacks.
    if let Err(error) = unsafe { enumerator.UnregisterEndpointNotificationCallback(&callback) } {
        super::AUDIO_DISABLED.store(true, Ordering::Release);
        shared.error(error);
        std::mem::forget((enumerator, callback, changed, generation, _mta));
    }
    result
}

fn capture_recover(
    shared: &Shared,
    enumerator: &IMMDeviceEnumerator,
    changed: &Event,
    generation: &AtomicU64,
    active: &Mutex<Option<String>>,
) -> Result<()> {
    let mut episode = Instant::now();
    let mut next_attempt = Instant::now();
    let mut client_generation = 0;
    let mut last_attempt = Instant::now() - Duration::from_secs(1);
    while !shared.cancelled.load(Ordering::Acquire) {
        if Instant::now() < next_attempt {
            let wait = next_attempt.saturating_duration_since(Instant::now());
            // SAFETY: both owned events remain alive through the bounded wait.
            let signalled = unsafe {
                WaitForMultipleObjects(
                    &[shared.capture_stop.handle(), changed.handle()],
                    false,
                    wait.as_millis()
                        .saturating_add(1)
                        .min(u128::from(u32::MAX - 1)) as u32,
                )
            };
            if signalled == WAIT_FAILED {
                return Err(std::io::Error::last_os_error()).context("WASAPI recovery event wait");
            }
            if shared.cancelled.load(Ordering::Acquire) {
                break;
            }
            if signalled.0 == 1 {
                next_attempt =
                    next_attempt.min((last_attempt + Duration::from_secs(1)).max(Instant::now()));
            }
            // Notification storms do not bypass the one-second attempt rate.
            if Instant::now() < next_attempt {
                continue;
            }
        }
        let opened_at = Instant::now();
        last_attempt = opened_at;
        let before = generation.load(Ordering::Relaxed);
        let activations = shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activations;
        let result = capture_client(
            shared,
            enumerator,
            changed,
            generation,
            before,
            client_generation,
            active,
        );
        client_generation += 1;
        shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .available = false;
        *active.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activations
            > activations
        {
            episode = Instant::now();
        }
        if let Err(e) = result {
            shared.error(e);
        }
        let delay = if episode.elapsed() < Duration::from_secs(5) {
            Duration::from_secs(1)
        } else {
            Duration::from_secs(30)
        };
        next_attempt = opened_at + delay;
    }
    Ok(())
}

fn device_id(device: &IMMDevice) -> Result<String> {
    // SAFETY: live owner-local interface; GetId transfers a task allocation.
    let raw = unsafe { device.GetId()? };
    // SAFETY: GetId returned a valid terminated UTF-16 string.
    let result = unsafe { raw.to_string() };
    // SAFETY: exactly one release of the GetId task allocation, even on decode failure.
    unsafe {
        CoTaskMemFree(Some(raw.0.cast()));
    }
    Ok(result?)
}

fn capture_client(
    shared: &Shared,
    enumerator: &IMMDeviceEnumerator,
    changed: &Event,
    generation: &AtomicU64,
    opened_generation: u64,
    client_generation: u64,
    active: &Mutex<Option<String>>,
) -> Result<()> {
    let event = Event::new(false)?;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
        nChannels: 2,
        nSamplesPerSec: 48000,
        nAvgBytesPerSec: 384000,
        nBlockAlign: 8,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    // SAFETY: every COM interface is created/used/released on this MTA thread.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    let id = device_id(&device)?;
    *active.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
    // SAFETY: activation parameters are absent; interface stays on this MTA.
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
    // SAFETY: format lives through initialization; event stays owned until Stop.
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            0,
            0,
            &format,
            None,
        )?;
        client.SetEventHandle(event.handle())?;
    }
    // SAFETY: initialized client and owner-local service lifetime.
    let capture: IAudioCaptureClient = unsafe { client.GetService()? };
    // Re-read after registration/open, before an obsolete endpoint can be installed.
    // SAFETY: live owner-local enumerator; returned endpoint remains local.
    let current = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    if generation.load(Ordering::Relaxed) != opened_generation || device_id(&current)? != id {
        return Ok(());
    }
    let mut frequency = 0;
    // SAFETY: writable owner-local QPC frequency, constant for this client.
    unsafe {
        QueryPerformanceFrequency(&mut frequency)?;
    }
    if frequency <= 0 {
        bail!("invalid WASAPI performance-counter frequency");
    }
    // SAFETY: client, service and event are fully initialized.
    unsafe {
        client.Start()?;
    }
    {
        let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        s.available = true;
        s.summary.reopen_count = s.activations;
        s.activations += 1;
    }
    tracing::info!(endpoint=%id, "system audio endpoint active");
    let mut previous: Option<(u64, u64, u64)> = None;
    let result = (|| -> Result<()> {
        while !shared.cancelled.load(Ordering::Acquire) {
            // One low-frequency health deadline discovers service failures even
            // when no packet event arrives; it does not infer failure from quiet.
            // SAFETY: all event handles are retained throughout this wait.
            let signalled = unsafe {
                WaitForMultipleObjects(
                    &[
                        shared.capture_stop.handle(),
                        event.handle(),
                        changed.handle(),
                    ],
                    false,
                    30_000,
                )
            };
            if signalled == WAIT_FAILED {
                return Err(std::io::Error::last_os_error()).context("WASAPI capture event wait");
            }
            if shared.cancelled.load(Ordering::Acquire) {
                break;
            }
            if generation.load(Ordering::Relaxed) != opened_generation {
                break;
            }
            // SAFETY: every preceding buffer has been released on this same owner.
            while unsafe { capture.GetNextPacketSize()? } > 0 {
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags, mut position, mut qpc) = (0, 0, 0, 0);
                // SAFETY: writable local outputs; acquired buffer is released below.
                unsafe {
                    capture.GetBuffer(
                        &mut data,
                        &mut frames,
                        &mut flags,
                        Some(&mut position),
                        Some(&mut qpc),
                    )?;
                }
                #[cfg(feature = "native-failure-injection")]
                if shared.inject_bad_timestamp.swap(false, Ordering::AcqRel) {
                    flags |= AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32;
                }
                let discontinuity = flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0;
                let result = (|| {
                    let mut counter = 0;
                    // SAFETY: writable owner-local counter; the acquired audio
                    // buffer is released below even when this clock read fails.
                    unsafe {
                        QueryPerformanceCounter(&mut counter)?;
                    }
                    let now_qpc = i128::from(counter) * 10_000_000 / i128::from(frequency);
                    let bad_clock = i128::from(qpc) > now_qpc
                        || previous.is_some_and(|(last_position, last_end, last_qpc)| {
                            !discontinuity
                                && (qpc <= last_qpc
                                    || position < last_position
                                    || position < last_end)
                        });
                    if bad_clock {
                        shared
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .summary
                            .discarded_frames += u64::from(frames);
                        Err(anyhow::anyhow!("future/backward/inconsistent WASAPI clock"))
                    } else {
                        accept(
                            shared,
                            data,
                            frames,
                            flags,
                            position,
                            qpc,
                            client_generation,
                        )
                    }
                })();
                // Release even rejected/invalid/silent packets before recovery.
                // SAFETY: exact acquired frame count released on the same service/thread.
                let released = unsafe { capture.ReleaseBuffer(frames) };
                result?;
                released?;
                previous = Some((position, position.saturating_add(u64::from(frames)), qpc));
            }
        }
        Ok(())
    })();
    // SAFETY: successfully started owner-local client; stop precedes release.
    unsafe {
        client.Stop()?;
    }
    result
}

pub(super) fn accept(
    shared: &Shared,
    data: *mut u8,
    frames: u32,
    flags: u32,
    position: u64,
    qpc: u64,
    generation: u64,
) -> Result<()> {
    let mut s = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    if frames == 0 {
        return Ok(());
    }
    let bytes = frames as usize * 8;
    if flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0
        || qpc == 0
        || bytes > BLOCK_BYTES
        || qpc > i64::MAX as u64
    {
        s.summary.discarded_frames += u64::from(frames);
        bail!("unusable WASAPI timestamp/packet size");
    }
    let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
    let discontinuity = flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0;
    s.summary.observed_frames += u64::from(frames);
    s.summary.confirmed_silent_frames += if silent { u64::from(frames) } else { 0 };
    s.summary.discontinuities += u64::from(discontinuity);
    s.summary.first_valid_qpc_100ns.get_or_insert(qpc);
    s.summary.last_valid_qpc_100ns = Some(qpc);
    if let Some((epoch, anchor)) = s.epoch {
        let now_qpc = i128::from(epoch) + (anchor.elapsed().as_nanos() / 100) as i128;
        let last_qpc = i128::from(qpc) + i128::from(frames) * 10_000_000 / 48_000;
        s.summary.maximum_delivery_delay_100ns = s
            .summary
            .maximum_delivery_delay_100ns
            .max(u64::try_from((now_qpc - last_qpc).max(0)).unwrap_or(u64::MAX));
    }
    let capacity = if s.epoch.is_some() { 24_000 } else { 48_000 };
    while s.queued_frames + u64::from(frames) > capacity || s.pool.is_empty() {
        let Some(mut old) = s.queue.pop_front() else {
            break;
        };
        s.queued_frames -= u64::from(old.frames);
        if s.epoch.is_none() {
            s.summary.pre_roll_overwritten_frames += u64::from(old.frames);
            s.overwritten_end_qpc = Some(
                s.overwritten_end_qpc.unwrap_or(0).max(
                    old.qpc
                        .saturating_add(u64::from(old.frames) * 10_000_000 / 48_000),
                ),
            );
        }
        if s.epoch.is_some() {
            s.summary.discarded_frames += u64::from(old.frames);
            s.summary.outcome = "degraded".into();
        }
        old.bytes.clear();
        s.pool.push(old.bytes);
    }
    let mut buffer = s.pool.pop().context("WASAPI packet pool exhausted")?;
    buffer.clear();
    if !silent {
        if data.is_null() {
            s.pool.push(buffer);
            bail!("WASAPI supplied null non-silent samples");
        }
        // SAFETY: GetBuffer retains at least frames*nBlockAlign bytes until the
        // same owner calls ReleaseBuffer; this is the sole packet-owned copy.
        buffer.extend_from_slice(unsafe { std::slice::from_raw_parts(data, bytes) });
    }
    s.queued_frames += u64::from(frames);
    s.queue.push_back(Packet {
        bytes: buffer,
        qpc,
        device: position,
        frames,
        silent,
        discontinuity,
        generation,
        offset: 0,
        slot: None,
    });
    // Packet arrival cannot advance the fixed due cursor or video lead cap.
    // The pump already waits for its next eligible block, mux progress or stop;
    // waking it for every endpoint period only repeats the same decision.
    Ok(())
}
