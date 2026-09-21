# QB-REPLAY-010 — Measured library and replay opening

## Purpose

Identify the smallest justified change that makes the games library or a selected
replay useful sooner. Execute a bounded attribution pass on the current production
Tauri/WebView path before selecting that change. Historical delay is evidence to
investigate, not permission to build a catalog, split playback payloads, or change
media delivery without a demonstrated cause.

This ExecPlan deliberately authorizes measurement and a next-work disposition.
It does not select a production optimization. Any implementation design must be
finalized and reviewed as a versioned superseding ExecPlan after the attribution
gate below. Preserve this plan and atomically update `execution.plan` plus the
existing checkpoint's exact ExecPlan association/current unit before implementing
the successor. Completing this pass alone does not complete QB-REPLAY-010 or its
production acceptance criteria.

## Relevant planning-time architecture

The design is grounded in `qb-replay-009-wip` at merge `3a750e2`.

- `app/src/App.tsx` starts independent games, clips, storage, settings and Data
  Dragon resources. `reloadLibrary` refetches games/clips/storage after deletion,
  export, retention and output-root changes; saving a game refetches games.
  The current loading condition gates useful library UI on games, clips, storage
  and settings. The benchmark's existing `library_useful` additionally waits for
  HEVC capability and two animation frames. Preserve that event's meaning.
- `app/src-tauri/src/lib.rs` exposes `list_games`, `list_clips`,
  `get_storage_usage` and `get_playback_probe`. These commands call synchronous
  library functions from commands marked `#[tauri::command(async)]`. Existing
  benchmark events report command durations, counts and payload readiness.
- `app/src-tauri/src/library.rs::list_games` enumerates directories and
  `read_game_summary` parses metadata and game logs. `list_clips` repeats
  `list_games` for source enrichment and calls `probe_duration_ms` once per clip
  through synchronous `Command::output`. `storage_usage` calls `list_games`
  again, then scans clips for byte accounting. There is no shared catalog or
  refresh snapshot. Repeated work is a candidate, not a measured scaling result.
- `library.rs::playback_probe` parses the complete strict schema-v2 bundle,
  builds participant/event/snapshot data and returns one `PlaybackProbe`.
  `ViewerScreen.tsx` mounts the primary video after that resource resolves.
  `htmlVideoPlaybackAdapter.ts` configures preload and its `open` method sets the
  source and calls native load. There is no separate minimal playback descriptor
  to instrument yet.
- `playbackController.ts` owns the single primary HTMLVideoElement, bounded
  seeking, distinct requested/dispatched/seeked/presented facts, recovery and
  disposal. `requestVideoFrameCallback` supplies authoritative presentation.
  Neither a `currentTime` assignment nor `seeked` proves displayed frame identity.
- `docs/architecture/replay-time.md` defines the 48,000,000-tick contract, exact
  frame/sample identity and strict schema-v2-only bundles. Metadata, game log and
  timeline must agree on `media_id`; schema-v1 fallback is prohibited. An older
  recording is supported here only when it satisfies that current contract.
- `docs/architecture/local-playback-security.md` owns capability URLs,
  Host/Origin/CSP, opened-handle containment and the 64-connection bound.
  `playback_server.rs::open_file_blocking` offloads admission with the connection
  permit retained by the blocking job, then streams the same validated handle.
  Logical output path and approved root publish together after settings save.
  Existing request/byte/range/resource telemetry is the delivery evidence seam.
- `tools/replay_benchmark/{build_corpus.py,prepare.ps1,matrix.py,run.ps1,analyze.py}`
  already provide generated bundles, sentinel staging, immutable launches,
  production collection and comparisons. Benchmark manifest schema v1 is distinct
  from recording bundle schema v2. Do not conflate them.

## Scope and non-goals

Measure useful library and cold/warm replay opening, including filesystem scans,
JSON parsing, clip probes, IPC/render readiness and post-payload media readiness.
Use existing events first; add only the bounded diagnostic spans needed to resolve
an actual ambiguity. Retain fixture identities and sanitized evidence.

Do not implement shared snapshots, caching, payload staging, a persisted catalog,
ReplayIndex, previews, zoom/LOD, GOP/fragment changes, decoder diagnostics or a
playback backend. Those are possible separately gated consumers of findings, not
the chosen architecture for this pass. No recorder change, capture benchmark,
full QB-REPLAY-008 campaign, new benchmark framework or generic import facility.

QB-REPLAY-015 is optional user-facing previous/next-frame stepping. Exact timing,
frame identity, frame-aligned clips/exports and event/media alignment remain
QB-REPLAY-012 responsibilities. Missing stepping cannot block this work or justify
WebCodecs, libmpv, native decoding, a custom playback backend, or substantial
replacement/re-architecture of HTMLVideoElement/WebView playback.

## Exploration findings

The accepted QB-REPLAY-008 report characterizes a five-game/two-clip library:
useful median/p95 192.4/214.04 ms; clip enumeration median 102.9 ms; games and
storage approximately 10 ms each. This is not large-library evidence.

Cold request-to-first-presented medians for short/representative/long media were
204.2/1681.9/3075.3 ms, while backend payload medians were 1.1/4.3/8.0 ms.
For long media, payload-ready and viewer-mounted medians were 31.9/32.1 ms,
metadata 2963.5 ms and canplay 3070.6 ms. Most measured open delay followed the
payload; the report does not causally distinguish container metadata, loopback
delivery, native readiness and UI work. Moving log parsing cannot be presumed to
remove that gap.

The QB-REPLAY-012 checkpoint explicitly classifies the schema-v1 008 baseline as
incompatible with a current schema-v2 before/after comparison. Its own r6 matrix
is current-only and undersampled for that comparison. Preserve both as historical
characterization; do not backport v2, fabricate a hybrid baseline or reuse an arm
as its own control. Its finalizer measurements concern recorder publication, not
library/replay opening.

QB-REPLAY-009 supplies the controller and controlled hardware-decoder acceptance
for one isolated fixture/device/runtime/load. Runtime status remains Unknown when
load provenance is absent. Its remaining human audible/A/V checks are deferred,
not passed; they are not a missing 010 implementation prerequisite. Do not reopen
its completed engineering or infer decoder selection from opening latency.

QB-REPLAY-011 supplies twenty matched seek/scrub trials and route/security proof.
Its accepted aggregate Job I/O write increase is 9,571,239 bytes (33.12%); process
and destination attribution remain unresolved. Request shape was unchanged,
delivered bytes rose 3.08% and write operations 0.83%, with no disposition-requiring
latency/cancellation/CPU/memory or new sustained-growth regression. This is a
resource watchpoint, not proof of startup disk-write amplification. Later narrow
011 corrections retained those measurements without claiming a fresh campaign.

Provenance, not mandatory recursive execution reads:

- `docs/performance/evidence/qb-replay-008-baseline-20260826-r1/{README.md,report.md,report.json}`.
- `docs/execution/qb-replay-009.md` and
  `docs/performance/evidence/qb-replay-009-controlled-decoder-20260909.md`.
- `docs/performance/evidence/qb-replay-011-20260910/{README.md,io-disposition.json}`.
- `docs/execution/qb-replay-012.md`, especially its comparison deviation.

## Chosen design and rationale

### A. Reuse observations before adding instrumentation

Keep the exact source revision, binary SHA-256, packaged runtime hashes,
OS/WebView/device identity, cache policy and immutable fixture hashes with every
launch. Preserve existing event meanings and use each producer's monotonic clock.
Use existing process-clock calibration where available; do not subtract unrelated
native/frontend clocks or match media requests by timing guesses.

First use existing command spans, payload-ready, mounted, metadata, canplay,
first-authoritative-frame and server records. Read the current diagnostic
`library_useful` together with the actual loading condition. If necessary add a
separately named benchmark-only games-painted/usable observation without changing
the old milestone or claiming that games paint means all optional data is ready.

For an unresolved material span, allow bounded benchmark-only aggregate counters:
directory entries, metadata/log reads and bytes, parse durations, clip-probe
count/duration, command duration, payload byte count, and maximum simultaneous
owned work. Emit one summary per command/open, not per entry/frame. Never add
extra filesystem reads, hashing or probes inside timed spans to measure them.
Native counters compile only with `replay-benchmark`; frontend taps activate only
after a validated benchmark manifest. No normal hot-path tracing or polling.

For post-payload attribution, retain request class/range offset/length,
admission/first-read/completion timing and presentation milestones. Reuse existing
session/request attribution. If correlation is insufficient, isolate one selected
fixture/load per cold process and report the remaining uncertainty; an additional
bounded opaque diagnostic join key is justified only for a specific unresolved
span. Never log bearer URLs, capability tokens, headers or raw local paths.

Any span extraction is a small feature-local analysis helper under
`tools/replay_benchmark/`, tested on synthetic records if committed. Do not expand
manifest/schema or observer infrastructure to produce prettier reports. Reject
missing phases, stale generations and ambiguous joins rather than invent timing.

### B. Bounded fixture and measurement design

Use ignored `build/perf/qb-replay-010/.chronobreak-replay-benchmark` for new input
specs, immutable prepared libraries and results. Never overwrite predecessor
sentinels or use a real user library. Preparation/refinement happens outside timed
windows, single-flight, with an explicit disk bound of 8 GiB for this corpus.
If the bound cannot be met, stop preparation and record the missing cell rather
than linking sources, using sparse fake video or silently truncating fixtures.

Available generated reference: the 240-second `native-current-v2` bundle at
`build/perf/qb-replay-012-webview/.chronobreak-replay-benchmark/library/games/1787904000`
(H.264/AAC, video SHA-256
`131a1a096bed8f3e8042059926a65cccc22733639d2bb63d3a2d705a6d7da5fe`).
Its exact replay end is 11,520,000,000 ticks, or 240 seconds. Copy/validate through
the existing preparer. The neighboring external fixture is historical coverage,
not a retained production recorder to benchmark here.

For 1,800-second representative-duration and 3,600-second long coverage, use the
existing `build_corpus.py` schema-v2 `repeat` transformation from this generated
native source. Label these as derived duration/scaling fixtures, never independent
long recorder or League evidence. Reuse an already valid identity-bound v2 fixture
of the required duration if available at execution. Do not replay the old v1
bundles or weaken time validation to avoid preparation.

Use five targeted cold-open cells, not the Cartesian product of every dimension:

| Cell | Library shape | Selected media | Question |
| --- | --- | --- | --- |
| S | Current small generated native library | 240 s | Current-path reference |
| R | Same small shape | 1,800 s derived | Media-duration effect |
| L | Same small shape | 3,600 s derived | Long post-payload delay |
| N | 50 games, 50 clips; sparse logs | Same 240 s | Enumeration/probe scaling |
| E | Same 50 games/50 clips; dense logs | Same 240 s | Parsing/semantic density |

Keep the same small library contents across S/R/L and vary only the selected
fixture. For N/E keep byte-identical media/clip copies and entry counts. Use
distinct validated IDs; clips use recognized names and valid generated exported
H.264/AAC media. Sparse logs use 30-second event/snapshot intervals; dense logs
use 1-second intervals. Generate through existing schema-v2 helpers, explicitly
record actual counts, and keep metadata/log identities consistent. N/E may use
copies of the small generated fixture for unselected games; no hardlinks or
recording capture is needed. This is synthetic library scaling, not a claim about
a representative real user's library.

Run five new-process `cold_open` trials for each cell. Use a fixed recorded seed,
five-second active warmup/cooldown and existing matrix order rules. For warm
characterization, run one `warm_open` process per cell: one initial mount and five
measured disposal/remount cycles (`iterations: 6`). The initial mount is excluded
from the five measured remount observations; pin this distinction in the manifest
and report. These warm observations are diagnostic characterization, not evidence
that the feature's before/after criterion is satisfied. Reuse startup/library spans
from cold launches instead of adding a redundant idle matrix. These 30 launches
are the attribution ceiling; do not add rate, seek, scrub or export campaigns.

Cold means fresh process, not cold Windows file cache. Preparation/full-decode and
post-run hashing warm filesystem caches. Report this limitation, and do not clear
system caches or modify host policy. Warm cycles are within-process observations,
not five independent process trials. For any later formal before/after warm
comparison, obtain five independent identity-matched process trials per arm for
only the affected cells. Do not inflate this pass for p95/p99 eligibility.

### C. Attribution and stopping rule

Report individual trials, median/min/max, source-stage durations/counts and
resource/error facts. A large duration alone identifies a phase, not its cause.
Use a controlled single-variable observation to distinguish a claimed cause:
clip count versus probe time, fixed-media sparse versus dense logs, or duration
versus requested/delivered ranges and metadata readiness. Do not sum overlapping
async spans into elapsed wall time or label aggregate Job I/O as disk I/O.

Advance to a production design only when the targeted useful/open milestone has
a reproducible issue, the concrete responsible work is attributable, and removing
that work could change the milestone beyond both 5% and the protocol repeatability
band `max(5 ms, 3 * reference MAD)`. This is a decision threshold for this pass,
not a product SLA or an automatic improvement claim. A concrete correctness or
unbounded-resource defect also warrants a focused disposition without that
latency threshold. Freeze the comparison and attribution question before runs.

After the bounded cells, choose one outcome:

1. **Attributed material issue:** record cause, affected cell, supporting spans,
   smallest proposed production unit, precise before/after observable and all
   remaining acceptance gates. Finalize a reviewed versioned superseding ExecPlan
   before changing behavior; atomically relink `execution.plan` and this feature's
   existing checkpoint to it, preserving the evidence plan. Prefer existing
   ownership mechanisms.
2. **No material issue:** record measurements and defer production restructuring.
   Leave 010 non-done; absence of a justified change does not satisfy its snapshot,
   staged-resource or improvement criteria. Any product decision to retire/revise
   those requirements must be explicit, never a silent completion claim. Set the
   checkpoint's active unit to none and its next action to await an explicit
   product disposition or newly scoped evidence; do not automatically restart
   attribution on the next session.
3. **Material delay, cause unresolved:** identify the single missing discriminating
   observation and the decision it would change. Permit one narrower diagnostic
   follow-up; if still unresolved, retain a non-done checkpoint with no active
   unit and a next action naming the missing evidence/external condition. Do not
   escalate to a full benchmark program or speculative architecture.

## Rejected alternatives and planning decisions

- A persisted catalog/database or ReplayIndex is premature; the small baseline
  does not establish scaling need, and QB-REPLAY-013 owns derived indexing.
- A shared snapshot or lazy clip-probe implementation is a plausible bounded
  candidate, not a design selected from current matched attribution.
- Splitting `PlaybackProbe` now would optimize milliseconds of historical payload
  work while leaving the multi-second post-payload span unexplained. Any future
  split must preserve strict validation and media identity before playback.
- Rewriting loopback delivery, adjusting media containers/GOPs or replacing the
  decoder is not warranted by a phase duration or the accepted 011 Job I/O flag.
- Recreating the 008 baseline, backporting schemas, and continuously proving
  decoder provenance would expand this task without resolving its bounded question.

## Milestones

1. **Prepare current-path attribution:** identify/freeze source and binary, stage
   the five cells using existing fixture tools, record counts/identities, inspect
   the known spans and add only necessary benchmark-only aggregate attribution.
   Each launch receipt binds media and full metadata/log hashes, the immutable
   manifest, packaged runtime hashes, source/binary identity and declared seed.
   Target `lib.rs`, `library.rs`, `App.tsx`, existing benchmark event seams and a
   small extraction helper only as needed. Check parsers and preflight before live
   collection; do not change behavior or normal payload contracts.
2. **Collect and explain:** execute the fixed cold/warm cells, validate immutable
   receipts, extract source-stage and resource observations, and decide whether
   one discriminating follow-up is needed. No compilation or heavy verification
   competes with measured playback.
3. **Disposition and handoff:** retain a sanitized report, update the execution
   checkpoint and concise canonical evidence, and apply outcome C above. Remove
   temporary taps or prove normal builds exclude retained benchmark-only code.
   A versioned production design is a separate reviewed handoff; no feature-done
   claim follows from measurement alone.

## Verification design

Before implementation of these measurement units, read
`docs/development/VERIFICATION.md` and apply the repository forward-engineering
skill; apply the Rust skill to any Rust work. Relevant commands from repository root:

```powershell
npm.cmd run desktop:build:benchmark --prefix app
python tools/replay_benchmark/build_corpus.py --spec <absolute-v2-corpus-spec> --media-runtime-root <absolute-packaged-runtime> --preflight-only
python tools/replay_benchmark/build_corpus.py --spec <absolute-v2-corpus-spec> --media-runtime-root <absolute-packaged-runtime>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <absolute-prepare-spec> -Manifest <absolute-prepared-template> -MediaRuntimeRoot <absolute-packaged-runtime>
python tools/replay_benchmark/matrix.py plan --template <absolute-prepared-template> --spec <absolute-matrix-spec>
python tools/replay_benchmark/matrix.py verify --plan <absolute-matrix-plan>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <absolute-launch-manifest> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <absolute-launch-manifest>
python tools/replay_benchmark/matrix.py verify --plan <absolute-matrix-plan> --require-results
python tools/replay_benchmark/analyze.py --input <absolute-run-root>
```

Each placeholder is resolved to an immutable path recorded in the checkpoint before
execution. Run launches in matrix order. Aggregate explicit roots using repeated
`--input` with `--output-dir`; use `--compare <reference-report.json>` only for
identity-compatible comparison, not historical 008 or within-process warm cycles
mislabelled as independent trials.

If code/tooling changes, run the corresponding complete checks:

```powershell
python -m unittest discover -s tools/replay_benchmark/tests -v
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo test --manifest-path app/src-tauri/Cargo.toml --features replay-benchmark
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --features replay-benchmark -- -D warnings
npm.cmd run test --prefix app
npm.cmd run check --prefix app
npm.cmd run build --prefix app
npm.cmd run desktop:build --prefix app
git diff --check
```

Run the canonical validator from `docs/development/VERIFICATION.md` after feature
or checkpoint linkage changes. Verify ordinary builds have no active attribution
taps. Fixture media requires packaged probe/full single-thread decode and preserved
source hashes outside timing. No recorder suites or live League campaign are
necessary for this measurement-only app/tool scope.

Acceptance mapping:

| 010 criterion | Evidence this pass must supply / subsequent gate |
| --- | --- |
| 1: material attribution before production change | Current v2 stage/scale measurements and explicit outcome C |
| 2: coherent snapshot and optional probes | Counts/cost of repeated scans/probes; any implementation remains for the reviewed successor |
| 3: useful-first resources and stale ordering | Exact loading/payload/paint boundary observations; future tests must exercise rapid navigation and stale results |
| 4: bounded blocking work | Inventory/counters of synchronous work; measurement cannot add unbounded jobs or child output |
| 5: filesystem authority and invalidation | Read-only fixture use, strict v2 rejection, source preservation; future mutation/retention/root-change tests remain required |
| 6: improvement without regression | Freeze a compatible current reference; no improvement/non-regression claim until a candidate passes matched evidence |

## Performance and reliability gates

Use the versioned QB-REPLAY-008 comparison rules: at least five matched process
trials per compared arm, p95/p99 only with at least forty observations in the exact
stratum; band `max(resolution floor, 3 * baseline MAD)`, floors 5 ms latency,
0.1 normalized CPU percentage point, 8 MiB memory and 1% bytes/counts. An unfavorable
change above both the band and 5% requires disposition. New errors, timeouts,
recoveries, stale state, source mutation, event loss or sustained growth always
require disposition. Invalid/incomplete trials never become passes.

Any added observer must be bounded and attributable. Reuse accepted instrumentation
without a new observer campaign by default; if added spans materially change
runtime cost or evidence, compare an enabled/disabled pair on the affected cell or
remove the spans. No full-corpus hashing in timed work, no per-frame traces,
unbounded task-per-entry fanout, task cancellation that leaks owned work, or lock
held across await. Existing slow OS calls cannot be declared cancellable merely
because their async waiter is cancelled.

Retain 011's accepted aggregate I/O attribution limitation and 009's deferred human
verification. Missing optional GPU/HEVC capability does not justify another decoder
experiment. Source media, security admission, exact time validation and ordinary
playback behavior remain unchanged throughout this pass.
