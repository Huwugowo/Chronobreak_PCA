# QB-CAP-009 execution checkpoint

Feature: `QB-CAP-009`
ExecPlan: `docs/exec-plans/qb-cap-009-process-owned-recording.md`
Updated: 2026-11-04

## Current milestone
Attached-source silence recovery implemented and validated with real WGC/NVENC capture, full decode and canonical publication. The corrected portable recorder is running with the previously validated fast app/helper build.

## Active unit
None. Lightweight implementation and focused validation complete; real League alt-tab confirmation remains an acceptance observation.

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
Actual League alt-tab behavior remains unverified. Generated-window silent-source rebuild, same-process recreation and minimize/restore now pass on the current hardware. No long soak or performance campaign is required for this implementation.

## Verification
- 2026-11-04 source-silence fix: all-target/all-feature recorder tests passed (271 library tests, 12 ignored; 7 integration tests; 3 lifecycle-fixture and 3 WGC-fixture example tests). Log: `.codex/logs/command-20261104-150900.log`. Coverage includes 120 seconds of visible source silence with healthy encode/mux progress, clean stop during silence, generation/visibility changes without false resumption, fresh-QPC resumption, observer-gap confirmation without source progress, and preserved encoder/mux/protocol failure precedence. The final obsolete test-only source branch was removed and its stop-time failure test rerun successfully (`.codex/logs/command-20261104-151213.log`).
- Current all-target/all-feature check, format check and strict Clippy passed: `.codex/logs/command-20261104-151007.log`. Production release build passed without `native-failure-injection`: `.codex/logs/command-20261104-150506.log`.
- Real generated-window `33 silent_restore` passed: `build/qb-data-003/capture-fixtures/a504eb17-abdc-47d6-8e83-24cda06ec6c0/result.json`. A feature-gated fault silences only the first leaf's callbacks after six seconds; replacement clears it. Logs prove an actual same-device leaf rebuild, fresh-QPC resumption, continued encoder/mux progress and no watchdog failure. Service events retain the same recording directory; full H.264 decode proves the restored green epoch and four moving samples, followed by canonical publication after original-process exit.
- Real generated-window `30 minimize_restore` passed: `build/qb-data-003/capture-fixtures/fc5719df-ef9b-4785-b18b-d1cf23f61138/result.json`; `16 recreate` passed: `build/qb-data-003/capture-fixtures/3761f8e4-e341-455f-b050-d85ca8cf9a90/result.json`. Both include waiting/fresh-resume events, same recording identity, full decode, moving restored footage and canonical publication. Only dedicated generated fixture libraries were used.
- Fixture corrections discovered by actual runs: static GDI content still generates WGC timestamps here, so pausing rendering alone did not exercise source silence (failed root `160711b7-d883-4c0f-9fec-5dae731be991`). A deterministic test-only callback fault now proves the actual recovery path. A subsequent real run rebuilt/resumed/published but the decoder rejected its expected process-exit duplicate tail (`90d53800-28e8-4de2-a699-e24a92cc1b25`). The validator now permits at most a short terminal tail while requiring four moving green samples immediately before it; negative frozen-restoration checks remain enforced. No production validation was weakened.
- Packaged fixed recorder with unchanged validated app SHA-256 `11121EE15DB959B3C76D3A5193EE659D172380775BBCDE0AE01548DE6D3C8F0E` under `build/release/watchdog-recovery-20261104`; runtime verification passed. Production recorder SHA-256 `14E3BA90CA3F6827C694BE7668995645DE51A64223376B421F8934898A7A87A5`. Confirmed the old recorder idle and no League process before replacement; running recorder PID 31968 and its preparation helper both resolve to this portable pair. Persistent preprod library/cache and unrelated edits were preserved.
- Final status endpoint confirmed PID 31968 `available`/`idle` with no capture problem or service failure. Canonical roadmap validator passed (114 items / 30 plan-checkpoint pairs), final diff whitespace check passed, and changed Rust-file format checks passed.
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
Window interruption and attached-source silence are recoverable and do not invalidate healthy footage. The existing latest-value worker control carries deduplicated generation/QPC rebuild requests; live worker evidence invalidates stale requests. Rebuild only the leaf, at the existing two-second cadence, after that HWND returns to foreground. Focus gates repair only. One watchdog availability state owns waiting/resumed status, and only actual fresh QPC proves resumption. Encoder/mux deadlines, protocol errors, same-process/same-adapter identity, strict callback cleanup and publication validation remain enforced. Continuous encoding can consume disk until process exit/user stop; existing write errors remain terminal and preserve partials. No additional worker, persisted health authority, automatic device rebuild or elapsed source-loss cutoff. Keep feature in progress pending real League acceptance. Original process-owned implementation was committed in `833c1dc`; this extension follows the user-approved lightweight implementation and focused validation.

## Blockers
None for this implementation. The previous NVENC `NO_ENCODE_DEVICE` restriction did not recur in the current interactive session. Real League acceptance remains an observation, not an automated fixture claim. Startup readiness, audio and library UX remain separate work.

## Next action
Confirm a real League alt-tab longer than 15 seconds returns to fresh footage in the same recording with the running fixed build. No further review or optimization campaign is required.
