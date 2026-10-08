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

## Provisional League context

The recording service uses the existing Live identity observation and bounded LCU
current-summoner/gameflow GETs to bind a provisional game/queue candidate after two
consecutive fresh coherent rounds. It stops LCU requests after binding and may save
one optional `league_match.json` before canonical media publication. Context failure
does not stop video. Game IDs remain decimal strings; no result or final statistics
are attached. QB-DATA-002 owns exact post-game confirmation. Playback projection and
the accepted app-closed recording check are complete; see
`docs/execution/qb-data-003.md` for evidence and the post-game handoff.

## League context feasibility probe

From the repository root, with the recorder and viewer closed:

```powershell
cargo run --manifest-path recorder/Cargo.toml --example league_context_probe -- --seconds 60 --scenario mid_match
```

This standalone QB-DATA-003 diagnostic makes the existing snapshot request trio at
ten-second-or-slower intervals and only the allowlisted LCU current-summoner/gameflow
GETs. Its active design gate is provisional identity: exact local Riot ID, LCU
`InProgress`, positive game ID, explicit queue ID (zero is valid), and compatible
map/queue mode. No roster array or champion manifest is parsed for association.
The Live observation uses the active-player and game-data request windows, even if
the existing roster request fails. It does not start capture, write a game log, change configuration or
install match context. Do not run another Live collector alongside it for the
feasibility session. It needs no packaged media runtime, elevation or process command
lines.
`QUEUEBACK_LCU_LOCKFILE`, if used, must name an absolute adjacent lockfile with the
same discovered LeagueClient PID/path; it is not a URL or identity bypass.

Output is version-2 JSON Lines marked `contract: provisional_v3`, containing
comparison booleans, timing and sanitized
availability/status codes; identities, credentials, raw bodies and game/queue ID
values remain in memory. `two_rounds_coherent` requires distinct fresh rounds with
the same required evidence and credential epoch; a failed or incoherent round clears
the pending comparison. It remains a diagnostic flag, not a persisted candidate.
The aggregate coherent-game counter detects an ID change after closure while keeping
one last-coherent ID private; it never supplies evidence for pair admission.
Ctrl+C or the 1–3600-second duration cancels network work; already-started OS workers
are joined, so an uninterruptible OS call can extend shutdown.

Scenario labels are `startup`, `mid_match`, `consecutive_games`, `recorder_restart`,
`normal_closure`, `app_closed`, `practice`, and `reconnect`. A label records operator
intent, not evidence that the scenario actually occurred. Preserve separate sanitized
reports and record actual coverage in the feature checkpoint. In particular,
restarting this diagnostic is not integrated-recorder restart acceptance.

The probe always emits `feasibility_pass: false` and `authoritative_confirmed: false`:
a reviewer must establish ordinary
lifecycle coverage before implementing provisional association. A provisional result
is not authoritative; QB-DATA-002 must later confirm its exact `gameId` through
`/lol-end-of-game/v1/eog-stats-block` before final results or statistics are attached.
See the QB-DATA-003 ExecPlan and checkpoint for the current contract and accepted
evidence. Recording duration is not a QB-DATA-003 acceptance condition.

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

The production recording service connects to the local Live Client API at
`127.0.0.1:2999` and the allowlisted League Client endpoints described above.
API disappearance never stops video capture; the game process/HWND lifecycle
remains the capture authority.
