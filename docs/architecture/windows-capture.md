# Windows capture architecture

QueueBack's optimized Windows recorder targets the exact League of Legends top-level HWND with Windows Graphics Capture (WGC). It does not capture the desktop, inject into League, hook DirectX, intercept input, or modify League settings. The implementation requires Windows 10 version 1903 or newer and the pinned QueueBack media runtime.

## Frame path

```text
League HWND
  -> Windows compositor / WGC (two D3D11 BGRA acquisition frames)
  -> latest-frame selection (older surfaced frames are counted as superseded)
  -> gfxcapture D3D11 render/output pool (eight BGRA textures)
  -> scale_d3d11 video processor (D3D11 NV12 textures)
  -> same-adapter encoder boundary
       NVENC: D3D11 texture registration, 16 surfaces
       AMF:   native D3D11 surface, async depth 4
       QSV:   direct D3D11-derived QSV hardware map, async depth 4
  -> 60-FPS CFR mux path
  -> AAC audio + fragmented MP4
```

Full video frames remain GPU-resident through acquisition, resize/color conversion, and encoder submission. `host_readback=false` means there is no full-frame `hwdownload`, CPU pixel conversion, or raw-frame pipe. WGC and the D3D11 video processor can still perform bounded GPU-to-GPU copies, so the path is described as GPU-resident rather than universally zero-copy.

The command explicitly creates the D3D11 device for the DXGI adapter containing the largest intersection of the League client area. Capture and encoder adapter LUIDs must match. QSV derives its device from that same D3D11 device and requires a direct hardware map. An unavailable encoder, failed direct map, or cross-adapter combination is unsupported for that candidate; it never enables an implicit host copy.

## Runtime contract

The Windows graph uses `queueback-ffmpeg-8.1.2-windows-x86_64-r3`, resolved through the shared immutable media-runtime contract. Startup verifies file hashes, exact version/build identity, `gfxcapture`, `scale_d3d11`, D3D11 support, compiled NVENC/AMF/QSV surfaces, and QueueBack capture diagnostics ABI 1. An arbitrary executable on `PATH` is not accepted.

The pinned FFmpeg patch has two narrowly scoped jobs:

- D3D11 video-processor output uses recycled single-slice textures. FFmpeg's stock fixed multi-slice texture array is rejected by the non-stereo output view on the validated driver. Dynamic pool allocation is bounded by the global 32-frame filter limit plus the selected finite encoder depth.
- WGC uses a two-frame acquisition pool, drains it to the newest available frame, fixes the gfxcapture hardware output pool at eight, and emits aggregate ABI-1 first-frame/progress/terminal counters. It never logs per frame.

The fragmented MP4 muxer flushes completed packets. Normal shutdown sends `q`, drains both FFmpeg pipes concurrently, waits under a finite deadline, and publishes only the successful private candidate as `video.mp4`. Abrupt encoder termination preserves already-flushed complete fragments; the dedicated fixture verifies such output with ffprobe and full decode.

## Ownership, flow control, and timing

One `RecordingSession` owns exactly one FFmpeg child, its stdin, two continuously drained pipes, a bounded latest-value diagnostics receiver, and one private output candidate. Startup is an owned cancellable task rather than a blocking service branch. League replacement, disappearance, or application shutdown cancels startup, explicitly terminates/reaps its child, and joins or aborts pipe drains under finite deadlines.

The finite resource contract is:

| Resource | NVENC | AMF | QSV |
| --- | ---: | ---: | ---: |
| WGC acquisition frames | 2 | 2 | 2 |
| gfxcapture BGRA output frames | 8 | 8 | 8 |
| globally buffered filter frames | 32 | 32 | 32 |
| encoder depth/surfaces | 16 | 4 | 4 |
| FFmpeg children | 1 | 1 | 1 |

At 1920x1080, the conservative declared texture budget is 232,243,200 bytes for NVENC and 194,918,400 bytes for AMF/QSV. It includes the two WGC frames, eight capture-output BGRA frames, the 32-frame NV12 filter bound, and the encoder depth. Encoder-internal allocations and driver bookkeeping are measured separately as process GPU memory; they are not mislabeled as texture-pool capacity.

When input outpaces the graph, the WGC callback retains the newest of the two surfaced frames and increments `source_frames_superseded`. FFmpeg separately reports CFR discards, CFR duplicates, encoded frames, and muxed bytes. These counters are monotonic and intentionally describe different ownership boundaries. Frames the Windows compositor never exposes are unknowable and outside QueueBack's accounting claim.

Startup is ready only after ABI validation, a real WGC frame with a positive `SystemRelativeTime` value, an encoded frame, positive mux progress, and a positive output timestamp. The first WGC `SystemRelativeTime`/QPC timestamp is mapped to Rust monotonic and wall clocks and becomes both the recording epoch and Live Client poller anchor. FFmpeg spawn time is not used as the video epoch.

## Window and failure behavior

- Focus loss and ordinary occlusion continue exact-window capture. The dedicated focused-occluder fixture proves changing decoded frames.
- Minimize can pause WGC source frames. Restore resumes within the existing recording; CFR duplicates during the pause remain explicit. No new recording-state/UI subsystem is introduced.
- Same-HWND size changes recreate the two-frame WGC pool and retain the configured output canvas. Each recreation is counted. Adapter identity remains fixed.
- A closed or replaced HWND while its process remains alive is detected on the service cadence, finalizes as failed/partial, and prevents a retry loop for that process generation.
- A normally disappearing League process remains the automatic match-end signal and requests graceful finalization.
- Encoder exit, malformed diagnostics, missing first-frame/output evidence, incompatible runtime, or candidate timeout fails that candidate. Each same-adapter candidate is tried at most once inside one 15-second total startup deadline.
- There is no automatic primary-display, Desktop Duplication, GDI, cross-adapter, or software-encoding fallback. Candidate fragments remain isolated and are never allowed to overwrite a successful canonical recording.

WGC may show an operating-system capture indicator. QueueBack requests cursor and border suppression where the public API permits it, but correctness does not depend on restricted border-removal capability. The recorder intentionally excludes the cursor from captured video; it does not manipulate the user's physical pointer.

## Support and validation matrix

| Combination | Implementation | Automated/integration evidence | Physical performance claim |
| --- | --- | --- | --- |
| Windows 10 1903+ / exact HWND / D3D11 | implemented | WGC moving-window, resize, minimize/restore, occlusion, target-close and forced-encoder fixtures | validated only with the measured adapter/encoder report |
| NVIDIA / NVENC / same adapter | implemented | planner, command, runtime, real RTX 4060 media fixtures | pending the QB-PERF-002 League matrix; becomes validated only for that recorded configuration |
| AMD / AMF / same adapter | implemented | planner, command, packaged runtime; no AMD device in the development machine | optimized-unvalidated; QB-PERF-003 owns physical validation |
| Intel / QSV / same adapter | implemented | planner, direct-map command, packaged runtime; no Intel device in the development machine | optimized-unvalidated; QB-PERF-004 owns physical validation |
| Cross-adapter encoder | deliberately not selected | rejection tests | cross-adapter-unvalidated |
| Missing WGC/runtime/direct interop | unsupported | deterministic failure tests | unsupported |
| GDI or full-display capture | not a production fallback | excluded by command tests and schema-v2 validation | no optimized-performance claim |

GPU-agnostic means the acquisition, timing, buffering, failure, metadata, and benchmark contracts do not branch on marketing names or PCI vendor IDs. It does not mean one RTX 4060 benchmark proves AMD or Intel performance. Every physical backend must pass the same frame-pacing and resource budgets before it receives a hardware-validated claim.
