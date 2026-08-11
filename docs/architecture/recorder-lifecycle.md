# Recorder lifecycle and synchronization

## Startup and steady state

`recorder/src/main.rs` loads configuration, initializes daily non-blocking logs, and selects diagnostics, headless, or tray mode. Production mode creates a winit/tray event loop; the service itself runs on Tokio.

`recorder/src/service.rs` creates the output `games/` directory, resolves ffmpeg, selects one concrete hardware encoder/codec/profile plan, detects an audio source, and then enters idle state. Encoder selection happens before League appears so setup cost and errors do not occur on the gameplay hot path.

`recorder/src/watcher.rs` refreshes the process table every two seconds. League process presence is the sole recording lifecycle authority:

1. appearance creates a unique timestamped game directory and starts ffmpeg;
2. PID replacement finalizes the old session and starts a new one;
3. disappearance finalizes the active recording;
4. shutdown finalizes an active session before the service exits.

Live Client API readiness and `GameEnd` are deliberately not start/stop signals.

## Capture and finalization

`recorder/src/encoder.rs` resolves ffmpeg from `LEAGUE_REPLAY_FFMPEG`, a packaged-resource candidate, or `PATH`. It prefers a League window capture target and retries the primary-display fallback when window-region startup fails. Recording is hardware encoded to fragmented MP4 with an audio source or silent stereo fallback.

Startup checks whether ffmpeg survives its initial launch period. During recording, the service checks whether the child exited on each watcher tick. An unexpected exit is surfaced as an error and the partial bundle is preserved. This is process-liveness detection, not proof that frames or bytes continue to advance; output-progress health monitoring remains separate roadmap work.

Normal stop writes `q` to ffmpeg, waits up to ten seconds, then kills it if necessary. There is no end-of-game full-file copy or remux. Fragmentation limits interruption loss to the last incomplete fragment when the container behaves as intended, but finalized media is not currently post-validated automatically.

## Live Client synchronization

`recorder/src/poller.rs` starts after ffmpeg so every stored event can use the recording clock. Calibration probes `/gamestats` until it observes five strictly advancing, clock-consistent samples and derives `game_start_video_offset_ms` from monotonic receive time and ffmpeg start time.

After calibration, independent loops:

- poll cumulative `/eventdata` every second, deduplicate by event ID, normalize fields, sort chronologically, and precompute `video_time_ms`;
- poll game, active-player, player-list, and event recovery data every ten seconds, storing snapshots and deriving item/level changes.

Requests are concurrent within a snapshot. Three consecutive failures stop only the affected polling task; successful requests reset the failure count. API loss never stops video. Poller cancellation and ffmpeg shutdown begin together so metadata work does not hold video finalization open.

## Persistence and failure behavior

`game_log.json` is rewritten atomically during capture via a temporary sibling and rename. Final `metadata.json` records the selected recording details, duration, local player, clock offset, and saved state. A polling startup failure causes the just-started capture to be stopped and reports recording startup failure rather than silently creating a video-only healthy session.

Current recoverability boundaries:

- ffmpeg startup failure is visible and the service can retry on a later process transition;
- unexpected ffmpeg exit is visible and partial media is retained;
- Live Client loss is recorded in diagnostics while video continues;
- interrupted fragmented MP4 and the last atomically written game log are intended to survive;
- there is no automatic repair, content-progress watchdog, storage preflight, or post-recording integrity scan yet.

## Performance constraints

The watcher interval is two seconds; event polling is one second; snapshots are ten seconds. Tokio missed ticks use delay semantics to avoid burst catch-up. JSON writes are incremental and no full video pass occurs at match end. These choices reduce expected game impact, but no accepted baseline-versus-capture FPS/frametime budget exists yet; see `QB-PERF-001`.
