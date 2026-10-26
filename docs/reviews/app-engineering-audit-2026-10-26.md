# Application engineering audit reconciliation — 2026-10-26

Reviewed input: `C:/Users/Hugo/Downloads/chronobreak_app_engineering_audit_updated_2026-10-26.md`.
Repository baseline: `39c948e4792d1d5f6000873eedba02124949318a`, exactly the report's snapshot; the working tree was clean at entry.

This is a dated review and evidence snapshot, not an execution tracker. `feature-list.json` owns lifecycle and follow-up status. The attached report's recommendations were evaluated as evidence, not treated as repository instructions.

## Assessment

The report's main direction is justified: preserve the current filesystem/media identity, bounded worker, exact timing and disposable-index architecture, and repair demonstrated mismatches first. Source inspection confirms the audio-less export and HEVC probe defects. It also confirms the export crash-recovery gap, synchronous startup retention and narrow video-byte accounting. Source inspection alone does not establish query starvation, startup cost, an exploit, or representative performance.

Two qualifications materially change the review:

- **A-07 overstates the offline cache finding.** `docs/architecture/media-library.md` explicitly permits an older core cache to serve other assets when the summoner manifest is unavailable. `ddragon::newest_cached_status` and `initialize` preserve that offline behavior. The fresh network path requires a summoner manifest; numeric/display-name spell resolution requires it; legacy internal spell-name fallback deliberately works without it. A core-cache `ready` status is therefore not proof of complete spell coverage, but rejecting the entire cache would contradict the current graceful fallback contract. Treat this as a possible capability/status clarity improvement, not a confirmed general offline-cache regression.
- **A-12 needs the portable/installer distinction.** Static CI does not exercise a packaged desktop application. However, completed `QB-DIST-001` already records exact portable-runtime identity/layout, sanitized-PATH diagnostics and generated-media encode/probe/thumbnail/decode evidence. The clean-machine installer, upgrade and uninstall gap belongs to the unstarted `QB-DIST-004`; existing portable evidence is not missing or invalidated.

## Finding-by-finding comparison

References below name current source symbols; the baseline commit fixes the audited line versions.

| Finding | Repository verdict and evidence | Canonical disposition |
| --- | --- | --- |
| A-01: video-only export | Confirmed. `replay-time::AudioTimelineV2` accepts absent audio, while `clip_export::ffmpeg_arguments` unconditionally selected `[0:a]` and output validation required AAC. | Selected bounded repair: `QB-CLIP-012`, preserving H.264/AAC and existing mix semantics. |
| A-02: HEVC false positive | Confirmed. `app/src/api.ts::waitForPresentedFrame` accepts a positive media clock at timeout without a presented frame. `record_hevc_probe_result` persists that result. No focused clock-only timeout regression existed. | `QB-REPLAY-025`; keep the closed units of `QB-REPLAY-009` intact. Require positive presentation evidence and test clock-only, error, timeout and cleanup paths. |
| A-03: export crash recovery | Confirmed gap. `clip_export::publish_export_paths` renames members sequentially. `Drop` and ordinary rollback cannot run after process death. The library counts `.part.mp4` bytes but does not display them as valid clips. | `QB-CLIP-013`, planned recovery design. Recorder recovery `QB-CAP-003` does not cover exports. |
| A-04: import supersedes foreground search | Code-supported risk. `historyController::pumpImport` requests refresh queries; `requestQuery` increments generation; an outstanding result is rejected after replacement. Existing tests do not prove eventual foreground admission under continuing imports. | `QB-LIB-019`; first construct the deterministic timing test. Existing `QB-LIB-014`/`016` do not establish this invariant. No starvation claim or scheduling change in this review. |
| A-05: startup retention | Confirmed synchronous path in `lib.rs` setup through `library::run_auto_delete`; cost remains unmeasured. Explicit cleanup already has an owned coordinator mutation lane. | Existing `QB-REPLAY-010` opening measurements and `QB-LIB-009` retention safety. Measure before deferring cleanup; do not add an unmanaged worker. |
| A-06: non-advancing pending | More precise source support than the report supplied. `league_index/import.rs::Index::import` can remain pending with zero new batches when a journal has a stable uncommitted suffix. `league-data` recovery also uses pending for productive bounded pagination. Frontend retries both at a short cadence. Work remains serialized and context invalidation stops retries. | `QB-LIB-020`; characterize unchanged-tail calls, productive calls and source changes before choosing retry semantics. This is not an unbounded-concurrency defect. |
| A-07: offline spell manifest | Intentional core-cache fallback, with ambiguous readiness/completeness if interpreted as full spell coverage. See `ddragon::{initialize,newest_cached_status,summarize_cache,spell_icon_filename}` and the durable offline contract above. | Preserve `QB-REPLAY-007`/`QB-LIB-012` offline behavior. No blanket manifest-required fallback change. Missing/corrupt spell coverage can be tested if capability reporting is later refined. |
| A-08: benchmark/screen coupling | Confirmed maintainability concern. `App.tsx` dispatches benchmark scenarios and completion policy; `ViewerScreen.tsx` includes the scenario-action interpreter and telemetry. This does not prove a runtime defect. | `QB-DEV-004`, a separate maintenance draft requiring behavior-equivalence evidence. Keep observation on real production paths. |
| A-09: typed IPC errors | Confirmed design limitation. The backend's typed `LibraryRefreshError` becomes a string through `command_error`; frontend `libraryError` normalizes strings. No specific incorrect user recovery was demonstrated. | `QB-DEV-005`, incremental typed-error draft; no global error rewrite. |
| A-10: storage semantics | Confirmed label/contract gap. `library::{build_library_snapshot_with,storage_usage}` counts game-video and clip-MP4 bytes, including undisplayed MP4s, while `StorageIndicator.tsx` labels the aggregate “Total”. Thumbnails/metadata/caches are outside that total. | `QB-LIB-021`, direct wording clarification. Preserve cheap snapshot accounting and existing `QB-LIB-009` semantics. |
| A-11: imported-path authority | Authority shape confirmed; no vulnerability demonstrated. UI picker filters MP3/WAV, but `music::resolve_imported` accepts any supplied existing supported absolute path. A picker-issued capability requirement is not established by the trusted local frontend contract. | Threat-model-dependent decision; retain existing validated path handling. No speculative security framework or bug-severity escalation. |
| A-12: release verification | Static CI gap confirmed; exact portable media-runtime verification already exists. `tools/media_runtime/smoke.ps1` exercises the tools directly, not a clean installed Tauri app. | Existing `QB-DIST-004`; preserve completed `QB-DIST-001` and static-only `QB-DEV-003` evidence. |
| A-13: stream-copy fast path | Opportunity, not defect. Export currently reports `full_reencode`; transforms, mix, byte caps and exact boundaries constrain eligibility. | Existing draft `QB-CLIP-004`. Require an eligible decision matrix and measured benefit. |
| A-14: authored backup/recovery | Foundation already exists: `league_authored` has versioned bounded records, revision checks, locking and synced atomic publication outside SQLite, plus cache-rebuild preservation tests. User-facing backup/export/recovery semantics and future draft/montage lifecycles remain undefined. | `QB-LIB-022`; build on the existing store, not a parallel persistence mechanism. |

## Selected correction and evidence

`QB-CLIP-012` preserves the existing output contract. Source audio presence comes from validated metadata; only an absent source gets finite stereo 48 kHz silence. Selected music still uses the existing two-input mix, so its gain behavior is preserved. The output AAC requirement, frame validation, 50 ms audio alignment bound, preset filters, encoder choices, size correction and publication protocol are retained.

The generated silence is explicitly duration bounded, then trimmed and timestamped through the existing audio chain. This follows the source options documented by [FFmpeg's anullsrc reference](https://ffmpeg.org/ffmpeg-filters.html#anullsrc); real packaged-runtime tests are needed in addition to the filter documentation.

Actual results on 2026-10-26:

| Check | Result |
| --- | --- |
| Pre-fix `cargo test --manifest-path app/src-tauri/Cargo.toml silent_source_arguments -- --nocapture` | Failed as expected on the unconditional `[0:a]` reference. |
| Pre-fix packaged `silent_source_packaged_export_matrix -- --ignored --nocapture` | Valid generated video-only source failed with FFmpeg filtergraph input binding, reproducing the functional defect. An earlier fixture attempt failed strict metadata parsing (`capture_backend` missing); the fixture was corrected without changing production validation. |
| `QUEUEBACK_TEST_MEDIA_RUNTIME=build/media-runtime/windows-x86_64`; `cargo test --manifest-path app/src-tauri/Cargo.toml silent_source -- --include-ignored --nocapture` | Both focused tests passed. The packaged matrix completed in 42.78 seconds including 18 exports: source audio absent/present × no/bundled/imported music × Discord/horizontal/vertical. Each output passed production H.264/AAC validation, exactly 150 frames / five seconds, audio alignment, full single-thread decoding and thumbnail checks; Discord remained below 10,000,000 bytes. Silent no-music outputs decoded to zero; all selected audible sources/music decoded to nonzero samples. Source SHA-256 checks passed after every batch. |
| Packaged invalid-source and cancellation scenarios | Corrupt dedicated source failed explicitly and retained its test bytes; cancelling an owned waiter during real encoding killed/reaped the child and completed cleanup. Both left the preexisting clip-entry count unchanged and no added staged or published files. Restored valid source hash remained unchanged. |
| `cargo test --manifest-path app/src-tauri/Cargo.toml` | 137 passed, zero failed, five opt-in tests ignored; the new packaged matrix was explicitly run separately above. Main target: zero tests, passed. Full log: `.codex/logs/command-20261026-111820.log`. |
| `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` | Passed. |
| `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings` | Passed. Full log: `.codex/logs/command-20261026-111929.log`. |
| Canonical validation block from `docs/development/VERIFICATION.md`; `git diff --check` | Passed: 95 feature/epic items and 15 immutable-plan/checkpoint pairs. |
| Fresh focused patch review | No actionable findings. The review checked validated audio presence, unchanged present-audio path, finite silence, AAC/frame/mix invariants and test coverage. |

The fixture uses generated 320×180 30 fps sources and the real software encoder
through the staged locked runtime. These tests establish correctness, not export
performance. Audio checks establish silence/audibility rather than quantitative
per-input gain ratios; the existing volume expressions and mix normalization are
unchanged. No frontend/IPC schema or capture implementation changed, so unrelated
frontend/recorder suites were not rerun for this bounded backend correction.

## Limits

This reconciliation inspects the named application paths; it is not a full-file security review or independent audit of upstream projects. Concurrency/startup hypotheses were not performance-certified. No real recording was used as destructive test data. No installer, driver matrix, real League or cross-device playback certification is claimed. Remaining recommendations are not implicitly implemented by completing the selected correction.
