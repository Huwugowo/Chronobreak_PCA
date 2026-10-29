# QB-REPLAY-010 execution checkpoint

Feature: `QB-REPLAY-010`
ExecPlan: `docs/exec-plans/qb-replay-010-replay-opening-acceptance.md`
Updated: 2026-10-29

## Current milestone

Integrated opening campaign discontinued by explicit product decision on
2026-10-29. Performance acceptance is inconclusive/deferred; no aggregate observer
or current full-feature performance gate passed. Status blocked means deferred
acceptance, not an established product defect. No active implementation unit.

The dated history below is preserved and superseded wherever it calls for
benchmark continuation, repair, replacement or further acceptance work. Closed
library/descriptor/UI units and their evidence remain accepted.

### Historical milestone context (superseded)

Integrated replay-opening acceptance M1 completed on 2026-10-12. The versioned
candidate/reference workload, strict same-clock extraction, frozen-campaign
acceptance and parity/regression tests pass. M2 source/subject freeze is complete
and fresh corpus preparation/assembly completed on 2026-10-13.
Three measurement attempts remain closed and preserved (identity admission,
Windows output-path finalization, then shared capability-cache state). R4 now
freezes private state per process with unchanged subjects/media and strict gates;
all 116 fresh static preflights passed. R4 controls then closed on incomplete
process telemetry in launch 16; no main measurements have run.
QB-REPLAY-010 remains in progress with its unchanged
acceptance criteria and no current performance or native-acceptance claim.

The source baseline is clean `main` at
`a82d81f005823768a21b8aef4a94bd954ce26903` before this documentation handoff. The
accepted reconciliation is committed as `a840c64` and included in consolidation
`13e68091` with the later UI line `bb9041e`; a82d81f changes documentation only.
The earlier instructions describing an uncommitted merge and a live sibling
checkout are historical, not current repository state. No commit or push is part
of this new plan. The September 27 integration record is
`docs/reviews/repository-consolidation-2026-09-27.md`.

Library V2/M4, minimal descriptor and UI reconciliation units remain closed.
Their accepted measurements/native observations are historical input, not proof
that the later consolidated tree passed every full-feature gate. Semantic details
already load after descriptor-first media; a new semantic backend is not an
outstanding implementation requirement. The remaining unit establishes integrated
opening performance, current normal-build acceptance and resource dispositions.

### Closed UI reconciliation unit

UI reconciliation M1-M3 completed on 2026-09-23 under
`docs/exec-plans/qb-replay-010-ui-reconciliation.md`. It combined root `3426cfa`
with UI `6c7174f` and five captured CSS edits, with automated/build/review and
user-operated native acceptance. At that handoff the merge was uncommitted;
subsequent committed consolidation is described above. Original patches, receipts
and native source audits remain under `build/qb010-ui-reconciliation/`.

### Closed descriptor unit

Minimal replay descriptor M1-M3 complete, 2026-09-22. The separate immutable
descriptor plan was finalized before implementation from checkpoint
`3b14df20aa7ab4e567fd0372c1abc72c07bbe18e`. Backend ownership and descriptor-first
viewer/navigation pass the required automated suites and normal/benchmark builds.
User-operated native acceptance passed on the freshly rebuilt normal subject and
dedicated strict-v2 A/B fixtures: real A/V, controls, held/failed/late details,
retry, clip preservation, stale navigation and overlapping-ID roots. The final
ungated Ahri check passed; the app and diagnostic listener are closed and all
source hashes remain unchanged. No descriptor defect required a product change.

QB-REPLAY-010 remains in progress for separately planned remaining replay scope
and full-feature performance/manual gates. No descriptor latency/I/O improvement
or full-feature completion is claimed. The closed V2/M4 library unit and its
immutable plan remain unchanged; no campaign or disposition was reopened.

### Closed library unit

Attribution M1-M3 concluded with a material library cause. Reviewed v2 library
implementation design is finalized; v2 M1, M2 and the requested frontend M3 are
implemented, verified and independently reviewed. M4's fresh replacement completed
all 50 valid launches and deterministic verification. N/E Games-usable gates pass
and S has no material regression. The four requested historical/resource/reliability
questions now have explicit evidence-backed dispositions with no production change
warranted. The V2 library implementation/benchmark unit is closed. User-assisted
Windows acceptance passed on disposable fixtures, including mutation, A/B/A root
ordering, export, cleanup, source preservation and optional-work overlap. M4 has
no remaining blocker. QB-REPLAY-010 as a whole remains in progress for its separate
descriptor, timeline and playback scope. The immutable attribution plan remains at
`docs/exec-plans/qb-replay-010-measured-library-and-replay-opening.md`.

## Active unit

None. Stop r4 telemetry diagnosis and the large benchmark campaign. No
replacement/top-up/main measurement or infrastructure repair without a specific
product-driven justification. No new product defect is established by the
incomplete benchmark telemetry.

### Historical campaign handoff (superseded; do not execute)

M2: r4 has frozen 22 matrices and 116 launches with unique pristine app-data and
scratch roots, matching declared input fingerprints and 224–230-character atomic
output paths. Review found no actionable issues. All 116 static preflights passed;
their final summary was completed from the intact successful receipts after the
original exec session ended. No preflight or measurement was repeated. R4 controls
1–15 finalized successfully. Launch 16 completed the app scenario (exit 0, no
timeout, source hashes unchanged, 66 process samples) but telemetry completeness
failed at 13:28:13 UTC. Session 54191 ended with exit 1; controls/016.json,
STOPPED.json and phase-failed.json close collection. No aggregate observer gate
or main launch is accepted. Diagnose the incomplete sample from preserved raw
evidence; no automatic retry or replacement.
The first corrected reference pair completed with one HEVC request in each arm
and identical semantic digest 4d45a75884a6d99f0c305d19a75734f6fd2118e04f0275a86ec81671b0aa5796.
This proves the cache-state correction for that pair, not aggregate acceptance.
Manual helper source is staged under build/qb010-opening-manual-20261013 using
the frozen normal candidate and generated S source. It has not been tested,
prepared, decoded or launched; defer media work until measurements stop.

R3 short-path replacement passed all 116 static preflights and focused
review (no actionable findings). All 16 controls finalized individually, proving
corrected identity admission and atomic terminal finalization. At 08:43:24 UTC,
the reference minimal/full aggregate failed semantic outcome equality in
validate_observer_control. controls/phase-failed.json and STOPPED.json close r3;
exec session 29899 ended with exit 1. No main measurement was launched and no
aggregate observer gate passed. The preserved diagnostic proves shared capability-cache state; prepare the
separately recorded r4 replacement with private state per process.
All four campaigns remain closed and unpooled. Reuse the normal candidate,
second-screen benchmark builds and accepted fixture/decode evidence only when
their dependencies remain valid. The r3 path budget is 224–230 characters for
all 116 atomic terminal files.

Replacement setup is now built/frozen; all 22 matrices and its 116-launch order
are fixed. The first 116 replacement static preflights passed. A reviewed
prelaunch guard correction is now frozen as a separate r2 tooling revision; its
all 116 static preflights passed (replacement-v2/preflight-r2/complete.json).
The first replacement control completed the app action window but failed runner
finalization at 08:11:35 UTC: WriteAllText rejected the long atomic terminal path.
controls/STOPPED.json closes this replacement after one attempt. No later launch
occurred; diagnostic investigation is active and no app process remains. Both paired
config-only builds passed and freeze the title-bar/decorations/maximize settings
before WebView creation; no new live launch has occurred. Candidate binary SHA
`ee54301c01439cdbfb8983e02b171350167ede662bbaa5f941ca6dacaeee8ded`,
reference `ae0cee56832a1922bc285790f9916cbcef1f00c5067ac609c7a5c23a706a5164`.
Their command/build/source receipts live under `replacement-v2/`; archive paths
are `build/qb010-opening-{candidate,reference}-secondary`. Source patches reverse-
apply cleanly. Fresh canonical input hashes authenticate 421 files/provenance
records; original media decode proof is reused, not rerun. Input/display guard
tests pass 4/4 (0.041 s), retained runner fail-stop tests pass 8/8 (0.480 s), and
opening analysis tests pass 17/17 (7.039 s), including action-count aliases and
planning from raw declarations. Design review found no actionable findings;
focused runtime-guard review found the display check preceded the cooldown.
The corrected run_campaign_r2.py checks immediately before subprocess launch,
inside the persisted failure boundary. Ten guard tests pass (0.368 s), including
call order and changed-display rejection without launch. Original helper,
certificate, order and preflight artifacts are preserved; input-reuse-r2.json and
specs/global-order-r2.json freeze only this tooling correction. Every subject,
manifest, launch sequence and threshold is unchanged, before live replacement
launches; preflight-r2 receipts bind the revised tooling/order.

### Replacement disposition and design (2026-10-13)

Launch 1 passed the generic runner, but frozen input admission compared the raw
scenario with an analyzer-normalized scenario containing an injected null action
count. Binary, config and original/result manifest hashes match. The user also
reported interacting with its windows. Exclude the attempt, retain its STOPPED
receipts, and pool no observations into a replacement. The correction canonicalizes
only action-count normalization; explicit numeric counts remain identity-bound.
Focused tests cover raw/normalized declarations and a full synthetic campaign.
`diagnostics/first-control-identity.json` retains the original mismatch and the
116-declaration diagnostic (not a performance acceptance).

The user requested movable bordered windows on their second monitor. Rebuild
only the two benchmark subjects in new source archives with identical Tauri
window configuration: `center:false`, logical `x:2304,y:64`, `maximized:true`,
`fullscreen:false`, `decorations:true`, `resizable:true`. Keep ordinary application
source and the frozen normal candidate unchanged. Native monitor enumeration
identified non-primary DISPLAY1 at physical (1920,0)-(3840,1080), work area ending
at y=1042, scale 125%; primary DISPLAY5 is (0,0)-(1920,1080), scale 100%.
The chosen logical point maps inside DISPLAY1 and outside the primary at both
monitors' respective scale. Local Tauri/tao applies position and maximize during
window construction before visibility/WebView timing. No post-launch move/resize
hook or hidden/minimized measurement is used. Record and verify topology before
collection; changed topology closes the phase before another launch.

Use a fresh `replacement-v2/` namespace inside the existing sentinel, with its
own builds, source patches/receipts, campaign/order, unique run IDs, matrices,
results, appdata and scratch. Share only the accepted read-only S/R/L/N/E corpus,
canonical no-retention configs and runtime bytes. This is explicit input reuse,
not another preparation claim: hash-bind the original preparation completion,
accepted manifests/receipts, current fixture/config/tool hashes, monitor policy
and analysis/runner versions in a replacement input certificate and frozen order.
Do not copy or fabricate producer receipts or redo accepted full media decodes.
Fresh matrix verification and all 116 static launch preflights remain required.

Keep the original protocol: seed 20261006, four adjacent minimal/full 60-second
R control pairs per subject, then ten cold/warm strata with five independent
processes per arm, six warm cycles, fixed 116-launch ceiling, alternating pair
leaders, five-second warmup/cooldown, original timeouts, unchanged thresholds and
fail-stop rules. The replacement is a separate collection with no pooling,
selective top-up or automatic retry. The configured display is a measurement
condition applied equally to both subjects, not a product optimization.

### Isolated-state replacement disposition (2026-10-13)

R3 completed all 16 native controls individually, then closed on semantic outcome
inequality. Preserved diagnostic `r3/diagnostics/observer-cache-failure.json` binds
all raw manifests/events/requests. Each subject's eight controls share one
app-data root: its first minimal process performed one HEVC capability request;
its subsequent seven performed none. Both subjects have identical action targets,
generations and ordered action events across each pair, with no error/recovery
difference. The backend writes `hevc-probe-v1.json` under the declared app-data
root; WebView also uses that root. This is inconsistent starting state, not a
playback defect or an analyzer false positive. Keep strict request-class identity
and all thresholds unchanged. All 22 numeric observer thresholds are individually
within their existing limits on a diagnostic calculation, but semantic failure
prevents accepting either observer matrix. No main run occurred.

Correct only matrix expansion/preparation: the optional `isolate_launch_roots`
contract derives app-data and scratch children by immutable run ID, with each
child's own offline ddragon cache. This small mechanism already exists in a
historical frozen planner; reuse its focused behavior, not that campaign or its
other tooling. Defaults remain backward compatible. All 116 processes in a
separate `r4/` order receive independently prepared pristine state; every warm
process still keeps its six cycles together. At preflight and immediately before
launch, reject app-data containing anything except the declared empty offline
`ddragon` directory, and reject nonempty scratch. Validate global uniqueness and
containment, retaining reparse checks. Never erase/reset a used profile or seed a
capability result to obtain equality.

Reuse the byte-identical second-screen subjects, accepted media/config/runtime,
full decode evidence, display policy, protocol/seed/ordering/counts/timeouts and
path budget. No product rebuild or media regeneration is invalidated. Freeze new
manifest/order/tool/input receipts and complete fresh static preflights before
measurement. The previous three campaigns remain closed, preserved and unpooled;
this separately recorded replacement addresses the proven preparation defect.
Matrix isolation tests pass 18/18 and analyzer tests 39/39, including a regression
that a HEVC request class in only one arm still rejects controls. Full Python
verification passed: 116 tests run, 115 passed and one existing opt-in skipped
(38.501 s; build/qb010-opening-isolated-state-tests.log). R4 launch guards passed
13/13 and input/display guards 4/4. Focused preparation/launch-guard review found
no actionable issues. The new input certificate authenticates 426 original input/
provenance files; all 116 fresh static preflights passed. No media was copied or
decoded again, and no app code or analyzer threshold changed.

### Short-path replacement disposition (2026-10-13)

Replacement-v2 control 1 completed its app window (exit 0, terminal complete,
67,337.15 ms) but its runner's atomic terminal rewrite generated a 265-character
path and failed in Windows PowerShell 5.1 File.WriteAllText. The result directory
and original terminal exist; this was not a missing parent. The analyzer and
runner-result receipt were never reached, so no timing observation is accepted.
Its STOPPED receipts and all other evidence remain immutable. Among its frozen
manifests, 108/116 atomic paths exceed 259 characters; do not resume that order.

The existing runner now reserves the actual atomic terminal suffix in both
preflight and normal validation, rejecting paths of 260 or more characters before
launch. The same partial-path helper serves validation and writing; atomic
publication and integrity checks remain intact. Dedicated temporary tests verify
259-character acceptance and 260-character rejection with no app launch. All 15
PowerShell contract tests passed (29.821 s). Full benchmark Python verification
then ran 113 tests: 112 passed, one opt-in test skipped (46.624 s). R3 wrapper
guards passed 10/10 (0.490 s), input/display guards 4/4 (0.032 s); all 116
normalized input fingerprints match their frozen declarations. The r3 reuse
certificate authenticates 426 input/provenance files, including the original
second-screen build receipts.

Create a separate `r3/` campaign namespace inside the same sentinel with shorter
matrix/run IDs. Reuse the byte-identical second-monitor binaries/source patches,
their original successful build receipts, and all accepted preparation evidence.
Fresh appdata, scratch, matrices, result paths, input/tool certificate and frozen
order remain mandatory. Validate every projected atomic path and run all fresh
static preflights. No rebuild or media decode is invalidated by this filename
correction. Protocol, counts, thresholds, display policy and no-pooling/fail-stop
rules remain exactly those recorded above. This is the separately recorded
replacement for the output-path defect, not a selective retry of the failed run.

M1 is complete: candidate changes are in `app/src/App.tsx`, `benchmark.ts` and
`components/ViewerScreen.tsx`; Python validation is in `analyze.py` and
`opening_acceptance.py` with strict campaign/report schemas. Reference source is
`build/qb010-opening-reference` at archived `3a750e2`; its reproducible three-file
patch and test-only parity fixtures are in
`tools/replay_benchmark/reference/opening/`. The durable benchmark protocol is
updated. Do not recreate closed product units or rerun accepted M1 groups unless
subsequent input changes invalidate them.

The new workload opens after actual Games paint/admission, excludes only the
candidate's benchmark-only all-clip drain, and permits media actions before
optional details while still requiring admitted details for a positive terminal.
The new extractor must not reuse payload-before-mount or cross-clock causal
assumptions. Five independent processes per arm/stratum and separate warm initial/
remount observations are mandatory. No native measurement launch before M1 tests
and M2 subject/fixture/preflight gates.

No UI merge, descriptor repair, M4 rerun or restoration of later sibling/stash
changes is needed. The accepted original units and their evidence remain below.

### Historical closed M4 execution

M4 closed 2026-09-21. The user completed the disposable normal-build sequence:
Games remained usable through an unavailable optional duration; save and clip
deletion occurred during a verified optional child hold; one unsaved recording was
deleted; A/B/A switching showed B's one Lux game/clip and restored A's intended
two-game/one-saved state; source and exported media played; cleanup retained the
saved game and clips; and export submission occurred during verified optional work.
`hold-export2.json` verifies the direct packaged `ffprobe` child and parent identity
with no watcher errors; the user confirmed **EXPORT 1 MP4** while HOLDING. The
retained `audit-export-overlap-verified.json` validates four generated H.264/AAC
exports, their selected durations and full decodes; it shows only deliberate A
fixture mutations and unchanged B. `audit-final-verified.json` records the final
one saved game, retained clips and restored Never retention. The helper's receipt
does not infer UI correctness; these observations are explicit user confirmation.
No benchmark was rerun and no production change was made for M4 closure.

2026-09-21 manual harness follow-up: user launched normal subject PID 1192 and
reported Clips ready, but `hold-roots.json` ended `not-observed`; folder switching
is not accepted. The old receipt contains no rejection details, so the exact
cause is unresolved. Standalone observation caught 10/10 fast invalid-media
probes (26-39 ms lifetime) plus two delayed controls; this does not reproduce or
explain the specific user attempt. No product defect or cache/retry failure is
established. A scout's active-batch retry hypothesis was not adopted: the UI
publishes batch results after the whole response, then clears the active batch
synchronously, so it did not establish the claimed user-click window.
The manual helper now resolves the expected probe path once, retains bounded
candidate/open/identity/path/exit/suspend diagnostics, propagates receipt-write
errors through its resume guard, and reports a failed hold without a traceback.
Path/parent verification is unchanged; the disposable helper permits a 30-second
manual window. Four focused rejection tests pass; replay Python ran 95 tests successfully (one
existing opt-in skip), py_compile/help and git diff --check passed. A standalone
owned packaged-child smoke proved suspension and successful resume; no UI pass
is inferred. Diagnostic sources/results: `build/qb010-m4/manual_watch_diagnostic.py`,
`manual-watch-diagnostic.json`, `manual_hold_smoke.py`, `manual-hold-smoke.json`.
This dated diagnostic record is superseded by the completed user-assisted sequence
above.

2026-09-21 resume: git HEAD remains `3a750e2`; the accumulated M1-M4 changes
are not yet committed. At that resume audit no app was running; launch/log slots were available
after preserving the closed PID 8328 receipts. There is no `hold-roots` receipt:
folder switching is the next unperformed check. `audit-resume-20260921.json`
reconfirms A has two recordings, one saved and nine clips, with the same
intentional deletions and saved metadata change; B is unchanged. Do not repeat
the benchmark or already accepted manual actions. Launch the normal subject from
the user's visible terminal, continue folder switching, then export/return and
cleanup, followed by final verification/documentation and the requested commit.

User-assisted acceptance update, 2026-09-17: the user launched the frozen normal
subject on their visible desktop. Initial Games, local optional failure/retry and
valid clip playback were reported working. The `games` hold caught a child, but
its navigation timing was not confirmed. The later `save` hold and explicit user
confirmation establish Games navigation/save during owned optional work: three
recordings, one saved, ten clips. `audit-save.json` confirms the saved metadata.
The user subsequently confirmed intentionally toggling SAVED several times;
the intermediate unsaved state is explained, not a reproduced regression.
The first `delete` attempt caught no child and remains failed test setup;
`delete2`, triggered by Retry duration, caught a verified child and the user
reported the instructed deletion sequence successful. Its child exited during
the hold; the audit confirms only source-less clip `1789400108_1789500008.mp4`
was removed. The user then saved the first recording and deleted a different
unsaved recording: two recordings, one saved, nine clips were explicitly confirmed.
`audit-recording-delete-verified.json` confirms only game `1789400101` was removed,
game `1789400102` is saved, retained media/log hashes match and B is unchanged.
Receipts are under the existing `manual-assisted-20260917` disposable workspace.
Those formerly remaining checks subsequently passed; this dated partial record is
superseded by the 2026-09-21 closure entry above.

2026-09-17 resume: collected the explicitly requested fresh 50-launch M4 replacement
under `campaigns/replacement-20260917-r1` in the existing M4 sentinel. Before-launch
disposition: `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/disposition.md`.
Exactly 25 new pairs; no old results pooled, no sequence-16 resume or selective
top-up. Old ceiling stays closed; new ceiling 50, lifetime maximum 65 attempts.
Frozen subjects, fixture/config bytes and measurement semantics stay unchanged.
Replacement collection completed at 2026-09-17T08:49:14.885530Z: 50 attempted,
50 valid, zero invalid; ten required-result matrices passed. Durable terminal:
new campaign `live/complete.json`, with all 50 started/completed receipts.
The former tool session expired after completion; receipts and strict bundle
validation establish completion. Never restart the one-shot runner. All 50
preflights, 10 plan verifications, normal
and benchmark desktop builds, frontend 107 tests, Rust 86 tests (one opt-in
ignored), replay Python 91 tests (one opt-in skipped), fmt/Clippy and reference
parity 2 tests passed. Receipts: `build/qb010-m4/checks/r2-*` and new campaign
`preflight/complete.json`. One reference test invocation used the wrong filename
and found no tests; the corrected exact retained parity test passed. Another
checkout's development app remains untouched, sampled at zero CPU over three
seconds, with its Cargo parents waiting. Proceed with that declared background
condition under the requested campaign authorization; retain whole-system CPU.

First V2 M4 campaign disposition: the fresh frozen campaign attempted 15 of 50 launches on
2026-09-16. Runs 1-14 are individually valid; run 15 is incomplete/invalid, with
no terminal, collector-finalization or post-hash evidence. On 2026-09-17 resume,
no campaign process remained. Do not resume the runner or launch 16 onward.
Preserve all artifacts. Every stratum has fewer than five valid processes per
arm, so the comparison cannot pass. Normal-build startup was observed; subsequent
interaction was environment-blocked at that time and later passed with user assistance.
Canonical evidence is retained at
`docs/performance/evidence/qb-replay-010-m4-20260917/`. M1/M2/M3 remain accepted.

## Completed

- 2026-10-12 opening acceptance M1: implemented the H.264 Games paint/current-origin
  gate without optional duration/HEVC/Data Dragon waits; preserved legacy gates.
  Candidate playback/actions can precede full details, but every positive cycle
  requires successful details admission. Failure/timeout/disposal stay terminal.
  Actual controller load generation, cycle and fixture bind bounded events and
  playback actions. Strict extraction rejects missing/duplicate/stale/mixed-clock
  evidence and keeps warm initial mounts separate from five-remount medians.
- Frozen-reference parity: eager games/clips/usage/settings and full-probe-before-
  media behavior remain intact. Seven App and two Viewer parity tests pass within
  the 96-test reference suite. The production patch SHA-256 is
  `bdea824fe36b8bffc4e613cc83518cffb2352ca4efe3903c6e8a7a9ea4f2c157`.
  Applying it to exact archived source reproduces all three tested files byte for
  byte (`build/qb010-opening-reference-patch-check-r2`).
- Python contract tests exercise all 116 synthetic raw bundles through the real
  validator/control/comparison pipeline, all 70 primary comparisons, malformed
  events, absent primaries and source/patch/binary/config/runtime/role mismatch.
  This is tooling correctness evidence, not measured performance. Frontend and
  analyzer reviews found generation/action association gaps; those were fixed
  and the affected tests pass. No unresolved M1 review finding remains.

- 2026-10-06 source reconciliation and next-unit planning: confirmed clean main
  a82d81f and committed reconciliation/consolidation ancestry. Reviewed a bounded
  `replay-opening-v1` workload and actual-before/current comparator, with full
  feature fixture coverage and fixed collection ceilings. Linked the separately
  finalized plan and initialized its first implementation cursor. No product
  implementation, benchmark, native acceptance or commit occurred in this pass.

- UI reconciliation M3 (2026-09-23): final full affected automated checks, normal
  and benchmark desktop builds, focused review and native acceptance passed.
  User-operated tests cover real A/V, seek/zoom/pan, clip edits and fullscreen
  mid-drag under held details, local failure/retry, late admission without media
  reload, semantic filters, normal editor/return, immediate/persistent Star state,
  Garen selection and overlapping-ID A/B/A. Gate restored; no pending requests;
  normal subject and scoped listener closed. Only Ahri save metadata reserialized
  (optional capture absent → null, saved remains its original true); all video/log
  hashes, all B files and generated source are unchanged. Work was uncommitted
  at that handoff; the later committed consolidation is recorded above.

- UI reconciliation M1-M2 (2026-09-23): retained captured UI/CSS work with existing
  controller ownership; added required strict roster parity, coherent development
  mocks matching the physical preview MP4, and optimistic save display overlay.
  Shared rail cleanup handles pointer identity, cancellation and disposal. Focused
  backend/frontend regressions pass, including fullscreen mid-drag and benchmark
  media/details arrival ordering. No extra roster filesystem read was introduced.

- UI integration planning: user confirmed the latest sibling line beyond named
  `ui/viewer-foundations-v1` (`82d6704`): participant summaries (`783e4bb`) and
  redesigned cards (`6c7174f`). Common ancestor is `e030161`. A read-only three-way
  merge dry run found App/API/LibraryScreen conflicts; no merge was applied.
  Root verified that the tentative Viewer merge retains descriptor-first resources
  and that summary roster derivation reuses already parsed logs. Incorrect scout
  assumptions about a no-log summary and lost descriptor were rejected against code.
  Fresh independent plan review requested explicit optimistic overlay lifecycle,
  strict roster boundary coverage and stable late-details media/viewport tests;
  all are specified in the finalized plan. A separate read-only scout confirmed
  the additional five CSS-only sibling edits and their dependency on MatchHistory.

- Descriptor M3: required automated verification and normal/benchmark builds
  passed. User-operated native acceptance passed on the fresh normal subject:
  held/failed/late optional details, explicit retry, A/V and ordinary controls,
  unchanged clip drafts/media, stale navigation and overlapping-ID A/B/A roots.
  The user confirmed the final ungated replay and app closure. Final audit shows
  no added/deleted/changed fixture source files; subject and listener are gone.

- Descriptor M1: four-field media projection from the unchanged strict-v2 bundle
  reader; missing/nonregular/empty video rejection; selected-token command with
  one owned blocking slot shared by the unchanged full probe. Worker ownership
  survives waiter drop; refresh/mutation/root A/B/A/shutdown reject late results.
- Descriptor M2: app-owned one-active/one-latest read scheduling, fresh viewer
  navigation origins, descriptor-first persistent media surface, optional full
  details after a paint opportunity, exact identity/time admission, local failure
  and retry, reactive existing stats/events without resetting playback or clip
  selection. Exporter and benchmark callers use the same tokenized read owner.
  Historical full-payload markers retain their meaning, not descriptor readiness.
- Deferred tests prove useful playback with held/failed details, unchanged primary
  element/source/load count/rate/clip draft after late admission or retry, unmount
  and descriptor retry, stale result rejection/coalescing, held assets/durations,
  Settings/exporter returns and overlapping-ID root changes. No ReplayTimeline,
  persistent ReplayIndex, payload split or delivery redesign was introduced.

- M4 replacement: all 25 adjacent pairs completed, five processes per arm in
  every required stratum. Reference/candidate/source/runtime identities, exact
  configs, all 409 fixture hashes and prior raw artifacts match. Reports,
  same-clock opening/resource details, matrices and provenance are retained in
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/`.
- Replacement cold Games medians R/C: S 166.4/168.2 ms (non-regression),
  N 2092.6/197.7 ms (90.55% faster), E 2386.6/493.3 ms (79.33% faster).
  Warm-process startup N 2087.3/209.7, E 2404.8/483.6 ms also passes the
  Games improvement gate. Five-remount per-process replay aggregates remain
  separate; no material replay first-frame regression was observed.
- Disposable assisted roots were prepared and baseline-audited after collection:
  initial A three Ahri games/ten clips; B one Lux game/one clip with overlapping
  IDs. Subsequent user-assisted observations and mutations are recorded above;
  the current A state is two games/one saved/nine clips. Remaining observations
  are in replacement evidence `manual-acceptance.md`.
- Mandatory product/workflow/feature/checkpoint/plan/verification bootstrap,
  forward-engineering and Rust skills; relevant time/security architecture.
- Earlier planning handoff at merged `3a750e2` reconciled optional 015 and replay
  ordering, preserving 009's deferred human gate and 011/012 evidence. Preserve
  those unrelated working-tree documentation/roadmap edits.
- Frozen benchmark app/runtime at source
  `3a750e2c003d6e3605b2f907ff19ad7898d3dbd2`, with zero product behavior diff.
- S/R/L strict-v2 corpus and N/E fixed-media/identity 50-game/50-clip libraries;
  all 203 staged media files passed packaged probing and full single-thread decode.
  Corpus size 2.198 GiB under the 8 GiB bound before result collection.
- All five immutable plans and 30 runner preflights passed. All 30 launches were
  attempted in seeded order; 28 are valid and invalid 14/18 remain preserved.
- Added `opening_attribution.py` and synthetic tests. Extracted 53 frontend cycles
  from 28 validated runs: 23 cold, five initial warm mounts, 25 warm remounts.
- Completed the one narrower diagnostic: aggregate timers on actual `list_clips`
  in one optimized native test process, N/E/E/N/N/E. Six calls each returned 50
  clips with positive durations. Removed timers; restored library.rs byte-for-byte.
- Sanitized measurements, diagnostic patch/binary/log identities, launch/artifact
  hashes and disposition retained at
  `docs/performance/evidence/qb-replay-010-20260915/`.
- V2 independent review required explicit primary event validity and secure probe
  input. Applied both; fresh bounded amendment review passed. Existing capability
  routing preserves opened-handle containment; versioned events preserve old reports.
- V2 M1: `library.rs` builds one snapshot with one summary/video-size read per
  accepted game, one clip enumeration, same-scan enrichment/accounting, storage-only
  invalid-name MP4s, and explicitly unknown durations without probes. Strict v2,
  incomplete cards, ordering and source-less clips retain their semantics.
- `library_coordinator.rs` owns one blocking scan slot and one complete snapshot.
  Busy admission never queues; cancellation/shutdown retain the worker permit;
  stale revision/root results (including A/B/A) cannot publish. Same-root scan and
  worker errors retain the previous view; a successful refresh always rebuilds.
- AppState owns the coordinator and exposes the real async `refresh_library` in
  both builds. MediaRoots clones path/ApprovedRoot under one read guard; capture
  and settings publication use coordinator-then-MediaRoots ordering. Settings
  persist before root/revision publication. Independent M1 diff review found no
  actionable issue; root review reconciled worker join failures into the same
  revision/failure path. Durable media-library architecture updated.

- V2 M2: optional duration command accepts at most eight distinct current clip IDs,
  runs one owned blocking batch/child, caches only current-snapshot success or local
  unavailable state, and supports explicit failure retry. Child stdout is capped at
  4 KiB with a ten-second deadline; cancellation/timeout/error kill and reap it
  before releasing the slot. Stderr is discarded. Blocking metadata checks reuse
  QB-REPLAY-011 validated handles and record volume/file identity, length and mtime.
- Probes use only the existing protected clip HTTP route and packaged ffprobe,
  forced MOV demuxing, external/absolute references disabled and `http,tcp` only.
  Replaced/deleted files and stale tokens cannot publish durations. Refresh and all
  invalidations clear optional results. No new delivery route or authority.
- Save/delete/retention and settings use one owned mutation slot off async workers.
  Selected tokens/root/IDs are checked at admission; stale/missing selections reject.
  Completion guards invalidate even on partial errors, panic or dropped waiters.
  Failed settings persistence retains the published root/revision. Export keeps its
  original destination; current matching-root views invalidate even after A/B/A,
  while original epoch/token gates reject old response admission.
- Independent full M2 diff review found no actionable issue. Root then corrected
  matching-root invalidation after export A/B/A or same-root settings; a fresh
  focused independent review found no actionable issue. Relevant media-library
  architecture is updated. All destructive tests use disposable tempfile fixtures.

- V2 M3: `LibraryController` is the sole frontend source for the core snapshot,
  selections, storage usage, mutations, refresh coalescing and optional duration
  state. Same-root refreshes retain cards but make them display-only until the
  new token is admitted; root changes clear cards/selections immediately. Request,
  root and navigation epochs reject late A/B/A and superseded responses while
  completion paths still invalidate the captured logical root.
- All save/delete/retention/settings/export actions carry the originating token.
  Settings and exporter completion reconcile filesystem effects even after stale
  UI responses; export admission retains the backend mutation permit through
  completion. Clips request at most eight visible IDs with one active and one
  coalesced pending intent; unavailable duration is local and retryable, and
  playback remains independent of duration.
- Games render from the core snapshot without waiting for durations or Data
  Dragon. Benchmark-only duration draining preserves historical `library_useful`
  semantics, while `games_library_usable-v1` records the post-paint Games gate.
- Fresh adversarial review found benchmark useful timing, retained-view error
  presentation, return-to-library refresh, and Settings-to-Clips visibility gaps;
  all four were fixed with focused regression coverage where practical.

- The first M4 campaign froze a minimally instrumented reference and corrected
  candidate after deferred App/reference condition-parity tests. It obtained 14
  valid runs, then an incomplete invalid attempt at sequence 15; its rejected
  matrices and all artifacts remain preserved separately.
- The replacement M4 campaign retained sanitized individual/aggregate reports,
  warm initial/remount observations, exact source/binary/artifact identities,
  passed all ten matrices with 50 valid launches, and confirmed all 409 prepared
  files still match their hashes. These are fresh observations, not reused
  attribution results; primary Games-usable gates pass. The complete engineering
  disposition is `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/engineering-disposition.md`:
  working-set and I/O costs are bounded benchmark-only consequences, handle growth
  is a process-sampler baseline artifact, and cancellations are HTTP stream
  teardown with no child-failure evidence. No production change was warranted.
- A separate frozen normal candidate opened disposable root A and showed its
  three Games cards and expected storage totals despite optional asset errors.
  Root B with overlapping IDs was prepared. The initial autonomous capture/pointer
  attempt failed, but later user-assisted acceptance superseded that dated state;
  its retained receipts/audits are recorded in the current milestone above.

## In flight

None. The following preparation and campaign narrative is retained historical
evidence; it does not authorize more collection or describe current processes.

M2 corpus preparation is complete. S/R/L passed the original packaged probe/full
decode. Both bulk N/E preparation attempts were interrupted without final
receipts; their partial libraries, started receipts and logs remain preserved.
The first N attempt copied 36 bundles; the second copied 17 N and 17 E bundles.
None supplies accepted decode evidence. No benchmark app was launched.

`specs/prepare_incremental.py` now invokes unchanged `prepare.ps1` per fixture in
isolated, immutable attempt directories under `fixture-preparation/{n,e}/`.
All 102 fixture groups passed (100 recordings and 100 clips), with finalized
spec/manifest/receipt hashes and full single-thread decodes. Resume revalidated
accepted groups without repeating their decode. The initial scale-00 specs failed
before copying because result-root leaf differed from run ID; corrected that
adapter and the corresponding planner template path, preserving failed receipts.

N/E assembly completed at 2026-10-13 07:26:13 UTC. Accepted manifests are
`assembled/{n,e}/attempt-001/manifest.json`; each has 200 verified files and its
own receipt plus atomic complete marker. Assembly rechecks every contributing
source/staged/final byte hash, size, recorded timestamp, exact fixture/decode
coverage, isolated config/cache and runtime identity before publication.
`preparation-prepare/complete.json` atomically binds those manifests and retained
S/R/L by hash. Interrupted assembly directories remain unaccepted and resumable
work never overwrites evidence. Four copied-fixture regressions pass for complete
reuse, one-sided interruption, post-copy mutation and interrupted atomic commit;
fresh focused review found no remaining actionable finalization issue.
No preparation worker remained at the 2026-10-13 resume inspection.

Fresh sentinel: `build/perf/qb-replay-010-opening-v1/.chronobreak-replay-benchmark`.
`source-freeze.json` records exact archives and three-file patches; candidate
source is `build/qb010-opening-candidate`, reference `build/qb010-opening-reference`.
Normal candidate build SHA-256:
`4105e50b1f8712f75cd021cf22bb8fe87c6bc72931f7e591282753c3e59cdd2f`.
Benchmark candidate:
`441c2d5721389bbd1ed154d5a3f3ed3e352441a016cb87a959205339c33befb9`.
Benchmark reference:
`a71dba892df504d69b80b046ac19fe44e38bfd208731ab9981a1f46e513d7184`.
Each is independently rebuilt through Tauri, copied under `subjects/` with the
locked runtime, and linked by `candidate-normal-build.json`, `candidate-build.json`
and `reference-build.json`. Normal was frozen before benchmark overwrote target.
The benchmark receipts hash all tracked app/media-runtime/replay-time source files
and packaged files. Reused Cargo cache is not claimed as an independent toolchain.

`specs/prepare_sources.py` verified retained generated S/R/L source identities and
created independent new copies; only new L log was densified through the strict-v2
helper (3,917 events/3,600 snapshots). N/E have 50 games/50 identical ten-second
clips, matching media/IDs/metadata; logs retain 7/8 and 259/240 events/snapshots.
`source-preparation.json` hashes every generated file
and retains source hashes/clip command. Generated bytes: 1,121,445,584; conservative
prepared corpus forecast: 2,511,326,624, below 8 GiB. Old roots were read-only.

`specs/plan_campaign.py` and `run_campaign.py` reuse the existing matrix/runner and
have passed syntax compilation; the campaign/order and 22 matrices are frozen.
All 116 static runner preflights passed at 07:33:21 UTC on 2026-10-13;
`preflight/complete.json` binds their count and frozen order hash. The controls
phase started at launch 1 at 07:36:46 UTC, using native process accounting outside
the restricted sandbox. The runner exited 0 (clean exit, unchanged fixture hashes
and generic analyzer acceptance), but campaign admission failed with Frozen input
or subject mismatch at 07:38:00 UTC. controls/001.json and STOPPED.json preserve
the failure; no further launch occurred. The user reported interacting with the
benchmark windows, independently excluding the attempt from unattended timing
evidence. No league-replay-app process remained after runner completion.
Main collection remains unrun; no automatic retry, top-up or replacement.
A runner review found resume-gap, exception-receipt and cross-matrix cooldown
issues. The wrapper now audits all 116 slots before collection, requires one
contiguous finalized-success prefix, persists execution/admission/finalization
failures without overwriting evidence, and checks five-second cooldowns across
all live receipts including the controls/main boundary. Eight focused mocked
failure-path tests pass; a fresh read-only review found no remaining actionable
issue in the changed fail-stop scope.
Planner creates 22 matrices (20 main, two controls), 16 control and 100 main
manifests with seed 20261006, adjacent/alternating pairs and five-second cooldown.
Runner persists exclusive started/completed receipts, validates each bundle and
frozen/measured identity, rejects incomplete attempts, and gates main on both
observer-control reports. The planner first rejected an absent matrix-plan parent
before creating a matrix; corrected setup and strictly reused identical static
templates while requiring unused result/appdata/scratch roots. Frozen on Oct 13:
`opening-campaign.json` SHA-256
`49852df93c125ea3e65fc267f136b2d3ea18447bd22b5e82eb8af2b9d9e781b1`;
`specs/global-order.json`
`b588a65c2deb4eb27df299eebb9c46ef66363b9a3530b9de6ae54c722f60af55`.
All 22 matrix plans passed static verification when created. Final prepared corpus
is 409 files / 1,135,958,159 bytes, below 8 GiB. No raw measurement exists yet.

All edits remain uncommitted. Current checkout has 60 canonical items, distinct
from the historical restored-primary 210-test/68-item snapshot.

### Historical V2 verification context

No benchmark, product-code change or M1-M3 implementation is in flight. M4 is
closed: its engineering disposition and user-assisted acceptance are complete.
The retained manual helper/procedure and evidence remain at
`tools/replay_benchmark/qb010_manual.py` and replacement evidence
`manual-acceptance.md`. Syntax, Windows self-process
identity/Toolhelp plumbing and optimized-Python rejection of a nine-second hold
were verified without launching/suspending an app. Focused review's resume-failure
ownership and unconditional-safety-check findings were corrected.

Historical completed build context:
The earlier interrupted desktop build obtained no completion evidence; both fresh
builds now passed with logs/exit receipts under `build/qb010-m2-builds/`. Isolated
pre-M2 source copies remain in ignored `build/qb010-m2-baseline/` for review.

No M3 implementation remains in flight. Preserve pre-existing unrelated edits,
including Cargo.toml's line-ending-only status.

M4 preflight (2026-09-16, before collection): fresh isolated reference
source with diagnostic tap is at `build/qb010-m4/reference-source`; exact patch is
`build/qb010-m4/reference-tap.patch`. Preflight found benchmark-only M3 readiness
defects: reactive duration publications could start a scenario before historical
details, and legacy namespaces skipped draining. App now separates readiness
started/emitted state, drains legacy Games scenarios too and rejects an incomplete
drain. Two deferred App integration cases and two isolated reference parity cases
pass. New analyzer cases enforce the explicit frontend origin, count types and
required primary comparison metric; 91 Python tests ran with one existing opt-in
skipped. These are measurement-path corrections, not a controller/snapshot
redesign. Reference/candidate freezing, builds and fixtures are complete; the
partial live/manual results remain insufficient for acceptance.

M4 deterministic checks in `build/qb010-m4/checks`: initial Rust default suite
failed the PowerShell-script cancellation fixture because effective execution
policy is Restricted. With process-local `PSExecutionPolicyPreference=Bypass`,
all 86 Rust tests pass (one opt-in ignored); production and tests are unchanged.
Fmt and Clippy pass. Final frontend 107 tests and both candidate desktop builds
(including TypeScript check and Vite build) pass with the readiness correction.

M4 preparation is frozen at
`build/perf/qb-replay-010-m4/.chronobreak-replay-benchmark`: 409 prepared files
(1,108,793,064 bytes), ten verified immutable plans, and all 50 runner preflights
passed. S retains the original three-bundle S/R/L library shape but selects S only;
N/E retain their exact 50-game/50-clip identities. Full prior decode evidence is
reused only after source/copy hashes and write times matched. Adjacent matched
pairs alternate leading arms in five seeded rounds (20260915), with five-second
warmup/cooldown. Each arm shares the fresh per-cell immutable library/config to
preserve the existing exact config-SHA comparison gate, with separate appdata,
scratch and result roots. Collection subsequently stopped at launch 15; see the
active-unit disposition above.

Frozen executable SHA-256: reference
`e58d5071afbe4b6c3563c0b33533d577984bf97b75e01ad4483fdf29a27fb230`;
candidate `b9d74f34018a4577bab7175f40932801dc257e1f701268a96bdf57c93a3db367`.
Both are fresh optimized Tauri builds. The normal candidate build is separately
frozen as `subjects/manual` for interactions. Early helper preparation failures
and a premature unusable candidate copy are preserved separately and were never
launched. Initial reference build lacked copied staged runtime; staging the same
locked bytes resolved it. Build command receipts prove success despite a local
logger's subsequent cp1252 console-print error (logger corrected).

Ignored root: `build/perf/qb-replay-010/.chronobreak-replay-benchmark`.

- `subject/current-3a750e2/subject.json`: frozen source/binary/runtime identity.
- `generated/.chronobreak-replay-time/sources/opening-srl-v2/corpus.json`:
  successful corpus; failed opening-srl-v1 remains preserved.
- `manifests/prepared-{srl,n,e}.json`: immutable prepared inputs/receipts.
- `matrix-plans/qb010-{s,r,l,n,e}-v1/matrix-plan.json`: six launches per cell.
- `specs/global-order.json`, `launch-NN.log`, `interruption-launch-14.json`:
  exact order/interruption history. Never restart run_cells.ps1 now.
- `reports/attribution-v3`: final local analysis. V1 failed phase assumptions;
  v2 ordinal mapping was corrected in v3. Canonical evidence uses v3 and checks
  each launch/scenario mapping. Earlier outputs remain preserved.
- `specs/clip-diagnostic.log`, `clip-diagnostic.patch`, original source bytes and
  diagnostic source hash: completed native follow-up, not active application code.
- `specs/checkpoint-before-v2.md`: detailed pre-handoff record preserved locally;
  this checkpoint remains the sole current execution authority.

## Remaining

No active engineering work. Criterion 6/current integrated performance acceptance
is inconclusive/deferred. The aggregate observer gate failed to establish valid
acceptance and all 100 main launches remain unrun. Current normal-build native
acceptance and the configured/resolved-root alias watchpoint are also unverified
and deferred; neither is a confirmed product failure.

Preserve all subjects, fixtures, reports and raw failed/accepted receipts under
`build/perf/qb-replay-010-opening-v1/` and the existing evidence paths below.
Prior library/descriptor/UI results remain closed and unpooled. Revisit only for
a specific product-driven question; do not repair benchmark infrastructure or
launch a replacement merely to complete the old plan.

### Closed descriptor boundary

The descriptor and full probe deliberately reread strict JSON. Media-first UI
staging is implemented and optional full details are deferred. Aggregate I/O and
integrated replay-opening behavior remain to be measured by this plan; a payload
split or persistent ReplayIndex is not presumed necessary. Earlier historical
remaining-scope bullets below were superseded by descriptor/UI closure.

### Historical V2 handoff list (superseded by closure)

- V2 M4: collection and engineering disposition are sufficient and closed. Preserve
  both campaigns; no further launches or selective top-ups are authorized by this
  replacement disposition. The historical useful/read-I/O cost is the documented
  benchmark-only compatibility drain; N-cold working-set and E-warm handle signals
  are bounded sampler/process effects; cancellations are server-side stream
  teardown with no child-failure evidence. Retain every individual growth signal.
- Complete the dedicated interaction sequence in an environment with working UI
  controls: Games with delayed optional work; Clips select/away/back; owned-batch
  save/delete/export/cleanup; overlapping A/B roots; optional failure; refreshed
  cards/counts, viewer return and playable exported media with source preservation.
- Full 010 minimal replay descriptor/staged semantic timeline criterion remains
  open. Unmodified playback parsing/off-thread work also needs disposition before
  full completion. V2 covers only the attributed library unit.
- Disposition the remaining full-feature post-payload delay/product gates without
  inferring that library measurements authorize payload splitting. The known
  configured-path/resolved-root identity mismatch remains a follow-up risk; only
  initial absolute-root rendering was observed, not alias/root-switch acceptance.

## Verification

2026-10-29 scope/state handoff: canonical validator passed (102 items / 22
plan/checkpoint pairs); whitespace check passed. Documentation/state changes
only; no product suite, native scenario or benchmark rerun is claimed.

- 2026-10-13 user-requested reconciliation incorporates the newer PC handoff while
  preserving the opening work and closed campaign evidence. Combined-source
  checks and review are recorded in
  `docs/reviews/repository-consolidation-2026-10-13.md`. Existing frozen subjects
  retain their original source identities and do not certify the combined app;
  no benchmark/native run or feature-completion claim was added. The next action
  remains diagnosis of r4 telemetry. The user explicitly authorized commit/push.

- 2026-10-12 M1 current candidate: focused frontend 36/36 passed
  (`command-20261012-104439.log`); complete `npm run test --prefix app` 214/214
  across 24 files passed (`104552`). `npm run check --prefix app` passed.
  `npm run build --prefix app` passed (`104644`).
- Archived reference: `npm run test --prefix build/qb010-opening-reference/app`
  passed 96/96 across 15 files (`104550`), including nine new parity tests.
  `npm run build --prefix build/qb010-opening-reference/app` passed (`104651`),
  including TypeScript checking. Dependencies came from the locked offline cache.
- `python -m unittest discover -s tools/replay_benchmark/tests -v`: 109 run,
  108 passed and one existing opt-in skipped. After independently binding archive
  source revision/patch fingerprints, focused `test_opening_acceptance.py` reran
  14/14 passed (8.140 s). Generic producer/schema behavior was unchanged.
- `cargo test --manifest-path app/src-tauri/Cargo.toml`: 92 passed, one existing
  opt-in ignored (`104621`). `cargo fmt --manifest-path app/src-tauri/Cargo.toml --
  --check` passed. `cargo clippy --manifest-path app/src-tauri/Cargo.toml
  --all-targets -- -D warnings` passed (`104757`). Logs above reside in `.codex/logs/`.
- A transient action-payload scope error was caught by the five Viewer cases;
  corrected to `props.opening` in both subjects, then all affected/full tests and
  builds passed. Patch verification initially compared CRLF archive-copy bytes
  against LF patch output; normalization plus fresh apply proved exact equality.
  Sandboxed test/build logger access failures ran no tests/build; approved tool
  execution with normal log access produced the passing receipts above.
- M2 desktop builds passed on 2026-10-12: normal candidate (release 47.54 s),
  benchmark candidate (53.72 s), benchmark reference (51.57 s), all with isolated
  source and `CARGO_TARGET_DIR` set to the existing app target and offline Cargo.
  The first archive build failed for absent relative packaged runtime; staging
  the locked copy fixed it without source changes. The prior automatic approval
  attempt did not execute because review hit a usage limit; user's continuation
  retried the normal approval path successfully. Normal and benchmark hashes and
  exact commands are in the build receipts above.
- M2 exposed two synthetic/real producer differences in new acceptance: launch
  manifests do not have analyzer fixture `fingerprints`; runner `source` describes
  collector checkout. Corrected via frozen successful build-receipt binding and
  prelaunch declared-input fingerprint versus measured comparison fingerprint.
  A fresh focused provenance review found path escape; containment/reparse checks
  now cover receipts, patches and result roots. A helper/parameter name collision
  was caught and corrected. Latest focused opening Python: 15/15 passed (10.910 s),
  including a full 116-bundle campaign without synthetic manifest fingerprints.
  Product binaries and generic analyzer/runner behavior were unchanged.
- 2026-10-12 runner recovery guards: eight focused tests passed (0.334 s), covering
  future started/completed gaps, cross-phase cooldown, subprocess timeout/launch
  failure, log I/O failure, bundle admission and phase-report failure. These launch
  no app and do not replace real preflights or controls. Preparation recovery
  preflights for N-r1/E passed; their full decode is in flight.
- 2026-10-13 preparation accepted: original S/R/L plus 102 independently finalized
  N/E fixture groups, 203 media files full-decoded overall; 409 final prepared
  files. Assembly regression tests passed 4/4 (1.329 s); fresh focused review
  passed. Partial earlier bulk outputs are preserved and excluded. Atomic aggregate
  receipt: `preparation-prepare/complete.json` at 07:26:13 UTC.
- Corpus full validation/assembly and all 116 static launch preflights passed.
  The first control failed campaign identity admission and was also user-interacted;
  collection is closed. Remaining controls, 100 main launches and current native
  acceptance remain not run. No performance or full-feature completion claim follows from
  the automated/build checks.

- 2026-10-06 planning handoff: canonical schema/lifecycle/link validation passed
  for 60 roadmap items and eight plan/checkpoint pairs; documentation whitespace
  and exact plan/checkpoint association checks passed. The source baseline before
  these documentation edits was clean main a82d81f. No product test/build, native
  scenario or performance run was attempted during assessment/planning.
- Current integration provenance: September 27 consolidation records 203 frontend
  tests plus TypeScript/Vite build for the UI merge. Rust tests, native acceptance
  and performance were explicitly not rerun. Its later 210-test restored-primary
  snapshot and 68-item validation are not the present clean source/catalog; retain
  this distinction when choosing final verification. September 23's 166 frontend/
  92 Rust, desktop builds and native observations below apply to that earlier unit.

- Reconciliation native acceptance closed 2026-09-23 08:27 UTC. User confirms
  A/B/A with correct Lux then Ahri/Garen labels and A/V; config returned to root-a
  with Never retention. After explicit title-bar X, closure-final.json records
  subject_running=false and diagnostic_listener_open=false. Gate had already
  restored ordinary fetch with zero pending responses. audit-after-close.json
  confirms every video/game-log hash and every B file unchanged; only the expected
  Ahri metadata rewrite differs, validated schema-equivalent with saved=true.
  Frozen normal subject 5651f0a851b222b7bd318605b3eda8064ba7bf747c7388adf31d7349d247cb82 and
  original generated source 131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe still
  match preparation. Detailed user observations and diagnostic disposition are in
  build/qb010-ui-reconciliation/native-acceptance.md; raw receipts/audits are in
  native-20260923/. Final frontend 166 tests and typecheck pass after the test-only
  reported-sequence regression; production binaries remain the verified subjects.

- Native ungated checks: user confirms windowed/fullscreen event and champion
  filters, editor opening and return with clip/playback intact, immediate/persistent
  Star state after two toggles, and correct Garen opening. Gate remains removed,
  zero pending. Post-control audit changes only Ahri metadata bytes: original and
  current saved=true; absent optional capture becomes null, schema-equivalent
  after existing Option-field normalization. Every video/game-log hash and all B
  files are unchanged. Root corrected its mistaken unsaved expectation for the
  initially saved Ahri fixture. User is performing final A/B/A and app closure.

- Native retry: user confirms playback and clip selection correct after exporter
  return/failure. Released retry #3, then additional queued #4. Initial finish
  refused a pending response; after #4 settled, finish restored ordinary fetch
  with zero held requests. Primary #2 retained same element/source/loadstart=1,
  readyState=4 and currentTime=152.560963 across delivery. User is checking normal
  export/return, semantic filters, save/unsave and Garen. Full frontend now passes
  166 tests plus typecheck after a test-only regression for the reported held
  exporter round trip (101323); production subject unchanged.

- Native partial observation 2026-09-23: user confirms moving video/audible audio,
  play/pause, seek, zoom/pan, clip edits and fullscreen including mid-drag all worked
  with details held. User then entered exporter and returned, reporting loading.
  Diagnostic showed completed Ahri probe #1 still intentionally held and no video;
  releasing #1 immediately mounted returned viewer #2 at readyState 4 with its new
  probe #2 held. This is the diagnostic occupying the single active reader, not an
  observed media/backend failure. The pending exporter intent was superseded by
  return. Captured baseline for viewer #2; injected local details failure #2 keeps
  same primary/source/loadstart count and readyState 4. Await user retry/clip
  confirmation, then release and remove gate for ungated exporter verification.

- Final benchmark desktop build passed after normal subject freeze, receipt
  build/qb010-ui-reconciliation/desktop-benchmark.log. Canonical validator passed
  60 items/eight pairs. Native fixture preparation completed 2026-09-23 07:50 UTC:
  normal executable SHA-256 5651f0a851b222b7bd318605b3eda8064ba7bf747c7388adf31d7349d247cb82;
  generated source 131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe unchanged;
  runtime queueback-ffmpeg-8.1.2-windows-x86_64-r6. Three media files fully decoded,
  A/B initial source audit unchanged. Helpers are copied into the deeper isolated
  directory with only repository-relative path adjustment. Diagnostic self-test
  passed. User-operated observations are retained in native-acceptance.md.

- Final combined checks 2026-09-23: frontend 22 files/165 tests passed (094342),
  benchmark paint regression 5 App tests passed (094315), typecheck passed; full
  app Rust 92 passed/one pre-existing packaged-runtime opt-in ignored (tool session
  50529, PSExecutionPolicyPreference=Bypass for the dedicated child fixture).
  Rust fmt and Clippy passed (093955), final standalone frontend and normal
  desktop builds passed. Build logs, retained command receipts and verification.json
  are under build/qb010-ui-reconciliation/. Git diff HEAD --check passed. Normal
  build was repeated after the benchmark guard change; older normal output is not
  the frozen subject. Reviewed source paths add only in-memory roster projection.

- Reconciliation 2026-09-23: focused suites passed after test-seam corrections
  (37 tests: command-20260923-092313; rail 6: 092509; App save/mock 6: 092654;
  Viewer 7: 093539; card 1: 093751). Rust library 15 passed (093327), including
  normalized roster order, missing-team/empty/incomplete parity; an initial enum
  assertion compile failure (093140) was corrected in tests. Viewer readiness
  URL equality failure (093505) was corrected to allow the controller session query.
  Full frontend 22 files/162 tests passed (093830); typecheck and Rust fmt passed;
  production frontend build passed (093854). All command receipts are in
  .codex/logs/command-<date-time>.log, with final receipts to be copied under build.

- UI integration 2026-09-22: typecheck passed twice. Focused initial and follow-up
  failures are retained in .codex/logs/command-20260922-165807.log and
  command-20260922-170123.log. The latter is 38 passed/5 failed as detailed above.
  Packaged ffprobe confirms imported mock MP4: 108000 frames at 60/1, time base
  1/15360, first PTS 0, duration 27648000 ticks, Constrained Baseline, no audio.
  The mock descriptor/probe now share its URL and exact timeline/profile. Binary
  media is unchanged. A prior wrong probe path failed setup only.

- 2026-09-22 UI planning only: bootstrap, applicable architecture and plan contract
  read; independent review reconciled as above. Product verification has not run
  against a combined tree because integration has not started. Earlier passing
  descriptor/M4 results below remain historical and are not a combined-UI pass.
  Handoff canonical validator passed: 60 roadmap items/eight plan-checkpoint pairs;
  git diff --check and the new plan's final-newline/trailing-whitespace checks
  passed. Root reviewed the complete plan and canonical diff. Only the three
  handoff documents differ in the root; no commit or product merge was made.

- 2026-09-22 final native confirmation: user replied "all passed and closed" to
  Ahri/details with no stale Lux, reopen Ahri with the hold removed, video/audio,
  play/pause/seek/fullscreen, then close the test app. This completes the actual
  user-operated descriptor procedure; DOM tests are not native playback proof.
  Agent verified subject PID 23268 and WebView2 debugger PID 26316 absent and
  zero listeners on the isolated debug port 52316. The original fetch had already
  been restored with no pending response in
  `diagnostic-2026-09-22T12-21-54-232Z-finish.json`.
  `python build/qb010-descriptor-manual-20260922/prepare.py audit final-closed`
  passed: both A/B roots unchanged, no added/deleted/changed source files.
  Fixture setup, human observations and raw diagnostics/audits remain in the
  ignored workspace; `manual-observations.md` records the instructed sequence
  and explicit replies. No production code changed during native acceptance.
- Final code review: root reviewed backend command/coordinator/projection changes,
  new Rust tests, frontend changes/tests, architecture and the complete canonical
  diff; a fresh read-only
  frontend scout found no actionable descriptor defect. Complete required
  automated results below remain applicable to the unchanged product tree.

- Native failure/retry: user confirmed the injected error text and working
  preview/pause, then confirmed multiple Retry clicks. Request 3 completed into
  the one latest pending request 4; after both authentic responses were released,
  pending/error cleared. Video 2, one loadstart/emptied, paused 71.929267 s, rate 1,
  unmuted volume 1 and windowed state were preserved through admission. A focused
  scout matched this to LibraryController's one-active/one-latest scheduler and
  ViewerScreen's explicit retry button; no automatic loop or defect established.
  The previous error stays visible during a held retry until success. Source
  `audit-retry-complete.json` passes A/B unchanged. Final visual draft confirmation
  after successful retry was subsequently confirmed by the user.

- Stale native navigation: user confirmed the instructed Ahri pending -> Games
  -> Garen selection sequence. Request 5 (old Ahri, game 1790100000) was verified
  held with no primary video. Release admitted only the queued current Garen
  descriptor and request 6/game 1790100001; video 4 mounted. Its own response
  release retained same element/source, loadstart/emptied 1/1, paused time 0/rate 1,
  and cleared details pending/error. User subsequently confirmed visible Garen
  and no stale Ahri before proceeding to root changes.

- Root A-to-B: user reported only Lux after switching via Settings, with Lux
  waiting on Recording. Config confirmed B. Old Ahri request 7 remained held
  and no primary video existed; release admitted B video 6 and request 8 using
  the same game ID 1790100000. User subsequently confirmed Lux playback and return
  to A. Config A, held B request 8 and absent primary video were verified before
  release. Current A video 7/request 9 then appeared. Release 9 and gate removal
  retained element/source, one loadstart/emptied, paused time 0/rate 1, with no
  pending/error. `audit-root-return.json` passes both roots unchanged. Final visual
  Ahri/no-Lux and ungated playback/close were subsequently confirmed as recorded
  above, followed by the final closed-app source audit.

- Assisted first native sequence: user confirmed moving picture, audible sound,
  play/pause/resume, rapid seeks, fullscreen controls/in-out and adjustment of
  both clip handles to a valid draft while details request 1 was verified held.
  Before/after release diagnostics retain identical element/source, loadstart 1,
  emptied 1, play/pause 61/61, seeked 105, paused 130.106781 s, rate 4, unmuted
  volume 1, windowed. Details became available without error. Frame counters
  were 11,307 total/3,276 dropped after the user's rapid seeking/rate/clip exercises;
  this is not a controlled performance or rate-capability claim. Source audit
  `audit-first-release.json` passes A/B unchanged. User subsequently confirmed
  visible details and unchanged clip draft. User evidence: workspace `manual-observations.md`;
  time-stamped baseline/release/status JSON retains supplementary diagnostics.

- 2026-09-22 assisted setup: `npm run desktop:build --prefix app` passed with
  TypeScript/Vite and optimized normal Tauri build; existing warnings non-fatal.
  Frozen normal subject SHA-256:
  `32fd21f9b41db6f6c4c4197699923f3c574d167b0debffee4ae79470ea0bff43`.
  `preparation.json` records the copied subject, packaged r6 runtime and new A/B
  fixtures. All three new videos passed strict exact-60-FPS validation and full
  single-thread decode (exit 0, empty stderr). Prelaunch audit
  `audit-prelaunch-resume.json` confirms all A/B source files unchanged. The generated
  non-personal input was read-only and unchanged; no M4 campaign or fixture was mutated.
- Setup-only failures were preserved: omitted builder keyword corrected before
  generation; v1 audio-remux rejected for nonexact rate/240.021354-second video.
  New v2 exact-copy recipe preserves the validated grid and source AAC. No parser
  tolerance or benchmark tooling changed. Audible output was subsequently
  confirmed by the user in the held-details native sequence above.
- Ignored `session.mjs` uses a child-only loopback debugger setting and isolated
  config/profile; no CSP/Origin/security bypass or alternate UI driver. The first
  unlaunched invoke-wrapper assumption was invalid because Tauri freezes invoke;
  corrected to the exact optional command's fetch response only. `node --check`
  and `gate.test.mjs` pass against installed Tauri 2.11.5 core/transport scripts,
  including authentic held response, failure without fallback/reissue, retry,
  callback cleanup and restoration. Native transport capture and restoration were
  subsequently verified in the held-response sequence above.
  The diagnostic holds frontend delivery, not backend worker ownership; native
  visuals/audio require explicit user observations. No payload, capability URL,
  arbitrary DOM text or input values are retained in diagnostic receipts.

- 2026-09-21 native-acceptance resume: `npm run desktop:build --prefix app`
  passed, including TypeScript/Vite and the optimized normal non-benchmark Tauri
  executable. No product code changed. Subject:
  `app/src-tauri/target/release/league-replay-app.exe`, 16,268,800 bytes,
  SHA-256 `a495d9c3cae12fbab629dd6d549035f3f0ea5bc1f8a4da9a76f926d2b4b7c58c`;
  last-write UTC `2026-09-21T13:58:10Z`. This supersedes the earlier handoff's
  benchmark-enabled last-build note below. Existing Vite/linker warnings remain
  non-fatal. No subject was launched.
- Supported computer-use preflight: imported `@oai/sky` through `node_repl` as
  instructed, then `sky.list_windows()` returned `Computer Use native pipe is
  unavailable: failed to connect native pipe: Le fichier spécifié est introuvable.
  (os error 2)`. Reset the JavaScript session, reinitialized and retried discovery;
  the identical error persisted. **Environment-blocked**, not a product failure.
  A subsequent resume repeated discovery and reset/retry with the same error;
  the normal executable still matches the recorded SHA-256. No app was launched.
  No alternate UI driver, browser/DOM substitute, real recording, old M4 fixture
  or acceptance artifact was used. At that preflight, A/V, controls, held/failed/
  late details, retry, clip state, stale navigation/root behavior and source-hash
  acceptance were unperformed; no commit was made. The 2026-09-22 user-operated
  sequence above subsequently completed those descriptor checks.
- A bounded read-only scout found no existing normal-app full-probe hold/fail/
  release helper. Existing M4 ffprobe-duration holds do not hold replay details.
  Establish a descriptor-specific controlled fixture setup when native access is
  restored; this finding does not authorize production hooks or a redesign.

- 2026-09-22 final canonical validator and `git diff --check` passed after the
  descriptor closure edits: 60 roadmap items and eight linked plan/checkpoint
  pairs. The V2 plan, benchmark tooling, campaigns,
  resource/cancellation dispositions and manual-acceptance artifacts are unchanged.
- 2026-09-21 descriptor final automated baseline: `cargo test --manifest-path
  app/src-tauri/Cargo.toml` passed 90 tests (one existing opt-in packaged-probe
  test ignored); app `cargo fmt -- --check` and `cargo clippy --all-targets --
  -D warnings` passed. The two newly included Rust test files were also formatted
  directly with rustfmt. No recorder/media-runtime checks or M4 campaign ran.
- `npm run test --prefix app` passed 123 tests in 17 files; `npm run check
  --prefix app` passed. The initial new deferred clip test used a one-second draft,
  correctly rejected by the existing five-second minimum; corrected to a valid
  ten-second fixture and rerun. No product minimum or validation was weakened.
- `npm run build --prefix app` passed. Both `npm run desktop:build --prefix app`
  and `npm run desktop:build:benchmark --prefix app` passed with fresh TypeScript/
  Vite builds after the final product-code edits. Their warning about the existing
  static/dynamic API import and Windows linker informational output did not fail
  builds. At that handoff the shared executable was benchmark-enabled; the fresh
  normal rebuild and native launch recorded above supersede that handoff state.
- `python -m unittest discover -s tools/replay_benchmark/tests -v` ran 95 tests
  successfully with one existing opt-in skip (94 passed). Detailed combined
  frontend/build/Python log: `.codex/logs/command-20260921-145022.log`.
- At the 2026-09-21 automated handoff, native descriptor acceptance was **not run**;
  the subsequent 2026-09-22 user-operated results above supersede that blocker.
  Automated DOM and ownership tests do not prove native decode, audible A/V, presented-frame or
  source-hash acceptance. Closed M4 acceptance is neither repeated nor reused as
  proof of these new viewer changes. No new benchmark/performance pass is claimed.

- Descriptor handoff validator passed: 60 roadmap items/eight plan-checkpoint
  pairs; whitespace passed. The initial apply_patch engine failed in sandbox;
  approved native apply_patch now works with Windows argument quoting corrected.
- M1: two Rust descriptor tests and two owned-read tests passed. Fixtures cover
  empty/older v2, strict malformed/v1 rejection, absent media, off-thread progress,
  dropped waiter/Busy ownership, failures, refresh/mutation/A-B-A/shutdown rejection.
  Initial compilation found only a missing Path qualification in the new test;
  corrected and rerun successfully. Frontend API tests 9/9 passed.
- M2 initial TypeScript check passed after threading required read-owner props
  into existing viewer/exporter test harnesses. Optional resource errors are read
  through a guarded accessor so failed details cannot throw through video memos.

- Descriptor bootstrap: PRODUCT, WORKFLOW, complete feature, complete checkpoint,
  V2 immutable plan, PLANS, VERIFICATION, forward-engineering/Rust guidance and
  applicable replay/library/security architecture read. HEAD matches 3b14df2;
  initial worktree clean. Product verification not run for descriptor planning.
- Root plan challenge resolved shared read-slot ownership, exact descriptor
  projection, strict bundle compatibility, all three full-probe callers,
  Settings/exporter/benchmark navigation origins and benchmark readiness ordering.
  Delegated scouts never initialized (pending_init); no independent review is
  claimed. Historical opening_attribution assumes payload-before-mount and is not
  reused as a descriptor measurement. Duplicate strict JSON reads are an explicit
  tradeoff, with no replay latency/I/O non-regression claim.

- 2026-09-17 replacement: all 50 strict bundles valid, all ten result matrices pass;
  required event/request loss zero, all source hashes unchanged. Same-clock
  resource/opening extractions cover every run. Full normal verification repeated:
  frontend 107 tests, reference parity 2, Rust 86 (one opt-in ignored), replay
  Python 91 run (one opt-in skipped), fmt/Clippy, normal and benchmark desktop
  builds including TypeScript/Vite passed. Sanitized `verification.json` preserves
  receipts, including the wrong-filename parity invocation and corrected pass.
- Native UI APIs were disabled for the initial autonomous attempt; the current
  `manual-status.json` records the later user-assisted acceptance, verified optional
  child receipts and final audits. The initial startup observation remains distinct
  from the subsequently completed sequence.
- 2026-09-16/17 M4: app Rust 86 passed/one opt-in ignored (process-local execution
  policy override for the script fixture); fmt and Clippy passed. Final frontend
  107/107 across 16 files and reference parity 2/2 passed. Replay Python suite ran
  91 tests, one existing opt-in skipped. Normal/benchmark candidate and benchmark
  reference desktop builds passed, including final npm check/build. Receipts and
  logs: `build/qb010-m4/checks`; sanitized receipts in M4 evidence `verification.json`.
- M4 measurement readiness corrections preserve legacy event semantics and the
  `games-library-usable-v1` contract. Independent measurement review requested
  reference parity cases; both passed before launch. Current tree source hashes
  and all three frozen subjects were rechecked on resume; identities match.
- All 14 completed runs in the first M4 campaign validate, with zero required
  event/request loss and source mutation. Run 15 is invalid: missing both
  terminals, collection result, post-hashes and runner result. All ten
  required-result matrix checks reject that incomplete campaign. The replacement
  has a separate 50/50 valid result; no first-campaign artifact was repaired.
- Initial manual startup alone passed: three Ahri cards, zero saved, storage 23.9 MB
  games / 2.67 MB clips / 26.6 MB total. The initial autonomous CUA attempt could
  not operate the window. This dated result was superseded by user-assisted A/B/A,
  export and cleanup acceptance; four exports now have retained H.264/AAC/full-decode
  evidence. The disposable roots were intentionally mutated only by those actions.
- Final canonical validator, whitespace and retained-evidence checks are recorded
  in M4 evidence `final-checks.json`. No architecture revision was justified by
  incomplete performance evidence or blocked interaction. M3 invariants remain.

- 2026-09-16 M2 final `cargo test --manifest-path app/src-tauri/Cargo.toml` passed
  86 tests with one explicit packaged-runtime test ignored in the default suite.
  `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` and
  `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`
  passed. Final log: `.codex/logs/command-20260916-104351.log`.
- Focused duration suite with `QUEUEBACK_TEST_MEDIA_RUNTIME` set to the staged r6
  root and `cargo test --manifest-path app/src-tauri/Cargo.toml --lib clip_duration
  -- --include-ignored` passed 7/7 (`command-20260916-103426.log`). This includes
  actual ten-second timeout, output overflow, failure/stderr isolation, signalled
  cancellation, metadata replacement/deletion, a real native HTTP child after a
  junction redirect, and packaged ffprobe over the protected route. A copied MOV
  with an external HTTP data reference never connected to its sentinel listener;
  input bytes remained unchanged. This is focused probe verification, not a replay
  attribution/performance run.
- Coordinator tests cover batch/ID/token bounds, explicit retry/cache reset, dropped
  caller scan/probe/mutation/settings ownership, offloading, supersession/shutdown,
  root A/B/A, save/delete/retention success and real partial clip deletion failure,
  settings failure preservation and captured export completion. Barriers/channels
  and child-ready signals establish races; no sleep establishes race ordering.
- `npm.cmd run test --prefix app` passed 87/87; `npm.cmd run check --prefix app` and
  `npm.cmd run build --prefix app` passed. `python -m unittest discover -s
  tools/replay_benchmark/tests -v` passed 88 tests, one existing opt-in test skipped.
  Log: `.codex/logs/command-20260916-103806.log`. Frontend/tool sources are unchanged.
- `npm.cmd run desktop:build --prefix app` and `npm.cmd run desktop:build:benchmark
  --prefix app` both passed on the final corrected M2 tree (only the existing
  non-fatal linker-output warning). Exact command/exit/hash receipts and full logs:
  `build/qb010-m2-builds/{normal,benchmark}-result.json` and corresponding `.log`
  files. The new snapshot/duration commands are registered in both builds; existing
  benchmark-only handlers remain behind their `replay-benchmark` cfg. No benchmark
  observation semantics or frontend taps changed in M2.
- M2 handoff canonical validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Final tracked diff, new-file whitespace and explicit included-test rustfmt
  checks passed. Immutable plans, attribution evidence and frontend files remain
  unchanged by this milestone. No performance improvement/manual UI claim is made.
- Initial focused compile failed only because a test moved its fixture path; fixed
  the test ownership. Initial junction test tried renaming a directory with an
  opened descendant, which Windows rejects; changed to replace an internal junction
  after validated admission, proving the intended reparse race. Both corrected tests
  pass. Initial logger sandbox denials were resolved by approved reruns.

- 2026-09-16 M3 verification: `npm.cmd run test --prefix app` passed 105 tests
  across 15 files, including deferred controller races for same-root refresh,
  A/B/A identity, stale-token mutation admission, navigation/disposal, bounded
  duration failure/retry/coalescing/drain, export invalidation, strict decoders,
  and Games rendering with Data Dragon offline. `npm.cmd run check --prefix app`
  and `npm.cmd run build --prefix app` passed. Rust `cargo test --manifest-path
  app/src-tauri/Cargo.toml --lib` passed 86 tests; `cargo fmt --manifest-path
  app/src-tauri/Cargo.toml -- --check` and `cargo clippy --manifest-path
  app/src-tauri/Cargo.toml --all-targets -- -D warnings` passed. Python replay
  benchmark tests passed 88 with one existing opt-in test skipped. No benchmark
  comparison was run. Both `npm.cmd run desktop:build --prefix app` and
  `npm.cmd run desktop:build:benchmark --prefix app` passed on the final tree;
  each emitted only the existing non-fatal linker-output warning.
- Fresh adversarial frontend review was performed after implementation. It found
  four substantive gaps: benchmark `library_useful` timing, retained-view error
  presentation, return refresh, and Settings-to-Clips duration visibility. The
  implementation now drains benchmark durations after `games_library_usable`,
  keeps same-root cards visible with an inline error, refreshes on library return,
  and restores `clipsVisible` on Settings return. The lower-confidence raw-path
  alias concern remains bounded by backend token/root admission and is recorded
  as a follow-up risk rather than changing the security contract.

- 2026-09-16 M1 focused `cargo test --manifest-path app/src-tauri/Cargo.toml --lib
  library`: 18 passed, covering 11 new deterministic scan/coordinator cases and
  existing strict library tests. One initial fixture assertion expected 38 bytes
  instead of its actual 40; corrected the assertion, not production accounting.
  Barrier/channel tests prove offloading, dropped-waiter permit retention and Busy
  behavior across roots, revision-only/A-B-A rejection, persistence failure,
  same-root failure/rebuild, worker panic and shutdown. No timing sleeps or media
  processes are used by these new tests; all filesystem fixtures use tempfile.
- Final M1 tree: `cargo test --manifest-path app/src-tauri/Cargo.toml` passed 72
  tests; `cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check` and
  `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings`
  passed. `cargo check --manifest-path app/src-tauri/Cargo.toml --features
  replay-benchmark`, `npm.cmd run check --prefix app` and
  `npm.cmd run build --prefix app` passed. Full Rust tests include the final
  worker-join failure handling. Logger sandbox denials on initial test/build
  attempts were resolved by approved reruns; they were not product failures.
  Local logs: `.codex/logs/command-20260916-095441.log` (focused),
  `command-20260916-095556.log` (full Rust), `command-20260916-095638.log` (build),
  `command-20260916-095711.log` (Clippy), `command-20260916-095840.log` (benchmark check).
- M1 does not claim UI readiness, optional-probe/mutation ownership, a speedup,
  release packaging or full-feature completion. No attribution/media-runtime/
  recorder campaign was repeated; later milestone manual/performance/build gates
  remain unrun for this candidate.
- M1 handoff canonical validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Final `git diff --check` and new M1 source/checkpoint whitespace checks
  passed; the linked immutable v2 plan and attribution evidence were not edited.
- Benchmark production build passed via `npm.cmd run desktop:build:benchmark --prefix app`,
  including TypeScript/Vite and optimized Tauri; non-fatal linker-output warning.
  Frozen executable SHA-256:
  `f1e3390cb29b3c9465eee3898a46af5d2c3c69894aeb9f7a5ad5c09a1ff75b49`.
  subject.json binds zero product-code diff, seed 20260915 and packaged r6 hashes.
- S/R/L exact grids: 14,400/108,000/216,000 frames, PTS 0, 256 ticks/sample at
  1/15360. All source hashes matched before/preflight/after generation. Video hashes:
  S `131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe`,
  R `c5c3ecf5802dffb8cbcf1382380c505d9e21d27064338037fb5e490230611e09`,
  L `c00f2577b7365f8db868c8243cd434d3453dd6dc42f7c79143ed0ed887052d95`.
- N/E corresponding media/metadata/IDs match. Sparse logs: 7 events/8 snapshots;
  dense: 259/240 per game. Fifty dedicated ten-second H.264/AAC clip copies each.
  Prepared S/R/L, N, E passed 3/100/100 complete decodes with empty error output.
  Exact recipes/counts are in specs/scaling-receipt.json.
- Required-result matrix checks for the earlier attribution campaign: S/R/E
  passed; L failed missing launch-14 terminal; N failed launch-18 telemetry
  completeness. Its cold counts S/R/L/N/E were 5/5/4/4/5. The replacement's ten
  matrices all passed.
- Final `python -m unittest discover -s tools/replay_benchmark/tests -v`: 88 tests
  OK, one opt-in media test skipped. Separate packaged-runtime corpus suite passed
  all 15 tests, including the real repeat/AAC regression and full decode. Helper's
  five synthetic tests and extraction from all 28 valid real runs passed.
- Native diagnostic: `cargo test --manifest-path app/src-tauri/Cargo.toml --release
  --features replay-benchmark --lib library::qb010_clip_work_attribution -- --exact
  --ignored --nocapture --test-threads=1`: one test passed, six measured calls,
  22.30 seconds. Test-only patch retained, source restored. All 400 prepared N/E
  files then matched preparation SHA-256/size receipts.
- Evidence checks passed: 30 identities, 28 exact valid mappings, 53 frontend
  cycles, current helper hashes and sanitized files. No bearer URLs, raw local
  paths or user identity are retained in shared reports.
- Final v2 handoff validator passed: 60 roadmap items / eight plan-checkpoint
  pairs. Required plan sections, UTF-8/final newline, new-document whitespace and
  `git diff --check` passed. This is not project-wide product-code completion.

## Deviations

- M4 sequence 15 (N-warm candidate trial 2) began at
  `2026-09-16T14:11:16.8087295Z` and ended without finalization during cycle six.
  Empty outer receipt/log and all partial raw artifacts remain. No process was
  running on resume; the external interruption's cause is unknown. No application
  failure is inferred from missing terminal evidence. The invalid-run rule and
  ceiling prevented completing this campaign; no further launch was made.
- Required manual interaction was actually attempted with the dedicated normal
  build, but the Windows capture/input environment prevented the sequence. Do not
  substitute deterministic race tests or the initial accessible Games view for it.
- M4 preflight fixed only demonstrated benchmark readiness/validation defects.
  No snapshot redesign, payload split, unrelated campaign or root-alias fix.

- Repeat transform make_zero shifted AAC-priming timestamps and stretched the first
  video packet from 256 to 912 ticks. Controlled four-second A/B isolated it;
  changing only repeat's avoid_negative_ts to disabled restored exact 60 FPS.
  Retained strict validation, stream-copy/GOP/fragment flags and independent audio.
  Failed corpus remains preserved; this was fixture tooling, not production work.
- L cold 3 (launch 14) was interrupted with terminal/collection/post-hash missing.
  N cold 4 (18) finished scenario and source hashes, but sample 2 saw 96 Job processes
  versus 95 creation notifications; next sample reconciled 118/118. Every sample
  must be complete, so neither trial is repaired or included in valid aggregates.
- Real extraction contradicted synthetic assumptions: calibrated native payload
  events can precede frontend request, and viewer-mounted generation precedes
  controller load generation. Helper now partitions frontend clocks only and uses
  loadstart generation. Native/server durations are not joined to cycles. Added
  native clock-offset regression; all validated real runs pass extraction.

## Decisions

- 2026-10-29 user-directed stop: discontinue the large integrated campaign;
  preserve every existing artifact and accepted product unit. Current performance
  acceptance is inconclusive/deferred, not passed. No replacement campaigns or
  benchmark-infrastructure diagnosis/repair without a specific product-driven
  justification. This supersedes historical execution instructions below and is
  reflected by a narrow scope amendment in the linked plan.

- 2026-10-12 implementation clarification: the generic analyzer currently emits
  schema version 1, despite the plan's clerical schema-v2 wording. Preserve that
  existing schema; opening acceptance has its own strict version-1 report.
- Real-launch refinement, 2026-10-12: the shared runner's `source` identifies its
  checkout, and launch manifests have no synthetic `fingerprints` object. Frozen
  app build receipts bind revision/patch/binary; actual runner binary hashes bind
  each run. Declared-input fingerprints freeze scenario/fixture/config/packaged
  runtime before launch. Full measured subject/comparison fingerprints then bind
  observed device/WebView/accounting identities consistently across matched runs.
  No future measurement is fabricated in a prelaunch spec; no raw result is edited.

- 2026-10-06: use actual pre-010 source 3a750e2 versus current a82d81f with frozen
  observation/workload patches. The result measures integrated changes and cannot
  isolate descriptor benefit from intervening UI changes. Preserve old event
  meanings; a new namespace observes the real clickable Games-to-media path.
- Fresh planning review required separating the H.264 Games admission from the
  legacy benchmark HEVC wait. The finalized design preserves normal capability
  initialization, tests a held probe, retains paused initial RVFC and uses a
  separate details/action-completion latch. No remaining blocking design finding.
- Final handoff source-path check: the ExecPlan calls the frontend observer file
  `app/src/replayBenchmark.ts`; the actual module is `app/src/benchmark.ts`, which
  exports `replayBenchmarkObserver` and owns `MINIMAL_EVENT_KINDS`. Use that module.
  This is a clerical path correction, not a design change; the linked immutable
  plan remains unchanged and the active target list above uses the real path.
- Full feature coverage uses S/R/L/N/E cold and warm, five independent processes
  per arm, plus four minimal/full observer pairs per subject. No pooling, selective
  top-up or reopening of M4. Invalid/interrupted collection stops and preserves its
  evidence; insufficient matrices stay insufficient. No product fix is inferred
  from the absence of measurement.
- Historical no-commit/uncommitted/sibling statements below describe their dated
  units. The current source is consolidated; this new plan itself includes no
  commit/push and never authorizes restoring unrelated recovered work.

- Reconciliation review (2026-09-23): fix the concrete pre-existing App benchmark
  fallback that emitted library_useful after the admitted Games view changed
  before painting. Fail this case with phase library_paint, include navigation
  epoch in both paint/drain admission, and retain successful milestone ordering.
  Tests cover stable/away/away-back. This is focused contract preservation, not a
  campaign rerun or a change to accepted historical M4 measurements.

- UI unit: preserve both original lines and the captured current sibling CSS diff;
  no commits. Keep LibraryController/Coordinator and protected playback authority.
  Optimistic Star is a bounded presentation overlay with mandatory filesystem
  refresh, never a second snapshot owner. Required participants expand the IPC
  GameSummary consistently in Rust, both strict decoders, TS and fixtures, without
  changing persisted v2 or the four-field descriptor. Existing ReplayTimeline is
  integrated, not rewritten; no semantic paging/payload producer or campaign.

- The current user request authorized one fresh bounded replacement campaign;
  its before-launch disposition is linked in Active unit. The former no-further-
  launch decision below remains the disposition of the first matrix. Original M4
  evidence is immutable and is not pooled with replacement trials.

- First-campaign gate disposition: insufficient evidence. Its cold Games medians
  were S 313.7/304.9 (one process/arm), N 2100.8/286.7 (two), E 2298.8/471.9
  (one); those observations cannot pass the n=5 gate. This remains historical
  evidence only.
- Replacement gate disposition: Games-usable medians R/C (ms) are S 166.4/168.2,
  N 2092.6/197.7, E 2386.6/493.3 cold and N 2087.3/209.7, E 2404.8/483.6 warm.
  N/E improvement exceeds both required thresholds and S has no material
  regression. Historical useful is slower by 22–27% because the compatibility
  drain adds about 2.45 seconds; protected-route requests/delivery and read I/O
  rise as documented in replacement evidence.
- Replacement engineering disposition (full table:
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/engineering-disposition.md`):
  N-cold working-set peak is concentrated in the WebView during the benchmark-only
  50-clip drain and has no sustained monotonic signal; E-warm handle growth is a
  lower candidate startup baseline with equal late-half growth and equal peak;
  read/request amplification and historical useful latency are the bounded,
  intentional compatibility drain; and candidate cancellations are incomplete
  HTTP bodies during ffprobe metadata completion or teardown. `run_child` retains
  success-status and kill/reap invariants, but campaign telemetry has no per-child
  exit code. CPU/private peaks pass matched thresholds; no sustained candidate
  private-memory, working-set or handle leak, server error, required event/request
  loss, source mutation, stale result, failed bundle, timeout or playback failure
  occurred. The retained partial-body diagnostic is
  `docs/performance/evidence/qb-replay-010-m4-replacement-20260917/cancellation-probe.json`.
  No production change is warranted; retain the cancellation/status correlation
  limitation as a future telemetry watchpoint.

- M2 requires `snapshot_token` on save/delete/retention/export IPC and an explicit
  `retry_unavailable` flag for durations. M3 now carries the originating token from
  each frontend action; missing or stale tokens still fail closed. Retained
  invalidated snapshots remain display-only until refresh succeeds. No tokenless
  legacy mutation bypass.
- Mutation invalidation is conservative for all admitted attempts, including errors
  before side effects; admission rejections do not invalidate. Export invalidation
  uses matching logical path/ApprovedRoot, while response admission additionally
  requires original root epoch and selected token. This observes late publications
  after A/B/A without delivering stale export results to a newer selection.

- M1 adds a separate lightweight clip wire type so existing `list_clips` duration
  semantics and frontend/benchmark readiness remain unchanged before M3. The new
  snapshot token is process-local opaque identity, never delivery authorization.
  Optional duration state and mutation-slot/invalidation wiring are now implemented
  in M2.
- Failed settings persistence leaves both the published root pair and coordinator
  revision/view untouched. M2 moves settings/mutation filesystem execution into its
  owned slot; no configuration/publication lock is held around that I/O.
- Useful cold medians S/R/L/N/E: 92.4/97.8/100.0/2121.6/2312.6 ms. N/E clip commands
  dominate at 1992.6/2196.4 ms. Direct probes take 1665.4-1817.3 ms for 50 clips;
  enrichment takes 19.0-24.2 ms sparse and 287.7-306.3 ms dense. This selects v2;
  it is not a measured candidate speedup.
- Media-duration delay mainly follows payload/mount; its cause remains unresolved.
  No payload split, container/delivery/backend rewrite or second diagnostic is
  authorized by this library finding.
- Cold means fresh process, not cold OS cache. Warm is five remounts within one
  process, initial mount separate, including the 100 ms remount timer. No p95/p99
  or independent-five-process warm claim.
- Thread/working-set growth flags remain watchpoints. Job I/O is not disk I/O.
  Preserve 011's accepted attribution limit and 009's deferred human A/V gate.
- S is generated native media; R/L are derived, not independent long recorder/League
  proof. Historical schema-v1 008 is not a current comparator. Strict v2 authority
  remains mandatory; optional 015 is not a prerequisite or a backend mandate.
- V2 uses a new Games-usable observation in both fresh reference and candidate,
  keeping old library-useful semantics. Its comparison is separately specified;
  do not top up or reinterpret the attribution trials.

## Blockers

Acceptance deferred by product choice. R4 launch 16 completed the application
scenario but failed process-telemetry completeness; that evidence remains invalid
for benchmark acceptance. No aggregate observer or main performance pass is
claimed. Unknown I/O/resource effects and the root-alias watchpoint remain
unproven; they do not authorize speculative product changes.

## Next action

No scheduled action. Keep the campaign discontinued and its evidence preserved.
Resume only when a specific product-driven justification identifies a concrete
question and bounded work worth doing; formal plan completion alone is not one.
