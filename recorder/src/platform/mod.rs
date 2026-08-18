#[cfg(not(target_os = "windows"))]
use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
    WindowsGraphicsCapture {
        pid: u32,
        generation: u64,
        hwnd: u64,
        width: u32,
        height: u32,
        dpi: u32,
        window_title: Option<String>,
        adapter_index: u32,
        adapter_luid: u64,
        adapter_name: String,
        output_name: String,
    },
    DesktopRegion {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        window_title: Option<String>,
    },
    AvFoundation {
        input: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTarget {
    pub source: CaptureSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTargetVisibility {
    Visible,
    PausedByWindowVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureTargetState {
    pub visibility: CaptureTargetVisibility,
    pub adapter_validated: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureTargetStateCache {
    #[cfg(target_os = "windows")]
    last_visible_bounds: Option<WindowsVisibleBounds>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsVisibleBounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
impl CaptureTargetStateCache {
    fn adapter_validation_required(&self, bounds: WindowsVisibleBounds) -> bool {
        self.last_visible_bounds != Some(bounds)
    }

    fn observe_validated_bounds(&mut self, bounds: WindowsVisibleBounds) {
        self.last_visible_bounds = Some(bounds);
    }
}

impl CaptureTarget {
    pub fn description(&self) -> String {
        match &self.source {
            CaptureSource::WindowsGraphicsCapture {
                pid,
                generation,
                hwnd,
                width,
                height,
                dpi,
                window_title,
                adapter_index,
                adapter_luid,
                adapter_name,
                output_name,
            } => format!(
                "PID {pid} generation {generation} window {window_title:?} HWND 0x{hwnd:x}, size {width}x{height} at {dpi} DPI, DXGI adapter {adapter_index} {adapter_name:?} LUID {adapter_luid:016x}, output {output_name:?}"
            ),
            CaptureSource::DesktopRegion {
                x,
                y,
                width,
                height,
                window_title,
            } => match window_title {
                Some(title) => {
                    format!("window {title:?} at {x},{y}, size {width}x{height}")
                }
                None => format!("primary display at {x},{y}, size {width}x{height}"),
            },
            CaptureSource::AvFoundation { input } => {
                format!("AVFoundation input {input:?}")
            }
        }
    }

    pub fn is_window_region(&self) -> bool {
        matches!(
            self.source,
            CaptureSource::WindowsGraphicsCapture { .. }
                | CaptureSource::DesktopRegion {
                    window_title: Some(_),
                    ..
                }
        )
    }

    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match self.source {
            CaptureSource::WindowsGraphicsCapture { width, height, .. } => Some((width, height)),
            CaptureSource::DesktopRegion { width, height, .. } => Some((width, height)),
            CaptureSource::AvFoundation { .. } => None,
        }
    }

    pub fn windows_hwnd(&self) -> Option<u64> {
        match self.source {
            CaptureSource::WindowsGraphicsCapture { hwnd, .. } => Some(hwnd),
            _ => None,
        }
    }

    pub fn windows_adapter_index(&self) -> Option<u32> {
        match self.source {
            CaptureSource::WindowsGraphicsCapture { adapter_index, .. } => Some(adapter_index),
            _ => None,
        }
    }

    pub fn windows_adapter_luid(&self) -> Option<u64> {
        match self.source {
            CaptureSource::WindowsGraphicsCapture { adapter_luid, .. } => Some(adapter_luid),
            _ => None,
        }
    }

    pub fn windows_adapter_name(&self) -> Option<&str> {
        match &self.source {
            CaptureSource::WindowsGraphicsCapture { adapter_name, .. } => Some(adapter_name),
            _ => None,
        }
    }

    pub fn windows_output_name(&self) -> Option<&str> {
        match &self.source {
            CaptureSource::WindowsGraphicsCapture { output_name, .. } => Some(output_name),
            _ => None,
        }
    }
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{
    capture_target_for_process, capture_target_visibility, fallback_capture_target,
    instant_from_qpc_100ns, query_capture_target_state, validate_capture_target,
    validate_capture_target_identity,
};

#[cfg(not(target_os = "windows"))]
pub fn validate_capture_target(_target: &CaptureTarget) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn validate_capture_target_identity(_target: &CaptureTarget) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn capture_target_visibility(
    _target: &CaptureTarget,
) -> anyhow::Result<CaptureTargetVisibility> {
    Ok(CaptureTargetVisibility::Visible)
}

#[cfg(not(target_os = "windows"))]
pub fn query_capture_target_state(
    _target: &CaptureTarget,
    _cache: &mut CaptureTargetStateCache,
) -> anyhow::Result<CaptureTargetState> {
    Ok(CaptureTargetState {
        visibility: CaptureTargetVisibility::Visible,
        adapter_validated: false,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn instant_from_qpc_100ns(_timestamp: i64) -> anyhow::Result<std::time::Instant> {
    Ok(std::time::Instant::now())
}

#[cfg(target_os = "macos")]
pub fn capture_target_for_process(_pid: u32) -> Result<CaptureTarget> {
    let input = std::env::var("LEAGUE_REPLAY_AVFOUNDATION_INPUT")
        .unwrap_or_else(|_| "1:default".to_owned());
    Ok(CaptureTarget {
        source: CaptureSource::AvFoundation { input },
    })
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn capture_target_for_process(_pid: u32) -> Result<CaptureTarget> {
    anyhow::bail!("League Replay Recorder supports only Windows and macOS")
}

#[cfg(target_os = "macos")]
pub fn fallback_capture_target() -> Result<CaptureTarget> {
    capture_target_for_process(0)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn fallback_capture_target() -> Result<CaptureTarget> {
    anyhow::bail!("League Replay Recorder supports only Windows and macOS")
}
