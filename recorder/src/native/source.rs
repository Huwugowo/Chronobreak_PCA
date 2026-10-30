use std::time::Duration;

use anyhow::{Context, Result};

use crate::platform::CaptureTarget;

use super::capture::{CapturedWgcFrame, NativeWgcCapture, NativeWgcTelemetrySnapshot};
use super::convert::NativeNv12Converter;
use super::d3d11::NativeD3d11Device;
use super::winrt::WinRtMtaGuard;

/// Thread-affine owner of the complete native source stack.
///
/// Field order is intentional: Rust drops fields in declaration order, so WGC
/// is detached and closed before the D3D11 device is released, and the WinRT
/// MTA guard is uninitialized last on the same thread. The guard is !Send/!Sync,
/// which prevents moving a live source to another thread after construction.
pub struct NativeWgcSource {
    capture: NativeWgcCapture,
    device: NativeD3d11Device,
    _winrt: WinRtMtaGuard,
}

impl NativeWgcSource {
    /// Construct this value inside the dedicated GPU submission worker.
    pub fn start(target: &CaptureTarget) -> Result<Self> {
        let winrt = WinRtMtaGuard::initialize()?;
        let adapter_luid = target
            .windows_adapter_luid()
            .context("native WGC target has no DXGI adapter LUID")?;
        let device = NativeD3d11Device::for_luid(&winrt, adapter_luid)?;
        let capture = NativeWgcCapture::prepare(&winrt, target, &device)
            .context("native WGC preparation failed before callback registration")?;
        let mut source = Self {
            capture,
            device,
            _winrt: winrt,
        };
        if let Err(error) = source.capture.start() {
            return Err(
                source.close_after_initialization_failure("WGC callback/session startup", error)
            );
        }
        Ok(source)
    }

    pub fn try_recv(&self) -> Result<Option<CapturedWgcFrame<'_>>> {
        self.capture.try_recv()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Option<CapturedWgcFrame<'_>>> {
        self.capture.recv_timeout(timeout)
    }

    pub(crate) fn receive_pending_timeout(&mut self, timeout: Duration) -> Result<(bool, u64)> {
        self.capture.receive_pending_timeout(timeout)
    }

    pub(crate) fn drain_handoff_to_pending(&mut self) -> Result<u64> {
        self.capture.drain_handoff_to_pending()
    }

    pub(crate) fn take_pending_through(&mut self, qpc_100ns: i64) -> Option<CapturedWgcFrame<'_>> {
        self.capture.take_pending_through(qpc_100ns)
    }

    pub(crate) fn record_worker_frame_discard(&mut self) {
        self.capture.record_worker_frame_discard();
    }

    pub fn telemetry(&self) -> NativeWgcTelemetrySnapshot {
        self.capture.telemetry()
    }

    pub fn pool_dimensions(&self) -> (u32, u32) {
        self.capture.pool_dimensions()
    }

    /// Recreate the capacity-two WGC pool after the caller has consumed and
    /// closed the frame that reported a new content size.
    pub fn recreate_for_content_size(&mut self, width: u32, height: u32) -> Result<()> {
        self.capture.recreate(&self.device, width, height)
    }

    pub fn adapter_index(&self) -> u32 {
        self.device.adapter_index()
    }

    pub fn adapter_luid(&self) -> u64 {
        self.device.adapter_luid()
    }

    pub fn adapter_name(&self) -> &str {
        self.device.adapter_name()
    }

    pub fn feature_level(&self) -> windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL {
        self.device.feature_level()
    }

    pub(crate) fn device(&self) -> &NativeD3d11Device {
        &self.device
    }

    /// Create the fixed four-slot GPU converter on this source's owning
    /// worker and same-adapter D3D11 device.
    pub fn create_nv12_converter(
        &self,
        output_width: u32,
        output_height: u32,
        fps: u32,
    ) -> Result<NativeNv12Converter> {
        let (input_width, input_height) = self.pool_dimensions();
        NativeNv12Converter::new(
            &self.device,
            input_width,
            input_height,
            output_width,
            output_height,
            fps,
        )
    }

    pub fn close(mut self) -> Result<NativeWgcTelemetrySnapshot> {
        match self.capture.shutdown() {
            Ok(()) => Ok(self.capture.telemetry()),
            Err(error) => {
                // A free-threaded WGC callback can still be unwinding when a
                // bounded shutdown reports failure. Dropping the source here
                // would release the WinRT objects/device and uninitialize the
                // worker MTA underneath that callback, which can crash inside
                // GraphicsCapture.dll. This is a terminal recording failure;
                // deliberately retain the complete source stack until process
                // exit rather than turning it into a use-after-unload.
                tracing::error!(
                    error = %error,
                    "retaining native WGC source after incomplete shutdown to protect late callbacks"
                );
                std::mem::forget(self);
                Err(error)
            }
        }
    }

    pub(crate) fn close_after_initialization_failure(
        self,
        stage: &'static str,
        error: anyhow::Error,
    ) -> anyhow::Error {
        initialization_failure(stage, error, || self.close().map(|_| ()))
    }
}

fn initialization_failure(
    stage: &'static str,
    error: anyhow::Error,
    cleanup: impl FnOnce() -> Result<()>,
) -> anyhow::Error {
    let cleanup = cleanup();
    tracing::error!(stage, error = %format!("{error:#}"), cleanup_ok = cleanup.is_ok(),
        cleanup_error = cleanup.as_ref().err().map(|error| format!("{error:#}")),
        "native partial initialization failed; source cleanup completed or retained safely");
    match cleanup {
        Ok(()) => error.context(format!("{stage}; source cleanup complete")),
        Err(cleanup) => error.context(format!(
            "{stage}; source retained after cleanup failure: {cleanup:#}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_initialization_closes_once_and_retains_primary_and_cleanup_errors() {
        for cleanup_fails in [false, true] {
            let calls = std::cell::Cell::new(0);
            let error = initialization_failure(
                "converter",
                anyhow::anyhow!("GPU setup failed").context("original stage"),
                || {
                    calls.set(calls.get() + 1);
                    if cleanup_fails {
                        anyhow::bail!("callback still owned");
                    }
                    Ok(())
                },
            );
            let message = format!("{error:#}");
            assert_eq!(calls.get(), 1);
            assert!(message.contains("original stage: GPU setup failed"));
            if cleanup_fails {
                assert!(message.contains("retained after cleanup failure: callback still owned"));
            } else {
                assert!(message.contains("source cleanup complete"));
            }
        }
    }
}
