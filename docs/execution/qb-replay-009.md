# QB-REPLAY-009 execution checkpoint

Feature: `QB-REPLAY-009`
ExecPlan: `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v2.md`
Updated: 2026-09-08

## Current milestone

M3 complete - primary playback integration at 1x and bounded decoder diagnostics,
including the four WIP audit corrections and real Windows verification. Stopped
at the requested M3 boundary. M4 has not started; feature remains ready / in-progress.

## Active unit

None. M3 implementation and verification are complete. Do not begin M4 in this
session. Unrelated skill and `.codex/` working-tree changes are preserved.

## Completed

- M3 audit correction on `qb-replay-009-wip` over `1900c75`: public play operation
  tokens and nudge lifetime are independent. `cancelPlay` settles superseded
  operations and clears their deadline; only the exact current operation can
  recover on timeout. Both late native resolve/reject are inert after a presented
  paused seek, with only the metrics timer remaining.
- `open` validates identity, URL and volume into locals before cancelling work or
  changing generation. NaN, infinity, negative and excessive volumes preserve the
  old source, snapshot, callbacks, listeners and functional playback/seek behavior.
- The sole primary JSX video owns `data-qb-primary-playback="true"`; the redundant
  adapter assignment was removed. Rendered Solid/jsdom coverage verifies the exact
  marked element across fullscreen/recovery and removal; an unrelated video is
  unmarked. Vite disables Solid hot refresh only in test mode.
- Native binding ignores lower generations, accepts identical equal generations,
  rejects equal conflicts, and replaces higher generations. Regression verifies
  g2 -> g1 rejection plus subsequent g2 load/player/property/marker association.
  The existing frontend stale-snapshot filter is unchanged: the root cause was
  native mutation. Two Clippy findings in the M3 diagnostic code were corrected.
- Primary ownership integration, bounded Windows subscriptions and decoder parsing
  from the WIP are validated. Durable ownership is recorded in
  `docs/architecture/desktop-replay.md`. No M4 rate/audio work was added.

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

None. The temporary direct probe attachment is removed from product source.
Final ordinary and benchmark production builds passed on the final product tree.

## Remaining

- M4: shared rate/audio controls and truthful observed-rate/8x fallback behavior.
- M5: final targeted comparison and applicable viewer/clip/export/A/V smoke after
  M4, including the two equal combined resource blocks with 30-second settling.
  M1's isolated lifecycle/layout runs did not provide those combined blocks.
  Audible behavior, rate capability/fallback and export playback remain unclaimed.
  Do not repeat the accepted 008/012 campaigns or expand benchmark infrastructure.

## Verification

2026-09-08 M3 audit-fix verification on the current working tree:

- `npm.cmd run test --prefix app -- playbackController.test.ts
  htmlVideoPlaybackAdapter.test.ts playbackDiagnostics.test.ts ViewerScreen.test.tsx`:
  30 tests passed (27 controller + adapter + bridge + rendered DOM).
- `npm.cmd run test --prefix app`: 61 tests / 10 files passed.
- `npm.cmd run check --prefix app`, frontend build, benchmark production build and
  `npm.cmd run desktop:build --prefix app`: passed. Both final builds exclude the
  temporary probe. jsdom is a development-only dependency with locked install.
- `cargo test --manifest-path app/src-tauri/Cargo.toml`: 43 tests passed;
  focused `playback_diagnostics` rerun: six tests passed. Rust all-target check,
  `cargo fmt ... -- --check`, and `cargo clippy ... --all-targets -- -D warnings`
  passed. `cmd /c` launch was used where the local logging hook's PowerShell script
  policy blocked direct command invocation. npm registry installation required
  approved escalation after sandbox EACCES. No policy/config change was made.
- Initial DOM test hit Solid HMR under Vitest; disabling HMR in test mode corrected
  the test setup. Initial typecheck found missing GameSummary fixture fields;
  the fixture now supplies them. These failures are not product runtime failures.
- Canonical validator from `docs/development/VERIFICATION.md` passed for all 60
  roadmap items and six plan/checkpoint pairs. Scoped `git diff --check` passed;
  immutable ExecPlans are unchanged and product source/build contain no direct
  probe attachment. Only the 009 feature entry changed in canonical feature state.
- Earlier WIP M3 integration passed 53 frontend tests and initial native diagnostic
  tests. Its first production seek run, `qb-replay-009-m3-seek-20260908-r1`, was
  analyzer-rejected for the omitted `authoritative` first-frame payload field;
  the WIP restored that field and `ready_state`. The audit runs above retain the
  existing benchmark meanings and successfully verify authoritative presentation.

All raw roots below are under
`build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark/results/`.
Commands use existing `tools/replay_benchmark/run.ps1 -Manifest <absolute path>`
with manifests in the same sentinel's `manifests/qb-replay-009/` named
`<kind>-m3-audit.json`. No runner/schema changes were needed. The canonical fixture
is `native-current-v2`, game `1787904000`: H.264 High 1920x1080 yuv420p 60 FPS, AAC,
240 seconds; video SHA256 `131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe`.
Fixture post-hashes matched in successful normal runs and the direct r4 run.

| M3 case / result root | Actual observation | Disposition |
| --- | --- | --- |
| `qb-replay-009-m3-audit-seek-20260908-r1` | 5 requested/dispatched/seeked/presented; 80.3-142.4ms request-to-RVFC, versus before 72.6-162.9ms; associated hardware; no error/recovery | Functional evidence obtained; runner rejected process telemetry completeness, so no process-resource claim from this run |
| `qb-replay-009-m3-audit-scrub-20260908-r1` | 40 intents, 13 dispatches, 27 pending replacements, 6 presented; before 40/14/26/6; hardware confirmed; no errors/recovery | Runner/analyzer passed |
| `qb-replay-009-m3-audit-layout-20260908-r1` | 40 fullscreen transitions, 5 seeks presented in 15.1-147.1ms (before 15.1-147.8ms); hardware confirmed; no errors/recovery | Runner/analyzer passed |
| `qb-replay-009-m3-audit-lifecycle-20260908-r1` | 5 viewer disposals with zero owned timers/listeners; clean exit and quiesced streams; short cycles close before decoder acquisition | Runner/analyzer passed for lifecycle; decoder acquisition is proved by direct/normal playback cases |
| `qb-replay-009-m3-audit-play_pause-20260908-r1` | 5s warmup + 60s ordinary playback, 0 seeks/errors/recoveries, 1 range request, 15/3963 dropped/total frames (before 1201/3938); D3D11 hardware remains associated | Runner/analyzer passed; bounded comparison, not statistical performance certification |
| `qb-replay-009-m3-direct-20260908-r4` | Real paused RVFC seek; same primary DOM object through paused fullscreen entry, pending seek and playing exit; two 1s half-open loops; synthetic media-error recovery on same node; g1 and g2 associated D3D11 hardware; late g1 bind returns g2; terminal budget stops at 2 recoveries with Retry visible for >5s; disposal leaves every owned count zero, native bind rejects removed owner, no late decoder events | All eight direct assertions passed; app terminal complete. Unchanged normal-playback analyzer rejects deliberate `media_error`, as expected; this is controlled-fault evidence |

Direct procedure is retained at ignored `build/perf/qb-replay-009/m3_probe.ts`.
It was temporarily imported/called at controller creation, then removed. This
probe observed native HTML playback and fixed CDP diagnostics in an optimized
production WebView, but injected its media-error events; it does not prove an
actual damaged-file or driver-failure mechanism. Recovery restored the last
presented 34.983333s target, presenting 34.966667s within the existing one-frame
tolerance and retaining paused intent. RVFC frame-boundary observations are not
relabelled as exact requested-time presentation.

Preserved failed direct roots: r1 had an incorrect probe comparison to the earlier
requested 35s rather than the last presented restoration target; r2 passed the
recovery/disposal sequence and was analyzer-rejected for its synthetic fault; r3
injected a second fault before the current-source barrier, correctly ignored by
the controller. r4 waited for real readiness and proved bounded terminal behavior.
No product tolerance or readiness validation was weakened.

Measured runtime is `Edg/152.0.4191.66`, CDP protocol 1.3, Windows 10 build 19045,
Dell G15 5511 / i7-11800H / Intel UHD and RTX 3050 Ti Laptop. Actual decoder is
`D3D11VideoDecoder`, platform flag true, exact current-player association true,
at initial open and recovered generation; no marker cross-check stall, software
fallback or unknown result is presented as hardware proof. Normal WebView GPU
configuration is retained: no added disable-GPU flags, browser-argument override
or remote debug port. Optional GPU CIM telemetry remains disabled and was not used
as decoder evidence. Runner verified packaged runtime r6 and both media-tool hashes
under `build/perf/qb-replay-009/resources/media-runtime`.

Binary SHA256 identities (source baseline `1900c75` plus the documented M3 diff):

- M1 preserved benchmark reference `build/perf/qb-replay-009/before.exe`, source
  `96bb7b6802138d0afee7863a15ae2ce509fd450d`:
  `846f585f0f35386832ccf55bb4a7fdcd03d5e13f80ef47b39506fe86c6833d0c`.
- Final ordinary `app/src-tauri/target/release/league-replay-app.exe`:
  `d00e1721706516006d67a8da96b29ed65d3c8c927fa4910c4ba6d18fda707212`.
- Final benchmark `build/perf/qb-replay-009/m3-final.exe`:
  `99edeb0a94f36e4ad916a304d53331cd51250886ede5726aa3c25521cecbb019`.
- Direct r4 instrumented `build/perf/qb-replay-009/m3-direct.exe`:
  `6b4c59d0c5d8128254b9f83a4eebb7a72babfdb228b2f4048e84b61b40c68715`.

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

The immutable v2 plan remains unchanged. The marker audit premise was partially
incorrect: `htmlVideoPlaybackAdapter.ts` already set the dataset marker before
opening media. The marker now resides explicitly in the primary JSX video and is
covered by a rendered DOM test. Native diagnostics were not redesigned.

The missing direct Windows cases used an ignored temporary probe attached to the
existing controller in a benchmark-feature production build. It used controller
commands, real RVFC/native decoding, layout UI events, one controlled synthetic
media-error episode, and existing fixed diagnostic IPC. No product API, remote
debug port, runner schema or persistent benchmark infrastructure was added. The
attachment was removed before the final normal production builds. Fault-run
analyzer rejections remain failures of the normal-playback analyzer, not normal
playback success or evidence of an actual corrupt-media/driver fault.

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
- Apply the repository `forward-engineering` skill for substantial implementation
  and the available Rust skill for Rust changes. Preserve unrelated working-tree
  changes; this pass changed only the M3 implementation/tests and related evidence.
- Focused review found no further demonstrated product defect. Its suggestion to
  pause from an obsolete fulfilled play callback was not applied: the concrete
  HTML adapter changes `paused` during the synchronous play call, independently
  of later promise settlement. The regression now models that synchronous change.
  See the [HTML internal play/pause steps](https://html.spec.whatwg.org/multipage/media.html#internal-play-steps);
  stale completions retain no authority to mutate newer work.

## Blockers

No product blocker established in M3. The early normal seek run was rejected by
the process telemetry contract despite complete seek/decoder events; retain that
limitation and do not use it for a process-resource claim. The unchanged analyzer
rejects deliberately injected media errors, so direct fault-run assertions and
actual decoder snapshots are retained separately from normal benchmark results.

## Next action

Stop at completed M3. Wait for a new request before starting M4; its first unit is
the approved shared rate/audio controls and bounded observed-rate limitation work.
