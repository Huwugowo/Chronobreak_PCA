# QueueBack FFmpeg 8.1.2 runtime provenance

Runtime ID: `queueback-ffmpeg-8.1.2-windows-x86_64-r6`

The runtime was built locally in an isolated, repository-owned MSYS2 tree and
is locked by the hashes in `media-runtime/runtime-lock.json`. QueueBack
startup, Cargo/npm builds, and ordinary tests never download or rebuild it.

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

The isolated root was bootstrapped from the official
`msys2-base-x86_64-20260611.tar.zst` archive, SHA-256
`ace898d250d7302a24259a0288d69354649365af9cc64c8bcc2f219bc1e28374`.
The materially relevant installed packages are:

- MSYS2 UCRT64 GCC `16.1.0-5`
- binutils `2.46-4`
- NASM `3.01-1`
- libx264 `0.165.r3222.b35605a-2`
- Intel oneVPL `2.16.0-1`
- pkgconf `1~2.5.1-1`
- GNU make `4.4.1-3`
- MSYS2 runtime `3.6.9-2`
- Git `2.54.0-1`

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
--extra-version=queueback-6-captureabi1-nvcodec12.2-amf1.4.36
--pkg-config-flags=--static
--extra-ldflags=-static
--extra-libs=-lstdc++
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

The nv-codec headers are installed into a repository-local prefix and that
prefix's `lib/pkgconfig` directory is prepended to `PKG_CONFIG_PATH` during
configuration. Current static oneVPL contains C++ objects while its MSYS2
`vpl.pc` omits the C++ runtime from `Libs.private`; the explicit
`--extra-libs=-lstdc++` above is therefore required for FFmpeg's static VPL
link probe and final binaries.

For this build, the three pinned repositories were cloned below the ignored
`build/media-runtime/source` tree, their commits were verified with
`git rev-parse HEAD`, and the locked patch was applied with `git apply`. FFmpeg
was configured out-of-tree below `build/media-runtime/build` with the options
above, then compiled and installed with GNU make into
`build/media-runtime/install/ffmpeg-8.1.2-queueback-r6`. The produced
executables import only Windows system/UCRT DLLs; vendor driver APIs are loaded
dynamically at runtime.
