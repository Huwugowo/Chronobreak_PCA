# QB-REPLAY-012: Canonical Replay Time Contract

This immutable ExecPlan records the planning-time design approved for `QB-REPLAY-012`. The feature entry owns the Definition of Done. This plan summarizes the predecessor baseline and protocol facts required below; its references are provenance to consult only for a concrete unresolved question.

## Purpose

Chronobreak currently carries several values called “time” that do not mean the same thing. The recorder sees Windows capture/QPC time, an internal exact CFR frame counter, FFmpeg-generated MP4 presentation timestamps, Live Client game time, and wall time. The app reduces most of those facts to integer milliseconds. The viewer can temporarily treat a requested seek target as if it were the frame actually presented, while clipping rounds decimal seconds through a configured FPS rather than validated media facts.

This feature establishes one exact, versioned replay coordinate system and typed conversions at every boundary. A recording produced after the change must say exactly which media generation it describes, where replay zero is, which video frame boundaries exist, how audio and game observations map to them, and what the browser has requested, seeked to, and actually presented. Clip/export ranges must be exact frame ranges over that same media identity.

The user-visible result is trustworthy navigation and clipping: an event jump, frame step, clip boundary, audio preview, and exported interval all refer to the same part of the recording. The architecture also becomes safe for later ReplayIndex, separate-audio, and multi-segment work without implementing those features here.

This is a deliberate development-time breaking change. Chronobreak has no production recording population to migrate. Schema version 2 is the only supported recording contract after implementation; all repository-owned fixtures and generators are rebuilt. There is no old-schema parser, compatibility mode, approximate fallback, or migration utility.

## Relevant planning-time architecture

- `recorder/src/native/clock.rs` owns `NativeCfrClock`. It anchors the first accepted capture frame and commits an exact rational output tick only after the encoder accepts a frame. Source drops and CFR duplicates remain separate diagnostics.
- Windows Graphics Capture exposes `SystemRelativeTime` in 100 ns QPC units. The native source maps the first valid QPC observation to a Rust `Instant`; the recorder also stores wall-clock `recorded_at`. QPC and wall time are capture provenance, not replay coordinates.
- The native encoder emits raw Annex-B H.264 without persistent timestamps. `recorder/src/native/mux.rs` asks FFmpeg to generate MP4 timestamps from the selected CFR rate. The saved MP4, not the transient encoder timestamp, is the playback authority.
- The external backend in `recorder/src/encoder.rs` uses `gfxcapture`, `scale_d3d11`, `-fps_mode cfr`, and `-r`. Fresh probe evidence shows that this does not currently guarantee an exact first-frame boundary even though later frame deltas are regular.
- Both backends publish `video.mp4` before `recorder/src/service.rs` writes final metadata. Child exit, nonempty output, and limited backend evidence gate publication, but there is no common strict media-timeline finalizer.
- `recorder/src/poller.rs` samples Live Client data. Its current calibration uses receive time rather than request midpoint, rounds floating-point seconds to milliseconds, and stores derived `video_time_ms` with saturating arithmetic. Events and snapshots do not follow one mapping path.
- `metadata.json` currently stores wall-derived `duration_ms`, `recording_fps`, and `video_offset_ms`. `game_log.json` stores game timestamps plus some precomputed video milliseconds. Neither file has the strict schema/identity/time contract required here.
- `app/src-tauri/src/library.rs` deserializes permissive millisecond fields, selects one of several offset sources, saturates snapshot math, and can discard events that map before replay zero. `PlaybackProbe` and frontend types expose bare JavaScript numbers whose timestamp domain is implicit.
- `app/src/components/ViewerScreen.tsx` has one optimistic `videoTimeMs`. A request immediately updates it, then `seeked`, `currentTime`, or `requestVideoFrameCallback` can overwrite it. The existing recovery generation does not distinguish two superseding seeks within the same loaded media generation.
- `requestVideoFrameCallback().mediaTime` is the relevant browser observation for a presented video frame. It is not an audio clock, and callbacks can be delayed or skipped by display scheduling. `HTMLMediaElement.currentTime` remains a useful browser-timeline observation but is not proof that a requested frame was presented.
- `app/src/viewerUtils.ts` rounds frame conversions through floating-point milliseconds. `app/src-tauri/src/clip_export.rs` accepts millisecond endpoints, formats decimal FFmpeg seeks to millisecond precision, and trusts configured FPS rather than a validated frame grid.
- The clip editor uses one primary video plus a backdrop video and optional music preview. The latter clocks are currently reconciled with ad hoc drift thresholds; only the primary presented video may be replay authority.
- `QB-REPLAY-008` supplies the production-WebView benchmark and its before/after comparison rule. `QB-REPLAY-009` owns the larger controller extraction and playback-rate UI. `QB-REPLAY-013` owns an actual ReplayIndex. `QB-AUDIO-001` owns capturing and controlling two audio groups.

## Scope and non-goals

In scope:

- a small pure Rust crate shared by the recorder and Tauri app for exact timeline values, rational arithmetic, validation, schema-v2 time structures, and bounded ffprobe fact parsing;
- a TypeScript mirror that consumes the same versioned golden fixtures and preserves exact wire values at the frontend boundary;
- strict schema-v2 `metadata.json` and `game_log.json`, one immutable media identity, exact video/audio facts, and one persisted game-to-replay calibration;
- a common post-recording finalizer that validates the saved MP4 before canonical publication and refuses unproven timeline claims;
- exact CFR normalization and validation for both current recorder backends without changing the media container, codec/profile policy, resolution, target FPS, or native timestamp-aware muxing architecture;
- typed Tauri/frontend payloads, exact event/snapshot mapping, distinct viewer request/dispatch/seeked/presented states, and bounded synchronization of secondary preview media;
- frame-addressed clip ranges and exports derived from validated media facts;
- deterministic native, external, nonzero-start, malformed, rational-rate, seek-state, event, A/V marker, and export fixtures;
- durable architecture documentation and before/after reliability/performance evidence.

Out of scope:

- supporting, parsing, migrating, repairing, or approximately playing schema-v1 development bundles;
- recording segmentation, restart/stitching, timestamp-aware in-process MP4 muxing, media-format changes, or recovery of interrupted partial media;
- implementing ReplayIndex, thumbnails, preview manifests, keyframe indexes, or a new database;
- implementing the `QB-REPLAY-009` controller extraction, new playback-rate controls, hardware-decoder selection, or a new playback backend;
- implementing separate captured audio stems or arbitrary source isolation;
- stream-copy, hybrid, or smart-cut export (`QB-CLIP-004` owns those strategies);
- inferring exactness from an encoder/backend name, configured FPS, container duration, or equal nominal `currentTime` assignments;
- claiming that encoded A/V timestamps measure physical speaker, headset, or device latency.

## Exploration findings

### Current time-domain map

| Domain | Current authority and unit | Required role after this feature |
| --- | --- | --- |
| Capture source | WGC `SystemRelativeTime`, QPC-derived 100 ns values | Capture provenance and poller calibration input only; never serialized as replay position without an explicit mapping |
| Native CFR schedule | `NativeCfrClock` rational frame tick | Producer expectation checked against finalized media |
| MP4 video | FFmpeg-generated presentation timestamps and stream time base | Physical playback authority, normalized to replay ticks and exact frame boundaries |
| MP4 audio | AAC packet/sample presentation timestamps | A stream mapped into the video-owned replay timeline, with independently recorded coverage |
| League game | Live Client floating-point seconds | Rounded once to signed integer microseconds and mapped by a persisted affine calibration |
| Wall | `recorded_at` plus elapsed stop time | UI/provenance only; never frame or clip authority |
| Browser request | user preview and dispatched `currentTime` assignment | Intent/state, never presented-position authority |
| Browser observation | `seeked`, `currentTime`, and RVFC `mediaTime` | Separate observations; RVFC is authoritative for presented video when available |
| Clip/export | millisecond endpoints plus configured FPS | Half-open exact source-frame range bound to `media_id` |
| Benchmark observer | process monotonic milliseconds | Observer latency/resource measurement only; never product replay time |

The domains cannot safely share one primitive alias. Conversions must name their source and destination and state whether they require exact divisibility or use a documented rounding direction.

### Dedicated media evidence

Read-only inspection used the packaged QueueBack ffprobe and only ignored, sentinel-owned replay/capture fixtures:

- A fresh five-second native fixture contains exactly 300 video frames at 60/1, a `1/15360` video time base, a 256-tick presentation step, zero video/audio start, and exactly five seconds of video and audio coverage.
- A fresh external-backend fixture contains 620 video packets. It starts at zero, but its first packet duration is 584 `1/15360` ticks before later packets settle to 256 ticks. Video coverage is 10.354688 seconds while audio extends to 10.773333 seconds, a 418.645 ms tail. `-fps_mode cfr -r` is therefore insufficient as the exact frame-grid contract.
- The accepted replay corpus has the same backend distinction. Its native file is on a regular 60 FPS grid and differs from audio by about 0.312 ms; its external file has about 367.979 ms of audio beyond video.
- A stream-summary ffprobe of the existing 387,517,179-byte one-hour native fixture completed in about 540 ms and returned about 1.1 KiB. This supports a bounded stream-summary finalization probe, not a per-packet scan on the normal recording path.
- On the native fixture, FFmpeg accurate input seek at exactly 1.000000 seconds decoded the same frame as source presentation frame 60 in a `framemd5` comparison. That is useful evidence for the export design, but output range and A/V validation remain required before an exactness claim.

Offline fixture verification may scan packets/frames. Production finalization must remain bounded and must not perform work proportional to every packet or media byte beyond the media tool’s bounded header/index inspection.

### Primary external specifications

- FFmpeg documents `-fps_mode cfr` as duplicating or dropping frames to match a requested rate, while muxing can still modify timestamps. The `fps` filter explicitly creates a constant-rate frame stream, and `start_time=0` pads or trims the initial boundary. See <https://ffmpeg.org/ffmpeg.html> and <https://ffmpeg.org/ffmpeg-filters.html>.
- The RVFC specification defines `mediaTime` as the presentation timestamp of the frame submitted for composition on the media element timeline; callbacks may be late and display refresh can prevent one callback per media frame. See <https://wicg.github.io/video-rvfc/>.
- The HTML media timeline uses seconds and need not begin at zero; the earliest playable position is related to the seekable range. Browser adaptation must therefore include the validated media origin rather than assuming `currentTime === replayTime`. See <https://html.spec.whatwg.org/multipage/media.html>.

## Chosen design

### 1. One versioned exact coordinate system

Add `replay-time/` with package name `chronobreak-replay-time`. It is a pure Rust library used by `recorder` and `app/src-tauri`; it has no Tauri, browser, Tokio, capture, or recorder lifecycle dependency. It owns exact rational types, checked conversions, schema-v2 time structures, strict decimal parsing/serialization, and bounded parsing of the small ffprobe JSON shape needed by the finalizer. Process launching and filesystem publication remain in their owning binaries.

Add `app/src/replayTime.ts` as the TypeScript boundary mirror. Rust and TypeScript consume one committed fixture file under `fixtures/replay-time/v2/`. Neither implementation is generated from the other; the shared fixtures and JSON Schema catch divergence.

Use a fixed schema-v2 replay scale:

```text
REPLAY_TICKS_PER_SECOND = 48,000,000
MAX_REPLAY_DURATION = 24 hours
```

This scale exactly represents:

- 30/1 and 60/1 frame durations;
- 30000/1001 frame durations;
- 48,000 Hz audio samples;
- the current `1/15360` MP4 video tick;
- integer milliseconds and integer microseconds.

Twenty-four hours is 4,147,200,000,000 replay ticks, below JavaScript’s `Number.MAX_SAFE_INTEGER`. Wire-format tick/frame/sample/PTS integers are nevertheless canonical decimal strings so a future schema cannot silently inherit a JavaScript precision limit. Unsigned strings accept only `0` or `[1-9][0-9]*`. Signed affine offsets accept `0` or `-?[1-9][0-9]*`; `-0`, plus signs, whitespace, exponents, decimal points, and leading zeroes are invalid. Parsers apply length and value limits before conversion.

Rust uses checked `u64`/`i64` values with `u128`/`i128` intermediates. TypeScript validates with `BigInt`, applies the 24-hour/safe-integer bound, and then uses branded integer `number` values in hot UI state. `BigInt` is never sent directly to DOM media APIs or passed to ordinary JSON serialization.

Core types are distinct even when their storage representation matches:

- `ReplayTick` and `SignedReplayTick`;
- `FrameIndex` and `FrameBoundary`;
- `AudioSampleIndex`;
- `RawMediaPts { value, time_base }`;
- `GameTick` at 1,000,000 ticks per second;
- `BrowserMediaSeconds` only at the DOM adapter;
- `ObserverMonotonicMs` only in benchmark code;
- `MediaId` as an opaque generation identity.

`Rational { numerator, denominator }` is normalized, has a positive nonzero denominator, and rejects overflow. Conversion calls choose one explicit policy: `Exact`, `Floor`, `Ceil`, or `NearestTiesToEven`. There is no default rounding. Source floating-point Live Client seconds are finite/range-checked and rounded once to game microseconds with nearest-ties-to-even; all subsequent mapping is integer/rational.

Replay zero is the first validated video presentation boundary. Video coverage is the half-open interval `[0, video_end)`, where `video_end` is the one-past-last-frame boundary. Container duration, video coverage, and audio coverage remain separate facts. An event instant is not silently snapped; only a UI/clip range projection selects a frame boundary.

### 2. Strict schema-v2 recording bundle

Every newly allocated recording directory receives a random lowercase hyphenated UUID `media_id`. The recorder writes it into the initial schema-v2 `game_log.json` before capture starts and repeats it in final schema-v2 `metadata.json`. It identifies one media generation; it is not a content hash and does not claim to detect arbitrary file replacement. Playback/export validate equality plus the strict probed facts that matter to their operation.

`metadata.json` remains the owner of ordinary recording/library facts and gains one required `media_timeline` object containing:

- contract version and fixed replay scale;
- `media_id`;
- video codec/profile, source MP4 time base, first presentation PTS, exact selected frame-rate rational, exact frame count, last/one-past-last presentation boundary, normalized replay coverage, and `exact_cfr` cadence proof;
- audio codec, sample rate, source time base, first presentation PTS, normalized start/end coverage, and whether encoded audio exists;
- container start/duration as non-authoritative probe facts;
- recorder-owned expected frame count/rate/backend evidence used to reconcile the final file;
- capture/QPC and wall anchors only as explicitly named provenance fields.

`game_log.json` owns descriptive League observations and one optional calibration object. It contains `schema_version: 2`, the same `media_id`, event/snapshot `game_tick` values, and no `video_time_ms`, `video_offset_ms`, or other precomputed replay position. The calibration stores status, sample count, sampling window, request-midpoint observations, accepted RTT bounds, residual/uncertainty bounds, and the checked affine map:

```text
replay_tick = game_tick * 48 + replay_tick_at_game_zero
```

The metadata timeline is not duplicated in the game log. A recording with valid media but unavailable Live Client calibration remains playable; its game events are explicitly unmapped. A mapped observation outside video coverage yields `before_media` or `after_media`, not saturation, silent dropping, or corruption.

Delete the old timing fields and all parsing branches that consume them. Rebuild source-controlled tests, mocks, fixture generators, and ignored sentinel-corpus preparation around schema v2. A missing, malformed, unknown, or identity-mismatched contract makes the development bundle non-playable with a precise “rebuild development library” diagnostic. Preserve bytes and partial-recovery policy; do not reinterpret the bundle. Historical accepted benchmark reports remain immutable evidence, while new benchmark inputs and result schemas use v2 product coordinates.

### 3. Common media finalization and exact CFR proof

Refactor both recorder backends to stop into a private `CompletedVideoCandidate`, not a public canonical video. It contains the partial path plus recorder-owned expected rate/frame-count/cadence evidence. A common finalizer in the recorder then:

1. waits for the media child/muxer to close and flush;
2. runs the packaged ffprobe with a fixed argument list, five-second timeout, and at most 1 MiB combined output;
3. validates exactly one supported video stream, the expected optional/required audio stream, parseable bounded rationals/integers, normalized recorder output start, duration/coverage consistency, codec/profile policy, and expected producer evidence;
4. builds `MediaTimelineV2` with checked conversions;
5. stages final metadata atomically;
6. renames the candidate to canonical `video.mp4`, then publishes canonical `metadata.json` atomically;
7. treats a crash or failure between those steps as incomplete/recoverable, never as a playable schema-v2 recording.

A normal finalizer does not request every packet/frame. Exact CFR proof combines the recorder-owned accepted-frame/CFR schedule with bounded stream facts. Dedicated offline fixtures additionally scan presentation PTS in presentation order and prove:

- the selected frame-rate rational is exact;
- first presentation maps to replay zero;
- every adjacent presentation boundary is one rational frame duration;
- frame count and the one-past-last boundary agree;
- decode-order DTS reordering does not change presentation-order PTS semantics.

The MP4 stream time base need not equal the reciprocal frame rate; rational equality is the criterion. B-frames are permitted only when presentation PTS still passes the grid proof. Missing or contradictory proof preserves `video.partial.mp4` and emits no canonical success metadata/video.

For the external producer, replace the current ambiguous double synchronization with one explicitly validated filter graph using FFmpeg’s `fps=fps=<selected-rational>:start_time=0` after GPU scaling. Remove redundant `-r`/`-fps_mode` behavior where it could create a second timestamp authority. Add `-shortest` to cap a known extra audio tail, but do not use it as proof of audio alignment. Runtime capability validation must require the `fps` filter. The native producer keeps its present exact CFR clock and media format.

### 4. Game-clock calibration

Change poller sampling to capture request start and finish in the same QPC/`Instant` domain and use the request midpoint. Persist only accepted samples. The deterministic reducer rejects nonfinite/regressing/restarted game clocks, excessive RTT, insufficient span, and residuals above the documented bound. It reports calibration as `available`, `temporarily_unavailable`, or `invalidated`, never as a guessed zero offset.

Use at least five accepted samples across at least 750 ms. Preserve the existing 250 ms neighborhood as an initial maximum residual/uncertainty gate, but expose the actual max RTT/residual/uncertainty so fixture evidence can tighten it without changing semantics. A frozen, backward, or restarted game clock invalidates the active fit and requires a fresh window. Calibration work remains at the current low polling cadence and adds no capture-frame work.

Events and snapshots store their exact `game_tick` once. App mapping calls the same affine conversion for both. A wall timestamp may label a recording in the library but cannot repair or substitute for a missing game mapping.

### 5. Strict backend and frontend boundaries

Replace bare millisecond/FPS product fields in `app/src-tauri/src/library.rs`, `app/src/types.ts`, and `app/src/api.ts` with versioned DTOs carrying canonical decimal strings, exact rationals, `media_id`, coverage, and explicit mapping status. Remove serde defaults from required timeline fields.

`PlaybackProbe` includes one validated media-timeline descriptor and mapped event/snapshot results. A future ReplayIndex may store only `ReplayTimeReference { media_id, replay_tick }` from this feature; this feature does not create or persist an index.

Keep milliseconds where they truly are observer/UI-duration values and name them accordingly. `benchmark.monotonic_ms` remains an observer metric. Formatting helpers may derive display milliseconds/seconds from a replay tick at the final UI edge but never feed a rounded display value back into navigation or export.

### 6. Viewer state and browser adaptation

Represent these states separately:

- user preview target;
- native seek dispatch target;
- browser `seeked` observation;
- first qualifying presented-frame observation;
- current authoritative presented video position.

Each loaded media has a generation, and every dispatched seek has a monotonically increasing seek epoch within that generation. A later epoch supersedes an earlier one even if the source URL/generation did not change. A `seeked` event or RVFC callback settles only the current epoch and only when its observed media position is within the exact target/frame tolerance. Stale/off-target callbacks are measured and ignored for state authority until a valid observation or the existing bounded recovery path resolves the attempt.

The browser adapter maps:

```text
browser currentTime = validated media presentation origin in seconds + replay tick / 48,000,000
```

It verifies the loaded element’s seekable start against the validated origin. Fixtures include a nonzero media start so accidental zero assumptions fail. RVFC `mediaTime` is authoritative for presented video when supported. A `currentTime` fallback is explicitly `media-clock-approximate`, never a frame-presentation proof and never a legacy compatibility path.

Play, pause, rate change, fullscreen/layout change, recovery, and frame stepping do not create a new coordinate transform. Recovery invalidates pending browser observations and increments generation. Frame stepping targets an exact adjacent frame boundary and settles on presented-frame evidence. This milestone makes only the minimum scheduler/state changes needed for the contract; the larger playback-controller extraction remains `QB-REPLAY-009`.

The primary video is master for clip preview. Backdrop video and optional music preview receive clip-relative targets derived from the primary replay tick, use seek epochs, and resynchronize only after a documented drift threshold/cooldown to avoid seek storms. Their clocks are measured slave observations and can never become clip endpoint authority. RVFC provides no direct proof about audible output; encoded A/V fixtures supply that evidence.

### 7. Exact clip and export ranges

Replace clip endpoint milliseconds with:

```text
ClipRange {
    media_id,
    start_frame,
    end_frame_exclusive
}
```

Ranges are half-open. UI time-to-range projection floors the start to the containing frame boundary and ceils the end to the first excluded boundary. Clamp only to `[0, frame_count]`; `end_frame_exclusive == frame_count` is valid. Reject empty ranges and the existing minimum-duration violation explicitly. Preserve an event’s exact tick separately from its frame-snapped clip window.

The Tauri exporter reloads the bundle, requires schema v2, rechecks `media_id` and media timeline facts, and rejects stale requests before touching output. Derive FFmpeg timestamps/counts from exact integers/rationals with sufficient decimal precision; do not round through JavaScript seconds or three-decimal strings.

Retain accurate input seek as an optimization only after fixture proof. Apply an exact decoded video trim/select boundary and reset output PTS, trim audio from the same canonical interval and reset audio PTS, then re-encode through the existing preset policy. Post-export validation proves zero-based output, expected first source frame, exact output frame count/coverage within one output time-base tick, and encoded video/audio marker alignment. If a supported codec/filter path cannot prove those facts, fail or record the requested and observed bounded result; never silently claim exactness. Copy/hybrid export stays out of scope.

### 8. A/V synchronization claim

Video owns replay zero and coverage. Audio stores its independent encoded start/end mapping to replay ticks. A controlled generator places visible frame markers and audio impulses at early, middle, and late points. Native and external recordings, WebView playback observations, clip preview, and exported clips must show no increasing drift and no marker disagreement greater than 50 ms. Audio outside video coverage is ignored/capped and cannot lengthen replay or clip ranges.

This proves the relationship encoded in the saved media and maintained by playback/export. It does not claim to measure hardware output latency or what a listener physically heard during capture.

Future two-group audio streams must reference the same `media_id` and media timeline and be slaved to primary presented video. `QB-AUDIO-001` remains responsible for implementing those streams and controls.

### 9. Future segment reservation

Reserve, but do not implement, a segment descriptor:

```text
SegmentTimeReference {
    media_id,
    match_time_at_replay_zero
}
```

An independent zero-based MP4 cannot be inserted into a match timeline without both values. Segment order, restart policy, overlap/gap policy, stitching, and recovery remain future work. The single current segment uses one immutable media identity and one replay zero.

### Rejected alternatives

- Keep schema v1 playable with approximate timing: rejected explicitly by the product owner because the app is in development and has no real users. It would multiply states, tests, branches, and ambiguity before launch.
- Use milliseconds as canonical storage: rejected because they cannot exactly address 60 FPS, 30000/1001 FPS, MP4 ticks, or audio samples.
- Use floating-point seconds everywhere: rejected because equality, rounding, serialization, and long-duration boundary behavior would be implicit and language-dependent.
- Use nanoseconds as a universal integer: rejected because common frame/sample rates are still not all integral and browser precision would remain a boundary problem.
- Store only raw MP4 PTS: rejected because source time bases vary and consumers still need one checked cross-stream/game coordinate; raw PTS remains recorded as provenance.
- Treat configured FPS/backend name as media proof: rejected by the fresh external fixture and by the distinction between producer intent and finalized presentation timestamps.
- Scan every packet during normal finalization: rejected as avoidable O(media) post-recording work. Use producer evidence plus a bounded stream summary; reserve full scans for dedicated fixtures.
- Make audio or music an independent playback clock: rejected because it permits drift and conflicting clip endpoints.
- Treat `seeked` or assigned `currentTime` as presented-frame proof: rejected because those are different browser events/states.
- Expand into ReplayIndex, segmentation, controller extraction, or stream-copy export: rejected because each has a separate feature owner and would make this feature unreviewable.

## Milestones

1. Add the pure `chronobreak-replay-time` crate, exact types/rational arithmetic, strict decimal wire grammar, fixed-scale/range constants, cross-language JSON Schema/golden fixtures, Rust tests, and TypeScript mirror tests.
2. Define strict metadata/game-log schema v2 and `media_id`; rebuild all repository mocks, tests, fixture generators, replay-benchmark corpus preparation, and current sentinel inputs. Remove old timing fields and parsing/fallback branches in the same change so there is only one supported contract.
3. Refactor backend stop results into the common bounded media finalizer. Add atomic publication ordering, strict ffprobe parsing/limits, normalized media facts, and failure-preserved partial tests.
4. Normalize the external backend through one explicit CFR filter authority, retain the native clock, cap audio tail, and prove both outputs with generated packet/frame and marker fixtures.
5. Replace poller millisecond/precomputed mappings with exact game microseconds, midpoint calibration, persisted fit quality, invalidation rules, and a single event/snapshot affine mapping path.
6. Replace Tauri and frontend replay payloads with strict typed timeline values. Add explicit unavailable/before/inside/after mapping results and retain observer milliseconds only where they are genuinely observer data.
7. Split viewer request/dispatch/seeked/presented state, add per-dispatch epochs and browser-origin adaptation, and slave backdrop/music preview to primary presented video without taking `QB-REPLAY-009` scope.
8. Replace clip/export endpoints with media-bound half-open frame ranges, exact FFmpeg boundary derivation, decoded trimming, and post-export frame/A/V validation.
9. Add `docs/architecture/replay-time.md`, update affected recorder/replay/export/performance documentation, run the full automated, fixture, WebView, and performance/reliability gates, record concrete evidence, and complete canonical state only when every acceptance criterion passes.

Each milestone ends with focused tests and a review of feature scope. Do not postpone old-field removal or fixture rebuilding into a later compatibility milestone; they are part of milestone 2.

## Verification

### Pure contract and cross-language fixtures

The committed golden suite must cover:

- strict signed/unsigned decimal grammar, zero, maximum duration, overflows, values above `2^53`, invalid rationals, and every rounding mode;
- 30/1, 60/1, and 30000/1001 frame grids; 48 kHz samples; `1/15360` raw PTS; and exact millisecond/microsecond conversion;
- frame zero, last frame, one-past-last boundary, between-frame event instants, floor/ceil clip projection, end-at-duration, empty range, and minimum duration;
- nonzero MP4 presentation origin, container/video/audio duration mismatch, audio lead/tail, off-grid/VFR rejection, and B-frame DTS/PTS reordering;
- calibration before/after game start, unavailable/invalidated calibration, large signed offset, before/inside/after media events, and media-identity mismatch;
- superseded seeks in one generation, stale-generation callbacks, off-target RVFC, recovery, and approximate currentTime fallback;
- stale export request, exact decoded trim, zero-based output, frame count, and early/middle/late A/V markers;
- malformed/missing/unknown schema-v2 fields and explicit rejection of schema-v1 inputs.

Planned focused commands:

```powershell
cargo test --manifest-path replay-time/Cargo.toml
cargo fmt --manifest-path replay-time/Cargo.toml -- --check
cargo clippy --manifest-path replay-time/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path recorder/Cargo.toml
cargo test --manifest-path app/src-tauri/Cargo.toml
npm run test --prefix app
```

### Recorder/media fixtures

Add `tools/replay_time/verify.ps1`. It creates or accepts only a dedicated root containing a `.chronobreak-replay-time` sentinel, uses the packaged media runtime, preserves failed output, and never mutates a real library. It generates short native and external recordings with visual timecodes/audio impulses; performs offline packet/frame scans; validates schema v2 and media identity; checks early/middle/late drift; tests a nonzero-start/edit-list-style media fixture; exports boundary/middle/end clips; and emits a bounded JSON result.

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_time/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -FixtureRoot build/replay-time/qb-replay-012
```

The normal finalizer test suite injects probe success, timeout, oversized output, malformed JSON, missing stream, wrong start/rate/count/coverage, codec mismatch, identity mismatch, publication failure, and crash-window states. It proves no canonical playable bundle appears without coherent video, metadata, and game-log identity.

### App/WebView behavior

Using only the generated schema-v2 fixture library, exercise play, pause, rapid superseding seeks, event jumps, frame steps, resume, all existing supported rate paths, recovery, fullscreen/layout transitions, clip endpoint edits, backdrop/music preview, and exports. Record request, dispatch, `seeked`, first qualifying RVFC frame, target error, generation/epoch, recovery, and fallback mode. Compare early/middle/late visible/audio markers and exact source/export frame identities.

Use the `QB-REPLAY-008` production-WebView harness for balanced before/after trials with identical fixture/runtime/environment fingerprints. Any new media error, timeout, recovery exhaustion, stale result, source mutation, event loss, or sustained resource growth fails. Other unfavorable deltas require disposition when they exceed both the baseline repeatability band and 5%, as defined by the replay benchmark protocol.

### Project-wide checks

Run all applicable commands from `docs/development/VERIFICATION.md`, including:

```powershell
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path recorder/Cargo.toml --all-features
cargo check --manifest-path app/src-tauri/Cargo.toml --all-targets
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
npm run desktop:build --prefix app
python -m unittest discover -s tools/replay_benchmark/tests -v
```

Also run the complete canonical feature-list schema/invariant/linked-artifact block from `docs/development/VERIFICATION.md`, packaged-media-runtime verification required by the fixture, and `git diff --check`.

## Performance and reliability gates

- Recording hot path: no replay-time serialization, ffprobe, file scan, allocation growing with duration, or frontend work runs per capture frame. Native accepted-frame scheduling stays unchanged. Poller work remains at its existing low cadence.
- Finalization: the production probe uses stream/format summaries only, caps combined output at 1 MiB, times out after five seconds, and preserves partial media on failure. Across at least five short, representative, and one-hour dedicated fixtures, its p95 must be no more than two seconds and any result above the existing replay-benchmark repeatability band plus 5% requires disposition. Retain exact elapsed/output evidence.
- External normalization: a generated 240-second external run must show exact selected-rate frame boundaries, bounded duplicate/drop diagnostics, no unbounded queue/memory growth, no increasing A/V drift, and no regression in target-close/finalization behavior. Compare capture cost through the applicable existing capture protocol; do not manufacture or broaden a negligible-League-impact claim.
- Payload/open: compare equivalent schema-v2 replay open and event-heavy fixtures with at least five balanced before/after trials. No media error or stale mapping is allowed; unfavorable latency/resource/payload deltas follow the `QB-REPLAY-008` repeatability-plus-5% disposition rule. Removing duplicated `video_time_ms` should keep game-log growth bounded.
- Viewer: repeated seek/rate/fullscreen/recovery and clip-preview torture must leave no unresolved current epoch, no old-generation authority, no seek storm, and no sustained resource growth.
- Export: source hashes remain unchanged; failed exports do not publish canonical clips; output media passes strict frame-count/start/coverage/A/V marker checks.
- Safety: every destructive/corrupt/stale fixture lives under a dedicated sentinel root. No real recording, clip library, user-selected media directory, configuration, or benchmark evidence is overwritten or deleted.
