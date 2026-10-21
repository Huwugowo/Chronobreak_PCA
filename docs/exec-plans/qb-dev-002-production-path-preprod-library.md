# QB-DEV-002: Production-path preprod recording library

## Purpose

Provide a normal current desktop application populated with fictional recorder output for replay-overlay and other UI development. Browsing, SQLite import, searching, playback, mutations and export must exercise production paths. A developer launches a disposable working library without exposing their everyday recordings or configuration.

## Relevant planning-time architecture

Recording authority is the bundle described in `docs/architecture/media-library.md`: `games/<timestamp>/video.mp4`, strict schema-v2 `metadata.json`, schema-v2 `game_log.json` manifest, committed `league_observations.jsonl` and roster hint `league_checkpoint.json`. Optional `league_match.json` and `league_result.json` carry validated provisional context and final results. SQLite under app-data `league-index` is a disposable projection, not the fixture authority. Authored records remain in the writable library.

`app/src-tauri/src/library.rs` owns metadata validation and descriptors; `league_index` owns reconciliation and history/replay queries; `clip_export.rs` owns actual export. Exact timeline/media identity follows `docs/architecture/replay-time.md`. The locked FFmpeg/ffprobe pair and `QUEUEBACK_MEDIA_RUNTIME_DIR` follow `docs/architecture/media-runtime.md`.

Normal startup in `app/src-tauri/src/lib.rs` reads configuration through `config::default_config_path`, already honoring `LEAGUE_REPLAY_CONFIG`, and derives mutable cache paths from Tauri app-local data. It has no normal app-data override. DDragon, music, HEVC probe and SQLite use that app-data root. Startup runs retention, so the dedicated configuration must set `auto_delete_days = 0`.

## Scope and non-goals

Add one representative static fictional recording, its required asset-cache seed, a small launch/reset script, generic app-data path selection and focused verification. Preserve the existing browser preview. Do not add a fake native API, preseed SQLite, introduce a general data generator, activate benchmark mode, start the recorder, or claim real League capture/performance validation. Installer distribution is separate work.

## Exploration findings

Production journal writing is available through `league-data/src/observation/journal.rs::Writer`, checkpoint `State::new/apply` and `Manifest::from_checkpoint(...).encode`; `recorder/src/poller/persistence.rs` shows persistence ordering. Test helpers in `library.rs` illustrate valid bundles but contain one player and placeholder video, so cannot be shipped unchanged.

`app/public/mock-replay.mp4` is an existing roughly 8 MB synthetic media candidate. Its actual streams, frame count, PTS and duration must be probed with the staged runtime before matching metadata is authored. Presentation mock descriptors are not media evidence.

Normal DDragon initialization attempts refresh and falls back to the newest valid cached manifests on failure. Native icons are stored below `<app-data>/ddragon/<version>/icons/<kind>/<normalized-key>.png`. Seeding a complete cache for the sample uses this normal fallback; benchmark-only offline initialization is unnecessary. Normal online refresh remains enabled.

Benchmark launch is unsuitable because it disables mutations and changes network/telemetry behavior. Its owned-directory safety patterns are useful provenance only.

## Chosen design and rationale

### Static data

Check in `fixtures/preprod-library/` with a README, content-hash manifest, a pristine `library/games/<timestamp>/` bundle and `app-data/ddragon/<version>/` seed. Use fictional player identities and ten participants, multiple committed roster snapshots with changing scores/inventories, diverse timed events, and consistent match/result sidecars. Document exact early/middle/late timestamps and expected overlay/search facts. Event clocks must map to the actual video duration; never label a short clip as a full thirty-minute recording.

Reuse the existing synthetic video if suitable. If audio is absent, add a one-time synthetic AAC track with the locked runtime and verify exact resulting timing. Record media provenance truthfully; do not claim a native capture backend produced this fixture. Author static telemetry using existing production serializers in a temporary or narrowly test-only authoring helper. Ship the resulting files, not a reusable generator service or parallel serialization implementation.

Seed native DDragon manifests and every champion/item/spell/rune image referenced by the sample. Preserve asset provenance and notices. Do not depend on the developer's personal cache or frontend preview routes. Verify both normal online initialization and cached fallback; network delay or missing unrelated assets must not block playback.

### Normal application isolation

Add generic `LEAGUE_REPLAY_APP_DATA_DIR` handling in normal startup. An explicitly configured path must be absolute, creatable and usable; failure must fail startup rather than silently use personal app data. The default remains Tauri app-local data when absent. Keep benchmark configuration ownership intact. No query, replay or export branch recognizes preprod.

`tools/preprod_launch.ps1` owns a fixed ignored `build/preprod` tree containing `library`, `config.toml`, `app-data` and `webview2`. On first use it copies pristine files, creates an ownership sentinel and writes a dedicated valid configuration with absolute output path and retention disabled. Ordinary launches preserve clips, save intent, authored records and cache. Set `LEAGUE_REPLAY_CONFIG`, `LEAGUE_REPLAY_APP_DATA_DIR`, `WEBVIEW2_USER_DATA_FOLDER` and the validated staged `QUEUEBACK_MEDIA_RUNTIME_DIR`; clear the UI-mock switch and benchmark activation variables for the child. Restore caller environment in `finally`. Use hidden background startup where applicable and do not launch a recorder process.

Before launching, validate the owned root, config output and cache locations. Reject a configuration changed to an external library. Refuse concurrent ownership/reset while the launched app is running. Reject reparse-point traversal. `-Reset` may remove only resolved descendants of the fixed sentinel-owned root; verify bounds before any recursive removal. Reset restores pristine sample data and cache seed, never changes user directories, and is never implicit.

Override inherited `LEAGUE_REPLAY_OUTPUT_PATH` in the child with the exact owned library path. The existing generic override pins effective storage even if Settings changes the configured path during the session. Validate both configuration and effective output before starting, and document this preprod restriction in the launcher README. Set `app.autostart = false` in the dedicated configuration; the current desktop app does not register startup or launch the recorder, so no additional startup mechanism is needed. Restore this variable along with all other modified environment values on exit.

The ownership sentinel must include a format version and canonical absolute root. Reject an existing root with missing/mismatched ownership before copying or resetting. Validate fixture source paths and destination chains against reparse traversal. Only a missing root may be initialized without a preexisting sentinel.

Launch the normal release executable built by `npm run desktop:build --prefix app`, without `replay-benchmark`. Document a fresh build before launch and record source revision, dirty-tree state, executable hash and runtime identity in the local run receipt. Do not present an existing binary as a fresh build. Document the initial sequence and repeated launch/reset commands in `fixtures/preprod-library/README.md`.

## Rejected alternatives and planning decisions

- Frontend mocks bypass native validation, SQL and export and cannot prove this environment.
- Prepopulated SQLite would bypass the recorder-output contract and break cache-rebuild evidence.
- Benchmark mode changes production behavior and prevents the mutations needed for UI development.
- A new data generator framework is disproportionate; one static representative fixture and normal serializers suffice.
- Reusing personal library/config/cache risks data and makes results machine-dependent.

## Milestones

1. Author and validate static sample files and native asset cache. Add focused fixture tests beside existing app library/index tests using production parsers and serializers. Establish actual media timing, fixture hashes and expected temporal/query facts before launch work.
2. Add generic app-data override with absent/valid/invalid-path tests. Add the owned launcher and reset safety checks under `tools/preprod/` or a dedicated test file. Test first copy, preserved mutation, explicit reset, external-config rejection, reparse rejection and concurrent-process refusal on temporary owned directories. Add no new package dependency merely for script tests.
3. Exercise normal import/query/descriptor/export with a temporary copy and empty app-data. Assert ten-player temporal state, event and participant history results, source identity, durable save intent and cache-rebuild parity. Export a real clip, inspect and fully decode it with the locked runtime. Keep test outputs separate from pristine fixtures.
4. Build and launch the actual normal desktop application, complete the smoke procedure below and document usable commands, expected data, limitations and provenance. Update applicable architecture documentation and canonical feature evidence only after the required gates pass.

## Verification design

Acceptance criteria 1 and 3: a `preprod_fixture` Rust test group invokes production library parsing, journal recovery/index reconciliation, history and replay query paths with no seeded database. Assert expected facts at named times, identity consistency and unchanged source hashes. Run `cargo test --manifest-path app/src-tauri/Cargo.toml preprod_fixture -- --nocapture`. Metadata must agree with actual locked-runtime probe output and full decode, including audio/video streams and exact frame timeline.

Acceptance criteria 2 and 5: a focused `tools/preprod/test_launch.ps1` exercises owned temporary roots and verifies caller environment restoration, first-run copying, mutation preservation and bounded explicit reset. Include malformed/missing sentinel, external output path, reparse-point and running-instance cases. Assert an in-session Settings output change cannot redirect effective storage. Run `powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod/test_launch.ps1`. Test fixture hashes before/after and ensure no operations resolve to personal paths.

Validate DDragon fallback deterministically with normal `initialize` and a test-only failing HTTP transport or denied network environment, only the copied seed present, without activating benchmark mode. Assert fallback version/status and every sample icon/name route. Then exercise ordinary online initialization separately, so online refresh cannot conceal an incomplete seed. Do not introduce a production offline mode just for this test.

Acceptance criteria 4 and 6: after `npm run desktop:build --prefix app`, run `powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod_launch.ps1`. In the real desktop window browse Games; filter a known event and participant; open replay; play with audio; seek forward/backward to documented roster/item changes and event markers; inspect overlay and fullscreen; persist save intent; export a bounded clip and reopen it from Clips with thumbnail and A/V playback. Close and relaunch to check persistence. With the app closed remove only the owned disposable SQLite cache, relaunch and confirm identical searches/replay facts. Explicit reset must restore pristine working data. Record build/runtime identity, actions, observed results, export probe/decode output and before/after source hashes in the execution checkpoint. Native UI checks remain required if automation is unavailable; browser preview is not a substitute.

Run applicable routine groups from `docs/development/VERIFICATION.md`: app Cargo tests/fmt/clippy, frontend tests/check/build and normal desktop build. If shared league-data or recorder code changes, run its applicable tests and the complete recorder check/test/fmt/clippy group. Run canonical feature-list validation and `git diff --check`. A blocked or unattempted desktop check keeps the feature non-done.

## Performance and reliability gates

No recorder hot-path change or capture benchmark is required. Preserve bounded normal library scanning/import/query behavior and production media validation. All export, reset, corruption and cache-rebuild tests operate on copied fixtures. The checked-in pristine fixture hashes must always remain unchanged. In the working library, video, journal and telemetry sidecars are immutable; saving intentionally changes metadata, so assert that exact saved-field delta and its persistence across restart/cache rebuild rather than demanding an unchanged metadata hash. Exclude mutable caches and added clip/authored outputs from immutable-source assertions. Reset must restore byte-identical fixture files. Launch must fail safely on invalid isolation/runtime setup. Do not weaken contracts to accept fixture data; correct the fixture or explicitly supersede the design if production contracts cannot represent it.
