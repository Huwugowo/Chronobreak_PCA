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
        let capture = NativeWgcCapture::start(&winrt, target, &device)?;
        Ok(Self {
            capture,
            device,
            _winrt: winrt,
        })
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

    pub(crate) fn take_pending(&mut self) -> Option<CapturedWgcFrame<'_>> {
        self.capture.take_pending()
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
        self.capture.shutdown()?;
        Ok(self.capture.telemetry())
    }
}

