use std::mem::ManuallyDrop;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::platform::CaptureTarget;

use super::capture::{CapturedWgcFrame, NativeWgcCapture, NativeWgcTelemetrySnapshot};
use super::convert::NativeNv12Converter;
use super::d3d11::NativeD3d11Device;
use super::winrt::WinRtMtaGuard;

/// Thread-affine owner of the complete native source stack.
///
/// Explicit destruction releases capture/device/MTA in that order, only after
/// callback release is proven. This also covers implicit drop and unwinding.
/// The MTA guard is !Send/!Sync, preventing movement to another thread.
pub struct NativeWgcSource {
    capture: ManuallyDrop<NativeWgcCapture>,
    device: ManuallyDrop<NativeD3d11Device>,
    _winrt: ManuallyDrop<WinRtMtaGuard>,
}

impl NativeWgcSource {
    /// Construct this value inside the dedicated GPU submission worker.
    pub fn start(target: &CaptureTarget) -> Result<Self> {
        Self::start_impl(
            target,
            #[cfg(test)]
            None,
        )
    }

    fn start_impl(
        target: &CaptureTarget,
        #[cfg(test)] failure: Option<super::capture::StartupFailurePoint>,
    ) -> Result<Self> {
        let winrt = WinRtMtaGuard::initialize()?;
        let adapter_luid = target
            .windows_adapter_luid()
            .context("native WGC target has no DXGI adapter LUID")?;
        let device = NativeD3d11Device::for_luid(&winrt, adapter_luid)?;
        let capture = NativeWgcCapture::prepare(&winrt, target, &device)
            .context("native WGC preparation failed before callback registration")?;
        let mut source = Self {
            capture: ManuallyDrop::new(capture),
            device: ManuallyDrop::new(device),
            _winrt: ManuallyDrop::new(winrt),
        };
        if let Err(error) = source.capture.start(
            #[cfg(test)]
            failure,
        ) {
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

    pub(crate) fn receive_pending_timeout(
        &mut self,
        timeout: Duration,
        after: i64,
        through: i64,
    ) -> Result<(bool, u64)> {
        self.capture
            .receive_pending_timeout(timeout, after, through)
    }

    pub(crate) fn poll_pending_for_tick(&mut self, after: i64, through: i64) -> Result<u64> {
        self.capture.poll_pending_for_tick(after, through)
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
            Err(error) if !self.capture.shutdown_complete() => {
                tracing::error!(
                    error = %error,
                    "retaining native WGC source after incomplete shutdown to protect late callbacks"
                );
                std::mem::forget(self);
                Err(error)
            }
            Err(error) => Err(error.context(
                "native WGC cleanup failed after callback release; source resources released",
            )),
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

impl Drop for NativeWgcSource {
    fn drop(&mut self) {
        let cleanup = self.capture.shutdown();
        if !self.capture.shutdown_complete() {
            tracing::error!(error = ?cleanup.err(),
                "retaining native WGC source on drop: callback release is unproven");
            return;
        }
        if let Err(error) = cleanup {
            tracing::error!(%error, "native WGC cleanup failed after callback release");
        }
        // SAFETY: this is the sole destruction site for these ManuallyDrop
        // fields. The source is thread-affine, all handler owners have released,
        // and shutdown_complete prevents capture's Drop from repeating cleanup.
        // Release WGC COM references before D3D11 and uninitialize the MTA last.
        unsafe {
            ManuallyDrop::drop(&mut self.capture);
            ManuallyDrop::drop(&mut self.device);
            ManuallyDrop::drop(&mut self._winrt);
        }
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
        Err(cleanup) => error.context(format!("{stage}; source cleanup failed: {cleanup:#}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a running generated wgc_fixture window and QUEUEBACK_TEST_WGC_PID"]
    fn generated_wgc_partial_start_rollback() -> Result<()> {
        use super::super::capture::StartupFailurePoint;
        use crate::platform::{CaptureSource, capture_target_for_process};

        let pid = std::env::var("QUEUEBACK_TEST_WGC_PID")?.parse()?;
        let target = capture_target_for_process(pid)?;
        anyhow::ensure!(
            matches!(&target.source,
            CaptureSource::WindowsGraphicsCapture { window_title: Some(title), .. }
                if title == "QueueBack WGC Changing-Window Fixture"),
            "rollback fixture accepts only the generated WGC window"
        );
        for point in [
            StartupFailurePoint::FrameArrived,
            StartupFailurePoint::Closed,
            StartupFailurePoint::StartCapture,
            StartupFailurePoint::AfterStartCapture,
        ] {
            let error = match NativeWgcSource::start_impl(&target, Some(point)) {
                Ok(source) => {
                    source.close()?;
                    anyhow::bail!("startup injection was not reached");
                }
                Err(error) => error,
            };
            let message = format!("{error:#}");
            anyhow::ensure!(
                message.contains(&format!("injected WGC {point:?} failure")),
                "did not reach {point:?}: {message}"
            );
            anyhow::ensure!(
                message.contains("source cleanup complete"),
                "rollback failed at {point:?}: {message}"
            );
            println!("WGC_ROLLBACK_PASS point={point:?} idle=true handlers_released=true");
        }
        // Successful owners also use the guarded Drop path without close().
        drop(NativeWgcSource::start(&target)?);
        Ok(())
    }

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
                assert!(message.contains("source cleanup failed: callback still owned"));
            } else {
                assert!(message.contains("source cleanup complete"));
            }
        }
    }
}
