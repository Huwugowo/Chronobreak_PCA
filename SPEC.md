# League Replay Tool — Project Specification

> **How to use this document family:**
> This file is the shared vocabulary. Open it alongside whichever module document you are working on. It defines the data contracts, file paths, and enum values that all modules must agree on. Never modify the JSON schemas or file system layout without updating this file first.

---

## Document Family

| File | Scope |
|---|---|
| `SPEC.md` ← *this file* | Architecture, data contracts, constraints, limitations |
| `RECORDER.md` | Process 1 — Rust recorder binary |
| `APP-VIEWER.md` | Process 2 — Viewer screen (windowed + fullscreen modes, clip mode; Stats deferred) |
| `APP-CLIP.md` | Process 2 — Clip exporter screen |
| `APP-LIBRARY.md` | Process 2 — Game library screen + settings |
| `PHASES.md` | Build order, entry conditions, validation criteria |

---

## 1. Project Overview

A two-process desktop application for League of Legends players that:
- **Records every game automatically** with zero meaningful performance impact using hardware-accelerated encoding
- **Enriches the recording** with granular game data polled from the League Live Client Data API during the game
- **Provides a post-game viewer** with synchronized player state and an event timeline
- **Enables frictionless clip creation** with auto-suggested moments, adjustable endpoints, optional music, and export as a universally compatible MP4 file

### Core Design Philosophy
- The recorder must be completely invisible and have zero performance impact on the game
- The app only runs post-game, but playback, seeking, navigation, and overlay interaction must remain visibly immediate and frame-smooth
- Core functionality requires no external accounts or credentials — the app works fully offline
- Recordings are plain standards-based MP4 files (H.264, or HEVC only after end-to-end capability validation); exported clips are always publishable H.264/AAC MP4 files — no proprietary formats, upload links, or accounts
- Until the public-distribution phase, superseded configs, schemas, and implementation paths are deleted rather than migrated; development recordings and fixtures are disposable

---

## 2. High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  PROCESS 1 — RECORDER                        │
│              (Rust binary, system tray, always on)           │
│                                                              │
│  Process Watcher                                             │
│    → detects League launch/close via process name            │
│                                                              │
│  On game launch:                                             │
│    → spawns ffmpeg (hardware encoded fragmented MP4 capture) │
│    → polls /gamestats until gameTime advances consistently   │
│      · calibrates game clock zero against video time         │
│    → spawns two concurrent async tasks once API is live:     │
│      · Event Loop: /eventdata, every 1s                      │
│      · Snapshot Loop: /allgamedata, every 10s                │
│                                                              │
│  On game close:                                              │
│    → stops ffmpeg gracefully and closes video.mp4            │
│    → finalizes game_log.json                                 │
│    → writes metadata.json                                    │
│    → bundles everything into /games/{timestamp}/             │
└─────────────────────────────────────────────────────────────┘
                            ↓ writes to disk
┌─────────────────────────────────────────────────────────────┐
│              SHARED FILE SYSTEM (game bundles)               │
│                                                              │
│  /games/{timestamp}/                                         │
│    ├── video.mp4         ← crash-tolerant fragmented capture │
│    ├── game_log.json     ← events, snapshots, derived changes│
│    └── metadata.json     ← champion, mode, duration, offset  │
└─────────────────────────────────────────────────────────────┘
                            ↓ reads from disk
┌─────────────────────────────────────────────────────────────┐
│                  PROCESS 2 — APP                             │
│        (Tauri v2 + SolidJS + TypeScript, opened on demand)   │
│                                                              │
│  Games Tab     → grid of recorded game bundles               │
│  Clips Tab     → grid of exported clips                      │
│  Viewer        → windowed or fullscreen video player         │
│  Stats Tab     → deferred end-of-game scoreboard             │
│  Clip Exporter → trim, music, ffmpeg export to MP4           │
└─────────────────────────────────────────────────────────────┘
```

The two processes share **only the file system**. No IPC, no sockets, no shared memory. The recorder writes, the app reads. They can run simultaneously without conflict.

---

## 3. Data Contracts

### 3.1 Data Sources Overview

Two data sources are used:

| Source | When available | Required? | What it provides |
|---|---|---|---|
| `localhost:2999` Live Client API | During the game only | ✅ Always | Events, roster, items, levels, CS, and local-player gold/HP |
| Riot Data Dragon CDN | Anytime, static per patch | ✅ Always | Item names, item icons, champion icons |

---

### 3.2 Live Client Data API

**Base URL:** `https://127.0.0.1:2999/liveclientdata/`

Note: The API uses HTTPS with a self-signed Riot certificate. Requests must either use the official Riot root certificate (`riotgames.pem`) or disable SSL verification. The recorder uses `reqwest` with certificate pinning or verification disabled.

**Two polling loops run concurrently after clock calibration** (see `RECORDER.md` Section 5 for implementation):

| Loop | Endpoint | Interval | Purpose |
|---|---|---|---|
| Event loop | `/eventdata` | Every 1s | Capture cumulative named events; `EventTime` retains millisecond timestamps |
| Snapshot loop | `/allgamedata` | Every 10s | Capture roster state (items, CS, level) plus local-player gold/HP, and reconcile its cumulative event list as a backup |

Both endpoints expose a cumulative event list. A shared `EventID` set deduplicates events found by either loop, and persisted events are kept in chronological order.

**Named events observed from the Live Client API:**

| Event | Key fields | Notes |
|---|---|---|
| `GameStart` | EventTime | Logged as a semantic event; its arrival time is not a sync anchor |
| `MinionsSpawning` | EventTime | ~1:05 in every game |
| `FirstBlood` | EventTime | Separate marker observed before the first `ChampionKill` |
| `FirstBrick` | EventTime, KillerName | First turret plate destroyed |
| `TurretKilled` | EventTime, TurretKilled, KillerName, Assisters | TurretKilled is the turret ID string |
| `InhibKilled` | EventTime, InhibKilled, KillerName, Assisters | |
| `InhibRespawningSoon` | EventTime, InhibRespawningSoon | Warning before respawn |
| `InhibRespawned` | EventTime, InhibRespawned | |
| `DragonKill` | EventTime, DragonType, Stolen, KillerName, Assisters | DragonType: Air/Earth/Fire/Water/Hextech/Chemtech/Elder |
| `HeraldKill` | EventTime, Stolen, KillerName, Assisters | Rift Herald |
| `HordeKill` | EventTime, KillerName, Assisters | Void Grub kill; one event per grub |
| `BaronKill` | EventTime, Stolen, KillerName, Assisters | |
| `ChampionKill` | EventTime, VictimName, KillerName, Assisters | |
| `Multikill` | EventTime, KillerName, KillStreak | KillStreak: 2=Double … 5=Penta |
| `Ace` | EventTime, Acer, AcingTeam | |
| `GameEnd` | EventTime, Result | Not reliable; ignored for stop detection and match result |

> **`GameEnd`:** `GameEnd` is optional telemetry, not a lifecycle signal. It is commonly absent when the player exits shortly before the Victory/Defeat screen. The recorder stores it if observed but never depends on it. Three consecutive API failures end only the affected polling task; the League process watcher remains the sole video-stop trigger. The app does not infer a match result from this event.

**Item and level changes are NOT named events.** They are state fields in `/allgamedata` snapshots and are detected by diffing consecutive snapshots:

| Change detected | How | Timestamp precision |
|---|---|---|
| Item purchased | Item appears in player's items array | Snapshot interval (~10s) |
| Item sold | Item disappears from player's items array | Snapshot interval (~10s) |
| Level up | Player's level field increases | Snapshot interval (~10s) |

This means **build order timestamps from the Live Client API are approximate** — accurate to within the snapshot interval.

Viego possession can temporarily replace his items with the possessed champion's items. Those transitions remain ordinary approximate snapshot changes in Phase 2; no champion-specific correction heuristic is applied.

**Snapshots from `/allgamedata` (every 10s):** champion, team, CS, level, and items for every player. Current gold and current/max HP are exposed only for the active local player, so those fields are `null` for everyone else. XP is not exposed and is not stored. Summoner spells and keystone are stored for all players on the first snapshot; the full rune ID list is available only for the local player.

### 3.3 Data Dragon (Static Data)

**Base URL:** `https://ddragon.leagueoflegends.com/`

Free Riot CDN, no authentication required. Used to resolve item IDs and champion IDs to display names and icon URLs.

**Fetch strategy:**
- On app launch, check current patch version: `GET /api/versions.json` → first element is current patch
- If cached version matches current patch: use cache
- If different: fetch fresh data and update cache
- If the CDN is unavailable: use the newest complete cached patch
- Cache location: the OS app-local data directory under `ddragon/{patch_version}/`

**Data fetched:**
- `GET /cdn/{version}/data/en_US/item.json` — maps item ID → name, description, icon path
- `GET /cdn/{version}/data/en_US/champion.json` — maps champion key → name, icon path
- Icons: `GET /cdn/{version}/img/item/{itemId}.png` — fetched on demand, cached locally

The app never shows raw item IDs to the user. Every item ID from the Live Client API is resolved through this cache before display.

---

### 3.4 Timestamp Sync

The video recording starts when `League of Legends.exe` is detected — this includes the loading screen, typically 2–4 minutes before in-game clock 0:00.

**Sync mechanism:**
1. Record `video_start_monotonic_ms` when video capture starts. Use a monotonic clock, never UTC wall time, for synchronization.
2. Probe the lightweight `/gamestats` endpoint every 250ms while it is available. API responsiveness alone does not mean gameplay has started: during loading, `gameTime` can remain stationary (empirically at approximately 18ms).
3. Treat the game as live only after a rolling window of five samples has a strictly increasing `gameTime`, spans at least 750ms of monotonic time, and keeps clock deltas and candidate offsets within 250ms. A frozen, backward, failed, or inconsistent window is discarded.
4. For each accepted sample, record the monotonic timestamp immediately after receiving the response body and calculate:

   ```text
   game_zero_monotonic_ms =
       response_received_monotonic_ms - (gameTime_seconds × 1000)
   ```

5. Take the median of the five candidates. This rejects bootstrap or request-timing outliers while preserving the entire loading-screen duration in the offset.
6. Calculate the position of game clock zero in the video:

   ```text
   video_offset_ms =
       game_zero_monotonic_ms - video_start_monotonic_ms
   ```

7. Fetch the first `/allgamedata` snapshot, then start the 1s event loop and 10s snapshot loop. Both loops tolerate three consecutive failures one second apart; a success resets the counter.
8. Map every named event and snapshot-derived change using the timestamp supplied by the game clock:

   ```text
   video_time_ms = video_offset_ms + (EventTime_seconds × 1000)
   ```

   For snapshot-derived changes, substitute the snapshot's `gameTime` for `EventTime`.

`GameStart` is still written to the event log, but the time at which it is first observed is never used for synchronization. Event lists are cumulative and may expose `GameStart` after the game clock has already advanced. In the 2026-07-30 capture, `GameStart.EventTime` was `0.0095s` but the event was first observed at `gameTime = 2.1195s`; arrival-time anchoring would have made every marker about 2.11 seconds late. Later full-game tests showed why validity alone is also insufficient: `/allgamedata` was already responsive during loading with `gameTime` frozen near 18ms. Advancing-clock calibration distinguishes loading from gameplay and places game clock zero correctly in the video.

### 3.5 `game_log.json` Schema

```json
{
  "game_start_video_offset_ms": 142300,
  "snapshots": [
    {
      "game_time_ms": 0,
      "players": [
        {
          "summoner_name": "Player1",
          "team": "ORDER",
          "champion": "Jinx",
          "gold": 500,
          "hp": 580,
          "hp_max": 580,
          "cs": 0,
          "level": 1,
          "items": [
            { "item_id": 1055, "slot": 0, "count": 1 }
          ],
          "summoner_spells": ["SummonerFlash", "SummonerDot"],
          "keystone_id": 8128,
          "rune_ids": [8128, 8126, 8138, 8135, 8304, 8345, 5005, 5008, 5011]
        }
      ]
    }
  ],
  "events": [
    {
      "type": "ChampionKill",
      "game_time_ms": 214500,
      "killer": "Player3",
      "victim": "Player7",
      "assisters": ["Player1"],
      "video_time_ms": 356800
    },
    {
      "type": "Multikill",
      "game_time_ms": 217000,
      "killer": "Player3",
      "kill_streak": 2,
      "video_time_ms": 359300
    }
  ],
  "snapshot_derived_changes": [
    {
      "game_time_ms": 220000,
      "player": "Player1",
      "change_type": "ItemPurchased",
      "item_id": 3006,
      "video_time_ms": 362000
    }
  ]
}
```

`gold`, `hp`, and `hp_max` contain numbers for the active local player and `null` for every other player. XP is not present because the Live Client API does not expose it.

`summoner_spells` and `keystone_id` are present for all players only on the **first snapshot**. `rune_ids` is also first-snapshot-only and is present only for the local player. These fields are stable and do not need re-recording.

`video_time_ms` is pre-computed on each event and snapshot-derived change at write time.

`snapshot_derived_changes` records item purchases, sales, and level-ups detected by diffing consecutive snapshots. Timestamps are approximate (±10s). Valid `change_type` values: `"ItemPurchased"`, `"ItemSold"`, `"LevelUp"`.

### 3.6 `metadata.json` Schema

```json
{
  "recorded_at": "2026-03-06T14:32:00Z",
  "duration_ms": 2134000,
  "game_mode": "CLASSIC",
  "local_player_summoner_name": "PlayerName",
  "local_player_champion": "Jinx",
  "local_player_team": "ORDER",
  "video_offset_ms": 142300,
  "encoder_used": "nvenc",
  "recording_codec": "hevc",
  "recording_profile": "high",
  "recording_resolution": "1920x1080",
  "recording_fps": 60,
  "saved": false
}
```

`recording_codec` is `"h264"` or `"hevc"`. `recording_profile` is the concrete
profile used for this file (`"very_low"`, `"low"`, `"medium"`, `"high"`, or
`"very_high"`); it is never `"auto"` in metadata.

If the Live Client API never becomes available, `game_mode`, the three `local_player_*` fields, and `video_offset_ms` are `null`; the video bundle is still finalized normally.

`saved: false` = eligible for auto-deletion. All clips have `saved: true` by default.

---

## 4. File System Layout

```
{output_path}/                          ← default: ~/LeagueReplays
│
├── games/
│   ├── {unix_timestamp}/               ← one directory per game
│   │   ├── video.mp4                   ← hardware-encoded recording
│   │   ├── game_log.json               ← events, snapshots, snapshot-derived changes
│   │   └── metadata.json              ← champion, mode, duration, offset, recording details, saved
│   │
│   └── {unix_timestamp}/
│       └── ...
│
└── clips/
    ├── {game_ts}_{clip_ts}.mp4         ← exported clip
    ├── {game_ts}_{clip_ts}.jpg         ← thumbnail sidecar (first frame, generated at export time)
    └── ...
```

No database. Everything is derived from these files at app launch. The game library is built by scanning `/games/` and reading each `metadata.json`.

---

## 5. Tech Stack

### Recorder (Process 1)
| Concern | Tool |
|---|---|
| Language | Rust (stable) |
| Async runtime | tokio |
| Process detection | sysinfo crate |
| HTTP client | reqwest (async) |
| JSON | serde + serde_json |
| Child process | tokio::process |
| Window detection | windows-rs (Windows), core-graphics (macOS) |
| System tray | tray-icon crate |
| Config | toml crate |
| Logging | tracing crate |

### App (Process 2)
| Concern | Tool |
|---|---|
| Desktop shell | Tauri v2 |
| UI | SolidJS + TypeScript |
| Styling | Plain CSS: global design tokens + component CSS modules |
| Graphs | Purpose-built SVG; Canvas only if profiling proves it necessary |
| Video | One persistent HTML5 `<video>` element via Rust local HTTP server |
| Clip export | ffmpeg via Rust backend |
| Video streaming | Rust HTTP server with byte-range support |
| Playback sync | `requestVideoFrameCallback`; `requestAnimationFrame`/media-event fallback |
| State | Solid signals and stores; no external state library |
| Navigation | Small typed in-app navigation state; no router dependency |
| File system | Narrow Rust commands; Tauri dialog plugin only for the native folder picker |
| Build | Vite |

#### Playback hot-path rules

- The `<video>` element owns decoding, buffering, seeking, playback rate, and audio. JavaScript never decodes video or copies frames through Canvas.
- The same `<video>` DOM node remains mounted while the replay changes between windowed and fullscreen layouts.
- Presented-frame time comes from `requestVideoFrameCallback().mediaTime`. Fallbacks exist for an older platform webview, but `timeupdate` is not the primary animation clock.
- Per-frame work is restricted to the playhead, visible clock, graph cursor, and nearest-event state. Marker positions and searchable event indexes are precomputed.
- Motion uses compositor-friendly `transform` and `opacity` wherever possible. Generic component, chart, animation, and state-management libraries are not added without a measured need.
- Playback validation records seek latency, process memory, and `getVideoPlaybackQuality()` frame counts against a large real recording in the actual Tauri webview.

### Shared
| Concern | Tool |
|---|---|
| Video processing | ffmpeg (bundled, not system-installed) |
| Distribution | Tauri Bundler (.msi + .exe, .dmg) |
| Auto-update | Tauri Updater plugin |

---

## 6. Platform Support

| Platform | Status | Screen capture | Audio | GPU |
|---|---|---|---|---|
| Windows 10+ | ✅ Supported | DXGI / gdigrab | dshow loopback | NVENC / AMF / QSV |
| macOS 12+ | ✅ Supported | ScreenCaptureKit / avfoundation | coreaudio | VideoToolbox (all) |
| Linux | ❌ Not supported | — | — | — |

macOS requires notarization (Apple Developer account, $99/yr) for distribution.

---

## 7. Constraints & Compliance

### Riot Games ToS — Permitted
- `localhost:2999` Live Client Data API
- Screen/window capture via OS APIs

### Riot Games ToS — Banned (must never implement)
- Direct game memory reading
- Process injection
- Any interaction with the game process beyond detection by name

### Vanguard Anti-Cheat
Architecturally identical to OBS, Medal, and Nvidia ShadowPlay — all Vanguard-compatible. No injection, no kernel drivers, standard OS APIs only.

## 8. Known Limitations

| Limitation | Notes |
|---|---|
| No spell cast data | Riot API does not expose this at any granularity. Video fills the gap. |
| No skillshot dodge detection | Riot tracks challenge totals, not per-game events. |
| Live Client gold/HP is local-player-only | Non-local values are stored as `null`; no team-gold or XP timeline is shown. |
| No minimap heatmaps | The local data source does not expose them. |
| No damage or vision totals | The local data source does not expose them. |
| Item timestamps are approximate | Snapshot-derived changes are accurate to the 10s polling interval. |
| Live Client API dies with game | Entire polling architecture exists for this reason. |

---

## 9. Defaulted Decisions

| Decision | Default | Rationale |
|---|---|---|
| Auto-delete duration | 30 days | Balances storage vs accessibility |
| Multi-monitor capture | Auto-detect League window | Avoids user configuration |
| Share mechanism | Open file in Finder/Explorer | User drags to Discord/Twitter |
| Clip pre/post-roll | Kill-type dependent (see `APP-VIEWER.md` §10.1) | Solo kills need less context than teamfights |
| Default recording profile | Auto-detected | Short hardware encode benchmark selects up to `high`; storage-heavy `very_high` remains an explicit override |
| Default recording codec | Auto | HEVC only when hardware encoding and app playback/seeking are both known to work; H.264 fallback |
| App frontend | Tauri 2 + SolidJS + TypeScript + Vite + plain CSS | Fine-grained DOM updates and direct media-element access without a bundled browser or general UI framework stack |

---

## 10. Rejected Approaches

### `.rofl` Replay Files
Rejected: expire after ~2 patches (~4 weeks); low quality; custom viewer requires client hooking (ToS risk) or format reverse engineering (legal grey area).

### Electron
Rejected: ~150MB bundle, ~300MB RAM at idle. Contradicts performance focus. Tauri provides identical DX with ~3–10MB bundle.

### Dear ImGui
Rejected: no native video player; building scrubber + graphs + clip UI in ImGui is a significant separate project; performance advantage is irrelevant for a post-game tool.

### React + General-Purpose Frontend Libraries
Rejected for this app: React can meet the performance target, but its component re-render model requires extra isolation for frame-frequency state. The viewer needs few ecosystem components, so SolidJS fine-grained updates plus direct DOM refs are a smaller and simpler fit. Tailwind, component kits, Recharts, animation libraries, routers, and external state stores are omitted; the UI uses purpose-built HTML, CSS, and SVG instead.

### Framework-Free TypeScript
Rejected: it removes a small runtime but makes this overlay-heavy application own component lifetime, cleanup, shared reactive state, and conditional DOM composition manually. SolidJS supplies those mechanics while retaining direct access to the native video element.

### Software Encoding
Rejected: CPU x264/x265 causes 5–15% FPS drop during gameplay. Hardware H.264/HEVC encoding (NVENC/AMF/QSV/VideoToolbox) achieves the required quality with much lower gameplay overhead.
