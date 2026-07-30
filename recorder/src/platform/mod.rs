#[cfg(not(target_os = "windows"))]
use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSource {
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

impl CaptureTarget {
    pub fn description(&self) -> String {
        match &self.source {
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
            CaptureSource::DesktopRegion {
                window_title: Some(_),
                ..
            }
        )
    }
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{capture_target_for_process, fallback_capture_target};

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
