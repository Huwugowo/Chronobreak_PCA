# QB-DEV-004 execution checkpoint

Feature: `QB-DEV-004`
ExecPlan: `docs/exec-plans/qb-dev-004-app-benchmark-boundary.md`
Updated: 2026-10-26

## Current milestone

M1 — freeze equivalence coverage and capture the native pre-refactor baseline before structural implementation.

## Active unit

Planning handoff; no product-code implementation unit has started. Existing static baseline was captured on clean `main` revision `933857dd74e032c8bf8286c0b7bcd5e4e0f1a669`. Next unit targets `App.benchmark.test.tsx`, `ViewerScreen.test.tsx` and fresh sentinel benchmark launch/result specifications, followed by baseline native runs.

## Completed

- Mandatory product/workflow/feature/verification/architecture bootstrap and forward-engineering skill read.
- Read-only scouts mapped screen policy, Rust/Tauri activation/data isolation, observation contracts, tooling and available generated-fixture metadata.
- Chosen first slice: App-level benchmark coordinator and explicit Viewer-to-App cycle callback; preserve the existing real viewer executor and production controller/player.
- Independent plan review identified creation-before-admission, exact host/identity contracts, the no-refresh cycle transition, callback replacement identity and baseline sequencing gaps. The finalized design incorporates all five corrections; root reviewed the corrected contracts before finalization.
- Documented the current benchmark boundary in `docs/architecture/desktop-replay.md`; that section describes current implementation, not a completed extraction.
- Existing frontend, replay analyzer and benchmark-enabled backend baselines captured before any product-code changes. Detailed results below.

## In flight

None. No structural refactor, new equivalence tests, native launch specification or native benchmark campaign has started.

## Remaining

- Add pre-refactor contract coverage for App scenario dispatch/cycles/cancellation and callback integration, without changing workload expectations.
- Capture current-tree normal desktop workflows and the native baseline/observer-control evidence required by the ExecPlan before M2. Rebuild and retain immutable executables; historical artifacts do not count.
- Implement the App coordinator/capability boundary and replace the window cycle event with the optional synchronous callback.
- Run post-change focused/full checks, paired native evidence, boundary review, durable architecture update and canonical completion validation.
- Preserve/report viewer executor, production telemetry/search, historical duration compatibility and frontend-bundle coupling as remaining migration scope.

## Verification

Actual 2026-10-26 results; raw logs are ignored under `build/perf/qb-dev-004/baseline-20261026/`.

- `npm run test --prefix app`: passed, 38 files / 378 tests (`frontend-tests.log`).
- `npm run build --prefix app`: TypeScript and production Vite build passed (`frontend-build.log`). Existing ineffective dynamic import warning: `api.ts` is also statically imported. Output JS 327.20 kB, gzip 103.72 kB; artifact size alone proves no runtime performance claim.
- `python -m unittest discover -s tools/replay_benchmark/tests -v`: passed, 128 tests discovered, 1 skipped (`analyzer-tests.log`). The skipped repeat/AAC fixture requires an explicitly configured packaged media runtime; it was not run.
- `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check`: passed (`tauri-fmt.log`).
- Initial `cargo test --manifest-path app/src-tauri/Cargo.toml --features replay-benchmark`: 161 passed / 1 failed / 9 ignored (`tauri-benchmark-tests.log`); `clip_duration::tests::actual_child_cancellation_after_ready_kills_and_reaps` failed its child-ready assertion and failed alone as well (`tauri-cancellation-focused.log`). A minimal child reproduction proved Windows PowerShell rejected the fixture `.ps1` because script execution was disabled (`cancellation-child-repro.log`).
- Process-local environment `PSExecutionPolicyPreference=Bypass`, restored afterward, permits that existing fixture. No repository code, test threshold or machine/user policy was changed. The isolated test passed and the complete benchmark-enabled suite then passed: 162 passed / 9 ignored plus binary/doc tests (`tauri-cancellation-process-policy.log`, `tauri-benchmark-tests-process-policy.log`).
- Normal `cargo test --manifest-path app/src-tauri/Cargo.toml` with the same process-local fixture policy: failed, 147 passed / 1 failed / 9 ignored (`tauri-default-tests-process-policy.log`). `export_staging::tests::invalid_ownership_foreign_entries_and_locked_files_are_retained` failed `report.retained >= 2` at `export_staging_tests.rs:102`. The exact isolated test passed (`tauri-export-staging-focused.log`); the full-suite failure remains part of the baseline, with cause unresolved. Do not claim the default suite passed or change export code under this refactor.
- Clippy, desktop executable builds, actual ordinary desktop workflows, native benchmark/control trials, performance equivalence and new contract tests: not run in the planning pass.
- Final planning handoff: canonical validator passed for 95 roadmap items / 18 plan-checkpoint pairs; `git diff --check` passed. These are artifact/state checks, not implementation acceptance.

## Deviations

None to the implementation design. Repository workflow requires finalizing a planned design/checkpoint and stopping the planning pass before product implementation. This handoff therefore does not claim the requested refactor is implemented.

## Decisions

- Move App policy/resources first; extracting the viewer executor simultaneously would combine two lifecycle migrations.
- Keep bounded production instrumentation and Rust feature/session/sentinel guards intact. Frontend benchmark code still ships in the normal bundle; packaging is a later measured decision.
- Preserve the historical cycle return's actual Games navigation transition without adding `openTab`'s extra refresh. Document its measurement scope rather than claiming identical user-click workflow coverage.
- Replace only the global cycle control event, retaining synchronous post-release notification and existing required telemetry/terminal order.
- Native pre-refactor evidence is a prerequisite for structural changes, not deferred until after candidate code overwrites the baseline executable.

## Blockers

No design blocker. Native/performance equivalence evidence has not been collected. The default Rust full-suite export-staging failure is unresolved baseline evidence; its isolated pass does not establish a green full suite. This must be characterized/dispositioned before final completion, without opportunistically changing export behavior.

## Next action

Begin M1: before source/test edits, build and retain the baseline production and benchmark executables and prepare fresh sentinel-owned native baseline launches using explicitly hash-checked generated fixtures. Record the planning-only dirty paths and executable-input/executable hashes, and capture native trials. Then add ordered App scenario/cycle/cancellation contract coverage against unchanged production inputs. Do not start the coordinator extraction until those pre-change checks and baseline captures are retained. Set this checkpoint's Active unit before starting that work.
