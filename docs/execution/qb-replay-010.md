# QB-REPLAY-010 execution checkpoint

Feature: `QB-REPLAY-010`
ExecPlan: `docs/exec-plans/qb-replay-010-library-snapshot-v2.md`
Updated: 2026-09-21

## Current milestone

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

No unresolved M3 or M4 engineering blocker remains. M4's primary comparison,
resource/reliability dispositions and user-assisted acceptance are complete.
The minimal replay descriptor, staged semantic timeline and playback
blocking-work disposition separately prevent full-feature completion. The first
campaign's N/L coverage gaps remain explicit and are not repaired by pooling.

## Next action

Do not launch another benchmark or resume the first runner. Preserve both
campaigns, the replacement's durable receipts and the completed engineering
disposition. Run the canonical validator, focused helper tests and normal
whitespace checks, then checkpoint M1-M4. The recommended next unit is the
minimal replay descriptor: it is the next unsatisfied useful-first dependency.
Do not begin it in this M4 closure session. The descriptor/staged timeline and
full-feature work remain separate; no automatic payload split or full-feature
completion follows this library result.
