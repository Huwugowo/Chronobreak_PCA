# PHASES.md — Build Order & Validation

> This is the standalone project roadmap. `SPEC.md` owns shared architecture and data contracts; the module documents own detailed behavior. A phase is complete only after its validation has passed. Superseded development schemas and code paths are deleted rather than migrated.

---

## Phase Overview

| # | Milestone | Status | Outcome |
|---|---|---|---|
| 1 | Recorder Core | Complete | League launch and close produce a playable, crash-tolerant video |
| 2 | Live Client Poller | Complete | Events and player snapshots are synchronized to video |
| 3 | App Shell + Library | Complete | Recorded games can be browsed and played |
| 4 | Viewer — Windowed Mode | Complete | Replay, timeline, filters, and player state work together |
| 5 | Viewer — Fullscreen Overlays | Complete | The replay has responsive fullscreen controls and overlays |
| 6 | Viewer — Stats Tab | Deferred | Reconsider only if a local-data scoreboard proves useful |
| 7 | Clip Creator + Exporter | Complete | Publishable Discord, horizontal, and vertical clips can be exported |
| 8 | Windows Self-Contained Beta | Next | A clean Windows PC can install and use the complete local workflow |
| 9 | Public Distribution | Later | Signed, updateable Windows/macOS releases are ready for general users |

Phase 6 is not a prerequisite for Phases 7–9.

---

## Phase 1 — Recorder Core

**Status:** Complete.

**Goal:** Game starts → recording starts → game closes → playable video is saved.

**Documents:** `SPEC.md` + `RECORDER.md`

**Delivered:**

- Two-second League process watcher
- Background ffmpeg capture with hardware H.264/HEVC capability selection
- Unified recording quality profiles
- Direct fragmented MP4 output with no post-game remux
- Crash-tolerant `video.mp4`
- Grey idle and red recording tray states
- No visible ffmpeg console window

**Passed when:** completed and interrupted games both leave immediately playable video without a long game-end freeze.

---

## Phase 2 — Live Client Poller

**Status:** Complete.

**Goal:** Events, snapshots, and snapshot-derived changes are synchronized to the recording.

**Documents:** `SPEC.md` + `RECORDER.md` §5

**Delivered:**

- `/gamestats` calibration accepts five strictly advancing, clock-consistent `gameTime` samples
- Cumulative `/eventdata` polling every second
- Concurrent `/gamestats`, `/activeplayer`, and `/playerlist` snapshots every ten seconds, with optional shared event recovery
- Item and level changes derived from consecutive snapshots
- Atomic incremental `game_log.json` writes
- Final `metadata.json` with recording, local-player, and clock-offset details
- `video_time_ms` precomputed for events and derived changes
- Independent polling failure handling and aggregate diagnostics

**Empirical decisions:**

- `GameStart` arrival time is not a synchronization anchor.
- API responsiveness is not a start signal because `gameTime` can remain frozen during loading.
- `GameEnd` is optional telemetry and is never used to stop video or infer a result.
- The League game process remains the sole video lifecycle signal.
- Cumulative events make one-second polling sufficient; faster polling adds load without preventing missed events.
- Non-local gold and HP are unavailable and remain `null`; XP is omitted. No team-gold timeline is fabricated.

**Passed when:** early-, middle-, and late-game markers land within roughly two seconds, logs remain complete, and a long game shows no polling-related progressive slowdown or exit freeze.

---

## Phase 3 — App Shell + Library

**Status:** Complete.

**Goal:** The app opens recorded games and exported clips through a lightweight desktop library.

**Documents:** `SPEC.md` + `APP-LIBRARY.md` + `APP-VIEWER.md` §3

**Delivered:**

- Tauri 2 + SolidJS + TypeScript + Vite + plain CSS shell
- Games and Clips tabs
- Filesystem-derived library; no database
- Save, delete, and retention controls
- One persistent HTML video element served by a local byte-range server
- Responsive seeking for multi-gigabyte recordings
- Presented-frame playback clock and diagnostics
- One-time HEVC playback/seek capability check with H.264 fallback
- Data Dragon cache for champion and item presentation

**Passed when:** real recordings can be browsed, played, and rapidly sought without memory scaling with file size or UI-induced progressive frame loss.

---

## Phase 4 — Viewer: Windowed Mode

**Status:** Complete.

**Goal:** Video, synchronized state, and navigation remain visible and responsive in the default viewer.

**Documents:** `SPEC.md` + `APP-VIEWER.md` §1–3

**Delivered:**

- Windowed replay surface with video, player state, timeline, and controls
- Timeline markers positioned by `video_time_ms`
- Click-to-seek rail and markers
- Presented-frame champion, KDA, CS, and level state
- Multi-select allied and enemy champion filtering
- Purpose-built modern visual system without a UI component framework

**Passed when:** playback, seeking, state updates, and filters behave correctly across real recordings.

---

## Phase 5 — Viewer: Fullscreen Overlays

**Status:** Complete.

**Goal:** Fullscreen replay navigation feels immediate while preserving the same playback state.

**Documents:** `SPEC.md` + `APP-VIEWER.md` §2.2 and §4–13

**Delivered:**

- Fullscreen toggle without remounting the video element
- Idle-fading top bar and responsive controls
- Ambient and active timeline states
- Floating event card
- Ten-player champion filter rail shared with windowed mode
- Preserved playhead and filter state between layouts

**Passed when:** fullscreen transitions, overlays, markers, and filters work without seeking, pausing, or losing state unexpectedly.

---

## Phase 6 — Viewer: Stats Tab

**Status:** Deferred indefinitely. It is not required by any current milestone, and the app renders no empty Stats tab or placeholder.

**Reason:** The local game source can support a modest scoreboard, but it cannot truthfully provide a final result, team gold, damage totals, vision totals, complete skill order, or complete rune pages for every player. That limited view is not currently valuable enough to justify another product surface.

**If reconsidered:** define the user problem first. Keep the implementation local-only and restrict it to data already stored in game bundles, such as champion, KDA, CS, level, items, objectives, and approximate build order.

---

## Phase 7 — Clip Creator + Exporter

**Status:** Complete and passed at annotated tag `phase-7` (`0fc8f7e`).

**Goal:** A replay moment can become a small, publishable clip and appear in the Clips library.

**Documents:** `SPEC.md` + `APP-CLIP.md` + `APP-VIEWER.md` §10

**Delivered:**

- Clip selection from kill markers or the timeline clip action
- Heuristic kill-type pre/post-roll windows
- Draggable endpoints, five-second minimum, and event snapping
- Discord preset with strict sub-10,000,000-byte verification and corrective retry
- Horizontal 16:9 and vertical 9:16 publishing presets
- Vertical blurred-context layout with adjustable sharp action crop
- No music, bundled music, and imported music workflows
- Looping music plus separate game/music volume controls
- H.264/AAC MP4 re-encode with hardware preference and software fallback
- Thumbnail sidecar and Clips-library integration
- Export progress and post-export file actions

**Passed when:** H.264 and HEVC source recordings export playable Discord, horizontal, and vertical H.264/AAC clips with correct duration, framing, audio, size, thumbnail, and library metadata.

---

## Phase 8 — Windows Self-Contained Beta

**Status:** Next.

**Goal:** A user on a clean Windows machine can install the product, leave the recorder running, play a game, review it, and export a clip without installing ffmpeg, Rust, Node, or any developer tool.

**Documents:** `SPEC.md` + `RECORDER.md` + `APP-LIBRARY.md` §5–6 + `APP-CLIP.md`

**Scope:**

- Bundle `recorder.exe`, `ffmpeg.exe`, and `ffprobe.exe` as Windows application resources
- Give the app and recorder one explicit packaged-tool path contract; remove dependence on system `PATH`
- Start exactly one background recorder instance from the installed app, with no console window
- Add a working Windows-login autostart toggle for the recorder
- Finish the settings needed for real use: recording profile, codec preference, output path, retention, and autostart
- Build a Windows installer containing everything required by recording, playback, probing, and export
- Show concise actionable errors when capture or packaged media tools cannot start
- Include required third-party notices for bundled binaries and assets
- Validate installation, upgrade-over-beta, and uninstall behavior without adding compatibility code for development recordings

**Explicitly out of scope:**

- Stats tab
- macOS packaging
- Public code signing and notarization
- Automatic updater or hosted update service
- Hosted game-data services or user accounts
- Expanding the bundled music library beyond what is needed to validate the workflow

**Entry conditions:**

- Phases 1–5 and 7 complete
- Phase 6 remains optional

**Validation:**

1. Build the Windows installer from a clean checkout.
2. Install it on a Windows machine with no system ffmpeg, Rust, or Node.
3. Launch the app; exactly one recorder tray process runs and no terminal appears.
4. Enable login startup, reboot or sign out/in, and confirm the recorder starts once without opening the app window.
5. Record a real game from loading screen through process exit; verify video, synchronized timeline, and metadata.
6. Open the installed app, rapidly seek the recording, filter markers, and enter/leave fullscreen.
7. Export and play one Discord clip and one vertical or horizontal clip using only bundled tools.
8. Change each shipped setting and confirm the next recording or retention run uses it.
9. Uninstall and confirm application binaries and startup registration are removed; user recordings remain intact.

**Complete when:** the entire local workflow passes on a clean Windows machine with no prerequisites and no developer commands.

---

## Phase 9 — Public Distribution

**Status:** Later, after Windows beta feedback.

**Goal:** Turn the proven beta into maintainable public Windows and macOS releases.

**Expected scope:**

- Resolve beta reliability, onboarding, and installer issues before broadening platforms
- Sign the Windows release
- Add macOS capture/tool packaging, signing, and notarization
- Add an updater only when a real release channel and hosting model exist
- Finalize product naming, icons, licensing, privacy text, diagnostics, and release documentation
- Validate fresh install, upgrade, rollback/recovery, and uninstall on supported OS versions

**Not assumed:** a backend, user account system, external match-data service, or Stats tab. Each requires a separate product decision based on demonstrated user value.

**Complete when:** signed builds install, update, run the full workflow, and uninstall cleanly on every supported platform.

---

## Working With an AI on a Phase

For each phase, provide `SPEC.md`, every module document listed for that phase, and this prompt:

> You are implementing Phase {N}, {Phase Name}, of the League Replay Tool. The full project architecture and data contracts are in SPEC.md. Build only what is in scope for this phase. Do not preserve superseded development schemas or implementation paths. If the current documents leave a material product decision unresolved, ask before assuming.

Keep implementation and validation focused on one active milestone. When validation passes, update this roadmap and create an annotated phase tag before starting the next milestone.
