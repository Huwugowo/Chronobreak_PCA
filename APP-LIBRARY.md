# APP-LIBRARY.md — Process 2: Game Library, Settings & Storage

> **Context:** This document covers the Game Library screen, the Settings screen, and all storage management logic. For data schemas and file paths, refer to `SPEC.md`. For the Viewer that the library opens into, refer to `APP-VIEWER.md`.
>
> **What this covers:** How the app discovers game bundles on disk, how they are displayed, how the user manages storage, and how the settings screen maps to config values.

---

## 1. App Shell

The app is a Tauri v2 window with a SolidJS frontend. The root level has **two tabs**:

```
[ Games ]  [ Clips ]
```

**Games tab** — all recorded game bundles as compact match-history rows, most recent first.
**Clips tab** — all exported clips as cards.

Both tabs are library views. No video plays at this level. The tab selection persists across app launches (stored in local app state).

Below the tab bar, a persistent storage indicator is always visible (see Section 3.4).

**Full screen list:**

| Screen | Navigation state | Document |
|---|---|---|
| Games tab | `{ screen: "library", tab: "games" }` | This file |
| Clips tab | `{ screen: "library", tab: "clips" }` | This file |
| Viewer | `{ screen: "viewer", gameTimestamp }` | `APP-VIEWER.md` |
| Settings | `{ screen: "settings", returnTo }` | This file |

Navigation is a small typed Solid store, not a URL router. It retains only the current screen and the minimal return state required for Back. The Settings screen is accessible from any screen via a persistent icon in the top-right corner of the shell.

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
```

The scan and auto-delete run in Rust through narrow Tauri commands. No network
account lookup or post-game enrichment job runs at startup.

---

## 3. Game Library Screen

### 3.1 Layout

- Full-width vertical match-history list, most recent first
- One compact row per recording; the list never changes into a card grid
- Wide rows use fixed columns for KDA, duration, and file size; narrower windows progressively collapse secondary columns while preserving champion, date, and KDA
- Storage indicator in the bottom-left corner (always visible)

### 3.2 Game Row

Each row is derived from `metadata.json` + derived values from `game_log.json`.

**Displayed on row:**
- Champion name (large, condensed bold)
- Champion portrait from the versioned Data Dragon cache, with a monogram fallback while unavailable
- Both summoner-spell icons and the keystone icon captured in the first snapshot
- Final item build from the last local-player snapshot, including the trinket slot
- KDA — computed from `game_log.json` events:
  - Kills: `ChampionKill` events where `killer == local_player_summoner_name`
  - Deaths: `ChampionKill` events where `victim == local_player_summoner_name`
  - Assists: `ChampionKill` events where `assisters` contains `local_player_summoner_name`
- Game mode (`metadata.game_mode`)
- Game duration (formatted as `MM:SS` from `metadata.duration_ms`)
- Date recorded (formatted as `Mar 6, 2026 · 14:32`)
- "Saved" badge if `metadata.saved == true`
- "Incomplete" badge if bundle has no `metadata.json` (partial recording — champion and KDA show as unknown, but the row still opens when `video.mp4` is present)
- Compact save/unsave and delete actions at the right edge; delete remains unavailable while a game is saved

**On hover:**
- The row receives a subtle highlight and directional cue without changing its height
- Delete still requires a confirmation dialog before executing

**On click:**
- Navigate to `/viewer/{gameTimestamp}`

The row requests image assets through the local app server. Icons are downloaded lazily from Data Dragon, cached by patch and kind, and then browser-cached as immutable files. Manifest or network failure never blocks the library: the row keeps its fixed layout and falls back to its monogram or empty icon slots.

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

**Layout:** Responsive grid of clip cards, most recent first. The Clips tab keeps its visual-card layout because thumbnails are the primary selection cue.

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
| Recording quality | Select | `auto` | `auto` / `very_low` / `low` / `medium` / `high` / `very_high`; each profile resolves resolution, FPS, bitrate, and encoder speed together |
| Codec | Select | `auto` | `auto` / `h264` / `hevc`; auto uses HEVC only after this app validates playback and seeking, otherwise H.264 |
| Output folder | Path picker | `~/LeagueReplays` | Opens native folder picker |

Changes to these settings are written immediately to `config.toml` and take effect on the next game recording. A note reads: "Changes take effect from the next recording."

When quality is `auto`, show the concrete recommendation returned by recorder
diagnostics (for example, "Detected: High · 1080p60"). A "Detect again" action reruns
the short encoder benchmark. Do not expose vendor-specific presets or separate raw
bitrate controls in the normal UI; the five portable profiles are the product
contract across Windows and macOS.

### 5.2 Storage Section

| Setting | Type | Default | Notes |
|---|---|---|---|
| Auto-delete after | Select | `30 days` | 7 / 14 / 30 / 60 / 90 days / Never |

`Never` is stored as `auto_delete_days = 0`. Zero disables age-based deletion; it
does not mean "delete immediately."

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

### 5.3 System Section

| Setting | Type | Default | Notes |
|---|---|---|---|
| Start with OS | Toggle | On | Registers/removes OS startup entry |

**Read-only info:**
```
Encoder in use:  NVENC
Selected format: HEVC · High · 1080p60
App version:     1.0.0
```

---

## 6. Auto-Delete Logic

Runs on every app launch, before rendering.

```
for each game in /games/:
  if config.auto_delete_days != 0 and metadata.saved == false:
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
| `open_clips_folder` | — | Opens the clips directory in Finder/Explorer |
| `get_settings` | — | `Settings` |
| `save_settings` | `SettingsUpdate` | `Settings` |
| `get_hevc_probe_status` | — | `{ tested, supported, probe_url }` |
| `record_hevc_probe_result` | `supported: boolean` | Updated probe status |
| `get_ddragon_status` | — | Patch, cache state, and catalog counts |
| `resolve_item_name` | `itemId: string` | `string | null` |

### `ClipSummary` type (returned to frontend)

```ts
type ClipSummary = {
  filename:         string;   // e.g. "1741267920_1741268352"
  game_timestamp:   string;   // game_ts portion — used to look up source game
  clip_timestamp:   string;   // clip_ts portion — creation time
  duration_ms:      number;
  file_size_bytes:  number;
  thumbnail_path:   string | null;  // absolute path to .jpg sidecar, null if missing
  thumbnail_url:    string | null;  // range server URL, null if the sidecar is missing
  video_url:        string;         // range server URL for modal playback
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
  saved:            boolean;
  incomplete:       boolean;
  video_size_bytes: number;
  video_available:  boolean;
};
```

KDA is derived by the Rust backend when building this list. The frontend never
reads `game_log.json` directly for the library view. No match-result badge is shown
because the supported local data sources do not provide a reliable final result.
