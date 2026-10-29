# QB-CAP-006 execution checkpoint

Feature: `QB-CAP-006`
ExecPlan: `docs/exec-plans/qb-cap-006-league-launch-safety-v2.md`
Updated: 2026-10-29

## Current milestone

Review corrections to `ffc4c48` implemented and automatically verified; controlled hardware/League acceptance remains pending.

## Active unit

Correction implementation and verification complete. Two read-only scouts checked the affected paths; no delegated edits.

## Completed

Required bootstrap, existing investigation and focused scout challenge. Plan reviewed for cancellation, descriptor-generation instability, PID reuse, fixture compatibility and unavailable-API behavior. User explicitly requested continuing through implementation, overriding the workflow's planning-only stop.

- Implemented passive readiness before bundle/native creation, returning the final revalidated target; missing data never grants admission. Three samples/four seconds, progressing gameTime ≥5 seconds, stable game/window identity and five-second maximum gap. Existing HTTP transport/parser reused.
- Tiny/hidden/closed/replaced windows and unavailable/frozen/regressed/stale game signals reset evidence. Cancellation interrupts requests and sleeps. UI stays idle during waiting.
- Process identity includes creation time. Read-only direct Windows process queries validate each pre/post-request target and final admission; unknown identity defers. Native failures are terminal for that process, while a replacement process earns new readiness. Active capture behavior remains unchanged.
- Added correlated, sanitized admission/native item/frame/startup/cleanup logs. Updated the existing generated service fixture with constructor-injected readiness behind the existing test feature; normal CLI/config always uses local Live data.
- Focused independent review found and resolved the stale watcher identity gap and avoided full-system process scans in admission. Final review found no remaining concrete defects in scope.
- Review corrections: lightweight HWND/PID/physical size/DPI/monitor checks during readiness; DXGI only at admission. Request-interval clock-offset intersection retains 250 ms combined jitter, accepts RTT up to the existing two-second network deadline and keeps 500 ms post-response freshness.
- Initial invalid/tiny capture-item dimensions use the validated HWND pool size. The same source waits for usable content before converter/audio/mux/NVENC startup; ongoing tiny frames extend startup waiting, stalls/errors/cancellation still stop it, and no native retry loop is introduced. Unusable content is rejected even when its dimensions equal the pool's.
- Capture preparation creates the full source/device/MTA owner before callbacks are registered. Later partial initialization closes it explicitly after dependent locals drop; stage, primary error chain and cleanup outcome are logged. Existing full-stack retention on failed callback quiescence is preserved.

## In flight

None for this review correction.

## Remaining

Run the unchanged repeated controlled Practice Tool absent/passive/native launch comparison on an interactive supported Windows setup. Obtain a successful generated native capture check there. No benchmark campaign was performed or claimed.

## Verification

Current review correction:

- Recorder locked all-target/all-feature check, tests, formatting check and strict Clippy passed. Tests: 215 library + 4 binary + 3 fixture tests (222 total), nine hardware/profile fixtures ignored. Seven added tests cover request timing uncertainty, accumulated drift, adapter-resolution/admission races, valid/tiny/invalid initial pool sizing, progress-only deadline extension and primary/cleanup error preservation. Existing cancellation and callback ownership tests also pass. Test/lint log: `.codex/logs/command-20261029-153237.log` (ignored local artifact).
- Feature artifact validation passed: 104 items / 23 plan-checkpoint pairs. Diff whitespace check passed. The existing descriptor generation/adapter admission diagnostic was preserved; final formatting and strict Clippy passed after that diagnostic-only adjustment.
- Hardware-only verification was not attempted for this correction. A source that stops delivering frames still fails closed; real WGC restoration/cleanup and League launch compatibility remain unverified on supported hardware.

Previous implementation (`ffc4c48`):

- `cargo test --manifest-path recorder/Cargo.toml --locked --all-targets --all-features`: passed 208 library + 4 binary + 3 fixture tests (215 total); nine hardware/profile fixtures ignored. Twelve added library tests cover progression/span, loading/frozen/regressed/jumped/stale clocks, latency, window/game identity, malformed/missing data, unavailable/hidden recovery, in-request replacement, pending-request cancellation, ten-minute unavailable wait without admission, temporary 1×1 restore, native failure suppression and process replacement/probing (some scenarios share tests).
- `cargo clippy --manifest-path recorder/Cargo.toml --locked --all-targets --all-features -- -D warnings`: passed. Initial final-run lint found a collapsible conditional in the new tiny-window test; corrected and rerun successfully.
- Recorder all-target/all-feature `cargo check`, formatting check and both generated-window example builds passed. Logs retained under ignored `build/verification/qb-cap-006/` (copied from the required verification logger).
- Feature artifact validation passed: 104 items / 23 plan-checkpoint pairs. `git diff --check` passed.
- One 10-second generated service fixture attempted in dedicated test storage: first invocation lacked its executable-relative runtime; retried with the existing validated whole-directory development runtime override. Evidence `build/qb-data-003/capture-fixtures/5e7ed16d-71bb-4dc8-ad23-28a016d52242/service.log` shows Collecting at 0 ms, Ready at 4030 ms, bundle/native initialization afterwards, then terminal native startup failure querying WGC support (`0x8007000E`, insufficient memory/resources). Clean service shutdown, one startup attempt, zero scheduled retries. This is a failed end-to-end capture check, not a capture pass or proof of game stability.
- No real League launch comparison, recording-completeness guarantee, historical incident attribution, performance or hardware support claim is made.

## Deviations

The review identified material limitations in initial sizing, fixed midpoint timing bounds, passive adapter work and partial initialization. Approved correction design supersedes the original plan without editing it. User explicitly requested implementation through commit and hardware-only verification pending.

## Decisions

No failure-open fallback. Opening seconds or the entire recording may be lost. Native initialization failures suppress that process; readiness misses remain passive. Recovery from initial tiny frames retains one graph and delays audio/encoding until usable content; it does not retry GPU creation. The API does not atomically bind renderer readiness to an HWND: local signals, conservative progression/stability and immediate revalidation reduce exposure but cannot guarantee graphics-driver containment. OS creation time uses the existing second-resolution identity; HWND validation supplies the immediate window safeguard.

## Blockers

No implementation blocker. Required controlled League launches were not run. Generated native capture is unverified in the current execution context because Windows rejected its WGC support query. Keep the feature non-done pending live evidence.

## Next action

Obtain the existing hardware/League acceptance evidence before marking QB-CAP-006 done. No hardware-only attempt belongs to this correction task.
