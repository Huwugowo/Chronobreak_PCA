use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::{IInspectable, factory};

use crate::platform::{CaptureTarget, validate_capture_target_identity};

use super::d3d11::{NativeD3d11Device, NativeSourceTexture};
use super::winrt::WinRtMtaGuard;
use super::{NATIVE_WGC_FRAME_POOL_CAPACITY, NATIVE_WGC_HANDOFF_CAPACITY};

const CALLBACK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NativeWgcCallbackErrorCategory {
    MissingEventSource = 1,
    FrameAcquisition = 2,
    InvalidTimestamp = 3,
    InvalidContentSize = 4,
    HandoffDisconnected = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWgcCallbackError {
    pub category: NativeWgcCallbackErrorCategory,
    pub hresult: Option<i32>,
}

struct QueuedWgcFrame {
    frame: Option<Direct3D11CaptureFrame>,
    qpc_100ns: i64,
    width: u32,
    height: u32,
}

impl QueuedWgcFrame {
    fn close_inner(&mut self) -> Result<()> {
        let Some(frame) = self.frame.take() else {
            return Ok(());
        };
        frame.Close().context("could not close native WGC frame")
    }
}

impl Drop for QueuedWgcFrame {
    fn drop(&mut self) {
        let _ = self.close_inner();
    }
}

/// One WGC frame admitted into the single-slot handoff to the native GPU
/// worker. Its lifetime is borrowed from the source, so safe code cannot tear
/// down WinRT/WGC before closing the frame. The internal pool surface is never
/// exposed as a cloneable COM handle.
pub struct CapturedWgcFrame<'source> {
    inner: QueuedWgcFrame,
    _source: PhantomData<&'source NativeWgcCapture>,
}

impl CapturedWgcFrame<'_> {
    pub fn qpc_100ns(&self) -> i64 {
        self.inner.qpc_100ns
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.inner.width, self.inner.height)
    }

    pub fn texture_desc(
        &self,
    ) -> Result<windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC> {
        Ok(self.source_texture()?.desc())
    }

    pub(super) fn source_texture(&self) -> Result<NativeSourceTexture<'_>> {
        let frame = self
            .inner
            .frame
            .as_ref()
            .context("native WGC frame was already closed")?;
        NativeSourceTexture::from_frame(frame)
    }

    pub fn close(mut self) -> Result<()> {
        self.inner.close_inner()
    }
}

#[derive(Default)]
struct NativeWgcTelemetry {
    arrivals: AtomicU64,
    admitted: AtomicU64,
    handoff_drops: AtomicU64,
    callback_errors: AtomicU64,
    first_arrival_qpc_100ns: AtomicI64,
    latest_arrival_qpc_100ns: AtomicI64,
    first_accepted_qpc_100ns: AtomicI64,
    latest_accepted_qpc_100ns: AtomicI64,
    first_callback_error: AtomicU64,
    recreations: AtomicU64,
    closed: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeWgcTelemetrySnapshot {
    pub arrivals: u64,
    pub admitted: u64,
    pub handoff_drops: u64,
    pub callback_errors: u64,
    pub first_arrival_qpc_100ns: Option<i64>,
    pub latest_arrival_qpc_100ns: Option<i64>,
    pub first_accepted_qpc_100ns: Option<i64>,
    pub latest_accepted_qpc_100ns: Option<i64>,
    pub first_callback_error: Option<NativeWgcCallbackError>,
    pub recreations: u64,
    pub closed: bool,
}

impl NativeWgcTelemetry {
    fn snapshot(&self) -> NativeWgcTelemetrySnapshot {
        let first_arrival = self.first_arrival_qpc_100ns.load(Ordering::Relaxed);
        let latest_arrival = self.latest_arrival_qpc_100ns.load(Ordering::Relaxed);
        let first_accepted = self.first_accepted_qpc_100ns.load(Ordering::Relaxed);
        let latest_accepted = self.latest_accepted_qpc_100ns.load(Ordering::Relaxed);
        NativeWgcTelemetrySnapshot {
            arrivals: self.arrivals.load(Ordering::Relaxed),
            admitted: self.admitted.load(Ordering::Relaxed),
            handoff_drops: self.handoff_drops.load(Ordering::Relaxed),
            callback_errors: self.callback_errors.load(Ordering::Relaxed),
            first_arrival_qpc_100ns: (first_arrival > 0).then_some(first_arrival),
            latest_arrival_qpc_100ns: (latest_arrival > 0).then_some(latest_arrival),
            first_accepted_qpc_100ns: (first_accepted > 0).then_some(first_accepted),
            latest_accepted_qpc_100ns: (latest_accepted > 0).then_some(latest_accepted),
            first_callback_error: decode_callback_error(
                self.first_callback_error.load(Ordering::Relaxed),
            ),
            recreations: self.recreations.load(Ordering::Relaxed),
            closed: self.closed.load(Ordering::Acquire),
        }
    }

    fn record_callback_error(
        &self,
        category: NativeWgcCallbackErrorCategory,
        hresult: Option<i32>,
    ) {
        self.callback_errors.fetch_add(1, Ordering::Relaxed);
        let raw_hresult = hresult
            .map(|value| u32::from_ne_bytes(value.to_ne_bytes()))
            .unwrap_or_default();
        let packed = (u64::from(category as u32) << 32) | u64::from(raw_hresult);
        let _ = self.first_callback_error.compare_exchange(
            0,
            packed,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }
}

#[derive(Default)]
struct FrameCallbackState {
    active: AtomicU64,
    reconfiguring: AtomicBool,
    stopping: AtomicBool,
}

impl FrameCallbackState {
    fn enter(&self) -> Option<FrameCallbackActivity<'_>> {
        self.active.fetch_add(1, Ordering::SeqCst);
        if self.reconfiguring.load(Ordering::SeqCst) || self.stopping.load(Ordering::SeqCst) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            None
        } else {
            Some(FrameCallbackActivity { state: self })
        }
    }

    fn wait_until_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.active.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        self.active.load(Ordering::SeqCst) == 0
    }
}

struct FrameCallbackActivity<'state> {
    state: &'state FrameCallbackState,
}

impl Drop for FrameCallbackActivity<'_> {
    fn drop(&mut self) {
        self.state.active.fetch_sub(1, Ordering::SeqCst);
    }
}

struct FrameAdmissionPause {
    state: Arc<FrameCallbackState>,
}

impl FrameAdmissionPause {
    fn begin(state: &Arc<FrameCallbackState>) -> Self {
        state.reconfiguring.store(true, Ordering::SeqCst);
        Self {
            state: state.clone(),
        }
    }
}

impl Drop for FrameAdmissionPause {
    fn drop(&mut self) {
        self.state.reconfiguring.store(false, Ordering::SeqCst);
    }
}

/// Exact-HWND WGC source with a two-frame WinRT pool and a capacity-one
/// software handoff. The FrameArrived callback never waits for downstream GPU
/// or encoder work; `try_send` failure is an intentional recorder-frame drop.
pub(crate) struct NativeWgcCapture {
    frame_pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    item: GraphicsCaptureItem,
    target_hwnd: HWND,
    frame_arrived_token: Option<i64>,
    item_closed_token: Option<i64>,
    receiver: Receiver<QueuedWgcFrame>,
    telemetry: Arc<NativeWgcTelemetry>,
    callback_state: Arc<FrameCallbackState>,
    pool_width: u32,
    pool_height: u32,
    shutdown_complete: bool,
}

impl NativeWgcCapture {
    pub(crate) fn start(
        _winrt: &WinRtMtaGuard,
        target: &CaptureTarget,
        device: &NativeD3d11Device,
    ) -> Result<Self> {
        validate_capture_target_identity(target)
            .context("native capture target is no longer the selected exact HWND")?;
        let hwnd = target
            .windows_hwnd()
            .context("native WGC requires an exact Windows HWND target")?;
        let adapter_index = target
            .windows_adapter_index()
            .context("native WGC target has no DXGI adapter index")?;
        let adapter_luid = target
            .windows_adapter_luid()
            .context("native WGC target has no DXGI adapter LUID")?;
        if adapter_luid != device.adapter_luid() {
            bail!(
                "native WGC/D3D11 device does not match target adapter: target index {adapter_index} LUID {adapter_luid:016x}, resolved device index {} LUID {:016x}",
                device.adapter_index(),
                device.adapter_luid()
            );
        }

        if !GraphicsCaptureSession::IsSupported()
            .context("could not query Windows Graphics Capture support")?
        {
            bail!("Windows Graphics Capture is not supported on this Windows build");
        }

        let interop: IGraphicsCaptureItemInterop =
            factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
                .context("could not obtain GraphicsCaptureItem Win32 interop factory")?;
        // SAFETY: the target identity was just revalidated, the integer stores
        // an HWND captured from EnumWindows, and CreateForWindow retains only
        // the window identity rather than the temporary pointer wrapper.
        let item: GraphicsCaptureItem =
            unsafe { interop.CreateForWindow(HWND(hwnd as usize as *mut _)) }
                .context("could not create native WGC item for exact HWND")?;
        let item_size = item
            .Size()
            .context("could not read native WGC target size")?;
        if item_size.Width <= 0 || item_size.Height <= 0 {
            bail!("native WGC target reported an empty content size");
        }

        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            device.winrt_device(),
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            NATIVE_WGC_FRAME_POOL_CAPACITY,
            item_size,
        )
        .context("could not create free-threaded native WGC frame pool")?;
        let session = frame_pool
            .CreateCaptureSession(&item)
            .context("could not create native WGC capture session")?;

        // Cursor capture is not required by Chronobreak. Older supported builds
        // may not expose this setter, so failure here is non-fatal and does not
        // change the capture correctness contract.
        let _ = session.SetIsCursorCaptureEnabled(false);

        let telemetry = Arc::new(NativeWgcTelemetry::default());
        let callback_state = Arc::new(FrameCallbackState::default());
        let (sender, receiver) = sync_channel::<QueuedWgcFrame>(NATIVE_WGC_HANDOFF_CAPACITY);

        let callback_telemetry = telemetry.clone();
        let frame_callback_state = callback_state.clone();
        let frame_arrived_token = frame_pool
            .FrameArrived(
                &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
                    move |pool, _| {
                        let Some(_activity) = frame_callback_state.enter() else {
                            return Ok(());
                        };
                        let Some(pool) = pool.as_ref() else {
                            callback_telemetry.record_callback_error(
                                NativeWgcCallbackErrorCategory::MissingEventSource,
                                None,
                            );
                            return Ok(());
                        };
                        callback_telemetry.arrivals.fetch_add(1, Ordering::Relaxed);
                        match pool.TryGetNextFrame() {
                            Ok(frame) => admit_frame(frame, &sender, &callback_telemetry),
                            Err(error) => callback_telemetry.record_callback_error(
                                NativeWgcCallbackErrorCategory::FrameAcquisition,
                                Some(error.code().0),
                            ),
                        }
                        Ok(())
                    },
                ),
            )
            .context("could not register native WGC FrameArrived callback")?;

        let closed_telemetry = telemetry.clone();
        let item_closed_token = item
            .Closed(
                &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                    closed_telemetry.closed.store(true, Ordering::Release);
                    Ok(())
                }),
            )
            .context("could not register native WGC target-closed callback")?;

        session
            .StartCapture()
            .context("could not start native WGC capture")?;

        Ok(Self {
            frame_pool,
            session,
            item,
            target_hwnd: HWND(hwnd as usize as *mut _),
            frame_arrived_token: Some(frame_arrived_token),
            item_closed_token: Some(item_closed_token),
            receiver,
            telemetry,
            callback_state,
            pool_width: item_size.Width as u32,
            pool_height: item_size.Height as u32,
            shutdown_complete: false,
        })
    }

    pub(crate) fn try_recv(&self) -> Result<Option<CapturedWgcFrame<'_>>> {
        match self.receiver.try_recv() {
            Ok(inner) => Ok(Some(CapturedWgcFrame {
                inner,
                _source: PhantomData,
            })),
            Err(TryRecvError::Empty) => {
                self.refresh_closed_state();
                Ok(None)
            }
            Err(TryRecvError::Disconnected) => bail!("native WGC callback handoff disconnected"),
        }
    }

    pub(crate) fn recv_timeout(&self, timeout: Duration) -> Result<Option<CapturedWgcFrame<'_>>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(inner) => Ok(Some(CapturedWgcFrame {
                inner,
                _source: PhantomData,
            })),
            Err(RecvTimeoutError::Timeout) => {
                self.refresh_closed_state();
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("native WGC callback handoff disconnected")
            }
        }
    }

    pub(crate) fn telemetry(&self) -> NativeWgcTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    fn refresh_closed_state(&self) {
        if self.telemetry.closed.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: `target_hwnd` came from the validated CaptureTarget. IsWindow
        // accepts stale HWND values and returns false rather than dereferencing
        // application memory.
        if !unsafe { IsWindow(Some(self.target_hwnd)).as_bool() } {
            self.telemetry.closed.store(true, Ordering::Release);
        }
    }

    pub(crate) fn pool_dimensions(&self) -> (u32, u32) {
        (self.pool_width, self.pool_height)
    }

    pub(crate) fn recreate(
        &mut self,
        device: &NativeD3d11Device,
        width: u32,
        height: u32,
    ) -> Result<()> {
        if (width, height) == self.pool_dimensions() {
            return Ok(());
        }
        let width_i32 = i32::try_from(width).context("native WGC width exceeds i32")?;
        let height_i32 = i32::try_from(height).context("native WGC height exceeds i32")?;
        if width_i32 <= 0 || height_i32 <= 0 {
            bail!("native WGC cannot recreate an empty frame pool");
        }

        let _pause = FrameAdmissionPause::begin(&self.callback_state);
        if !self
            .callback_state
            .wait_until_idle(CALLBACK_SHUTDOWN_TIMEOUT)
        {
            bail!(
                "native WGC frame callback did not pause within {} ms",
                CALLBACK_SHUTDOWN_TIMEOUT.as_millis()
            );
        }
        while let Ok(frame) = self.receiver.try_recv() {
            drop(frame);
        }
        self.frame_pool
            .Recreate(
                device.winrt_device(),
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                NATIVE_WGC_FRAME_POOL_CAPACITY,
                SizeInt32 {
                    Width: width_i32,
                    Height: height_i32,
                },
            )
            .context("could not recreate native WGC frame pool after target resize")?;
        self.pool_width = width;
        self.pool_height = height;
        self.telemetry.recreations.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) -> Result<()> {
        if self.shutdown_complete {
            return Ok(());
        }

        let mut first_error = None;
        self.callback_state.stopping.store(true, Ordering::SeqCst);
        if let Some(token) = self.frame_arrived_token.take()
            && let Err(error) = self.frame_pool.RemoveFrameArrived(token)
        {
            first_error = Some(
                anyhow::Error::new(error).context("could not detach native WGC frame callback"),
            );
        }
        if let Some(token) = self.item_closed_token.take()
            && let Err(error) = self.item.RemoveClosed(token)
            && first_error.is_none()
        {
            first_error = Some(
                anyhow::Error::new(error).context("could not detach native WGC close callback"),
            );
        }
        if let Err(error) = self.session.Close()
            && first_error.is_none()
        {
            first_error =
                Some(anyhow::Error::new(error).context("could not close native WGC session"));
        }
        if let Err(error) = self.frame_pool.Close()
            && first_error.is_none()
        {
            first_error =
                Some(anyhow::Error::new(error).context("could not close native WGC frame pool"));
        }

        if !self
            .callback_state
            .wait_until_idle(CALLBACK_SHUTDOWN_TIMEOUT)
            && first_error.is_none()
        {
            first_error = Some(anyhow::anyhow!(
                "native WGC frame callback did not stop within {} ms",
                CALLBACK_SHUTDOWN_TIMEOUT.as_millis()
            ));
        }

        let deadline = Instant::now() + CALLBACK_SHUTDOWN_TIMEOUT;
        while (Arc::strong_count(&self.telemetry) != 1
            || Arc::strong_count(&self.callback_state) != 1)
            && Instant::now() < deadline
        {
            std::thread::yield_now();
        }
        if (Arc::strong_count(&self.telemetry) != 1 || Arc::strong_count(&self.callback_state) != 1)
            && first_error.is_none()
        {
            first_error = Some(anyhow::anyhow!(
                "native WGC callbacks did not quiesce within {} ms",
                CALLBACK_SHUTDOWN_TIMEOUT.as_millis()
            ));
        }

        while let Ok(frame) = self.receiver.try_recv() {
            drop(frame);
        }
        self.shutdown_complete = true;
        first_error.map_or(Ok(()), Err)
    }
}

fn admit_frame(
    frame: Direct3D11CaptureFrame,
    sender: &SyncSender<QueuedWgcFrame>,
    telemetry: &NativeWgcTelemetry,
) {
    let timestamp = match frame.SystemRelativeTime() {
        Ok(value) if value.Duration > 0 => value.Duration,
        Ok(_) => {
            telemetry.record_callback_error(NativeWgcCallbackErrorCategory::InvalidTimestamp, None);
            let _ = frame.Close();
            return;
        }
        Err(error) => {
            telemetry.record_callback_error(
                NativeWgcCallbackErrorCategory::InvalidTimestamp,
                Some(error.code().0),
            );
            let _ = frame.Close();
            return;
        }
    };
    let size = match frame.ContentSize() {
        Ok(value) if value.Width > 0 && value.Height > 0 => value,
        Ok(_) => {
            telemetry
                .record_callback_error(NativeWgcCallbackErrorCategory::InvalidContentSize, None);
            let _ = frame.Close();
            return;
        }
        Err(error) => {
            telemetry.record_callback_error(
                NativeWgcCallbackErrorCategory::InvalidContentSize,
                Some(error.code().0),
            );
            let _ = frame.Close();
            return;
        }
    };

    telemetry
        .first_arrival_qpc_100ns
        .compare_exchange(0, timestamp, Ordering::Relaxed, Ordering::Relaxed)
        .ok();
    telemetry
        .latest_arrival_qpc_100ns
        .store(timestamp, Ordering::Relaxed);

    let admitted = QueuedWgcFrame {
        frame: Some(frame),
        qpc_100ns: timestamp,
        width: size.Width as u32,
        height: size.Height as u32,
    };
    match sender.try_send(admitted) {
        Ok(()) => {
            telemetry.admitted.fetch_add(1, Ordering::Relaxed);
            telemetry
                .first_accepted_qpc_100ns
                .compare_exchange(0, timestamp, Ordering::Relaxed, Ordering::Relaxed)
                .ok();
            telemetry
                .latest_accepted_qpc_100ns
                .store(timestamp, Ordering::Relaxed);
        }
        Err(TrySendError::Full(frame)) => {
            telemetry.handoff_drops.fetch_add(1, Ordering::Relaxed);
            drop(frame);
        }
        Err(TrySendError::Disconnected(frame)) => {
            telemetry
                .record_callback_error(NativeWgcCallbackErrorCategory::HandoffDisconnected, None);
            drop(frame);
        }
    }
}

fn decode_callback_error(packed: u64) -> Option<NativeWgcCallbackError> {
    if packed == 0 {
        return None;
    }
    let category = match (packed >> 32) as u32 {
        1 => NativeWgcCallbackErrorCategory::MissingEventSource,
        2 => NativeWgcCallbackErrorCategory::FrameAcquisition,
        3 => NativeWgcCallbackErrorCategory::InvalidTimestamp,
        4 => NativeWgcCallbackErrorCategory::InvalidContentSize,
        5 => NativeWgcCallbackErrorCategory::HandoffDisconnected,
        _ => return None,
    };
    let raw_hresult = (packed & u64::from(u32::MAX)) as u32;
    Some(NativeWgcCallbackError {
        category,
        hresult: (raw_hresult != 0).then(|| i32::from_ne_bytes(raw_hresult.to_ne_bytes())),
    })
}

impl Drop for NativeWgcCapture {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_error_latch_preserves_first_category_and_hresult_bits() {
        let telemetry = NativeWgcTelemetry::default();
        telemetry.record_callback_error(
            NativeWgcCallbackErrorCategory::FrameAcquisition,
            Some(i32::from_ne_bytes(0x887a_0005_u32.to_ne_bytes())),
        );
        telemetry.record_callback_error(NativeWgcCallbackErrorCategory::HandoffDisconnected, None);

        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.callback_errors, 2);
        assert_eq!(
            snapshot.first_callback_error,
            Some(NativeWgcCallbackError {
                category: NativeWgcCallbackErrorCategory::FrameAcquisition,
                hresult: Some(i32::from_ne_bytes(0x887a_0005_u32.to_ne_bytes())),
            })
        );
    }
}

