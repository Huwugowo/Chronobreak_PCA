# QB-DEV-002: Real-data preprod for desktop UI development

## Purpose

Make the normal current desktop app useful immediately for replay-overlay and other UI development: real Master Yi Swiftplay data, a clearly synthetic placeholder video, real SQLite import, working searches, playback, saves and clip export. Support ordinary frontend hot reload under the same isolation. Prioritize this usable workflow over fixture infrastructure.

## Relevant planning-time architecture

The filesystem bundle is authoritative: `video.mp4`, strict v2 `metadata.json`, v2 telemetry manifest `game_log.json`, committed `league_observations.jsonl`, `league_checkpoint.json`, and optional match/result sidecars. SQLite is a disposable app-side projection. `library.rs` owns scanning/descriptors, `league_index` owns import and queries, and `clip_export.rs` owns export. Existing media delivery and replay-time validation remain unchanged. See `docs/architecture/media-library.md`, `replay-time.md` and `media-runtime.md` for current contracts.

Normal startup already accepts `LEAGUE_REPLAY_CONFIG` and `LEAGUE_REPLAY_OUTPUT_PATH`. Mutable native caches derive from app-local data, which needs one generic absolute-directory override. WebView profile isolation uses `WEBVIEW2_USER_DATA_FOLDER`. `QUEUEBACK_MEDIA_RUNTIME_DIR` selects the existing hash-validated staged runtime. Benchmark mode is unsuitable because it changes behavior and blocks mutations.

## Scope and non-goals

Use actual recording data from `Huwugowo/Chronobreak_PCA` commit `7802292d5fa6b9c1a59f8bcc667bd55513adc756`. No original MP4 is available, and the user explicitly requests a meaningless substitute. Keep the main Swiftplay recording and a separate Practice Tool edge-case dataset. Add a small owned launch/reset script, generic app-data isolation, and bounded app-side roster/event association work if required by the real data.

No recorder implementation changes, fake APIs, prepopulated SQLite, special preprod branches in consumers, artificial corrected clocks, general data generator, standalone test framework, custom playback stack, installer work or new asset-network abstraction. Preserve the separate browser preview. Do not claim visual gameplay/event synchronization from placeholder video.

## Exploration findings

The archive README/manifest identify recorder source `42a14671da790156d486de8c26c65908edecc593`, runtime r6, original sidecar hashes, and omitted videos. Preserve the original real identities and observations as supplied; do not silently anonymize/rewrite the committed journals.

`games/1792431584` is Master Yi Swiftplay, queue 890. Its original metadata declares H.264 High, 1920x1080, exact 60 FPS, first video PTS 0, time base 1/15360, 60,398 frames and one-past-last PTS 15,461,888. Video coverage is 1,006.633333 seconds; container/audio duration is 1,006.634667 seconds. Audio was recorded as silent stereo fallback. `games/1792432630` is Akali Practice Tool: 7,915 frames, 131.916667 seconds of video, 131.925333 seconds container/audio. Neither MP4 is in GitHub.

Swiftplay contains ten participants and 108 event revisions: the first GameStart has no referenced calibration and stays unmapped; 107 subsequent events map inside video coverage. Concrete UI points include MinionsSpawning at replay 52.430140 seconds and the final Multikill at 1,002.359883 seconds. Practice Tool GameEnd maps to approximately 1,291.859996 seconds, beyond its 131.916667-second video. Compute these from each observation's own calibration, never from a guessed current mapping.

Roster identities and event actor strings use different representations: exact full Riot IDs versus names without tags. The shared producer identity resolver deliberately requires full identity; merely relaxing it would change recorder behavior and would not repair existing stored unresolved actor references. Any correction belongs in the app's rebuildable projection and must preserve raw evidence and ambiguity.

## Chosen design and rationale

### Real data and placeholder media

Keep byte-identical supplied sidecars under `fixtures/preprod-library/` with concise provenance, original hash references, and a table of useful actual UI scenarios. Swiftplay is the default dataset; Practice Tool remains separate for adverse timing cases. Do not require an invented full match before using real data.

Create each needed placeholder MP4 once with the existing locked runtime, not through a shipped generator framework. Swiftplay comes first: generate and prove exactly 60,398 frames at the original frame grid, dimensions, time base and H.264 High profile, with a simple visual labelled synthetic and silent AAC audio. A changing clock or pattern is helpful if existing filters support it, but is not a prerequisite. Probe actual streams, frame count and boundaries and fully decode before accepting the fixture.

Preserve the original `metadata.json` separately with its archive hash. The playable fixture's ordinary `metadata.json` must be a clearly documented derived file describing the actual synthetic media: every `media_timeline` field must agree with measured generated output and truthful producer/runtime evidence, including audio/container facts. Do not carry native capture/mux statistics over as evidence for generated bytes. Validate the derived file through the existing strict production parser without relaxing its contract. Keep the exact original video grid so recorded game calibrations remain usable; use actual generated audio coverage rather than fabricating a match. Keep all telemetry/match/result sidecars byte-identical. Deliberately retain the original media UUID only to bind this immutable real telemetry to its non-original surrogate, record that fact in fixture provenance, and isolate it from any original recording library. Record both original metadata and surrogate hashes. Never call this original media identity/content or visual alignment evidence.

The checked-in original source remains pristine; the separately identified derived metadata is part of the prepared playback fixture. All saves, clips and cache writes use a copied working library. A README must prominently state that this tests recorded data, UI state, seek mechanics and export, but cannot establish visual event alignment or original capture quality. Practice Tool timing is not repaired to fit its substitute.

### Small normal-app launch

Add `LEAGUE_REPLAY_APP_DATA_DIR` at normal startup: absent uses the existing Tauri default, present must be an absolute usable path and fails explicitly if invalid. No consumer knows about preprod.

One `tools/preprod_launch.ps1` uses a fixed ignored `build/preprod` root with `library`, dedicated config, `app-data`, and `webview2`. First use copies prepared fixture files. Repeated launches preserve saves and clips. `-Reset` explicitly restores the working copy and clears owned disposable state while the app is stopped. `-Dev` launches ordinary Tauri dev with frontend mocks disabled for hot reload; otherwise launch the normal release build. Document a fresh `npm run desktop:build --prefix app` before release use. Keep build/runtime identity and verification notes in the existing execution checkpoint, not a new receipt subsystem.

Set child-scoped config, app-data, WebView and runtime variables; pin `LEAGUE_REPLAY_OUTPUT_PATH` to the owned library throughout the session, overriding any inherited value or Settings edit. Disable retention and set autostart false in the dedicated config. Clear mock/benchmark activation variables. Restore caller environment in `finally`. Do not launch the recorder or alter Windows startup registration.

Essential safety stays small and explicit: canonical fixed-root checks, a versioned ownership sentinel bound to that root, no fixture/destination reparse traversal, refusal to reuse an existing unowned directory, and no reset while the owned app session is running. Verify absolute resolved bounds before recursive deletion; delete only the owned working tree. Copy files rather than hardlinking originals. Do not add a general process manager, configurable workspace manager or locking framework.

Use normal DDragon refresh/cache handling and verify the sample's actual champion/item/spell/rune assets in the desktop UI. Reuse a small existing native-cache seed if needed for reliable first use. Missing unrelated/offline assets should degrade through existing behavior. A new offline mode or HTTP injection abstraction is outside scope.

### Roster-to-event relationship

Preserve source actor strings, journal hashes, producer status and participant IDs. The app may derive an association from a validated roster in the same media/run/source epoch only. Exact full Riot-ID matches retain priority. A tagless actor can associate only through the roster's explicit validated `riot_id_game_name` with exactly one eligible participant. Permit ASCII case-insensitive equality for observed aliases such as `Zyra Bot` versus `Zyra bot`; use that same equality for collision detection and retain original strings. Do not add broader Unicode normalization, fuzzy matching or parsing a display name into identity. Duplicates, conflicting identity fields, missing data and non-player names stay unresolved. Do not infer identity from champion, team, position, statistics or cross-recording history.

Derive only for raw unresolved actors; preserve explicit conflicts and existing resolved references. Validate full identity and split-name consistency before admitting roster candidates. Use the distinct participant dictionary over the validated source epoch, not a guessed nearest-time roster. Recompute associations only after complete source validation and atomically publish derived actor links, effective coverage summaries and ready status, so a later duplicate cannot leave an earlier false unique match. Partial import rows remain non-queryable. Keep this within existing bounded import ownership; no per-event worker or query-time journal scan.

Represent derived association and its basis separately from raw producer evidence. In `league_index/import.rs` and `schema.sql`, retain raw actor columns/revision JSON and store derived identity separately. Update `coverage.rs`, history participant joins in `query.rs`, and replay projection in `replay.rs` consistently: serialized stored revisions must remain raw, while the response distinguishes effective app association from producer evidence. Apply this to event rendering/filtering, local timelines and unresolved-coverage semantics; do not fix only the overlay. Reuse existing schema and IPC types where truthful; add only the minimal explicit distinction needed. Bump `league_index.rs::VERSION` from 4 to 5 through its existing disposable-cache mechanism; no recording schema migration. No journal rewriting or shared producer resolver change is permitted.

Keep the relationship correction bounded within this feature because the primary real-data UI workflow needs it. If evidence shows it requires a broader identity protocol or substantial consumer redesign, record and split that work rather than building a parallel resolver system.

### Practice Tool behavior

Use its original clocks, source epochs and per-observation calibrations. The supplied Practice Tool journal has three events and two roster identities, accelerated forward clock samples with a maximum observed step around 32.7 game seconds, and no observed backward clock drop. Do not claim this recording proves backward-reset handling; use existing focused unit fixtures for that case. Preserve observations even when no valid media coordinate is available. Test out-of-range events, forward discontinuities and gracefully unavailable mappings. No clamping into the video, stretching time, synthesizing an offset, borrowing a later/current calibration, or adding corrected event timestamps. Valid video playback stays independent of unavailable telemetry. Distinguish positive mapped observations from unknown negative-query coverage.

## Rejected alternatives and planning decisions

- Fabricated matches postpone useful UI work and conceal the real feed relationship defect.
- Replacing the missing video with unrelated gameplay would imply alignment that cannot be verified; a labelled meaningless substitute is honest.
- Rewriting stored identities/timing or loosening producer validation destroys the real-world regression evidence.
- Seeding database rows and preview APIs bypass production import and cannot establish this environment.
- Benchmark mode and new reset/test frameworks add behavior or infrastructure the UI workflow does not need.

## Milestones

1. Prepare the unchanged real sidecars and Swiftplay surrogate, document provenance and useful real timestamps, and validate import through existing app/journal tests. Practice Tool data stays available as a secondary fixture; its video can follow after the primary workflow is usable.
2. Add the minimal generic app-data override and small launcher. Run the real desktop app immediately against Swiftplay; verify visible assets, video, timeline and overlay. Provide release and hot-reload commands. Keep observed actor-link limitations visible until milestone 3.
3. Correct the bounded app-side derived roster/event association with focused regression cases and cache rebuild. Validate the real Swiftplay participant/event queries and temporal overlay states. Validate Practice Tool mapping failures without altering source timing.
4. Complete actual save/restart, seek, clip export/reopen and SQLite rebuild flows. Fix production-path issues exposed by these flows within the approved scope; record larger discovered work separately. Run applicable completion checks and document remaining limits truthfully.

## Verification design

Use existing Rust test locations and test runner, adding a compact `preprod_fixture` group where useful: `cargo test --manifest-path app/src-tauri/Cargo.toml preprod_fixture -- --nocapture`. Exercise journal validation, normal import into empty SQLite, real event/participant queries, expected temporal roster facts, reimport parity and unchanged source evidence. Cover exact full identity, unique tagless name, duplicate name/different tags, conflicting/missing roster identity, non-player actor, and media/run/epoch isolation. Preserve unknown coverage when associations or clocks remain unavailable.

Focused launcher checks need no new testing dependency: first use, preserved save/clip, explicit reset, restored caller environment, invalid ownership/external path/reparse rejection, and running-session reset refusal on an owned throwaway root. Inspect effective output/config/cache paths, never personal recordings. Verify Settings cannot move effective storage outside the pinned library.

Run `npm run desktop:build --prefix app`, then `powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod_launch.ps1`; use `-Dev` for UI hot reload and `-Reset` only after closing. In the real desktop app browse Swiftplay; show referenced assets; inspect early/middle/late overlay state; search actual events/participants; play and seek both directions and via markers; save, restart and verify persistence; export a real bounded clip, reopen it with thumbnail and verify synthetic A/V playback. Use the existing Settings cache-rebuild action and compare queries. Validate Practice Tool unavailable/out-of-range behavior separately. Placeholder silence is expected; do not require audible game sound or claim gameplay synchronization.

Probe and fully decode generated media and exported clips with the locked runtime. Run applicable app Cargo tests/fmt/clippy, frontend tests/check/build, canonical validation and `git diff --check` per `docs/development/VERIFICATION.md`. No recorder checks are added because recorder code must remain unchanged. Required desktop checks that cannot run remain not-run/environment-blocked, never passed.

## Performance and reliability gates

No recorder hot-path changes or capture-performance claims. Keep import/query resource bounds. Personal data is never a destructive test target. Pristine sidecar hashes remain unchanged. Working video/journals remain unchanged by browse/export/restart/reindex; saving has only its expected metadata delta and exported artifacts are additional files. Reset restores pristine working data. Practice Tool invalid mappings remain unavailable rather than being coerced into range. Keep no known discrepancy hidden behind fixture labels or test weakening.
