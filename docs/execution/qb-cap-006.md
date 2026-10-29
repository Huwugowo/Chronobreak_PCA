# QB-CAP-006 execution checkpoint

Feature: `QB-CAP-006`
ExecPlan: `docs/exec-plans/qb-cap-006-league-launch-safety.md`
Updated: 2026-10-29

## Current milestone

Milestones 1–3 implementation and focused automated verification complete; controlled live launch comparison remains outstanding.

## Active unit

No product-code unit in flight. Final diff/commit handoff.

## Completed

Required bootstrap, existing investigation and focused scout challenge. Plan reviewed for cancellation, descriptor-generation instability, PID reuse, fixture compatibility and unavailable-API behavior. User explicitly requested continuing through implementation, overriding the workflow's planning-only stop.

- Implemented passive readiness before bundle/native creation, returning the final revalidated target; missing data never grants admission. Three samples/four seconds, progressing gameTime ≥5 seconds, stable game/window identity, 500 ms response bound and five-second maximum gap. Existing HTTP transport/parser reused.
- Tiny/hidden/closed/replaced windows and unavailable/frozen/regressed/stale game signals reset evidence. Cancellation interrupts requests and sleeps. UI stays idle during waiting.
- Process identity includes creation time. Read-only direct Windows process queries validate each pre/post-request target and final admission; unknown identity defers. Native failures are terminal for that process, while a replacement process earns new readiness. Active capture behavior remains unchanged.
- Added correlated, sanitized admission/native item/frame/startup/cleanup logs. Updated the existing generated service fixture with constructor-injected readiness behind the existing test feature; normal CLI/config always uses local Live data.
- Focused independent review found and resolved the stale watcher identity gap and avoided full-system process scans in admission. Final review found no remaining concrete defects in scope.

## In flight

None.

## Remaining

Run the unchanged repeated controlled Practice Tool absent/passive/native launch comparison on an interactive supported Windows setup. Obtain a successful generated native capture check there. No benchmark campaign was performed or claimed.

## Verification

- `cargo test --manifest-path recorder/Cargo.toml --locked --all-targets --all-features`: passed 208 library + 4 binary + 3 fixture tests (215 total); nine hardware/profile fixtures ignored. Twelve added library tests cover progression/span, loading/frozen/regressed/jumped/stale clocks, latency, window/game identity, malformed/missing data, unavailable/hidden recovery, in-request replacement, pending-request cancellation, ten-minute unavailable wait without admission, temporary 1×1 restore, native failure suppression and process replacement/probing (some scenarios share tests).
- `cargo clippy --manifest-path recorder/Cargo.toml --locked --all-targets --all-features -- -D warnings`: passed. Initial final-run lint found a collapsible conditional in the new tiny-window test; corrected and rerun successfully.
- Recorder all-target/all-feature `cargo check`, formatting check and both generated-window example builds passed. Logs retained under ignored `build/verification/qb-cap-006/` (copied from the required verification logger).
- Feature artifact validation passed: 104 items / 23 plan-checkpoint pairs. `git diff --check` passed.
- One 10-second generated service fixture attempted in dedicated test storage: first invocation lacked its executable-relative runtime; retried with the existing validated whole-directory development runtime override. Evidence `build/qb-data-003/capture-fixtures/5e7ed16d-71bb-4dc8-ad23-28a016d52242/service.log` shows Collecting at 0 ms, Ready at 4030 ms, bundle/native initialization afterwards, then terminal native startup failure querying WGC support (`0x8007000E`, insufficient memory/resources). Clean service shutdown, one startup attempt, zero scheduled retries. This is a failed end-to-end capture check, not a capture pass or proof of game stability.
- No real League launch comparison, recording-completeness guarantee, historical incident attribution, performance or hardware support claim is made.

## Deviations

No material design deviation. Process generation validation uses direct query-only Win32 process creation times to avoid sysinfo's full process snapshot during readiness.

## Decisions

No failure-open fallback. Opening seconds or the entire recording may be lost. Native initialization failures suppress that process; readiness misses remain passive. Existing active tiny-frame handling stays unchanged. The API does not atomically bind renderer readiness to an HWND: local signals, conservative progression/stability and immediate revalidation reduce exposure but cannot guarantee graphics-driver containment. OS creation time uses the existing second-resolution identity; HWND validation supplies the immediate window safeguard.

## Blockers

No implementation blocker. Required controlled League launches were not run. Generated native capture is unverified in the current execution context because Windows rejected its WGC support query. Keep the feature non-done pending live evidence.

## Next action

On a supported interactive desktop, repeat the generated fixture with the pinned runtime, then collect the controlled Practice Tool launch comparison. Preserve the current implementation and passing focused evidence; do not reopen the separate performance matrix or active tiny-frame behavior.
