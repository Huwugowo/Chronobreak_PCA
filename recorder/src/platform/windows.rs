use std::sync::atomic::{AtomicU64, Ordering};

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

use super::{CaptureSource, CaptureTarget};

static TARGET_GENERATION: AtomicU64 = AtomicU64::new(1);

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
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
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
    let CaptureSource::WindowsGraphicsCapture { hwnd, .. } = &target.source else {
        return Ok(());
    };
    let hwnd = HWND(*hwnd as usize as *mut _);
    if !unsafe { IsWindowVisible(hwnd).as_bool() } || unsafe { IsIconic(hwnd).as_bool() } {
        anyhow::bail!("the selected League HWND is hidden or minimized");
    }
    Ok(())
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
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        anyhow::bail!("the selected League HWND was closed");
    }
    let mut current_pid = 0_u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut current_pid));
    }
    if current_pid != *pid {
        anyhow::bail!("the selected League HWND now belongs to a different process");
    }
    // A minimized/temporarily hidden exact window remains the same capture
    // identity. WGC may pause frames and resume after restore, so defer bounds
    // and monitor checks until the HWND is visible again.
    if unsafe { IsIconic(hwnd).as_bool() } || !unsafe { IsWindowVisible(hwnd).as_bool() } {
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

pub fn instant_from_qpc_100ns(timestamp: i64) -> Result<std::time::Instant> {
    if timestamp <= 0 {
        anyhow::bail!("WGC first-frame QPC timestamp is not positive");
    }
    let mut counter = 0_i64;
    let mut frequency = 0_i64;
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
    let delta_100ns = now_100ns
        .checked_sub(i128::from(timestamp))
        .context("WGC first-frame timestamp is ahead of the local QPC clock")?;
    if delta_100ns < 0 {
        anyhow::bail!("WGC first-frame timestamp is ahead of the local QPC clock");
    }
    let delta_100ns =
        u64::try_from(delta_100ns).context("WGC first-frame timestamp delta is too large")?;
    let elapsed = std::time::Duration::from_nanos(
        delta_100ns
            .checked_mul(100)
            .context("WGC first-frame timestamp delta overflowed")?,
    );
    std::time::Instant::now()
        .checked_sub(elapsed)
        .context("WGC first-frame timestamp predates the process monotonic clock")
}

fn find_largest_window(pid: u32) -> Result<Option<WindowCandidate>> {
    let mut context = SearchContext { pid, best: None };
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
    let context = unsafe { &mut *(lparam.0 as *mut SearchContext) };
    if !unsafe { IsWindowVisible(hwnd).as_bool() } || unsafe { IsIconic(hwnd).as_bool() } {
        return BOOL(1);
    }

    let mut window_pid = 0_u32;
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
    let context = unsafe { &mut *(lparam.0 as *mut MonitorSearchContext) };
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
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.context("could not create the DXGI adapter factory")?;
    let mut adapter_index = 0_u32;
    loop {
        let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error).context("could not enumerate DXGI adapters"),
        };
        let adapter_description =
            unsafe { adapter.GetDesc1() }.context("could not inspect a DXGI adapter")?;
        let mut output_index = 0_u32;
        loop {
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(error).context("could not enumerate DXGI outputs"),
            };
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
    unsafe { GetClientRect(hwnd, &mut rect) }.ok()?;

    let mut top_left = POINT {
        x: rect.left,
        y: rect.top,
    };
    let mut bottom_right = POINT {
        x: rect.right,
        y: rect.bottom,
    };
    if !unsafe { ClientToScreen(hwnd, &mut top_left).as_bool() }
        || !unsafe { ClientToScreen(hwnd, &mut bottom_right).as_bool() }
    {
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
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0_u16; length as usize + 1];
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
