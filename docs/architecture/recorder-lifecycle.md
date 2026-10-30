# Recorder lifecycle and synchronization

Process ownership, availability and login registration are described in
[Recorder supervision](recorder-supervision.md).

## Startup and steady state

`recorder/src/main.rs` loads configuration, initializes daily non-blocking logs, and selects diagnostics, headless, or tray mode. Production tray mode runs the UI event loop while the recording service runs on Tokio.

The service resolves the immutable packaged media runtime, performs the bounded owner-only interrupted-publication pass described in [replay-time.md](replay-time.md), and detects audio once. Shutdown is checked between recovery candidates; an admitted probe is joined before shutdown or capture admission. It then checks League process presence every two seconds. Windows capture planning deliberately waits for the real League HWND because its monitor and DXGI adapter are part of the recording contract.

League process presence remains the automatic lifecycle authority:

1. appearance makes the process eligible while its visible capture HWND is discovered;
2. an owned cancellable startup task starts the native recorder inside one 15-second deadline;
3. PID replacement cancels/reaps startup or finalizes the old session before a new generation can start;
4. disappearance or closure of the active game HWND gracefully finalizes an active recording;
5. application shutdown cancels startup and finalizes an active recording before emitting completion.

Live Client readiness and `GameEnd` are not start/stop signals. A process with no ready window is retried on the next watcher tick. Retryable startup failures follow the bounded 2/5/10/30-second schedule only after cleanup, while terminal incompatibility and an unexpected active-backend exit suppress runaway retries for that process generation.

## Capture and finalization

On Windows, the sole production video path is in-process exact-HWND Windows Graphics Capture on an explicit D3D11 adapter, D3D11 resize/BGRA-to-NV12 conversion, and direct NVENC H.264 submission. There is no external capture-backend selector and no automatic GDI, primary-display, software, cross-adapter, codec, or vendor fallback. See [windows-capture.md](windows-capture.md) for the complete supported surface and frame/resource contract.

The service owns one native implementation: a supervised GPU worker, bounded completion thread, four NVENC slots, diagnostics receiver, and one FFmpeg child restricted to audio encoding and fragmented-MP4 muxing. Startup is announced only after a real first WGC frame and advancing encode/mux evidence. Cooperative cleanup performs the blocking worker join off the async executor; child pipes are continuously drained and reaped under finite deadlines.

The service validates the captured HWND/PID/adapter identity while active. A closed HWND is a terminal capture boundary because League can destroy the game window before its process disappears; after a ready recording, that boundary follows the normal flush/finalization path. A replaced HWND or changed process/adapter identity remains a failed/partial outcome. Focus loss, occlusion, and temporary minimize do not change identity; WGC may pause while minimized and resume after restore.

Normal stop ends native capture, drains bounded GPU/encoder/mux work, flushes the muxer, and reaps every owned task/thread/child under finite deadlines. A clean result requires an intentional stop boundary, terminal WGC/encode/mux evidence, consistent frame accounting, successful mux exit, and nonempty output. The common finalizer validates the private candidate before renaming it to canonical `video.mp4`; unsuccessful fragments are preserved and never overwrite it. Completed MP4 fragments are flushed during capture so forced-encoder fixtures remain recoverable.

## Live Client synchronization

`PollerSession` starts only after video readiness. Its monotonic epoch is the first Windows Graphics Capture `SystemRelativeTime`/QPC frame, mapped into the Rust clock, rather than FFmpeg spawn time. Calibration probes `/gamestats` until it observes five strictly advancing, clock-consistent samples and records a midpoint-derived affine schema-v2 game-to-replay calibration.

One acquisition owner schedules independent endpoints:

- `/gamestats` every 250 ms during calibration, then every second;
- cumulative `/eventdata` every second, with cursor optimization disabled;
- `/playerlist` every two seconds, independent of AP and calibration availability;
- identity-only `/activeplayer` bootstrap and fresh ten-second association work,
  ending as soon as collector admission closes or binding completes.

At most one request per endpoint and four total may run. Bodies and parsing are
bounded; requests retain a two-second deadline. Missed cadence skips are accounted
without catch-up bursts. Repeated failures retain gaps and continue retrying; no
three-failure retirement remains. Fresh AP/GS evidence feeds the existing two-round
LCU association without extra Live requests. Equal game clocks invalidate current
calibration; backward clocks require an admitted source-epoch boundary before new
polls. Local participants and event actors use exact identity references. Unknown
events and corrections are retained, with dedup progress acknowledged only after
the journal commits. Unmapped observations remain available as unmapped evidence.
API loss never stops video. Poller cancellation and recorder-backend shutdown begin
together; aggregate diagnostics attribute acquisition and writer activity separately.

## Persistence

Schema-v2 `metadata.json` is canonical media authority, including validated timeline
and immutable `media_id`. Optional telemetry has an independently versioned compact
append-only `league_observations.jsonl`, a small `game_log.json` commit manifest and
an occasional `league_checkpoint.json` current-state hint. Shared `league-data`
owns typed records, sparse field status, event revisions, roster payload sharing,
commit framing and recovery. No runtime accumulated-history writer remains.

The queue transfers owned typed records without a JSON handoff. Conservative
credits bound admitted and in-flight record capacities, normalization scratch and
journal bytes to 4 MiB/128 packets, reserving 64 KiB/eight packets for gap/control
evidence. Credits include spare collection capacity and string escaping; they are
not actual heap or encoded-byte measurements. A blocking worker groups at two
seconds/256 KiB of credits/256 records and syncs before acknowledging commits.
Event fingerprints stream into their digest; unchanged cumulative events need no
payload copies. Checkpoint updates validate fallible roster changes before
updating current context, without cloning the entire checkpoint on each commit.
The initial manifest anchors the journal header. Thereafter the worker replaces
the manifest with the checkpoint every 60 seconds and on normal stop; recovery
validates the stale anchor and may consume later committed batches. These are
triggers, not hard filesystem deadlines. Directory generation checks,
delete-shared handles and relative creation/rename prevent late writes recreating
a deleted or replaced bundle. Failed persistence leaves an unknown tail.

Normal stop closes admission and drains accepted packets. A five-second shutdown
deadline or dropped waiter marks the writer abandoned and transfers its handle to
a process-lifetime supervisor. The worker checks abandonment between operations;
an already running OS operation may finish against its pinned destination. It
does not continue queued batches or publish a new hint after observing cancellation.
Four permits bound all active or abandoned blocking writers; exhausted capacity
disables optional telemetry for a new recording while capture continues. Finished
retired handles are collected on subsequent starts/transfers. Telemetry setup or
shutdown failure never changes video publication validity.

New recordings use only the strict schema-v2 contract; old timing fields, schema-v1 parsing, approximate fallback, and migration are intentionally absent. The optional capture object records backend/ABI, adapter identities, direct interop, GPU stages, finite bounds, runtime ID, first/latest QPC values, final counters, and terminal evidence. Recording bundle IDs are Unix seconds with an optional canonical `-1` through `-999` collision suffix; application browsing, playback, save/delete, clip association, and asset routing all accept that same safe contract.

Live Client poller degradation does not invalidate otherwise healthy video. Encoder/capture failure remains an explicit recorder error while fragmented partial output and the validated committed telemetry prefix remain recoverable. The app does not infer clean completion from diagnostics.

## Match-context ownership

The recording service owns a bounded LCU collector fed by the existing poller's
latest active-player/game-data observation. It adds no Live requests. Two consecutive
fresh coherent rounds with unchanged credentials bind one immutable candidate;
failure clears pending evidence, and binding stops further LCU requests. Admission
closes before network cancellation and owned-worker joins. API availability never
drives capture readiness or shutdown. The standalone diagnostic remains separate.

QB-DATA-003 records only a provisional in-game candidate: exact local Riot ID
agreement between LCU current-summoner and Live active-player, LCU `InProgress`,
positive `gameId`, explicit `queueId`, and compatible map/queue mode. LCU
`teamOne`, `teamTwo`, and `playerChampionSelections` are diagnostic observations,
not binding requirements, because bot and mode responses can be incomplete.

Before canonical media publication, an owned filesystem worker may install one
versioned, at-most-64-KiB `league_match.json`. It retains the allocation directory's
Windows file identity, rejects missing/replaced/reparse directories, and creates
and atomically renames an exclusive temporary relative to the pinned directory
handle without replacing an existing destination. It never recreates directories
or changes metadata. Worker completion precedes publication, including abandonment
of its awaiting future; optional failures preserve healthy media publication.
These checks protect the context destination, not sibling media from deletion.
Blocking discovery/filesystem calls remain owned and joined and may extend shutdown.

QB-DATA-002 owns post-game confirmation through the read-only LCU
`/lol-end-of-game/v1/eog-stats-block` response. It may attach final result and
statistics only when its `gameId` exactly equals the provisional candidate's `gameId`.
Optional completed participant/champion facts can corroborate recorded Live data,
but cannot replace exact game-ID confirmation. A provisional candidate remains
explicitly non-authoritative until that handoff succeeds.

The service snapshots that candidate at capture closure and starts a separate
EOG job alongside poller/video shutdown. One coordinator owns at most four pending
jobs, dropping the oldest on overflow: 18 attempts maximum, at least ten seconds
between attempts, and expiry 180 seconds after the original closure. The bounded
atomic `league_eog_jobs.json` ledger at the library root retains accepted jobs,
pre-dispatch attempt reservations, publication, confirmed facts and terminal state.
Its exclusive process lock prevents two coordinators from spending one budget.
Restart reopens only matching media/directory generations and preserves expiry,
attempts and cadence. Clock rollback or uncertain expiry closes pending work.
No credentials or live handles are persisted. A shared EOG response can satisfy
several exact-ID checks while each job consumes its own reserved attempt. Transport
permits serialize LCU HTTP and joined discovery workers across both collectors.
Cancellation, expiry and deletion remain observable during acquisition. Lifecycle
queue saturation cancels pending work explicitly. Ledger failure suppresses further
requests; optional setup failure leaves recording available.
Aggregate request/byte/latency, pending, result-write and terminal counters emit
when jobs terminate and at joined shutdown. Expiry, deletion and cancellation are
counted separately, including during an owned acquisition. Diagnostics include
the last HTTP status and unavailable/not-ready attempt counts, never identities.

Confirmed facts wait for healthy canonical publication. The late result writer
retains the allocation directory identity and installs one immutable, at-most-
256-KiB `league_result.json` through exclusive creation, flush and handle-relative
no-replace rename. Directory, temporary and read handles permit deletion. Before
rename it rechecks the directory generation, core media UUID/timeline, provisional
game ID and canonical video presence. A valid already-installed result is accepted
idempotently after restart. Failure removes only its own temporary and
cannot change media readiness, saved state or provisional context.

`chronobreak-league-data` owns the shared EOG decoder and schema-v2 result contract.
The recorder skips unlisted fields, retains source identifiers, sparse field issues,
stat aliases, individual early flags, nullable counters and coarse family coverage,
rejects association contradictions, and keeps stats with an unknown outcome when
binary WIN corroboration disagrees. WIN:0 remains unknown until a real local LOSS
payload establishes that encoding; aggregate team.stats.WIN is never outcome proof.
Each deferred EOG HTTP attempt has a fresh acquisition run and monotonic origin;
its RFC3339 wall receipt is retained independently of replay coordinates. Recovered
sidecars revalidate nested game IDs and issue/value consistency. Older development
result schemas are optional unavailable data, without affecting healthy media.

## Performance constraints

Watcher cadence is two seconds. Live acquisition uses the bounded schedules above;
their resource/frame-impact targets remain provisional pending measurements and
live validation. Capture diagnostics are aggregate first/progress/terminal messages,
not per-frame logs. Video remains in a bounded GPU-resident graph, and no full-video
pass occurs at match end.

`QB-PERF-002` applies the common benchmark budget: capped average-FPS loss <=2%, 1%-low loss <=5%, p95 increase <=5%, p99 increase <=8%, additional non-displayed rate <=0.1 percentage point, plus CPU/GPU saturation and memory/output/lifecycle safety gates. Physical validation is per adapter/encoder even though the implementation contract is vendor-neutral.
