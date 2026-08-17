//! Native Windows recorder engine under QB-PERF-005.
//!
//! This module is intentionally not wired into the production service yet.
//! Milestones 1-2 establish the real same-adapter D3D11 + exact-HWND WGC
//! source. Conversion, direct NVENC and mux/session integration layer on top of
//! this boundary without changing target discovery or League lifecycle code.

mod capture;
mod convert;
mod d3d11;
mod encode;
mod mux;
mod nvenc;
mod source;
mod winrt;

pub use capture::{
    CapturedWgcFrame, NativeWgcCallbackError, NativeWgcCallbackErrorCategory,
    NativeWgcTelemetrySnapshot,
};
pub use convert::{ConvertedNv12Frame, NativeNv12Converter, NativeNv12TelemetrySnapshot};
pub use encode::{NativeNvencEncoder, NativeNvencTelemetrySnapshot};
pub use mux::NativeMuxPlan;
pub use nvenc::{NvencApiVersion, NvencDriverProbe, NvencH264Capability};
pub use source::NativeWgcSource;

/// WGC owns two source frames. The callback must never extend this into an
/// unbounded software queue.
pub const NATIVE_WGC_FRAME_POOL_CAPACITY: i32 = 2;

/// Exactly one captured frame may wait between WGC and the GPU worker.
pub const NATIVE_WGC_HANDOFF_CAPACITY: usize = 1;

/// Milestone 3/4 reserves exactly four encoder-facing NV12/NVENC slots.
pub const NATIVE_ENCODER_SLOT_COUNT: usize = 4;

