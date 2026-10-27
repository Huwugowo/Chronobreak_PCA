# QB-DEV-007 execution checkpoint

Feature: `QB-DEV-007`
ExecPlan: `docs/exec-plans/qb-dev-007-viewer-benchmark-boundary.md`
Updated: 2026-10-27

## Current milestone

M1 and M2 complete; M3 — verify the completed tree and fresh native candidate.

## Active unit

Run automated gates, build/archive the candidate, then repeat the frozen native workflows and ordinary desktop procedure. The 14 frozen Viewer traces remain unchanged.

## Completed

- Mandatory product/workflow/feature/plan-contract/verification bootstrap, QB-DEV-004 checkpoint then immutable v2 plan, and applicable current replay architecture reads.
- Two read-only scouts mapped remaining Viewer execution/ownership and measurement/test contracts. Root inspected critical readiness, synchronous seek/event, action, render/resize and terminal/cleanup paths.
- Independent plan review found two required clarifications: preserve existing secondary clip-preview media under production ownership, and specify the exact render subject/load/source fields and helper-owned action-entry identity comparisons. Both are incorporated. Root additionally made timer arming, successful endpoint cleanup, intentional media-release eligibility and opening workload IDs explicit; no code changed.
- Final immutable plan defines one harness-owned Viewer runner, bounded production host/shared seek primitive, real-source observation notifications, narrowed resize/render subjects and unchanged action/evidence/release/terminal contracts.

## In flight

- Frontend full suite/typecheck/build and Python harness tests passed; native/Rust checks are running. Candidate builds are next.
- M2 implementation and final focused gate are validated; native candidate results remain outstanding.

## Remaining

M3 native/Rust checks and desktop builds; candidate workflow/ordinary UI evidence; architecture and canonical completion evidence.

## Verification

- M2: runner owns action/scenario/readiness/evidence/timer/RAF/abort/terminal policy; Viewer binds live typed capabilities and narrow render/resize facts before controller open. Ordinary and harness seek paths share the same current-loop/controller/requested-preview primitive. Final focused eight-file gate passed 223 tests, all 14 M1 snapshots unchanged; TypeScript passed. Direct shared-seek integration subsequently verified identical loop-before-seek and live preview with one UI ID allocation (initial test counted the mount's initial clip seek; counter reset at the operation boundary).
- New cancellation regression reproduced playback starting after abort during native geometry/listener admission (two failing tests). Added boundary/identity checks; both pass while native drain/restoration stays intact. Runner tests cover pending failure claim/original reason during release disposal, stale observer/mount, all async action boundaries, intentional disposed readiness, helper publication suppression, finite timer/RAF cleanup. Render tests cover real identity/source/participant/fixed-tick admission and cancellation during decode without late RAF allocation.
- Read-only boundary review accepted failure/cancellation fixes and flagged a possible partial-roster acceptance. Root checked the complete helper path: `renderedReplayImages` already rejects `players.length !== 10` before admitted/ready emission. Added an explicit sample-count=10/projected-count=9 regression instead of duplicating that existing condition or weakening it. No source change required.
- M3 Python harness suite passed 132 tests with one environment-dependent skip (`m3-python-tests.log`); no analyzer/threshold changes.
- M3 frontend full suite passed 526 tests (40 files), typecheck and production build passed (`m3-frontend-*.log`); subsequent one-case render regression is included in the final focused gate. Existing Vite dynamic-import warning retained.


- M1 baseline before source/test edits: all eight required native runs passed runner, analyzer and matrix verification (`qbd7b/.../plans/b7`); idle conditioning passed, both index rows `ready,facts_current=1`. Added settled-memory passed with exactly six Viewer cycles and seven quiet library windows (`plans/b7mem`). All runs have preserved source hashes, zero required loss, clean finite shutdown. Logs retained under `build/verification/qb-dev-007/m1-*.log`; immutable raw results under the corresponding sentinel `results/`. A representative run supports exercised behavior, not statistical performance equivalence.
- Pre-extraction real-builder Viewer contracts: 14 frozen snapshots (every action branch, 334-action whole-app, default five-cycle layout/rate ladder, explicit event/endpoint/paused seeks, opening/lifecycle variants); synchronous evidence is emitted inside the test controller seek. Existing real controller/media integration remains. Focused seven-file suite passed 166 tests and TypeScript passed. Snapshots generated against the unmodified executor, then independently rerun without update. Initial test mock type errors were corrected; no production changes.

- Current normal and benchmark Tauri CLI builds passed; archived executable/resources and SHA-256 manifests are at `build/perf/qbd7-freeze/baseline-{normal,benchmark}`. Benchmark executable SHA-256 is `1faba12d5c81c3c5b4759fb75b309f281580ab63be14585fe5159d3172beee51`. Input hashes (307 tracked executable/tool inputs), revision/dirty paths and tool versions are beside them. Build logs: `build/verification/qb-dev-007/m1-build-{normal,benchmark}.log`. Existing Vite dynamic-import and native linker warnings retained.
- Existing focused seven-file frontend gate passed: 152 tests (`m1-focused-before.log`). An initial npm hook was denied writing `.codex/logs`; direct `npm.cmd` with no profile ran successfully.
- Fresh baseline preparation fully decoded both checked-in generated preproduction bundles, verified all source hashes and offline cache; receipt `qbd7b/.../manifests/base.preparation.json`, log `m1-prepare.log`. Native idle preflight passed.
- Frozen real builder action lists/hashes are at `build/perf/qbd7-freeze/action-contracts.json`: whole-app 334 actions, layout 75, default rate 19, all declared seek/control/opening/lifecycle variants. No source/test edits or candidate checks yet. QB-DEV-006 results remain separate evidence.

## Deviations

- Added the plan's conditional settled-memory native workflow because ordinary lifecycle startup-to-peak growth did not establish retained resources. Both roles must keep this added workflow. No source/test/measurement change.

## Decisions

- Keep App coordination and synchronous captured cycle callback unchanged.
- Runner owns experiment policy/resources/evidence waiters; Viewer retains primary controller/data/layout/clip ownership and authoritative emitters. No raw controller/video exposure or new player.
- Native baseline must be captured against current executable inputs before editing source/tests; historical retained binaries are provenance.
- Preserve measurement/filter/timing contracts. Representative native and deterministic cleanup/order checks are mandatory; statistical/control escalation follows explicit observed triggers and remains necessary for numerical claims.

## Blockers

None. Full approved M1–M3 implementation authorized on 2026-10-27.

## Next action

Commit M2 after its final focused check, finish automated native gates/builds, archive candidate binaries, and prepare a new identity-matched sentinel root for native comparison. Do not overwrite baseline results.
