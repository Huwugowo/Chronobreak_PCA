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

- Native/Rust tests, format and both Clippy configurations passed. Both Tauri candidates built/archived. Candidate full-decode/source hashes passed, and `c7idle` conditioning passed runner/analyzer/matrix; both index rows are ready/facts-current. `b7cs` is running its first settled-index minimal/full control campaign (exec session 25604); `c7cs`, `c7` and `c7mem` remain unlaunched.
- Control initial-state audit passed: both roles have identical fixture/offline-cache/schema/version/admitted-recording facts; each role snapshot was independently reproduced from the normal source via SQLite backup, and 16 initial receipt hashes are frozen in `build/perf/qbd7-freeze/control-initial-comparison.json`. Driver checks that binding plus exact tree/file hashes, reparse absence and empty scratch before every preflight, and refuses previously attempted matrices/logs.
- M2 committed as `d7bd788` after 223 focused tests/typecheck. Architecture now describes implemented ownership. Native candidate results and ordinary desktop evidence remain outstanding.

## Remaining

M3 native/Rust checks and desktop builds; candidate workflow/ordinary UI evidence; architecture and canonical completion evidence.

## Verification

- M2: runner owns action/scenario/readiness/evidence/timer/RAF/abort/terminal policy; Viewer binds live typed capabilities and narrow render/resize facts before controller open. Ordinary and harness seek paths share the same current-loop/controller/requested-preview primitive. Final focused eight-file gate passed 223 tests, all 14 M1 snapshots unchanged; TypeScript passed. Direct shared-seek integration subsequently verified identical loop-before-seek and live preview with one UI ID allocation (initial test counted the mount's initial clip seek; counter reset at the operation boundary).
- New cancellation regression reproduced playback starting after abort during native geometry/listener admission (two failing tests). Added boundary/identity checks; both pass while native drain/restoration stays intact. Runner tests cover pending failure claim/original reason during release disposal, stale observer/mount, all async action boundaries, intentional disposed readiness, helper publication suppression, finite timer/RAF cleanup. Render tests cover real identity/source/participant/fixed-tick admission and cancellation during decode without late RAF allocation.
- Read-only boundary review accepted failure/cancellation fixes and flagged a possible partial-roster acceptance. Root checked the complete helper path: `renderedReplayImages` already rejects `players.length !== 10` before admitted/ready emission. Added an explicit sample-count=10/projected-count=9 regression instead of duplicating that existing condition or weakening it. No source change required.
- M3 native automated gates passed: default 148 tests / benchmark 162 tests, each nine explicitly ignored packaged/characterization fixtures; formatting and both all-target Clippy configurations exit 0. Ignored fixtures are not claimed run; no backend source changed. Logs/status: `m3-rust-*.log`, `m3-rust-status.json`.
- M3 normal and benchmark Tauri CLI builds passed with existing Vite/linker warnings (`m3-build-{normal,benchmark}.log`). Archives at `build/perf/qbd7-freeze/candidate-{normal,benchmark}`; normal SHA `d10331277ac7d791a6ab53c634eefd12701af19d6dfd2a1193d7c13d18f07962`, benchmark SHA `11d277332b8b754eccd57420a0cef61e2488a76245cbe9ba646b6ffe21c9a29e`; `candidate-inputs.json` records revision `d7bd788` and 309 executable/tool inputs. No implementation changes after these builds.
- M3 Python harness suite ran 132 tests with the packaged-runtime case initially skipped (`m3-python-tests.log`); that exact case was then run with `QUEUEBACK_TEST_MEDIA_RUNTIME` and passed (`m3-python-runtime.log`). No analyzer/threshold changes.
- M3 frontend full suite passed 526 tests (40 files), typecheck and production build passed (`m3-frontend-*.log`); subsequent one-case render regression is included in the final focused gate. Existing Vite dynamic-import warning retained.


- M1 baseline before source/test edits: all eight required native runs passed runner, analyzer and matrix verification (`qbd7b/.../plans/b7`); idle conditioning passed, both index rows `ready,facts_current=1`. Added settled-memory passed with exactly six Viewer cycles and seven quiet library windows (`plans/b7mem`). All runs have preserved source hashes, zero required loss, clean finite shutdown. Logs retained under `build/verification/qb-dev-007/m1-*.log`; immutable raw results under the corresponding sentinel `results/`. A representative run supports exercised behavior, not statistical performance equivalence.
- Pre-extraction real-builder Viewer contracts: 14 frozen snapshots (every action branch, 334-action whole-app, default five-cycle layout/rate ladder, explicit event/endpoint/paused seeks, opening/lifecycle variants); synchronous evidence is emitted inside the test controller seek. Existing real controller/media integration remains. Focused seven-file suite passed 166 tests and TypeScript passed. Snapshots generated against the unmodified executor, then independently rerun without update. Initial test mock type errors were corrected; no production changes.

- Current normal and benchmark Tauri CLI builds passed; archived executable/resources and SHA-256 manifests are at `build/perf/qbd7-freeze/baseline-{normal,benchmark}`. Benchmark executable SHA-256 is `1faba12d5c81c3c5b4759fb75b309f281580ab63be14585fe5159d3172beee51`. Input hashes (307 tracked executable/tool inputs), revision/dirty paths and tool versions are beside them. Build logs: `build/verification/qb-dev-007/m1-build-{normal,benchmark}.log`. Existing Vite dynamic-import and native linker warnings retained.
- Existing focused seven-file frontend gate passed: 152 tests (`m1-focused-before.log`). An initial npm hook was denied writing `.codex/logs`; direct `npm.cmd` with no profile ran successfully.
- Fresh baseline preparation fully decoded both checked-in generated preproduction bundles, verified all source hashes and offline cache; receipt `qbd7b/.../manifests/base.preparation.json`, log `m1-prepare.log`. Native idle preflight passed.
- Frozen real builder action lists/hashes are at `build/perf/qbd7-freeze/action-contracts.json`: whole-app 334 actions, layout 75, default rate 19, all declared seek/control/opening/lifecycle variants. These were frozen before source/test edits; QB-DEV-006 results remain separate evidence.

## Deviations

- M3 protocol audit found no observer-overhead control for these binary identities. This triggers the approved control escalation: initial `b7ctl`/`c7ctl` specifications (cache-only baseline failure retained), then replacement settled-index `b7cs`/`c7cs` specifications, four alternating adjacent 60-second minimal/full pairs per role on the Swiftplay fixture, paused between deterministic seeks, fresh isolated per-launch app-data/scratch roots and identical offline cache. Original M1 representative traces remain immutable behavioral evidence; they are not a controlled statistical performance baseline. Controls and their unchanged analyzer gates must pass before accepting the remaining M3 evidence.

- Added the plan's conditional settled-memory native workflow because ordinary lifecycle startup-to-peak growth did not establish retained resources. Both roles must keep this added workflow. No source/test/measurement change.

## Decisions

- The plan's normal-settled-index condition applies to observer controls too. Protocol clarification makes this explicit without changing default cache-only conditions, workloads, deadlines or gates. A read-only review accepted the condition and found local receipt gaps; fixed cross-role equality/provenance/schema/singleton identity and overwrite refusal before any replacement launch. Snapshot provenance audit initially hit a Windows SQLite handle cleanup error; explicit connection closing fixed it and the second audit passed. Cache-only failure and first audit log remain retained.

- Keep App coordination and synchronous captured cycle callback unchanged.
- Runner owns experiment policy/resources/evidence waiters; Viewer retains primary controller/data/layout/clip ownership and authoritative emitters. No raw controller/video exposure or new player.
- Native baseline must be captured against current executable inputs before editing source/tests; historical retained binaries are provenance.
- Preserve measurement/filter/timing contracts. Representative native and deterministic cleanup/order checks are mandatory; statistical/control escalation follows explicit observed triggers and remains necessary for numerical claims.

## Blockers

- Ordinary native UI automation is unavailable in this environment: window capture repeatedly timed out (`FrameArrived timed out`, `window capture timed out`); accessibility reads show the isolated two-recording library, but indexed click fails `coordinate input geometry is unavailable` and keyboard input did not move focus. Both owned normal test launches were stopped; fixture root and logs retained at `build/perf/qbd7-ui-candidate`. No open/play/layout/endpoint/Settings check is claimed passed; this mandatory gate needs functioning native automation or recorded operator evidence.
- First baseline observer-control run `b7ctl-001-minimal-p1-minimal` failed at 31 s with `opening readiness/details timed out` on the archived M1 binary. Its cache-only isolated app-data had both recordings `pending,facts_current=1`; versioned opening started 37 seek actions before details admission. No required loss/source mutation, exit 0/owned Job empty. All artifacts are retained. Investigation confirmed the known QB-PERF-005 empty-index condition, not a candidate regression. The approved normal-settled condition is now explicitly documented in the protocol; per-role SQLite backup/ready facts/root/hash receipts and exact per-launch tree verification precede replacement isolated controls. `b7cs` is prepared (snapshot SHA `770c3f838cbac885f8208b738954e65f4b5bd56632c49d29a9147b10d873e00a`); `c7idle` passed, candidate snapshot SHA `a8ed3723a28ebf178f77a5775ea3172fb469c6709f4a169aec192bd8c5e8f61d`. Both replacement matrices are prepared and cross-role state audited (`m3-control-initial-audit-2.log`). No source/timeout/analyzer/workload change.

Full approved M1–M3 implementation authorized on 2026-10-27.

## Next action

Finish running `b7cs` (session 25604), aggregate its minimal/full reports with unchanged observer-control analyzer gates, then run/aggregate `c7cs` and unchanged `c7`/`c7mem` without concurrent CPU-heavy work. Aggregate minimal/full control reports with existing analyzer gates; retain failures. Compare nine candidate workflows with M1 raw/report evidence. Ordinary native UI remains unavailable and must not be claimed passed.
