# League Replay Tool — Project Specification

> **How to use this document family:**
> This file is the shared vocabulary. Open it alongside whichever module document you are working on. It defines the data contracts, file paths, and enum values that all modules must agree on. Never modify the JSON schemas or file system layout without updating this file first.

---

## Document Family

| File | Scope |
|---|---|
| `SPEC.md` ← *this file* | Architecture, data contracts, constraints, limitations |
| `RECORDER.md` | Process 1 — Rust recorder binary |
| `APP-VIEWER.md` | Process 2 — Viewer screen (windowed + fullscreen modes, Stats tab, clip mode) |
| `APP-CLIP.md` | Process 2 — Clip exporter screen |
| `APP-LIBRARY.md` | Process 2 — Game library screen + settings |
| `PHASES.md` | Build order, entry conditions, validation criteria |

---

## 1. Project Overview

A two-process desktop application for League of Legends players that:
- **Records every game automatically** with zero meaningful performance impact using hardware-accelerated encoding
- **Enriches the recording** with granular game data polled from the League Live Client Data API during the game
- **Provides a post-game viewer** with a synchronized stats panel, event timeline, and gold diff graph
- **Enables frictionless clip creation** with auto-suggested moments, adjustable endpoints, optional music, and export as a universally compatible MP4 file

### Core Design Philosophy
- The recorder must be completely invisible and have zero performance impact on the game
- The app only runs post-game — performance constraints are relaxed on the app side
- Core functionality requires no external accounts, API keys, or credentials — the app works fully offline
- Match V5 enrichment (damage stats, vision score, exact item timestamps) is optional and requires a Riot ID — the app degrades gracefully without it
- Output is always a plain H.264 MP4 file — no proprietary formats, no upload links, no accounts

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
│    → polls /allgamedata until a valid gameTime is available  │
│      · calibrates game clock zero against video time         │
│    → spawns two concurrent async tasks once API is live:     │
│      · Event Loop: /eventdata, continuous polling            │
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
│    └── metadata.json     ← champion, win, matchv5_fetched    │
└─────────────────────────────────────────────────────────────┘
                            ↓ reads from disk
┌─────────────────────────────────────────────────────────────┐
│                  PROCESS 2 — APP                             │
│         (Tauri v2 + React + TypeScript, opened on demand)    │
│                                                              │
│  Games Tab     → grid of recorded game bundles               │
│  Clips Tab     → grid of exported clips                      │
│  Viewer        → windowed or fullscreen video player         │
│  Stats Tab     → end-of-game scoreboard, expandable rows     │
│  Clip Exporter → trim, music, ffmpeg export to MP4           │
└─────────────────────────────────────────────────────────────┘
```

The two processes share **only the file system**. No IPC, no sockets, no shared memory. The recorder writes, the app reads. They can run simultaneously without conflict.

---

## 3. Data Contracts

### 3.1 Data Sources Overview

Three data sources are used, with different availability and optionality:

| Source | When available | Required? | What it provides |
|---|---|---|---|
| `localhost:2999` Live Client API | During the game only | ✅ Always | Events, snapshots, real-time state |
| Riot Match V5 Timeline API | Post-game, via Riot servers | ⚙️ Optional | Damage stats, exact item timestamps, vision/CC/ward data, fight boundaries |
| Riot Data Dragon CDN | Anytime, static per patch | ✅ Always | Item names, item icons, champion icons |

The app is **fully functional without Match V5**. Features that depend on it degrade gracefully — columns are hidden rather than shown empty, and clip positioning falls back to heuristics. See Section 3.5 for the degradation map.

---

### 3.2 Live Client Data API

**Base URL:** `https://127.0.0.1:2999/liveclientdata/`

Note: The API uses HTTPS with a self-signed Riot certificate. Requests must either use the official Riot root certificate (`riotgames.pem`) or disable SSL verification. The recorder uses `reqwest` with certificate pinning or verification disabled.

**Two polling loops run concurrently post-GameStart** (see `RECORDER.md` Section 5 for implementation):

| Loop | Endpoint | Interval | Purpose |
|---|---|---|---|
| Event loop | `/eventdata` | As fast as response allows (~continuous) | Capture all named events with millisecond precision |
| Snapshot loop | `/allgamedata` | Every 10s | Capture player state (gold, HP, items, CS, level) |

**Named events from `/eventdata`** — these are the only events the API fires. This list is complete and confirmed from Riot's official static file:

| Event | Key fields | Notes |
|---|---|---|
| `GameStart` | EventTime | Logged as a semantic event; its arrival time is not a sync anchor |
| `MinionsSpawning` | EventTime | ~1:05 in every game |
| `FirstBrick` | EventTime, KillerName | First turret plate destroyed |
| `TurretKilled` | EventTime, TurretKilled, KillerName, Assisters | TurretKilled is the turret ID string |
| `InhibKilled` | EventTime, InhibKilled, KillerName, Assisters | |
| `InhibRespawningSoon` | EventTime, InhibRespawningSoon | Warning before respawn |
| `InhibRespawned` | EventTime, InhibRespawned | |
| `DragonKill` | EventTime, DragonType, Stolen, KillerName, Assisters | DragonType: Air/Earth/Fire/Water/Hextech/Chemtech/Elder |
| `HeraldKill` | EventTime, Stolen, KillerName, Assisters | Rift Herald |
| `BaronKill` | EventTime, Stolen, KillerName, Assisters | |
| `ChampionKill` | EventTime, VictimName, KillerName, Assisters | |
| `Multikill` | EventTime, KillerName, KillStreak | KillStreak: 2=Double … 5=Penta |
| `Ace` | EventTime, Acer, AcingTeam | |
| `GameEnd` | EventTime, Result | Not reliable; ignored for stop detection and win/loss |

> **`GameEnd` and win/loss:** Empirical capture showed that the Live Client API can disappear without exposing a `GameEnd` event. The recorder therefore never depends on `GameEnd`: API unavailability after successful polling is the game-stop signal. Win/loss comes from Match V5 `GAME_END`; if Match V5 is unavailable, derive it from the final gold differential when possible, otherwise set `win: null`. Record the derivation method as `win_method: "matchv5" | "derived" | "unknown"`.

**Items, level changes, and gold changes are NOT named events.** They are state fields in `/allgamedata` snapshots. They are detected by diffing consecutive snapshots:

| Change detected | How | Timestamp precision |
|---|---|---|
| Item purchased | Item appears in player's items array | Snapshot interval (~10s) |
| Item sold | Item disappears from player's items array | Snapshot interval (~10s) |
| Level up | Player's level field increases | Snapshot interval (~10s) |
| Gold change | Player's gold field changes | Snapshot interval (~10s) |

This means **build order timestamps from the Live Client API are approximate** — accurate to within the snapshot interval. Exact timestamps require Match V5 (see Section 3.3).

**Snapshots from `/allgamedata` (every 10s):** gold (current), HP (current + max), CS, XP, level, items (all slots) per player, plus runes and summoner spells (stable — only needed from first snapshot).

### 3.3 Match V5 Timeline API (Optional Enrichment)

**Base URL:** `https://{region}.api.riotgames.com/lol/match/v5/`

Fetched once, post-game, by the app on launch if a Riot ID is configured in settings. The result is merged into the game bundle and cached — the API is never called twice for the same game.

**Fetch sequence:**
```
1. GET /riot/account/v1/accounts/by-riot-id/{gameName}/{tagLine}  → PUUID
2. GET /lol/match/v5/matches/by-puuid/{puuid}/ids?count=1         → latest match ID
3. GET /lol/match/v5/matches/{matchId}                            → match result + per-player stats
4. GET /lol/match/v5/matches/{matchId}/timeline                   → full event timeline
```

**What Match V5 uniquely provides** (not available from Live Client API):

| Data | Source | Used for |
|---|---|---|
| `totalDamageDealtToChampions` | Match result | Recap stats tab |
| `totalDamageTaken` | Match result | Recap stats tab |
| `totalHealsOnTeammates` | Match result | Recap stats tab |
| `totalDamageShieldedOnTeammates` | Match result | Recap stats tab |
| `visionScore` | Match result | Recap stats tab |
| `wardsPlaced` / `wardsKilled` | Match result | Recap stats tab |
| `totalTimeCCDealt` | Match result | Recap stats tab |
| `totalTimeSpentDead` | Match result | Recap stats tab |
| `objectivesStolen` | Match result | Recap stats tab |
| `ITEM_PURCHASED` events with exact timestamps | Timeline | Exact build order |
| `ITEM_UNDO` events | Timeline | Accurate build history |
| `SKILL_LEVEL_UP` events for all players | Timeline | Skill order for all 10 players |
| `victimDamageReceived[]` timestamps per kill | Timeline | Exact fight-start for clip auto-positioning |
| `GAME_END` event with `winningTeam` | Timeline | Definitive win/loss |
| `DRAGON_SOUL_GIVEN` event | Timeline | Dragon soul in objective timeline |
| `TURRET_PLATE_DESTROYED` events | Timeline | Plate gold in objective timeline |

**API key progression:**

| Phase | Key type | User friction | Rate limit |
|---|---|---|---|
| Development | Personal dev key (free, instant) | User enters key in settings. Key expires every 24h. | 20 req/s |
| Early distribution | Production key (your key, server-side) | User enters Riot ID only. App calls your backend. | Higher, permanent |
| Scaled distribution | RSO (per-user OAuth) | "Connect with Riot" button — no manual input | Per-user |

The app backend (thin server, one endpoint) holds the production key. The distributed binary never contains a key. See Section 7 for compliance notes.

**Graceful degradation:** see Section 3.5.

---

### 3.4 Data Dragon (Static Data)

**Base URL:** `https://ddragon.leagueoflegends.com/`

Free Riot CDN, no authentication required. Used to resolve item IDs and champion IDs to display names and icon URLs.

**Fetch strategy:**
- On app launch, check current patch version: `GET /api/versions.json` → first element is current patch
- If cached version matches current patch: use cache
- If different: fetch fresh data and update cache
- Cache location: `{app_data}/ddragon/{patch_version}/`

**Data fetched:**
- `GET /cdn/{version}/data/en_US/item.json` — maps item ID → name, description, icon path
- `GET /cdn/{version}/data/en_US/champion.json` — maps champion key → name, icon path
- Icons: `GET /cdn/{version}/img/item/{itemId}.png` — fetched on demand, cached locally

The app never shows raw item IDs to the user. Every item ID from the Live Client API or Match V5 is resolved through this cache before display.

---

### 3.5 Degradation Map (Without Match V5)

When Match V5 data is unavailable (no Riot ID configured, or API call failed), the app degrades as follows:

| Feature | With Match V5 | Without Match V5 |
|---|---|---|
| Damage dealt / taken / healing columns | Shown | Hidden (not shown as empty) |
| Vision score | Shown | Hidden |
| Ward stats | Shown | Hidden |
| CC score / time dead | Shown | Hidden |
| Item build order timestamps | Exact (ms precision) | Approximate (±10s from snapshot) |
| Skill order | All 10 players | Local player only |
| Clip auto-positioning | Exact fight start from `victimDamageReceived` | Heuristic — see `APP-VIEWER.md` §11.1 |
| Win/loss | Definitive from `GAME_END` | Derived from final gold diff when possible |

**UI treatment:** a non-blocking banner at the top of the Stats tab reads: *"Connect your Riot account to unlock damage stats, vision score, and precise build timings. [Connect →]"* Users who ignore it get a complete experience minus those columns.

---

### 3.6 Timestamp Sync

The video recording starts when `League of Legends.exe` is detected — this includes the loading screen, typically 2–4 minutes before in-game clock 0:00.

**Sync mechanism:**
1. Record `video_start_monotonic_ms` when video capture starts. Use a monotonic clock, never UTC wall time, for synchronization.
2. Poll `/allgamedata` continuously until responses contain a valid `gameData.gameTime`. A valid response means the game is live; do not wait for the `GameStart` event.
3. For five rapid successful responses, record the monotonic timestamp immediately after receiving each response body and calculate:

   ```text
   game_zero_monotonic_ms =
       response_received_monotonic_ms - (gameTime_seconds × 1000)
   ```

4. Take the median of the five candidates. This rejects bootstrap or request-timing outliers.
5. Calculate the position of game clock zero in the video:

   ```text
   video_offset_ms =
       game_zero_monotonic_ms - video_start_monotonic_ms
   ```

6. Map every named event and snapshot-derived change using the timestamp supplied by the game clock:

   ```text
   video_time_ms = video_offset_ms + (EventTime_seconds × 1000)
   ```

   For snapshot-derived changes, substitute the snapshot's `gameTime` for `EventTime`.

`GameStart` is still written to the event log, but the time at which it is first observed is never used for synchronization. Event lists are cumulative and may expose `GameStart` after the game clock has already advanced. In the 2026-07-30 capture, `GameStart.EventTime` was `0.0095s` but the event was first observed at `gameTime = 2.1195s`; arrival-time anchoring would have made every marker about 2.11 seconds late. After the first bootstrap response was excluded, `response_received_monotonic - gameTime` stayed within a 1.8ms range over the capture.

### 3.7 `game_log.json` Schema

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
          "xp": 0,
          "level": 1,
          "items": [
            { "item_id": 1055, "slot": 0, "count": 1 }
          ],
          "summoner_spells": ["SummonerFlash", "SummonerIgnite"],
          "keystone_id": 8128,
          "rune_ids": [8128, 8126, 8138, 8135, 8304, 8345]
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
  ],
  "matchv5": null
}
```

`summoner_spells`, `keystone_id`, and `rune_ids` are only present on the **first snapshot** — they are stable for the entire game and do not need re-recording.

`video_time_ms` is pre-computed on each event and snapshot-derived change at write time.

`matchv5` is `null` until enrichment is fetched post-game. When populated it contains the merged Match V5 match result and timeline data. The app reads this field to populate damage/vision columns and exact item timestamps.

`snapshot_derived_changes` records item purchases, sales, and level-ups detected by diffing consecutive snapshots. Timestamps are approximate (±10s). Valid `change_type` values: `"ItemPurchased"`, `"ItemSold"`, `"LevelUp"`.

### 3.8 `metadata.json` Schema

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
  "recording_resolution": "1920x1080",
  "recording_fps": 60,
  "win": null,
  "win_method": "unknown",
  "matchv5_fetched": false,
  "saved": false
}
```

`win_method` values: `"matchv5"` (Match V5 `GAME_END`), `"derived"` (final gold differential fallback), `"unknown"`.

`matchv5_fetched: false` = app should attempt to fetch on next launch if Riot ID is configured.

`saved: false` = eligible for auto-deletion. All clips have `saved: true` by default.

---

## 4. File System Layout

```
{output_path}/                          ← default: ~/LeagueReplays
│
├── games/
│   ├── {unix_timestamp}/               ← one directory per game
│   │   ├── video.mp4                   ← hardware-encoded recording
│   │   ├── game_log.json               ← events, snapshots, snapshot-derived changes, matchv5 field
│   │   └── metadata.json              ← champion, mode, duration, offset, win, matchv5_fetched, saved
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
| Framework | Tauri v2 |
| Language | TypeScript + React 18 |
| Styling | Tailwind CSS |
| Graphs | Recharts |
| Video | HTML5 `<video>` via Rust local HTTP server |
| Clip export | ffmpeg via Rust backend |
| Video streaming | Rust `hyper` or Tauri asset protocol |
| State | React Context + useReducer |
| File system | Tauri fs plugin |
| Build | Vite |

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

### API Keys
The Live Client Data API requires no key. The Riot Match V5 API requires a key if used — the key is never bundled in the distributed binary. It is held server-side (production key phase) or entered by the user (dev key phase). See Section 3.3 for the key progression plan.

---

## 8. Known Limitations

| Limitation | Notes |
|---|---|
| No spell cast data | Riot API does not expose this at any granularity. Video fills the gap. |
| No skillshot dodge detection | Riot tracks challenge totals, not per-game events. |
| Gold/HP snapshots at 10s granularity | Sufficient for graph display. |
| No minimap heatmaps | Not in Live Client API or Match V5. Possible future addition if Riot exposes it. |
| Damage stats require Match V5 | Not available from Live Client API. App degrades gracefully without them. |
| Item timestamps approximate without Match V5 | Live Client API snapshots at 10s — exact timestamps need Match V5 post-game. |
| Live Client API dies with game | Entire polling architecture exists for this reason. |

---

## 9. Defaulted Decisions

| Decision | Default | Rationale |
|---|---|---|
| Auto-delete duration | 30 days | Balances storage vs accessibility |
| Multi-monitor capture | Auto-detect League window | Avoids user configuration |
| Share mechanism | Open file in Finder/Explorer | User drags to Discord/Twitter |
| Clip pre/post-roll | Kill-type dependent (see `APP-VIEWER.md` §11.1) | Solo kills need less context than teamfights |
| Default recording resolution | Match game resolution | No upscale/downscale artifacts |
| Default bitrate | 20 Mbps | ~5GB per 35min game |
| Match V5 enrichment | Optional, off by default | Requires Riot ID — zero friction for users who don't want it |

---

## 10. Rejected Approaches

### `.rofl` Replay Files
Rejected: expire after ~2 patches (~4 weeks); low quality; custom viewer requires client hooking (ToS risk) or format reverse engineering (legal grey area).

### Riot Match V5 as Primary / Live Data Source
Rejected as a live data source: unavailable during the game; requires credentials; 1-minute granularity too coarse for event timeline. **Accepted as an optional post-game enrichment source** — see Section 3.3.

### Electron
Rejected: ~150MB bundle, ~300MB RAM at idle. Contradicts performance focus. Tauri provides identical DX with ~3–10MB bundle.

### Dear ImGui
Rejected: no native video player; building scrubber + graphs + clip UI in ImGui is a significant separate project; performance advantage is irrelevant for a post-game tool.

### Software Encoding
Rejected: x264/x265 causes 5–15% FPS drop during gameplay. Hardware encoding (NVENC/AMF/QSV/VideoToolbox) achieves same quality at 1–3% overhead.
