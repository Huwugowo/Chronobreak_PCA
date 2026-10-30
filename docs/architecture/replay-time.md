# Canonical replay-time contract

`QB-REPLAY-012` defines schema-v2 replay time. It deliberately has no schema-v1 reader, millisecond fallback, or migration path. A coordinate is never passed as an unlabelled numeric timestamp: persisted exact coordinates are canonical decimal strings, and hot frontend replay positions are checked safe integer ticks.

## Coordinate system

The canonical scale is **48,000,000 replay ticks per second**. A v2 recording is limited to 24 hours, so every valid replay tick is exactly representable in a JavaScript safe integer. `replay-time` owns the Rust types, checked rational conversions, strict decimal grammar, media validation, game calibration, and golden fixtures; `app/src/replayTime.ts` is its TypeScript mirror.

The first validated video presentation boundary is replay tick zero. `media_id` is an opaque UUID allocated for that recording generation. It is required to match in `metadata.json`, `game_log.json`, playback payloads, clip ranges, and export requests. It is generation identity, not a content hash.

| Domain | Authority / zero | Unit and range | Rules and allowed conversion |
| --- | --- | --- | --- |
| Capture source | WGC `SystemRelativeTime` QPC; first accepted source observation | signed 100 ns provenance value | Monotonic while capture is healthy. It is never a replay coordinate; it can only feed a recorded capture anchor/calibration. |
| Producer cadence | Native CFR clock or external `fps` filter; first selected output frame | exact rational frame boundary | Frame count is monotonic. It is producer evidence only and must match finalized media. |
| Video media | validated MP4 video PTS; first validated video PTS is replay zero | raw signed PTS plus 48 MHz replay ticks; `[0, 24 h]` replay coverage | PTS-to-replay conversion is exact rational arithmetic. A recording must have one video stream, exact CFR, matching cadence/count, and contiguous declared coverage. |
| Audio media | validated audio PTS relative to video media origin | raw signed PTS plus signed replay ticks | Audio has independent start/end coverage and cannot lengthen video replay coverage. Its mapping is exact; audio outside video coverage is capped/ignored for replay and clip authority. |
| League game | Live Client game clock, rounded once at ingestion | signed integer microseconds (`GameTick`) | Monotonic observations calibrate through one persisted midpoint-derived affine mapping. Conversion yields unavailable, before-media, inside-media, or after-media; no saturating fallback. |
| Wall clock | recorder `recorded_at` | RFC3339 wall time | Provenance/UI only; never a playback, frame, or clip coordinate. |
| Browser request | viewer intent and dispatched media-element seek | replay tick plus generation/epoch | A request is not evidence of seek or presentation. Browser seconds are converted at the media PTS origin only. |
| Browser seeked | `seeked` / `currentTime` observation | replay tick plus generation/epoch | A qualifying observed target is a seeked observation only. It may support recovery diagnostics, never presented-frame authority. |
| Browser presentation | `requestVideoFrameCallback().mediaTime` for primary video | replay tick plus generation/epoch | The only presented-video authority when RVFC is available. A `currentTime` fallback is explicitly media-clock-approximate and cannot settle a frame seek. |
| Clip/export | validated video frame grid | half-open `[start_frame, end_frame_exclusive)` with `media_id` | Start projection floors to the containing frame, end projection ceils to the first excluded frame. Empty/out-of-grid/identity-mismatched ranges are rejected. |
| Benchmark observer | benchmark process monotonic clock | observer milliseconds | Measurements only; it cannot enter product payloads or replay conversion. |

All rational multiplication/division is checked for overflow. A conversion must request `exact`, `floor`, `ceil`, or nearest-ties-to-even rounding; it must not inherit host floating-point behavior. Exact conversions reject non-integral results.

## Persisted recording contract

`metadata.json` carries `schema_version: 2`, `media_id`, and a `media_timeline`. The timeline records the validated video stream time base, first and one-past-last PTS, exact rate, frame count, replay end, audio coverage, container facts, and recorder cadence/runtime evidence. `game_log.json` carries the same media identity and timing schema version, plus an independent telemetry format version, committed journal anchor and current calibration. Game-tick observations and their captured calibration references live in `league_observations.jsonl`. Observations without a usable referenced calibration remain unmapped. Equal game-clock samples invalidate current calibration; a backward clock starts a new source epoch and boundary. Older observations retain their own mapping evidence.

Media descriptors and export read strict bounded metadata independently of optional
telemetry. Missing/unsupported telemetry cannot invalidate an otherwise valid video
timeline or its exact frame ranges.

The optional League SQL projection stores each event/roster replay tick using that
observation's referenced calibration. Window queries compare these projected
coordinates directly; they do not invert the current calibration for older source
epochs. A changed media timeline invalidates the projection. Returned roster samples
include their source time, mapped replay time and age, without interpolation.
Replay windows also carry source-wide participant identities for champion filters
and event colors before the first mapped roster sample. Riot IDs, champion names
and teams are each included only when consistent within the same media/run/epoch;
missing or conflicting descriptors stay null. This context supplies no timed
roster, scores or items; a missing roster at the playhead remains missing.
Champion filter positions and ally/enemy grouping use that consistent identity
context, sorted by participant ID, throughout the selected source. Timed roster
samples may reorder or omit players without moving the filter portraits.
The compact replay overlay owns a separate paged, source-scoped recorded timeline,
independent of event-window lifetime. Binary lookup displays the latest complete
sample at/before the video, including local KDA, CS, level and sampled game clock.
Its lookup follows observed video position (RVFC, or the
explicit media-clock approximation), never an unpresented seek request. A sample
mapped after that position is hidden on backward seek. Game clock is formatted
directly from that sample's signed game tick, without extrapolation or correction
of Practice Tool clock jumps. Missing fields remain unavailable.
The fixed champion rails join spell slots, level and death state from the same
available sample by source-scoped participant identity. Source identities own
fixed rail slots; known spell asset variants mount/decode once in those slots,
and only their visibility follows selected spell labels. After complete timeline
loading, a distinct first-recorded loadout supplies only the first mapped sample's
two nullable spell labels during the opening segment before that sample (or before
the first video observation). Tooltips identify this context as first recorded.
It supplies no level, death, timer, clock or scores. Normal temporal samples fully
replace its slots, including nulls; later unavailable samples, pending-page
frontiers and errors cannot reuse it. Source changes clear the context. Loading
an asset alone never selects it. A dead portrait is grey;
its countdown subtracts observed replay elapsed time from the sampled respawn
timer and rounds up to whole seconds, with zero as the minimum. Playback pause,
speed and seeks therefore affect the countdown through the primary video, with
no wall-clock interval. New roster samples override the display; reaching zero
alone never marks a champion alive. Missing death/timer fields do not imply a
death or a respawn, and a future roster is hidden during backward seeks. This
display does not reconstruct or correct an accelerated/discontinuous game clock.

The recorder writes only a private `video.partial.mp4` while recording. On stop, it performs a bounded packaged-ffprobe stream-summary probe (five seconds and one MiB combined output maximum), reconciles the candidate against recorder evidence, and constructs v2 metadata. Windows publication syncs `metadata.pending.json`, then uses two separate handle-relative no-replace renames for video and metadata; the pair is not one atomic transaction. The additive `publication` receipt binds the media UUID to directory/media file IDs, volume, length and last-write time. Uncertain, malformed, timeout, or identity-mismatched candidates are never promoted.

Only after orderly recorder teardown, while exclusive ownership is retained, one maintenance pass may examine up to 512 direct/discovered entries and 512 prefix searches, admitting at most four probes within ten seconds. These are admission/work-count bounds, not hard filesystem completion deadlines. Fresh League checks gate the pass, discovery regions, candidate pinning and probes; a completed entry only needs a cheap canonical-metadata existence check. No recovery runs at startup or in the running watcher loop. The recognized recorder-private `.qb-cap-003.cursor`, bound to the games directory identity, retains one fixed-size advisory search prefix across exits. Budget-limited regions split into grammatical name prefixes; exhausted/empty regions advance, so a finite stable library is eventually covered across eligible exits without a growing skipped scan or depending on enumeration order. Exact-prefix bundle names are handled before descendants. New entries behind the cursor wait for wrap. Unknown, malformed or copied cursor bytes are preserved; bounded root-pass recovery remains possible, but persistent progress is then unavailable. Unchanged hints are not rewritten.

A receipt-bound pending metadata/private-or-canonical-video pair must pass the same strict validator with exact persisted timeline equality before publication completes in place. Canonical metadata is never replaced. Bare fragments, old receipt-less pairs, conflicting artifacts and invalid/inaccessible output retain their bytes with a structured disposition and reason in recorder logs; ordinary library refresh remains read-only. No decoding, media hashing, remux, GPU work or periodic recovery scan runs in production. File generation receipts detect ordinary replacement/modification, not hostile edits that restore identity facts. Continuous League presence or no orderly exits can defer maintenance indefinitely. Already admitted probes and blocking OS operations remain owned and can extend exit; real League performance and interactive playback evidence remain pending.

The recorder extends the existing ffprobe summary with actual video dimensions and requires 1920x1080 H.264 High before publication. These are encoded stream-header facts, not values copied from recording configuration. The generic replay-time summary parser and persisted timeline schema remain unchanged.

Every native candidate also passes an independent empty-moov fragmented-MP4 timing check, including when ffprobe reports `nb_frames`. The reader resolves `mdhd` timescale, `trex`/`tfhd` defaults, `tfdt` origins and `trun` sample durations. Native output has no B frames: composition offsets must be absent/zero, each duration must equal one exact frame step, and decode/presentation coverage must be contiguous from zero. Only an absent edit list or the muxer's single open-ended, zero-offset, rate-one identity edit is accepted. Selected video sample sizes/data offsets must fit the associated complete `mdat`. Missing, malformed, unsupported or ambiguous evidence prevents publication. Independent count, time base and end PTS must agree with ffprobe and the existing strict producer/timeline reconciliation.

The reader shares the five-second probe deadline, retains the million-box-visit limit and 24-hour video bound, reads sample tables in 4-KiB chunks, and seeks past encoded payload. Work is O(box visits + explicit sample-table entries), with O(1) memory; inherited constant-duration runs are checked algebraically. For the observed native layout, sample sizes cost four table bytes per frame (about 0.864 MB/hour at 60 FPS), plus fragment/header reads. Payload size alone does not increase scan bytes; a synthetic 8-GiB `mdat` is tested without reading its contents. The blocking reader remains owned until joined; OS I/O can outlast a cooperative deadline. No decoding, extra process, GPU work or background finalization is added. Existing inline finalization scheduling is retained by explicit product direction. This proves the container presentation grid and observed stream format, not decoded-frame correctness, frozen-picture absence, later in-band parameter changes, or full payload integrity.

## Viewer synchronization

The viewer tracks four distinct facts: requested preview target, dispatched seek, seeked observation, and presented frame. Every dispatched seek has a monotonically increasing epoch within a media generation. Recovery increments the generation and invalidates every older event. A qualifying RVFC frame settles only its current generation/epoch; a stale or off-target callback cannot overwrite presentation state.

Ordinary seek queue updates retain the last accepted frame and its observed
overlay state until a new accepted frame arrives. In particular, a later
`seeked` event cannot erase an RVFC that arrived first. Repeated requests for
the same in-flight frame share its seek and presentation result; an idle seek
can skip native seeking only when the matching frame is already presented
(or the browser explicitly uses the media-clock approximation).

Play, pause, rate, and fullscreen/layout changes preserve the same coordinate system. QB-REPLAY-012 owns exact replay timing, frame identity, frame-aligned clip/export boundaries, and event/media alignment independently of user stepping controls. If optional previous/next-frame stepping (QB-REPLAY-015) is implemented, it consumes the adjacent validated frame boundary and existing presented-frame evidence. Its absence does not block replay work; inability to implement it cleanly within the existing HTMLVideoElement/WebView stack does not authorize WebCodecs, libmpv, native decoding, a custom playback backend, or replacement/substantial re-architecture of playback. Backdrop and optional music previews are slaves: they derive a clip-relative target from primary presented video, use their own epochs, and resynchronize under a bounded drift/cooldown policy. They never become clip or replay authority.

## Future segments

Segmentation is intentionally not implemented. A future independently zero-based media segment must carry:

```text
SegmentTimeReference { media_id, match_time_at_replay_zero }
```

Without both immutable media identity and explicit match-relative start, a segment may not be interpreted on a match timeline. Segment ordering, gaps, overlaps, stitching, restart recovery, and ReplayIndex remain separately owned work.
