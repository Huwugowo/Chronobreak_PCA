use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem,
    GraphicsCaptureSession, IDirect3D11CaptureFramePoolStatics2, IGraphicsCaptureSessionStatics,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::core::{IInspectable, Interface, Type, factory};

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

#[derive(Debug)]
struct LatestPending<T> {
    value: Option<T>,
    replacements: u64,
    high_water_mark: u64,
}

impl<T> Default for LatestPending<T> {
    fn default() -> Self {
        Self {
            value: None,
            replacements: 0,
            high_water_mark: 0,
        }
    }
}

impl<T> LatestPending<T> {
    fn replace(&mut self, value: T) -> bool {
        let replaced = self.value.replace(value).is_some();
        if replaced {
            self.replacements = self.replacements.saturating_add(1);
        }
        self.high_water_mark = 1;
        replaced
    }

    fn take(&mut self) -> Option<T> {
        self.value.take()
    }

    fn clear(&mut self) -> bool {
        self.value.take().is_some()
    }
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
    pub pending_frame_replacements: u64,
    pub pending_frame_high_water_mark: u64,
    pub worker_frame_discards: u64,
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
            pending_frame_replacements: 0,
            pending_frame_high_water_mark: 0,
            worker_frame_discards: 0,
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
    owners: AtomicU64,
    reconfiguring: AtomicBool,
    stopping: AtomicBool,
    wait_lock: Mutex<()>,
    idle: Condvar,
}

impl FrameCallbackState {
    fn enter(&self) -> Option<FrameCallbackActivity<'_>> {
        self.active.fetch_add(1, Ordering::AcqRel);
        if self.reconfiguring.load(Ordering::Acquire) || self.stopping.load(Ordering::Acquire) {
            self.leave();
            None
        } else {
            Some(FrameCallbackActivity { state: self })
        }
    }

    fn wait_until_idle(&self, timeout: Duration) -> bool {
        self.wait_for(timeout, false)
    }

    fn wait_until_released(&self, timeout: Duration) -> bool {
        self.wait_for(timeout, true)
    }

    fn wait_for(&self, timeout: Duration, require_owner_release: bool) -> bool {
        let deadline = Instant::now() + timeout;
        let Ok(mut guard) = self.wait_lock.lock() else {
            return false;
        };
        loop {
            let idle = self.active.load(Ordering::Acquire) == 0;
            let owners_released =
                !require_owner_release || self.owners.load(Ordering::Acquire) == 0;
            if idle && owners_released {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let Ok((next_guard, wait)) = self.idle.wait_timeout(guard, remaining) else {
                return false;
            };
            guard = next_guard;
            if wait.timed_out() {
                return self.active.load(Ordering::Acquire) == 0
                    && (!require_owner_release || self.owners.load(Ordering::Acquire) == 0);
            }
        }
    }

    fn leave(&self) {
        if self.active.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.notify_waiters();
        }
    }

    fn add_owner(&self) {
        self.owners.fetch_add(1, Ordering::Release);
    }

    fn remove_owner(&self) {
        if self.owners.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.notify_waiters();
        }
    }

    fn notify_waiters(&self) {
        // Taking the same mutex as the waiter prevents a last-callback
        // notification from being lost between predicate check and wait.
        if let Ok(_guard) = self.wait_lock.lock() {
            self.idle.notify_all();
        }
    }
}

struct FrameCallbackActivity<'state> {
    state: &'state FrameCallbackState,
}

impl Drop for FrameCallbackActivity<'_> {
    fn drop(&mut self) {
        self.state.leave();
    }
}

struct FrameCallbackOwner {
    state: Arc<FrameCallbackState>,
}

impl FrameCallbackOwner {
    fn new(state: &Arc<FrameCallbackState>) -> Self {
        state.add_owner();
        Self {
            state: Arc::clone(state),
        }
    }
}

impl Drop for FrameCallbackOwner {
    fn drop(&mut self) {
        self.state.remove_owner();
    }
}

struct FrameAdmissionPause {
    state: Arc<FrameCallbackState>,
}

impl FrameAdmissionPause {
    fn begin(state: &Arc<FrameCallbackState>) -> Self {
        state.reconfiguring.store(true, Ordering::Release);
        Self {
            state: state.clone(),
        }
    }
}

impl Drop for FrameAdmissionPause {
    fn drop(&mut self) {
        self.state.reconfiguring.store(false, Ordering::Release);
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
    pending: LatestPending<QueuedWgcFrame>,
    worker_frame_discards: u64,
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

        if !graphics_capture_supported()? {
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

        let frame_pool = create_free_threaded_frame_pool(device, item_size)?;
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
        let frame_callback_owner = FrameCallbackOwner::new(&callback_state);
        let frame_arrived_token = frame_pool
            .FrameArrived(
                &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
                    move |pool, _| {
                        let Some(_activity) = frame_callback_owner.state.enter() else {
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
        let closed_callback_owner = FrameCallbackOwner::new(&callback_state);
        let item_closed_token = item
            .Closed(
                &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                    let _owner = &closed_callback_owner;
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
            pending: LatestPending::default(),
            worker_frame_discards: 0,
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

    pub(crate) fn receive_pending_timeout(&mut self, timeout: Duration) -> Result<(bool, u64)> {
        match self.receiver.recv_timeout(timeout) {
            Ok(frame) => {
                let replacements = u64::from(self.pending.replace(frame))
                    .saturating_add(self.drain_handoff_to_pending()?);
                Ok((true, replacements))
            }
            Err(RecvTimeoutError::Timeout) => {
                self.refresh_closed_state();
                Ok((false, 0))
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("native WGC callback handoff disconnected")
            }
        }
    }

    pub(crate) fn drain_handoff_to_pending(&mut self) -> Result<u64> {
        let mut replacements = 0_u64;
        loop {
            match self.receiver.try_recv() {
                Ok(frame) => {
                    if self.pending.replace(frame) {
                        replacements = replacements.saturating_add(1);
                    }
                }
                Err(TryRecvError::Empty) => return Ok(replacements),
                Err(TryRecvError::Disconnected) => {
                    bail!("native WGC callback handoff disconnected")
                }
            }
        }
    }

    pub(crate) fn take_pending(&mut self) -> Option<CapturedWgcFrame<'_>> {
        self.pending.take().map(|inner| CapturedWgcFrame {
            inner,
            _source: PhantomData,
        })
    }

    pub(crate) fn record_worker_frame_discard(&mut self) {
        self.worker_frame_discards = self.worker_frame_discards.saturating_add(1);
    }

    pub(crate) fn telemetry(&self) -> NativeWgcTelemetrySnapshot {
        let mut snapshot = self.telemetry.snapshot();
        snapshot.pending_frame_replacements = self.pending.replacements;
        snapshot.pending_frame_high_water_mark = self.pending.high_water_mark;
        snapshot.worker_frame_discards = self.worker_frame_discards;
        snapshot
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
            self.worker_frame_discards = self.worker_frame_discards.saturating_add(1);
        }
        if self.pending.clear() {
            self.worker_frame_discards = self.worker_frame_discards.saturating_add(1);
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
        self.callback_state.stopping.store(true, Ordering::Release);
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

        while let Ok(frame) = self.receiver.try_recv() {
            drop(frame);
            self.worker_frame_discards = self.worker_frame_discards.saturating_add(1);
        }
        if self.pending.clear() {
            self.worker_frame_discards = self.worker_frame_discards.saturating_add(1);
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
            .wait_until_released(CALLBACK_SHUTDOWN_TIMEOUT)
            && first_error.is_none()
        {
            first_error = Some(anyhow::anyhow!(
                "native WGC callbacks did not quiesce within {} ms",
                CALLBACK_SHUTDOWN_TIMEOUT.as_millis()
            ));
        }

        self.shutdown_complete = true;
        first_error.map_or(Ok(()), Err)
    }
}

/// Load the WGC statics for this worker instead of using the generated
/// process-static FactoryCache. The returned COM pointer is therefore dropped
/// before this worker's explicit WinRT MTA guard is uninitialized.
fn graphics_capture_supported() -> Result<bool> {
    let statics: IGraphicsCaptureSessionStatics =
        factory::<GraphicsCaptureSession, IGraphicsCaptureSessionStatics>()
            .context("could not load Windows Graphics Capture session statics")?;
    let mut supported = false;
    // SAFETY: `statics` is a live factory loaded in the current MTA and
    // `supported` is a live, aligned bool output for this synchronous call.
    unsafe {
        (Interface::vtable(&statics).IsSupported)(Interface::as_raw(&statics), &mut supported)
            .ok()
            .context("could not query Windows Graphics Capture support")?;
    }
    Ok(supported)
}

/// Create the free-threaded pool through a worker-scoped statics interface.
/// This avoids retaining a generated global factory pointer across separate
/// worker MTA lifetimes and consecutive recordings.
fn create_free_threaded_frame_pool(
    device: &NativeD3d11Device,
    size: SizeInt32,
) -> Result<Direct3D11CaptureFramePool> {
    let statics: IDirect3D11CaptureFramePoolStatics2 =
        factory::<Direct3D11CaptureFramePool, IDirect3D11CaptureFramePoolStatics2>()
            .context("could not load native WGC frame-pool statics")?;
    let mut result = std::ptr::null_mut();
    // SAFETY: both COM interfaces are live in this worker MTA; `result` is a
    // valid out pointer, and Type::from_abi consumes the returned reference
    // only after the HRESULT reports success.
    unsafe {
        (Interface::vtable(&statics).CreateFreeThreaded)(
            Interface::as_raw(&statics),
            Interface::as_raw(device.winrt_device()),
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            NATIVE_WGC_FRAME_POOL_CAPACITY,
            size,
            &mut result,
        )
        .and_then(|| Type::from_abi(result))
        .context("could not create free-threaded native WGC frame pool")
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

    #[test]
    fn latest_pending_policy_selects_the_freshest_source_at_sixty_hz() {
        const NANOS_PER_SECOND: u128 = 1_000_000_000;
        const OUTPUT_RATE: u128 = 60;
        const OUTPUT_TICKS: u64 = 60;

        for source_rate in [60_u64, 144, 240] {
            let mut pending = LatestPending::default();
            let mut source_index = 1_u64;
            let mut arrivals = 1_u64;
            let mut snapshot_copies = 1_u64;
            let mut observed_replacements = 0_u64;

            for tick_index in 1..OUTPUT_TICKS {
                let tick_nanos = u128::from(tick_index) * NANOS_PER_SECOND / OUTPUT_RATE;
                while u128::from(source_index) * NANOS_PER_SECOND / u128::from(source_rate)
                    <= tick_nanos
                {
                    let source_nanos =
                        u128::from(source_index) * NANOS_PER_SECOND / u128::from(source_rate);
                    if pending.replace((source_index, source_nanos)) {
                        observed_replacements = observed_replacements.saturating_add(1);
                    }
                    arrivals = arrivals.saturating_add(1);
                    source_index = source_index.saturating_add(1);
                }

                let (selected_index, selected_nanos) =
                    pending.take().expect("each output tick has a source");
                assert_eq!(
                    selected_index,
                    source_index - 1,
                    "source rate {source_rate} did not select its freshest eligible source"
                );
                assert!(
                    tick_nanos.saturating_sub(selected_nanos) <= NANOS_PER_SECOND / OUTPUT_RATE,
                    "source rate {source_rate} exceeded one output interval of source age"
                );
                snapshot_copies = snapshot_copies.saturating_add(1);
            }

            assert_eq!(pending.high_water_mark, 1, "source rate {source_rate}");
            assert_eq!(
                pending.replacements, observed_replacements,
                "source rate {source_rate} replacement accounting differs"
            );
            assert_eq!(
                pending.replacements,
                arrivals.saturating_sub(snapshot_copies),
                "source rate {source_rate} did not coalesce every excess admitted source"
            );
            assert!(snapshot_copies <= OUTPUT_TICKS + 1);
        }
    }

    #[test]
    fn replacing_pending_ownership_drops_the_older_frame_immediately() {
        struct DropCounter(Arc<AtomicU64>);

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let drops = Arc::new(AtomicU64::new(0));
        let mut pending = LatestPending::default();
        assert!(!pending.replace(DropCounter(Arc::clone(&drops))));
        assert_eq!(drops.load(Ordering::Relaxed), 0);

        assert!(pending.replace(DropCounter(Arc::clone(&drops))));
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert_eq!(pending.high_water_mark, 1);

        drop(pending.take());
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn callback_quiescence_is_notified_without_polling() {
        let state = Arc::new(FrameCallbackState::default());
        let worker_state = Arc::clone(&state);
        let (started_sender, started_receiver) = sync_channel(0);
        let worker = std::thread::spawn(move || {
            let _activity = worker_state.enter().expect("callback should enter");
            started_sender.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(30));
        });
        started_receiver.recv().unwrap();
        assert!(state.wait_until_idle(Duration::from_secs(1)));
        worker.join().unwrap();
    }

    #[test]
    fn registered_callback_owner_release_wakes_shutdown() {
        let state = Arc::new(FrameCallbackState::default());
        let owner = FrameCallbackOwner::new(&state);
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            drop(owner);
        });
        assert!(state.wait_until_released(Duration::from_secs(1)));
        worker.join().unwrap();
    }
}

