# APP-VIEWER.md — Process 2: Viewer Screen

> **Context:** This document covers the Viewer screen only. For data schemas, file paths, and shared architecture, refer to `SPEC.md`. For the clip exporter that follows clip creation, refer to `APP-CLIP.md`. For the game library that precedes the viewer, refer to `APP-LIBRARY.md`.
>
> **What this screen does:** Plays back a recorded game bundle in windowed or fullscreen mode and provides clip selection on the synchronized timeline. The Stats tab described in Section 15 is deferred and is not exposed as an empty tab in the current app.

---

## 1. Screen Structure

The current Viewer is one Replay surface with a back action, a `▶ Replay` context label,
and a shared game header (champion, KDA, video duration, and date). No inactive Stats
control or placeholder is rendered while Phase 6 is deferred.

---

## 2. Replay — Two Render Modes

Replay has two distinct render modes. They are toggled by the user and share the same underlying state (playhead position, gold graph open/closed, clip range, etc.).

### 2.1 Windowed Mode (default)

The default view when a game is opened. The video occupies roughly 60% of the window. Panels sit alongside and below it — always visible, no interaction required to reveal data.

```
┌─────────────────────────────────┬───────────────────┐
│                                 │  Champion  KDA     │
│         VIDEO (60%)             │  Gold diff         │
│                                 │  CS  Level         │
│                                 ├───────────────────┤
│                                 │  Champion filter   │
│                                 │  Allies / Enemies  │
├─────────────────────────────────┴───────────────────┤
│  Scrubber  ·  Controls  ·  Timeline                  │
└──────────────────────────────────────────────────────┘
```

In windowed mode the scrubber and champion filter are always visible — no hover or click required. The gold graph is also visible when Match V5 timeline data is present; its space collapses cleanly when enrichment is unavailable. The USG panel layout (as prototyped in `league-replay-usgfx-v3.jsx`) is the reference for this mode.

### 2.2 Fullscreen Mode

Activated by: double-clicking the video, pressing `F`, or clicking the fullscreen button in the windowed controls.

The video expands to fill 100% of the window. The panel layout disappears. All UI elements become overlays floating above the footage. Sections 4–10 and 12–14 of this document (top bar, scrubber, gold graph, event card, and champion filter rail) define the fullscreen presentation. Section 3 (video playback architecture), Section 9 (shared champion-filter behaviour), and Section 11 (clip mode) apply to both modes.

Exited by: pressing `Escape`, pressing `F` again, or clicking the exit button that appears in the top bar. Returns to windowed mode with playhead and state preserved.

**What persists between modes:** playhead position, play/pause state, gold graph open/closed state, selected champion filters, and clip endpoints.

---

## 3. Video Playback Architecture

Video files are large (5–10GB). They must never be loaded into memory.

- The Rust backend runs a local HTTP server with **range request support**
- The SolidJS frontend uses one persistent HTML5 `<video>` tag pointed at `http://127.0.0.1:{port}/games/{timestamp}/video.mp4`
- Seeking, buffering, and playback rate are all handled natively by the browser engine
- No custom video decoding is implemented
- The video DOM node is not unmounted when the viewer changes between windowed and fullscreen modes

**Sync between video and data:**

The primary clock is `requestVideoFrameCallback`, which reports the media timestamp of each frame submitted to the compositor. Only the few DOM values that visually track playback subscribe to this hot signal:

```ts
const onVideoFrame: VideoFrameRequestCallback = (_now, frame) => {
  setVideoTimeMs(frame.mediaTime * 1000);
  frameCallbackId = videoElement.requestVideoFrameCallback(onVideoFrame);
};

frameCallbackId = videoElement.requestVideoFrameCallback(onVideoFrame);
```

If the platform webview lacks this API, use a `requestAnimationFrame` loop while playing and media events (`seeked`, `loadedmetadata`, `pause`) while stationary. `timeupdate` may assist that fallback but is never the primary animation clock.

Event arrays remain immutable and sorted by `video_time_ms`. The champion-filtered view and marker positions are recomputed only when selection changes; nearest-event lookup uses a moving cursor or binary search instead of scanning the complete list on every frame. The video time hot path must never cause the viewer component tree to rerun.

Game clock time is derived as: `game_clock_ms = video_time_ms - metadata.video_offset_ms`

---

## 4. Layout — Overview (Fullscreen Mode)

The viewer is a **fullscreen experience**. The video occupies 100% of the window. All UI elements are overlays that float above the footage. Nothing sits beside or below the video — the frame is never cropped or letterboxed to make room for panels.

**Guiding principle:** nothing requires the user to look away from the video. Every element appears over the footage at the moment it is relevant, and disappears when it is not.

```
┌───────────────────────────────────────────────────────┬──────┐
│  [TOP BAR — fades after 3.2s idle]                    │ ALL  │
│  REPLAY · JINX · 8/2/11 · +1800G      14:32 · REC    ├──────┤
│                                                       │ALLY  │
│                                                       │ [JI] │
│              VIDEO (fullscreen, 100%)                 │ [OR] │
│                                                       │  ·   │
│  [EVENT CARD — animates in on event proximity]        ├──────┤
│  ┌──────────────────┐                                 │ENEMY │
│  │ MULTI-KILL       │                                 │ [VI] │
│  │ DOUBLE KILL      │                                 │ [TH] │
│  │ JINX × 2 UNITS   │                                 │  ·   │
│  └──────────────────┘                                 │      │
│                                                       │      │
│  [GOLD GRAPH — slides up when triggered]              │      │
│  ┌─────────────────────────────────────────────────┐  │      │
│  │ Gold Differential          +1800G · ADVANTAGE   │  │      │
│  │ ▁▂▄▆▅▃▄▇██▅▃▄▆████                             │  │      │
│  └─────────────────────────────────────────────────┘  │      │
│  [▴ GOLD +1800]   ← present with Match V5 timeline    │      │
├───────────────────────────────────────────────────────┤      │
│  0:00 ·····|●|·····|●|·····|●|·····|●|·····|●· 32:14 │      │
│  [▶ Play] [▴ Gold] | 14:32 / 32:14 | [✦ Clip]        │      │
└───────────────────────────────────────────────────────┴──────┘
```

**Z-index layer order (bottom to top):**
1. `<video>` — base layer
2. Top bar overlay
3. Gold graph drawer
4. Gold tab
5. Scrubber zone
6. Champion filter rail (right side)
7. Event card
8. Grain texture overlay

---

## 5. Top Bar

**Position:** absolute, top: 0, left: 0, right: 0.

**Content (left to right):**
- App wordmark ("REPLAY") + company line in mono type
- Divider
- Champion name + KDA (kills / deaths / assists)
- **Right side:** match clock, REC indicator (pulsing dot), and current gold differential when Match V5 timeline data is available

**Visibility behaviour:**
- Fades **out** after **3.2 seconds** of mouse inactivity anywhere in the window. Transition: `opacity 1→0`, 350ms ease.
- Fades **in** instantly on any mouse movement anywhere in the window (`mousemove` event on the root container, resets a `setTimeout`). Transition: `opacity 0→1`, 150ms ease.
- When hidden: `opacity: 0`, `pointer-events: none` — does not intercept clicks.

**Interaction with event card:**
- When the top bar is visible, the event card sits at `top: 58px` (below the bar).
- When the top bar is hidden, the event card shifts to `top: 20px`. Transition: `top 200ms ease`.

---

## 6. Scrubber Zone

**Position:** absolute, bottom: 0, left: 0, right: 68px (leaves room for the champion filter rail).

### 6.1 Two States

**Ambient state** (default):
- Height: **44px**
- Visible: gradient fade (dark at bottom, transparent above), bare scrubber rail with event markers and playhead, current timestamp in small mono type (bottom-left)
- Gradient opacity: 60%

**Active state** (mouse is within scrubber zone):
- Height: **96px** — expands upward over 250ms, `cubic-bezier(.4,0,.2,1)`
- Reveals: minute tick marks, time labels, controls row
- Gradient opacity: 90%
- Returns to ambient 0ms after mouse leaves (no delay)

### 6.2 Scrubber Rail

- Full width (left: 0 to right edge of scrubber zone)
- Background track: 2px, `rgba(255,255,255,.15)`
- Fill (played portion): 2px, blue `#002FA7`, with subtle blue glow (`box-shadow: 0 0 6px #002FA7`)
- Playhead: 12×12px square, blue fill, white border — always visible in both states
- Transition on seek: `left 0.04s linear`

**Minute tick marks** (visible in active state only):
- Minor ticks every 1 minute: 4px tall, `rgba(255,255,255,.10)`
- Major ticks every 4 minutes: 8px tall, `rgba(255,255,255,.28)`
- Positioned absolutely above the rail

**Time labels** (visible in active state only):
- Shown at: 0:00, 8:00, 16:00, 24:00, match end time
- Font: monospace (TX-02), 7px, `rgba(255,255,255,.30)`

### 6.3 Event Markers

Event markers are **always visible** in both ambient and active states.

- Shape: small downward-pointing pin (8px × 12px: rectangle + triangle)
- Ally events: blue `#002FA7`
- Enemy events: red `#C2001C`
- Positioned at `left: (event.video_time_ms / total_duration_ms) * 100%`
- Clicking a marker seeks the video to that event's `video_time_ms`
- `e.stopPropagation()` — marker clicks do not propagate to the rail's seek handler

### 6.4 Controls Row

Visible in active state. Semi-visible (opacity 0.6) in ambient state.

**Layout (left to right):**
- `▶ Play` / `⏸ Pause` button — blue filled (primary action)
- `▴ Gold` toggle button — ghost style, toggles gold graph drawer; omitted without Match V5 timeline data
- Divider
- Time readout: `14:32 / 32:14` — mono type
- Divider
- `✦ Clip` button — ghost style, activates clip mode

---

## 7. Gold Graph Drawer

This drawer is rendered only when `matchv5` contains participant timeline frames. The Live Client API exposes current gold only for the active local player, so it cannot produce a truthful team differential and no estimate is shown.

### 7.1 Behaviour

- Position: absolute, left: 0, right: 68px, bottom: 0.
- **Closed:** `transform: translateY(100%)` — fully hidden below scrubber.
- **Open:** `transform: translateY(0)` — slides up over 300ms, `cubic-bezier(.4,0,.2,1)`.
- Background: `linear-gradient(to top, rgba(0,0,0,.92) 0%, rgba(0,0,0,.76) 100%)`.
- Border-top: 1px, `rgba(255,255,255,.07)`.
- Covers the lower ~25% of the video when open.

**Triggers to open:** clicking the gold tab, or clicking the "▴ Gold" button in controls.
**Triggers to close:** clicking either trigger again.

**Bottom padding:** dynamically adjusts — increases when scrubber is active (96px) vs ambient (44px), so graph content is never occluded by the scrubber expanding.

### 7.2 Gold Graph Content

**Header row:**
- Left: section label "Gold Differential" in condensed caps
- Right: current gold value + "ADVANTAGE" / "DEFICIT" label, coloured blue or red

**Graph:**
- X axis: match duration (0 to end)
- Y axis: gold differential (your team total − enemy team total), range ±4500
- Area above zero: faint blue fill
- Area below zero: faint red fill
- Line: blue when positive, red when negative, 1.8px stroke
- Vertical cursor: tracks current video time, same colour as line, with diamond marker at intersection
- Granularity: Match V5 participant-frame interval (typically 60s)
- Hovering the graph does **not** seek — it is read-only display. Seeking is via the scrubber only.

**X-axis labels:** 0:00, 8:00, 16:00, 24:00, match end — mono type.
**Y-axis labels:** +4K, +2K, 0, -2K, -4K — mono type.

### 7.3 Gold Tab

- Position: absolute, just above the scrubber zone (bottom adjusts with scrubber state), left: 20px.
- Present whenever Match V5 participant timeline frames are available; otherwise omitted.
- Opacity: 0.5 at rest, 1.0 when scrubber is active or drawer is open.
- Content: chevron icon (▴/▾) + "GOLD" label + current gold value (coloured blue or red).
- Clicking toggles the drawer open/closed.

---

## 8. Floating Event Card

### 8.1 Trigger

The frontend evaluates when the presented-frame clock crosses an event proximity boundary:

```ts
const nearEvent = nearestEvent(eventsByVideoTime, videoTimeMs, 1000);
```

When `nearEvent` changes to a new event (not the same event as currently shown):
- Dismiss current card (if any) with exit animation
- Show new card with enter animation

### 8.2 Appearance

- Position: absolute, top (variable — see top bar interaction), left: 20px.
- Background: `rgba(6,6,10,.93)`, border: `1px solid rgba(255,255,255,.10)`.
- Left border: 3px, blue for ally events / red for enemy events.
- `pointer-events: none` — never intercepts clicks.
- Min-width: 200px.

**Content:**
- Event category (e.g. "ELIMINATION") — mono, 7px, dimmed
- Event label (e.g. "DOUBLE KILL") — condensed bold, 17px, coloured blue or red
- Event detail line (e.g. "JINX × 2 UNITS") — mono, 8px, dimmed

### 8.3 Animations

**Enter:** `translateX(-12px) → translateX(0)`, opacity 0→1, 250ms, `cubic-bezier(.22,1,.36,1)`.

**Auto-dismiss:** after **3.5 seconds**, exit animation plays.

**Exit:** `translateX(0) → translateX(-8px)`, opacity 1→0, 220ms ease. Card is unmounted after exit completes.

---

## 9. Champion Timeline Filter

The event list is not rendered as a second navigation surface. The scrubber is the sole event timeline; the side UI controls which champion-related markers it contains.

### 9.1 Roster Source

The backend derives one compact replay roster from the first complete Live Client snapshot:

- `summoner_name`
- `champion`
- relation to the local player: `ally` or `enemy`

The roster is available without Match V5 and is computed once when the replay opens. Champion identity is displayed as a lightweight monogram until cached Data Dragon portrait delivery exists; the full champion and summoner names remain available in the windowed panel, tooltip, and accessible label.

### 9.2 Filter Semantics

- Default: no champion selected; all event markers are shown.
- Clicking a champion toggles that champion independently. Allies and enemies can be selected together.
- With one or more champions selected, an event remains visible when any selected summoner appears as its `killer`, `victim`, `assister`, or `acer`.
- Riot taglines are ignored and names are compared case-insensitively, so `Player` matches `Player#EUW`.
- System and team-only events with no participant identity are hidden while a champion filter is active.
- `ALL` clears every selection and restores the complete timeline.
- Filtering changes only marker/card visibility. It never seeks, pauses, or changes the playhead.
- Selection persists when switching between windowed and fullscreen modes.

The filtered array is derived only when selection changes. Playback-frame updates continue to use the pre-sorted result and binary nearest-event lookup.

### 9.3 Windowed Presentation

The lower half of the right panel contains two compact columns, Allies and Enemies, with five champion buttons each. Every button shows champion identity and summoner name. Ally controls use blue accents; enemy controls use red. The local player has a small white corner indicator. Selected buttons receive a team-coloured border and background.

---

## 10. Fullscreen Champion Rail

**Position:** absolute, top: 0, bottom: 0, right: 0.
**Width:** 68px. Never hides and never expands over the video.
**Background:** `rgba(4,5,8,.82)` with a subtle left border.

The rail contains:

1. `ALL` reset button
2. `ALLIES` label and five compact champion buttons
3. `ENEMIES` label and five compact champion buttons

Each champion button is a 38px square using the same ally/enemy and selected states as the windowed panel. Hover and accessible labels expose the champion and summoner name. At short window heights the controls compact to 34px rather than becoming scroll-driven.

The top bar, gold drawer, and scrubber end at `right: 68px`; the filter therefore never obscures playback controls. The same selection signal drives both the windowed panel and this rail, so entering or exiting fullscreen does not reset the filter.

---

## 11. Clip Mode Activation

Clip mode is triggered from the viewer. It does not navigate to a new screen — it modifies the scrubber in place.

**Triggers:**
- Clicking any event marker on the scrubber (also seeks to that event)
- Clicking the "✦ Clip" button in the controls row

**On activation:**
- Smart default clip window is calculated (see below)
- Two draggable endpoint handles appear on the scrubber rail
- The rail fill between endpoints is highlighted distinctly (lighter blue)
- A "Export Clip →" button replaces the "✦ Clip" button in controls
- A Cancel action, or `Escape` in windowed mode, exits clip mode

**Dragging endpoints:**
- Handles are draggable along the rail
- Minimum clip duration: 5 seconds
- Endpoints snap to event marker positions when within 2 seconds
- Both endpoints are clamped to 0 and total duration

**On "Export Clip →":** navigate to the Clip Exporter screen (see `APP-CLIP.md`), passing `{ gameTimestamp, clipStartMs, clipEndMs }`.

When the Clip button is used without a marker, a kill within ten seconds of the
playhead is used as the anchor. Otherwise the playhead itself receives the standard
8-second pre-roll and 5-second post-roll. Non-kill markers use that same standard
window.

---

### 11.1 Clip Auto-Positioning

The default clip window is calculated at activation time. Two modes depending on Match V5 data availability.

**With Match V5 data (precise mode):**

The `CHAMPION_KILL` event in the Match V5 timeline includes `victimDamageReceived[]` — an array of every damage instance the victim took, each with a timestamp in game-clock milliseconds. The earliest timestamp is when the fight actually started.

Match V5 timestamps are in game-clock time and must be converted to `video_time_ms` before use:

```
fight_start_game_ms = min(victimDamageReceived[].timestamp)
clip_start = (fight_start_game_ms - 1000) + game_log.game_start_video_offset_ms
clip_end   = kill_event.video_time_ms + 3000
```

`kill_event.video_time_ms` is already pre-computed in `game_log.json` at write time — use it directly. Only the fight-start timestamp from Match V5 requires the conversion.

**Without Match V5 data (heuristic mode):**

Fall back to kill-type-aware windows based on the event's fields:

| Kill type | Detection | Pre-roll | Post-roll |
|---|---|---|---|
| Solo kill | `assisters` array is empty | 8s | 5s |
| Kill with assisters (teamfight) | `assisters` has ≥1 entry | 15s | 8s |
| Multi-kill | `Multikill` event fired within 5s | 20s | 10s |
| Penta kill | `Multikill.KillStreak == 5` | 25s | 15s |

Pre-roll is always longer than post-roll — the fight buildup is the interesting part, not the aftermath.

Both modes produce a starting point that the user can then adjust by dragging the endpoint handles.

---

## 12. Typography System

Three type faces used throughout the viewer:

| Alias | Actual font | Weight | Use |
|---|---|---|---|
| TX-96 | Barlow Condensed | 900 | Champion name, large clock, KDA digits, stamp |
| TX-76 | Barlow Condensed | 800 | Nav, buttons, section labels, event labels |
| TX-02 | DM Mono / Berkeley Mono | 400 | All data values, timestamps, mono readouts |

**Rule:** TX-02 for any number or technical string. TX-76 for any label or action. TX-96 for any hero display element.

---

## 13. Colour Reference

| Name | Hex | Use |
|---|---|---|
| Blue | `#002FA7` | Ally events, scrubber fill, play button, gold positive |
| Blue Light | `#4A70E0` | Gold graph line (positive), glow accents |
| Red | `#C2001C` | Enemy events, gold negative, death states |
| White | `#F8F5EF` | Primary text on dark backgrounds |
| Ink | `#0C0C0C` | Video background |

---

## 14. State Summary — Fullscreen Mode Only

The following table describes visibility behaviour for overlay elements. These elements only exist in fullscreen mode (Section 2.2). In windowed mode, all panels are always visible and no hover/idle logic applies.

| Element | Always visible | Trigger to show | Trigger to hide |
|---|---|---|---|
| Video | ✅ | — | — |
| Top bar | On mouse activity | Any mouse move | 3.2s idle timer |
| Scrubber (ambient) | ✅ | — | — |
| Scrubber (active) | — | Mouse enters scrubber zone | Mouse leaves |
| Gold tab (with Match V5 timeline) | ✅ (dim) | — | — |
| Gold graph drawer (with Match V5 timeline) | — | Click gold tab or Gold button | Click again |
| Event card | — | Playhead within ~1s of event | 3.5s auto-dismiss |
| Champion filter rail | ✅ | — | — |
| Filtered marker set | No selection = all events | Select one or more champions | Click `ALL` |

---

## 15. Stats Tab

> **Deferred:** This remains future product context, not part of the current build or a
> prerequisite for clip creation. Until it is scheduled again, the Viewer exposes no
> Stats tab or placeholder.

The Stats tab is a static end-of-game scoreboard. No video plays. No animation. Pure data.

### 15.1 Layout

```
┌──────────────────────────────────────────────────────────────┐
│  ← Back   JINX  8/2/11  +3800G  32:14  Mar 6 2026           │
│  [ ▶ Replay ]  [ ≡ Stats ●]                                  │
├──────────────────────────────────────────────────────────────┤
│  [Connect Riot account to unlock damage & vision stats →]    │  ← only if matchv5_fetched = false
├──────────────────────────────────────────────────────────────┤
│  BLUE TEAM  ·  VICTORY                                       │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ Champion  Spells  KDA   CS    Gold  Items              │  │
│  │ ─────────────────────────────────────────────────────  │  │
│  │ ▸ Jinx ★  F/I    8/2/11  187  14.2k  [items row]      │  │  ← local player, expandable
│  │   Thresh   F/I   3/4/12  22   9.1k   [items row]      │  │
│  │   ...                                                  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  RED TEAM  ·  DEFEAT                                         │
│  ┌────────────────────────────────────────────────────────┐  │
│  │   ...                                                  │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### 15.2 Scoreboard Columns

Always shown (available from Live Client API):

| Column | Data source | Format |
|---|---|---|
| Champion icon + name | Snapshot / Data Dragon | Icon + TX-76 name |
| Summoner spells | First snapshot | Two spell icons |
| Keystone rune | First snapshot | Icon |
| KDA | Kill events | K / D / A with kill participation % |
| CS | Last snapshot `scores.creepScore` | Integer + CS/min |
| Level | Last snapshot `level` | Integer |
| Items | Last snapshot items array | 6 item icons + trinket, resolved via Data Dragon |
| Multi-kill badge | Multikill events | Double / Triple / Quadra / Penta |

Shown only when `matchv5_fetched = true`:

| Column | Match V5 field |
|---|---|
| Damage dealt | `totalDamageDealtToChampions` |
| Damage taken | `totalDamageTaken` |
| Vision score | `visionScore` |
| Wards placed / killed | `wardsPlaced` / `wardsKilled` |
| Gold | Match result `goldEarned` | Formatted (14.2k) |

When Match V5 columns are hidden, a compact notice appears in the column header area rather than empty columns.

### 15.3 Expanded Row

Clicking any player row expands it inline (accordion). The row height grows to reveal:

**Build timeline** — item icons in purchase order, each with a timestamp below it:
- With Match V5: exact timestamps from `ITEM_PURCHASED` events
- Without Match V5: approximate timestamps from snapshot diffs, displayed as "~8:40"

**Skill order** — Q/W/E/R boxes in the order they were levelled:
- With Match V5: all 10 players
- Without Match V5: unavailable

**Full rune page** — keystone + 5 runes + 3 stat shards when Match V5 is available. Without enrichment, the local player's Live Client rune IDs are shown; other players show the keystone only.

**Additional stats** (Match V5 only, shown when available):
- Healing done (self) and to teammates
- CC time dealt
- Time spent dead
- Objectives stolen badge

Clicking the row again collapses it. Only one row can be expanded at a time.

### 15.4 Team-Level Aggregates

Below each team's player rows, a summary bar:

- Total team kills / deaths
- Total objectives (dragons, baron, heralds, turrets, inhibitors)
- Total team gold when Match V5 data is available; otherwise omitted

### 15.5 Typography and Colour

Follows the system-wide USG aesthetic. Numbers in TX-02, labels in TX-76, champion names in TX-76. Local player row has a subtle blue-tinted background. Win team has a faint blue header, losing team a faint red header. Items shown at 28px icon size in the collapsed row, 32px in expanded.
