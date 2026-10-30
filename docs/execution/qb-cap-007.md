# QB-CAP-007 execution checkpoint

Feature: `QB-CAP-007`
ExecPlan: `docs/exec-plans/qb-cap-007-partial-wgc-startup.md`
Updated: 2026-10-30

## Current milestone

Implementation and focused verification complete.

## Active unit

None.

## Completed

Bootstrap, actual lifecycle trace, authoritative WinRT/bindings inspection and
read-only design challenge. Full owner construction already precedes callbacks.

Implemented idle-before-drain/Close, owner-proof-based completion/retention,
token preservation on removal errors and accurate initialization error context.
Implicit source Drop now uses the same proof before capture/device/MTA release.
Added three deterministic tests using actual COM delegates and one ignored
generated-WGC rollback fixture; injections compile only under cfg(test).

Focused implementation review found no separate ownership defect. Its coverage
challenge was reconciled: Closed injection follows real FrameArrived registration,
StartCapture injection follows both real registrations; neither pretends to force
a vendor HRESULT. Strengthened the ambiguous retained-handler error test to run
through the production startup sequence and made after-start injection await a
real captured frame before failing. No pipeline changes.

## In flight

None.

## Remaining

No remaining source work for this correction. League compatibility/performance
and actual vendor failure/hang behavior remain separate observations, not claimed
passes.

## Verification

- Final locked recorder all-target/all-feature check passed
  (`.codex/logs/command-20261030-154838.log`).
- Final locked recorder all-target/all-feature tests passed: 244 library + 4
  binary + 3 fixture tests (251), 11 ignored
  (`.codex/logs/command-20261030-154728.log`). Three new deterministic tests cover
  operation/token boundaries, actual generated COM AddRef/Release and late Invoke,
  retained registration after an error, idle failure skipping drain/Close, owner
  failure requiring retention, and drop-safe cleanup errors after final Release.
- Final strict all-target/all-feature Clippy passed
  (`.codex/logs/command-20261030-154845.log`); formatting check passed.
- `generated_wgc_partial_start_rollback -- --ignored --nocapture` passed against
  the existing generated `wgc_fixture.exe` window. All four points passed:
  FrameArrived operation boundary; Closed boundary after real frame registration;
  StartCapture boundary after both real registrations; after actual StartCapture
  and a received real frame. Each returned the original injected error with
  `source cleanup complete`, establishing successful pool/session cleanup and
  zero handler owners before source destruction. A subsequent successful owner
  also exercised implicit Drop. Evidence:
  `build/verification/qb-cap-007/rollback-test.log` and fixture logs.
- The initial WGC run also passed; the final rerun was specifically required by
  the new real-frame precondition. The fixtures create no recording or media file.
- Native API errors were injected at the call boundaries, not forced inside
  GraphicsCapture.dll. Retained-handler and timeout cases use actual COM delegates
  with deterministic unit seams; they are not proof of vendor failure behavior.
  Teardown ordering and stopping/admission invariants are source-level guarantees;
  the native fixture establishes the corresponding successful rollback path.
- Initial restricted check never ran because the mandatory command logger could
  not write its protected log; rerun with log access passed.

## Deviations

The user explicitly requests implementation through commit in this session,
overriding the workflow's stop after the planning handoff.

## Decisions

Return before Close when idle proof fails. A cleanup HRESULT error does not
require retention after idle and handler-owner release are proven. No UAF claim;
vendor docs do not specify synchronous revocation. Preserve pool/session order.
The implicit Drop path previously ignored failed shutdown and could defeat
retention. ManuallyDrop fields and one explicit ordered destruction site apply
the same rule to implicit Drop/unwind; no frame-path or startup work is added.

## Blockers

None for this correction. Real generated WGC worked in this session despite the
earlier CAP-006 context's support-query failure. League launch/render/FPS/GPU and
interactive playback observations were not run. No hard native-call interruption
or exhaustive driver-race guarantee is claimed.

## Next action

No further QB-CAP-007 source action. Pursue League/hardware performance only when available.
