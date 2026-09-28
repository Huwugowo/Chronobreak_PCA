# QB-CLIP-010 execution checkpoint

Feature: `QB-CLIP-010`
ExecPlan: `docs/exec-plans/qb-clip-008-owned-export.md`
Updated: 2026-09-27

## Current milestone

Owned export pipeline and verification complete, coordinated with QB-CLIP-008.

## Active unit

None.

## Completed

Design review established one bounded worker is the correct common lifecycle/filesystem boundary.

`export_clip` now moves owned inputs and `MutationCompletion` into `export_process::run_owned`. A captured runtime handle drives the async pipeline on the blocking thread. Existing synchronous filesystem/JSON stages therefore stay off async workers; no new per-stage task fan-out or permits.

## In flight

None.

## Remaining

None.

## Verification

2026-09-27: `export_blocking_worker_retains_admission_and_cleans_before_release` passed on a current-thread runtime. It proves a different worker thread, timer progress during held synchronous work, Busy mutation admission after waiter cancellation, cleanup before release, invalidation and unchanged source fixture bytes. Full app tests, fmt, Clippy, benchmark-feature compilation and generated-media export passed; receipts and exact commands are retained under `build/review-fixes-20260927/` and summarized in QB-CLIP-008's checkpoint.

## Deviations

None.

## Decisions

All synchronous export stages stay on the same owned worker; no filesystem or JSON work under coordinator publication locks.

## Blockers

None.

## Next action

No further action for QB-CLIP-010.
