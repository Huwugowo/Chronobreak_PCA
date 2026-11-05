//! One-way mux observations. These never grant video admission or PCM credit.
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};

pub(super) struct Event(OwnedHandle);

impl Event {
    pub(super) fn new(manual: bool) -> Result<Self> {
        // SAFETY: unnamed event, no borrowed security attributes or name.
        let handle = unsafe { CreateEventW(None, manual, false, None)? };
        // SAFETY: CreateEvent transfers one valid owned handle.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle.0) }))
    }

    pub(super) fn handle(&self) -> HANDLE {
        HANDLE(self.0.as_raw_handle())
    }

    pub(super) fn set(&self) {
        // SAFETY: the event remains owned for this call.
        let _ = unsafe { SetEvent(self.handle()) };
    }

    pub(super) fn reset(&self) -> Result<()> {
        // SAFETY: valid owned event; reset does not release the handle.
        unsafe {
            ResetEvent(self.handle())?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct FlowSnapshot {
    pub(crate) frames: u64,
    pub(crate) generation: u64,
    pub(crate) pending_since: Option<Instant>,
}

pub(crate) struct AudioFlow {
    state: Mutex<FlowSnapshot>,
    pub(super) wake: Event,
}

impl AudioFlow {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            state: Mutex::new(FlowSnapshot::default()),
            wake: Event::new(false)?,
        })
    }

    pub(crate) fn snapshot(&self) -> FlowSnapshot {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn wait(&self, timeout: Duration) {
        let millis = u32::try_from(timeout.as_millis().saturating_add(1)).unwrap_or(u32::MAX - 1);
        // SAFETY: event is retained by this flow throughout the wait.
        unsafe {
            WaitForSingleObject(self.wake.handle(), millis);
        }
    }

    pub(crate) fn progress(&self, frames: u64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if frames > state.frames {
            state.frames = frames;
            drop(state);
            self.wake.set();
        }
    }

    pub(crate) fn begin(&self) -> PendingWrite<'_> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.pending_since = Some(Instant::now());
        drop(state);
        self.wake.set();
        PendingWrite(self)
    }
}

pub(crate) struct PendingWrite<'a>(&'a AudioFlow);

impl Drop for PendingWrite<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.pending_since = None;
        drop(state);
        self.0.wake.set();
    }
}

/// Only a continuously pending real sink operation can qualify the EOF escape.
#[derive(Default)]
pub(crate) struct LeadEscape {
    armed: Option<(u64, u64, Instant)>,
}

impl LeadEscape {
    pub(crate) fn deadline(
        &mut self,
        capped: bool,
        flow: FlowSnapshot,
        now: Instant,
    ) -> Option<Instant> {
        if !capped || flow.pending_since.is_none() {
            self.armed = None;
            return None;
        }
        let (_, _, since) = *self
            .armed
            .get_or_insert((flow.frames, flow.generation, now));
        if self
            .armed
            .is_some_and(|(m, g, _)| m != flow.frames || g != flow.generation)
        {
            self.armed = Some((flow.frames, flow.generation, now));
            return Some(now + Duration::from_secs(1));
        }
        Some(since + Duration::from_secs(1))
    }
}
