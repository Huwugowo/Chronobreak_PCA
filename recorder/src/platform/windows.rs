use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ERROR_NOT_FOUND, IDXGIFactory1};
use windows::Win32::Graphics::Gdi::{ClientToScreen, EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetSystemMetrics, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SM_CXSCREEN, SM_CYSCREEN,
};
use windows::core::BOOL;

use super::{CaptureSource, CaptureTarget, CaptureTargetVisibility};

static TARGET_GENERATION: AtomicU64 = AtomicU64::new(1);
const MAX_WGC_FUTURE_QPC_SKEW_100NS: i128 = 1_000_000; // 100 ms.

#[derive(Debug)]
struct WindowCandidate {
    area: u64,
    hwnd: HWND,
    width: u32,
    height: u32,
    screen_rect: RECT,
    title: Option<String>,
}

#[derive(Debug)]
struct AdapterSelection {
    index: u32,
    luid: u64,
    name: String,
    output_name: String,
}

struct SearchContext {
    pid: u32,
    best: Option<WindowCandidate>,
}

struct MonitorSearchContext {
    target: RECT,
    best: Option<(u64, HMONITOR)>,
}

pub fn capture_target_for_process(pid: u32) -> Result<CaptureTarget> {
    if let Some(candidate) = find_largest_window(pid)? {
        let monitor = monitor_with_largest_intersection(candidate.screen_rect)?;
        let adapter = adapter_for_monitor(monitor)?;
        // SAFETY: `candidate.hwnd` is an opaque handle supplied by the current
        // synchronous EnumWindows pass; this query neither owns nor closes it.
        let dpi = unsafe { GetDpiForWindow(candidate.hwnd) };
        if dpi == 0 {
            anyhow::bail!("could not determine League window DPI");
        }
        return Ok(CaptureTarget {
            source: CaptureSource::WindowsGraphicsCapture {
                pid,
                generation: TARGET_GENERATION.fetch_add(1, Ordering::Relaxed),
                hwnd: candidate.hwnd.0 as usize as u64,
                width: even(candidate.width),
                height: even(candidate.height),
                dpi,
                window_title: candidate.title,
                adapter_index: adapter.index,
                adapter_luid: adapter.luid,
                adapter_name: adapter.name,
                output_name: adapter.output_name,
            },
        });
    }

    anyhow::bail!("could not find a visible League window for PID {pid}")
}

pub fn fallback_capture_target() -> Result<CaptureTarget> {
    // SAFETY: GetSystemMetrics takes value-only metric identifiers and does not
    // read caller-provided memory or transfer ownership.
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if width <= 0 || height <= 0 {
        anyhow::bail!("could not determine a primary display capture size");
    }

    Ok(CaptureTarget {
        source: CaptureSource::DesktopRegion {
            x: 0,
            y: 0,
            width: even(width as u32),
            height: even(height as u32),
            window_title: None,
        },
    })
}

pub fn validate_capture_target(target: &CaptureTarget) -> Result<()> {
    validate_capture_target_identity(target)?;
    if capture_target_visibility(target)? == CaptureTargetVisibility::PausedByWindowVisibility {
        anyhow::bail!("the selected League HWND is hidden or minimized");
    }
    Ok(())
}

pub fn capture_target_visibility(target: &CaptureTarget) -> Result<CaptureTargetVisibility> {
    let CaptureSource::WindowsGraphicsCapture { hwnd, .. } = &target.source else {
        return Ok(CaptureTargetVisibility::Visible);
    };
    let hwnd = HWND(*hwnd as usize as *mut _);
    // SAFETY: `hwnd` is used only as an opaque Win32 handle. These predicates
    // do not dereference application memory or retain the handle.
    let (exists, minimized, visible) = unsafe {
        (
            IsWindow(Some(hwnd)).as_bool(),
            IsIconic(hwnd).as_bool(),
            IsWindowVisible(hwnd).as_bool(),
        )
    };
    if !exists {
        anyhow::bail!("the selected League HWND was closed");
    }
    if minimized || !visible {
        return Ok(CaptureTargetVisibility::PausedByWindowVisibility);
    }
    Ok(CaptureTargetVisibility::Visible)
}

pub fn validate_capture_target_identity(target: &CaptureTarget) -> Result<()> {
    let CaptureSource::WindowsGraphicsCapture {
        pid,
        hwnd,
        adapter_luid,
        ..
    } = &target.source
    else {
        return Ok(());
    };
    let hwnd = HWND(*hwnd as usize as *mut _);
    // SAFETY: `hwnd` is an opaque, non-owned handle and IsWindow performs the
    // validity query without dereferencing caller memory.
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        anyhow::bail!("the selected League HWND was closed");
    }
    let mut current_pid = 0_u32;
    // SAFETY: `current_pid` is a live, aligned u32 for the duration of this
    // call, and `hwnd` is passed only as a non-owned opaque handle.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut current_pid));
    }
    if current_pid != *pid {
        anyhow::bail!("the selected League HWND now belongs to a different process");
    }
    // A minimized/temporarily hidden exact window remains the same capture
    // identity. WGC may pause frames and resume after restore, so defer bounds
    // and monitor checks until the HWND is visible again.
    if capture_target_visibility(target)? == CaptureTargetVisibility::PausedByWindowVisibility {
        return Ok(());
    }
    let (x, y, width, height) = client_bounds(hwnd).context("League HWND has no client area")?;
    let current_monitor = monitor_with_largest_intersection(RECT {
        left: x,
        top: y,
        right: x.saturating_add(width as i32),
        bottom: y.saturating_add(height as i32),
    })?;
    let current_adapter = adapter_for_monitor(current_monitor)?;
    if current_adapter.luid != *adapter_luid {
        anyhow::bail!(
            "the League window moved to DXGI adapter {:016x}; rediscovery is required",
            current_adapter.luid
        );
    }
    Ok(())
}

pub fn instant_from_qpc_100ns(timestamp: i64) -> Result<Instant> {
    if timestamp <= 0 {
        anyhow::bail!("WGC first-frame QPC timestamp is not positive");
    }
    let mut counter = 0_i64;
    let mut frequency = 0_i64;
    // SAFETY: both output pointers refer to distinct, live, aligned i64 values
    // that remain valid for the complete duration of each synchronous call.
    unsafe {
        QueryPerformanceCounter(&mut counter)
            .context("could not read QueryPerformanceCounter for the WGC clock anchor")?;
        QueryPerformanceFrequency(&mut frequency)
            .context("could not read QueryPerformanceFrequency for the WGC clock anchor")?;
    }
    if counter <= 0 || frequency <= 0 {
        anyhow::bail!("Windows returned an invalid performance-counter clock");
    }
    let now_100ns = i128::from(counter)
        .checked_mul(10_000_000)
        .and_then(|value| value.checked_div(i128::from(frequency)))
        .context("WGC performance-counter conversion overflowed")?;
    instant_from_qpc_sample(timestamp, now_100ns, Instant::now())
}

fn instant_from_qpc_sample(timestamp: i64, now_100ns: i128, now: Instant) -> Result<Instant> {
    let delta_100ns = now_100ns
        .checked_sub(i128::from(timestamp))
        .context("WGC first-frame timestamp delta overflowed")?;
    if delta_100ns >= 0 {
        let elapsed = duration_from_100ns(delta_100ns)?;
        return now
            .checked_sub(elapsed)
            .context("WGC first-frame timestamp predates the process monotonic clock");
    }

    let future_100ns = delta_100ns
        .checked_neg()
        .context("WGC future timestamp delta overflowed")?;
    if future_100ns > MAX_WGC_FUTURE_QPC_SKEW_100NS {
        anyhow::bail!(
            "WGC first-frame timestamp is implausibly ahead of the local QPC clock by {future_100ns} 100-ns ticks"
        );
    }
    now.checked_add(duration_from_100ns(future_100ns)?)
        .context("WGC first-frame timestamp exceeds the process monotonic clock range")
}

fn duration_from_100ns(ticks: i128) -> Result<Duration> {
    let ticks = u64::try_from(ticks).context("WGC timestamp delta is too large")?;
    Ok(Duration::from_nanos(
        ticks
            .checked_mul(100)
            .context("WGC timestamp delta overflowed")?,
    ))
}

fn find_largest_window(pid: u32) -> Result<Option<WindowCandidate>> {
    let mut context = SearchContext { pid, best: None };
    // SAFETY: EnumWindows invokes `enum_window` synchronously on this thread.
    // `context` stays live and exclusively borrowed until enumeration returns,
    // and the callback does not retain its LPARAM pointer.
    unsafe {
        EnumWindows(
            Some(enum_window),
            LPARAM((&mut context as *mut SearchContext) as isize),
        )
        .context("EnumWindows failed while locating the League window")?;
    }
    Ok(context.best)
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: the only caller encodes a live, exclusively borrowed
    // `SearchContext` in LPARAM for the duration of synchronous enumeration.
    let context = unsafe { &mut *(lparam.0 as *mut SearchContext) };
    // SAFETY: EnumWindows supplied `hwnd`; both calls are non-owning queries on
    // that opaque handle and do not retain it.
    let (visible, minimized) =
        unsafe { (IsWindowVisible(hwnd).as_bool(), IsIconic(hwnd).as_bool()) };
    if !visible || minimized {
        return BOOL(1);
    }

    let mut window_pid = 0_u32;
    // SAFETY: `window_pid` is live and aligned for this synchronous output,
    // while `hwnd` was supplied by the active EnumWindows callback.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
    }
    if window_pid != context.pid {
        return BOOL(1);
    }

    let Some((x, y, width, height)) = client_bounds(hwnd) else {
        return BOOL(1);
    };
    if width < 320 || height < 200 {
        return BOOL(1);
    }

    let area = u64::from(width) * u64::from(height);
    if context
        .best
        .as_ref()
        .is_some_and(|candidate| candidate.area >= area)
    {
        return BOOL(1);
    }

    context.best = Some(WindowCandidate {
        area,
        hwnd,
        width,
        height,
        screen_rect: RECT {
            left: x,
            top: y,
            right: x.saturating_add(width as i32),
            bottom: y.saturating_add(height as i32),
        },
        title: window_title(hwnd),
    });
    BOOL(1)
}

fn monitor_with_largest_intersection(target: RECT) -> Result<HMONITOR> {
    let mut context = MonitorSearchContext { target, best: None };
    // SAFETY: EnumDisplayMonitors calls the callback synchronously. `context`
    // remains live and exclusively borrowed until it returns, and neither a
    // device context nor clipping rectangle is supplied by the caller.
    unsafe {
        if !EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_intersection),
            LPARAM((&mut context as *mut MonitorSearchContext) as isize),
        )
        .as_bool()
        {
            anyhow::bail!("could not enumerate monitors for the League window");
        }
    }
    context
        .best
        .map(|(_, monitor)| monitor)
        .context("the League client area does not intersect an attached monitor")
}

unsafe extern "system" fn enum_monitor_intersection(
    monitor: HMONITOR,
    _device: HDC,
    bounds: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    if bounds.is_null() {
        return BOOL(1);
    }
    // SAFETY: the caller passes a live, exclusively borrowed context through
    // LPARAM for synchronous enumeration; the callback never retains it.
    let context = unsafe { &mut *(lparam.0 as *mut MonitorSearchContext) };
    // SAFETY: EnumDisplayMonitors guarantees that non-null `bounds` points to
    // a readable RECT for the duration of this callback.
    let area = intersection_area(context.target, unsafe { *bounds });
    if area > 0
        && context
            .best
            .as_ref()
            .is_none_or(|(best_area, _)| area > *best_area)
    {
        context.best = Some((area, monitor));
    }
    BOOL(1)
}

fn intersection_area(left: RECT, right: RECT) -> u64 {
    let width = left.right.min(right.right) - left.left.max(right.left);
    let height = left.bottom.min(right.bottom) - left.top.max(right.top);
    if width <= 0 || height <= 0 {
        0
    } else {
        (width as u64).saturating_mul(height as u64)
    }
}

fn adapter_for_monitor(monitor: HMONITOR) -> Result<AdapterSelection> {
    // SAFETY: the Windows binding initializes and returns an owned, reference-
    // counted IDXGIFactory1 interface; no raw caller pointer is supplied.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.context("could not create the DXGI adapter factory")?;
    let mut adapter_index = 0_u32;
    loop {
        // SAFETY: `factory` owns a valid COM interface pointer, and the binding
        // returns an owned adapter wrapper or an HRESULT for this value index.
        let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error).context("could not enumerate DXGI adapters"),
        };
        // SAFETY: `adapter` is a live owned COM wrapper and the binding returns
        // the fixed-size description by value without retaining Rust memory.
        let adapter_description =
            unsafe { adapter.GetDesc1() }.context("could not inspect a DXGI adapter")?;
        let mut output_index = 0_u32;
        loop {
            // SAFETY: `adapter` remains live for the call, and the binding
            // returns an owned output wrapper or an HRESULT for the value index.
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(error).context("could not enumerate DXGI outputs"),
            };
            // SAFETY: `output` is a live owned COM wrapper and the binding
            // writes its fixed-size description into binding-managed storage.
            let output_description =
                unsafe { output.GetDesc() }.context("could not inspect a DXGI output")?;
            if output_description.Monitor.0 == monitor.0
                && output_description.AttachedToDesktop.as_bool()
            {
                return Ok(AdapterSelection {
                    index: adapter_index,
                    luid: luid_value(
                        adapter_description.AdapterLuid.LowPart,
                        adapter_description.AdapterLuid.HighPart,
                    ),
                    name: wide_string(&adapter_description.Description),
                    output_name: wide_string(&output_description.DeviceName),
                });
            }
            output_index = output_index.saturating_add(1);
        }
        adapter_index = adapter_index.saturating_add(1);
    }

    anyhow::bail!("the League window monitor is not attached to an enumerated DXGI adapter")
}

fn luid_value(low: u32, high: i32) -> u64 {
    ((high as u32 as u64) << 32) | u64::from(low)
}

fn wide_string(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

fn client_bounds(hwnd: HWND) -> Option<(i32, i32, u32, u32)> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a live, aligned output value and `hwnd` is queried only
    // as a non-owned opaque handle for the duration of this synchronous call.
    unsafe { GetClientRect(hwnd, &mut rect) }.ok()?;

    let mut top_left = POINT {
        x: rect.left,
        y: rect.top,
    };
    let mut bottom_right = POINT {
        x: rect.right,
        y: rect.bottom,
    };
    // SAFETY: both POINT values are distinct, live, aligned in/out buffers and
    // `hwnd` remains a non-owned opaque handle. The calls are synchronous.
    let converted = unsafe {
        ClientToScreen(hwnd, &mut top_left).as_bool()
            && ClientToScreen(hwnd, &mut bottom_right).as_bool()
    };
    if !converted {
        return None;
    }

    let width = bottom_right.x.checked_sub(top_left.x)?;
    let height = bottom_right.y.checked_sub(top_left.y)?;
    if width <= 0 || height <= 0 {
        return None;
    }

    Some((top_left.x, top_left.y, width as u32, height as u32))
}

fn window_title(hwnd: HWND) -> Option<String> {
    // SAFETY: this is a non-owning length query on the opaque HWND and does not
    // read or retain any caller-provided buffer.
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize + 1];
    // SAFETY: the Windows slice binding receives the full initialized buffer;
    // it can write at most its length, and the buffer remains live for the call.
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn even(value: u32) -> u32 {
    value.saturating_sub(value % 2).max(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_capture_dimensions_down_to_even_values() {
        assert_eq!(even(1921), 1920);
        assert_eq!(even(1080), 1080);
        assert_eq!(even(1), 2);
    }

    #[test]
    fn combines_signed_dxgi_luid_without_losing_bits() {
        assert_eq!(luid_value(0x89ab_cdef, -2), 0xffff_fffe_89ab_cdef);
    }

    #[test]
    fn qpc_anchor_accepts_one_compositor_interval_of_future_skew() {
        let now = Instant::now();
        let past = instant_from_qpc_sample(9_900_000, 10_000_000, now).unwrap();
        assert_eq!(past, now - Duration::from_millis(10));

        let future = instant_from_qpc_sample(10_151_036, 10_000_000, now).unwrap();
        assert_eq!(future, now + Duration::from_nanos(15_103_600));

        assert!(
            instant_from_qpc_sample(
                10_000_000 + MAX_WGC_FUTURE_QPC_SKEW_100NS as i64 + 1,
                10_000_000,
                now,
            )
            .is_err()
        );
    }

    #[test]
    fn monitor_selection_math_handles_negative_coordinates_and_partial_overlap() {
        let window = RECT {
            left: -400,
            top: 100,
            right: 600,
            bottom: 700,
        };
        let left_monitor = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let primary = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert_eq!(intersection_area(window, left_monitor), 240_000);
        assert_eq!(intersection_area(window, primary), 360_000);
        assert_eq!(
            intersection_area(
                window,
                RECT {
                    left: 2000,
                    top: 0,
                    right: 3000,
                    bottom: 1000,
                }
            ),
            0
        );
    }
}
