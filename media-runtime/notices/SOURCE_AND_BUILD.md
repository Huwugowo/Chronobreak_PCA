# QueueBack FFmpeg 8.1.2 runtime provenance

Runtime ID: `queueback-ffmpeg-8.1.2-windows-x86_64-r5`

The runtime is built locally by the checked-in maintainer script and locked by
the hashes in `media-runtime/runtime-lock.json`. QueueBack startup, Cargo/npm
builds, and ordinary tests never download or rebuild it.

## Pinned sources

- FFmpeg tag `n8.1.2`, commit
  `38b88335f99e76ed89ff3c93f877fdefce736c13`,
  <https://github.com/FFmpeg/FFmpeg/tree/n8.1.2>
- nv-codec-headers tag `n12.2.72.0`, commit
  `c69278340ab1d5559c7d7bf0edf615dc33ddbba7`,
  <https://github.com/FFmpeg/nv-codec-headers/tree/n12.2.72.0>
- AMD AMF tag `v1.4.36`, commit
  `16f7d73e0b45c473e903e46981ed0b91efc4c091`,
  <https://github.com/GPUOpen-LibrariesAndSDKs/AMF/tree/v1.4.36>

The selected nv-codec headers require Windows NVIDIA driver 551.76 or newer;
QueueBack's validation machine uses 566.03. This deliberate pin avoids the
610-series minimum imposed by the rejected generic 8.1.2 binary while keeping
the FFmpeg source current enough for Windows Graphics Capture.

## Pinned Windows toolchain inputs

- MSYS2 UCRT64 GCC `14.2.0-2`
- binutils `2.43.1-1`
- NASM `2.16.03-1`
- x264 `0.164.r3161.a354f11-3`
- Intel oneVPL `2.13.0-1`
- pkgconf `1~2.3.0-1`
- GNU make `4.4.1-2`
- MSYS2 runtime `3.5.4-8`

AMD's MSYS2 1.4.35 headers are intentionally overridden by the pinned 1.4.36
source above because FFmpeg 8.1.2 requires 1.4.36 or newer.

## QueueBack source patch

`media-runtime/patches/ffmpeg-8.1.2-queueback-wgc.patch` changes the two pinned
WGC/D3D11 filter files. Stock FFmpeg 8.1.2 creates one
ten-element NV12 D3D11 texture array for the video-processor output pool. The
non-stereo D3D11 output-view contract requires a single array element, and the
NVIDIA 566.03 driver rejects that stock allocation with `E_INVALIDARG` before
encoding starts. The patch makes FFmpeg's existing `AVBufferPool` allocate and
recycle individual `ArraySize=1` D3D11 textures. Frames remain GPU-resident,
retain render-target and video-encoder binding, and are bounded by the filter
graph plus encoder in-flight limits. The patch path and SHA-256 are part of the
runtime lock, and the build script rejects source or patch drift. The same
patch fixes the WGC output pool at eight surfaces, retains upstream's
single-frame acquisition behavior, synchronizes callback-counter reads, and emits the versioned
`queueback_capture abi=1` ready/first-frame/progress/terminal counters used for
startup truth and bounded-flow evidence. It logs only the first frame, every
120 delivered frames, and terminal state, so diagnostics do not add per-frame
I/O.

## Configuration

The exact generated configuration is embedded in `ffmpeg -version` and checked
by the runtime resolver. The material options are:

```text
--extra-version=queueback-5-captureabi1-nvcodec12.2-amf1.4.36
--pkg-config-flags=--static
--extra-ldflags=-static
--enable-gpl --enable-version3 --enable-static --disable-shared
--disable-debug --disable-doc --disable-ffplay --disable-autodetect
--enable-libx264 --enable-libvpl --enable-amf
--enable-d3d11va --enable-dxva2 --enable-mediafoundation
--enable-ffnvcodec --enable-nvenc
--enable-filter=gfxcapture --enable-filter=scale_d3d11
--disable-filter=amf_capture --enable-indev=dshow
```

`amf_capture` is an unrelated AMD desktop-capture source and is disabled; the
QueueBack production graph uses vendor-neutral `gfxcapture`. AMF H.264/HEVC
encoding and D3D11 texture input remain enabled.

Use `tools/media_runtime/build_ffmpeg.ps1` to verify/acquire the exact source
revisions explicitly, configure, compile, and install into the ignored build
tree. Use `prepare.ps1` to stage only the locked runtime files. The produced
executables import only Windows system/UCRT DLLs; vendor driver APIs are loaded
dynamically at runtime.
