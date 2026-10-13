# QB-REPLAY-010 — Integrated replay-opening acceptance

## Purpose

Establish whether the integrated application makes the Games library and selected
recording useful sooner, while preserving correct playback and bounded resource
use. Measure the actual Games-to-replay path, including media presentation before
optional details. Complete the remaining evidence for QB-REPLAY-010 without
assuming that another production optimization is necessary.

## Relevant planning-time architecture

The candidate product baseline is `a82d81f005823768a21b8aef4a94bd954ce26903`.
Its product tree is the September 27 consolidation at `13e68091`; the subsequent
commit changes documentation only. It includes the accepted library coordinator,
minimal replay descriptor and consolidated timeline/HUD. The historical reference
is `3a750e2c003d6e3605b2f907ff19ad7898d3dbd2`, the strict-v2, secured application
before QB-REPLAY-010. Freeze complete source and instrumentation patches for both.
This comparison measures the integrated change; intervening UI changes prevent
attributing a difference exclusively to the descriptor.

`LibraryController` owns frontend snapshot/token/root/navigation admission and
one active replay read plus one replaceable latest intent. `LibraryCoordinator`
owns blocking scan, replay-read, mutation and optional duration slots. Descriptor
and full-probe commands use the strict bundle reader independently. The descriptor
contains token, game ID, protected video URL and exact media timeline; it performs
no semantic projection. The optional full probe supplies semantic details after
two animation frames. Both reads remain bounded and off async workers. There is
no persistent replay cache and no existing aggregate-I/O reduction claim.

`ViewerScreen.tsx` retains one primary video/controller across details arrival and
fullscreen. `playbackController.ts` emits `first_presented_frame` from authoritative
RVFC evidence. `viewer_mounted` is emitted after media opening begins and is not
an exact media-open boundary. Exact replay ticks/frame intervals and opened-handle
delivery remain governed by `docs/architecture/replay-time.md` and
`docs/architecture/local-playback-security.md`.

The benchmark frontend is in `app/src/replayBenchmark.ts`, `App.tsx` and
`ViewerScreen.tsx`; Rust's bounded collector is in `app/src-tauri/src/benchmark.rs`.
`tools/replay_benchmark/{prepare.ps1,matrix.py,run.ps1,analyze.py}` own sentinel
preparation, immutable launch plans, Windows collection, validation and reports.
Existing manifest/matrix schema v1 supports cold/warm opening and observer controls.
Current raw events and reports use their existing schemas; add an explicitly
versioned opening measurement contract without changing old event meanings.

## Scope and non-goals

Add benchmark-only workload routing and bounded observation/analysis sufficient
to measure current opening. Build fresh matched subjects, run the specified
opening matrix and focused normal-build acceptance, and disposition the results.
Normal product behavior must be unchanged by the measurement implementation.

Keep closed library M4, descriptor and reconciliation evidence and all old plans
unchanged. Fresh measurements of the integrated tree do not reopen or pool their
campaigns. The benchmark-only historical duration drain remains intact for old
workloads. No new semantic pipeline, ReplayIndex, cache, payload split, decoder,
delivery route, recorder/media-policy change, HEVC campaign, capture benchmark or
optional frame-step feature is part of this unit. QB-REPLAY-013 owns a future index.
Do not restore archived/sibling work or the unrelated retained stash. No commit
or push is part of this plan.

## Exploration findings

The remaining feature gate is integrated replay-opening performance and native
acceptance. Semantic details are already optional; wording about an unimplemented
timeline in older checkpoint history is not a new backend requirement.

The existing benchmark has two material workload biases for this question:

- `App.tsx` drains every clip duration before historical `library_useful` and
  scenario navigation. This is compatibility work absent from normal Games opens.
- `ViewerScreen.runBenchmarkScenario` waits for `details()` before executing
  actions. It cannot establish media interaction while details are pending.

`opening_attribution.py` requires payload-before-mount and rejects a negative
payload-to-mount interval. Generic first-event opening metrics also do not describe
every warm cycle, and native/frontend calibrated timestamps cannot establish
cross-clock causal intervals. Retain those historical tools/metrics unchanged;
the new contract uses frontend intervals and explicit cycle/generation identity.

The reference has all-Games/Clips/usage/settings readiness before its LibraryScreen
becomes clickable; an Open button requires available media. The candidate requires
a current actionable snapshot and selected origin. The readiness mechanisms differ
legitimately. Both arms must observe their actual clickable Games view, not make
the reference artificially lazy or the candidate artificially eager.

## Chosen design and rationale

### One new diagnostic workload

Select the new behavior only for the exact scenario namespace
`qb010-opening-v1-`, with explicit validation of supported opening/control kinds.
Use `measurement_contract: "replay-opening-v1"` on its required events. Unknown
versions or malformed namespaces fail closed rather than falling through to a
legacy readiness rule. Normal mode and all existing namespaces retain behavior.

In each arm, emit a new `opening_library_usable` after two animation frames with
the Games view still actually clickable. Capture and revalidate the candidate's
root/request/token/navigation/actionable/not-refreshing origin. On the reference,
retain its real resource/media-availability conditions and revalidate navigation.
Record gate kind (`snapshot` or `legacy-resources`), fixture identity and finite
counts; never log a root, URL, capability or personal name. The candidate may reuse
the exact existing Games paint admission helper, without changing the
`games-library-usable-v1` event contract or interpreting it as a different gate.
Place this H.264 workload's admission before the legacy benchmark HEVC-capability
wait. Neither arm's actual clickable marker/navigation may wait on an unrelated
HEVC capability result. Preserve ordinary production capability initialization
and any eventual diagnostic observation; do not suppress normal startup work.
The new H.264 required-event set does not require a completed HEVC capability
probe. Legacy namespace capability gates and events remain unchanged.

Immediately open the selected fixture through that arm's normal App navigation.
For this namespace only, do not call the candidate's all-clip `drainDurations`.
Keep reference production eager work. Do not emit historical `library_useful`
early or synthesize it from the new marker; it is not a required event in this
new workload. Validate the new required-event set explicitly. Legacy workloads
still drain and emit `library_useful` with exactly their original semantics.

The new viewer workload may run its opening/play actions once the existing media
readiness and authoritative presentation prerequisites are satisfied, even while
details are pending. It must use the same controller and actions as a user; no
second player, fake RVFC, seek-based speed, forced eager probe, or altered optional
resource scheduling. Require successfully admitted full details before a positive
cycle/terminal result, with the existing finite timeout/error behavior. Details
may settle before or after presentation/actions; neither order is imposed.
Retain the controller's paused initial RVFC path: it arms presentation observation
on load/metadata and does not need an invented bootstrap play before first frame.
Remove only the new namespace's details prerequisite from action admission and
use an explicit details/action-completion latch before releasing/closing a cycle.
Test a paused initial frame and subsequent play while details are still held.
The observer-control seek workload uses the same opening entry path, then its
existing paused, briefly-play-to-present control sequence.

### Opening events and analysis

Carry one bounded opening-cycle identity from App intent through viewer resources
and controller admission. Cold has one cycle; warm has initial cycle 0 and measured
cycles 1–5. Bind cycle, fixture and navigation/request identity before the async
read, then bind the controller's actual load generation at media open. Mount-time
and media-load generations need not be equal. Dropped/superseded responses never
publish a current-cycle marker. Preserve existing event names and meanings.

Add only finite milestone events to the frontend minimal-event allowlist:

- `opening_requested`, immediately before normal selected navigation;
- candidate-only `opening_descriptor_requested` and
  `opening_descriptor_ready`, the latter after successful strict decoding/admission;
- `opening_media_open_requested`, immediately before the real controller open,
  with a bound result/generation event if needed to avoid predicting generation;
- `opening_details_ready`, after accepted full semantic details in either arm;
- `opening_library_usable` as above, joined to the existing frontend
  `library_requested` origin.

Reuse existing loadstart, metadata, canplay and authoritative first-presented-frame
events with explicit cycle/load association; do not add per-frame telemetry.
RVFC with the current generation is mandatory. A `currentTime`, RAF, `seeked` or
non-authoritative fallback cannot satisfy presentation. There must be one accepted
intent, media-open and first frame per positive cycle, plus exactly one admitted
details result; descriptor events are required only for the candidate role.
An explicit frozen subject role prevents accepting missing candidate descriptors
as a reference. Roles are evidence metadata, never production behavior switches.

Implement `tools/replay_benchmark/opening_acceptance.py` and a strict versioned
output schema/tests. It consumes fully validated raw bundles, retains the generic
schema-v2 report and produces additional per-cycle/per-process opening evidence.
Reject missing, duplicate, mixed-source, nonfinite, misordered, stale, cross-fixture
or cross-generation required records. Use differences within the frontend clock
only. Keep native processing spans and collector/Job counters in their own clocks;
never subtract native backend readiness from frontend intent.

Compare these shared metrics by exact scenario/fixture/config/runtime identity:

- frontend library request → actual clickable Games paint;
- selected intent → media-open request, loadstart, metadata, canplay and first RVFC;
- selected intent → full details (reported separately from media usefulness).

Candidate descriptor request → admitted descriptor, descriptor → media-open and
descriptor → details are candidate-only decomposition. Reference full-payload
timing is not a synthetic descriptor measurement. Optional details versus first
frame is an ordering fact; do not require a nonnegative payload-to-mount interval.
Retain initial warm mounts separately. Compute each warm process's median over
its five remounts, then compare five independent process values per arm. Never
treat five cycles in one process as five independent trials. Keep the existing
100 ms remount scheduling delay visible and identically defined in both arms.

Require all declared primary cells and metrics, both subject roles, equal trial
counts, explicit measurement version and matching non-subject fingerprints.
Different source/binary hashes are expected between arms and must be recorded.
Missing primaries cannot silently disappear from a comparison. Old analyzer output
must remain backward compatible; extend its namespace-specific validation to
route this workload without weakening legacy checks. The new report must not use
generic cross-clock opening intervals for acceptance.

### Subjects, fixtures and fixed matrix

Use a fresh root
`build/perf/qb-replay-010-opening-v1/.chronobreak-replay-benchmark`, refused if it
already contains a campaign. Preserve failures and all earlier roots. Prepare
isolated source copies with `git archive` (not a new active worktree), instrument
the reference only at corresponding observation/workload boundaries, and retain
exact patches/source hashes. Use one locked packaged runtime, toolchain, WebView,
device, display state and offline Data Dragon policy for both subjects. Reference
changes must not add candidate controllers, descriptors, lazy library behavior or
other production behavior. Build/freeze separate optimized benchmark executables
and a normal candidate; copied pre-existing binaries are not build evidence.

Use only generated, nonpersonal strict-v2 media. The retained generated S/R/L
sources below `build/perf/qb-replay-010/.chronobreak-replay-benchmark/generated/`
are read-only candidate inputs after identity validation. Copy into new fixtures;
never mutate old prepared/campaign/manual roots. If a source cannot be verified,
stop preparation and record the missing input; do not substitute real recordings.

Freeze five fixture strata before launching:

| Cell | Library and selected recording |
| --- | --- |
| S | Small three-game S/R/L library, selected four-minute H.264/AAC S |
| R | Same small library, selected thirty-minute H.264/AAC R |
| L | Same small library, selected sixty-minute H.264/AAC L with bounded event-heavy log |
| N | Fifty games and fifty ten-second clips; selected short H.264/AAC; sparse logs |
| E | Same N media/IDs/clip bytes, with bounded event-heavy logs |

R/L are derived scaling fixtures, not independent long recorder/League evidence.
Use existing corpus builders and strict-v2 validation; exact frame grid, media IDs,
calibration, event/snapshot counts and file identities belong in the frozen specs.
Use the established sparse/dense sizes (7/8 and 259/240 events/snapshots) where
valid for the selected media. All MP4s require packaged probe and full single-thread
decode. Both arms share each cell's immutable library and exact config bytes;
app-data, scratch and result roots are distinct. Retention is zero. No reparse
points, links or user media are permitted in benchmark fixtures. Cap prepared
corpus at 8 GiB before collection; do not change content once subjects are frozen.

The full feature specifies small/large, sparse/event-heavy, short/representative/
long coverage. Use all five cells for both `cold_open` and `warm_open`. Each cell/
kind has five adjacent reference/candidate pairs: 10 strata, 50 pairs, 100
production launches. Warm launches each have six cycles. Cold initial observations
and warm initial/remount observations are kept distinct. Use five-second active
warmup and five-second inter-process cooldown, seed 20261006, seeded stratum order
per round and alternating pair leaders. Freeze the complete order before launch 1.
Process timeouts are 120 seconds cold and 180 seconds warm; collector finalization
retains its separate 15-second bound. Preserve existing shorter action/readiness
timeouts rather than extending them to obtain a pass.

Before those 100 launches, collect four adjacent minimal/full 60-second observer
control pairs on R for each frozen subject: 16 processes total. They exercise the
same new entry/taps, use identical deterministic paused seeks, and differ only in
observer profile. Both subjects must pass control before the main campaign starts.
The ceiling is 116 automated measurement launches for this plan, excluding
non-launching preflights and the explicitly separate normal-build manual subject.

A runner must persist started/completed receipts and the frozen order, reject a
duplicate run/result directory, and stop on the first invalid/failed trial. After
interruption, inspect receipts and owned process state before doing anything else.
Only an untouched next manifest after a fully finalized successful previous run
may continue the fixed order. An incomplete/invalid attempt closes collection;
no selective top-up, overwrite, silent retry, or automatic replacement campaign.
Retain insufficient matrices as insufficient. A replacement requires a separate
recorded disposition/design, without pooling observations or changing thresholds.

## Rejected alternatives and planning decisions

- A hypothetical rollback switch on the current app would measure an invented
  reference. Use the actual before/after source states and disclose attribution limits.
- Reusing M4 numbers or one warm process cannot establish current integrated
  replay performance with five independent trials per arm.
- Running the full historical clip drain measures a different startup workload;
  renaming its early event would silently change historical meaning.
- Forcing optional details to finish before opening/play hides the accepted
  useful-first behavior. Dropping details from positive terminal validation would
  instead conceal failures; separate these two boundaries.
- More sparse cells alone would not cover the feature's long/event-heavy scope.
  Extra HEVC, seek/rate/export performance matrices are separate work.
- No speedup claim justifies relaxed strict-v2 parsing, delivery checks, source
  preservation, stale-result admission or weaker benchmark validity.

## Milestones

1. Implement the versioned workload, bounded taps and strict per-cycle extractor,
   preserving legacy behavior. Add focused frontend/Python contract tests and a
   minimal reference patch with parity tests. Update the durable benchmark protocol
   for the new contract. No live measurements until the contract tests pass.
2. Build/freeze subjects and prepare the fixed corpus/configs/order. Run all static
   preflights, then both observer-control matrices. Checkpoint exact subject/input
   hashes, observer dispositions and the next untouched launch before main collection.
3. Run the one fixed 100-launch matrix, validate complete results, extract matched
   cold/warm/process resource evidence and disposition every threshold/hard signal.
   A new product defect or materially unexplained regression stops completion;
   record the smallest affected unit for a separately reviewed corrective design.
4. Verify the current normal candidate on disposable A/B fixtures and close the
   applicable automated/native gates. Update checkpoint, feature evidence and any
   newly established durable facts. Mark done only if all unchanged criteria pass;
   otherwise retain the exact remaining gate. Do not implement speculative fixes.

## Verification design

M1 tests control held details with first-frame/play before release, both legal
details/presentation orders, failure/retry and superseded navigation, Games paint
away/away-back, held optional duration/Data Dragon/HEVC capability results, and no all-library duration
calls in the new workload. Legacy App readiness/drain and full-payload tests must
remain passing. Reference parity tests preserve its real clickable gate and eager
work. Analyzer fixtures cover strict event/version/role rejection, changed native
clock offsets, duplicate/missing/stale events, non-authoritative frames, first-load
versus mount generation, six warm cycles and equal independent counts, source/
config/runtime mismatch, missing primaries, and preserved old report behavior.

Use the existing commands; record exact commands, exits and subject/input identities:

```powershell
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
python -m unittest discover -s tools/replay_benchmark/tests -v
npm run desktop:build --prefix app
npm run desktop:build:benchmark --prefix app
git diff --check
```

Freeze the normal binary before building the benchmark into the same output path.
Build the reference with its corresponding benchmark desktop command in the
isolated source directory. Run focused tests before affected full groups; repeat
only checks invalidated by later changes. Do not rerun recorder/capture suites or
closed campaigns. Run canonical validation from VERIFICATION.md after final state
edits. Any environment-blocked or unrun check remains explicitly non-passing.

Let `$openingRoot` be the resolved fresh sentinel above; retain prepare specs and
all generated matrix-plan paths there. Static preparation and execution use:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <frozen-prepare-spec> -Manifest <new-template> -MediaRuntimeRoot <locked-runtime-root> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <frozen-prepare-spec> -Manifest <new-template> -MediaRuntimeRoot <locked-runtime-root>
python tools/replay_benchmark/matrix.py plan --template <new-template> --spec <frozen-matrix-spec>
python tools/replay_benchmark/matrix.py verify --plan <matrix-plan>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <ordered-launch-manifest> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <ordered-launch-manifest>
python tools/replay_benchmark/matrix.py verify --plan <matrix-plan> --require-results
python tools/replay_benchmark/analyze.py --input <minimal-run> --output-dir <minimal-report>
python tools/replay_benchmark/analyze.py --input <full-run> --output-dir <full-report> --observer-control <minimal-report/report.json>
python tools/replay_benchmark/analyze.py --input <candidate-run> --output-dir <candidate-report> --compare <reference-report/report.json>
python tools/replay_benchmark/opening_acceptance.py --campaign <frozen-campaign-spec> --output-dir <opening-report>
```

Repeat `--input` for every declared member of an aggregate. The new campaign-spec
schema binds subject roles, exact ordered run/manifests, expected strata/counts,
observation version, source patches and validation reports; it cannot discover
or select favorable runs. Generic analyzer comparison retains compatible resource
metrics; the opening report owns same-clock versioned primary comparisons.

Normal-build acceptance uses a fresh generated-media A/B library with overlapping
IDs, strict-v2 bundles, source hashes, isolated config/profile and retention zero.
Copy the existing scoped response-hold diagnostic only after checking its Tauri
transport compatibility and restoration self-test. It holds optional frontend
response delivery, not a backend-worker cancellation proof. Record actual human
audio/visual observations where native UI automation is unavailable.

Exercise A/V, play/pause/seek, zoom/pan and semantic filters; clip editing and
fullscreen mid-drag; held/failed/retried/late details without media/viewport reset;
rapid different-game navigation and Settings/exporter return; saved-state refresh;
one disposable clip deletion/export and A/B/A while optional work is pending.
Use normal protected playback/export and verify a generated export's H.264/AAC,
selected frame interval and full decode. Include a dedicated fixture-only alias
of root A plus A/alias-A/B/A selection to disposition the configured/resolved-root
watchpoint. Aliases are confined to this manual workspace, never the reparse-free
benchmark corpus; do not change containment/case validation to obtain acceptance.
Restore the diagnostic/config, verify zero pending held responses and closure of
owned app/listener, and compare all fixture hashes. Explain only deliberate fixture
mutations; unchanged recordings/logs must match. Preserve previous manual evidence.

AC1 remains supported by the closed attribution. M1/current automated tests and
native acceptance verify preservation of AC2–5; explicit descriptor/semantic
independence maps to AC3 and owned blocking tests to AC4. The ten matched strata,
source/resource gates and engineering disposition address AC6. The new normal
acceptance covers changed integrated UI boundaries; closed M4 storage internals
are not reopened merely to repeat their history.

Keep detailed raw receipts under the new ignored root and sanitized reports under
`docs/performance/evidence/qb-replay-010-opening-v1/`. Include all attempted runs,
per-cycle/process values, matched summaries, comparison eligibility, hashes,
source/config/runtime fingerprints, observer controls, native observations,
deviations and a gate-by-gate disposition. No capability URLs or personal paths.

## Performance and reliability gates

Retain the original Games improvement requirement for N/E: both cold and warm
process-start Games-readiness medians improve by more than 5% and
`max(5 ms, 3 * matched reference MAD)`. The new actual-clickable marker has its own
measurement version; never pool it with historical `games-library-usable-v1`.
S/R/L library and all selected-intent-to-first-frame cells must have no unresolved
disposition-requiring regression. The established targeted optimization is Games
opening; do not require or claim an invented independent descriptor speedup.
Report any measured replay improvement with its integrated-source attribution.

Use five independent processes per arm/stratum and compare medians. Do not claim
p95/p99 with fewer than 40 observations in that exact stratum, or treat within-
process remounts as independent. An unfavorable change requires disposition when
it exceeds both 5% and `max(resolution floor, 3 * reference MAD)`: floors are
5 ms latency, 0.1 normalized CPU percentage point, 8 MiB memory, and 1% bytes/counts.
Evaluate process CPU/private/working memory, peak/late growth, handles/threads,
Job read/write bytes/operations, route-level delivered bytes/requests, losses,
cancellations and playback quality. Job I/O is aggregate accounting, not disk I/O
or proof that one JSON read caused a delay. Startup and full-process costs remain
distinct; no shared-clock assumption supplies absent phase-level resource samples.

Observer controls require identical deterministic action/semantic outcomes, zero
required loss; count/byte/drop medians within the larger of resolution (one unit
or 1%) and three MAD unless the increase is at most 5%; seek median/p95 increases
at most 5%/10%; private peak increase at most 16 MiB; CPU increase at most the larger
of 2% or one accounting quantum per process. Failed controls prevent main collection.

Wrong/stale results, source mutation, playback failures, recovery/timeout, missing
core telemetry or required events, invalid identity, and sustained-growth signals
always require disposition. Product correctness/source preservation failures
cannot be accepted as benign noise. Do not waive new startup I/O amplification as
the old compatibility-drain cost: that drain is absent here. A genuine bounded
counter/workload artifact needs concrete evidence and an explicit limitation;
unexplained amplification or sustained growth prevents completion. Passing measured
windows does not establish an indefinite leak-free soak or hardware-wide behavior.

The instrumentation must retain bounded event queues and owned process/child
lifetimes, add no per-entry task fan-out, timed hashing, production probe or new
delivery authority, and hold no lock across I/O/await. Preserve strict v2 and
fail-closed delivery throughout. Any needed production repair must be justified
by a concrete failed invariant and its own reviewed implementation boundary.
