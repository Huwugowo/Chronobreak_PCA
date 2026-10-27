# QB-DEV-007 execution checkpoint

Feature: `QB-DEV-007`
ExecPlan: `docs/exec-plans/qb-dev-007-viewer-benchmark-boundary.md`
Updated: 2026-10-27

## Current milestone

M1 complete; M2 — implement the bounded Viewer runner and shared production operations.

## Active unit

Extract benchmark action/evidence/lifetime policy into `benchmarkViewer.ts`; bind live production capabilities and narrow render/resize subjects. Preserve the 14 frozen Viewer traces unchanged.

## Completed

- Mandatory product/workflow/feature/plan-contract/verification bootstrap, QB-DEV-004 checkpoint then immutable v2 plan, and applicable current replay architecture reads.
- Two read-only scouts mapped remaining Viewer execution/ownership and measurement/test contracts. Root inspected critical readiness, synchronous seek/event, action, render/resize and terminal/cleanup paths.
- Independent plan review found two required clarifications: preserve existing secondary clip-preview media under production ownership, and specify the exact render subject/load/source fields and helper-owned action-entry identity comparisons. Both are incorporated. Root additionally made timer arming, successful endpoint cleanup, intentional media-release eligibility and opening workload IDs explicit; no code changed.
- Final immutable plan defines one harness-owned Viewer runner, bounded production host/shared seek primitive, real-source observation notifications, narrowed resize/render subjects and unchanged action/evidence/release/terminal contracts.

## In flight

None. M2 source edits are next.

## Remaining

M2 runner extraction; M3 automated/native/ordinary UI verification, boundary review, architecture and completion evidence.

## Verification

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

Implement M2 from the immutable design. First factor the shared production seek/clip operations, then runner policy and owner binding/narrow helper subjects; run the frozen focused gate without snapshot updates.
