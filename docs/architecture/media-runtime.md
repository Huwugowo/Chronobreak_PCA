# Packaged media runtime

QueueBack's Windows recorder and Tauri app share one immutable FFmpeg/ffprobe contract implemented by `media-runtime/`. Neither process trusts a tool merely because it is named `ffmpeg`, appears on PATH, or is paired with a self-declared adjacent manifest.

## Identity and layout

`media-runtime/runtime-lock.json` version 3 is embedded into both Rust consumers. It identifies the Windows x86-64 runtime, exact FFmpeg/source/toolchain build, checked source patches, required capabilities, and every staged file by relative path, size, and SHA-256. The staged and installed layout is:

```text
media-runtime/
  runtime-manifest.json
  bin/ffmpeg.exe
  bin/ffprobe.exe
  licenses/
```

The resolver treats the directory as one indivisible pair. It rejects unsafe paths, missing or mixed files, manifest/lock disagreement, hash or size mismatch, incompatible version/build output, execution timeout, and missing baseline capability. The expected lock is compiled into QueueBack; the adjacent manifest cannot redefine expected hashes.

Windows production discovery is `<executable>/resources/media-runtime`. `QUEUEBACK_MEDIA_RUNTIME_DIR` is an explicit whole-directory development override subject to identical validation. PATH, `LEAGUE_REPLAY_FFMPEG`, and single-tool overrides are not production fallbacks.

## Build and provenance

The current runtime is `queueback-ffmpeg-8.1.2-windows-x86_64-r3`. It is a static GPLv3 QueueBack build pinned to FFmpeg `n8.1.2`, nv-codec-headers `n12.2.72.0`, AMD AMF `v1.4.36`, one hash-locked WGC/D3D11 compatibility and diagnostics patch, and exact MSYS2 UCRT64 package versions. The patch replaces stock FFmpeg's invalid multi-slice non-stereo video-processor output allocation with recycled single-slice D3D11 textures; it does not add a CPU copy. It also fixes WGC's output pool at eight, drains its two-frame acquisition pool to the latest frame, counts superseded frames, and exposes the low-rate `queueback_capture abi=1` machine protocol. The runtime includes Windows Graphics Capture (`gfxcapture`), D3D11 conversion (`scale_d3d11`), libx264, and compiled NVENC/AMF/QSV H.264 and HEVC surfaces. Compiled capability is not a claim that every physical GPU/driver combination was measured or works.

`tools/media_runtime/build_ffmpeg.ps1 -Acquire` is the only source-acquisition/build path and is a deliberate maintainer action. Ordinary Cargo/npm builds, application startup, recording, and tests never download, rebuild, update, or mutate the runtime. `prepare.ps1` stages already-built locked files atomically; `verify.ps1`, `test_prepare.ps1`, `smoke.ps1`, and `stage_release.ps1` prove integrity and the portable layout using generated fixtures rather than user media.

The initially considered generic FFmpeg 8.1.2 Windows build was rejected after real NVENC probes required a newer NVIDIA driver than the supported machine had installed. The pinned nv-codec 12.2 build passes H.264 and HEVC NVENC probes on that existing driver while retaining the AMD and Intel compile surfaces. Physical AMD/Intel performance validation remains separate work.

## Consumer behavior

The recorder resolves and validates `MediaTools` once before diagnostics or capture. Encoder discovery, probes, and the recording child receive the exact resolved ffmpeg path and report the runtime ID; there is no per-frame or periodic validation work.

The app resolves the same pair once during setup. Browsing and playback do not intrinsically need FFmpeg, so a missing/invalid runtime does not hide a valid recording or turn it Unknown. Duration probing can omit the optional tool; clip export and thumbnail operations return the stored actionable runtime error when ffmpeg is unavailable.

`QB-PERF-002` extends this same runtime and resolver for the GPU-native Windows graph. It must not introduce a parallel binary-discovery mechanism or reinterpret compiled vendor surfaces as hardware performance evidence.
