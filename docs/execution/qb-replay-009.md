# QB-REPLAY-009 execution checkpoint

Feature: `QB-REPLAY-009`
ExecPlan: `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v2.md`
Updated: 2026-09-08

## Current milestone

M3 - Integrate the tested controller at 1x, then bounded decoder diagnostics.
The bounded M1 reference is captured, with observation gaps explicitly retained
below. The feature remains ready / in-progress.

## Active unit

Integrate `PlaybackSurface` and existing benchmark actions through controller
commands/snapshots/events; retain clip/layout policy and the persistent video node.
Remove the superseded scheduler/listeners/media mutations. Project presented ticks
to match state and requested ticks to the visual cursor. Native diagnostics follow
the established/tested M2 boundary.

No native diagnostic adapter, benchmark contract upgrade, corpus build, or broad
matrix is a prerequisite to M2. Reuse current measurement seams or direct
documented observations. Preserve the existing dirty tree.

## Completed

- M2: concrete controller + HTML adapter, typed snapshots/outcomes, exact seek facts,
  one-active-plus-latest scheduling, distinct deadlines, generation/intent callback
  invalidation, bounded nudge/recovery, activation-sensitive play, basic rate/audio
  preferences, loops and cleanup. Adapter conversion covers rational frames and
  nonzero video PTS origin. Viewer remained unchanged through this test gate.
- M2 read-only review found and root fixed rejected-nudge playback and mislabelled
  open/native failures. Regression tests pass. Recovery throttle/target restoration
  was corrected during the initial test pass. No architectural change.

- 2026-09-08 M1: five bounded existing-runner production cases completed with
  fixture post-hashes preserved and analyzer acceptance: play_pause r3, seek r1,
  scrub r1, layout r1, lifecycle r1. Raw roots are the fixture sentinel's
  `results/qb-replay-009-before-<kind>-20260908-rN`; concise raw summary is
  `build/perf/qb-replay-009/before-summary.json`. Existing benchmark infrastructure
  and the immutable plan are unchanged.

- Original planning read the full mandatory contracts and relevant architecture,
  confirmed the three dependencies done, and completed bounded frontend/native
  exploration and plan review. No product implementation was made.
- The original design established exclusive primary-media ownership, distinct
  timing facts/generations, bounded scheduling/recovery, truthful failed-rate
  semantics, per-player decoder association, and the QB-REPLAY-015 boundary.
  Those architectural decisions remain in v2.
- 2026-09-07 proportionality review found that original M1/M5 and canonical
  verification required another broad benchmark campaign before controller work.
  The user explicitly requested correcting that scope before implementation.
- Created a self-contained versioned replacement at the linked v2 path. Preserved
  `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder.md` as immutable
  history and repointed this checkpoint and the canonical feature to v2.
- Replaced full baseline/corpus/observer/schema work with a small controller suite,
  material-result escalation, and an explicit stopping rule. Clarified canonical
  8x acceptance: visible bounded limitation handling is distinct from 8x success.
- Final read-only review of the v2 feature, plan, and checkpoint found no material
  scope, architectural-preservation, or consistency issue. Canonical validation
  and preservation checks passed; the corrected planning handoff is complete.

## In flight

Mandatory implementation bootstrap completed, including the v2 plan, verification,
applicable architecture, and forward-engineering skill. Read-only scouts are
complete. Initial working tree was clean. M2 passes its focused test/typecheck
gate; viewer integration is now active. `viewerSeekState` retains the last
dispatched fact so `seeked` can be recorded after RVFC. Rate watchdog belongs to
M4 and is absent. Integration helper is ignored at
`build/perf/qb-replay-009/integrate_viewer.py`; root owns all product edits.

M3 viewer integration is applied and passes TypeScript plus 53 frontend tests.
All primary source/currentTime/rate/play/pause/quality accesses now live behind the
controller; primary element ref is relinquished after adapter creation. The
production 1x seek case completed five seeks without media/recovery failures, but
analyzer rejected the omitted `authoritative` field on first-frame events.
That existing payload field and ready_state are restored; the raw run remains
failed integration evidence at `results/qb-replay-009-m3-seek-20260908-r1`.

Bounded Windows CDP implementation now compiles in `playback_diagnostics.rs` and
`playback_diagnostics/native.rs`, using direct pinned Windows/WebView2 imports,
WebView-thread COM ownership, a 64KiB bounded pending queue, exact token/URL
association, DOM marker cross-check, and hardware/software/unknown parser states.
Four focused Rust diagnostic tests passed. Frontend bridge is wired; production
decoder acquisition and native subscription/resource cleanup are not yet verified.

M1 uses existing strict-v2 fixture `native-current-v2`, game `1787904000`, under
`build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark/library/`.
Both JSON files explicitly declare schema 2; video is validated H.264 High,
1920x1080, yuv420p, exact 60 FPS, AAC, 240 seconds. Do not use the older v1 corpus.
Fresh current production reference is preserved with packaged resources at
`build/perf/qb-replay-009/before.exe` (SHA256
`7cb68471276bee2acbb1f93fe5c3638fc79666e4822ffeaaff632cddd779845a`),
source HEAD `96bb7b6802138d0afee7863a15ae2ce509fd450d`, with only canonical 009
state changed before building. Existing-schema manifests for play_pause (5s warmup,
60s measured), seek, scrub, layout, and five lifecycle cycles are in the fixture
sentinel's `manifests/qb-replay-009/`; result IDs end `20260908-r1`.
First runner launch failed startup: the ordinary production build has the
`replay-benchmark` feature disabled, so it returns no benchmark session and ignores
benchmark manifest startup. The packaged runtime was resolved correctly. This is
a build-selection error, not playback evidence. Rebuilding with the existing
`npm.cmd run desktop:build:benchmark --prefix app` release script; use a fresh r2
manifest/result for the failed play_pause case and preserve r1 artifacts.

Benchmark release build passed and now occupies `before.exe` (SHA256
`846f585f0f35386832ccf55bb4a7fdcd03d5e13f80ef47b39506fe86c6833d0c`);
ordinary release was retained as `ordinary-before.exe`. The r2 launch failed
before startup because Windows PowerShell's UTF8 writer added a BOM that Rust's
JSON reader rejects. All fresh M1 manifests now use UTF-8 without BOM. The r3
play_pause run passed; r1/r2 failures are preserved and not playback evidence.

## Remaining

- M1 observation gaps retained for completion: the existing layout action exercises
  40 transitions and endpoint mode but no sustained short loop. The runner's
  isolated lifecycle/layout runs do not supply the specified two equal combined
  blocks with 30s settling. Controlled fault and terminal checks, purely paused
  seek, audible behavior, and direct persistent-node observation remain unobserved.
  These are not passes. The preserved before binary remains available for the
  smallest focused substitute; do not expand benchmark infrastructure.
- M3: integrate the persistent viewer at 1x, route existing benchmark media actions
  through the controller, and add bounded product decoder diagnostics.
- M4: add shared rate/audio controls and truthful observed-rate/8x fallback behavior.
- M5: applicable automated checks, targeted production after cases and focused
  resource comparison, relevant durable documentation, and completion evidence.
  Escalate only material findings that could change implementation/architecture;
  stop when the contract and targeted verification support completion.

## Verification

- M2: full frontend suite passed (50 tests) before the final three failure tests;
  latest focused command `npm.cmd run test --prefix app -- playbackController.test.ts
  htmlVideoPlaybackAdapter.test.ts viewerSeekState.test.ts` passed 25 tests in 3
  files. `npm.cmd run check --prefix app` passed after final M2 fixes. These are
  deterministic lifecycle results, not actual codec/rate/decoder claims.

| M1 case | Actual before observation | Result / limit |
| --- | --- | --- |
| Ordinary 1x | 5s warmup + 60s playback; no seeks/errors/recoveries; 1 range request; 1201/3938 dropped/total frames | Passed execution; nonzero drops are a comparison reference, not an absolute performance pass |
| Directed seeks | 5 requested/dispatched/seeked/presented; 72.6-162.9ms request-to-RVFC | Passed; playing case |
| Rapid scrub | 40 intents, 14 native dispatches, 26 pending replacements, 6 non-superseded presented observations; final burst settled | Passed; no errors/recovery |
| Layout/endpoint mode | 40 fullscreen transitions, 5 seeks presented in 15.1-147.8ms; no errors/recovery | Passed execution; actual sustained loop not observed |
| Disposal | Existing lifecycle action completed exactly 5 viewer cycles; clean exit, telemetry quiesced, hashes unchanged | Passed execution; combined settled resource blocks still required |

Runtime/device: WebView2 `152.0.4191.66`, Windows 10 build 19045, Dell G15 5511,
Intel i7-11800H, Intel UHD / NVIDIA RTX 3050 Ti Laptop. GPU CIM remains disabled as
accepted; this is not a decoder-path observation. Process samples are retained in
each raw root. Do not interpret startup-to-last memory growth as a leak test.

- 2026-09-08 M1: `npm.cmd run desktop:build --prefix app` passed, including
  frontend typecheck/build and release Tauri build. Plain `npm` was blocked by
  local PowerShell script policy; the installed `npm.cmd` launcher works.
- 2026-09-08 pre-extraction frontend baseline: `npm.cmd run test --prefix app`
  passed (6 files, 31 tests).
- M1 runner preflight passed for `play_pause-before.json` after placing the
  manifest inside its required sentinel root. Initial out-of-sentinel manifest
  failed `PATH_ESCAPE` before any run was created; the existing path validation
  was preserved. No benchmark tooling or schema changed.
- M1 initial runner launch failed `APP_STARTUP_TIMEOUT` after 30s, with no
  frontend observations. Required feature-flag cause is identified above; no
  decoder or controller conclusion is drawn from this invalid run.

- Original 2026-09-07 planning: the canonical Python validator from
  `docs/development/VERIFICATION.md` passed for 60 roadmap items and six
  plan/checkpoint pairs with the original linkage. Its original feature diff and
  Markdown whitespace checks passed. These historical planning checks do not
  validate the replacement linkage.
- 2026-09-07 v2: executed the exact canonical Python validator extracted from
  `docs/development/VERIFICATION.md`; passed for 60 roadmap items and six
  plan/checkpoint pairs. QB-REPLAY-009 remains ready / not-started with done
  dependencies, and feature/checkpoint linkage resolves to the same v2 plan.
- 2026-09-07 v2: required plan sections, UTF-8/final-newline/trailing-whitespace
  checks, and `git diff --check` scoped to the three changed files passed.
  SHA256 verification confirmed the original plan unchanged
  (`1ba6425dd7f02a622d74082de3db3138b5bab09e3fb70fa0a91c0861435cc0e1`).
  A session-start file-hash comparison and roadmap comparison confirmed only the
  new v2 plan, this checkpoint, and the 009 feature entry changed; unrelated
  working-tree changes and all other feature entries were preserved.
- Product tests, production build, actual decoder identification, playback/rate
  measurements, and manual audiovisual checks: not run in either planning pass.
  No new playback, performance, or hardware-decoder success is claimed.

## Deviations

The user-requested pre-implementation correction supersedes the broad verification
design. Fresh full matrices, corpus preparation, observer pairs, benchmark
schema/event/report changes, and formal comparison coverage are removed from
default requirements. This is an explicit planning requirement revision, not a
relaxation after implementation failure.

The original plan remains unchanged. There are no implementation deviations
because implementation has not started.

## Decisions

- Accepted QB-REPLAY-008 and QB-REPLAY-012 evidence is authoritative input. Do not
  reopen predecessor plans/reports or reproduce their proof by default. Use
  current exact replay-time and seek-state helpers.
- The 008 ten-minute torture finding remains accepted; its short layout resource
  flag informs only the focused transition/cleanup check, not a proven leak.
- Record shared before/after behavior on existing fixtures; verify new UI,
  recovery outcomes, and decoder diagnostics after integration. Do not retrofit
  diagnostics into the old viewer to manufacture a matched observer baseline.
- An 8x selection with lower working fallback is an explicit runtime limitation,
  never successful 8x. Retain selected/applied/observed distinctions, bounded
  recovery, no automatic reapplication of failed 8x, and no seek-based emulation.
- Hardware decoder proof is per-player evidence, not generic GPU utilization.
  Unknown/software outcomes stay explicit. Missing evidence needed for the
  decoder-path requirement needs focused resolution; perfect telemetry is not
  a separate workstream.
- Stopping rule: once the functional contract and targeted validation show no
  material regression or unresolved architectural uncertainty, complete applicable
  checks/docs and stop. Benchmark/verifier/evidence imperfections are non-blocking
  unless they could invalidate that conclusion. Broader work needs a recorded
  material finding, the decision it could change, and why a narrower check cannot
  resolve it.
- General frame stepping remains QB-REPLAY-015; the clip editor's multi-element
  preview and alternate playback backends remain outside 009.
- Original planning located `forward-engineering` at
  `C:/Users/Hugo/.omp/skills/forward-engineering/SKILL.md`. Apply it for substantial
  implementation and the available Rust skill for Rust changes.
- Preserve existing unrelated product/canonical working-tree changes. This pass
  is limited to the 009 feature entry, v2 plan, and this checkpoint.

## Blockers

None established for the corrected planning handoff. Production behavior and
decoder acquisition remain future focused checks, not assumed passes or
prerequisite benchmark campaigns.

## Next action

Apply and typecheck the viewer integration, preserving existing benchmark field
meanings. Then add/test the bounded decoder diagnostic adapter and run focused
production 1x cases. Retain M1 observation gaps as explicit completion gates.
