# League Replay Recorder

The recorder provides automatic League capture plus synchronized Live Client event
and snapshot logging. Durable product and lifecycle details live under `../docs/`;
the retired phase specifications under `../Old_spec/` are historical only.

## Build

```powershell
cargo build --release
```

The binary is written to `target/release/recorder.exe` on Windows.

The Windows recorder requires QueueBack's exact paired r6 media runtime at
`resources/media-runtime` beside `recorder.exe`. During development only,
`QUEUEBACK_MEDIA_RUNTIME_DIR` may point to a complete staged runtime that passes
the same embedded lock. The recorder does not accept `LEAGUE_REPLAY_FFMPEG`, a
bare PATH executable, a partial pair, or an incompatible build.

Build/stage/verification commands and the exact runtime contract are documented
in `../docs/development/VERIFICATION.md` and
`../docs/architecture/media-runtime.md`.

## Diagnostics

```powershell
cargo run -- --diagnose
```

This validates configuration and packaged-runtime identity, reports the fixed native
H.264 High recording plan and runtime ID, and discovers Windows loopback audio. The
production graph is created only after League exposes its exact HWND and DXGI adapter.
Windows has one recorder backend: in-process WGC/D3D11/NVENC video with FFmpeg used
only for AAC audio and fragmented-MP4 muxing. There is no backend selector or
cross-backend fallback. Read `../docs/architecture/windows-capture.md` for the
finite-pool, lifecycle, and support-label contract. Hardware capture/encode proof is
owned by the dedicated native fixture and live-validation procedures below, not by
inferring support from the packaged FFmpeg encoder list.

New configurations use:

```toml
[recording]
profile = "auto" # auto | high
codec = "auto"   # auto | h264
```

On Windows, `auto` resolves to the fixed H.264 High 1920x1080 60-FPS native plan.
HEVC and other profile values are rejected during recorder initialization; they are
not silently mapped to H.264 High and no external compatibility backend is retained.

If neither `Stereo Mix` nor a DirectShow WASAPI loopback device is available, the
recorder keeps the video recording alive with a silent stereo track. To select a
specific DirectShow device during development:

```powershell
$env:LEAGUE_REPLAY_AUDIO_DEVICE = "Your loopback device"
```

Set the value to `silent` to force the silent-track fallback.

## Development mode

`--headless` runs the recorder without a tray and stops cleanly on Ctrl+C. The
production mode has a grey idle tray icon and red recording icon.

`--tray-smoke-test` creates the real tray icon and service, then shuts both down
automatically after three seconds. It is intended for build validation only.

`LEAGUE_REPLAY_PROCESS_NAME` can override the watched process name for controlled
development tests. The production defaults remain `League of Legends.exe` on Windows
and `League of Legends` on macOS.

The dedicated native Windows fixture never targets League or a user recording:

```powershell
& ..\tools\native_backend\run_native_fixture.ps1 -Scenario steady -DurationSeconds 10
```

The runner also covers `resize`, `minimize_restore`, `occlusion`, and `close_window`,
plus injected `nvenc_failure`. Normal and failed-partial output is checked with
ffprobe, full decode, changing-frame hashes, native frame accounting, and finite
resource bounds. `-CollectResources -KeepTargetVisible` adds process/GPU-memory
sampling and keeps the generated GDI surface composited during the bounded soak; the
separate occlusion scenario owns hidden-window behavior. Outputs stay under
sentinel-owned `build/perf`. Additional native source, NVENC, MP4, lifecycle, and
matched-backend probes live under `examples/native_*` and `../tools/native_backend/`.

Windows may show its normal capture indicator. QueueBack requests no captured cursor
and public border suppression, but it does not manipulate the physical pointer or use
restricted permissions to hide the indicator. There is no automatic display/GDI
fallback when the optimized graph is unsupported.

## Phase 2 live validation

1. Stage the packaged runtime, build release, and run the release recorder. Do not use
   `cargo run` for performance measurement.
2. Confirm the grey tray icon appears.
3. Launch League and play a game lasting at least 25 minutes.
4. Confirm the tray icon is red while `League of Legends.exe` is running.
   No terminal window should open when ffmpeg starts.
5. End the game or close League and confirm the tray returns to grey.
6. Inspect `{output_path}/games/{timestamp}/`:
   - `video.mp4` exists and plays;
   - `game_log.json` and `metadata.json` both use schema version 2 and the same
     nonzero `media_id`; the game log contains integer-microsecond game observations
     and, when the Live Client is available, a persisted calibration;
   - `metadata.json` contains a validated `media_timeline` with the concrete profile,
     codec, exact frame grid, audio coverage, and saved state;
   - the tray returns to grey without a full-file conversion or system-wide I/O stall.
7. Seek to early-, mid-, and late-game events through the app's schema-v2 replay-tick
   mapping and confirm each event occurs within approximately two seconds.
8. Note in-game FPS near 5, 15, and 25 minutes; it should not progressively degrade
   because of Live Client polling. Confirm game exit causes no multi-second PC freeze.
9. Use the dedicated failure fixture—not a real recording—for destructive encoder
   interruption. A normal League process disappearance is the automatic match-end
   signal and is finalized gracefully.

The recorder connects only to the local Live Client API at `127.0.0.1:2999`. It never
connects to the League Client API, and API disappearance never stops video capture;
only the League game process watcher does that.
