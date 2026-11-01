# QB-CAP-009 execution checkpoint

Feature: `QB-CAP-009`
ExecPlan: `docs/exec-plans/qb-cap-009-process-owned-recording.md`
Updated: 2026-11-01

## Current milestone
Review of `833c1dc` complete. No actionable production-lifetime regression found; three fixture defects corrected and focused checks passed.

## Active unit
None. Production implementation and fixture review fixes are complete; actual NVENC recreation/minimize capture remains unverified.

## Completed
- Retained limited-rights Windows `OwnedHandle` shared through `Arc<OriginalProcess>` from readiness through capture/stop. A different watcher selection cannot replace the still-live original owner. No new unsafe Send/Sync implementation.
- WGC leaf detaches/replaces on the existing GPU worker; device/MTA, converter, encoder, mux, last image and CFR epoch persist. Reuse existing resize handling and bounded two-tick catch-up. Reacquisition is limited to once per two seconds.
- Typed window disappearance is recoverable. Other WGC/driver/callback/encoder/mux failures remain terminal. Callback quiescence is required before leaf release; unproven cleanup retains the whole guarded context and suppresses new capture until relaunch.
- Removed the source-loss timeout. Healthy last-image CFR continues until original-process exit or user stop, subject to real resource/write errors. Only source freshness pauses; downstream watchdogs remain enforced. Source counters and first failures are cumulative.
- Window closure no longer stops the worker or independently vetoes publication. Automatic stop still confirms the exact original process after join; existing terminal/media/frame-count validation is unchanged. Tray reports waiting for a window and resumes normal recording status on fresh source progress.
- Extended existing generated-window/service fixtures with close-window, same-process recreation and minimize/restore scenarios, synchronized after Recording. Full decode checks timeline length, replacement color epoch and restored motion. Uses only generated media in dedicated fixture roots.

- Review corrections: enforce enough time after restore (recreate >=13 seconds; minimize/restore >=27 seconds), require decoded restored color plus sustained motion, and require waiting then resumed service events for recreation. The marker now turns green after minimize/restore too. Convert decoded H.264 to RGB before shrinking so chroma sampling cannot pull in surrounding colors; ignore small compression noise when checking motion.
- Publication-wait/kill errors now remain inside the exercise result until service shutdown/join and fixture reap complete. Primary and cleanup errors are both retained.

## In flight
None. Unrelated app/audio/probe changes and the separate library proposal remain untouched.

## Remaining
Decoded continuity for recreation/minimize and actual League behavior remain unverified on compatible hardware. No long soak or performance campaign is required for committing the implementation.

## Verification
- `cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features`: passed, 266 library + 7 integration + 3 fixture tests; 12 ignored. Includes window-first finalization, explicit-stop versus unexpected worker exit, failure precedence, detached-source watchdog/rearm, cumulative leaf accounting, existing callback-quiescence and exact-process-handle tests. Log: `.codex/logs/command-20261101-023649.log`.
- `cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings`: passed. Log: `.codex/logs/command-20261101-023813.log`.
- `cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features`: passed.
- `cargo fmt --manifest-path recorder/Cargo.toml -- --check`: passed.
- Existing fixture examples built with `native-failure-injection`. Short real-service `16 recreate` run admitted the generated HWND and created the WGC item, then NVENC returned `NO_ENCODE_DEVICE`; cleanup completed and no replay was falsely published. Log: `build/qb-data-003/capture-fixtures/2be643ea-798a-4470-8526-b8191d6323c6/service.log`. Therefore no decoded recreation/minimize or real-game runtime pass is claimed. A first restricted-environment attempt failed earlier at WGC support with `0x8007000E`.
- Canonical roadmap validator passed (107 items / 27 plan-checkpoint pairs); formatting and staged diff whitespace checks passed.
- Read-only review of process ownership, detach/reattach, resource lifetime and failure/publication precedence found no additional actionable regression.

- Review validation: existing generated WGC rollback test passed on a real fixture HWND for all four injected startup failure points (FrameArrived, Closed, StartCapture, AfterStartCapture), plus normal guarded Drop. Command: `cargo test --manifest-path recorder/Cargo.toml --lib --all-features generated_wgc_partial_start_rollback -- --ignored --nocapture` with `QUEUEBACK_TEST_WGC_PID` set to the generated child. One test passed; no encoder required.
- Final review checks: six targeted example tests passed (three new duration/decoded-evidence/error-preservation regressions plus three existing WGC marker tests); all-target/all-feature check, format check and strict Clippy passed. Logs: `.codex/logs/command-20261101-074832.log`, `.codex/logs/command-20261101-074839.log`. Unchanged production code retains the prior full-suite pass; no redundant full suite or long hardware run was required.
- Actual synthetic H.264 decode verified the fixture's RGB thumbnail filter, red-to-green epoch and four advancing final samples across 13 frames. Artifact: `build/cap009-review-9tpg3r2i/synthetic-restoration.mp4`. An initial version scaled subsampled chroma before RGB conversion and misread green as red; the corrected filter passed. This validates fixture pixel extraction, not real WGC/NVENC recording continuity.
- Independent read-only reviews of service/process/watchdog, native lifetime/callback/error paths and CFR/fixture behavior found the three fixture defects above; no additional out-of-scope defect was identified.

## Deviations
User explicitly replaced the five-minute cutoff and oversized mandatory verification with proportionate checks, and authorized direct implementation/commit without another planning approval. Initial restricted test runs could not pin ancestor publication directories or write command logs; the permitted local test run passed. Lint findings were corrected. No validation or safety invariant was weakened.

## Decisions
Window interruption is recoverable and does not invalidate healthy footage. Continuous encoding can consume disk until the process exits/user stops; existing write errors are terminal and preserve partials. No additional worker, persisted health authority, automatic device rebuild or arbitrary timeout. Keep feature in progress solely because decoded hardware continuity is unverified. Production implementation was committed in `833c1dc`; the review fixes preserve that design.

## Blockers
Runtime continuity check: this environment's NVENC session returned `NO_ENCODE_DEVICE`. Startup readiness, audio and library UX remain separate work.

## Next action
When a compatible NVENC interactive session is available, run the existing service fixture with `16 recreate` and `30 minimize_restore` (and a short `close_window` run); record actual capture/decode results without expanding validation scope.
