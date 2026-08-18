//! Native Windows recorder engine under QB-PERF-005.
//!
//! The production service can select this developer-gated backend while the
//! FFmpeg reference path remains available. It keeps capture, conversion,
//! direct NVENC, muxing, and lifecycle ownership inside the exact-HWND,
//! same-adapter boundary without changing League target discovery.

mod capture;
mod clock;
mod convert;
mod d3d11;
mod encode;
mod lifecycle;
mod mux;
mod nvenc;
mod session;
mod source;
mod winrt;

pub use capture::{
    CapturedWgcFrame, NativeWgcCallbackError, NativeWgcCallbackErrorCategory,
    NativeWgcTelemetrySnapshot,
};
pub use clock::{NativeCfrClock, NativeCfrTelemetrySnapshot, NativeCfrTick};
pub use convert::{ConvertedNv12Frame, NativeNv12Converter, NativeNv12TelemetrySnapshot};
pub use encode::{NativeNvencEncoder, NativeNvencTelemetrySnapshot};
pub(crate) use lifecycle::{NativeRecordingSession, NativeRecordingStartupFailureDisposition};
pub use mux::{NativeMuxPlan, NativeMuxProcess, NativeMuxTelemetrySnapshot};
pub use nvenc::{NvencApiVersion, NvencDriverProbe, NvencH264Capability};
pub use session::{NativeRecorderSession, NativeSessionTelemetrySnapshot};
pub use source::NativeWgcSource;

/// WGC owns two source frames. The callback must never extend this into an
/// unbounded software queue.
pub const NATIVE_WGC_FRAME_POOL_CAPACITY: i32 = 2;

/// Exactly one captured frame may wait between WGC and the GPU worker.
pub const NATIVE_WGC_HANDOFF_CAPACITY: usize = 1;

/// Milestone 3/4 reserves exactly four encoder-facing NV12/NVENC slots.
pub const NATIVE_ENCODER_SLOT_COUNT: usize = 4;

