# QB-PERF-005 recorder review remediation addendum

Status: implementation in progress; Packages 0-4 complete.

Date: 2026-08-18

## Authority and purpose

This is a PC-B implementation addendum to the authoritative QB-PERF-005
native-recorder plan. It does not replace that plan, create a new backend, or
authorize Milestone 8 or 9. The PC-B subset does not contain the canonical
`feature-list.json`, `PLANS.md`, application reader, or complete architecture
documentation. When this branch is reconciled with the full repository, link
this addendum from QB-PERF-005's canonical state and record any contradictions
before implementation continues.

The outcome is a recorder that retains the current media, lifecycle, and
bounded-resource contracts while removing confirmed avoidable work and making
transient startup failures recoverable. Every performance change must be
independently measurable and independently revertible.

The product priorities remain, in order:

1. recording reliability;
2. negligible measurable League impact;
3. correct, recoverable media and game data;
4. optimization of recorder overhead.

## Review disposition

| ID | Finding | Assessment | Disposition |
| --- | --- | --- | --- |
| R1 | Seven invariant D3D11 video-processor setters execute for every output frame | Confirmed. At 60 FPS this is about 420 avoidable driver calls per second. | Hoist to processor creation/reconfiguration first. |
| R2 | Every admitted WGC frame is copied into the source snapshot before 60 Hz CFR admission | Confirmed and contrary to the QB-PERF-005 timing design, which selects the newest admissible source for each output tick. | Introduce a bounded latest-pending-frame state and copy at most once per output tick. |
| R3 | Any startup or runtime failure poisons the League PID until it exits | Confirmed. A transient capture, driver, or loading failure can lose the rest of a match. | Add typed failure disposition and capped-backoff startup retry. Runtime restart requires a separate segment contract. |
| R4 | A late worker emits overdue ticks in a tight catch-up loop | Confirmed. More importantly, the clock advances before slot admission, so a full encoder ring can shorten the video while audio continues. | Make tick advancement transactional and catch up in bounded, interleaved batches. Do not skip timeline ticks with the current raw-H.264 mux boundary. |
| R5 | Process and exact-target validation do more work than needed every two seconds | Confirmed. Process refresh requests all metrics; target validation repeats visibility checks and recreates/enumerates DXGI state. | Use minimal process refresh and cached target validation. |
| R6 | Poller persistence clones and pretty-serializes the complete growing log on every event batch and snapshot, then forces a durable atomic rewrite | Confirmed algorithmically, but existing evidence does not show it is the current capture bottleneck. | Instrument first; add a coalescing single writer only if the measured gate is crossed. |
| R7 | `storage.auto_delete_days` is configured but unused | Confirmed product debt, not a recorder hot-path optimization. | Track as a separate destructive-retention feature in the full repository. Do not implement it in this plan. |
| R8 | Strict Clippy passes while pedantic/nursery produce hundreds of mostly structural warnings | Not a runtime defect signal. Broad cleanup would obscure performance attribution. | Do not pursue warning-count reduction. Fix only stale or misleading documentation touched by this work. |

No P0 memory-safety defect, unbounded video queue, full-frame host readback, or
steady-state full-size texture allocation was found. The unsafe/FFI surface
therefore receives focused regression tests, not a broad rewrite.

## Hard invariants

- WGC pool capacity remains exactly 2.
- Callback-to-worker handoff remains capacity 1 and nonblocking.
- At most one source frame may be pending in the worker; pending plus handoff
  may never exceed the two WGC pool surfaces.
- NV12/NVENC slot count and maximum in flight remain exactly 4.
- The WGC callback never waits for the worker, encoder, mux, disk, or async
  runtime.
- The D3D11 immediate context remains owned by one GPU worker.
- The NVENC completion thread remains bounded to the four completion slots and
  performs no D3D11 immediate-context work.
- No full-frame CPU readback, software pixel conversion, hidden cross-adapter
  copy, or automatic fallback is introduced.
- Both native and FFmpeg reference backends remain available until the real
  League Milestone 8 decision.
- The pinned r6 runtime and QB-PERF-002 attribution are not changed.
- Existing partial recordings and failed-attempt evidence are preserved.
- No League process is started on PC B.

## Recorder/replay boundary clarified by the replay-engine idea brief

The advanced replay ideas strengthen the case for co-designing recorder output
and replay behavior, but they do not justify changing the recorder format in
this remediation pass. They add one immediate requirement: timestamp domains
must be named and tested so a future replay index, seek engine, clip exporter,
or segment model does not mistake capture time for presentation time.

Keep these domains distinct:

| Domain | Authority | Intended use |
| --- | --- | --- |
| Source/capture time | WGC `SystemRelativeTime` / QPC in 100-ns units | Source freshness, monotonic capture evidence, first-frame anchoring, and health. It describes when pixels surfaced, not when an output sample is presented. |
| Media presentation time | CFR output tick index with exact rational time base 1/60 | MP4 PTS/duration, frame stepping, keyframe/sample indexing, replay seeks, and clip boundaries. Tick zero is media time zero. |
| Game time | Live Client game seconds mapped through the calibrated video offset | Event navigation, semantic preview density, bookmarks, and match-relative UI. |
| Wall time | `recorded_at` plus elapsed monotonic duration | Library ordering and user-facing recording date, not frame/sample addressing. |
| Future match/segment time | A durable match-session identity plus each segment's match-relative start | Recovery across multiple media files. This does not exist yet and must be designed before active-session restart. |

The native encoder currently uses a two-second GOP/IDR period (120 frames at
60 FPS), no B-frames, and fragmented MP4 at keyframes. Those are already
reasonable replay-oriented baseline choices: no B-frames simplify decode order
and stepping, while the two-second random-access interval matches the least
aggressive candidate in the idea brief. They are not proven optimal.

The current native mux intentionally discards NVENC's input timestamp metadata
at the raw Annex-B pipe and has FFmpeg generate CFR PTS from frame order. That
is acceptable while every committed tick produces exactly one encoded sample:
media time is then the exact tick index, and a replay index can derive keyframe
and sample PTS from the finalized MP4. It is also why Package 5 must not skip
committed timeline ticks.

Do not generate thumbnails, waveform pyramids, or a rich `ReplayIndex` on the
recording hot path in this plan. The safest initial replay-index architecture is
post-finalization or lazy/rebuildable background work derived from canonical
media plus game metadata. Only a small versioned timing/identity manifest may
eventually belong to recorder finalization, and only after the full replay code
and media reader are audited.

## Required implementation order

Each work package is a separate reviewed commit. Run its focused tests and the
static suite before beginning the next package. If a package regresses media,
resource bounds, or preliminary A/B results, stop and revert or redesign that
package rather than compensating elsewhere.

### Package 0: Baseline and observability

Before optimizing, add only the counters and test seams needed to prove the
findings and their fixes.

Native telemetry must expose, at least in fixture output:

- processor-state configuration count;
- WGC arrivals, admitted frames, handoff drops, pending-frame replacements,
  pending high-water mark, and source-snapshot copies;
- scheduled, submitted, completed, and muxed ticks/frames;
- no-slot admission failures, late-deadline count, maximum tick lateness,
  catch-up submissions, and maximum catch-up batch;
- resize/reconfiguration counts and source age at the selected tick;
- process refresh count, cheap target checks, bounds changes, monitor scans,
  and DXGI adapter enumerations;
- startup attempt number, failure disposition, backoff, and cleanup result;
- game-log revision, durable revision, requested/coalesced writes, serialized
  bytes, and serialization/write/sync durations.

The baseline must also write a timing audit for each fixture: first source QPC,
selected-source QPC, output tick index, expected rational media PTS, muxed frame
count, ffprobe stream time base/start/duration, audio start/duration, and game
offset when a poller fixture supplies one. Verify conversions at the boundaries
without adding QPC values to the MP4 as replay timestamps.

Do not expand stable recording metadata or the FFmpeg diagnostics ABI merely
for experimental counters. Prefer native fixture summaries and structured
logs. Promote a field to durable metadata only if Milestone 8 needs it to
validate a production invariant.

Add deterministic, feature-gated stress seams for logical source arrivals at
60, 144, and 240 Hz and GPU-worker stalls of 100, 250, and 500 ms. Live WGC
evidence may only claim the display/compositor rate the PC actually produces;
the higher rates may be proven through the pure admission-policy seam when the
physical display cannot produce them.

Exit criteria:

- a pre-change baseline records all new metrics without changing media output
  or queue capacities;
- fixture output accounts for every admitted/superseded source and every
  scheduled/submitted/completed/muxed tick;
- `media_pts(tick) = tick / 60` remains exact in rational arithmetic and is not
  derived from source-arrival spacing;
- the existing short native lifecycle and media decode still pass.

### Package 1: Hoist invariant D3D11 processor state

Refactor `recorder/src/native/convert.rs` so a single helper configures color
spaces, progressive frame format, auto-processing mode, source rectangle,
destination rectangle, and output target rectangle. Invoke it once after
processor creation and once after each accepted input-size reconfiguration.
The per-frame blit path should construct the input stream and call only
`VideoProcessorBlt` plus required local COM lifetime handling.

Also correct the stale module-level statement in `recorder/src/native/mod.rs`
that says native is not wired into the service.

Exit criteria:

- `processor_state_configurations == processor_recreations + 1`;
- no invariant `VideoProcessorSet*` call remains reachable from the per-frame
  blit function;
- steady, resize, minimize/restore, full decode, frame-change, and exact
  60-FPS duration checks pass;
- CPU/GPU measurements are neutral or better. Correctness, not a noisy tiny
  gain, is the merge gate.

### Package 2: Reduce control-plane polling cost

In `recorder/src/watcher.rs`, refresh only the process information required to
discover the configured executable and PID. Continue to remove dead processes
so appear/replace/disappear transitions remain correct. Do not add a second
process watcher unless profiling proves the minimal all-process scan remains
material.

On Windows, replace the service's separate identity and visibility calls with
one target-state query. Every two-second tick may cheaply check HWND existence,
PID ownership, minimized/visible state, and current client bounds. Cache the
last visible screen rectangle/monitor result in `ActiveRecording`. Enumerate
monitors and DXGI adapters only on initial validation or when visible bounds
change; a minimized window retains its last validated identity. A monitor move
must still be detected on the next service tick and must never silently become
a cross-adapter recording.

Exit criteria:

- one visibility/identity query occurs per active service tick;
- steady visible capture performs zero repeated DXGI factory/adapter scans
  after initial validation;
- resize on the same adapter remains active;
- minimize/restore remains a watchdog pause/resume;
- HWND reuse, PID replacement, close, and cross-adapter movement remain
  terminal for the active candidate and trigger rediscovery/recovery policy;
- process transition tests pass with the minimal refresh specification.

### Package 3: Recover from retryable startup failures

Replace `failed_process: Option<LeagueProcess>` with an explicit per-process
retry state. Introduce a typed boundary result whose disposition is one of:

- cancelled because the process/service changed;
- retryable after complete cleanup;
- terminal for this process/configuration.

Classify errors where they originate; do not infer disposition by matching
`anyhow` strings. Retryable examples include first-frame timeout, temporary
WGC/device readiness, mux spawn/pipe readiness, and a target that must be
rediscovered. Terminal examples include invalid configuration, unsupported or
incompatible runtime/API/codec, diagnostics protocol violation, and internal
resource/ownership invariant failure.

After a retryable failure, fully join/stop the attempted session and poller,
preserve any partial directory, then retry the same live PID on the schedule
2 s, 5 s, 10 s, 30 s, and every 30 s thereafter. Reset attempts when the PID
changes or disappears. Only one startup task or active recording may exist.
Coalesce repeated user-visible errors so the tray/log is informative without
emitting the same alert every 30 seconds.

This package covers failures before `ActiveRecording` is published. It does
not restart an already active recording: doing that safely requires a
backward-compatible match/segment identity, partial-segment library behavior,
and poller re-anchoring that the PC-B subset cannot validate.

Exit criteria:

- paused-time tests prove the exact backoff sequence and reset behavior;
- retryable failures retry only after cleanup and eventually become active;
- terminal failures do not retry;
- shutdown, PID replacement, and disappearance cancel pending backoff/startup;
- no duplicate worker, poller, mux process, or active session is possible;
- every failed directory remains unique and is never overwritten or deleted.

### Package 4: Coalesce WGC sources before the snapshot copy

Move pending-frame ownership into a capture/session abstraction that can
enforce lifetimes without a self-referential borrow. After the first source
anchors the CFR clock, the worker retains at most one pending WGC frame. New
admitted frames replace and promptly close the older pending frame. The worker
drains any already queued handoff frame before a due tick, stages the freshest
pending frame into the persistent BGRA snapshot, closes the WGC frame, records
its source timestamp, and then submits the tick. If no new frame exists, the
existing snapshot supplies the CFR duplicate.

The callback keeps its current nonblocking capacity-one `try_send` behavior.
It must not take a mutex or overwrite a worker-owned frame. Pending replacement
belongs exclusively to the GPU worker. Resize, target close, error, and
shutdown paths must close both pending and handoff frames before WGC teardown.

Telemetry semantics must remain distinct:

- handoff drop: callback could not admit the incoming frame;
- pending replacement/CFR discard: worker accepted a newer frame for the same
  output interval;
- snapshot copy: the selected source was copied for an output tick;
- duplicate: an output tick reused the prior snapshot;
- no-slot failure: output tick could not yet enter the encoder ring.

Exit criteria:

- WGC pool is 2, handoff is 1, pending high-water mark is 1, and no callback
  blocks;
- deterministic 60/144/240-Hz policy tests select the newest source available
  at each 60-Hz tick;
- in steady state, `source_snapshot_copies <= scheduled_ticks + 1` (the extra
  copy is the first-frame anchor), independent of source rate;
- healthy source age is at most one output interval in the deterministic
  policy test;
- accounting distinguishes and reconciles arrivals, callback drops, pending
  replacements, copies, duplicates, conversions, submissions, completions,
  and muxed frames;
- resize/minimize/restore/close and a 240-second bounded run pass with no pool
  starvation, sustained memory growth, or frame freeze.

### Package 5: Make late-tick handling transactional and bounded

Do not implement the review's apparent quick fix of skipping missed output
ticks. The current mux child consumes raw Annex-B video declared as 60-FPS CFR;
it has no packet timestamps or durations with which to represent a gap.
Skipping encoded ticks would shorten video relative to wall-clock audio.

Instead, split the clock operation into a due-tick peek and a commit. Commit a
tick only after conversion has leased a slot and NVENC has accepted the
submission. A temporarily full slot ring leaves the tick due; it does not
silently advance the media timeline. Recover overdue ticks in small bounded
batches (initial maximum: two submissions per scheduler pass), interleaving a
nonblocking source drain, stop/target checks, and evidence publication between
batches. No frame queue is added: overdue work is represented by the clock's
next index and the persistent latest snapshot.

Treat the tick index as the future-proof presentation timestamp authority.
Source QPC attached to the selected image remains separate provenance. If a
later mux accepts explicit packet timestamps, convert the tick index to the
container time base with exact rational arithmetic; do not expose absolute QPC
as MP4 PTS and do not use the truncated per-frame duration repeatedly.

Normal external stop records a wall-clock stop tick and gets a finite final
catch-up/flush deadline. Failure to restore the required frame count within
that deadline preserves a partial recording and reports a timing failure; it
must not publish a deceptively short canonical MP4.

If bounded catch-up cannot meet the performance and duration gates, stop this
package. The alternative is a separately planned timestamp-aware native mux or
packet interface that can encode variable-duration gaps; silent tick skipping
is not an accepted fallback.

Exit criteria under 100/250/500-ms injected worker stalls:

- late count and maximum lateness are truthful and nonzero;
- maximum catch-up batch is at most two;
- source admission continues to obey the 2/1/1 bounds and does not freeze;
- the clock does not commit a tick rejected for lack of a slot;
- submitted, completed, and muxed frame counts reconcile;
- ffprobe packet/sample ordering agrees with the committed tick count and
  nominal 1/60 presentation timeline;
- decoded video remains changing after recovery and fully decodes;
- video duration tracks elapsed recording time and audio duration within the
  existing media tolerance, with no start-time regression;
- recovery does not busy-spin or create sustained CPU/GPU/resource growth.

### Package 6: Profile-gated poller persistence

Use Package 0 telemetry on a representative long synthetic log and later on a
real match. Implement this package only if at least one condition is observed:

- p95 durable JSON write exceeds 10 ms;
- total JSON serialization/write time exceeds 0.5% of recording elapsed time;
- cumulative bytes rewritten exceed 100 times the final log size; or
- writes measurably contend with polling/finalization.

If triggered, replace the two direct writers with one writer task that owns
the durable revision. Mutations increment a revision and notify the writer.
The writer coalesces concurrent event/snapshot revisions over at most 250 ms,
clones only the newest required state, performs one atomic durable rewrite, and
records the durable revision. Keep pretty JSON and `sync_all` initially so the
format and recoverability contract do not change. Stop forces the newest
revision through the existing finalization deadline.

Append-only journaling, compact JSON, relaxed fsync cadence, or a larger
crash-loss window require separate measured justification. Do not add them as
speculative optimizations.

Exit criteria if implemented:

- concurrent event/snapshot updates produce one newest-revision write;
- no acknowledged revision is lost at normal stop;
- crash-loss exposure increases by no more than the 250-ms coalescing bound;
- write failure remains observable and finalization stays finite;
- polling never holds the game-log state lock across serialization or I/O;
- the profile gate improves without regressing poller correctness tests.

### Package 7: Full regression and evidence refresh

After Packages 1-5, and Package 6 only if triggered:

1. Run formatting, all-target/all-feature check, strict Clippy, and the complete
   available non-League test suite while continuing to filter only the absent
   R11 empirical fixture.
2. Run focused clock, conversion, capture, lifecycle, service, watcher,
   poller, and storage tests.
3. Run short real WGC steady, resize, minimize/restore, occlusion, target-close,
   mux-failure, NVENC-failure, and injected-stall scenarios against dedicated
   fixtures, never user recordings.
4. Inspect the produced MP4's stream time base, frame/sample timestamps,
   keyframe cadence, fragment/keyframe structure, audio/video starts, and
   duration. Preserve this as the recording-for-replay baseline; do not tune it
   in the remediation comparison.
5. Run one native 240-second release lifecycle fixture with the pinned r6 mux
   runtime, resource collection, ffprobe, full video/audio decode, changing
   decoded frames, and complete counter reconciliation.
6. Run a matched preliminary A/B in both orders only after the native run and
   static suite pass. Store output in new immutable ignored evidence roots and
   compare against, but never overwrite, the 2026-08-18 baseline.
7. Do not claim League impact, M7 formal completion, an M8 winner, or permission
   to remove a backend from PC-B evidence.

Minimum static commands from the PC-B repository root:

```powershell
cargo fmt --manifest-path recorder/Cargo.toml --all -- --check
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features -- --skip focused_snapshot_parts_remain_parseable_from_empirical_capture
git diff --check
```

The final non-League acceptance report must state per package:

- exact code delta and preserved invariants;
- tests and runtime fixtures actually run;
- before/after CPU, GPU, memory, source-copy, scheduler, and media results;
- any regression, uncertainty, or external gate;
- whether the package was kept, redesigned, reverted, or not triggered.

## Work explicitly outside this addendum

### Runtime recording restart

Restarting after an active backend/watchdog/device failure would create a
second media and poller segment. Before implementing it, the full repository
must define backward-compatible `match_session_id`/segment metadata, browsing
and playback behavior for partial segments, poller re-anchoring, and whether
segments are presented separately or stitched. Until then, preserve the
partial recording and report the failure; do not silently produce unrelated
directories that look like complete matches.

The replay ideas make this gate more important, not less. A future segment
contract should let one replay index address several immutable media segments
on one match-relative timeline. It should not concatenate files or reinterpret
independent zero-based MP4 PTS without explicit segment offsets.

### Recording-for-replay experiments

Create a separate, profile-led feature after the current replay implementation
has baseline open/seek/scrub/export measurements. Start from the existing
two-second GOP and compare, at minimum, two seconds, one second, and 500 ms.
Measure recorder CPU/GPU/latency, bitrate/file size, fragment count/mux cost,
random-seek p50/p95/p99, keyframe-to-target decode distance, frame stepping,
and keyframe-aligned stream-copy clip behavior. Apply the same test matrix to
both remaining recorder backends while both exist.

Changing GOP/IDR cadence, adding explicit timestamped packet muxing, separate
audio tracks, or producing a `ReplayIndex` is accepted only when replay gains
are measured and the recorder's reliability and League-impact gates remain
unchanged. A likely first index is derived after finalization and is versioned,
rebuildable, and bound to a media identity/hash so stale data cannot become
authoritative.

### Automatic retention

`auto_delete_days` needs its own destructive-storage feature and fixtures. It
must specify saved-recording exemptions, active/partial recording behavior,
directory ownership/sentinels, symlink/reparse-point defense, clock handling,
recoverability/trash behavior, and dry-run/audit evidence. No recording or
fixture directory may be deleted as part of performance remediation.

### Broad cleanup and backend removal

Do not refactor the complete unsafe/FFI layer, chase pedantic/nursery warning
counts, change codec quality, grow resource pools, replace the r6 runtime, or
remove either backend. Those changes would destroy attribution or exceed the
review findings.

## Decision log

- 2026-08-18: Treat R2 as an implementation drift from QB-PERF-005's existing
  newest-source-per-tick design, not as a new buffering architecture.
- 2026-08-18: Permit one worker-owned pending WGC frame because the complete
  bound remains the audited two pool surfaces: one pending plus one handoff.
  The callback remains nonblocking and does not replace pending state.
- 2026-08-18: Reject silent late-tick skipping while FFmpeg derives raw-video
  duration from frame count. Use transactional bounded catch-up; require a
  timestamp-aware mux plan before gaps can be represented safely.
- 2026-08-18: Implement retryable startup recovery first. Defer active-session
  restart until the full product can define segment semantics.
- 2026-08-18: Make poller persistence profile-gated because current evidence
  identifies the video path, not Live Client polling, as the performance
  target.
- 2026-08-18: Keep automatic retention out of optimization work because it is
  destructive and needs independent product/safety acceptance criteria.
- 2026-08-18: The replay-engine idea brief changes the plan by making output
  tick index/time base an explicit cross-system presentation-time contract. It
  does not trigger an immediate mux rewrite: exact CFR frame-order timestamps
  are suitable for replay indexing as long as tick commit and encoded sample
  counts remain one-to-one.
- 2026-08-18: Preserve the current two-second GOP/no-B-frame/fragmented-MP4
  output as the replay baseline. Shorter GOPs, timestamp-aware muxing, and index
  generation are separate measured experiments, not speculative remediation.
- 2026-08-18: Preserve terminal cross-adapter behavior when a window straddles
  PC B's differently driven displays and its largest monitor intersection
  changes adapters. Hysteresis or adapter rebinding is a later session-segment
  design, not a control-plane polling optimization.
- 2026-08-18: Package 3 classifies startup failures at the FFmpeg/native
  lifecycle boundaries and retries only dynamic pre-publication failures after
  cleanup. Diagnostics/ownership/timestamp invariants are terminal. Active
  failures remain blocked for the PID because implicit restart still lacks a
  match-segment contract.
- 2026-08-18: Package 4 keeps the first source as tick-zero authority, then
  coalesces admitted WGC frames in one worker-owned pending slot and copies
  only the freshest source at a due tick. Resize-transition surfaces whose
  texture allocation lags `ContentSize` are closed and accounted while the
  last valid snapshot supplies CFR.
- 2026-08-18: Package 5 makes the CFR clock transactional: due-tick inspection
  is read-only and commit follows successful NVENC submission. A full slot ring
  leaves the same tick due, waits one millisecond, and retries without copying
  another source first. Catch-up is capped at two submissions per scheduler
  pass, and normal stop receives a finite five-second final catch-up deadline.
  Live 100/250/500-ms stall arms retained every frame and exact A/V duration.
- 2026-08-18: Package 6's synthetic 50-minute log crossed both the p95 write
  and 100x rewrite-amplification gates. A single writer now coalesces revisions
  for at most 250 ms, clones only the newest state, and force-flushes the final
  acknowledged revision. The modeled event/snapshot workload reduced writes
  and cumulative bytes by about 25% while preserving identical final JSON.
  The residual 151.6x long-log ratio is documented; journaling/format/fsync
  policy changes remain out of scope pending real-match evidence.
- 2026-08-18: Package 7 passed the complete available non-League static suite,
  fresh native lifecycle/failure arms, media audit, 240-second release soak,
  and matched preliminary A/B in both orders. Keep all remediation packages
  and both backends. R11, real League impact, canonical-repository integration,
  segment/rebind design, and M8/M9 remain external gates.

## Progress

- [x] Reproduce and classify the review findings against the current PC-B tree.
- [x] Reconcile them with the audited QB-PERF-005 timing and resource contract.
- [x] Define package order, invariants, decision gates, and verification.
- [x] Package 0: baseline and observability. Evidence:
  `docs/PACKAGE0_RECORDER_OBSERVABILITY_EVIDENCE.md`.
- [x] Package 1: invariant D3D11 state. Evidence:
  `docs/PACKAGE1_D3D11_PROCESSOR_STATE_EVIDENCE.md`.
- [x] Package 2: control-plane polling. Evidence:
  `docs/PACKAGE2_CONTROL_PLANE_EVIDENCE.md`.
- [x] Package 3: retryable startup recovery. Evidence:
  `docs/PACKAGE3_STARTUP_RECOVERY_EVIDENCE.md`.
- [x] Package 4: WGC source coalescing. Evidence:
  `docs/PACKAGE4_WGC_SOURCE_COALESCING_EVIDENCE.md`.
- [x] Package 5: transactional late-tick recovery. Evidence:
  `docs/PACKAGE5_TRANSACTIONAL_TICK_RECOVERY_EVIDENCE.md`.
- [x] Package 6: profile-gated poller persistence (gate triggered). Evidence:
  `docs/PACKAGE6_POLLER_PERSISTENCE_EVIDENCE.md`.
- [x] Package 7: full non-League evidence refresh. Evidence:
  `docs/PACKAGE7_FINAL_NON_LEAGUE_ACCEPTANCE.md`.
- [ ] Reconcile into the canonical full repository and complete external League
  gates before M8/M9.

## Completion

Planning is complete when this addendum is linked from PC-B worktree state and
contains no unresolved implementation decision that can be answered from the
current repository. Product implementation begins in a fresh context.

Remediation is complete only when every implemented package satisfies its exit
criteria, the available non-League suite and media/resource gates pass, the
evidence report is truthful, and the full repository records the remaining
runtime-segmentation, retention, R11, League lifecycle, and M8 gates without
claiming they were completed on PC B.
