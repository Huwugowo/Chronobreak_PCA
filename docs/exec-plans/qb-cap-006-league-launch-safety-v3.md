# QB-CAP-006 — bounded initial-frame recovery

## Purpose

Stop abnormal native startup even when tiny frames keep arriving, while allowing a short recovery period within one source.

## Relevant planning-time architecture

The passive GameStats/window gate precedes native startup. The service supplies a 15-second startup timeout; lifecycle and initial-source waits currently extend deadlines on progress. The source wait has a five-second inactivity budget. Explicit source close and worker join preserve callback/device/MTA ownership. CaptureTarget dimensions are physical client dimensions rounded down to even pixels.

## Scope and non-goals

Correct absolute deadlines, initial content sizing and terminal recovery diagnostics only. Preserve passive admission, active capture, cancellation, media publication and native per-process failure suppression. Keep callback failure-injection assurance under QB-CAP-007 and physical capture/League verification pending.

## Exploration findings

Continuously unusable frames can refresh both deadlines forever. A valid NV12 crop can still represent a 4x4 surface. Initial pool sizing and first-frame recovery therefore need an admission predicate distinct from converter capabilities. Existing lifecycle tests expose explicit joins and can prove bounded startup without GPU simulation.

## Chosen design and rationale

Compute one absolute deadline before spawning the native worker; pass it to initial-frame recovery and lifecycle waiting. Reuse the existing 15-second budget for direct native-session callers. Keep the five-second source inactivity deadline, capped by the absolute deadline. Check the deadline before accepting startup and cap frame waits to remaining time. Timeout requests cooperative stop, joins and fails terminally for that process; it never schedules GPU retries. This bounds the admission/recovery loop, not an unreturning driver call or cleanup join.

Require initial item/frame dimensions to cover the admitted physical client dimensions, in addition to NV12 convertibility. Prefer suitable item dimensions; otherwise allocate the same pool from the validated target. Larger border-inclusive surfaces remain acceptable; shrinkage defers until recovery expires. No arbitrary pixel threshold or percentage tolerance is introduced; real WGC compatibility remains pending.

On recovery failure emit one structured event with elapsed time, unusable count, first/latest observed dimensions, expected dimensions, terminal stage/reason and underlying error. Retain the same facts in the returned error chain. Do not log per frame or change teardown ownership.

## Rejected alternatives and planning decisions

Reject unlimited progress extension, GPU retries, arbitrary minimum resolutions, backend redesign and broad callback injection. A bounded conservative false rejection is preferable to keeping WGC active during abnormal launch.

## Milestones

1. Implement absolute/inactivity bounds, initial content predicate and terminal telemetry.
2. Add focused regression tests, run recorder checks, update concise evidence and commit.

## Verification design

Test continuous progress hitting the absolute cap, earlier inactivity, late readiness rejection and explicit terminal worker join; tiny/small/odd/normal/border-inclusive dimensions; and distinct no-frame/unusable-frame diagnostics. Run locked recorder all-target/all-feature check/tests, fmt and strict Clippy, canonical feature validation and diff hygiene. Do not run hardware fixtures.

## Performance and reliability gates

Constant-size recovery state, no new per-frame allocations/logging, no new GPU initialization attempts. Preserve explicit callback-safe cleanup, source accounting and recording epoch. Hardware compatibility and hard driver-hang containment are not claimed.
