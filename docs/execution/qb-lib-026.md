# QB-LIB-026 execution checkpoint

Feature: `QB-LIB-026`
ExecPlan: `docs/exec-plans/qb-lib-025-guarded-library-mutations-v2.md`
Updated: 2026-11-03

## Current milestone

All milestones complete: shared mutation arbitration and final regressions verified.

## Active unit

None.

## Completed

Mandatory bootstrap and bounded exploration completed. Startup retention is an
additional caller outside coordinator admission and must acquire the same lease.
Plan review found no blocking defect; added explicit same-process contention
verification. Shared plan/checkpoints linked; canonical validation passed
(111 items / 30 plan-checkpoint pairs).
Four focused tests passed: distinct handles in one process and aliases contend;
different roots progress; exact-handle removal succeeds and root authority rejects
replacement. No additional in-process lease registry is needed on Windows.
All mutation routes, including startup retention, integrated with the scoped lease.
Actual-process tests cover both orderings of save versus manual/retention deletion,
stale pre-lock eligibility, alias contention, independent-root progress and killed
holder release. Caller cancellation keeps lease ownership until the worker exits.

## In flight

None.

## Remaining

None within the feature scope.

## Verification

Focused `library_mutation` command: 4 passed; log
`.codex/logs/command-20261103-171404.log`. Integrated library: 31 passed / 1 ignored
(log 171842); subprocess filter: 9 passed (172119); full pre-correction backend:
182 passed / 10 ignored (172240). Frontend check/build and 54 library/history tests
passed (build 172120, tests 172238). All logs under `.codex/logs/`, 2026-11-03.
Post-correction `library_mutation`: 10 passed (1.69 s tests), log 172716.
Final backend suite: 184 passed / 10 existing opt-in ignored, log 172922.
Backend formatting and strict all-target Clippy passed; log 173007 for Clippy.
Frontend results remain applicable because the correction is native-only.
These tests establish bounded fixture contention and crash/cancellation ownership;
they do not establish negligible whole-app or capture overhead.

## Deviations

QB-LIB-025 required a retained-tree preflight correction. It is implemented and
focused-tested; finalized v2 preserves this feature's lease/eligibility contracts.

## Decisions

One nonblocking kernel-held lease per guarded canonical library, independent of
export staging. Eligibility is read under the lease, which survives dropped async
waiters. No recorder-wide serialization or negligible-overhead claim is added.

## Blockers

None.

## Next action

None; required checks passed and concise completion evidence is in the feature list.
