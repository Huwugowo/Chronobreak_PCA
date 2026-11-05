# QB-CAP-006 execution checkpoint

Feature: `QB-CAP-006`
ExecPlan: `docs/exec-plans/qb-cap-006-window-admission-v4.md`
Updated: 2026-11-05

## Current milestone

V4 window-only admission implemented and automatically verified. The generated production-service run admitted capture without Live data after 2.025 seconds; real League behavior remains unverified.

## Active unit

None. Focused implementation, independent review and automated verification are complete.

## Completed

Required bootstrap, existing investigation and focused scout challenge. Plan reviewed for cancellation, descriptor-generation instability, PID reuse, fixture compatibility and unavailable-API behavior. User explicitly requested continuing through implementation, overriding the workflow's planning-only stop.

- V4 (2026-11-01): replaced Live GameStats admission with two matching usable physical-window observations spanning two seconds. Window absence clears stability; identity/geometry changes and gaps over five seconds restart it. Final DXGI/snapshot/cancellation validation remains fresh within 500 ms. Real query/adapter failures propagate terminally instead of becoming silent retries.
- Removed pre-capture HTTP/readiness source ownership and synthetic GameStats injection. The existing generated-window service fixture now uses production admission and unavailable Live polling. Exact process ownership, Alt+F4/restart boundaries, native deadlines/sizing/cleanup and publication validation are unchanged.
- The following bullets and older verification sections describe predecessor implementations, retained as provenance rather than current gameplay admission policy.

- Implemented passive readiness before bundle/native creation, returning the final revalidated target; missing data never grants admission. Three samples/four seconds, progressing gameTime ≥5 seconds, stable game/window identity and five-second maximum gap. Existing HTTP transport/parser reused.
- Tiny/hidden/closed/replaced windows and unavailable/frozen/regressed/stale game signals reset evidence. Cancellation interrupts requests and sleeps. UI stays idle during waiting.
- Process identity includes creation time. Read-only direct Windows process queries validate each pre/post-request target and final admission; unknown identity defers. Native failures are terminal for that process, while a replacement process earns new readiness. Active capture behavior remains unchanged.
- Added correlated, sanitized admission/native item/frame/startup/cleanup logs. Updated the existing generated service fixture with constructor-injected readiness behind the existing test feature; normal CLI/config always uses local Live data.
- Focused independent review found and resolved the stale watcher identity gap and avoided full-system process scans in admission. Final review found no remaining concrete defects in scope.
- Review corrections: lightweight HWND/PID/physical size/DPI/monitor checks during readiness; DXGI only at admission. Request-interval clock-offset intersection retains 250 ms combined jitter, accepts RTT up to the existing two-second network deadline and keeps 500 ms post-response freshness.
- Initial invalid/tiny capture-item dimensions use the validated HWND pool size. The same source waits for usable content before converter/audio/mux/NVENC startup; ongoing tiny frames extend startup waiting, stalls/errors/cancellation still stop it, and no native retry loop is introduced. Unusable content is rejected even when its dimensions equal the pool's.
- Capture preparation creates the full source/device/MTA owner before callbacks are registered. Later partial initialization closes it explicitly after dependent locals drop; stage, primary error chain and cleanup outcome are logged. Existing full-stack retention on failed callback quiescence is preserved.
- Follow-up correction bounds native startup by one absolute 15-second deadline beginning before worker spawn, retaining five-second source inactivity. Ongoing unusable frames cannot extend that cap; timeout is terminal and explicitly joins the worker. Initial item/frame admission requires dimensions covering the admitted physical client, separate from converter capability. One recovery failure event/error chain retains count, first/latest/expected dimensions, elapsed time and terminal reason.
- Review of `962b1cb`: a shared guarded-stage helper checks cancellation/deadline before source construction, recovery, converter, mux, NVENC and completion. Post-source checks remain inside the existing initialization result closure, dropping dependent locals before explicit source close. Already-reported protocol errors precede generic timeout while cancellation retains priority.

## In flight

None.

## Remaining

Compare switching away during the League loading screen with the recorder stopped. The user reports that switching away during active gameplay succeeds with their rebuilt recorder and app, while switching away during the loading screen still leaves the game black. The recorder-off comparison is pending and the cause remains unattributed.

## Verification

V4 window admission (2026-11-01):

- Locked recorder all-target/all-feature check, tests, fmt and strict Clippy passed. Tests: 262 library + 7 binary + 3 fixture (272 total), 12 hardware/runtime tests ignored. Nine admission regressions cover no-Live admission, a positive stability span, unavailable/tiny/wrong-process windows, geometry/identity/gap resets, final HWND/monitor races, slow resolution, process-exit cancellation/new-generation readiness, cancellation during sleep/resolution and query/adapter errors. Check log `.codex/logs/command-20261101-103454.log`, test log `.codex/logs/command-20261101-103511.log`, Clippy log `.codex/logs/command-20261101-103606.log`.
- Both existing generated examples built. One isolated 10-second steady service fixture used production admission with Live API unavailable: Collecting at 0 ms, Ready after 2025 ms, WGC item created, NVENC returned `NO_ENCODE_DEVICE` after 195 ms of native initialization, `cleanup_ok=true`, one startup attempt, zero retries, no canonical publication and no remaining fixture child. End-to-end encoding failed; this establishes real window-only admission and safe failure, not playable video. Evidence: `build/qb-data-003/capture-fixtures/f99ba028-1bc4-486d-82eb-23f92f474f08/service.log`.
- No real League launch, black-screen causality, successful NVENC encoding or capture-performance claim. Existing process/native tests pass unchanged.
- Focused independent review found no actionable defects. Canonical validation passed (107 items, 27 plan/checkpoint pairs); formatting and diff whitespace checks passed.

Live observation, 2026-10-31:

- One ordinary League session deferred on aggregate clock/window readiness, admitted at gameTime 36.624569s and began recording at 23:00:54 Europe/Paris. Native capture advanced until selected-HWND closure; the later process-end confirmation failure is tracked under QB-CAP-002. Sanitized runtime log, image identity, probe and diagnostic limits: `docs/reviews/recorder-live-incident-2026-10-31.md`. This is neither the controlled repeated launch comparison nor a finalized recording pass; remaining acceptance gates are unchanged. No product-code change or new test-suite run.

Construction-boundary correction:

- Locked recorder all-target/all-feature check and tests passed: 224 library + 4 binary + 3 fixture (231 total), nine hardware/profile fixtures ignored. Four new regressions cover already-cancelled/expired session entry, cancellation between constructors with dependent destruction, a constructor exceeding its deadline preventing mux/NVENC entry, and protocol-error precedence with explicit worker join. Tests exercise the real session entry and shared stage helper; they do not simulate proof of native driver or COM rollback behavior. Log: `.codex/logs/command-20261029-163834.log`.
- Recorder formatting check and strict all-target/all-feature Clippy passed. Canonical feature validation and diff whitespace checks passed.
- No hardware fixtures or League capture were run. Strict expected-client sizing remains conservative; real transitions, hard driver hangs and QB-CAP-007 partial native failure injection remain unverified.

Bounded-recovery correction:

- Locked recorder all-target/all-feature check/tests, formatting check and strict Clippy passed. Tests: 220 library + 4 binary + 3 fixture (227 total), nine hardware/profile fixtures ignored. Five new regressions cover the absolute cap under continuous progress, explicit terminal worker join, already-expired startup despite ready evidence, expected-client size admission, and no-frame versus unusable-frame diagnostic preservation. Existing sizing and inactivity tests were updated. Log: `.codex/logs/command-20261029-155718.log`.
- No hardware tests were run. Real WGC transitions may conservatively fail the expected-client-size policy; hardware/League compatibility remains pending. The deadline bounds cooperative startup, not an unreturning native call or cleanup join. Partial callback-registration/StartCapture/quiescence failure injection remains QB-CAP-007.
- Canonical feature validation passed (104 items, 23 plan/checkpoint pairs); diff whitespace check passed.

Previous review correction (`dcaaa0f`):

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

2026-11-01: the October 31 recording demonstrates delayed Live-clock admission. The user explicitly requested fixing startup after the lifecycle audit and clarified that the first process was Alt+F4 closed before relaunch. Version 4 replaces gameplay readiness with two stable window observations, preserving the native safety boundary. No speculative overlap policy is introduced. The user's earlier override authorizes direct implementation, proportionate checks and commit.

Review of `dcaaa0f` demonstrated that progress-based recovery lacked an absolute bound and crop convertibility admitted tiny surfaces. Version 3 supersedes that design while preserving earlier plans. The requested focused correction proceeds through implementation and commit; deeper failure injection stays under QB-CAP-007.

Review of `962b1cb` exposed missing enforcement at native constructor boundaries. The correction implements the existing v3 cancellation/deadline contract and keeps its design, dimensions policy and artifact links unchanged.

## Decisions

Current policy is v4 window stability, independent of the Live clock. Retain the native 15-second absolute/five-second inactivity bounds, expected-client sizing, one native attempt per process, callback quiescence and exact FILETIME/OwnedHandle identity from CAP-009. Even-rounded physical client dimensions accept larger border-inclusive content and conservatively reject shrinkage during initial construction. There is no timeout admission fallback or GPU retry loop. A window change during native construction may still fail startup safely; replaceable-source recovery applies once the native graph is established. Window stability and revalidation do not prove driver safety or League compatibility.

## Blockers

No implementation blocker. The current generated capture reaches WGC but NVENC returns NO_ENCODE_DEVICE. Real League behavior and successful capture are unverified; do not claim black-screen resolution.

## Next action

Observe ordinary real-League startup/alt-tab/exit/relaunch behavior on supported hardware before marking the feature done. The focused correction is implemented and verified without a long soak or benchmark gate.
