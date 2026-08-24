# Preliminary non-League backend A/B

## Purpose and boundary

`tools/native_backend/run_preliminary_backend_ab.ps1` performs a matched,
sequential comparison between the existing optimized FFmpeg/WGC backend and
the provisional native WGC/D3D11/direct-NVENC backend. Each arm records the
same animated, borderless 1920x1080 exact-HWND fixture for 240 seconds by
default.

This is useful preliminary performance evidence. It is not QB-PERF-002 and it
does not satisfy the audited M8 League comparison. It must not trigger the M9
backend-removal decision.

## Current state

The harness and both real lifecycle arms are implemented. A truthful r6 media
runtime was built on PC B from the same pinned FFmpeg 8.1.2, nv-codec, AMF and
QueueBack WGC patch inputs as r5, using a fresh isolated MSYS2 UCRT64 toolchain.
The runtime preflight and packaged-runtime integration test pass.

On 2026-08-18, a 10-second end-to-end smoke comparison and two mirrored
240-second comparisons passed. The full evidence roots are:

```text
evidence/preliminary-backend-ab/20260818-103855  ffmpeg-native
evidence/preliminary-backend-ab/20260818-105008  native-ffmpeg
```

Stock FFmpeg is not accepted as a substitute for the locked r6 runtime.

## Required runtime

Supply a staged directory whose `runtime-manifest.json` is the embedded
`queueback-ffmpeg-8.1.2-windows-x86_64-r6` lock and whose every locked file
matches its declared byte length and SHA-256. In particular:

```text
bin/ffmpeg.exe
  1dc19648acdcaa7ac2497689837feb32788d404e7705f756e331a2f535bfe015
bin/ffprobe.exe
  639da6703f05c4e0f436d8602b39f44070cbe8233c5dd942775e00f25004615e
```

The preflight also requires the exact r6 version banner, `gfxcapture`,
`scale_d3d11` and `h264_nvenc`. The local staged directory is
`build/media-runtime/windows-x86_64`. Provenance and the isolated toolchain
recipe are in `media-runtime/notices/SOURCE_AND_BUILD.md`; PC B's existing
`C:\msys64` installation was not modified.

## Commands

Run from the PC-B repository root in Windows PowerShell. Use an absolute runtime
path.

```powershell
$runtime = (Resolve-Path 'build\media-runtime\windows-x86_64').Path

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File tools\native_backend\run_preliminary_backend_ab.ps1 `
  -MediaRuntimeRoot $runtime `
  -PreflightOnly
```

Do not continue unless preflight prints
`CHRONOBREAK_PRELIMINARY_AB_PREFLIGHT=PASS`. Close unrelated `ffmpeg.exe`
processes, leave the animated fixture unobstructed, and avoid other heavy work.

Run the first pair:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File tools\native_backend\run_preliminary_backend_ab.ps1 `
  -MediaRuntimeRoot $runtime `
  -DurationSeconds 240 `
  -Order ffmpeg-native `
  -CooldownSeconds 15
```

For stronger preliminary evidence, run a second pair with the order reversed
after the machine returns to idle:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File tools\native_backend\run_preliminary_backend_ab.ps1 `
  -MediaRuntimeRoot $runtime `
  -DurationSeconds 240 `
  -Order native-ffmpeg `
  -CooldownSeconds 15
```

## Results

All four full arms produced valid 1920x1080 nominal-60-FPS H.264/AAC media,
terminal evidence, successful ffprobe inspection and a full decode.

| Order | Backend | CPU seconds | Machine CPU | Peak private MiB | Decoded frames |
| --- | --- | ---: | ---: | ---: | ---: |
| ffmpeg-native | FFmpeg/WGC | 10.844 | 0.284% | 233.91 | 14,432 |
| ffmpeg-native | native | 4.297 | 0.112% | 159.48 | 14,478 |
| native-ffmpeg | native | 4.266 | 0.111% | 160.35 | 14,482 |
| native-ffmpeg | FFmpeg/WGC | 11.844 | 0.309% | 237.32 | 14,423 |

Across the two orders, native averaged 4.281 CPU-seconds versus 11.344 for
FFmpeg/WGC, a 62.3% reduction. Native averaged 159.91 MiB peak combined private
memory versus 235.62 MiB, saving 75.70 MiB or 32.1%. Native CPU results differed
by 0.7% between orders; FFmpeg results differed by 8.8%, but both orders point
in the same direction.

Native's CFR evidence reported roughly 1,300 duplicate and 1,300 discard
decisions per run, while FFmpeg reported 7-17 duplicates and no discards. Both
had zero pool recreations, and source-frame superseding was 0-1. A post-run
exact-hash check of decoded 160x90 grayscale frames found fewer consecutive
repeats in native (13.9-15.2%) than FFmpeg (26.7-27.8%), so the scheduler
counters do not directly imply more repeated decoded frames on this fixture.
Game-motion quality and pacing still require later validation with the actual
game.

The native files were about 19% lower bitrate on this synthetic fixture. That
is not a quality result: encoder behavior and temporal repetition differ, and
no objective or subjective quality metric was collected.

## Controls and evidence

The runner:

- release-builds one Rust library-test executable and runs it directly;
- uses the same fixture PID, dimensions, High/H.264 profile and silent-audio
  source for both arms;
- rejects a busy FFmpeg process list before either arm;
- attributes only the exact locked FFmpeg executable whose parent is the Rust
  test process;
- samples Rust and recording-FFmpeg CPU/private memory once per second;
- keeps post-recording decode processes separate from the longest-lived
  recording child;
- requires H.264/AAC, 1920x1080, 60/1 FPS, bounded duration/frame counts,
  terminal recorder evidence, ffprobe success and a full decode;
- restores pre-existing recorder environment variables and kills only fixture
  or directly owned test processes during cleanup.

Each run creates a new ignored evidence directory under:

```text
evidence/preliminary-backend-ab/<timestamp>/
```

Read `comparison-summary.json` first. For each arm,
`total_machine_cpu_percent` includes both the Rust recorder process and its
longest-lived recording FFmpeg child. On the native arm that child is mux-only;
on the FFmpeg arm it owns capture/video encode/mux. Validation/full-decode
children are listed separately and excluded. Preserve both media files, both
terminal-evidence files, process CSVs, logs and ffprobe JSON.

This fixture does not reproduce League rendering behavior and does not collect
the full QB-PERF schema-v2 GPU/system metric set. Report it as preliminary
process-attributed CPU/memory/media evidence only.
