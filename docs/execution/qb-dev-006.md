# QB-DEV-006 execution checkpoint

Feature: `QB-DEV-006`
ExecPlan: `docs/exec-plans/qb-dev-006-first-use-export-validation.md`
Updated: 2026-10-27

## Current milestone

Complete - first-use and subsequent native exports pass with strict output/media/source/shutdown validation.

## Active unit

None. Implementation, regressions, review and required verification are complete.

## Completed

Mandatory bootstrap, bounded staging/runner exploration and reviewed planning handoff. Review corrections: exact ordinal names, exclusive read-only lease access, failure-closed classification, runner-only diagnostic scope and recursive preexisting preservation assertions. Canonical validation passed: 96 items / 19 plan-checkpoint pairs.

## In flight

None.

## Remaining

None for this feature.

## Verification

2026-10-27: actual PowerShell validator red reproduced the two original first-use directory errors; 1 failure / 3 passing test groups. After the runner correction all 4 groups passed (24 scenario invocations), including strict negative shapes/output errors and recursive preexisting staging byte preservation. Logs: `build/verification/qb-dev-006/validator-{red,green}.log`. The first backend launch was blocked before Cargo by the automatic logger denying `.codex/logs` writes; direct Cargo retry passed. The initial preparation/matrix commands rejected relative paths or a missing plan parent before launch; absolute-path commands with the explicit parent succeeded. Historical failed native evidence remains unchanged.

- Full replay tooling/analyzer suite passed: 132 discovered / 1 existing packaged-runtime-dependent skip, 64.102 seconds (`build/verification/qb-dev-006/replay-suite.log`). Focused unchanged backend staging suite passed: 6 tests (`backend-staging-retry.log`). The opt-in packaged collision/benchmark-isolation fixture also passed with process-local runtime/policy selectors restored afterward: 1 test, generated temporary media, collision bytes/source hashes and retained earlier batch preserved (`backend-preservation.log`).
- Fresh read-only code review found no actionable runner/test defect. Focused regression repeat passed 4/4; canonical red evidence is recorded above. Only runner classification and tests change executable/tool behavior; no production Rust/frontend changes.

- Fresh native verification root: `build/perf/qbd6/.chronobreak-replay-benchmark`. Preparation copied both checked-in generated preproduction bundles, fully decoded them and matched source hashes; no cleanup. Explicit source spec/cache/runtime/binary receipt: `inputs/initial-condition.json`; immutable binary SHA-256 `f59ce61d5fdd18752334950280ae28fed0076577fc5146b785da7a834eefb4c1`. This preserved production benchmark executable was not rebuilt; no executable inputs change in QB-DEV-006.
- Preparation command: `powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <root>/inputs/prepare.json -Manifest <root>/manifests/base.json -MediaRuntimeRoot <repo>/build/perf/qb-dev-004/implementation-20261026/executables/candidate-benchmark/resources/media-runtime`, using absolute paths. Each matrix used `python tools/replay_benchmark/matrix.py plan --template <root>/manifests/base.json --spec <root>/inputs/export-{first-r2,next}.json`; preflight/run used `powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <absolute-launch-path>` (plus `-PreflightOnly` for preflight).
- First restricted-desktop attempt `results/d6-first-001-export-representative-01-full` reached backend startup but timed out before frontend session initialization at the unchanged 30-second gate. Runner terminated its owned Job; no export/staging occurred. It remains failed. Fresh native retry with permitted desktop process access passed: `results/d6-first-r2-001-export-representative-01-full`; pre-launch staging absence was asserted. Validation has preexisting count 0, exactly two media entries, recognized exclusive empty lease residue, zero errors, fully decoded 5000 ms H.264/AAC output. MP4 SHA-256 `5869ecae0082822de7c34e61fd097fb000d86199288747b639734eea3a8e6df0`.
- Subsequent real export `results/d6-next-001-export-representative-01-full` passed with three preexisting direct entries and no new staging residue. Both runner/analyzer reports are valid; source hashes unchanged, zero required event/request loss, app exit 0, no forced termination/timeout and Job active processes 0. `matrix.py verify --plan <root>/plans/{d6-first-r2,d6-next}/matrix-plan.json --require-results` passed for both matrices. `inputs/after-first.json` and `inputs/final-condition.json` prove all first-export MP4/JPEG/lease bytes and hashes unchanged after the second run. Native logs are under `build/verification/qb-dev-006/native-*.log`; failed results were never overwritten.
- Final canonical schema/invariant/linked-artifact validation and `git diff --check` passed. Rust/frontend/recorder full suites and new desktop builds were not run: production code is unchanged. No statistical performance claim or campaign is made.

## Deviations

No design deviation. The initial desktop-restricted attempt failed before export; fresh permitted native runs supply the required evidence. An existing opt-in production collision/isolation fixture supplements focused staging tests.

## Decisions

Preserve production and preexisting staging behavior. Recognize exact new idle residue only; it is shape evidence, not durable parent provenance. Keep analyzer output-entry semantics unchanged.

## Blockers

None.

## Next action

No further work. Preserve the immutable plan, strict runner classification and failed/successful evidence. Viewer benchmark separation is an independent planning task.
