# Windows capture architecture

QueueBack targets the exact League of Legends top-level HWND with Windows Graphics Capture (WGC). It does not capture the desktop, inject into League, hook DirectX, intercept input, or modify League settings. The optimized paths require Windows 10 version 1903 or newer and the pinned QueueBack media runtime.

## Backend selection

The in-process native backend is the Windows default. At process startup, `QUEUEBACK_WINDOWS_RECORDER_BACKEND` selects the backend:

| Value | Backend |
| --- | --- |
| unset, empty, or `native` | `native-wgc-d3d11-nvenc` |
| `ffmpeg` or `ffmpeg-wgc` | `ffmpeg-wgc` |

An invalid value is a startup error. A native startup failure never silently switches to FFmpeg: operators must select the fallback explicitly so diagnostics and performance claims remain truthful.

The native backend currently supports H.264, the high 1920x1080 60-FPS plan, and NVIDIA NVENC. The external FFmpeg backend remains the compatibility path for HEVC, other recording profiles, AMD/AMF, and Intel/QSV.

## Default native frame path

```text
League HWND
  -> WGC free-threaded frame pool (2 D3D11 BGRA frames)
  -> nonblocking callback-to-worker latest-frame handoff (capacity 1)
  -> one D3D11 GPU worker and one pending source frame
  -> D3D11 video-processor resize and BGRA-to-NV12 conversion
  -> four owned NV12/NVENC slots
  -> direct NVENC H.264 submission and bounded completion thread
  -> FFmpeg video-bitstream pipe + DirectShow/silent AAC audio
  -> fragmented MP4
```

The WGC callback never waits for the GPU worker, encoder, muxer, disk, or runtime work. Newer surfaced frames supersede older unconsumed frames and increment a separate counter. Tick admission is transactional: a CFR tick commits only after successful NVENC submission. Scheduled, submitted, completed, and muxed frame accounting is reconciled before a successful final result.

Full video frames remain GPU-resident through acquisition, resize/color conversion, and encoder submission. There is no per-frame full-surface readback, CPU pixel conversion, or raw-frame transfer. WGC, conversion, and encoder surfaces may still require bounded GPU-to-GPU copies, so the path is described as GPU-resident rather than universally zero-copy.

FFmpeg remains one supervised child, but on the native path it does not capture or encode video. It receives the native H.264 bitstream, captures the selected loopback or silent audio source, encodes AAC, and owns fragmented-MP4 muxing. Pipe-write and explicit-flush latency are measured separately, including deterministic test-only stall injection.

## Explicit FFmpeg fallback

The retained `ffmpeg-wgc` backend keeps the vendor-neutral graph:

```text
League HWND
  -> WGC / gfxcapture D3D11 BGRA frames
  -> bounded latest-frame selection
  -> scale_d3d11 NV12 conversion
  -> same-adapter NVENC, AMF, or direct-mapped QSV
  -> AAC audio + fragmented MP4
```

The external graph uses two WGC acquisition frames, eight gfxcapture BGRA output frames, a global 32-frame filter bound, and encoder depth 16 for NVENC or 4 for AMF/QSV. Capture and encoder adapter LUIDs must match. A missing encoder, failed direct map, or cross-adapter combination rejects that candidate rather than enabling a hidden host copy.

## Runtime contract

Both backends resolve `queueback-ffmpeg-8.1.2-windows-x86_64-r6` through the shared immutable media-runtime contract. Startup verifies file hashes, exact version/build identity, FFmpeg/ffprobe pairing, `gfxcapture`, `scale_d3d11`, D3D11 support, compiled NVENC/AMF/QSV surfaces, and QueueBack capture diagnostics ABI 1. An arbitrary executable on `PATH` is not accepted.

The r6 build pins FFmpeg 8.1.2, nv-codec-headers 12.2.72.0, AMF 1.4.36, the QueueBack WGC patch, and its exact MSYS2 toolchain. It enables ffnvcodec and links the static C++ runtime required by the current libvpl package. The whole-app build and staging scripts consume the same lock; ordinary Cargo verification does not download or rebuild FFmpeg.

The fragmented MP4 muxer flushes completed packets. Successful recording publishes only a validated private candidate as `video.mp4`. Failed startup or active-session failure preserves a recoverable partial candidate and an actionable error rather than claiming canonical success.

## Ownership, timing, and lifecycle

Native ownership is explicit and bounded. A lifecycle owner supervises the GPU worker; cooperative startup cancellation and stop execute blocking joins through Tokio's blocking pool. A fail-safe Drop join remains for invariant-breaking unwinds. An in-process worker cannot forcibly terminate a GPU-driver call that never returns, so hard driver-hang containment remains an open supervised-process boundary rather than a solved claim.

Startup is ready only after a real WGC frame with a positive `SystemRelativeTime`/QPC value, encoded output, mux progress, and a positive output timestamp. The first WGC QPC timestamp maps to Rust monotonic and wall clocks and becomes both the recording epoch and Live Client poller anchor. Process-spawn time is not the video epoch.

Capture diagnostics separately expose surfaced and superseded source frames, CFR discards and duplications, encoded/completed/muxed frames, pool recreations, bounded depths, first/latest QPC, output progress, writer/flush timing, and terminal errors. Counters are monotonic and compositor frames never surfaced by Windows remain outside the claim.

## Window and failure behavior

- Focus loss and ordinary occlusion continue exact-window capture.
- Minimize can pause WGC. The progress watchdog pauses its stall deadline while the target is hidden and resets deadlines after restore.
- Same-HWND size changes recreate the bounded WGC pool while retaining the configured output canvas.
- A closed or replaced HWND is detected and finalized as failed/partial rather than frozen success.
- Normal League process disappearance remains the automatic match-end signal and requests graceful finalization.
- Startup retry uses the bounded 2, 5, 10, and 30-second schedule only after the previous attempt has cleaned up. Terminal incompatibility blocks retries for that process generation.
- There is no automatic primary-display, Desktop Duplication, GDI, software, cross-adapter, or cross-backend fallback.

WGC may show an operating-system capture indicator. QueueBack requests cursor exclusion and public border suppression where available; correctness never depends on restricted capabilities or manipulating the user's pointer.

## Support and validation matrix

| Combination | Production path | State | Physical performance claim |
| --- | --- | --- | --- |
| Windows 10 1903+ / exact HWND / D3D11 | native default and FFmpeg fallback | implemented with synthetic lifecycle/media fixtures | measured adapter/backend only |
| NVIDIA / NVENC / H.264 high 1080p60 | native default | implemented and non-League fixture-validated on RTX 4060 | pending valid QB-PERF-002 League matrix |
| NVIDIA / HEVC or non-high profile | explicit FFmpeg fallback | implemented | pending selected configuration evidence |
| AMD / AMF / same adapter | explicit FFmpeg fallback | implemented planner/runtime path | optimized-unvalidated; QB-PERF-003 owns hardware validation |
| Intel / QSV / same adapter | explicit FFmpeg fallback | implemented planner/runtime path | optimized-unvalidated; QB-PERF-004 owns hardware validation |
| Cross-adapter encoder | neither backend selects it | deliberately unsupported | cross-adapter-unvalidated |
| Missing WGC/runtime/direct interop | neither backend succeeds | unsupported with diagnostics | unsupported |

The default change is not a vendor-wide performance claim. QueueBack remains truthful about which backend, adapter, codec, and interop path actually ran, and every physical combination must pass the common frame-pacing and resource budgets before receiving a hardware-validated label.
