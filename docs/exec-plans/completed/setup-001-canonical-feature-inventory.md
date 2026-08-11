# SETUP-001 — Build Canonical Chronobreak Feature Inventory

## Purpose

Replace the one-item setup state with a truthful canonical Chronobreak roadmap grounded in the repository. Future agents must be able to distinguish shipped foundations, executable work, broad epics, and unrefined ideas without relying on chat history or the old seed.

## Relevant current architecture

- `recorder/src/main.rs` owns recorder modes, logging, diagnostics, and the tray/headless entry points.
- `recorder/src/service.rs`, `watcher.rs`, `encoder.rs`, `poller.rs`, and `storage.rs` own League-process detection, ffmpeg capture, Live Client synchronization, and game-bundle persistence.
- `app/src-tauri/src/lib.rs` exposes the Tauri command boundary and starts the loopback playback server.
- `app/src-tauri/src/library.rs`, `playback_server.rs`, `ddragon.rs`, `clip_export.rs`, and `music.rs` own filesystem indexing, byte-range media delivery, static assets, and exports.
- `app/src/` is the SolidJS library/viewer/export UI.
- `feature-list.json` and `feature-list.schema.json` are the canonical work state and its structural contract.

## Scope / non-goals

This work audits and documents existing behavior, reconciles the product-intent seed, records verification entry points and evidence, and archives the seed. It does not implement or alter product functionality. Existing user changes in viewer, audio, playback, and clip files are preserved.

## Exploration findings

- The product is split into an always-available recorder process and a separate Tauri/SolidJS review process; neither invokes the other in the current development architecture.
- The recorder uses League process presence—not Live Client API state or `GameEnd`—as the video lifecycle authority.
- Game bundles and clip MP4/JPEG artifacts are ordinary filesystem files. The app rebuilds its library view by scanning them; there is no database.
- The app serves recordings, clips, music, HEVC probe media, and Data Dragon assets from an ephemeral loopback HTTP origin so the webview can stream and seek without loading whole files.
- Existing phase completion prose is useful manual evidence but does not replace rerunning available automated checks.
- The seed has 34 stable roadmap IDs: seven epics and 27 concrete features. It omits the self-contained Windows beta/distribution work recorded in `PHASES.md`.

## Chosen design

Preserve seed IDs and requirements, keep all epics at `stage: draft`, and promote only concrete features whose definitions pass the readiness gate. Mark existing behavior `done` only when implementation, runnable checks, and explicit phase/manual evidence together support the acceptance criteria. Add distribution work that is not represented by a seed item. Keep verification results in this plan and feature evidence; keep command definitions in `docs/development/VERIFICATION.md`.

## Milestones

1. Audit recorder, app, specifications, test surfaces, and working-tree ownership.
2. Reconcile the seed into `feature-list.json`, including dependencies, workflow, readiness, and evidence.
3. Document architecture boundaries and authoritative verification commands.
4. Archive the seed in a clearly non-canonical location and remove competing roadmap wording.
5. Run project-wide verification, record truthful outcomes, validate the inventory schema, and remove `SETUP-001` only after the completion gate passes.

## Verification

- `cargo test --manifest-path recorder/Cargo.toml`
- `cargo fmt --manifest-path recorder/Cargo.toml -- --check`
- `cargo test --manifest-path app/src-tauri/Cargo.toml`
- `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check`
- `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`
- `npm run check --prefix app`
- `npm run build --prefix app`
- `npm run desktop:build --prefix app` when the Windows environment permits it
- Python `jsonschema` validation of `feature-list.json` against `feature-list.schema.json`
- Recorder diagnostics and the documented League/media/UI procedures when their external prerequisites are available

## Performance / reliability

No capture or media code is changed. The audit records existing diagnostics and manual reliability/performance procedures without claiming they ran in an environment lacking League, capture hardware, ffmpeg, or interactive Windows UI. Destructive storage validation is restricted to temporary fixture libraries.

## Progress

- [x] Audit repository and preserve unrelated user changes.
- [x] Reconcile canonical inventory.
- [x] Document architecture and verification.
- [x] Archive seed and remove competing roadmap wording.
- [x] Run verification and record outcomes.
- [x] Complete SETUP-001 and leave a restart pointer.

## Deviations / surprises

- Sandboxed Vite and Tauri builds reproduced Windows `spawn EPERM`. Both exact commands passed when rerun in the approved environment, so the final result is passing with the sandbox limitation recorded rather than environment-blocked.
- Sandboxed recorder diagnostics could not create the normal AppData rolling log. The approved rerun passed and selected NVENC/HEVC/high with silent stereo fallback.
- The historical phase exporter always re-encodes to H.264/AAC, so it does not satisfy the seed's broader conditional no-reencode promise. The shipped exporter is recorded separately as `QB-CLIP-007`; `QB-CLIP-004` remains draft.
- Existing age retention does not satisfy the seed's maximum-storage-budget and capacity-estimation promise. The shipped baseline is `QB-LIB-009`; `QB-LIB-004` remains ready.

## Decision log

- 2026-08-11: Treat phase-complete statements as explicit historical/manual evidence candidates, but rerun every locally available automated check before relying on them.
- 2026-08-11: Preserve the dirty viewer/clip/audio changes as user-owned and audit them read-only.
- 2026-08-11: Preserve all 34 seed IDs and add 13 concrete discoveries for shipped foundations, a decomposed import/relink objective, and the decomposed Windows beta; do not force broader seed promises to `done` merely because a narrower phase shipped.
- 2026-08-11: Reclassify `PHASES.md` as a historical delivery record so it does not compete with `feature-list.json`.

## Completion

Completed 2026-08-11.

The canonical inventory contains 47 unique items: nine draft epics, 38 concrete features, 32 ready features, and 11 narrowly evidenced done features. All 34 seed IDs remain present. `SETUP-001` is absent from canonical state. The archived seed is byte-identical to the original Git blob (`a5485f7365674664349b80b55a98c637fe3f191e`).

Verification results against the current working tree:

- `cargo test --manifest-path recorder/Cargo.toml` — passed (31 library tests, one binary test, doc tests).
- `cargo fmt --manifest-path recorder/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path recorder/Cargo.toml --all-targets -- -D warnings` — passed.
- `cargo build --release --manifest-path recorder/Cargo.toml` — passed.
- `cargo test --manifest-path app/src-tauri/Cargo.toml` — passed (29 tests plus doc/binary targets).
- `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` — passed.
- `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings` — passed.
- `npm run check --prefix app` — passed.
- `npm run build --prefix app` — sandbox attempt hit `spawn EPERM`; approved rerun passed.
- `npm run desktop:build --prefix app` — sandbox attempt hit `spawn EPERM`; approved rerun passed and produced `app/src-tauri/target/release/league-replay-app.exe`.
- Python Draft 2020-12 schema plus ID/parent/dependency/epic-stage/cycle invariants — passed for 47 items.
- `cargo run --manifest-path recorder/Cargo.toml -- --diagnose` — approved rerun passed with ffmpeg 6.1, NVENC, HEVC, high profile, and silent stereo fallback.
- `cargo run --manifest-path recorder/Cargo.toml -- --tray-smoke-test` — passed on the interactive Windows desktop.
- `ffprobe` structure inspection and full ffmpeg decode of `app/src-tauri/resources/hevc-probe.mp4` — passed (HEVC, 24 fps, 3 seconds).
- `git diff --check` — passed.

Not run: a real League match lifecycle/synchronization/performance procedure, H.264/HEVC source export playback matrix, forced interruption against a fixture recording, clean-machine installer workflow, and a baseline-versus-capture FPS/frametime benchmark. These require League, representative user-approved/test media, or not-yet-built distribution/benchmark infrastructure. No completion claim relies on them beyond the explicit historical manual evidence already recorded in `PHASES.md`.

No product code was changed by SETUP-001. User-owned viewer/clip/audio changes present at audit start were preserved.
