# RECORDER.md — Process 1: Rust Recorder Binary

> **Context:** This document is self-contained. It covers everything needed to build the recorder binary. For data schemas (`game_log.json`, `metadata.json`) and file system layout, refer to `SPEC.md` sections 3 and 4.
>
> **What this process does:** Watches for League of Legends to launch, records the screen with hardware encoding, polls the Live Client Data API to build a structured event log, then finalises the game bundle on game close.
>
> **What this process does NOT do:** It has no UI beyond a system tray icon. It never reads from disk. It never communicates with the app.
>
> **Implemented through Phase 2:** Phase 1 provides process watching,
> hardware-accelerated fragmented MP4 capture, and tray states. Phase 2 adds Live
> Client polling plus incrementally durable `game_log.json` and final
> `metadata.json`. Neither phase connects to the League Client API, and neither
> `GameEnd` nor Live Client availability controls video recording.

---

## 1. Overview

A lean Rust binary. No UI framework. Single system tray icon. Auto-starts with the OS. The user never needs to open or interact with it during normal use.

**Crate structure:**
```
recorder/
  Cargo.toml
  src/
    main.rs        ← startup, tray, top-level event loop
    watcher.rs     ← process detection
    encoder.rs     ← ffmpeg capture lifecycle
    storage.rs     ← directory allocation and atomic JSON writes
    poller.rs      ← Live Client polling, persisted models, snapshot diffs
    config.rs      ← config.toml read/write
```

---

## 2. Startup Sequence

```
1. Launch → check for existing config.toml in platform config dir
2. If no config → write defaults (see Section 8)
3. Query GPU vendor via sysinfo → select encoder (see Section 8)
4. Show system tray icon (idle state)
5. Spawn async task: Process Watcher
6. Run tray event loop (blocking main thread)
```

---

## 3. Process Watcher

- Polls running processes every **2 seconds** using the `sysinfo` crate
- Target process names:
  - Windows: `League of Legends.exe`
  - macOS: `League of Legends`
- On process **appears** → trigger Recording Start (Section 4)
- On process **disappears** → trigger Recording Stop (Section 6)
- Handles League crashes identically to a normal close — process disappears, stop sequence runs

---

## 4. Recording Start Sequence

Phase 1:

```
1. Create output directory: {output_path}/games/{unix_timestamp}/
2. Detect League window position and display (see Section 6)
3. Spawn ffmpeg child process (see Section 9)
   - Output: {output_path}/games/{unix_timestamp}/video.mp4
   - Two-second keyframe fragments keep completed footage decodable after interruption
4. Update tray icon → recording state (red circle)
```

Phase 2 extends this sequence:

```
5. Record video capture start using a monotonic clock (used later for video_offset_ms calculation)
6. Start game-clock calibration poller (see Section 5.1)
   - On first valid gameTime: collect five rapid clock samples, calibrate video_offset_ms, then spawn Event Loop + Snapshot Loop concurrently (see Section 5.2 and 5.3)
```

---

## 5. Live Client Data Poller

Two independent async tasks run concurrently after the five-sample clock calibration, sharing access to the in-memory game log via `Arc<Mutex<GameLog>>`. They have different polling rates because they serve different purposes: events need to be captured as close to real time as possible, while snapshots are inherently coarse state.

### 5.1 Pre-Game Clock Calibration — `/allgamedata` (continuous)

Before the Live Client API becomes available, only the calibration loop runs. All synchronization timestamps use a monotonic clock such as `std::time::Instant`.

```
loop:
  GET https://127.0.0.1:2999/liveclientdata/allgamedata

  → ConnectionError / 503:
      API not ready yet (loading screen)
      sleep 1s, continue

  → 200 with valid gameData.gameTime:
      · the game is live; do not wait for the GameStart event
      · collect five successful responses without a fixed sleep
      · immediately after each response body is received:
          candidate_game_zero_ms =
              response_received_monotonic_ms - gameTime_ms
      · game_zero_monotonic_ms = median(five candidates)
      · video_offset_ms =
          game_zero_monotonic_ms - video_capture_start_monotonic_ms
      · write video_offset_ms to in-memory metadata
      · spawn Event Loop task (Section 5.2)
      · spawn Snapshot Loop task (Section 5.3)
      · exit this calibration loop
```

The first API response may be a bootstrap outlier, so a single sample is not sufficient. The median also makes ordinary request-latency variation harmless. Buffer raw event `EventTime` values until calibration is ready, or start the Event Loop immediately after calibration; `/eventdata` is cumulative, so the initial events remain available.

**Never calculate `video_offset_ms` from the time at which `GameStart` is observed.** In the empirical capture, `GameStart.EventTime` was `0.0095s`, but it first appeared in a response at `gameTime = 2.1195s`. Arrival-time anchoring would shift every video marker approximately 2.11 seconds late.

### 5.2 Post-Calibration — Event Loop (continuous)

Polls `/eventdata` as fast as the API responds. No fixed sleep interval — the next request fires immediately after the previous response is processed. This ensures named events (kills, objectives) are captured with minimum latency.

```rust
tokio::spawn(async move {
    let mut last_event_count = 0;
    loop {
        match client.get(".../eventdata").send().await {
            Err(_) => {
                // API is unavailable: stop this polling task only.
                // The process watcher remains the sole video stop trigger.
                break;
            }
            Ok(response) => {
                let events = parse_events(response).await;

                // diff: new events are always appended, never reordered
                let new_events = &events[last_event_count..];
                last_event_count = events.len();

                for event in new_events {
                    let video_time_ms =
                        video_offset_ms + event.event_time_seconds * 1000.0;
                    let mut log = game_log.lock().unwrap();
                    log.push_event(event, video_time_ms);
                    // flush to disk atomically after each batch
                }

                // no sleep — fire next request immediately
            }
        }
    }
});
```

**Why continuous polling:** The API responds in 10–200ms on localhost. Continuous polling gives sub-second event capture. Kill events, multikills, and objective events are captured nearly in real time.

**API loss:** A `ConnectionError` ends the affected polling task. It never stops ffmpeg or finalizes the bundle. The process watcher remains authoritative for video lifetime and finalization; `GameEnd` is logged if present but never controls stopping or win/loss.

### 5.3 Post-Calibration — Snapshot Loop (every 10s)

Polls `/allgamedata` every 10 seconds. Detects item and level changes by diffing consecutive snapshots.

The endpoint exposes champion, team, items, CS, level, spells, and keystone for all players. Current gold and current/max HP are available only for the active local player and are stored as `null` for everyone else. XP is not exposed. The full rune ID list is stored only for the local player on the first snapshot.

```rust
tokio::spawn(async move {
    let mut prev_snapshot: Option<Snapshot> = None;
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        match client.get(".../allgamedata").send().await {
            Err(_) => break, // stop this polling task only; video remains process-controlled
            Ok(response) => {
                let snapshot = parse_snapshot(response).await;

                // detect changes vs previous snapshot
                if let Some(prev) = &prev_snapshot {
                    let changes = diff_snapshots(prev, &snapshot);
                    // changes: ItemPurchased, ItemSold, LevelUp per player
                    // each change gets video_time_ms = snapshot.game_time_ms + video_offset_ms
                    let mut log = game_log.lock().unwrap();
                    for change in changes {
                        log.push_snapshot_change(change);
                    }
                }

                let mut log = game_log.lock().unwrap();
                log.push_snapshot(snapshot.clone());
                prev_snapshot = Some(snapshot);
                // flush to disk atomically
            }
        }
    }
});
```

**Snapshot diff logic:**

```rust
fn diff_snapshots(prev: &Snapshot, curr: &Snapshot) -> Vec<SnapshotChange> {
    let mut changes = vec![];
    for (prev_player, curr_player) in prev.players.iter().zip(curr.players.iter()) {
        // items: detect additions and removals by item_id
        for item in &curr_player.items {
            if !prev_player.items.contains(item) {
                changes.push(SnapshotChange::ItemPurchased { player: &curr_player.name, item_id: item.item_id });
            }
        }
        for item in &prev_player.items {
            if !curr_player.items.contains(item) {
                changes.push(SnapshotChange::ItemSold { player: &curr_player.name, item_id: item.item_id });
            }
        }
        // level
        if curr_player.level > prev_player.level {
            changes.push(SnapshotChange::LevelUp { player: &curr_player.name, new_level: curr_player.level });
        }
    }
    changes
}
```

Timestamps on snapshot-derived changes are approximate — accurate to within the 10s snapshot interval. Exact timestamps are available post-game via Match V5.

### 5.4 Shared State

Both tasks share `Arc<Mutex<GameLog>>`. The lock is held only during the append + flush operation, never during the HTTP request. Contention is negligible.

```rust
let game_log = Arc::new(Mutex::new(GameLog::new()));
let log_for_events   = Arc::clone(&game_log);
let log_for_snapshots = Arc::clone(&game_log);
// each clone is moved into its respective task
```

### 5.5 Incremental Write Strategy

`game_log.json` is written to disk after every batch of changes (both tasks flush after each successful poll). Not held in memory until game end:

```
1. Lock game_log
2. Append new data
3. Serialize to JSON
4. Write to game_log.json.tmp
5. Rename .tmp → game_log.json  (atomic on all target platforms)
6. Release lock
```

---

## 6. Recording Stop Sequence

Video recording stops when the Process Watcher detects that the League game process has
disappeared. This is the Phase 1 trigger and remains authoritative in later phases.
Neither `GameEnd` nor Live Client API availability controls the video lifetime.

Phase 1:

```
1. Send graceful stop signal to ffmpeg (write 'q' to ffmpeg stdin)
2. Wait for ffmpeg to exit cleanly (max 10s timeout → force kill if exceeded)
3. Verify video.mp4 is non-empty
4. Update tray icon → idle state (grey circle)
```

There is no full-file read, copy, or remux at game end. If ffmpeg is interrupted,
`video.mp4` remains playable through its last completed fragment. With a two-second
fragment interval, at most approximately two seconds of trailing footage may be lost.

Phase 2 starts poller cancellation at the same time as ffmpeg shutdown so metadata work never delays video closure:

```
1. Cancel the calibration, Event Loop, and Snapshot Loop tasks while ffmpeg closes
2. Flush game_log.json one final time after the polling tasks stop
3. Write metadata.json with:
   - win: null and win_method: "unknown" (resolved post-game)
   - matchv5_fetched: false  ← always false at this point; app fetches post-game
   - All other fields (see SPEC.md §3.8 for full schema)
```

**Note on win/loss:** The recorder does not consume `GameEnd`. It writes `win: null` and `win_method: "unknown"` at stop time. The app resolves the result via Match V5 on next launch; without enrichment the result remains unknown because the Live Client API does not expose a complete final team-gold state.

**Why fragmented MP4:**
Ordinary MP4 depends on final index data and may be unreadable after interruption.
Fragmented MP4 stores packet metadata alongside short media fragments, so completed
fragments remain decodable without a final full-file conversion. This keeps game-end
finalization effectively immediate. The trade-off is lower compatibility with some
older players and editors; the app playback and stream-copy clip paths must remain part
of validation.

---

## 7. Window Capture Strategy

### Windows
```
1. Find League window handle by process name (windows-rs crate)
2. Attempt window capture: ffmpeg -f gdigrab -i hwnd:{handle}
3. If League is fullscreen exclusive: ffmpeg -f d3d11grab (DXGI Desktop Duplication)
4. Fallback: capture full primary monitor
```

### macOS
```
ffmpeg -f avfoundation -capture_cursor 0 -i {window_index}
Window index resolved by matching window title "League of Legends"
```

**Multi-monitor:** auto-detect which monitor the League window is on using window position + monitor bounds. No user configuration needed.

---

## 8. Hardware Encoder Selection

Run once on startup. Result is stored in memory and used for all subsequent recordings.

```
Query GPU vendor via sysinfo:

Windows:
  "NVIDIA" → h264_nvenc
  "AMD"    → h264_amf
  "Intel"  → h264_qsv
  Multiple GPUs → prefer discrete (NVIDIA > AMD > Intel)

macOS:
  always → h264_videotoolbox
  (works on Intel, AMD, and Apple Silicon M-series)
```

Encoder used is recorded in `metadata.json` (`encoder_used` field). The app layer has no awareness of which encoder was used — the output is always H.264 MP4.

---

## 9. ffmpeg Recording Commands

### Windows — NVENC
```bash
ffmpeg \
  -f gdigrab -framerate {fps} -i hwnd:{handle} \
  -f dshow -i audio="Stereo Mix" \
  -vcodec h264_nvenc \
  -preset p4 \
  -b:v {bitrate_kbps}k \
  -maxrate {bitrate_kbps * 1.5}k \
  -bufsize {bitrate_kbps * 2}k \
  -g {fps * 2} \
  -vf scale={width}:{height} \
  -acodec aac -b:a 192k \
  -movflags +frag_keyframe+empty_moov+default_base_moof \
  -y {output_path}/games/{timestamp}/video.mp4
```

> **Audio note:** "Stereo Mix" is a Windows loopback device that captures all system audio. It is disabled by default on many machines (enable via Sound settings → Recording → Show Disabled Devices). If unavailable, fall back to `audio="WASAPI loopback"` or omit audio entirely (record video-only with a silent audio track) rather than failing the recording. Document the fallback in the implementation.

### Windows — AMF (AMD)
Replace `-vcodec h264_nvenc -preset p4` with `-vcodec h264_amf -quality quality`

### Windows — QuickSync (Intel)
Replace with `-vcodec h264_qsv -preset medium`

### macOS — VideoToolbox
```bash
ffmpeg \
  -f avfoundation -framerate {fps} -i {window_index}:default \
  -vcodec h264_videotoolbox \
  -b:v {bitrate_kbps}k \
  -g {fps * 2} \
  -vf scale={width}:{height} \
  -acodec aac -b:a 192k \
  -movflags +frag_keyframe+empty_moov+default_base_moof \
  -y {output_path}/games/{timestamp}/video.mp4
```

---

## 10. Configuration (`config.toml`)

Default location:
- Windows: `%APPDATA%\LeagueReplay\config.toml`
- macOS: `~/Library/Application Support/LeagueReplay/config.toml`

Both the recorder and the app read and write this file. The recorder uses `[recording]`, `[storage]`, and `[app]` sections. The app additionally reads and writes `[riot_account]`. The recorder ignores `[riot_account]` entirely.

```toml
[recording]
resolution = "source"       # "source" | "1920x1080" | "2560x1440"
fps = 60                    # 30 | 60
bitrate_kbps = 20000        # default 20 Mbps

[storage]
output_path = "~/LeagueReplays"
auto_delete_days = 30       # recordings older than this deleted on app launch
                            # unless metadata.saved == true

[app]
autostart = true            # register OS startup entry on first launch

[riot_account]
riot_id = ""                # "gameName#tagLine" — empty means enrichment disabled
api_key = ""                # personal dev key (expires 24h) or left empty in production phase
```

---

## 11. Tray Icon States

| State | Icon | Condition |
|---|---|---|
| Idle | Grey circle | Waiting for League to launch |
| Recording | Red circle | League running, ffmpeg active |

**Right-click context menu:**
- Open App
- Settings
- Quit

---

## 12. Crash Resilience

| Scenario | Behaviour |
|---|---|
| ffmpeg killed mid-game | Completed MP4 fragments remain playable; up to the current approximately two-second fragment may be lost. |
| Recorder crashes mid-game | game_log.json has all data up to last successful atomic write. |
| OS power loss | Completed MP4 fragments survive. game_log.json survives to last atomic write. |
| Incomplete bundle on next launch | `video.mp4` is already playable; no video recovery conversion is required. Phase 2 writes incomplete structured metadata as needed. |
| Live Client API ends without a result | win: null, win_method: "unknown" in metadata. App resolves via Match V5 on next launch if Riot ID configured. |

---

## 13. Build & Output

```bash
cd recorder
cargo build --release
```

Output: `recorder/target/release/recorder.exe` (Windows) or `recorder/target/release/recorder` (macOS).

Single binary, no runtime dependencies, ~2–5MB. ffmpeg is referenced from the bundled path set in config.

**ffmpeg:** bundled with the app under `resources/ffmpeg`. The recorder resolves the path at startup — it does not rely on a system-installed ffmpeg.
