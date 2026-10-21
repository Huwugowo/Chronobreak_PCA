# QB-DEV-002 execution checkpoint

Feature: `QB-DEV-002`
ExecPlan: `docs/exec-plans/qb-dev-002-production-path-preprod-library-v3.md`
Updated: 2026-10-20

## Current milestone

Complete: all four milestones and required feature checks passed on 2026-10-20.

## Active unit

None. Current `main` is 42a1467 and matches origin/main (checked 2026-10-20); current working changes are included in both release and dev. Unrelated preview edits are preserved.

## Completed

- Inspected `Huwugowo/Chronobreak_PCA` commit `7802292d5fa6b9c1a59f8bcc667bd55513adc756`; downloaded archive under ignored `build/preprod-investigation/Huwugowo-Chronobreak_PCA-7802292`. Verified all 15 manifest-listed payload hashes and sizes. No MP4 is supplied.
- User confirmed original video unavailable and requested a meaningless substitute. Main dataset: Master Yi Swiftplay `games/1792431584`; secondary: Akali Practice Tool `games/1792432630`.
- Independently reviewed and superseded the original fictional-data plan with v2; preserved the original plan unchanged. Updated canonical real-data scope. Resolved exact surrogate provenance/media authority and atomic complete-import actor/coverage publication.
- Prepared both strict synthetic-media bundles and documented original/substitute provenance. Added the fixed owned launcher, generic app-data override, SQLite v5 raw/effective actor association and real-data regression tests. No recorder implementation changes.
- Verified actual production release and ordinary Tauri dev with mocks disabled, League absent, real Data Dragon assets and normal media/SQLite consumers. The saved recording and exported clip survive release-to-dev restart. A temporary CSS edit hot-reloaded in the actual desktop; the original stylesheet was restored byte-for-byte and its live style removed.
- Fixed the concrete clip-duration IPC mismatch exposed by export: backend now serializes `state`, matching the established frontend contract. Regression passes; restarted desktop displays the exported clip as 0:13 instead of unavailable.

## In flight

None. Final fresh release built in 1m06s and is open through the ordinary isolated launcher (session 96941, process 23792). The temporary local WebView test port is confirmed closed. The tested dev session is closed.

## Remaining

No required feature work remains. Broader UI scenarios explicitly listed as untested below remain outside this bounded verification.

## Verification

- 2026-10-20: SHA-256/size checks passed for 15 archived payloads. This is not production journal/schema acceptance or playback evidence.
- Swiftplay inspection: ten participants, 108 events; initial GameStart lacks referenced calibration; 107 events map within video. MinionsSpawning maps to 52.430140 seconds, latest Multikill to 1002.359883 seconds. Formula is game microseconds times 48 plus the observation's referenced calibration offset. An exploratory scout arithmetic error was corrected before finalization; no timing-unit bug is evidenced.
- Practice Tool: three events/two roster identities; GameEnd maps to about 1291.859996 seconds, after 131.916667 seconds video coverage. Forward clock steps reach about 32.7 seconds; no backward drop observed. Do not claim this recording proves backward reset.
- 2026-10-20: targeted app-data override test passed. Existing index group passed (17 tests, 2 existing ignored measurements); frontend suite passed (28 files/238 tests) before the latest added association assertion. TypeScript and fresh normal Tauri release build passed, including production frontend embedding.
- Locked r6 ffprobe verified Swiftplay 60,398 frames and Practice 7,915 frames, 1920x1080 H.264 High at 60 FPS/timebase 1/15360. Every video packet PTS sorted into the exact 256-unit grid, starting at zero. Both full decodes passed. Probe/facts are in ignored build/preprod-probe-*.json and build/preprod-surrogate-facts-*.json.
- Initial full app tests found authoring-script noncanonical zero in derived container rational; corrected by reducing/normalizing measured rationals (no parser change). Existing child-ready cancellation test fails inside the process sandbox and passes outside; the failure was environmental, not encoding load. Full app suite outside sandbox passed: 126 passed, 3 existing ignored. Clippy all targets with -D warnings and formatting check passed. Latest frontend suite: 28 files/239 tests passed.
- Real preprod regression passed: strict normal descriptors, both committed journals imported from empty SQLite, Swiftplay 108 raw/107 in-media events, ten-player temporal roster, local-killer/history predicates, separate raw/effective actors, Practice after-media event, complete rebuild equivalence, unchanged journals. Added alias identity/collision and media/run/epoch regression also passed.
- First-use/reuse/reset and rejection of unowned/version-mismatched root, concurrent lock, destination/source junctions and changed retention passed in an ignored temporary harness, with outside marker preserved. Actual active-session reset was refused. All 12 real sidecars/original metadata hashes and sizes match the pinned manifest. Canonical validation (64 features/11 pairs) and git diff --check passed.
- Actual release desktop: both history entries, ten Swiftplay portraits, champion/item assets, 107 timeline events, seeking to 52.433333s and 554.516666s with temporal roster, playback advancement, participant timeline filter, exact Riot ID + ChampionKill history filter, Save, real 13-second H.264/AAC export, thumbnail and native clip playback pass. Exported output fully decodes (780 frames/13.000000s). Practice plays 131.916667s independently of its unavailable mappings. Rebuild restores the same 107 events; bounded first import/rebuild takes roughly 1–2 minutes, not a deadlock. Raw coverage gaps remain visible.
- Final app suite: 127 passed, 0 failed, 3 existing ignored (53.38s; outside the Windows process sandbox). Existing packaged protected-route ffprobe/external-reference regression also passed against locked r6. Final fmt and Clippy all targets with -D warnings pass. Frontend remained at 28 files/239 passing tests; final release invokes TypeScript and Vite again.
- Final ordinary `tools/preprod_launch.ps1` release build passed: TypeScript/Vite production build plus fresh Tauri optimized executable (1m06s), then actual release process 23792 launched from the current checkout. The launcher remains alive to own the isolated session. No temporary debug port is left open.
- Actual dev desktop: saved intent and exported clip persist after restart, clip duration is 13s, late marker seeks to 940s with ten-player sampled roster and backward seek returns to 52.433333s. CSS hot reload and live restoration pass. Native Data Dragon champion/item assets load; two absent images are item ID 0 empty slots, retaining existing fallback. No new data fields or asset infrastructure were added.
- Final preservation check: all 12 original sidecars/archived metadata still match manifest hashes/sizes; all 14 prepared working files match pristine fixtures except Swiftplay's expected `saved: true` metadata field. Video/journals remain unchanged after browse/save/export/restart/rebuild. Pristine fixture reset/safety is covered by the earlier owned throwaway-root checks.
- Limitations: synthetic static video/silent audio cannot establish gameplay alignment or original capture quality. Coverage gaps/unmapped observations and Practice after-media data remain truthful. Fullscreen transitions, manual clip-endpoint dragging and offline asset outage were not separately exercised in this bounded feature. Native WGC pointer/capture automation failed; actual desktop UI checks used a temporary local WebView test port, now closed.

## Deviations

Explicit user requirement change before implementation: real Swiftplay telemetry with synthetic video, Practice Tool adverse timing, simpler tooling, no recorder implementation changes. V2 supersedes the original synthetic-match design.

The clip-duration wire-tag defect was fixed within milestone 4 because actual export/reopen exposed it. No change to probe security, decoder strictness or error handling was needed.

## Decisions

- V3 makes only the user's scope clarifications to v2: valid filesystem inputs define the boundary, League is not required, release/dev use latest integrated main, established data responsibilities stay settled, and existing functionality is reported as working/incomplete/untested without added gates. Fixtures, media, launch/reset, association fix, safety and verification are retained. Earlier plans remain unchanged.

- Preserve original metadata/hash separately; prepared playback metadata must describe actual generated media. Keep telemetry byte-identical and original media UUID solely as a documented fixture association, never original-media evidence.
- Generate original Swiftplay video grid (60398 frames, 60 FPS, 1920x1080, timebase 1/15360) once with locked runtime; no generator framework.
- App index alone derives unambiguous actor/game-name associations, including observed ASCII bot-name casing differences. Preserve producer evidence, atomically publish effective links/coverage after complete import, rebuild v4 caches through v5.
- Keep fixed owned launch/reset safety and pinned storage; ordinary Tauri dev supports hot reload. No special preprod consumers, test framework or recorder changes.
- Preserve unrelated uncommitted QB-DEV-001 preview changes.

## Blockers

No implementation blocker. Original video cannot be obtained; synthetic media is user-authorized. Visual gameplay/event synchronization and original capture-quality validation remain unavailable by design.

## Next action

Use `tools/preprod_launch.ps1 -Dev` after closing the open release app for UI work; add `-Reset` only when a pristine working library is wanted. No implementation follow-up is required for QB-DEV-002. Preserve unrelated preview edits.
