use std::marker::PhantomData;
use std::rc::Rc;

use anyhow::{Context, Result};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

/// Thread-affine Windows Runtime MTA initialization for the native capture/GPU
/// worker. The `Rc` phantom deliberately makes this guard !Send/!Sync so its
/// matching `RoUninitialize` cannot run on a different thread.
pub(crate) struct WinRtMtaGuard {
    _thread_affinity: PhantomData<Rc<()>>,
}

impl WinRtMtaGuard {
    pub(crate) fn initialize() -> Result<Self> {
        // SAFETY: initialization and the matching uninitialization are owned by
        // this !Send/!Sync guard and therefore occur on the same thread.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .context("could not initialize Windows Runtime MTA for native capture")?;
        Ok(Self {
            _thread_affinity: PhantomData,
        })
    }
}

impl Drop for WinRtMtaGuard {
    fn drop(&mut self) {
        // SAFETY: construction succeeded on this thread and the affinity marker
        // prevents moving the guard before this matching uninitialization.
        unsafe { RoUninitialize() };
    }
}

