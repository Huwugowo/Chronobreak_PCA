# QB-PERF-005 — Standalone replay performance

## Purpose

Establish whether the release replay app has a material responsiveness or resource problem during ordinary use, and make only the smallest justified fix. The user explicitly excludes concurrent recording and recorder compatibility. This design supersedes the recorder-coupled whole-app design without repeating accepted standalone measurements.

## Relevant planning-time architecture

The existing `tools/replay_benchmark/prepare.ps1`, `run.ps1`, `matrix.py` and `analyze.py` own fixture integrity, fresh app/WebView roots, bounded Windows Job cleanup and validated reporting. The release app embeds its frontend through `npm run desktop:build:benchmark --prefix app`; actions are feature-gated and use production playback and history controllers. `run.ps1` samples app/WebView process CPU and private/working memory. Its original embedded C# Job helper is sufficient for standalone runs.

The current replay changes add actual search input through the normal 180ms debounce and HistoryController, native window resizing, combined actions and benchmark-only final-seek settlement. Search admission and paint, native geometry and authoritative video presentation remain distinct. Keep generation, timeout and source-integrity checks. Do not change production controller semantics merely to satisfy observations.

The dedicated preproduction library has two complete current-format bundles, populated offline Data Dragon assets, recorded events/rosters, and static synthetic H.264/AAC video. Current preparation/import receipts and accepted standalone search, resize, seek and combined traces can be reused with their exact binary/source identities. Static footage supports UI conclusions but cannot establish representative moving-video decode cost.

## Scope and non-goals

Retain useful replay instrumentation and validated evidence. Remove this task's unfinished recorder control, paired supervisor, new GPU helper and cross-process clock infrastructure, preserving a recoverable copy under ignored `build/perf/` before removal. Restore the embedded Job helper and original line endings while preserving actual replay scenario changes.

Complete one bounded moving-video replay check and one separate cold-index/search diagnostic lasting at most 120 seconds. Publish a concise sanitized report of accepted old and new evidence. Make a minimal production fix only for a material observed problem; otherwise finish with no further optimization justified by these measurements.

No League sessions/FPS claims, Smite changes, recorder pairs, mandatory GPU collector, framework replacement, exhaustive matrix, arbitrary CPU target, speculative HUD/query refactor or unrelated feature work.

## Exploration findings

Existing evidence covers populated assets, actual search/debounce/admission, native resize, rapid seeks and a combined standalone workflow. Repeating those runs with unchanged inputs adds little value. Remaining meaningful uncertainty is moving-video behavior and search/query behavior during initial indexing. One Python tooling check currently fails after the Job helper extraction; restore the intended original ownership mechanism before deciding whether a test needs changing.

## Chosen design and rationale

### Cleanup and retained behavior

Identify current task-owned recorder/helper diffs narrowly. Save their exact patch and untracked sources to a new ignored archive. Remove only additions demonstrated to belong to abandoned QB-PERF-005 work, preserving unrelated changes and raw results. Restore the original embedded C# helper from the baseline only after proving the extracted helper is identical, and preserve new replay scenario support and all unrelated edits while fixing line-ending churn. Remove app QPC anchoring used only by the abandoned supervisor. Run focused tooling checks after cleanup.

### Bounded missing checks

Prefer an existing safely readable moving recording, copied into a dedicated test library. Preserve the original source and complete current telemetry; if a disposable fixture substitutes media, explicitly record that provenance and do not claim event-to-picture alignment. Validate stream/timeline compatibility and changing decoded frames. If no suitable moving footage is available, record that limitation rather than silently calling static footage representative or building a new media-generation system.

Use existing release scenarios and runner. Preserve the populated offline assets, selected-generation render gate, final-seek evidence and bounded cleanup. Run the moving-video combined workload once; accepted samples are descriptive, not an independent-trial distribution. A replacement is justified only by a concrete invalidity with the failed run retained. Report CPU, private/working memory, dropped frames, action latency and available settled-memory observations. Do not infer a leak from a single high-water mark.

For cold indexing, use a fresh disposable cache and actual production import/search. Reuse existing observations where possible, adding only a narrow benchmark-only startup action if necessary to submit real search while indexing is active. Preserve partial-result semantics and distinguish dispatch, coalescence, admission and completed-but-superseded queries. Stop within 120 seconds. If indexing finishes too quickly to overlap, report the observed fast completion and do not inflate the library to manufacture contention.

### Findings and stop condition

Analyze validated existing results without rerunning them. Report exact source/binary conditions, sample counts, failures/exclusions, descriptive limits and unavailable GPU evidence. A production fix requires a reproducible user-visible delay, error, sustained frame-loss pattern or supported retained-growth signal. Validate only the affected dependency cone after a fix. If the bounded checks reveal no material issue, the deliverable is the evidence and that conclusion, not another optimization cycle.

## Rejected alternatives and planning decisions

- Recorder pairing answers a question the user explicitly removed.
- New GPU infrastructure and elaborate process supervision delay the useful replay conclusion.
- Static synthetic footage cannot substitute for moving-video evidence.
- More repetitions without a specific unresolved question do not justify an open-ended benchmark campaign.
- A CPU percentage target alone does not establish a meaningful product problem.

## Milestones

1. Finalize by linking `feature-list.json.execution.plan` and the checkpoint to this exact path, preserving the superseded plan. Archive and remove abandoned recorder/GPU additions, restore the embedded runner helper and line endings, and restore passing replay-tooling checks.
2. Summarize accepted replay evidence; execute only the bounded moving-video and indexing/search checks, retaining any invalid trials and limitations.
3. Apply a minimal fix only if justified, complete applicable verification, and update the report, protocol, checkpoint and canonical feature evidence truthfully.

## Verification design

AC1: existing preparation/source hashes, complete current telemetry and offline-asset receipts plus retained generation/render/decode gates; no original recording writes.

AC2: existing and bounded new action traces establish search, resize, seek/presentation, frame drops and app resource evidence. Report memory retention only where comparable settled samples exist. Run `python -m unittest discover -s tools/replay_benchmark/tests -v`, `npm run test --prefix app`, `npm run check --prefix app`, and `npm run build --prefix app` for retained interfaces.

AC3: all new runs are recorder-inactive. Preserve bounded Job teardown, process attribution and source integrity. GPU observations may be explicitly unavailable. Qualify moving-video and catalog representativeness.

AC4: a sanitized JSON/Markdown report under `docs/performance/evidence/` records validated results, counts, uncertainty, exclusions and dispositions, including the separate at-most-120s indexing diagnostic. No implied cancellation of a running query.

For retained Tauri changes, run ordinary and `replay-benchmark` Rust tests, fmt and applicable all-target Clippy per `docs/development/VERIFICATION.md`. Removing abandoned recorder-only diffs should leave that subsystem unchanged; do not rerun native hardware suites. Build the benchmark release through the Tauri CLI for new live checks and the ordinary release for production parity. Run `python build/validate_features.py` for the final canonical state.

## Performance and reliability gates

Do not weaken missing-phase, source-integrity, generation, timeout, media-error or owned-cleanup checks to accept a trial. Preserve original raw failures separately. Reject claims that exceed fixture/device/runtime/sample support. Existing accepted work remains valid unless an actual changed dependency invalidates it. Whole-feature completion requires the revised feature criteria and applicable checks, not completion of the superseded recorder plan.
