# QB-PERF-002 execution checkpoint

Feature: `QB-PERF-002`
ExecPlan: `docs/exec-plans/qb-perf-002-low-overhead-native-windows-capture-v3.md`
Updated: 2026-09-29

## Current milestone

Superseding-plan Milestone 5: valid target League capped/uncapped performance proof, support-matrix disposition, and final completion verification.

## Active unit

No product-code unit is active. The fragmented-MP4 finalizer correction passed synthetic checks, temporary-copy validation/publication of both prior partials, the locked release rebuild, and the subsequent real Practice Tool run `1790670285`. That run finalized normally, passed full decode, and the user confirmed playback in the app. Previous timestamp/size/teardown fixes remain in place. The formal performance matrix remains outstanding; the user has deferred audio work and selected data-feature refinement after committing these fixes.

Do not substitute current-workstation generated-window evidence, the historical native/external A/B, the accepted-invalid QB-PERF-001 report, or a different GPU/CPU target for that matrix.

## Completed

- Integrated the in-process exact-HWND WGC/D3D11/NV12/direct-NVENC H.264 High recorder and one FFmpeg audio/mux child.
- Retired the FFmpeg-driven WGC recorder, backend selector/candidate retry, external runner, and unvalidated HEVC/profile/AMF/QSV product claims. There is no automatic or explicit cross-backend fallback.
- Preserved the paired packaged-runtime/build/staging, recorder lifecycle, collision-suffixed bundle discovery, Live Client polling, finalization, and application contracts.
- Passed the recorded packaged-runtime/release checks, historical matched generated-window A/B, native source probe, native lifecycle/failure matrix, and accepted 1,800-second native arm described below.
- Preserved the original immutable dual-backend plan as history and finalized the reviewed self-contained native-only superseding plan `docs/exec-plans/qb-perf-002-low-overhead-native-windows-capture-v3.md`.
- After a 2026-09-28 live League capture stopped on a non-advancing WGC source timestamp, made the pending slot retain the greatest timestamp and discard equal/older admitted frames with exact worker-discard accounting. The stale frame no longer replaces the last good GPU snapshot or terminates the graph. Built the corrected release recorder and started it for the next match.
- Reproduced the subsequent practice-tool error with a generated 1x1 window transition, then prevented unusable NV12 crop sizes from reaching pool recreation. Resize preparation now shares the close/count staging path, and the watchdog follows copied source QPC rather than callback admission. Added bounded first-size logging, native unusable-size telemetry, and a hardware regression that proves actual 1x1 frames were discarded and video resumed after restore.
- Corrected the League end-of-game lifecycle: an exact selected HWND closure is now treated as the terminal capture boundary and routed through graceful native worker stop, mux flush, metadata publication, and `Idle` projection; replaced/foreign HWND identity failures remain explicit partial failures. Added a service classification test and documented the lifecycle contract.
- Corrected fragmented-MP4 finalized-media validation: a private fixed-memory header reader supplies the missing actual sample count, skips media payload, enforces the shared five-second deadline, and rejects malformed/truncated fragments. Shared replay-time checks and producer reconciliation remain unchanged. Finalization errors now log their full cause chain.

## In flight

None. Fragmented-MP4 correction, focused checks, release build, and real recording/app playback confirmation are complete. Prior partial recordings were not rewritten. Later QB-REPLAY-012 work may share recorder files but does not satisfy this feature's missing formal League matrix.

## Remaining

- Run one valid capped baseline/capture pair and one valid uncapped baseline/capture pair on the required Ryzen 5 5600X/RTX 4060 League setup.
- Pass every unchanged capped frame/resource gate and disposition every substantial capped or uncapped finding.
- Keep unimplemented native AMD/AMF and Intel/QSV adapters explicitly unsupported; draft `QB-PERF-003` and `QB-PERF-004` own any future implementation plus physical validation.
- Complete the native support matrix, architecture/operator inspection, packaged-runtime/project-wide verification, and canonical evidence update.

## Verification

Restart assumptions from the 2026-08-24 checkpoints:

- Recorder all-target/all-feature tests passed: 125 library tests and 3 binary tests; 5 hardware/profile fixtures explicitly ignored. Recorder formatting, all examples, and strict all-target/all-feature Clippy passed.
- Tauri tests passed 34/34 with strict Clippy; frontend type checking and production build passed.
- Media-runtime tests passed 10/10 with strict Clippy; capture-benchmark tests passed 60/60.
- The audited r6 packaged-runtime contract, corrupt-input/atomic preparation, sanitized-PATH encode/probe/export/full-decode smoke, optimized application/recorder builds, portable release verification, recorder diagnostics, and isolated app startup smoke passed. Evidence remains under ignored `build/media-runtime` and `build/perf` roots.
- Native source probe `build/perf/queueback-native-source/20260824-110114` delivered 588 frames with zero reported source/consumer drops, pool recreations, or conversion failures and returned all four bounded NV12 slots.
- Matched 240-second native/external generated-window A/B evidence at `evidence/preliminary-backend-ab/20260824-113718` produced decodable H.264/AAC media. Native used 0.144873% machine CPU and 157.980 MiB maximum combined private memory; external used 0.396960% and 235.867 MiB. This is directional non-League evidence only.
- Native steady, resize, minimize/restore, and occlusion fixtures produced exact 600-frame/10-second H.264/AAC files; target-close and injected-NVENC-failure fixtures preserved exact decodable partials. Evidence roots are dated `20260824-120516` through `20260824-120744` under ignored `build/perf/qb-perf-002-native-fixture`.
- Accepted 1,800-second native arm `20260824-120801-steady-none-1d73e07c` produced 108,000 reconciled frames and exact 1,800-second streams with bounded counters. The original wrapper lost its in-memory formal resource series and timed out a full count/decode probe; only live resource checkpoints and strict 60-second decode windows are claimed.
- Current-tree diff hygiene passed at the recorded checkpoint.
- 2026-09-28 timestamp correction: `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features` passed (118 library and 3 binary tests; 5 ignored hardware/profile fixtures); `cargo fmt --manifest-path recorder/Cargo.toml -- --check` and strict all-target/all-feature Clippy passed. The new timestamp-order test covers an equal and an older pending frame, retention of the newest QPC, and admitted-source reconciliation.
- The dedicated 10-second steady fixture passed on the interactive desktop at `build/perf/qb-perf-002-native-fixture/20260928-130046-steady-none-df904b7b`: 600 changing 1080p60 H.264/AAC frames, full decode, and source ownership `567 admitted = 538 copies + 29 replacements + 0 worker discards`. `cargo build --release --locked --manifest-path recorder/Cargo.toml` passed; the corrected release recorder initialized with the packaged r6 runtime at 2026-09-28 11:02:36 UTC. This does not exercise a stale timestamp in a live game or satisfy the formal League performance matrix.

No valid League matrix, negligible-impact claim, AMD/Intel implementation or hardware validation, or feature-completion claim is recorded.

2026-09-28 second-failure correction:

- Practice-tool capture at 11:29:06-11:30:20 UTC failed on `native WGC input is too small for NV12`; the owned frame also caused a one-frame accounting gap (`4381 != 3961 + 99 + 320`). The saved 75.776-second H.264/AAC partial passed full decode without modification. Recorder Live Client calibration, 73 event requests, and 8 snapshots succeeded. Alt-Tab/minimize was possible but not confirmed by the user; the standalone API remained available for roughly 12 seconds after the capture failure.
- Before the size fix, `run_native_fixture.ps1 -Scenario tiny_resize -DurationSeconds 10` reproduced the same conversion error and one-frame accounting gap at `build/perf/qb-perf-002-native-fixture/20260928-133428-tiny_resize-none-deb20ad2`.
- Corrected tiny-size/restore passed at `20260928-144215-tiny_resize-none-ede39ece`; normal resize and minimize/restore passed at `20260928-144321-resize-none-e2b8552b` and `20260928-144356-minimize_restore-none-31922de4`, respectively, under the same fixture root. Each produced exactly 600 decoded 1080p60 H.264/AAC frames and 60 distinct final-second frames.
- The final stronger tiny-size fixture at `20260928-144635-tiny_resize-none-84f1946c` proved actual first rejected content size 1x1, 153 unusable-size discards, no pool/processor recreation, 600 encoded/decoded frames, full decode, and 60 distinct final-second frames. Owned-flow accounting reconciled as `537 admitted = 348 copies + 36 replacements + 153 worker discards`.
- Final recorder all-target/all-feature tests passed (120 library, 3 binary, 3 fixture tests; 5 hardware/profile tests explicitly ignored). Recorder all-target/all-feature check, formatting, strict Clippy, and locked release build passed. Unit coverage includes unusable chroma crops and recording QPC evidence selecting copied frames instead of newer callback arrivals.
- The latest practice-tool run at `12:58:34Z` reached approximately 92 seconds and produced a fully decodable 1080p60 H.264/AAC partial, with no timestamp or NV12 conversion failure. Windows then reported recorder PID 18552 faulting in `GraphicsCapture.dll_unloaded` with access violation `0xc0000005` during target teardown. The native fix now closes the frame pool before the session, drains encoder/mux before releasing WGC, and retains the source stack if bounded callback shutdown cannot prove quiescence; unit tests, strict Clippy, and the locked release build passed after the change.
- The subsequent real game bundle `C:\Users\hugo\LeagueReplays\games\1790606051` was captured by the pre-target-close-fix release. It contains 709.013 seconds of probeable 1920x1080 H.264/AAC partial media (tail decode exit 0) and an atomically maintained `game_log.json` with 84 events and 70 snapshots; recorder poller diagnostics report 812 successful responses and zero JSON write failures. Finalization stopped when the selected HWND closed, leaving no canonical `video.mp4`/`metadata.json`. The independent API capture recorded 610 successful `/gamestats`, 63 `/eventdata`, and 69 `/allgamedata` responses before the game endpoint disappeared; this is retained as pre-fix evidence, not a completion claim.
- The target-close correction was then implemented and verified locally: 121 recorder library tests, 3 binary tests, and 3 fixture tests passed (5 ignored), strict all-target/all-feature Clippy passed, formatting passed, the locked release build passed, and `--diagnose` resolved the staged r6 runtime. A real post-fix League finalization run remains required.
- Capture-benchmark analyzer/collector tests passed 60/60; canonical validation passed for 60 items and 8 plan/checkpoint pairs; diff hygiene passed. The corrected release initialized at 12:48:59 UTC (PID 18552) with the verified r6 runtime. Its existing silent-audio fallback remains unchanged. The separate API diagnostic completed its 40-minute duration at 12:04:03 UTC and is stopped.

2026-09-29 fragmented-MP4 finalizer correction:

- Run `1790608674` logged graceful selected-HWND closure, reconciled 4,705 encoded frames, and a 118,909,806-byte partial, then failed publication. Its ffprobe summary omitted `nb_frames`; a separate read-only packet count found exactly 4,705. The live poller reported 87 successful responses and zero JSON write failures. This failure was the validator's missing fragmented-container count, not capture or target-close teardown.
- Added five fragment-reader unit tests covering video/audio separation, malformed/truncated/missing media boxes, ambiguous tracks, nonempty initial tables, optional sample fields, deadline/box limits, and an 8-GiB virtual media payload that must never be read. Focused finalizer tests passed 8/8; the opt-in real-file test was then exercised separately.
- `cargo test --manifest-path recorder/Cargo.toml --all-features finalizer::tests::real_fragmented_media_validates_and_publishes_a_copy -- --ignored --exact --nocapture` passed using the staged r6 ffprobe and `QUEUEBACK_FINALIZER_MEDIA`/`QUEUEBACK_FINALIZER_FRAMES` for `1790608674` (4,705 samples, 46.5447 ms validation) and `1790606051` (42,540 samples, 91.6437 ms). Both rejected an intentional one-frame producer mismatch and successfully published only disposable copies. Original SHA-256 hashes were unchanged before/after each check; the user library remains partial and has not been recovered or modified.
- Recorder all-target/all-feature check and tests passed (126 library, 3 binary, 3 fixture tests; 6 opt-in tests ignored in the default suite, including the separately exercised real-file test). Formatting and strict all-target/all-feature Clippy passed. These are focused correctness/latency observations, not a new formal long-file benchmark or a full live-game success claim.
- `cargo build --release --locked --manifest-path recorder/Cargo.toml` passed, replacing `recorder/target/release/recorder.exe`. Canonical validation passed for 60 items and 8 plan/checkpoint pairs; diff hygiene passed.
- Release `--diagnose` resolved the staged r6 runtime and NVENC/H.264 High. The default-log attempt was sandbox-denied under AppData; rerunning with `LEAGUE_REPLAY_LOG_DIR` under `build/perf/finalizer-fix-diagnostics` passed. Diagnostics still reports the existing silent-stereo fallback (no Windows loopback device found); this fix does not change audio discovery.
- Real post-fix Practice Tool run `1790670285` produced canonical `video.mp4`, `metadata.json`, and identity-matched schema-v2 `game_log.json`. Selected-HWND closure at 08:25:28.136 UTC followed the normal finalization path and publication completed at 08:25:28.200 UTC with no recorder error. The 63,752,518-byte video contains exactly 2,463 H.264 1920x1080/60-FPS frames (41.05 seconds); packaged ffprobe packet count matches producer/metadata, and strict single-thread full decode passed. The poller retained available calibration, 2 events and 4 snapshots with 45 successful responses and zero JSON write failures. The independent diagnostic captured LCU phases and Live data; raw captures remain local/ignored. The user subsequently launched the app and confirmed it all worked. This is real finalization/playback evidence, not a formal performance or long-match reliability pass. Audio remains the known silent fallback and is explicitly deferred by the user.

## Deviations

The original approved plan retained a packaged external FFmpeg WGC backend as an explicit alternative and current AMF/QSV route. The 2026-09-01 product architecture decision and capability audit materially invalidated that design: the external path had no unique validated production obligation, duplicated capture/lifecycle ownership, and exposed only unvalidated codec/vendor combinations. The original plan remains unchanged as history; the versioned v3 plan is the current design.

The first 1,800-second wrapper discarded its in-memory resource samples after a fixed 60-second frame-count probe timed out. The wrapper was corrected, but the product owner accepted the completed arm without repetition and without treating the missing formal series/full-file decode as passed.

## Decisions

- Native WGC/D3D11/direct-NVENC H.264 High 1080p60 is the sole supported Windows production recorder path.
- FFmpeg remains the audio encoder, fragmented-MP4 muxer, packaged probe/export runtime, and fixture tool; it is not a Windows video-capture backend.
- Unimplemented AMD/AMF, Intel/QSV, HEVC, and non-High native combinations are unsupported, not unvalidated current product paths. Their draft follow-ups may change that only after implementation and full evidence.
- The accepted 1,800-second local arm is not rerun solely for the lost sample series. This does not waive the target League matrix or broaden hardware claims.
- QB-PERF-001 remains invalid-but-accepted diagnostic pre-change evidence; it is not a formal performance pass.

## Blockers

The formal comparison on the specified interactive Ryzen 5 5600X/RTX 4060 target has not been run. The combined release now has a successful real short-session finalization and user-confirmed app playback, but that is not the required sustained capture/performance matrix. Generated-window or short-session evidence cannot substitute for that matrix.

## Next action

Recorder-fix verification is complete. The user's next requested work is refinement of the data features using existing captures, with audio deferred. When returning to QB-PERF-002, run the outstanding formal performance matrix under the approved v3 design; do not reopen the verified finalizer correction without new contradictory evidence.
