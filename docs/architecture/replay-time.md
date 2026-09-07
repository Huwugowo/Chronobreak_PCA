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

`metadata.json` carries `schema_version: 2`, `media_id`, and a `media_timeline`. The timeline records the validated video stream time base, first and one-past-last PTS, exact rate, frame count, replay end, audio coverage, container facts, and recorder cadence/runtime evidence. `game_log.json` carries the same schema version and media ID plus game-tick observations and the optional persisted calibration.

The recorder writes only a private `video.partial.mp4` while recording. On stop, it performs a bounded packaged-ffprobe stream-summary probe (five seconds and one MiB combined output maximum), reconciles the candidate against recorder evidence, constructs v2 metadata, and atomically publishes canonical video and metadata. Uncertain, malformed, timeout, or identity-mismatched candidates remain partial and are never promoted.

## Viewer synchronization

The viewer tracks four distinct facts: requested preview target, dispatched seek, seeked observation, and presented frame. Every dispatched seek has a monotonically increasing epoch within a media generation. Recovery increments the generation and invalidates every older event. A qualifying RVFC frame settles only its current generation/epoch; a stale or off-target callback cannot overwrite presentation state.

Play, pause, rate, fullscreen/layout changes, and frame stepping preserve the same coordinate system. Frame stepping addresses the adjacent validated frame boundary. Backdrop and optional music previews are slaves: they derive a clip-relative target from primary presented video, use their own epochs, and resynchronize under a bounded drift/cooldown policy. They never become clip or replay authority.

## Future segments

Segmentation is intentionally not implemented. A future independently zero-based media segment must carry:

```text
SegmentTimeReference { media_id, match_time_at_replay_zero }
```

Without both immutable media identity and explicit match-relative start, a segment may not be interpreted on a match timeline. Segment ordering, gaps, overlaps, stitching, restart recovery, and ReplayIndex remain separately owned work.
