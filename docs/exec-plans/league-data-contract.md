# league-data-contract — lightweight engineering plan

One goal: implement the accepted [League data contract](../product/league-data-contract.md)
as one continuous effort, with four verifiable milestones. The user approved this
lightweight plan with data-foundation scope and provisional performance targets.
It deliberately does not create child features, rewrite historical plans, or require
the normal feature/checkpoint ceremony. The contract supplies requirements; this
document supplies the implementation boundaries. Inspection date: 2026-10-14.

The user's cutover instruction overrides older telemetry-compatibility requirements:
replace development readers/writers and fixtures together, without migration or
dual formats. Preserve media, saved state, exact replay time and deletion protection.
Unsupported old telemetry can be unavailable without destroying its recording.

## What exists, and what actually remains

| Area | Reuse | Remaining work found in code |
| --- | --- | --- |
| Acquisition | `recorder/src/poller.rs`: focused HTTP routes, cancellation, delayed missed ticks, five-sample fast calibration | GS/AP/PL are joined every 10 s; AP failure suppresses roster snapshots. Separate endpoint schedules, bounded production response bodies, independent timing, missingness and durable coverage are absent. Both ordinary loops stop after three failures. |
| Identity and final facts | `league_client/{association,transport,result,post_game,sidecar}.rs`: exact fresh identity rounds, bounded LCU transport, exact EOG confirmation, deletion-aware atomic sidecars; shared `league-data` stat aliases | Stop AP after identity admission closes; replace name/tag-stripping joins with participant references. Retain participant identifiers, individual early flags and field-level reasons. Pending EOG jobs/attempts/expiry exist only in memory. LOSS remains deliberately disabled. |
| Events and roster | Existing event normalization, roster samples and game ticks | `normalize_event` loses EventID/original source time/Recipient. `reconcile_events` marks IDs seen before validation and cannot retain corrections. Nested item errors become empty inventories; defaulted fields become zero. Most newly required roster fields and observation evidence are discarded. |
| Persistence | One joined writer; atomic small-file helper; generation-pinned context/result writers | `persist_latest_game_log` clones the entire history; `write_json_atomic_with_stats` pretty-serializes, syncs and replaces it. Coalescing reduces write count, not growing rewrite cost. No committed event boundary, crash-tail coverage or restart-safe acquisition state. |
| Queries and consumers | `library.rs`, `LibraryCoordinator`, exact `replay-time` types, existing timeline/export UI | No SQLite dependency or index. Scans, descriptors and export authority currently read the full game log. History KDA/build come from events/last sample, not final facts. Backend and frontend strip Riot tags; replay projection drops `stolen`. Semantic filters, roster-at-time queries and reproducible recipes are incomplete. |

Reuse calibration/identity/retry/deletion tests, ignored 50-minute poller profiles,
generated captures and capture/replay benchmarks. No new tests or benchmarks ran
for this proposal; historical passes do not validate the new contract.

## Implementation shape

### Acquisition and one shared model

Extend `league-data` with shared typed source epochs, participants, observations,
events, coverage and final facts. Preserve zero/empty separately from
absent/error/invalid/conflict; not-applicable requires evidence. Share provenance
and use sparse status metadata rather than repeating strings on every scalar.

One owner schedules each endpoint. GS switches between existing 250 ms calibration
and candidate 1 s normal polling; EV is 1 s; PL is candidate 2 s. AP is identity-only
bootstrap and fresh 10 s association work, ending on binding/admission closure.
Reuse fresh GS evidence without a second GS requester. PL/EV continue independently
when AP, GS calibration or LCU is unavailable. Unmapped observations remain unmapped.
Preserve exact two-round association and the first valid local AP-to-PL match.

Bound bodies, parsing, total concurrency and writer bytes; preserve the 2 s network
deadline. Reserve capacity for gap/failure accounting. Backpressure skips polls
with coverage evidence, never catch-up bursts. Disk failure leaves an unknown tail
after the last commit. Acquisition, parsing and persistence never run on the capture
thread or control video readiness/shutdown.

Keep cumulative EV retrieval until cursor behavior is verified. Commit source ID,
original time, allowlisted payload and ingestion status before advancing progress.
Dedup by source epoch/ID; retain corrections/conflicts and unknown-kind envelopes.
Bound lookup state; exhaustion produces explicit degraded coverage, not silent
eviction, fabricated identity or duplicates.

### Incremental durable source, disposable SQL projection

Proposed starting format: compact append-only `league_observations.jsonl`, plus a
small `game_log.json` timing/format manifest and an occasional atomic
`league_checkpoint.json`. Preserve existing media/time schema semantics; version
the telemetry format independently. Keep `league_match.json` and
`league_result.json` owners, updating their schemas where the contract needs it.

Each batch has sequence bounds and a checksum-bearing commit record; flush before
acknowledging durability. Checkpoints hold committed current context/roster payloads,
epoch, calibration references and recovery position, never accumulated history.
Rebuild event dedup lookup by streaming committed events.

Retain every observation's request interval, run/clock domain, wall receipt,
calibration reference and status. Equal observations reference shared per-participant
payloads; missing/error transitions are changes. Periodic full states bound recovery;
avoid field-by-field patch chains.

Measure a 2 s/256 KiB flush trigger and 60 s checkpoint target first, without
claiming hard disk deadlines. Recovery accepts validated committed prefixes, rejects
interior corruption and reports the tail gap. Process-kill and torn-write fixtures
are separate checks, neither proving arbitrary hardware power-loss durability.

The EOG coordinator owns a small atomic ledger outside SQLite: accepted jobs,
pre-dispatch attempt reservations, original closure/expiry, publication and completion.
Preserve four jobs, 18 attempts and 180 s across restart. Revalidate directory/media
identity; persist no credentials/handles. Uncertain expiry or clock rollback closes
the job. Reconcile already-installed LR idempotently; deletion/failed publication
cannot recreate a bundle. Share one EOG response among eligible exact-ID checks
without resetting budgets.

Restart recovers committed data/EOG work, without video stitching. A new recording
keeps its own `media_id`/clock domain. Unproven source continuity requires a new
epoch and a gap, not reuse of an old event cursor.

### SQLite and product reads

Add one app-side SQLite module: prepared statements, bounded transactions, one
writer, short reads, local cache keyed to approved library root. Recorder acquisition
does not depend on it. Benchmark WAL for concurrent reads/indexing and bound its
checkpoints ([SQLite documentation](https://www.sqlite.org/wal.html)). No ORM or
additional search service is needed.

Use ordinary relational tables around these query families:

- recordings/match association and source revisions;
- participants, source identifiers and observed team memberships;
- final player/team facts, preserving field status and source distinctions;
- events and actor roles;
- observations, shared payloads and coverage intervals;
- stored derivation/recipe provenance, input references and ordered clip ranges.

Key recordings by `media_id` and validated bundle, not globally by League game ID.
Ambiguous participants stay unresolved; position/champion/tag-stripped names are
not identity proof.
Indexes cover recording date/queue/outcome, participant/champion lookup,
event-kind/actor-role predicates, and `(media_id, participant_id, game_tick,
observation_id)` lookups. Preserve integer ticks, numeric ward scores, explicit
missingness, arbitrary teams and source-specific identities.

Import committed sources; transactionally advance rows and consumed revision.
Fingerprints cover media identity, committed prefix, LM/LR, normalizer/rule versions.
Refresh checks manifests/file changes, not every journal byte. Reconcile deletion,
saved state and late EOG; unchanged history should not be reprocessed. Explicit
rebuild verifies content.

Reuse `LibraryCoordinator` epochs, snapshot admission and owned slots; SQL never
authorizes media. Split small strict media/timing reads from optional telemetry.
Prioritize selected-recording indexing, paginate history and window replay data.
Playback never waits for a global rebuild or queries SQL/IPC every frame.

History uses EOG KDA/build when available; recorded fallback stays labelled.
Incomplete final counters remain unknown; zero deaths yields a labelled perfect
KDA rather than a stored infinite ratio. Historical state returns source observation,
age and coverage, with no gap interpolation. Negative queries distinguish no
observed match from unknown coverage.

## Four milestones and their exit checks

| Milestone | Deliverable | Independently verifiable exit |
| --- | --- | --- |
| **1. Acquire the contract** | Shared field/status/identity types; full retained GS/PL/EV/EOG fields; independent scheduler; bounded AP identity work; aggregate measurement hooks. Exercise via a fixture sink before production cutover. | Sanitized payloads and fake time prove every acquisition row, fresh identity rounds, calibration unchanged, nested errors, unknown kinds, corrections, failures/recovery and no overlap/bursts. Benchmark old/new normalization and schedules on the same inputs; establish explicit safety caps and evaluate provisional performance targets against measurements. |
| **2. Persist and recover it** | Journal/checkpoints, payload sharing, durable retrieval boundary and EOG ledger; wire recorder service and replace growing GL writer. Update small core readers and deletion-safe LR validation together. | Round-trip every retained field/status/time; kill between write/flush/checkpoint steps; recover correct prefix without duplicate events or missing successful equal observations. Test disk full, saturated queue, deletion/replacement, repeated restart and original EOG budgets. A generated capture still finalizes with missing/failed data. |
| **3. Query it through SQLite** | Rebuildable indexed projection; history/search, participant/category events and roster state use shared semantics; durable data and relationships needed by bookmarks, clustering, montages and recaps. Remove obsolete telemetry DTOs and regenerate fixtures. | Delete/rebuild the fixture index and obtain identical semantic results. Test late EOG, root A/B/A, stale results, relocation, correction/version invalidation, final-versus-recorded values, ambiguous identities and truthful negative queries. Round-trip stored bookmark/recipe records and their source relationships without feature-specific algorithms. Verify representative SQL plans and warm-query latency; existing playback/export use exact frame ranges. |
| **4. Validate on League and choose defaults** | Installed-client evidence, chosen cadence/cursor settings, measured resource/performance results and current architecture documentation. | Real startup/mid-match/closure, restart, consecutive games, unavailable/delayed EOG and primary-local LOSS checks; observed special fields/modes; controlled League frame/resource comparison. Retain unverified capabilities as unknown/disabled. Do not describe fixture passes as live validation. |

M1-M3 cover the contract's data foundation: P01-P34, P40-P55, P62-P70 and
T01-T13. For P56-P61, persist the data and relationships future features need:
bookmark/recipe records, timestamps and mapping uncertainty, media/event/participant
references, ordered frame ranges, reasons and rule/version provenance. Authored
records have durable ownership outside disposable SQLite; persistence and rebuild
tests must preserve their contents and relationships.

Live bookmark creation/triggers, clustering algorithms, montage/recap generation,
automatic selection and clip-boundary policies are outside this effort. Their
parameters and defaults need not be decided here. Completion establishes the data
foundation for those features, not their algorithms or user-facing behavior.

## Performance measurements and constraints, throughout the work

The old schedule is approximately 1.3 steady Live requests/s; the candidate is
2.5/s excluding calibration/identity/LCU. Faster sampling is therefore a cost to
justify, not an optimization claim. Order of preference: protect League frame
pacing, bound memory and work, reduce CPU/disk traffic, then improve query latency.
Do not trade source correctness for a faster benchmark.

The proposed numeric budgets below are provisional targets to evaluate against
measurements, not hard acceptance requirements. Establish the M1 baseline, then
validate or revise these values using representative workloads and documented
tradeoffs. Bounded work, source correctness and existing project-wide capture/media
gates remain requirements.

- Initially target average acquisition/persistence CPU <=1% of one logical core
  and incremental peak private memory <=32 MiB on a 10-player, two-hour fixture.
  Start by measuring a 4 MiB writer queue; test larger supported rosters separately.
- Journal bytes written grow linearly with retained input; target total written
  bytes at most twice final journal/checkpoint bytes. No history-sized clone,
  rewrite, or unbounded queue in steady state or finalization.
- With 10,000 fixture catalog entries and a representative dense replay corpus,
  target p95 <=50 ms for a warm 50-row history page/semantic filter and <=25 ms
  for a bounded roster/event window. Measure IPC/UI separately. Unchanged refresh
  must not parse all journals; playback must not depend on global rebuild completion.
- Run release-mode 10/60/120-minute equivalent inputs and a 24-hour virtual
  boundedness case. Retain requests/bytes per endpoint, parse/write CPU, p50/p95/p99
  latency, queue high-water, private memory, disk growth, durability lag, rebuild
  throughput and query plans. Short synthetic runs do not establish long-run RSS.
- Compare production-focused candidates at matched freshness; an aggregate
  experiment replaces requests rather than overlapping them. No automatic switch
  to aggregate retrieval. Use a controlled old/new recorder comparison to attribute
  changes, then the existing baseline/capture protocol for total game impact.
- Retain the common capped capture limits: average FPS loss <=2%, 1%-low loss <=5%,
  p95/p99 frametime increase <=5%/8%, additional non-displayed frames <=0.1 percentage
  point, plus existing resource/media gates. Run app closed and a separate bounded
  app/index activity case. SQLite rebuilds yield or pause if they disturb capture.

If a provisional target is missed, profile the cause and use measurements to choose
between implementation tuning and a justified target revision. Record the evidence
and tradeoff; missing a provisional number alone does not fail the milestone.
Do not reduce required coverage, enable an unverified cursor, or misreport results.

## Evidence boundary and completion

**Fixtures can prove:** parser/schema semantics, all missingness states, identity
rejection, timing calculations, request/queue/retry limits, crash-prefix recovery,
EOG ledger idempotency, deletion safety, index rebuild equivalence, bookmark/recipe
record and relationship preservation, query correctness and measured local resource
costs. Use temporary libraries and synthetic/sanitized identities only.

**A running League client is needed for:** installed schema and EV cursor semantics
(inclusivity, stale/future/empty cursors, reset and corrections where observable),
fresh AP/LCU lifecycle behavior, primary-local LOSS encoding, real delayed EOG,
item slots/stat units, FirstBlood/Horde/inhibitor fields, nonzero respawn timers,
mode availability and actual game FPS/frame-time cost. Absence of an observed rare
event proves neither non-support nor not-applicable. Cursor optimization can remain
disabled without blocking the conservative implementation.

Before implementation completion, run shared `league-data`/`replay-time` tests and
the recorder/app/frontend checks in [VERIFICATION.md](../development/VERIFICATION.md),
including recorder check/test/fmt/Clippy, app Rust test/fmt/Clippy, frontend
test/check/build and applicable production builds/generated-media checks. Add the
new fixture/recovery/query tests to those existing suites, not a parallel harness.
Update recorder-lifecycle, media-library and replay-time documentation where changed.
Keep a compact milestone result table here under the requested lightweight workflow
once execution begins. Until live checks run, report **fixture-verified with named
live checks outstanding**, not completion of the agreed data-foundation scope.

First implementation action: establish the shared record/status types and a
sanitized endpoint replay fixture with baseline counters, then replace the joined
snapshot acquisition behind that fixture sink. No additional feature split or
predecessor-plan archaeology is required.

## Milestone results

| Date | Milestone | Result and forward cursor |
| --- | --- | --- |
| 2026-10-14 | M1, partial | Shared allowlisted GS/AP/PL/EV parser and source/field-status/timing records: six fixture tests passed. Independent acquisition scaffold: seven fake-time tests passed for calibration, endpoint isolation, recovery after four failures, AP closure, no overlap/bursts, timeout/cancellation and backpressure. Recorder all-target/all-feature check passed before tests were added. No service cutover yet. |
| 2026-10-15 | M1 fixture work; M2 journal primitive next | Shared tests: 14 passed; scheduler: 14 passed, two measurement cases ignored in the routine run. Retains EOG identifiers/individual flags/aliases, bounded event revisions with post-commit acknowledgement, exact participant and actor references, fresh AP/GS rounds. Review fixes verified: atomic source/calibration admission, identity state committed only on admission, auxiliary retries without false source gaps, frozen-clock calibration invalidation without epoch reset, distinct backward-reset boundary and old in-flight rejection. Logs: `command-20261015-095416.log`, `command-20261015-095427.log` under `.codex/logs`. Production association/service cutover and the event-capacity degraded-coverage path still require writer integration; M1 is not closed. |
| 2026-10-15 | Baseline measurements | Release decoder comparison and 24-hour virtual scheduler passed (two ignored tests explicitly run; `.codex/logs/command-20261015-094037.log`). For 10/60/120-minute equivalents, old/new decode wall time was 3,980/53,820; 21,507/292,251; 45,907/523,722 microseconds. Candidate input bytes were 3,878,100 / 23,268,600 / 46,537,200 versus old 1,038,900 / 6,233,400 / 12,466,800; candidate AP closed. The pre-review 24-hour virtual run made 86,403 GS, 86,400 EV and 43,200 PL requests, maximum three in flight, no failures/skips/unreported gaps, maximum serialized row 12,239 bytes. These characterize fixture decoding/scheduling only; CPU, long-run private memory, durability/write cost and live League frame impact remain unmeasured. Retain candidate cadence provisionally, with no resource-target claim. |
| 2026-10-15 | M2 primitive, partial | Added bounded compact journal framing with chained SHA-256 commit records and flush/sync before acknowledgement. Recovery streams committed batches and reports an incomplete suffix; corruption is rejected. Writer-side EV normalization preserves every poll, retains revisions once, records capacity gaps and rebuilds dedup from validated committed revisions. Whole roster payload sharing preserves missing/error changes and emits a full state every 60 seconds. Shared tests: 11 journal/recovery plus 14 model tests passed (`command-20261015-101019.log`). Review found stale resets could clear event dedup; epoch transition validation and regression tests now pass. Before roster sharing, all recorder targets/features passed (187 library + 3 integration tests; 8 ignored, `command-20261015-100404.log`), recorder check and strict Clippy passed (`command-20261015-100552.log`, `command-20261015-100602.log`). Scheduler-to-journal fixture covers capacity degradation and repeated EV observations. |
| 2026-10-15 | M2 queue/checkpoint, partial | Real-file exclusive creation and process-kill checks pass at entry/commit write, flush and sync boundaries; these do not claim hardware power-loss durability. Bounded 4 MiB queue reserves 64 KiB/eight slots for control/gap records; worker groups at 2 seconds/256 KiB and drains admitted work on stop. Committed event/roster dictionaries use batch overlays, without cloning history. Small checkpoints retain current source/evidence, calibration, local reference and a complete roster dictionary; replay/resume equality passes. Review fixed closure/admission coordination and enforced serialized checkpoint cap. Shared: 17 journal/checkpoint + 14 model tests (`command-20261015-104320.log`); 44 poller tests (`command-20261015-104402.log`). Saturated resets cannot overwrite a pending epoch boundary. |
| 2026-10-15 | M2 media and recording integration, in progress | Media-only descriptor/export reads use strict bounded metadata. Optional telemetry no longer hides saved intent or authorizes deletion: 21 library tests passed (`command-20261015-104503.log`). Telemetry worker now owns a generation-pinned, delete-shared directory guard; journal creation is exclusive and checkpoint/manifest replacement is relative to the pinned handle. Eleven sidecar tests passed (`command-20261015-143416.log`). Production session now uses independent acquisition and journal persistence; old runtime history writer removed, decoder DTOs retained only as a test-only measurement baseline. Collector closes AP admission; service logs telemetry startup failure and continues capture. Recorder all-target/all-feature tests passed: 176 library + 3 integration, six ignored (`command-20261015-143909.log`). App telemetry readers/fixtures and LR manifest validation still need coordinated cutover; no end-to-end completion claim. |
| 2026-10-15 | M2 production telemetry cutover, fixture verified | App reader/fixtures now consume the current journal, preserve full actor identities and event `stolen`/`recipient`, validate the exact manifest anchor, report torn tails/gaps, and tolerate unsupported telemetry without losing healthy media/saved protection. Production AP admission closure and session cancellation regressions pass. Review fixed detached blocking writers: four process-lifetime permits and retained task handles bound abandoned disk work; cancellation stops further batches/hints after any in-flight pinned operation returns. Seven persistence tests passed (`command-20261015-152317.log`); full recorder 181 + 3 tests passed, six ignored (`command-20261015-152906.log`); app 103 tests passed, one ignored (`command-20261015-153009.log`); shared 17 journal + 14 model tests passed (`command-20261015-153101.log`). Recorder/app strict Clippy passed (`152731`, `152806`); frontend 241 tests passed (`152805`), TypeScript check and production build passed (`153102`). Architecture updated to current cutover. Logs are under `.codex/logs`. The abandoned-worker timeout test initially stalled because paused time does not advance with a blocked OS worker; explicit fake-clock advancement corrected the fixture. |
| 2026-10-16 | M2 EOG ledger/shared v2 facts; M3 primitives in progress | Restart ledger preserves original expiry, attempt reservations, directory generation and confirmed results; full control queue fails closed. Production EOG now uses the shared allowlisted decoder, retaining IDs, field issues, stat aliases, individual early flags and request receipts in LR v2. Review fixed nested persisted game IDs, contradictory issue paths, RFC3339 receipts and per-attempt acquisition clock domains. Recorder 55 focused tests passed (`102652`); frontend 18 focused tests and typecheck passed (`102716`). Earlier strict recorder Clippy passed (`095820`), before the latest review fixes. Shared 18 journal/checkpoint + 14 model tests passed (`101756`), including bounded resumable commit pages. App test cutover found an old unsupported-version fixture (`103310`): updated to reject v1; rerun remains. SQLite dependency/schema, bounded importer/query code and separate coordinator slots are in flight and not validated or exposed to consumers yet. All log names use `command-20261016-` under `.codex/logs`. |
| 2026-10-16 | M3 projection and authored foundation, consumers in progress | Added local WAL SQLite with independent bounded writer/read slots, accepted-root/snapshot admission, incremental four-commit import pages, exact source anchors and revisions, scoped participants, event/roster/history queries and nullable final/recorded facts. Durable authored records live outside SQL, with revision CAS and preserved ordered ranges/input references. Full app library suite passed: 116 tests, one ignored (`113000`), resolving the old version fixture. Focused SQL now passes 11 tests (`114250`), including rebuild equivalence, corrections, epoch reuse, stale source/read snapshot, nullable EOG, empty roster and truthful coverage. Six authored tests pass (`114524`), including immediate save/reconcile/query and delete/rebuild preservation. Review corrected stale authored reads and early/missing Events coverage. TypeScript passes with strict query decoders and initial history consumer wiring; history background import, pagination/filter admission tests and selected replay window integration remain in flight. |
| 2026-10-16 | M3 consumers and recovery, fixture verified; release checks in progress | History now uses 50-row keyset pages, semantic filters and nullable final/recorded statistics. Selected replay uses calibration-aware windows, explicit source epochs, exact participant references and a four-page cache; descriptor-first playback/export no longer depends on the retired whole-probe DTO/API. Settings rebuilds a corrupt disposable cache without deleting source or authored data. Lifecycle/root A/B/A/cancellation tests pass (`121104`, `122215`); frontend full suite passes 233 tests (`123158`), production frontend build passes (`152111`). Full app Rust suite passes 121 tests, two ignored (`152452`). Review fixes add unchecked-source state on refresh, resumable cancellation, stale-scope removal, 2 MiB history bounds and materialized coverage counts. Recorder check, 191 library + three integration tests, strict Clippy and fmt pass (`121822`, `121929`, `122016`; fmt rerun); debug/release recorder builds pass (`122811`, `122921`). Shared model/journal and replay-time suites pass (`121640`, `121754`). Query measurement fixtures exposed numeric tick serialization and an invented calibration in synthetic rows; these are being corrected to use exact typed ticks and the production mapping. No measured-query acceptance is claimed yet. All short log references use `command-20261016-` under `.codex/logs`. |
| 2026-10-19 | M3 release query measurements and final checks | Corrected measurement fixtures now use typed ticks and the imported production calibration. Release run passed on 10,000 catalog rows, 2,048 dense projected events and 80 warm samples per query (`command-20261019-092711.log`): p50/p95/p99 milliseconds were history 2.8501/5.1339/5.3108, semantic history 17.9731/27.5066/28.564, replay window 1.9425/2.0308/2.0477, roster 0.2097/0.2406/0.2778. Timings use the complete public APIs and meet provisional local-query targets; the accompanying simplified EXPLAIN statements require replacement with the exact constituent production SQL before claiming plan evidence. These do not measure IPC, UI, rebuild throughput or game impact. Frontend 235 tests pass (`command-20261019-093702.log`), including fixed mock event timing/source identity and stale rebuild navigation. App strict Clippy passes (`command-20261019-093831.log`); shared/app formatting corrected, replay-time format check passes. |
| 2026-10-19 | M2 accelerated persistence measurements and routine checks | Release 10/60/120-minute-equivalent and virtual-day burst profiles pass (`command-20261019-095930.log`), including explicit zero manifest/checkpoint failures. Inputs of 1,500/9,000/18,000/216,000 simple GameStats-shaped rows took 0.145/0.322/0.528/4.882 s; queue high-water was 880,273/2,262,000/2,273,520/2,285,040 bytes within the 4 MiB cap. Final retained bytes were 961,721/5,783,020/11,600,375/140,136,494; successful logical journal/sidecar writes were about 1.002 times retained size. Durability-lag histogram p50/p95/p99 upper bounds were 16.383/32.767/32.767 ms for the shortest profile and 131.071/131.071/131.071 ms for the other three; exact maxima were 31.843/84.719/92.641/104.515 ms. Bounded histogram saturation is explicit; these are per-admitted-packet queue-to-commit lags. Burst input does not exercise the real-time 2 s/60 s flush/checkpoint cadence, representative roster payloads, CPU, private memory or League impact. Recorder 192 unit + three integration tests pass (`095823`), check/strict Clippy pass (`095542`, `095906`); shared 18 journal/recovery + 15 model tests (`095919`) and replay-time 14 tests (`100244`) pass. Short log names use `command-20261019-` under `.codex/logs`. |

The exact production SQL rerun passed (`command-20261019-100639.log`). Shared SQL
constants now drive both production and the constituent EXPLAIN statements, with
the same predicates, joins and bindings. Over the same 10,000-row/2,048-event
fixture and 80 warm samples, p50/p95/p99 milliseconds were history
2.6994/3.3918/3.4606, semantic history 15.7591/19.1546/20.1760, replay
1.2675/1.3487/1.5608 and roster 0.1335/0.1848/0.2123. History scans the recording
date index and uses indexed correlated identity/event/actor lookups; replay uses
the replay-window index with indexed revision/evidence/calibration joins; roster
uses the roster-time covering index and evidence key. These are constituent SQL
plans, not every supporting query executed by the timed public APIs. Full app Rust
tests pass (121, three ignored; `command-20261019-100452.log`) and strict Clippy
passes (`command-20261019-100623.log`). No query behavior or optimization changed.

The production desktop build (`npm run desktop:build --prefix app`) passed on
2026-10-19, including the embedded frontend/type check. The real-journal release
measurement also passed (`command-20261019-094618.log`): 1,157,095 source bytes,
65 commits and 2,048 additional retained events imported in 17 bounded steps.
Initial import/rebuild took 333.810/362.031 ms (3.306/3.048 MiB/s); per-step
p50/p95/p99 were 18.369/28.213/28.213 and 19.189/27.316/27.316 ms. The retained
SQLite/WAL/SHM sizes were 7,012,328/9,389,040 bytes. An unchanged-source accepted
refresh parsed zero journal batches, with p50/p95/p99 0.4458/0.5202/4.3834 ms over
80 warm samples. This is one small real journal on the local fixture volume, not
cold-disk, network-library, IPC, UI or capture-impact evidence. A fractional source
time serialization regression passes (`command-20261019-094525.log`); synthetic
source numbers enter through JSON parsing before fingerprinting, like Live data.

The generated production-service capture passed on 2026-10-19 in sentinel-owned
`build/qb-data-003/capture-fixtures/964989be-b833-4177-b318-af440f77a43d/`.
Release examples built (`command-20261019-094734.log`); `result.json` records
canonical publication with unavailable Live/context data and no delayed-LCU claim.
The service log retains 21 committed records, 9,653 journal bytes, 3,166-byte queue
high-water, zero rejected packets and no writer/checkpoint/manifest failure.
Packaged ffprobe reports 1920x1080 H.264 at 60/1 FPS plus AAC, duration 13.824 s;
full single-thread decode returned zero with no errors. The test requested ten
seconds after the Recording event; capture startup/finalization explain the longer
media duration. Two resident-memory samples around 74 MB are short whole-service
observations, not incremental private-memory or long-run resource acceptance.

The Windows-only ignored
`poller::acquisition::tests::contract_realtime_acquisition_persistence_resource_profile`
now exercises the production scheduler, QueueSink and WriterSession in real time.
`QUEUEBACK_TEST_LEAGUE_SOAK_SECONDS` defaults to 7,200 and accepts 75..=86,400.
Its 75-second release rehearsal passed on 2026-10-19; raw JSON, command receipt and
test output are retained under
`build/qb-data-003/resource-fixtures/a80f43c0-6569-4a99-a29e-bb4573e1497a/`.
The resource window includes writer startup and graceful drain, excluding recovery
readback. Over 75.205 seconds, 75 process samples observed private bytes of
2,002,944 baseline / 3,313,664 peak / 3,149,824 final (+1.25 MiB sampled peak).
User+kernel CPU increased by 15 ms (about 0.020% of one logical core). The fixture
made 77 GS, 74 EV and 38 PL requests, with 4,523 / 40,774 / 441,598 input bytes;
all endpoint failures/skips and unreported gaps/resets/identity rounds were zero.
It durably committed 198 records in 38 batches, recovered exactly those records
with no tail, wrote two checkpoints and rejected no admissions. Queue high-water
was 31,752 bytes and maximum queue-to-commit lag was 2.019015 seconds. Logical
write bytes were 176,800 versus 140,109 final journal/manifest/checkpoint bytes
(1.262 times retained size), with zero writer/sidecar failure. The raw bounded lag
histogram is retained in the report. These are whole-test-process samples using
fake transport, a static ten-player roster and five-event corpus; they exclude
HTTP/TLS/LCU, capture and growing event-history costs. The one-second sampled peak
is not an exact maximum. The two-hour run and larger-roster resource cases are
not run; this rehearsal does not establish the provisional CPU/private-memory
targets or long-run boundedness. Recorder routine tests pass with the fixture
compiled (192 unit + three integration, nine ignored; `100952`), all-target/features
check passes (`101602`), strict Clippy passes (`101701`) and all four Rust component
format checks pass. Short log names use `command-20261019-` under `.codex/logs`.

Final production builds pass on 2026-10-19: recorder debug/release (`101712`,
`101728`) and `npm run desktop:build --prefix app`, including frontend type checking
and Vite production output. The desktop executable was rebuilt at 08:21:02 UTC.
The existing dynamic-import and linker informational warnings do not fail the build.

Focused optimization implementation (2026-10-19, user authorized): QueueSink now
takes owned records and returns ownership on rejected admission. No JSON encoding
or decoding occurs at the queue boundary. Saturating conservative credits include
collection spare capacity, map storage, escaped journal strings and roster
normalization copies; leases remain held through durable commit. Queue limits,
control reserve, admission/close coordination and abandoned-worker ownership stay
bounded. Event normalization consumes cumulative inputs, hashes source fields
directly into SHA-256 without cloning events or allocating JSON buffers, and moves
only retained revisions into the journal batch. Missing IDs, conflicts, corrections,
reversions, exact actors and post-sync acknowledgement keep their existing rules.
The journal serializes borrowed entries into one bounded commit buffer. Decoder
normalization moves validated strings out of the strict duplicate-aware Value
tree; validation, field issues, missingness and numeric representations remain.
No streaming parser or capture/NVENC/mux change was introduced.

Manifest hints now accompany the 60-second checkpoint and graceful drain, after
initial exclusive creation; every journal batch still flushes and syncs before
acknowledgement. The existing exact stale-anchor recovery path consumes subsequent
commits. Checkpoint application validates fallible roster changes before mutation,
stages a roster reader only for affected batches and materializes only the final
roster, instead of cloning the complete checkpoint per commit. Reader validation
no longer clones its map twice. SQLite schema v4 adds disposable per-source/endpoint
coverage totals: compute once at the validated end of a projection and publish
atomically with its ready status/position. Replay and roster reads share those
whole-source values; gaps and current revision actor counts retain requested-range
queries. Partial imports retain their old coverage semantics. No whole-source
aggregate was added to each import batch.

Current optimization verification: shared 18 journal/checkpoint + 15 model tests,
replay-time 14 tests, recorder 193 library + three binary + three fixture tests,
app 121 tests and frontend 235 tests pass. Recovery, failed-sync dedup, event
fingerprint equivalence, correction/reversion, roster rollback, bounded ownership
admission and cached/raw coverage equivalence are exercised. The first app run
and an isolated retry failed because Windows PowerShell blocked the temporary
child script under the default Restricted execution policy. The complete suite
then passed with `PSExecutionPolicyPreference=Bypass` scoped to that test process;
no machine policy or test/product source was changed. Strict shared/recorder/app
Clippy, recorder all-target/all-feature check, all four Rust format checks and
frontend typecheck/production build pass. Recorder debug/release and the complete
Tauri production build also pass. Logs for this pass are retained under
`build/focused-optimization-20261019/`.

Existing release measurements pass against the optimized implementation. With
10,000 catalog recordings, 2,048 representative events and 80 query samples, p95
is 2.790 ms for default history, 15.202 ms for semantic history, 0.671 ms for a
replay window and 0.093 ms for roster lookup (`sql-query-profile.log`). These are
local fixture results, not IPC/UI latency. The ten-player decoder schedule reports
112-118 us per PlayerList decode for the longer runs, versus approximately 34 us
for the previous smaller direct-Serde model (`decoder-profile.log`). Moving
validated strings removes copies, but the strict decoder still does more contract
work; this pass does not establish parity with that older model.

Three 75-second real-time production-writer rehearsals each admit 197 packets,
recover 198 records in 39 commits, and report no rejections, write failures, skipped
requests or unreported gaps. Manifest replacements fall from 39 in the earlier
rehearsal to three (initial, 60-second checkpoint, graceful drain), and manifest
bytes from 22,154 to 1,492-1,502. Logical total writes are 156,311-156,341 bytes
versus 176,800 previously; this measures issued bytes, not physical disk writes.
Maximum commit lag stays below two seconds. Sampled incremental private-memory
peaks are 307,200-749,568 bytes; the queue high-water value of 561,991 is now a
conservative capacity credit and must not be compared with the earlier serialized
byte counter. See `realtime-writer-profile.log`, its `-repeat` counterpart and
`realtime-writer-thread-profile.log`.

Whole-test-process CPU is higher than the earlier single sample: 672-813 ms
(0.89-1.08% of one logical core over 75 seconds), versus 15 ms previously. Thread
snapshots show mostly kernel time on one thread. A small sampler-only check using
the same sysinfo refresh consumes 141-156 ms for 21 refreshes, demonstrating that
the fixture's resource sampling accounts for much of the current CPU reading.
A separate one-off hash comparison verifies identical fingerprints and finds only
a small streamed/buffered difference, insufficient to explain the process delta.
Both checks are ignored local artifacts (`sampler-check.log`, `hash-compare.log`),
not added benchmark infrastructure. The historical CPU result is not a controlled
A/B comparison; no overall CPU or game-interference improvement is established.

Forward cursor (2026-10-19, user-directed closeout): the foundation implementation
and focused optimization are complete and their routine builds, tests and targeted
release measurements pass. Preserve that work in a local commit, then assess the
existing automated response-to-acquisition-to-journal-to-SQL-to-application path.
Fill only meaningful integration gaps using current fixtures and test suites, run
focused checks, record their exact boundary and commit the closeout. No additional
optimization, benchmark framework or architectural redesign is authorized here.

M1-M3 have implemented producers, durable recovery, projection and consumers with
the fixture evidence above; the closeout assessment must establish where those
layers are tested together. M4 installed-client validation has not run. Outstanding
validation includes sustained growing-history/larger-roster resource cases,
two-hour measurements and the controlled League comparison. Earlier generated-
capture, accelerated persistence and query/rebuild fixtures remain supporting
evidence; capture/NVENC/mux code was not changed by the optimization. IPC/UI costs
and physical write amplification remain unmeasured. Report engineering closeout
as fixture-verified with named live checks outstanding; do not turn those checks
into passes or claim M4 completion.

Design decisions: LR installation validates bounded media/context/manifest identity;
it does not certify journal durability or consume historical telemetry. Library
summary checkpoints are explicitly disposable current-state hints; replay validates
the journal and exact anchor. These small reads do not scan growing history.
The ledger preserves original expiry even for confirmed unpublished work. It saves
reservations before HTTP, restores only original directory generations, reconciles
installed LR idempotently and fails closed on a saturated control channel. Fixtures
cover repeated restart, clock rollback, budgets, publication ordering and deletion.
Resource measurements and live checks remain.
No live/performance acceptance or completion of the agreed foundation is claimed.

Existing working-tree state at implementation start: this approved plan was the
only untracked file; tracked sources were clean on `main`. The lightweight contract
is not linked to an older feature entry; its accepted scope overrides the conflicting
draft QB-DATA-004 acquisition/compatibility requirements. No older feature is closed
or relabelled by this implementation.
