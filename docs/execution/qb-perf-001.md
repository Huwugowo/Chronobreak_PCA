# QB-PERF-001 execution checkpoint

Feature: `QB-PERF-001`
ExecPlan: `docs/exec-plans/completed/qb-perf-001-capture-benchmark-budget.md`
Updated: 2026-08-27

## Current milestone

Closed on 2026-08-12 by explicit product decision. This completed checkpoint preserves detailed evidence that no longer belongs in the concise feature-list completion claim.

## Active unit

None; the feature is complete.

## Completed

- Implemented the isolated PowerShell collector/finalizer, deterministic Python analyzer, durable capture benchmark protocol, sanitized reports, and ignored raw-result layout.
- Completed all eight Ryzen 5 5600X/RTX 4060 NVENC runs and media/resource finalization.
- Preserved the matrix as formally invalid diagnostic evidence because of focus contamination and telemetry gaps.
- Added `QB-PERF-002` as the planned optimization disposition and retained AMD/Intel validation under `QB-PERF-003`/`QB-PERF-004`.

## In flight

None.

## Remaining

None for QB-PERF-001. A clean pre-change rerun was explicitly waived for this feature only. QB-PERF-002 still owns a valid post-change matrix and every unchanged performance gate.

## Verification

- `python -m unittest discover -s tools/capture_benchmark/tests -v` passed 57 tests in 55.521 seconds at closure.
- Python compilation, PowerShell parsing, analyzer CLI/missing-PresentMon preflight, deterministic/sanitized report fixtures, and operator preflight with PresentMon 2.5.1 passed.
- All four 1920x1080 60-FPS HEVC capture files passed ffprobe, full decode, output progress, bounded-memory, process-liveness, encoder-error, and poller-error checks.
- Applicable recorder/Tauri/frontend checks, recorder release build, feature-list schema/invariants/dependency checks, linked-plan existence, stale-reference inspection, and `git diff --check` passed at closure.
- Accepted diagnostic artifacts remain under `build/perf/qb-perf-001-20260812-032909`, including the invalid-labeled JSON/Markdown report and eight raw run bundles.

## Deviations

The completed matrix could not satisfy formal validity: three runs contained five-to-six-second telemetry gaps and several capped arms had focus contamination caused by necessary single-display Alt-Tab checks.

## Decisions

The product owner accepted the complete invalid matrix as sufficient diagnostic pre-change characterization and waived only a clean QB-PERF-001 rerun. The report remains `INVALID`; it proves neither negligible impact nor AMD/Intel support and does not weaken QB-PERF-002.

## Blockers

None; the explicit acceptance decision closed this feature while preserving its limitations.

## Next action

None for QB-PERF-001. Use QB-PERF-002 for the required valid post-change proof.
