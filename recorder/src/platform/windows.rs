use anyhow::{Context, Result};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetSystemMetrics, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SM_CXSCREEN, SM_CYSCREEN,
};
use windows::core::BOOL;

use super::{CaptureSource, CaptureTarget};

#[derive(Debug)]
struct WindowCandidate {
    area: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    title: Option<String>,
}

struct SearchContext {
    pid: u32,
    best: Option<WindowCandidate>,
}

pub fn capture_target_for_process(pid: u32) -> Result<CaptureTarget> {
    if let Some(candidate) = find_largest_window(pid)? {
        return Ok(CaptureTarget {
            source: CaptureSource::DesktopRegion {
                x: candidate.x,
                y: candidate.y,
                width: even(candidate.width),
                height: even(candidate.height),
                window_title: candidate.title,
            },
        });
    }

    fallback_capture_target()
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
        x,
        y,
        width,
        height,
        title: window_title(hwnd),
    });
    BOOL(1)
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
}
