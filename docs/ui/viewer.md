# Chronobreak Viewer

## 1. Purpose

Opening a recording puts Chronobreak into a **replay-first environment**.

The Viewer is not a dashboard containing a video. The replay itself is the primary product surface. Application chrome, metadata, filters, playback controls, and clip tools exist to support the replay and must not compete with it.

The Viewer has two presentation modes:

- **Windowed Viewer** — preserves the minimum application navigation required to leave the replay while making the game dominate the available window.
- **Fullscreen Viewer** — removes normal application chrome and presents the replay as an immersive League-inspired HUD.

The two modes should share the same playback, timeline, filtering, and clip concepts rather than behaving like unrelated interfaces.

---

## 2. Core principles

1. **The game is primary.**
   - Video receives the largest useful share of the Viewer.
   - Avoid dashboard-style composition around the replay.

2. **One timeline, one seek model.**
   - The Chronobreak event timeline is the canonical seek bar.
   - Do not introduce a second generic video seek bar.

3. **Controls behave like video-player controls.**
   - Play/pause, time, volume, speed, and fullscreen belong to the video-player control layer.
   - Do not place them in a separate large dashboard panel.

4. **Replay information is contextual, not ornamental.**
   - Persistent UI must justify the space it occupies.
   - Avoid large metadata headers, cards, and duplicated match information.

5. **League replay mode is a structural reference, not a visual skin.**
   - Team champion positioning and champion-based event filtering are useful familiar patterns.
   - Do not recreate the entire spectator HUD.

6. **Preserve existing replay behavior while changing presentation.**
   - Playback, seeking, clip editing, event mapping, diagnostics infrastructure, and media behavior must not be casually rewritten as part of the UI redesign.

---

## 3. Windowed Viewer

The Windowed Viewer should feel close to fullscreen while preserving the application navigation required to leave the replay and reach other product surfaces.

### 3.1 Navigation

Use compact replay-specific chrome rather than preserving the full Library composition unchanged.

The navigation surface must provide:

- a clear route back to Match History;
- continued access to essential top-level application destinations/actions where required;
- compact replay identity only when useful.

The exact compact navigation composition is a visual implementation decision.

The Viewer does not require:

- a large hero header;
- a large champion title block;
- dashboard-style match summary cards;
- permanent development diagnostics.

The regular application header should not consume significant replay space merely for consistency with browsing screens.

### 3.2 Replay composition

The primary Windowed Viewer composition is:

1. compact Viewer navigation;
2. compact allied roster;
3. dominant replay/video surface;
4. compact enemy roster;
5. Chronobreak timeline;
6. only supporting actions that cannot live naturally in the player/timeline.

Conceptually:

`allied roster | video | enemy roster`

with the canonical timeline immediately associated with the replay.

The replay should begin as high in the usable window as practical.

---

## 4. Video surface

The video frame is the primary Viewer surface.

Avoid unnecessary nested borders, panels, cards, and containers around it.

### 4.1 Player control overlay

In Windowed Viewer mode, ordinary playback controls live inside the video surface as a conventional player overlay.

The control layer may contain:

- play / pause;
- current time and total duration;
- mute / volume;
- playback speed;
- fullscreen;
- Clip entry when appropriate.

The player control overlay must **not** contain its own seek track. Seeking belongs to the canonical Chronobreak timeline.

These controls should not require a separate large horizontal panel beneath the replay.

Transport controls may become visually quieter while the pointer is inactive, provided discoverability, accessibility, and playback state remain clear.

### 4.2 Video interaction

Existing direct video interactions may remain where useful, including:

- click to play/pause;
- double-click to enter fullscreen.

Avoid introducing gestures that conflict with timeline seeking or clip editing.

---

## 5. Canonical Chronobreak timeline

The Chronobreak timeline **replaces the ordinary video seek bar**.

It must preserve the familiar interaction model of a standard seek control while adding replay-specific information.

### 5.1 Transport behavior

The timeline supports:

- click to seek;
- pointer drag / scrub;
- visible current position;
- played/progress indication;
- a clear playhead;
- keyboard seeking where supported;
- current/total time presentation nearby or in the player controls.

Seeking remains the timeline's primary job.

Event visualization must not make basic seeking difficult or imprecise.

### 5.2 Replay annotations

The same timeline may display:

- replay events;
- champion-filtered events;
- selected/current event state;
- clip selection;
- clip start/end handles.

Do not create a second event rail that duplicates the same time axis without a demonstrated need.

### 5.3 Hover

The timeline may support useful hover feedback such as:

- timestamp;
- relevant event information;
- thumbnail preview if the existing playback/media architecture can provide it efficiently.

Thumbnail preview is not required for the initial Viewer UI refactor and must not trigger speculative media/backend work.

---

## 6. Champion HUD and event filtering

Champion selection is part of replay navigation, not a separate analytics dashboard.

### 6.1 Team presentation

Present the two teams as compact vertical champion rosters:

- allied team on the left;
- enemy team on the right.

Each roster represents five players when authoritative participant/team data is available.

Champion portrait/icon is the primary visual identifier.

Avoid:

- large champion cards;
- text-heavy permanent panels;
- recreating the complete League spectator HUD.

Additional player/champion information may appear only when it remains compact and useful.

### 6.2 Windowed presentation

In Windowed Viewer mode, the two champion rosters flank the video as narrow replay-adjacent rails.

They should normally sit outside/adjacent to the video image rather than consuming a large internal overlay area.

The video remains the dominant surface.

When width becomes constrained, roster presentation should simplify before the replay is reduced to an unusable size.

### 6.3 Fullscreen presentation

In Fullscreen Viewer mode, the champion rosters are overlaid on the left and right edges of the video, inspired by League replay/spectator mode.

The overlays must remain compact and must not substantially obscure gameplay.

### 6.4 Filtering interaction

Champion selection filters the events shown on the Chronobreak timeline.

The initial interaction contract is single-champion filtering:

- default state shows events for everyone;
- clicking one champion filters the timeline to events involving that champion;
- clicking a different champion switches the filter to that champion;
- clicking the currently selected champion again, or using an equivalent clear action, restores all events;
- selected state is visually obvious;
- filtering events does not seek the replay or alter playback state.

Do not introduce multi-champion filtering unless a later product decision specifically requires it.

Reuse existing participant/event filtering logic where possible even if the current presentation/state shape changes.

---

## 7. Match and live-state information

Do not maintain a permanent large `LIVE STATE` dashboard beside the replay.

Information such as:

- current K / D / A;
- level;
- CS;
- game clock;
- champion/player identity;
- final match K / D / A;
- capture date;

may still be useful, but should be presented compactly, contextually, or on demand.

Do not duplicate information simply because it exists in the data contract.

The Viewer should remain understandable if most of this metadata is visually absent.

---

## 8. Clip workflow

Clip creation is a first-class Viewer action but must not create a second replay-navigation model.

### 8.1 Entering clip mode

Clip mode uses the canonical Chronobreak timeline.

When active, the timeline may add:

- selected range;
- start handle;
- end handle;
- endpoint editing feedback.

Existing frame-aligned clip behavior must be preserved.

### 8.2 Clip controls

`Clip`, `Cancel`, preview, and export controls should remain compact and directly associated with the player/timeline.

Do not restore a large separate clip-control dashboard.

Exact placement may differ between Windowed and Fullscreen modes as long as the same clip state and interaction model are preserved.

### 8.3 Export

Actual clip encoding/export remains outside the Viewer presentation contract.

The Viewer produces/edits the clip range and hands the resulting draft to the existing export flow.

---

## 9. Fullscreen Viewer

Fullscreen is an expansion of the same Viewer, not a separate product mode.

Fullscreen removes normal application navigation and uses the video as the visual canvas.

The fullscreen HUD may contain:

- left/right champion roster overlays;
- the canonical Chronobreak timeline;
- playback controls;
- clip controls when clip mode is active;
- lightweight contextual replay information.

The canonical timeline and playback controls may overlay the lower portion of the video in fullscreen rather than reserving permanent external layout space.

Transient transport/timeline controls may auto-hide while idle where doing so does not harm usability.

Champion rosters are not required to auto-hide with transport controls; their persistence should be decided based on replay readability.

Moving between Windowed and Fullscreen Viewer must preserve:

- playback position;
- playing/paused state;
- champion event filter;
- clip selection;
- relevant playback settings.

---

## 10. Existing presentation to remove or demote

The Viewer redesign should not preserve the following merely because they already exist:

- oversized champion/game hero header;
- permanent `LIVE STATE` side panel;
- large `CHAMPIONS` filter panel;
- separate large playback-controls row;
- permanent normal-user playback diagnostics;
- excessive nested boxes/borders around replay regions.

Do not introduce a separate generic seek bar alongside the Chronobreak timeline.

Underlying behavior may be reused even when its current visual container disappears.

---

## 11. Diagnostics

Playback diagnostics are development/benchmark tooling.

They must not occupy persistent space in the normal Viewer.

Diagnostics may remain available through a development-only mechanism or explicit diagnostic mode.

Do not delete useful instrumentation merely to hide it from the product UI.

---

## 12. Window sizing

The Viewer is desktop-first.

When available space decreases:

1. protect useful video size;
2. simplify champion-roster presentation;
3. compress nonessential metadata;
4. compress spacing;
5. hide/defer low-priority contextual information.

Do not preserve large dashboard panels by shrinking the replay.

Do not introduce a mobile card layout for the Windows desktop product.

---

## 13. Accessibility

The Viewer must preserve:

- keyboard-accessible back/navigation actions;
- keyboard-accessible playback controls;
- keyboard-accessible timeline seeking;
- accessible names for champion filters;
- a non-color-only selected champion state;
- accessible clip handles and endpoint descriptions;
- clear focus treatment;
- accessible fullscreen entry/exit.

Custom timeline behavior must preserve slider semantics or provide an equivalent accessible interaction model.

---

## 14. Performance

The Viewer is performance-sensitive because playback occurs while the UI is active.

UI work should avoid:

- per-frame reactive work unrelated to visible playback state;
- heavy effects on large video-adjacent surfaces;
- unnecessary per-event DOM expansion;
- new large UI/icon dependencies;
- duplicated timeline rendering systems.

Do not begin native/media-engine work unless a required Viewer interaction demonstrates a concrete limitation in the existing playback architecture.

---

## 15. Production data

Only show semantic facts supported by authoritative Viewer/replay data.

Do not fabricate:

- team membership;
- champion/player identity;
- match outcome;
- live statistics;
- event ownership;
- queue/mode identity.

If the desired left/right team presentation cannot be derived authoritatively from the current participant contract, treat that as a specific data-contract gap rather than guessing team membership in the UI.

Development fixtures may exercise richer states than production data currently exposes, but fixture assumptions are not part of the canonical Viewer contract.

---

## 16. Implementation discipline

The Viewer redesign is primarily a frontend/UI refactor.

Prefer reuse of existing behavior and data:

- playback controller;
- video adapter;
- timeline/replay tick mapping;
- event markers;
- participant data;
- event filtering helpers;
- clip range logic;
- fullscreen state;
- playback settings.

Only create backend/native work when the UI exposes a specific missing capability or missing authoritative data.

Implement in validated checkpoints rather than as one monolithic rewrite.

Recommended sequence:

1. Viewer shell and replay-first composition;
2. canonical timeline geometry;
3. video-overlay playback controls;
4. left/right champion HUD and single-champion event filtering;
5. fullscreen convergence;
6. clip-mode integration;
7. diagnostics/product cleanup;
8. accessibility, minimum-window, and final visual review.

---

## 17. Change discipline

Update this spec when a material Viewer product/UX decision changes.

Do not update it for:

- temporary fixture changes;
- benchmark mechanics;
- migration-only implementation steps;
- tiny optical corrections;
- filenames used during the refactor.

Patterns that become application-wide should move into `chronobreak-ui-foundations.md` rather than being duplicated here.