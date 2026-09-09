# QB-REPLAY-009 execution checkpoint

Feature: `QB-REPLAY-009`
ExecPlan: `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder-v3.md`
Updated: 2026-09-09

## Current milestone

M5 completion gates on `qb-replay-009-wip` over `78da82b`: controlled production
hardware-decoder acceptance is satisfied under the user's approved v3 clarification.
The ordinary production source is exactly restored and its fresh build passed.
M1-M5 functional/resource evidence and corrective audit r2 remain settled.
Only human audible/A-V observations remain. At the user's request, they are
deferred / not performed because no suitable audio fixture is currently available.
The requirement remains unchanged; feature stays ready / in-progress.

## Active unit

No implementation or automated measurement remains. The human listening/A-V gate
is deferred as recorded below. No further decoder experiment or runtime mechanism
is needed; normal current-load status remains Unknown without trustworthy provenance.

## Completed

- 2026-09-09 approved decoder clarification: self-contained reviewed v3 linked;
  v1/v2 preserved. Canonical criteria 3/4 and decoder verification explicitly allow
  controlled production proof while preserving conservative runtime status.
- Controlled `...-controlled-decoder-20260909-r2` proves the hardware-normal path
  for the canonical H.264 fixture/device/runtime/load: one fresh profile, marked
  primary video and load; D3D11VideoDecoder/platform=true; RVFC advancement; no
  loss, competing load, error, recovery or contradictory decoder observation.
  Detailed result: `docs/performance/evidence/qb-replay-009-controlled-decoder-20260909.md`.
- Temporary acceptance taps detached byte-for-byte; ordinary release build passed,
  SHA256 `8c7e6f72dc500f675d72f9fd8f7f7fca532ea86f2a5f4384d99d3ad4a4482840`.
  No permanent product code, benchmark tool/schema, tracing or runtime overhead added.


- 2026-09-09 audit corrections over `78da82b`: CDP properties are explicitly
  player-scoped and cannot establish current-load decoder status. Exact load URL
  and marker still establish player association, but path stays Unknown with no
  current decoder name/platform value. No arrival-order rule, timer, retry or
  remount is used. Regression covers `kLoad(g2)` delivered before stale decoder(g1),
  load-before-bind, repeated URLs and replayed player discovery. Earlier hardware
  and software path claims from this channel are withdrawn, including initial
  acquisition: the protocol has no property-to-load identity.
- Shared audio controls render `snapshot.media.muted/volume`; snapshot reads the
  element once, while top-level mute/volume retain recovery intent. Volume input
  resets its native thumb to the applied value even when that value is unchanged.
  Rendered controller/HTML-adapter tests cover ignored mute and ignored, rejected,
  transformed volume, repeated rejected input and intent restored on recovery.

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

- Superseded attempt, not an ownership proof: 2026-09-09 M3 change at `kLoad`: candidate properties were
  cleared before a new URL is reconciled. Candidates remain across bindings.
  Reused-player g1 -> bind g2 -> load g2 stays unknown until fresh decoder and
  platform properties arrive; repeated identical-URL loads also invalidate.
  A separate regression preserves fresh load-before-bind evidence. The earlier
  generation's hardware evidence could still cross a load under reordered delivery;
  the new correction above supersedes this conclusion and its decoder claims.

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

None. Temporary raw decoder/frontend/adapter taps are detached; all three files
match their session-start bytes. The ordinary executable was rebuilt successfully.
Ignored temporary patch, source snapshots and raw evidence are retained for review.
No permanent product or benchmark code was added by this resumed unit.

## Remaining

**2026-09-09: deferred / not performed (`not run`), not PASS.** The user reports
that no suitable audio fixture is currently available and explicitly requests
postponing the remaining human audible/A-V validation. All observations below
remain required; none is completed or waived. Existing AAC, rate/control and
video-presentation evidence cannot establish audible output or A/V synchronization.

Retain this checklist for a later human session with a suitable audible recording
and exported clip containing recognizable paired visual/audio cues. Use the
ordinary release executable. The existing dedicated fixture and optional ignored
`build/perf/qb-replay-009/manual-audio.ps1` launcher do not establish fixture
suitability. Do not run this checklist now, create generic import/test-fixture
infrastructure, or rerun automated M1-M5 or decoder work for this deferred gate.

1. Play a known audible section at `0.25x`, `0.5x`, `1x`, `2x`, `4x`, `8x`, about
   10 seconds per selection (seek back as needed). For each, report audible / silent /
   distorted and the displayed applied rate or limitation. Rate property evidence
   already exists; the missing evidence is what is heard.
2. At 1x, test mute/unmute and volume `100% -> 25% -> 0% -> 100%` in windowed and
   fullscreen layouts. Report whether mute/zero are silent, 25% is quieter and
   unmute/100% restores sound without an unexpected jump. Check sound continuity
   when entering/exiting fullscreen.
3. Select 8x once. If it limits/falls back, report sound before and after fallback,
   displayed fallback rate and approximate recovery time. Then choose 1x and report
   whether normal sound resumes. If 8x works and no fallback occurs, report
   `fallback not triggered`; do not repeat attempts to manufacture a failure and
   do not count unobserved fallback audio as passed.
4. At 1x, compare a recognizable visible/audible cue early and late in the viewer,
   then in its existing exported clip. Report in sync / audio leads / audio lags,
   approximate offset if perceptible, and whether it grows. A test pattern/tone
   without a recognizable paired cue is `not assessable`, not a pass; identify a
   suitable existing representative clip instead. No new export benchmark is needed.

Report the recording/clip name, audio output device, and the four results. Missing
or not-assessable observations remain open. After the report, reconcile acceptance
and canonical completion without rerunning unaffected technical checks.

## Verification

2026-09-09 manual gate: **deferred / not performed (`not run`), not PASS**, at the
user's request because no suitable audio fixture is currently available. This
covers human rate/mute/volume/layout/fallback audibility and viewer/export A/V
observations. Automated and controlled production results below remain valid;
this deferral changes neither acceptance criteria nor required verification.

Documentation-only deferral checks passed: canonical validation of 60 roadmap
items and six plan/checkpoint pairs, plus scoped whitespace checks. Only this
feature's concise evidence changed in the roadmap; all criteria, verification,
lifecycle fields, other features, product code, plans and the controlled evidence
report are unchanged from this deferral session's start. No app test, build or
production measurement was invalidated or rerun.

Final canonical validation passed for 60 roadmap items and six plan/checkpoint
pairs. Session-start comparison confirms only QB-REPLAY-009 changed in the
roadmap, the immutable v2 hash is unchanged, and all temporarily touched product
sources match their original bytes. Ordinary assets contain no capture/audit tap
markers. Scoped diff/new-document whitespace checks passed. The optional manual
launcher passed PowerShell parsing only; it has not been launched and supplies no
human evidence. Existing unrelated `.agents`/`.codex` changes remain preserved.

2026-09-09 final controlled result and limitations are recorded in
[`qb-replay-009-controlled-decoder-20260909.md`](../performance/evidence/qb-replay-009-controlled-decoder-20260909.md).
The native transcript has ten contiguous records/4233 bytes, one player and one
load; the ready-before-source dependency is explicit, while native/frontend clock
estimates are not used to invent event ordering. `playerCreated` exposes no node
ID in this capture: association uses the exact unique URL plus the independent
marked-singleton source observation, not an unobserved DOM.describeNode result.
All eight focused diagnostic tests passed, including native queue/loss coverage;
benchmark and detached ordinary production builds passed TypeScript/Vite/Tauri.
The ignored direct isolation verifier passed. Runner/analyzer accepted the normal
play_pause scenario (15006.5691 ms, 67 records, zero drops, preserved hashes).
Runtime remained Unknown; this independent proof does not restore historical
player-property-to-generation claims or label recovered generations.

Verification limitations/failures retained: preflight caught the fresh profile's
wrong cache root before launch; corrected preflight passed. Sandbox r1 created no
WebView/frontend, timed out after 30 seconds and supplies no decoder evidence.
R2 was the sole completed capture after correcting Windows execution access.
Post-exit WMI supplied no live command lines; launch configuration/environment and
actual D3D11 selection are separately recorded. Temporary frontend field typo was
fixed before building. A formatting attempt selected Rust 2021 for this Rust 2024
source and did not run; detachment restores the already-verified source bytes.
The local logging hook required escalation for focused test logs. The optional
attachment review agent failed due service quota; root reviewed the patch, fixed
its close-marker drain race before r2, and checked the complete raw assertions.
No required technical check is being waived; no audible/A-V observation is invented.

2026-09-09 task A inspected the existing
`results/qb-replay-009-audit-fixes-20260909-r2` before any rerun. All eight
`audit_direct_check` assertions passed: initial associated decoder stays Unknown,
native mute/volume controls, controlled ignored mute, repeated rejected and
transformed volume input, fullscreen applied state, same-element recovery restoring
intent with Unknown decoder, and disposal with all owned counts zero. Both initial
and recovered current decoder name/platform fields remained absent; none of the
observed snapshots claimed hardware/software confirmation. These injected audio
faults prove truthful controls, not a spontaneous WebView failure or audible output.

The app completed in 6099.4373 ms; 87 accepted event records, zero drops. Collection
completed with exit code 0, no forced termination/timeouts, reconciled telemetry
and unchanged fixture/source hashes. Instrumented binary SHA256:
`4c56270dcbb09a4023cd595135e0494b76c46b83e34d3475053e1330e88643a4`.
The analyzer rejected only missing `pause_requested` / `pause_complete`: the
tagged direct procedure does not run the ordinary `play_pause` scenario's two
transition cycles (`tools/replay_benchmark/analyze.py`, scenario evidence gate).
Its early rejection is not analyzer acceptance; direct assertions, terminal,
collection and hashes were inspected separately. Retain the failed analyzer files
unchanged. The v2 plan explicitly permits direct documented validation, so the
smallest validation correction is this separate disposition, with no analyzer or
product change and no r3. `audit_probe.ts` remains ignored and detached from
`ViewerScreen.tsx`.

Task A final `npm.cmd run desktop:build --prefix app` passed, including TypeScript,
Vite and optimized Tauri compilation. Ordinary executable SHA256:
`a04f99db856e644aac0bfdfe8840bccbd861b3c97085c64e240266b5b5a13097`.
Source and built assets contain none of `runAuditProbe`, `audit_direct_check`,
`Controlled rejected volume`, or `direct_audit`. No product code changed in this
resume, and the already-passed focused frontend/Rust/fmt/Clippy results remain
applicable. The linker emitted its Windows import-library creation warning;
the build exited 0.

Task A/B canonical validation passed for 60 roadmap items and six plan/checkpoint
pairs. This resume changes only the checkpoint and the 009 evidence entry;
acceptance criteria, verification definitions and immutable v2 design are preserved.
The v2 hash still matches the session-start value. A broader comparison against
HEAD could not prove session preservation because four other roadmap entries
already differ in the shared working tree (`QB-CLIP-004`, `QB-REPLAY-016`,
`EPIC-AUDIO`, `QB-AUDIO-001`); they were not edited by this task. Within 009, only
evidence differs from HEAD. Scoped whitespace checks passed; unrelated changes
remain intact.

2026-09-09 audit unit: the focused frontend command `npm.cmd run test --prefix app
-- PlaybackControls.test.tsx playbackController.test.ts htmlVideoPlaybackAdapter.test.ts
ViewerScreen.test.tsx` passed 52 tests / 4 files; log
`.codex/logs/command-20260909-110029.log`. `npm.cmd run check --prefix app` passed.
`cargo test --manifest-path app/src-tauri/Cargo.toml playback_diagnostics` passed
8 tests; log `command-20260909-110202.log`. Rust fmt check passed. Clippy found the
two now-unconstructible positive status variants after removing unproven decoder
promotion; they were removed from this native producer. Clippy rerun passed
(`.codex/logs/command-20260909-110429.log`). The first frontend attempt was blocked by
the logging hook's protected `.codex/logs` write and then passed with escalation;
no configuration changed.

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

### 2026-09-09 user-directed manual verification deferral

Postpone only the remaining human audible/A-V observations because no suitable
audio fixture is currently available. Record them as deferred / not performed;
do not infer PASS, weaken the requirement, or mark QB-REPLAY-009 done. The current
checkpoint and roadmap identify no remaining engineering task. Stop at this gate;
preserve completed automated/production evidence and immutable plans. No generic
import/test-fixture infrastructure or repeat M1-M5/corrective r2 campaign is needed.

### 2026-09-09 adopted user clarification

The user explicitly adopted the prior recommendation: hardware remains the normal
production requirement; Unknown never proves it; runtime decoder status stays
conservative without current-load provenance. Continuous identification of every
reload/recovery is not required. Controlled safely attributed production evidence
may establish the hardware path. No tracing/ETW/polling/timing heuristic or new
hot-path/runtime overhead is justified for proof. The requirement change is
recorded in canonical criteria 3/4 and verification. Reviewed self-contained v3
supersedes v2's decoder prescription; v1/v2 are preserved unchanged. Read-only plan
review found no material issue. Implementation and measurement are authorized by
the same user request; this is a resumed execution, not a new planning-only pass.
Historical proposals and withdrawn claims below retain their original context;
this adoption supersedes their not-yet-adopted wording.


### 2026-09-09 task B: decoder proof decision

**Decision: no new runtime decoder mechanism is justified.** Preserve Unknown
and absent current decoder name/platform fields. Do not add tracing, ETW,
per-frame instrumentation, polling, CDP refresh traffic, settling delays or
same-player heuristics. Investigation used the current implementation, current
official CDP/WebView2 contracts and Chromium source; it did not execute a new
decoder experiment. No alternative's overhead was measured, so none is claimed
negligible. This is the bounded stopping point for B.

The source provides two different facts: `kLoad.url` plus the primary DOM marker
associates a player; decoder properties identify a decoder for that player.
Neither `PlayerProperty` nor the containing property event supplies a load ID,
generation or property timestamp. Chronological order inside an events batch
does not order that batch against a separate property message. `playerCreated`
can replay active-player discovery. Awaiting enable or describing the DOM node
does not create missing decoder provenance. See the
[Media protocol](https://chromedevtools.github.io/devtools-protocol/tot/Media/).
Chromium defines decoder name and hardware flag separately from the document
`kFrameUrl`; the latter is not the media URL. See
[media properties](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/media_log_properties.h).

The existing implementation already has five Media subscriptions, initial
`Browser.getVersion` / `Media.enable`, a DOM description per candidate needing a
marker, and `Media.disable` on close (`playback_diagnostics/native.rs`). It also
has a 100 ms worker tick, a 16-event queue, 64 KiB queued payload limit, eight
concurrent candidates and 64 properties per candidate. There is no per-frame CDP
request. These are existing costs, not evidence of negligible isolated overhead;
this task adds none. `reconcile` in `playback_diagnostics.rs` correctly separates
association from decoder provenance.

| Candidate | What it proves | Correlation to this load | Runtime cost | False-positive risk | Sufficient? |
| --- | --- | --- | --- | --- | --- |
| Existing CDP Media properties + exact load URL/DOM marker | Player association and player-scoped decoder declaration | No property-to-load key across opens/recovery | Existing bounded subscription/worker described above | Stale decoder properties can be paired with a later load | No current-load confirmation |
| HTML capability/quality signals, MediaCapabilities, CDP SystemInfo | Format/acceleration capability predictions or playback progress | No selected-decoder identity for this load | Small one-shot query or existing metrics; no benefit to polling | Capable GPU or smooth playback can coexist with software decode | No |
| Fresh single-load production acceptance capture using CDP Media | That the selected hardware decoder was observed for the sole controlled load | Isolation must independently exclude every other load for the matched primary player | Short bounded raw-event retention only in an acceptance build; no added normal cost; collection cost unmeasured | Low only when all isolation conditions below are demonstrated; otherwise unproven | Conditionally sufficient for that observed load/interval, not obtained here or transferable to a reload |
| CDP Tracing / Perfetto media events | Internal pipeline/decoder activity | No supported complete join from decoder trace identity to this element's load found | Trace production, buffers and flush/I/O; extra collection cost unmeasured | Temporal/track guesses can attribute another pipeline or decoder initialization | No supported current-load proof found; no trace experiment justified |
| ETW / GPUView / GPU engine activity | GPU/kernel/video work, sometimes attributable to a process/context | No documented join to the HTML load/generation | System collection and ETL I/O; unmeasured here | Other video, compositor or driver work; decoder creation is not proof of this stream | No |
| WebView2 CDP host API or DevTools Media UI | Access to the same protocol/diagnostic data | Adds no missing load key | Method/event transport or open debugging UI | Same provenance limitation | No independent proof source |

Capability claims follow [Media Capabilities](https://w3c.github.io/media-capabilities/)
and [SystemInfo](https://chromedevtools.github.io/devtools-protocol/tot/SystemInfo/).
[Tracing](https://chromedevtools.github.io/devtools-protocol/tot/Tracing/) supports
bounded start/end, category filters, trace buffers/streams and loss reporting; its
public contract does not provide the required join. Internal decoder trace events
are implementation details, as illustrated by Chromium's
[decoder stream](https://chromium.googlesource.com/chromium/src/+/master/media/filters/decoder_stream.cc).
[GPUView](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/using-gpuview)
collects system video/kernel events, not HTML load ownership.
[WebView2's CDP integration](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol)
provides protocol access, not additional decoder semantics. CDP Media remains
experimental and [tip-of-tree compatibility is not guaranteed](https://chromedevtools.github.io/devtools-protocol/);
source inspection is not proof that a specific Edge build emits every trace field.

The conditional acceptance candidate is an inference from controlled isolation,
not a new API guarantee. It would use a fresh app/WebView context and the ordinary
primary video, subscription completed before its first source assignment, a
recorded single controller/adapter load with unique session URL, matching player
and DOM marker, and a reviewed hardware decoder name (expected Windows
`D3D11VideoDecoder`) plus `kIsPlatformVideoDecoder=true`. Retain complete
bounded lifecycle/raw events and presentation evidence; exclude pre-existing or
ambiguous players, competing loads, source changes, reloads, recovery, navigation,
event loss and contradictory decoder observations from that interval. An unknown
decoder name/wrapper or missing data remains inconclusive. This proves hardware
use during that controlled load, not a label valid indefinitely or after reload.
No replacement video/decoder enters normal replay to make isolation true.

Existing r2 does not satisfy that candidate: its decoder snapshots intentionally
omit raw names/platform values; it exercises recovery; it does not retain a full
raw CDP/lifecycle isolation transcript. Earlier derived D3D11 confirmations do not
retroactively acquire these prerequisites. Their withdrawal remains in force.

Requirement reading and smallest honest adjustment (recommendation, **not an
adopted acceptance change or passing result**):

- Canonical acceptance criterion 3 asks for production validation of the expected
  hardware path; it does not explicitly require a permanent per-generation
  runtime decoder identity. Criterion 4 still requires hardware as normal path
  and software as diagnosed compatibility fallback. Neither permits Unknown to
  count as hardware-normal performance evidence. Criteria 5/8 and manual
  verification permit truthful stale/unknown diagnostic outcomes.
- The v2 plan's `Decoder identity and canonical media` prescription to promote
  matched player properties, retain decoder transitions, and prove the path
  through recovery assumes more provenance than the protocol supplies. That
  positive-confirmation claim must change. Preserve the immutable v2 plan;
  any adopted replacement design must explicitly supersede that portion rather
  than silently editing it or restoring the invalid heuristic.
- Proposed canonical clarification for criteria 3/4 and decoder verification:
  **Hardware remains the required normal path for canonical supported media.
  Demonstrate it with load-correlated, controlled production acceptance evidence
  for the recorded fixture/runtime/device and observed interval. Runtime decoder
  identity is best-effort and remains Unknown whenever current-load provenance
  is absent, including after reload/recovery. Diagnose a proven software fallback;
  Unknown is neither software proof nor hardware confirmation. Neither software
  nor Unknown supports hardware-normal performance claims. Permanent decoder
  identity and guaranteed detection of every fallback are not required.**
- Recovery acceptance would prove evidence invalidation and truthful Unknown,
  while hardware-path acceptance would use the isolated load above. No existing
  hardware gate is marked passed. If that bounded capture cannot demonstrate its
  prerequisites, stop with hardware unverified; do not progress to tracing/ETW or
  an expanding series of probes merely to recover HardwareConfirmed.

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

Only required human audible output and viewer/export A/V observations remain,
deferred / not performed because no suitable audio fixture is currently available.
Hardware-normal evidence is obtained for the controlled canonical
production load; arbitrary-load provenance is not required under approved v3.
No unresolved product defect or technical acceptance blocker is established.
Keep non-done until the remaining human evidence is reported and assessed.

## Next action

Stop engineering work here. Resume the deferred checklist under Remaining when
a suitable audio fixture and human verification are available. If those checks
pass, record the actual results and reconcile final completion/canonical validation.
If they reveal a concrete defect, investigate only that dependency cone. Do not
build generic fixture/import infrastructure or rerun controlled decoder capture,
corrective r2 or completed M1-M5 work merely for confidence.
