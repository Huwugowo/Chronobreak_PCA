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

The harness and both real lifecycle arms are implemented in commit `9ef92f8`.
PowerShell parsing, Rust formatting, all-target/all-feature compilation, strict
Clippy, the focused encoder suite and the consolidated non-League suite pass.

The A/B has **not** been run. This worktree has the r5 source, lock and notices,
but not the exact locked `ffmpeg.exe` and `ffprobe.exe` files. The harness was
run in preflight-only mode and correctly failed closed with:

```text
CHRONOBREAK-AB-RUNTIME-MISSING
```

Stock FFmpeg 9 is intentionally not accepted as a substitute.

## Required runtime

Supply a staged directory whose `runtime-manifest.json` is the embedded
`queueback-ffmpeg-8.1.2-windows-x86_64-r5` lock and whose every locked file
matches its declared byte length and SHA-256. In particular:

```text
bin/ffmpeg.exe
  bc00f4dcc7870d216015c592b2821def0329cf58c084826027b009c36e1cd39a
bin/ffprobe.exe
  fcbc63a4552f56c325704495d91a2b3cb8404f91e5c280c0abf4749269a65d2f
```

The preflight also requires the exact r5 version banner, `gfxcapture`,
`scale_d3d11` and `h264_nvenc`. Prefer a known-good PC-A staged r5 directory.
If reproduction is required, use an isolated MSYS2 root with the exact locked
package versions; do not silently upgrade the lock or overwrite PC B's current
`C:\msys64` installation.

## Commands

Run from the PC-B repository root in Windows PowerShell. Use an absolute runtime
path.

```powershell
$runtime = 'D:\path\to\queueback-ffmpeg-8.1.2-windows-x86_64-r5'

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
