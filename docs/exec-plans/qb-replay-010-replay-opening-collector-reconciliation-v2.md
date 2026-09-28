# QB-REPLAY-010 — Replay opening collector reconciliation v2

## Purpose

Correct the replay benchmark collector's Job/notification acquisition race, then
measure the accepted combined descriptor/UI opening under one fresh, fixed
comparison. Reliable telemetry is required to assess opening and resource costs.

This design supersedes
`docs/exec-plans/qb-replay-010-replay-opening-comparison.md` for the affected
collector and future comparison. Preserve that immutable plan and its closed
raw campaign. Accepted library, descriptor and UI work remains the baseline.

## Relevant planning-time architecture

The candidate product is integrated commit
`a840c645e5080ec13a7cb23c540eabc388676a83`. Its parents are descriptor commit
`3426cfa0340373b13a9aa4930b75f41271b7c0d2` and UI commit
`6c7174fd8edb4f299e0a0ee0b77a1683a96ee629`. The reference is
`3b14df20aa7ab4e567fd0372c1abc72c07bbe18e`, the accepted library implementation
immediately before the descriptor. This measures the combined descriptor/UI
change, not a causal separation. Existing replay benchmark tooling is identical
between these reference/candidate commits.

`LibraryController` owns snapshot/origins and one-active/one-latest selected
reads. `LibraryCoordinator::read_replay` in
`app/src-tauri/src/library_coordinator.rs` owns one blocking descriptor/full-probe
slot through worker completion; commands in `lib.rs` do not parse bundles on async
workers. `library.rs::replay_descriptor` uses the strict bundle reader, validates
both JSON files and returns four media fields. Optional full details intentionally
read JSON again. The descriptor avoids semantic projection, not validation
proportional to log size.

`ViewerScreen.tsx` mounts the persistent media/controller from the descriptor and
requests optional details after two animation frames. Benchmark actions wait for
both details and presented media regardless of arrival order. Participant cards
reuse existing parsed logs; their projection and the shared timeline add work
whose aggregate performance has not been measured.

`App.tsx` emits frontend `replay_requested` before opening and
`viewer_reopen_requested` before each warm disposal/remount, including the existing
100 ms remount delay. The loadstart generation identifies media events.
`playback_payload_ready` means the full semantic probe and may precede or follow
mount/frame. `games-library-usable-v1` requires scenario IDs prefixed
`qb010-library-v2-`, an admitted Games view and two-frame paint opportunity.
Historical `library_useful` additionally waits for the benchmark-only optional
clip-duration drain. Both arms have that same drain.

The existing runner, matrix verifier and analyzer own source identity, Windows
Job collection, terminal validity, request accounting and sanitized comparison.
Minimal observation retains required opening/controller events and core
process/server evidence. Full-observer overhead control applies to full-observer
campaigns; this design uses minimal observation only.

## Scope and non-goals

Change only the external runner's Job snapshot / notification acquisition,
focused Windows tests, and corresponding collector protocol documentation.
Reuse the opening comparison, global driver, matrix isolation and environment
admission. After the collector gate, collect one new sealed comparison with
at most 100 app launches. The closed seven-attempt campaign plus this ceiling
give a lifetime opening-comparison maximum of 107 attempts. Forfeit all 93 unused
old slots; no continuation, pooling, pilot, app smoke or selective replacement.

No product behavior, frontend instrumentation, event/manifest schema, scenario
driver, strict-v2 parser, decoder, delivery route, persistent index or semantic
backend change. No M4 restart, held-details performance scenario, export/rate/seek/
capture campaign, QB-REPLAY-009 human acceptance, HEVC or independent long-recorder
claim. Keep work uncommitted; preserve sibling edits. A product defect needs
focused disposition and a separately reviewed design if its fix changes scope.

## Exploration findings

The collector snapshots Job accounting before a second notification drain.
Creation during that drain can advance the notification count beyond the older
Job total. The failed observation exhibits 28 total processes versus 29 creation
notifications. Later and terminal equality cannot repair the rejected sample.
The same acquisition window exists at finalization. A deterministic diagnostic
using the actual extracted functions reproduces the race with native boundaries
mocked; this is not a native or performance pass for the proposed correction.

The current native `DrainNotifications` loop drains until the queue is empty.
An outer deadline alone cannot bound continuous churn. Bound each native batch
as part of correcting acquisition; do not add another observer or relax validity.

The original `tools/replay_benchmark/opening_attribution.py` assumes full payload
before mount and computes payload-to-mount. Leave it and historical reports
unchanged. Generic analyzer validity requires both events without that ordering
and can validate both subjects. No additional production tap is needed for
request-to-presented-frame comparison. No descriptor-ready event exists, so this
unit cannot measure strict descriptor duration or attribute a change to overlap.

Criteria 2–5 have implemented mechanisms and accepted deterministic/native
evidence; no finding establishes a further semantic pipeline requirement.
Criterion 6 still needs combined replay/resource evidence, particularly for
duplicate JSON validation. Accepted M4 improvement is a separate earlier
before/after comparison. The new reference already contains that improvement.

Retained strict-v2 manifests are under
`build/perf/qb-replay-010/.chronobreak-replay-benchmark/manifests/`:
`prepared-srl.json`, `prepared-n.json`, `prepared-e.json`. Corresponding
`library-srl`, `library-n`, `library-e` directories are read-only sources for
fresh copies, never execution roots. S/R/L have 240/1800/3600-second H.264/AAC
media at exact 60 FPS. R/L are derived scaling, not independent recorder sessions.
N/E each have 50 games/50 clips; selected `scale-00` uses identical 240-second media
with sparse versus dense logs. Do not describe N/E as representative-duration
media. Recheck retained source hashes and fully decode fresh positives with the
packaged runtime. A missing/changed source is setup failure, not permission for
silent substitution.

The five cells cover workload axes, not their full Cartesian product: three
media durations in a small library and matched sparse/dense large libraries.
This exercises known scan cardinality, selected JSON cost and media-open duration
cost without turning an opening check into a factorial campaign. E's selected
log is about 1.87 MB versus R/L's 0.44/0.89 MB, so the largest selected parse is
covered alongside the longest media. It does not establish every long-media ×
large-library interaction; retain that coverage limit in the result. The feature
specifies these workload classes, not all combinations. No universal library-size,
codec, duration or causal-independence claim follows.

## Chosen design and rationale

### Bounded Job accounting reconciliation

Retain the suspended-start, assign-before-resume, non-breakaway kill-on-close Job
and one completion-port consumer. Correct the producer in
`tools/replay_benchmark/run.ps1`; keep analyzer and terminal validity strict.

Replace the stale-snapshot `Sync-JobNotifications` acquisition pattern with one
shared `Get-ReconciledJobObservation` helper, used by `New-ObserverSample` and
terminal `collection.job_accounting`. Return a captured object with the fresh Job
snapshot, creation-notification count at that observation, and all notifications
consumed during it. Use that captured count in membership, aggregate and final
output, never a later global read. CPU/I/O and cumulative Job totals come from
the same snapshot. Live process rows/PID lists remain sampled views; equality is
an accounting checkpoint, not an atomic Windows process-table snapshot.

Own one monotonic deadline: 100 ms per ordinary sample, 500 ms for final accounting
(the existing two 250 ms final budgets combined). Start it before the first drain.
The first drain is nonblocking. After every drain query a fresh Job snapshot and
capture the count. If counts differ in either direction, repeat a bounded drain
and fresh query while budget remains; never use a permanently cached expected
total or reset the deadline. Requested notification waits are at most 10 ms,
capped to remaining whole milliseconds. Recheck before native calls and after
each pass. Preserve actual elapsed acquisition in existing sample timing.

Change native `DrainNotifications` to return a
`QueueBackReplayJobNotificationBatch` with `Notifications`, `NativeErrorCode`,
`QueueEmpty` and `HitBatchLimit`. Add production `Get-JobNotificationBatch` as a
PowerShell wrapper around that call. `Receive-JobNotifications` appends every
returned notification and advances the existing creation count before the helper
handles an error. Its returned batch retains the metadata as well as the records.
No live CLI fault-injection switch or second consumer is introduced.

Bound a native call to 256 messages in production (tests may request 1-256).
The batch invariants are: 0 <= record count <= cap; `QueueEmpty` means an actual
empty/timeout result; `HitBatchLimit` means exactly cap records; those two flags
are mutually exclusive. A non-timeout native error returns the partial records
and nonzero `NativeErrorCode` instead of throwing away its local list; it always
fails, even when creation counts happen to match. A full batch requires another
pass to observe a drained queue. Keep queued messages and every consumed record.

Factor the native dequeue loop into a small method taking a dequeue callback.
Production supplies the existing `GetQueuedCompletionStatus` adapter; a test
supplies a deterministic sequence of records followed by a non-timeout error,
empty/timeout or cap boundary. Exercise the actual shared loop, not a test copy.
Keep the public native method's return/error contract above; unexpected exceptions
still fail with all available diagnostics. This seam proves native partial-batch
preservation without relying on probabilistically closing a live port mid-drain.

Also cap the entire acquisition at 32 batches per ordinary sample and 128 at
finalization (8192 / 32768 received messages maximum). Expiry or pass-cap exhaustion
fails strictly, without another drain/query attempt. The one deadline still
applies across all passes; neither cap extends it. This bounds requested waits
and collector work; it is not a hard real-time Windows/native-return guarantee.

Success requires QueueEmpty, no native error, exact equality and completion
within both the deadline and pass cap. Validate batch invariants before use.
Expiry before success fails through existing `PROCESS_JOB`, with the last actual
total/count/batch and expiry reason in diagnostics. Before throwing, write a
failure-only `job-reconciliation-error.json` under the created result root:
reason, elapsed/budget, last available snapshot/count, batch facts and every
notification consumed by this acquisition. Earlier successful acquisitions
remain in their existing artifacts. Preserve partial native-batch records before
propagating a native API error; do not lose them merely because the native drain
failed after receiving messages. This diagnostic is tool-local failure evidence,
not a new valid sample or a substitute collection-result. Write it through an
exclusive temporary artifact and publish without overwrite;
leave a partial artifact intact if publication fails. A diagnostic write failure
must preserve both the original reconciliation cause and the write error in the
runner failure message, never replace the original cause or become success.
Cleanup still runs through the existing owned Job path; propagate native errors.
Never fabricate equality, clamp differences, discard samples, skip initial
samples, retry the app or reinterpret old evidence.

Finalization uses the same helper after existing root/child grace and owned
cleanup. Keep exact gates: active=0, forced terminations=0, unobserved=0,
notifications=total, unchanged hashes and finalizer duration <=15 seconds.
A bad active sample remains invalid after a complete final pair. Keep one-second
cadence, the 1500 ms cadence flag and 2500 ms adjacent-gap validity ceiling.

Update `docs/performance/replay-benchmark.md` to state this acquisition boundary.
Wire schemas and metric meanings stay unchanged: the correction acquires the
existing fields coherently. Frozen tool hashes distinguish old/new collection;
both replacement arms use the identical corrected collector.

### Collector verification boundary

Add a PowerShell harness that AST-extracts only the actual named definitions
from `run.ps1`, without executing its top level. Invoke it from Python unittest
with a finite subprocess timeout. Override the actual production
`Get-JobNotificationBatch` and `Get-BenchmarkJobSnapshot` wrappers at their boundaries.
Deterministically exercise stable equality, notification-ahead 28/29,
snapshot-ahead delivery, multiple advances, missing/duplicate notifications,
continuous full batches, invalid batch metadata, native errors after partial
records, deadline/pass-cap exhaustion and diagnostic write/publication failures.
Verify every consumed notification survives, deadline is not reset, waits are bounded,
and sample/final CPU/I/O/count fields use the returned captured observation even
if the global count later changes. Cover both sample and final integration,
not just the helper. Test the native shared dequeue loop's partial-record/error
return separately from the PowerShell wrapper consumer's preservation of that
return. Static substring assertions alone are insufficient.

Keep `Test-ProcessTelemetryComplete` and generic analyzer rejection fixtures for
nonzero unobserved counts, missing samples, excessive gaps, forced termination
and invalid final accounting. Add tests only for assertions not already covered.

Add a finite native Job harness using the existing native definitions: create
Job+port, start a dedicated disposable shell suspended, assign before resume,
and start at most 64 short-lived children inside that Job. No app/media/user data.
Temporary logs and finally-owned cleanup are mandatory. Cap each native case at
10 seconds and the subprocess at 30 seconds; terminate only its own Job on failure.
Exercise multiple native batches (a small test batch limit may be configurable;
production is 256), record exact new/exit counts, final equality, active=0,
acquisition durations and closure. Native timeout/failure fails the gate.
This proves collector/native behavior, not opening performance or observer
overhead, and is not an extra production app launch.

Review the producer correction and tests before freezing a new campaign. Run the
full replay tooling suite because the collector is shared. Do not repeat accepted
product/native checks when their sources and product bytes are unchanged.

### Offline measurement contract

Reuse `tools/replay_benchmark/opening_comparison.py` and its focused tests with
output contract `replay-opening-comparison-v1`. Consume only fully validated run
bundles and the frozen campaign mapping. Never promote an invalid generic bundle
to valid or weaken existing comparison/validation.

Use frontend events for durations. Partition by initial `replay_requested` and
warm `viewer_reopen_requested` boundaries in validated analyzer monotonic order,
retaining original JSONL order for equal-time ties. Each completed
cycle requires one mount, one loadstart generation, metadata, first canplay, an
authoritative RVFC first frame, one boundary-local full-payload observation and
cycle/final completion. Full payload may occur before or after mount/frame but
must occur before its cycle completion. Media observations must match loadstart generation; the mount event can
carry an earlier controller generation. Reject missing/duplicate boundaries,
ambiguous loads, stale media, negative request durations, incomplete cycles and
non-authoritative frames. Full-payload events have no load/generation key: their
boundary-local timing is not proof of payload identity. Missing/duplicate events
or a previous cycle completing without its payload reject; an otherwise
indistinguishable lone stale payload cannot be detected by this telemetry. Retain
that explicit limitation, relying on accepted keyed-controller and native
correctness evidence rather than claiming a new generation-bound payload proof.

Report request-to-mount/loadstart/metadata/canplay/first-frame/full-payload,
mount-to-media milestones and signed `full_payload_ready_minus_mount_ms`.
Never revive the old nonnegative payload-to-mount assumption. Do not causally join
native/backend/server records to frontend cycles. Those facts retain their own
clock and aggregate scope.

Cold comparisons use five process values per arm/cell. Warm comparisons use five
independent processes per arm/cell: retain initial mount separately and exactly
five remounts. Compute each process's remount median first, then compare the five
process medians and reference MAD. Retain all 25 rows without treating them as
independent processes. Warm startup/library metrics stay separate from remounts.
No p95/p99 claim. The new comparison must require every declared primary
metric and every declared gated resource/readiness metric on both arms with a
symmetric required metric inventory and balanced eligible evidence; a missing/ineligible metric
is insufficient, not a pass because generic comparison emitted no regression.
Apply the same threshold function to the custom warm process-median rows and
retain its counts, reference MAD, band and eligibility in output.

Require matching fixture/scenario/config/runtime/environment fingerprints using
existing normalized comparison identity; do not weaken exact config-SHA checks.
`opening_environment.ps1` AST-imports existing runner identity functions, refuses
WebView overrides and freezes the unique registered runtime's version/hash plus
machine/display/runtime fields. Run it again before each launch. Actual live
normalized metadata must exactly match. Installed WebView bytes are checked
prelaunch; live telemetry identifies descendant versions, not executable bytes.
CPU accounting quantum is independently sampled in each runner; any mismatch
rejects without tolerance or retry. A fresh campaign freezes a fresh internally
matched environment, not the closed campaign's incidental installed version.
The signed payload-minus-mount metric is required on both arms as a neutral
ordering diagnostic, not a lower-is-better percentage gate. Full-payload
request latency remains gated. Explicitly freeze required metric names and
units in the output contract; GPU absence is a declared limitation, not a zero
or mandatory metric. Allowlist sanitized aliases, timings, counters and hashes. Never output bearer
URLs, capabilities, local account paths, raw game logs or user identifiers.

### Fixed campaign and subjects

Use new sentinel
`build/perf/qb-replay-010-replay-opening-v2/.chronobreak-replay-benchmark` and
ignored workspace `build/qb010-replay-comparison-v2/`. Refuse existing plan/
launch destinations; preserve interrupted preparation without silent overwrite.

Reuse the exact archived subjects from the closed campaign after verifying source
archives, commit/build receipts, binary hashes and identical packaged runtime.
Reference SHA-256:
`34bd87bda70ea723a9c2d874a8c612038be3fb2149073d6753fc24ea48bd41c0`.
Candidate SHA-256:
`46323b37c1bb7e29d2df89f5dca115e7837796c1eacc5d7342b517ebbe854f1f`.
Copy archives, optimized binaries, build receipts and runtime into the new sentinel
with original-build provenance. Do not rebuild unchanged accepted product for a
collector change. If reuse identity cannot be proven, stop setup for disposition;
do not substitute another binary. Runtime remains
`queueback-ffmpeg-8.1.2-windows-x86_64-r6`. Retained provenance binds
`npm run desktop:build:benchmark --prefix app` to each exact archived commit.
Never reset this worktree or change the sibling. Documentation/tool edits do not
invalidate the accepted product/native subject.

Prepare three fresh libraries from the explicit retained manifests. S/R/L select
their matching fixture from the same three-game/no-clip library. N/E select
`scale-00` from their respective 50-game/50-clip libraries. Within each cell,
both arms share the immutable library and exact config bytes, retention 0,
HEVC disabled, offline empty Data Dragon state, and have fresh isolated
per-launch app-data/scratch/results. Refuse existing destinations. Full decode,
source/copy SHA-256 and write-time receipts precede measurement. Stage at most
8 GiB of media; never delete old sources/results to make space.

The matrix is five cells S/R/L/N/E × cold/warm × five independent trials × two
arms = **100 maximum production launches in 50 adjacent pairs**. Five warm
processes support process-level comparison; one process's remounts cannot provide
five independent observations. Cold uses `cold_open`; warm uses `warm_open` with
`iterations: 6`. Both set `warmup_seconds: 5`, use existing play/stable/pause
actions, minimal observer and matching fixture/parameters. Matching scenario IDs
start `qb010-library-v2-replay-opening-` to retain the Games metric contract.

Freeze twenty per-arm/cell/mode matrix plans of five launches and one global
order before collection. Generate five seeded rounds (20260923), shuffle the ten
cell/mode pairs per round, alternate leading arm by pair ordinal, and use five
seconds cooldown between launches. Runner timeout is 180 seconds per process;
existing finite shutdown/collector finalization/post-hash rules are unchanged.

Every preflight must pass before collection. No pilot, extra live smoke,
opportunistic repetition, selective replacement or automatic retry is outside
the ceiling. An invalid launch stays preserved, stops collection and makes the
campaign insufficient. Further collection/replacement is outside this plan and requires a separately
reviewed disposition. At interruption, retain started/completed receipts and verify exact
subject closure. Resume the next unused manifest only if every started launch
has valid terminal/collector/post-hash evidence, cooldown is met and all frozen
identities still match. An ambiguous/incomplete started slot is invalid; never
restart it or infer success from process absence.

Reuse `tools/replay_benchmark/opening_campaign.py` around existing runner/
matrix mechanisms. Separate matrix plans bind different prepared templates and
subjects; they cannot verify inter-plan order. The new sentinel's immutable
`campaign-plan.json` (tool-local schema version 1, no app manifest-schema change)
and this tool's final verifier own the global contract. Each of 100 entries
binds sequence, pair, round, cell, mode, arm, parent matrix-plan path/SHA-256,
launch-manifest path/SHA-256, run/trial/scenario IDs, frozen subject SHA-256 and
unique result root. All paths resolve within the new sentinel. Each pair must
match the declared cell/mode and contain exactly one reference and candidate,
with identical scenario IDs and normalized comparison identity (fixture identity,
exact config SHA, minimal observer, runtime, Data Dragon and environment), removing
only the existing implementation-subject material. Enforce this at campaign
plan generation, prelaunch admission and final verification; a shared scenario
prefix alone is insufficient. Verify exact membership in all twenty plans and the frozen seed/order/cooldown.

Use an exclusive one-shot/resume lock and write durable per-sequence started
receipts before launch; completed receipts bind the exact bundle validation and
terminal/collector/post-hash identities. The final verifier requires all 100
slots, verifies their global start/completion order and five-second cooldown,
compares every frozen identity, and rejects missing/duplicate/restarted slots,
unplanned launch receipts/result roots, partial runs or ceiling violations.
Pass all twenty local verifiers AND this global verifier. Resume checks the same
prefix invariants before advancing and cannot rewrite started/completed history.
Test cross-plan order violations, unmatched pairs, altered subjects/manifests,
extra/duplicate/partial slots and interrupted-prefix admission using fake runs.
The driver may only launch frozen order, validate completion and advance; no
overwrite or unrelated process termination. Declare display/background conditions
before collection. Windows remain visible/always-on-top with no operator input;
hiding/minimization, core telemetry loss or environment identity drift invalidates
evidence under existing rules.

## Rejected alternatives and planning decisions

- 3426cfa as reference hides the descriptor change; old M4 measurements mix
  products/conditions and cannot be the new matched reference.
- New descriptor events/schema or readiness drivers increase scope unnecessarily.
- Full observation adds an overhead-control campaign without a needed metric.
- One warm process characterizes that process, not five independent trials.
- Rewriting duplicate parsing before measuring its harm skips attribution.

## Milestones

1. Implement the shared bounded acquisition helper, native drain cap, sample/final
   integration and focused tests. Prove race and genuine-loss cases, run the
   native disposable-Job harness and full replay suite, review the correction,
   and document the acquisition contract. Stop if a collector gate fails.
2. Freeze one replacement with the proven collector and verified unchanged
   subjects. Copy/hash/decode fresh fixtures, freeze environment, twenty matrices
   and 100-slot order, and pass all no-launch preflights. Record new 100 / lifetime
   107 ceilings and closed predecessor identity before collection. Collect once
   using existing exclusive ownership and receipts.
3. Verify each bundle, all local matrices and complete global order. Produce
   sanitized cycle/resource comparisons; disposition every regression and
   individual growth/error signal. Reconcile unchanged acceptance criteria.
   An invalid slot stops this replacement; no further campaign is authorized.
   Keep exact unresolved findings if a gate cannot pass.

## Verification design

Run tooling checks and commands from the relevant source root:

~~~powershell
python -m unittest discover -s tools/replay_benchmark/tests -v
python -m py_compile tools/replay_benchmark/opening_comparison.py tools/replay_benchmark/opening_campaign.py
python tools/replay_benchmark/matrix.py verify --plan <matrix-plan>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <manifest> -AppBinary <frozen-subject> -TimeoutSeconds 180 -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <manifest> -AppBinary <frozen-subject> -TimeoutSeconds 180
python tools/replay_benchmark/analyze.py --input <run-root>
python tools/replay_benchmark/matrix.py verify --plan <matrix-plan> --require-results
python tools/replay_benchmark/opening_campaign.py verify --campaign <campaign-plan> --require-results
python tools/replay_benchmark/opening_comparison.py --campaign <campaign-plan> --output <fresh-comparison-json>
python tools/replay_benchmark/analyze.py --input <candidate-run> --output-dir <candidate-report> --compare <matched-reference-report>
git diff --check
~~~

Repeat analyzer `--input` for five explicit process roots per matched cell/mode/
arm; never aggregate unlike cells. Use the existing extractor CLI and freeze
resolved invocations in the new campaign inputs. Do not execute placeholders. Test the driver with a fake
runner/process, not extra app launches. Run canonical validation from
VERIFICATION.md at handoff/closure.

Tests accept payload-before-mount and frame-before-payload; reject malformed,
stale media/non-RVFC/incomplete data; cover a payload crossing a boundary,
reject observable missing/duplicate evidence and disclose unobservable payload
identity rather than fabricate correlation; separate six warm cycles and initial mount;
enforce five-process aggregation, matching identities and sanitized output.
Retain generic analyzer validity and legacy extractor tests. Targeted source/test
inspection must prove needed events preserve meaning in both subjects; no parity
patch is planned. Unexpected incompatibility stops preparation for disposition.

Criteria 1–2 retain closed attribution/M4 evidence. Criteria 3–5 retain descriptor,
coordinator and combined UI automated/native evidence bound to unchanged candidate
product sources. Tool tests do not replace native A/V acceptance. Criterion 6
combines separately identified historical library improvement with new combined
replay/regression evidence; never pool or arithmetically combine historical
speedups. Further product changes require design reconciliation and invalidate
only their affected verification.

Retain raw artifacts under the new sentinel and sanitized reports under
`docs/performance/evidence/qb-replay-010-replay-opening-v2-<campaign-date>/`.
Include source/build/runtime identities, order/manifest hashes, every individual
and aggregate measurement, initial/remount rows, matrix checks, failure receipts,
integrity proofs, test/build commands and engineering dispositions. Actual dates
and results belong in evidence/checkpoint, not this plan.

## Performance and reliability gates

Primary metrics are frontend request-to-authoritative-first-frame and
Games-request-to-Games-usable. Also gate metadata/canplay, full-payload readiness,
historical library useful and whole-process resource/request costs; earlier media
must not hide a deferred-work regression. Speedup claims require improvement
beyond both 5% and reference repeatability band. The prior M4 improvement does
not require a second descriptor speedup; this unit establishes combined regression.

Use five matched process observations per arm/stratum. Band is max(resolution
floor, 3 × reference MAD), with unchanged floors: latency 5 ms, normalized CPU
0.1 percentage point, memory 8 MiB, byte/count 1%. An unfavorable median change
requires disposition beyond both band and 5%. Every new error, timeout, recovery,
source mutation, required loss, stale/wrong result or sustained-growth signal
requires disposition. Inspect individual traces, private/working memory,
handles/threads and late-cycle behavior, not just medians. Six mounts do not
prove indefinite leak freedom.

Retain aggregate Job versus physical disk-I/O, cancellation versus child failure,
missing GPU, fresh-process versus cold-OS-cache and derived-media limitations.
Both new arms have the compatibility drain; its old disposition does not excuse
new amplification without evidence. Strict terminal/Job/source validity is
mandatory. An explanation is not an automatic pass: record measured cost, cause,
product relevance and action for every flag. Unresolved harmful regressions keep
QB-REPLAY-010 in progress. No tolerance changes, schema relaxation, selective
sample removal or unsupported completion claims.
