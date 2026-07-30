# PHASES.md — Build Order & Validation

> **How to use this document:** Work through phases in order. Do not start a phase until the previous phase's validation criteria are met. Each phase lists which documents to open alongside `SPEC.md` when working with an AI. The AI prompt template is at the end of this document.

---

## Phase Overview

| # | Name | Process | Documents needed |
|---|---|---|---|
| 1 | Recorder Core | Recorder | `SPEC.md` + `RECORDER.md` |
| 2 | Live Client Poller | Recorder | `SPEC.md` + `RECORDER.md` §5 |
| 3 | App Shell + Library | App | `SPEC.md` + `APP-LIBRARY.md` |
| 4 | Viewer — Windowed Mode | App | `SPEC.md` + `APP-VIEWER.md` §1–3 |
| 5 | Viewer — Fullscreen Overlays | App | `SPEC.md` + `APP-VIEWER.md` §4–12 |
| 6 | Viewer — Stats Tab | App | `SPEC.md` + `APP-VIEWER.md` §15 |
| 7 | Clip Creator + Exporter | App | `SPEC.md` + `APP-CLIP.md` + `APP-VIEWER.md` §11 |
| 8 | Match V5 Enrichment | App | `SPEC.md` §3.3–3.5 + `APP-VIEWER.md` §15 + `APP-LIBRARY.md` §5.4 |
| 9 | Polish + Distribution | Both | `SPEC.md` + `APP-LIBRARY.md` §5–6 |

---

## Phase 1 — Recorder Core

**Goal:** Game starts → recording starts → game ends → playable video saved on disk.

**Documents:** `SPEC.md` + `RECORDER.md`

**Scope:**
- Process watcher (detects League launch and close)
- ffmpeg spawn with hardware encoding (NVENC / AMF / QSV / VideoToolbox)
- Direct fragmented MP4 recording with two-second keyframe fragments
- Crash-tolerant output: completed fragments remain playable after interruption
- Video-only output: the game directory contains exactly `video.mp4`; no post-game remux
- System tray icon: idle (grey) and recording (red) states

Phase 1 does not connect to the League Client or Live Client APIs and does not create
`metadata.json`, `game_log.json`, or any temporary metadata file.

**Entry conditions:**
- Rust toolchain installed (`rustup`)
- ffmpeg binary available at a known path (system-installed is fine at this phase; bundled in Phase 9)
- A GPU with hardware encoding available

**Validation:**
1. Launch the recorder binary — tray icon appears (grey)
2. Launch League of Legends — tray icon turns red
3. Play through the loading screen and at least 5 minutes of a game
4. Close League (or surrender) — tray icon returns to grey
5. Check `~/LeagueReplays/games/{timestamp}/`:
   - `video.mp4` exists and is playable in VLC
   - `video.mp4` is the only file in the completed game directory
   - the tray returns to grey without a full-file conversion or system-wide I/O stall
6. Repeat with an unexpected League crash — `video.mp4` remains playable up to its last
   completed fragment (at most approximately two seconds may be lost)

**Complete when:** a completed game directory consistently contains only a playable
`video.mp4`, game-end finalization is effectively immediate, and interrupted recordings
remain playable up to their last completed fragment.

---

## Phase 2 — Live Client Poller

**Goal:** `game_log.json` is fully populated with events, snapshots, and snapshot-derived changes, all time-synced to the video.

**Documents:** `SPEC.md` + `RECORDER.md` §5

**Scope:**
- Pre-game `/allgamedata` calibration poller (detect valid `gameTime`, calculate `video_offset_ms` from five monotonic clock samples)
- Post-calibration **two concurrent async tasks**:
  - Event Loop: continuous polling of `/eventdata`, captures named events until the API becomes unavailable
  - Snapshot Loop: 10s polling of `/allgamedata`, appends snapshots, diffs consecutive snapshots to detect item changes and level-ups
- `game_log.json` written incrementally (atomic write after every batch)
- `metadata.json` written when the League game process closes, using data collected by the poller
- `video_time_ms` pre-computed on all events and snapshot-derived changes at write time
- Recorder writes `win: null` and `win_method: "unknown"`; post-game enrichment resolves the result through Match V5 or final-gold derivation
- The process watcher remains the only video stop trigger; neither `GameEnd` nor a Live Client API connection failure stops recording

**Entry conditions:**
- Phase 1 complete and validated

**Empirical decision — 2026-07-30:**
> The Live Client API became unavailable without a reliable `GameEnd`; the recorder must not use that event for stopping or win/loss. `GameStart` existed, but was first observed about 2.11 seconds after its own `EventTime`, so event arrival time must not be the video anchor. Synchronize with the median of five `response_received_monotonic - gameTime` samples. Excluding the initial bootstrap response, this clock relation stayed within a 1.8ms range during the capture.

**Validation:**
1. Play a full game (minimum 10 minutes, ideally to completion)
2. Inspect `game_log.json`:
   - `game_start_video_offset_ms` is present and nonzero (typically 120,000–240,000ms)
   - `snapshots` has one entry every ~10 seconds
   - `events` contains kill events with `video_time_ms` values
   - `snapshot_derived_changes` contains item purchases with approximate timestamps
3. Cross-check a kill event: seek `video.mp4` in VLC to `video_time_ms / 1000` seconds — kill should occur within ~1 second
4. Kill the recorder mid-game — confirm `game_log.json` on disk has all events up to the last write
5. Check `metadata.json`: `win` and `win_method` are present; `matchv5_fetched` is `false`

**Complete when:** `video_time_ms` on kill events is accurate to within 2 seconds when checked against the video.

---

## Phase 3 — App Shell + Library

**Goal:** App launches, shows the Games and Clips tabs, opens a game, plays video.

**Documents:** `SPEC.md` + `APP-LIBRARY.md`

**Scope:**
- Tauri v2 scaffolding (React + TypeScript + Tailwind + Vite)
- Root-level two-tab navigation: Games tab + Clips tab
- Rust backend: `list_games` command (scans `/games/`, derives KDA from events, returns `GameSummary[]`)
- Rust local HTTP server: streams `video.mp4` with range request support
- Games tab: grid of cards from `GameSummary[]`, save/unsave toggle, delete with confirmation
- Clips tab: grid of clip cards with thumbnail, duration, source game info
- Basic Viewer screen: just the `<video>` element and back button — no overlays yet
- Auto-delete job on launch
- Settings screen: output path + auto-delete duration (minimum viable — full settings in Phase 9)
- Data Dragon initialisation: on launch, check patch version and fetch/update item + champion cache

**Entry conditions:**
- Phase 2 complete and validated
- At least 3 game bundles on disk for library testing

**Validation:**
1. `npm run tauri dev` launches without errors
2. Games tab shows all recorded games with correct champion, KDA, win/loss badge, duration, date
3. Clicking a game card navigates to the viewer — video plays and is seekable
4. Video does not load into memory (RAM usage stable regardless of file size)
5. Clips tab renders correctly (place a test `.mp4` and matching `.jpg` sidecar manually into `{output_path}/clips/` to verify the tab displays the card with thumbnail — actual clip export with automatic sidecar generation is built in Phase 7)
6. Auto-delete removes games older than threshold (manually backdate `recorded_at` to test)
7. Data Dragon cache is written to disk — item IDs resolve to names in the browser console

**Complete when:** both library tabs work, video plays smoothly, and Data Dragon is initialised.

---

## Phase 4 — Viewer: Windowed Mode

**Goal:** The default viewer layout — windowed, panels always visible, video + data side by side.

**Documents:** `SPEC.md` + `APP-VIEWER.md` §1–3

**Scope:**
- Two-tab header: Replay / Stats (Stats tab is empty placeholder at this phase)
- Windowed layout: video ~60% width, stats panel alongside, scrubber below
- Stats panel: champion, KDA, current gold diff, CS, level — driven by `timeupdate` sync
- Scrubber: rail, fill, playhead tracking `currentTime`, event markers positioned by `video_time_ms`, clicking rail or marker seeks
- Controls row: play/pause button, time readout, fullscreen toggle button (fullscreen mode built in Phase 5)
- Event feed panel: scrollable list of events, active row highlights as playhead passes each event
- USG aesthetic throughout (Barlow Condensed + DM Mono, `#002FA7` blue)

**Entry conditions:**
- Phase 3 complete and validated

**Validation:**
1. Opening a game shows the windowed layout — video and panels visible simultaneously
2. Scrubber playhead tracks video in real time
3. Clicking the rail seeks correctly; clicking an event marker seeks to within 1 second of the event
4. Stats panel values update as the video plays (gold diff, CS, level change over time)
5. Event feed highlights the correct row as playhead passes each event
6. UI matches the USG aesthetic reference (`league-replay-usgfx-v3.jsx`)

**Complete when:** windowed viewer is fully functional and the aesthetic is consistent.

---

## Phase 5 — Viewer: Fullscreen Overlays

**Goal:** Fullscreen mode is fully functional with all overlay elements.

**Documents:** `SPEC.md` + `APP-VIEWER.md` §2.2, §4–12, §14

**Scope:**
- Fullscreen toggle: double-click video or F key → fills window, panels disappear, overlays appear
- Escape or F → returns to windowed mode, state preserved
- Top bar: champion, KDA, gold diff, clock, REC indicator — fades after 3.2s idle, reappears on mouse move
- Scrubber: ambient (44px) and active (96px) states, minute ticks, event markers
- Gold graph drawer: slides up from bottom, gold tab always visible
- Floating event card: animates in/out on event proximity, 3.5s auto-dismiss
- Right tick strip: always visible, proportional event dots, playhead needle
- Right event panel: slides in from tick strip click, event rows, gold/CS summary

**Entry conditions:**
- Phase 4 complete and validated

**Validation:**
1. Double-clicking video enters fullscreen — video fills window, panels gone, overlays present
2. Escape returns to windowed mode — playhead position preserved
3. Top bar fades after 3.2s, reappears instantly on mouse move
4. Scrubber ambient/active transition works on hover
5. Gold graph drawer opens and closes; cursor tracks playhead in real time
6. Event card animates in when playhead is within ~1 second of a kill, auto-dismisses after 3.5s
7. Tick strip dots are proportionally positioned; playhead needle moves in real time
8. Event panel slides in on tick strip click; clicking a row seeks to that event

**Complete when:** all overlay elements behave as specified in `APP-VIEWER.md` §14 state summary table.

---

## Phase 6 — Viewer: Stats Tab

**Goal:** The Stats tab shows a complete end-of-game scoreboard for all 10 players.

**Documents:** `SPEC.md` + `APP-VIEWER.md` §15

**Scope:**
- Stats tab renders scoreboard: two teams, 5 players each
- Collapsed row: champion icon, summoner spells, keystone, KDA, kill participation, CS, gold, level, items (resolved via Data Dragon)
- Multi-kill badge on row if applicable
- Expanded row (accordion): item build order with approximate timestamps (from `snapshot_derived_changes`), skill order (local player only at this phase — all players in Phase 8), full rune page
- Team aggregates bar: total kills, objectives, gold
- Local player row highlighted
- Win/loss header per team
- Graceful state when `win: null` — show "Result unknown" rather than Win/Loss

**Entry conditions:**
- Phase 5 complete and validated
- Data Dragon cache working (from Phase 3)

**Validation:**
1. Stats tab renders for every game in the library
2. All 10 players shown with correct champion icons (resolved via Data Dragon)
3. Items display correctly — IDs resolved to icons, not raw numbers
4. Clicking any player row expands it; clicking again collapses it; only one row open at a time
5. Build order shows items in the correct purchase sequence with approximate timestamps
6. Skill order shown for the local player; other players show "Skill order unavailable"
7. Team aggregates are correct

**Complete when:** Stats tab is accurate and all players' data is readable without Match V5.

---

## Phase 7 — Clip Creator + Exporter

**Goal:** User can create a clip, add music, export a valid MP4, and find it in the Clips tab.

**Documents:** `SPEC.md` + `APP-CLIP.md` + `APP-VIEWER.md` §11

**Scope:**
- Clip mode activation: clicking a kill marker or the Clip button on the scrubber (works in both windowed and fullscreen modes)
- Smart auto-positioning: heuristic mode (solo kill / teamfight / multikill / penta pre/post-roll windows) — see `APP-VIEWER.md` §11.1
- Draggable endpoint handles with 5-second minimum and event snapping
- "Export Clip →" navigates to Clip Exporter screen
- Music selection: No music / built-in library / import file
- Music looping: short tracks loop to fill the clip duration (never cut clip short)
- Audio mix sliders (game audio + music volume)
- ffmpeg export pipeline (Rust Tauri command, `-vcodec copy`)
- Thumbnail sidecar (`.jpg`) generated immediately after each export
- Export progress via Tauri events
- Post-export: "Open in Finder" + "Copy path" actions
- Exported clip and sidecar appear in Clips tab

**Entry conditions:**
- Phase 6 complete and validated
- At least one bundled music track in `resources/music/`

**Validation:**
1. Clicking a kill marker activates clip mode with correct heuristic window (solo kill: 8s pre / 5s post; test others)
2. Clip mode works correctly from both windowed and fullscreen viewer modes
3. Endpoint handles are draggable; 5-second minimum enforced
4. Export with a music track shorter than the clip — confirm clip is NOT truncated (music loops)
5. Clip export completes in under 5 seconds for a 30-second clip
6. Output plays in VLC with game audio + music correctly mixed for the full clip duration
7. Output plays in Discord (drag-and-drop share test)
8. `.jpg` sidecar is created alongside the `.mp4` in `{output_path}/clips/`
9. Exported clip appears in Clips tab with correct thumbnail, duration, and source game info

**Complete when:** a clip can be exported, shared externally, and found in the Clips tab.

---

## Phase 8 — Match V5 Enrichment

**Goal:** Match V5 data is fetched post-game, merged into the bundle, and upgrades the Stats tab and clip positioning.

**Documents:** `SPEC.md` §3.3–3.5 + `APP-VIEWER.md` §11.1, §15 + `APP-LIBRARY.md` §5.4

**Scope:**
- Settings: Riot ID field + API key field (dev key phase)
- Background enrichment on app launch: for every game with `matchv5_fetched: false`, fetch Match V5 match result + timeline and merge into `game_log.json` under the `matchv5` key (see `SPEC.md` §3.7)
- Set `metadata.json` `matchv5_fetched: true` on success; leave `false` and log error on failure
- Stats tab upgrades when Match V5 data present: damage dealt/taken, vision score, ward stats, CC score, time dead, exact item timestamps, skill order for all players
- Win/loss resolved from Match V5 `GAME_END` event for games where `win_method: "unknown"`
- Clip auto-positioning upgrades to precise mode: `victimDamageReceived` timestamps for exact fight start
- "Enriched" badge on game card in library when `matchv5_fetched: true`
- Degradation banner in Stats tab when `matchv5_fetched: false`

**Entry conditions:**
- Phase 7 complete and validated
- A personal Riot developer API key obtained from `developer.riotgames.com`

**Validation:**
1. Enter Riot ID + API key in settings — app fetches Match V5 for all existing games on restart
2. "Enriched" badge appears on game cards
3. Stats tab now shows damage dealt/taken columns, vision score, ward stats
4. Expanded rows show exact item timestamps (e.g. "3:24" not "~3:30")
5. Skill order shown for all 10 players (not just local player)
6. Clip auto-positioning for a kill with Match V5 data uses exact fight start — compare against heuristic to verify improvement
7. A game with `win_method: "unknown"` has its win/loss resolved after enrichment

**Complete when:** Match V5 data enriches the Stats tab and clip positioning, and the app degrades gracefully when Riot ID is not configured.

---

## Phase 9 — Polish + Distribution

**Goal:** App is ready for public distribution as an installer on a clean machine.

**Documents:** `SPEC.md` + `APP-LIBRARY.md` §5–6

**Scope:**
- ffmpeg bundled inside Tauri app (`resources/ffmpeg`); recorder resolves path at runtime — no system ffmpeg required
- Settings screen: all fields functional (resolution, FPS, bitrate, output path, auto-delete, autostart, Riot ID, API key)
- Storage breakdown in settings: per-section totals + per-game list with save toggle and delete
- Auto-update via Tauri Updater plugin (requires hosted update manifest URL)
- Code signing:
  - Windows: `.msi` + `.exe`, code signing certificate
  - macOS: `.dmg`, Apple Developer notarization ($99/yr account required)
- Installer testing on clean machines (no dev tools, no system ffmpeg)

**Entry conditions:**
- Phases 1–8 complete and validated
- Code signing certificates obtained
- Update server URL configured in `tauri.conf.json`
- At least 10 bundled music tracks in `resources/music/`

**Validation:**
1. `npm run tauri build` produces `.msi` and `.dmg` without errors
2. Install `.msi` on a clean Windows machine — recorder starts automatically with Windows, tray icon appears
3. Full Phase 1–8 validation passes on the installed build (not dev server)
4. App works with no system ffmpeg installed — uses bundled binary
5. Auto-update: push version bump, open old installed version, confirm update is offered and installs correctly

**Complete when:** the app installs and fully validates on a fresh machine with no prerequisites.

---

## Working With an AI on a Phase

For each phase, give the AI:

1. The full contents of `SPEC.md`
2. The full contents of every document listed in the phase's "Documents needed" column
3. This prompt prefix:

> "You are implementing Phase {N} ({Phase Name}) of the League Replay Tool. The full project architecture and data contracts are in SPEC.md. This phase focuses on {one-sentence description}. Build only what is in scope for this phase — do not implement future phases. If anything in the spec is ambiguous or missing, ask before assuming."

Keep conversations to one phase at a time. Start a fresh conversation for each phase. When validation passes, update this document to mark the phase complete before moving on.
