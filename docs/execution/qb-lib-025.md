# QB-LIB-025 execution checkpoint

Feature: `QB-LIB-025`
ExecPlan: `docs/exec-plans/qb-lib-025-guarded-library-mutations-v2.md`
Updated: 2026-11-03

## Current milestone

All milestones complete: guarded mutation authority and retained-tree deletion
verified against the final implementation.

## Active unit

None.

## Completed

Read mandatory product/workflow, both complete feature entries, plan and verification
contracts, and applicable storage/playback architecture. Read-only scouts traced
every mutation caller and existing Windows ownership primitives.
Plan review found no blocking defect; added explicit same-process lock contention
verification. Finalized/linked shared plan and both checkpoints; canonical
validation passed (111 items / 30 plan-checkpoint pairs).
Shared guards extracted; exact-handle recursive/clip removal and canonical root
lease implemented. Four focused ownership tests passed, including intended final
directory deletion, descendant rename exclusion, aliases and root rejection.
Caller integration, admission-boundary replacement and cancellation regressions,
actual subprocess arbitration/crash tests, and durable architecture updates are
complete. Retained-tree correction and rejection tests implemented under v2.

## In flight

None.

## Remaining

None within the feature scope.

## Verification

`cargo test --manifest-path app/src-tauri/Cargo.toml library_mutation -- --nocapture`:
4 passed (43.1 s including compilation), `.codex/logs/command-20261103-171404.log`.
Integrated `library_`: 31 passed / 1 ignored; log `command-20261103-171842.log`.
New actual-process `library_mutation`: 9 passed; log `command-20261103-172119.log`.
Pre-correction full backend: 182 passed / 10 ignored; log `command-20261103-172240.log`.
Frontend check/build passed; build log `command-20261103-172120.log`. Focused
library/history frontend: 54 passed; log `command-20261103-172238.log`.
First integration compile found the authored-record caller still expecting Path;
fixed to use the retained canonical root before these passes (failed log 171735).
Pre-correction backend passes above are historical; final backend rerun below.
Post-correction `library_mutation`: 10 passed in 1.69 s test time (22.3 s with
compilation), log `command-20261103-172716.log`. Covers manual and retention
preflight rejection for junctions, locked members, depth/4,096-entry limits,
preservation of all content and handle release.
Final `cargo test --manifest-path app/src-tauri/Cargo.toml`: 184 passed, 10
existing opt-in cases ignored, log `command-20261103-172922.log` (26.63 s tests).
Includes exact retained descendant handles, all mutation routes before/after
admission, export/root/settings regressions and process cases. Final `cargo fmt
--manifest-path app/src-tauri/Cargo.toml -- --check` and `cargo clippy
--manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings` passed;
Clippy log `command-20261103-173007.log`. A temporary Windows handle probe also
observed ordinary ancestor rename denied (Win32 error 5) while the descendant
library handle was retained. All destructive scenarios used generated fixtures.
No hardware, live League or whole-app performance validation claimed.

## Deviations

Implementation review found streaming removal could delete earlier members before
rejecting a later reparse/unsupported/inaccessible member. Stop that unit and
supersede the depth-only traversal with a bounded retained-tree preflight design.
The original immutable plan is preserved. Finalized v2 changes the handle budget
to 4,096 tree entries plus fixed overhead; the correction is implemented/tested.

## Decisions

Both user-requested fixes share one approved design and coordinated implementation.
Never release a target guard and then delete by pathname. Parent handles do not
freeze child names. Remove via the retained DELETE-capable target handle.

## Blockers

None.

## Next action

None; required checks passed and concise completion evidence is in the feature list.
