# Windows capture architecture

QueueBack targets the exact League of Legends top-level HWND with Windows Graphics Capture (WGC). It does not capture the desktop, inject into League, hook DirectX, intercept input, or modify League settings. The optimized paths require Windows 10 version 1903 or newer and the pinned QueueBack media runtime.

## Production backend

Windows has one production recorder backend:
`native-wgc-d3d11-nvenc`. There is no environment selector and no automatic or
operator-selected cross-backend fallback. The supported recording plan is H.264 High,
1920x1080 at 60 FPS on a same-adapter NVIDIA NVENC device. Unsupported codec/profile
configuration fails during initialization rather than being silently remapped.

HEVC, non-High recording profiles, AMD/AMF, and Intel/QSV were available only through
the retired FFmpeg-driven WGC comparison backend and had no validated production
combination. They are unsupported by the current product architecture. Future vendor
or codec support must extend and validate the native recorder contract rather than
revive a parallel capture backend.

## Native frame path

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

## FFmpeg boundary

The native recorder starts one supervised FFmpeg child, but FFmpeg performs no video
capture, filtering, scaling, or encoding. It receives the already encoded native
Annex-B H.264 packet sequence, captures the selected DirectShow loopback or silent
audio source, encodes AAC, and muxes fragmented MP4. Packaged ffprobe performs the
bounded common finalization probe. FFmpeg remains the correct implementation for
muxing, probing, export/transcoding, and fixture tooling; those uses are not recorder
backends.

## Runtime contract

The recorder resolves `queueback-ffmpeg-8.1.2-windows-x86_64-r6` through the shared
immutable media-runtime contract. Startup verifies file hashes, exact version/build
identity, and the FFmpeg/ffprobe pair. The currently pinned artifact still contains
the historical WGC/D3D11 patch and compiled hardware encoders as reproducible build
provenance, but the production recorder neither selects nor invokes those capture
filters or FFmpeg video encoders.

The r6 build pins FFmpeg 8.1.2, nv-codec-headers 12.2.72.0, AMF 1.4.36, the QueueBack
WGC patch, and its exact MSYS2 toolchain. Ordinary builds and startup do not rebuild
the runtime. Removing unused compiled capabilities requires a separately locked,
rebuilt runtime revision; it is not part of retiring the recorder backend.

The fragmented MP4 muxer flushes completed packets. Successful recording publishes only a validated private candidate as `video.mp4`. Failed startup or active-session failure preserves a recoverable partial candidate and an actionable error rather than claiming canonical success.

## Ownership, timing, and lifecycle

Native ownership is explicit and bounded. A lifecycle owner supervises the GPU worker; cooperative startup cancellation and stop execute blocking joins through Tokio's blocking pool. A fail-safe Drop join remains for invariant-breaking unwinds. An in-process worker cannot forcibly terminate a GPU-driver call that never returns, so hard driver-hang containment remains an open supervised-process boundary rather than a solved claim.

Startup is ready only after a real WGC frame with a positive `SystemRelativeTime`/QPC value, encoded output, mux progress, and a positive output timestamp. The first WGC QPC timestamp maps to Rust monotonic and wall clocks and becomes both the recording epoch and Live Client poller anchor. Process-spawn time is not the video epoch.

Capture diagnostics separately expose surfaced and superseded source frames, CFR discards and duplications, encoded/completed/muxed frames, pool recreations, bounded depths, first/latest QPC, output progress, writer/flush timing, and terminal errors. Counters are monotonic and compositor frames never surfaced by Windows remain outside the claim.

## Window and failure behavior

- Focus loss and ordinary occlusion continue exact-window capture.
- Minimize can pause WGC. The progress watchdog pauses its stall deadline while the target is hidden and resets deadlines after restore.
- Same-HWND size changes recreate the bounded WGC pool while retaining the configured output canvas.
- Target discovery and visibility/bounds queries temporarily use per-monitor-v2
  thread DPI awareness, then restore the caller's prior context. Window sizes
  therefore remain physical pixels even when the app or fixture process was
  initialized under a DPI-virtualized context.
- A closed or replaced HWND is detected and finalized as failed/partial rather than frozen success.
- Normal League process disappearance remains the automatic match-end signal and requests graceful finalization.
- Startup retry uses the bounded 2, 5, 10, and 30-second schedule only after the previous attempt has cleaned up. Terminal incompatibility blocks retries for that process generation.
- There is no automatic primary-display, Desktop Duplication, GDI, software, cross-adapter, or cross-backend fallback.

WGC may show an operating-system capture indicator. QueueBack requests cursor exclusion and public border suppression where available; correctness never depends on restricted capabilities or manipulating the user's pointer.

## Support and validation matrix

| Combination | Production path | State | Physical performance claim |
| --- | --- | --- | --- |
| Windows 10 1903+ / exact HWND / D3D11 | native WGC/D3D11/NVENC | implemented with synthetic lifecycle/media fixtures | measured adapter/backend only |
| NVIDIA / NVENC / H.264 High 1080p60 | native production backend | implemented and non-League fixture-validated on RTX 3050 Ti Laptop GPU | pending valid QB-PERF-002 League matrix on the target RTX 4060 system |
| HEVC or non-High recording profile | none | unsupported | none |
| AMD / AMF | none | unsupported by the native recorder | none |
| Intel / QSV | none | unsupported by the native recorder | none |
| Cross-adapter encoder | none | deliberately unsupported | cross-adapter-unvalidated |
| Missing WGC/runtime/direct NVENC interop | none | unsupported with diagnostics | unsupported |

The supported native combination is not a vendor-wide performance claim. Every
future physical combination must first exist in the native architecture and pass the
same frame-pacing, lifecycle, media-integrity, and resource budgets before receiving
a hardware-validated label.
