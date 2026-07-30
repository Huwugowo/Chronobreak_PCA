# APP-LIBRARY.md — Process 2: Game Library, Settings & Storage

> **Context:** This document covers the Game Library screen, the Settings screen, and all storage management logic. For data schemas and file paths, refer to `SPEC.md`. For the Viewer that the library opens into, refer to `APP-VIEWER.md`.
>
> **What this covers:** How the app discovers game bundles on disk, how they are displayed, how the user manages storage, and how the settings screen maps to config values.

---

## 1. App Shell

The app is a Tauri v2 window with a React frontend. The root level has **two tabs**:

```
[ Games ]  [ Clips ]
```

**Games tab** — all recorded game bundles as cards, most recent first.
**Clips tab** — all exported clips as cards.

Both tabs are library views. No video plays at this level. The tab selection persists across app launches (stored in local app state).

Below the tab bar, a persistent storage indicator is always visible (see Section 3.4).

**Full screen list:**

| Screen | Route | Document |
|---|---|---|
| Games tab | `/` | This file |
| Clips tab | `/clips` | This file |
| Viewer | `/viewer/:gameTimestamp` | `APP-VIEWER.md` |
| Settings | `/settings` | This file |

Navigation is handled by React Router. The Settings screen is accessible from any screen via a persistent icon in the top-right corner of the shell.

---

## 2. App Startup Sequence

On every launch, before rendering any UI:

```
1. Tauri backend scans {output_path}/games/
   → for each subdirectory, read metadata.json
   → if metadata.json missing: create a stub GameSummary with incomplete: true
     (video.mp4 may still be present from a crashed recording — show a card with limited info)
   → sort by recorded_at descending (incomplete stubs sort to bottom)

2. Run auto-delete job:
   → for each game where metadata.saved == false:
       → if recorded_at < (now - auto_delete_days):
           → delete entire game directory
   → incomplete bundles (no metadata.json) are NOT auto-deleted
     — they require manual deletion from the library

3. Return game list to frontend (includes incomplete stubs)
4. Render Games tab

5. If Riot ID is configured in settings:
   → in background, for each game where matchv5_fetched == false:
       → fetch Match V5 data (see SPEC.md §3.3)
       → on success: write matchv5 field to game_log.json, set matchv5_fetched: true in metadata.json
       → on failure: leave matchv5_fetched: false, log error, retry next launch
       → update game card badge in UI when complete (no full reload required)
```

The scan and auto-delete run in Rust (Tauri command). The Match V5 fetch runs as a background Tauri task after the UI is already visible — it never blocks the initial render.

---

## 3. Game Library Screen

### 3.1 Layout

- Grid of game cards, most recent first
- Responsive: 2 columns minimum, up to 4 columns on wide windows
- Storage indicator in the bottom-left corner (always visible)

### 3.2 Game Card

Each card is derived from `metadata.json` + derived values from `game_log.json`.

**Displayed on card:**
- Champion name (large, condensed bold)
- KDA — computed from `game_log.json` events:
  - Kills: `ChampionKill` events where `killer == local_player_summoner_name`
  - Deaths: `ChampionKill` events where `victim == local_player_summoner_name`
  - Assists: `ChampionKill` events where `assisters` contains `local_player_summoner_name`
- Game mode (`metadata.game_mode`)
- Game duration (formatted as `MM:SS` from `metadata.duration_ms`)
- Date recorded (formatted as `Mar 6, 2026 · 14:32`)
- Win / Loss badge — read directly from `metadata.win`. If `metadata.win` is `null` (win_method: "unknown"), show a neutral "?" badge until resolved by Match V5 enrichment
- "Saved" badge if `metadata.saved == true`
- "Incomplete" badge if bundle has no `metadata.json` (partial recording — champion and KDA will show as unknown, but the card is still clickable to open the video if video.mp4 is present)

**On hover:**
- "Delete" button appears on unsaved games and on incomplete bundles — never on saved games
- Delete requires a confirmation dialog before executing

**On click:**
- Navigate to `/viewer/{gameTimestamp}`

### 3.3 Empty State

If no games have been recorded yet:
- Large centered message: "No recordings yet"
- Subtitle: "Start League of Legends — the recorder will capture your next game automatically."
- No error, no action required

### 3.4 Storage Indicator

Persistent bottom-left element, visible on all screens.

Displays:
- Total disk usage of all game recordings (sum of video.mp4 sizes)
- Total disk usage of all clips
- Combined total

```
Recordings: 42.3 GB  ·  Clips: 1.2 GB  ·  Total: 43.5 GB
[Manage storage →]
```

Clicking "Manage storage" opens Settings directly at the storage section.

---

## 4. Clips Tab

**Layout:** Grid of clip cards, most recent first. Same column count as the Games tab.

**Each card displays:**
- Thumbnail (first frame of the clip — see thumbnail mechanism below)
- Duration (formatted as `MM:SS`)
- Source game info: champion name + date (derived from filename `{game_ts}_{clip_ts}.mp4` → look up game_ts in the games directory)
- File size

**Thumbnail mechanism:** At export time, the Rust backend extracts the first frame of the clip using ffmpeg and saves it as a `.jpg` sidecar alongside the clip:
```
{output_path}/clips/{game_ts}_{clip_ts}.mp4
{output_path}/clips/{game_ts}_{clip_ts}.jpg   ← thumbnail sidecar
```
The Clips tab loads thumbnails from these sidecars. If a sidecar is missing (e.g. for a clip created before this feature), the card shows a dark placeholder.

**On click:** plays the clip inline in a simple modal player — just a `<video>` element with basic controls. No stats tab, no scrubber enhancements. Clips are clean footage only.

**Actions:**
- "Open in Finder / Explorer" — opens the clips folder
- "Delete" — deletes both the `.mp4` and `.jpg` sidecar (requires confirmation)

Clips are never auto-deleted (they always have `saved: true`). The only way to delete a clip is manually from this tab or from Settings.

---

## 5. Settings Screen

Accessed via the persistent settings icon (top-right corner of any screen).

### 5.1 Recording Section

| Setting | Type | Default | Notes |
|---|---|---|---|
| Resolution | Select | `source` | `source` / `1920x1080` / `2560x1440` |
| Frame rate | Select | `60` | `30` / `60` |
| Bitrate | Slider | `20000` | 5000–50000 kbps, step 1000 |
| Output folder | Path picker | `~/LeagueReplays` | Opens native folder picker |

Changes to these settings are written immediately to `config.toml` and take effect on the next game recording. A note reads: "Changes take effect from the next recording."

### 5.2 Storage Section

| Setting | Type | Default | Notes |
|---|---|---|---|
| Auto-delete after | Select | `30 days` | 7 / 14 / 30 / 60 / 90 days / Never |

**Storage breakdown:**
```
Games:  42.3 GB  (12 recordings)
Clips:   1.2 GB  (4 clips)
Total:  43.5 GB

[ Clean up now ]
```

"Clean up now" runs the auto-delete job immediately and refreshes the display. It respects the `saved` flag — it only deletes eligible games.

**Per-game management:**
A scrollable list of all recorded games (same data as library cards, compact row format), each with:
- Champion + date + duration + size
- Toggle: "Saved" (prevents auto-delete)
- "Delete" button (requires confirmation)

### 5.3 Riot Account (Match V5 Enrichment)

| Setting | Type | Default | Notes |
|---|---|---|---|
| Riot ID | Text field | Empty | Format: `gameName#tagLine` e.g. `PlayerName#EUW` |
| API key | Password field | Empty | Dev key only — expires every 24h. Hidden in production key phase. |

When a Riot ID is configured, the app attempts to fetch Match V5 data for any game where `matchv5_fetched = false`. This happens automatically on app launch, in the background, after the game list is rendered.

Status per game is shown as a small badge on the game card: "Enriched" when Match V5 data is present.

If the fetch fails (rate limit, wrong ID, network error), the game remains `matchv5_fetched = false` and the app retries on the next launch.

### 5.4 System Section

| Setting | Type | Default | Notes |
|---|---|---|---|
| Start with OS | Toggle | On | Registers/removes OS startup entry |

**Read-only info:**
```
Detected GPU:    NVIDIA GeForce RTX 4070
Encoder in use:  h264_nvenc
App version:     1.0.0
```

---

## 6. Auto-Delete Logic

Runs on every app launch, before rendering.

```
for each game in /games/:
  if metadata.saved == false:
    age = now - metadata.recorded_at
    if age > config.auto_delete_days * 86400 * 1000:
      delete directory recursively
```

**Never deletes:**
- Any game with `metadata.saved == true`
- Any file in `/clips/` (clips are always saved)
- Incomplete bundles (no `metadata.json`) — these are left for the user to manage manually

---

## 7. Save / Unsave a Game

The `saved` flag in `metadata.json` is the only persistence mechanism.

**To save:** write `saved: true` to `metadata.json`.
**To unsave:** write `saved: false` to `metadata.json`.

This is a Tauri command that the frontend calls. It reads the file, modifies the field, and writes it back atomically.

---

## 8. Tauri Commands Required

The following Tauri commands (Rust → frontend) are required for this module:

| Command | Input | Output |
|---|---|---|
| `list_games` | — | `GameSummary[]` |
| `list_clips` | — | `ClipSummary[]` |
| `delete_game` | `gameTimestamp: string` | `Result<()>` |
| `delete_clip` | `clipFilename: string` | `Result<()>` (deletes .mp4 + .jpg sidecar) |
| `save_game` | `gameTimestamp: string, saved: bool` | `Result<()>` |
| `get_storage_usage` | — | `{ games_bytes, clips_bytes }` |
| `run_auto_delete` | — | `{ deleted_count: number }` |
| `open_output_folder` | — | Opens Finder/Explorer |
| `get_settings` | — | `AppConfig` |
| `save_settings` | `AppConfig` | `Result<()>` |
| `fetch_matchv5` | `gameTimestamp: string` | `Result<MatchV5Data>` |
| `get_matchv5_status` | `gameTimestamp: string` | `{ fetched: bool, error?: string }` |

### `ClipSummary` type (returned to frontend)

```ts
type ClipSummary = {
  filename:         string;   // e.g. "1741267920_1741268352"
  game_timestamp:   string;   // game_ts portion — used to look up source game
  clip_timestamp:   string;   // clip_ts portion — creation time
  duration_ms:      number;
  file_size_bytes:  number;
  thumbnail_path:   string | null;  // absolute path to .jpg sidecar, null if missing
  source_champion:  string | null;  // from source game metadata, null if game deleted
  source_date:      string | null;  // from source game metadata
};
```

### `GameSummary` type (returned to frontend)

```ts
type GameSummary = {
  timestamp:        string;
  champion:         string;
  game_mode:        string;
  duration_ms:      number;
  recorded_at:      string;  // ISO 8601
  kills:            number;  // derived from events
  deaths:           number;  // derived from events
  assists:          number;  // derived from events
  win:              boolean | null;  // null if win_method == "unknown"
  win_method:       "matchv5" | "derived" | "unknown";
  saved:            boolean;
  incomplete:       boolean;
  matchv5_fetched:  boolean;
  video_size_bytes: number;
};
```

KDA and win/loss are derived by the Rust backend when building this list. The frontend never reads `game_log.json` directly for the library view.

The `matchv5_fetched` flag drives the enrichment badge on the game card and the background fetch trigger on app launch.
