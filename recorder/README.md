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

This validates configuration, ffmpeg discovery, and actual hardware encoder
initialization. It also reports whether a Windows loopback audio device was found.

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
3. Launch League and play for at least five minutes.
4. Confirm the tray icon is red while `League of Legends.exe` is running.
5. End the game or close League and confirm the tray returns to grey.
6. Inspect `{output_path}/games/{timestamp}/`:
   - `video.mp4` exists and plays;
   - `game_log.json` contains snapshots, events, and a non-null
     `game_start_video_offset_ms`;
   - `metadata.json` contains the local player, recording settings, `win: null`, and
     `matchv5_fetched: false`;
   - the tray returns to grey without a full-file conversion or system-wide I/O stall.
7. Seek to a kill event's `video_time_ms / 1000` position in VLC and confirm the kill
   occurs within approximately two seconds.
8. Repeat once after forcibly closing League. The fragmented `video.mp4` should remain
   playable up to its last completed fragment; at most approximately two seconds may
   be missing.

The recorder connects only to the local Live Client API at `127.0.0.1:2999`. It never
connects to the League Client API, and API disappearance never stops video capture;
only the League game process watcher does that.
