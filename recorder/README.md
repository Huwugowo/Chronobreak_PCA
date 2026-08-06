# League Replay Recorder

The recorder currently implements Phases 1 and 2 described by `../SPEC.md`,
`../RECORDER.md`, and `../PHASES.md`: fragmented MP4 capture plus synchronized Live
Client event and snapshot logging.

## Build

```powershell
cargo build --release
```

The binary is written to `target/release/recorder.exe` on Windows.

The recorder resolves ffmpeg in this order:

1. `LEAGUE_REPLAY_FFMPEG`
2. `resources/ffmpeg/ffmpeg.exe` beside the recorder
3. `ffmpeg` from `PATH`

Bundling ffmpeg is intentionally deferred to Phase 9.

## Diagnostics

```powershell
cargo run -- --diagnose
```

This validates configuration, ffmpeg discovery, an actual hardware encode, and the
selected concrete codec/profile. With `profile = "auto"`, diagnostics runs the short
encode benchmark documented in `../RECORDER.md` Section 8. It also reports whether a
Windows loopback audio device was found.

New configurations use:

```toml
[recording]
profile = "auto" # auto | very_low | low | medium | high | very_high
codec = "auto"   # auto | h264 | hevc
```

Until the Phase 3 app validates HEVC in its real webview, codec auto conservatively
selects H.264. `codec = "hevc"` explicitly enables hardware HEVC now; validate the
resulting file in the intended player before keeping that override.

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

## Phase 2 live validation

1. Run `cargo run --release`.
2. Confirm the grey tray icon appears.
3. Launch League and play a game lasting at least 25 minutes.
4. Confirm the tray icon is red while `League of Legends.exe` is running.
   No terminal window should open when ffmpeg starts.
5. End the game or close League and confirm the tray returns to grey.
6. Inspect `{output_path}/games/{timestamp}/`:
   - `video.mp4` exists and plays;
   - `game_log.json` contains snapshots, events, and a non-null
     `game_start_video_offset_ms`;
   - `metadata.json` contains the local player, concrete recording profile, codec,
     resolution/FPS, `win: null`, and `matchv5_fetched: false`;
   - the tray returns to grey without a full-file conversion or system-wide I/O stall.
7. Seek to early-, mid-, and late-game events at `video_time_ms / 1000` in VLC and
   confirm each event occurs within approximately two seconds.
8. Note in-game FPS near 5, 15, and 25 minutes; it should not progressively degrade
   because of Live Client polling. Confirm game exit causes no multi-second PC freeze.
9. Repeat once after forcibly closing League. The fragmented `video.mp4` should remain
   playable up to its last completed fragment; at most approximately two seconds may
   be missing.

The recorder connects only to the local Live Client API at `127.0.0.1:2999`. It never
connects to the League Client API, and API disappearance never stops video capture;
only the League game process watcher does that.
