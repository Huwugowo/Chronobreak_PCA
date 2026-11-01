# QB-CAP-009 execution checkpoint

Feature: `QB-CAP-009`
ExecPlan: `docs/exec-plans/qb-cap-009-process-owned-recording.md`
Updated: 2026-11-01

## Current milestone
Production implementation complete; focused automated checks passed. Short hardware continuity validation could not start encoding.

## Active unit
Implementation integrated and ready for use; no further production change is currently identified. Hardware continuity follow-up remains separate from this commit.

## Completed
- Retained limited-rights Windows `OwnedHandle` shared through `Arc<OriginalProcess>` from readiness through capture/stop. A different watcher selection cannot replace the still-live original owner. No new unsafe Send/Sync implementation.
- WGC leaf detaches/replaces on the existing GPU worker; device/MTA, converter, encoder, mux, last image and CFR epoch persist. Reuse existing resize handling and bounded two-tick catch-up. Reacquisition is limited to once per two seconds.
- Typed window disappearance is recoverable. Other WGC/driver/callback/encoder/mux failures remain terminal. Callback quiescence is required before leaf release; unproven cleanup retains the whole guarded context and suppresses new capture until relaunch.
- Removed the source-loss timeout. Healthy last-image CFR continues until original-process exit or user stop, subject to real resource/write errors. Only source freshness pauses; downstream watchdogs remain enforced. Source counters and first failures are cumulative.
- Window closure no longer stops the worker or independently vetoes publication. Automatic stop still confirms the exact original process after join; existing terminal/media/frame-count validation is unchanged. Tray reports waiting for a window and resumes normal recording status on fresh source progress.
- Extended existing generated-window/service fixtures with close-window, same-process recreation and minimize/restore scenarios, synchronized after Recording. Full decode checks timeline length, replacement color epoch and restored motion. Uses only generated media in dedicated fixture roots.

## In flight
None. Preserve unrelated app/audio/probe changes and the separate library proposal.

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

## Deviations
User explicitly replaced the five-minute cutoff and oversized mandatory verification with proportionate checks, and authorized direct implementation/commit without another planning approval. Initial restricted test runs could not pin ancestor publication directories or write command logs; the permitted local test run passed. Lint findings were corrected. No validation or safety invariant was weakened.

## Decisions
Window interruption is recoverable and does not invalidate healthy footage. Continuous encoding can consume disk until the process exits/user stops; existing write errors are terminal and preserve partials. No additional worker, persisted health authority, automatic device rebuild or arbitrary timeout. Keep feature in progress solely because decoded hardware continuity is unverified; implementation is ready to commit.

## Blockers
Runtime continuity check: this environment's NVENC session returned `NO_ENCODE_DEVICE`. Startup readiness, audio and library UX remain separate work.

## Next action
When an NVENC-capable interactive session is available, run the existing fixture briefly for recreation and minimize/restore and inspect its decode result; then record actual results without expanding the validation scope.
