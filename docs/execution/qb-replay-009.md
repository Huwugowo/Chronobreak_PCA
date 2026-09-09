# QB-REPLAY-009 execution checkpoint

Feature: `QB-REPLAY-009`
ExecPlan: `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v2.md`
Updated: 2026-09-09

## Current milestone

M4 implementation and production checks are complete. M5 automated, production
build, targeted functional and resource checks are obtained. Feature remains ready / in-progress because required audible/A/V
observations are unreported. M3 commit `0fe1f4f` remains immutable.

## Active unit

Obtain the required listening observations before making the feature completion
decision. Implementation and targeted automated/production work are complete.

## Completed

- M4: shared six-rate/mute/volume controls in both layouts; selected/applied/
  observed rate and independent audio intent; finite verification, fallback and
  recovery; non-renewable capability deadline after the production r1 buffering
  finding; regression and focused review corrections. Production r2 exercised
  every native rate, paused rapid changes, focused fullscreen controls, recovery,
  short loop, EOF and disposal. Separate controlled slow-8x evidence proved the
  visible fallback without rate-emulating seeks or failed-rate reapplication.
- M5: 80 frontend and 45 Rust tests; applicable formatting/Clippy; clean ordinary
  and benchmark production builds; targeted seek/scrub comparison; two equal
  resource blocks and 30-second settling; champion filter and clip endpoint
  editing; existing exported-clip playback and source preservation. Temporary
  probes are detached. Analyzer limitations are explicitly retained below.

- 2026-09-09 independent M3 finding fixed at `kLoad`: candidate properties are
  cleared before a new URL is reconciled. Candidates remain across bindings.
  Reused-player g1 -> bind g2 -> load g2 stays unknown until fresh decoder and
  platform properties arrive; repeated identical-URL loads also invalidate.
  A separate regression preserves fresh load-before-bind evidence. The earlier
  generation's hardware evidence is no longer reusable for a later load.

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

No product edits or production runs remain active. The required human listening
observations are the only unfinished completion work.
Ignored M4/M5 procedures and results remain available for focused follow-up;
`ViewerScreen.tsx` and final builds contain no probe imports/calls.

## Remaining

- Obtain audible behavior at each selected rate, mute/volume and failed-rate/
  fallback sound behavior, plus viewer/export A/V synchronization. Property
  assignments and AAC stream presence do not establish these observations.
- After those observations, make the completion decision and validate canonical
  state. Do not repeat accepted 008/012 campaigns, completed M4 checks or
  analyzer-only rejected procedures.

## Verification

Final canonical validation passed for 60 roadmap items and six plan/checkpoint
pairs on 2026-09-09. Scoped whitespace checks passed; the immutable v2 plan and
M3 HEAD remain unchanged, nothing is staged, and unrelated `.agents`/`.codex`
working-tree changes were preserved. No listening/A/V pass is recorded.

M5 resources passed the bounded settling/ownership check. Using the final ten
one-second samples before each settled marker, block 1 -> block 2 medians were:
private memory 389.8 -> 385.1 MiB; working set 535.6 -> 536.7 MiB; CPU 2.28 ->
2.41%; handles 3906.5 -> 3891; threads 225 -> 215; eight processes in both.
Private memory, handles and threads did not accumulate across equal blocks; the
1.1 MiB working-set difference does not establish sustained growth. Each settled
controller owned one frame callback, 17 listeners, two timers and no active/pending
seek; every disposed controller released all counts. Server requests increased
6 -> 11 for the five expected reloads, with zero cancellations/recoveries/errors.
Final per-player quality was 38/1970 and 39/1973 dropped/total frames; these are
different mounted players, so their counters must not be subtracted as one stream.
No exact combined before baseline exists; this proves bounded after settling,
with M1's isolated runs as context, not a matched combined resource delta.

R1's 60-second ordinary interval added 3599 frames and 11 drops (0.31%), with
no new seek epoch, recovery or server request. Its last-ten-second medians were
376.8 MiB private, 539.8 MiB working set, 7.31% CPU, 3875 handles, 212 threads and
eight processes. Controller ownership remained 1 frame / 17 listeners / 2 timers,
with no outstanding seek. The run's later planned-navigation terminal failure
does not erase these interval observations and is not a successful whole-run claim.
M1 ordinary playback recorded 1201 drops/3938 frames and zero seeks/recoveries;
the after interval shows no new drop/seek/recovery regression. Different startup/
recovery boundaries preclude a precise whole-run improvement claim.

Final clean ordinary and benchmark production builds passed on 2026-09-09,
including frontend typecheck/build. Both exclude every temporary probe import/call.
Clean benchmark binary SHA256:
`85fa3487d691a55cf1e2990765497adb8ae7c2df45fd524666fb732c62c3b164`.
Ordinary binary SHA256:
`e86b5ba56f97382b14ef7a16155ba3fa0a88a933ebf37d8bc32e590f57b9d260`.
The required Rust formatting/Clippy results from the decoder-fix unit remain valid;
no Rust edits followed those checks.

Final clean comparison uses `results/qb-replay-009-m5-seek-20260909-r1` and
`qb-replay-009-m5-scrub-20260909-r1`, with unchanged fixture/runtime and preserved
source hashes. Concise numbers: `build/perf/qb-replay-009/m5-comparison.json`.
Seek passed with analyzer acceptance: five requests/dispatches/presentations,
69–99.2 ms request-to-presentation versus M1's 72.6–162.9 ms. Scrub completed all
five bursts at their latest target, with 40 requests / 14 native dispatches / 26
pending replacements, matching M1's request/dispatch/replacement counts. Latest
presentation latency was 89.3–236.8 ms versus M1's 16.6–492.4 ms. Supersession
observations differ after extraction (35 explicit cancellations versus eight in
M1); do not compare those counts as equivalent scheduler work. Neither final case
had media errors or recoveries, and scrub terminal reported one dropped frame of
279 total. No material regression is established by this bounded comparison.

The scrub analyzer rejected seek-41 solely because RVFC presentation arrived at
4572.0566 ms before `seeked` at 4575.0566 ms. Dispatch preceded both (4563.7566 ms),
all phases exist, and presentation was within the accepted one-frame tolerance.
`analyze.py:924-929` assumes dispatch -> seeked -> presented. The controller's
approved contract and existing regression explicitly allow presentation before
seeked while retaining the native slot until seeked. This is an analyzer ordering
limitation, not a product defect; retain its rejection and direct evidence, without
changing playback ordering or expanding benchmark infrastructure to obtain a pass.

M5 direct r2 (`results/qb-replay-009-m5-direct-20260909-r2`) used binary SHA256
`d6479c2d02857c230423ccba3f35a1c486cfebaa38b87cdae66a612c6ae3b93e`.
All 19 direct checks passed: two equal five-remount blocks, twenty total layout
transitions, directed/latest-scrub presentation, 30-second settling after each
block, champion-filter toggle/restore, both clip endpoint controls with keyboard
preview, existing exported clip playback, and final disposal with all owned counts
zero. The five-second 1080p clip `1787904000_1788167913.mp4` presented 270 frames
(media time 0.016667 to 4.983333) without media error and reached `ended`.
R2 had 11 mounts/disposals, zero media errors/recoveries, 114 seek requests / 31
native dispatches with latest presentation verified per burst, and associated
D3D11/platform evidence on Edg/152.0.4191.66. It ran on the same Dell G15/i7-11800H,
Windows 10 build 19045, Intel UHD/RTX 3050 Ti configuration and r6 media runtime.
No GPU-disable flag or optional unbounded GPU CIM collection was introduced.
Terminal completed in 105.88 seconds; source hashes and telemetry reconciled,
843 accepted records / zero drops. The ordinary play_pause analyzer rejected
missing pause_requested/pause_complete events, as expected for this direct
procedure. The probe bypassed only the exact expected planned-navigation terminal
reason; all other failures retained their existing handling. It is now detached.

2026-09-09 final product-tree frontend suite passed 80 tests in 11 files, and the
complete Rust app suite passed 45 tests. Logs: `.codex/logs/command-20260909-094407.log`
and `command-20260909-094405.log`. No product code changed after these suites;
temporary procedure attachments still require removal and fresh final builds.

M5 direct r1 (`results/qb-replay-009-m5-direct-20260909-r1`) used binary SHA256
`aa91ac495ef7aa514d4d73fffcb6939ad6df938e155876e81f3776e8d088b0d5`.
An ignored controlled adapter fault reported an accepted 8x property while real
video continued at 1x. Real RVFC evidence triggered the visible 1x fallback with
no rate-emulating seeks, then reload retained selected 8x/applied 1x. This proves
limitation handling, not native 8x incapability. The following 60-second ordinary
segment completed its start/end observations. At the first planned viewer close,
the unchanged ordinary scenario failed with `viewer disposed before benchmark
completion`; no combined-block or terminal success is claimed. R1 is retained,
with zero observer queue drops. Its remaining checks require the repaired direct
procedure; it does not establish a product defect.

2026-09-09 M4 r2 actually ran against the decoder-fixed benchmark binary SHA256
`59a49158de5f2b700301fcbf02ad9a649b0bbd604cfb2a2334838cacccc4e34d`.
Root: `results/qb-replay-009-m4-direct-20260908-r2` under the existing sentinel
(run ID retained from its prepared manifest). All ten direct assertions passed:
six native selections; paused rapid rate/recovery preferences; short loop; EOF;
disposal with every owned count zero. Selected/applied matched all six rates;
observed was 0.24862, 0.49997, 0.99997, 2.00000, 3.98320, 7.99947x. This run's
8x capability success is separate from r1's repeated-buffering failure, and does
not itself prove visible failed-8x handling. R2 confirmed associated D3D11/platform
hardware at g1 and fresh g2 after recovery with the fixed load ownership.
App/runner terminal was complete; source post-hashes and telemetry reconciled.
The unchanged ordinary play_pause analyzer rejected the direct procedure because
it deliberately completed before the normal scenario's pause_requested /
pause_complete events. This is direct-check evidence, not analyzer acceptance.
Windowed screenshots: ignored `build/perf/qb-replay-009/m4-r2-window.png` and
`m4-r2-fullscreen.png` (the latter was captured after fullscreen had already exited).

Decoder-fix Clippy `--all-targets -- -D warnings` and benchmark production build
also passed. No unrelated M3/M4 suite was replayed for this narrow native change.

2026-09-09 decoder ownership fix: `cargo test --manifest-path
app/src-tauri/Cargo.toml playback_diagnostics` passed 8 tests; `cargo fmt
--manifest-path app/src-tauri/Cargo.toml -- --check` passed. This supersedes the
old test assumption that properties received before `kLoad` can prove that load.
The fixed M4 r2 build subsequently confirmed recovered-generation hardware
evidence; unrelated completed M3/M4 evidence remains accepted.

2026-09-08 M4 initial focused validation: 41 controller tests passed (including
the six deterministic rate windows, freeze/slow/rejected fallback, buffering,
hidden/loop/end/no-RVFC cases and preference retention); frontend typecheck passed.
The first pass exposed watchdog removal of a just-completed window's observed
result when the next window had fewer than three samples. The missing-evidence
branch now applies only before any measured result; all six regressions pass.
The local logging hook needed sandbox escalation for its `.codex/logs` output;
no hook/configuration file was changed. Subsequent UI/timer edits still need checks.

M4 later validation: full frontend suite passed (76 tests / 11 files) and frontend
production build/typecheck passed with shared UI integration. Focused review found
the first buffering timeout incorrectly delayed reload until a second five-second
window and audio assignments could throw/misreport ignored mute. Both were fixed;
missing RVFC callbacks now trigger an explicitly unknown evidence timeout with
finite fallback/reload, while an unavailable RVFC API remains unknown without
invented presentation. That focused controller suite passed 44 tests; the later capability-deadline
regression brought it to 45, included in the final 80-test frontend suite. New
controller events use `playback_rate_observed`, preserving the existing benchmark
`rate_observed` payload meanings. Final ordinary production checks remain pending.

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

- M5 combined after blocks use the existing App viewer-cycle event from temporary
  ignored instrumentation, not a new runner/schema. M1's isolated lifecycle/layout/
  scrub runs remain the shared before reference; they are not an exact combined
  process-resource baseline. Assess retained work and settling across equal after
  blocks, with no manufactured matched combined resource claim.

- M4 production r1 exposed repeated brief buffering at 8x, renewing the two-second
  settling/three-second measurement window indefinitely. The attempt now has a
  ten-second evidence deadline that buffering cannot renew; intentional seeks,
  pauses, hidden documents, loops and reloads restart eligibility. Failure is
  explicitly unknown/unverified with the same bounded fallback, never a measured
  stall or 8x success. No tolerance or native-rate emulation changed.

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
  changes; M4/M5 work follows the immutable M3 checkpoint.
- Focused review found no further demonstrated product defect. Its suggestion to
  pause from an obsolete fulfilled play callback was not applied: the concrete
  HTML adapter changes `paused` during the synchronous play call, independently
  of later promise settlement. The regression now models that synchronous change.
  See the [HTML internal play/pause steps](https://html.spec.whatwg.org/multipage/media.html#internal-play-steps);
  stale completions retain no authority to mutate newer work.

## Blockers

Required audible output/A/V observation has not been reported by a human. The
feature remains non-done. No product blocker is established by the targeted
functional evidence. Retain direct-run and presentation-before-seeked analyzer
rejections as evidence limitations, not passes or playback defects.

## Next action

Obtain a listening pass on the existing sentinel fixture: 0.25x/0.5x/1x/2x/4x/8x, mute and volume, failed-rate/fallback,
and viewer/export A/V. Record the actual observed sound or runtime limitation;
keep unobserved cases non-passing. Reuse the retained M4/M5 procedures only for
missing observations, without redoing capability/resource work. If those checks
satisfy acceptance, update canonical status and validate it. Preserve `0fe1f4f`,
the immutable v2 plan, and unrelated `.agents`/`.codex` changes.
