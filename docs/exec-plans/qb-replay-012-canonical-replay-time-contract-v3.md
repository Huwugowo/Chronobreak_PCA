# QB-REPLAY-012: Canonical Replay Time Contract, Native Recorder Revision

This immutable ExecPlan supersedes
`docs/exec-plans/qb-replay-012-canonical-replay-time-contract-v2.md`. That reviewed
draft and the original `docs/exec-plans/qb-replay-012-canonical-replay-time-contract.md`
remain unchanged as planning history. This plan is self-contained: predecessor plans are
provenance only and are not required to execute its exact schema-v2, native-recorder,
fixture, finalization, playback, or export design.

## Purpose

Chronobreak must use one exact replay coordinate system across capture provenance,
encoded video presentation, encoded audio presentation, League game observations,
browser seek/presentation state, and clip/export boundaries. A user-visible event,
frame step, clip endpoint, and exported frame must identify the same instant in the
same media generation.

The Windows recorder must also have one truthful production architecture. The native
WGC/D3D11/NVENC recorder is that architecture. The retired FFmpeg-driven WGC recorder
must not remain as a selector, fallback, compatibility claim, or required verification
subject. FFmpeg remains responsible where it is the appropriate implementation:
audio encoding, fragmented-MP4 muxing, bounded probing, export/transcoding, and test
fixture generation.

This remains a pre-release breaking schema cutover. Schema v2 is the only supported
recording-bundle contract. No schema-v1 parser, migration, repair, or approximate
timing fallback is added.

## Relevant planning-time architecture

- `replay-time/` is the pure Rust owner of exact rational values, strict decimal wire
  grammar, schema-v2 timeline structures, and bounded ffprobe fact parsing.
- `app/src/replayTime.ts` mirrors that contract at the browser boundary and shares the
  committed golden fixtures under `fixtures/replay-time/v2/`.
- `recorder/src/native/clock.rs` owns exact CFR admission. A tick commits only after
  native NVENC submission succeeds; source supersession and CFR duplication/discard
  remain separate diagnostics.
- `recorder/src/native/session.rs` owns exact-HWND WGC acquisition, bounded D3D11
  conversion resources, direct NVENC submission/completion, and terminal accounting.
- `recorder/src/native/mux.rs` owns the boundary from accepted Annex-B H.264 packets to
  the supervised FFmpeg audio/mux process. FFmpeg does not capture, scale, or encode
  production Windows video.
- `recorder/src/finalizer.rs` owns bounded packaged-ffprobe validation and publication
  of a private `CompletedVideoCandidate` as canonical `video.mp4` plus schema-v2
  metadata.
- `recorder/src/poller.rs` maps Live Client observations through one persisted affine
  calibration based on capture-clock request midpoints.
- Tauri library/export code and the React viewer consume strict media identity and
  exact frame/tick values. The primary presented video frame is playback authority.
- `tools/native_backend/run_native_fixture.ps1` is the hardware-backed recorder
  fixture. `tools/replay_time/verify.ps1` owns the sentinel-scoped end-to-end replay
  contract verification and must consume genuine native-recorder evidence.
- The packaged r6 media runtime still contains historical WGC filters and hardware
  encoders. Its immutable hash/build provenance is not a production recorder surface;
  removing compiled capabilities requires a separately locked runtime revision.

## Scope and non-goals

In scope:

- the fixed exact replay coordinate system and strict schema-v2 bundle contract;
- one immutable `media_id`, validated video/audio timeline facts, and checked
  game-to-replay calibration;
- one Windows production recorder: native exact-HWND WGC, D3D11 conversion, and direct
  H.264 High 1080p60 NVENC;
- clean removal of the FFmpeg-driven WGC capture backend, selector, fallback,
  backend-specific candidate planning, active runner, and production support claims;
- exact native video presentation timestamps at the FFmpeg mux boundary;
- shared fragmented-MP4 behavior that represents AAC priming without shifting the
  zero-based video grid and preserves truthful partial/finalization semantics;
- a bounded common finalizer and ordered per-file candidate/metadata publication with explicit crash-window classification;
- exact viewer seek epochs, presented-frame authority, and frame-addressed export;
- strict native-recorder visual/audio marker evidence, long-run resource evidence,
  finalizer timing evidence, and production WebView/export verification.

Out of scope:

- restoring an external capture backend for HEVC, non-High profiles, AMD/AMF,
  Intel/QSV, or process-level driver-hang containment;
- silently remapping unsupported Windows codec/profile/vendor choices;
- rebuilding the pinned media runtime solely to remove unused compiled capabilities;
- schema-v1 compatibility, migration, approximate playback, or inferred timeline
  repair;
- timestamp-aware in-process MP4 muxing, recording segmentation, stitching, or partial
  media recovery beyond the existing preserved-candidate policy;
- ReplayIndex implementation, controller extraction, new playback-rate UI, separate
  captured audio stems, or copy/hybrid export.

## Exploration findings

- Full video frames can remain GPU-resident from WGC through D3D11 conversion and
  native NVENC without a per-frame host readback or CPU pixel conversion.
- The FFmpeg-driven recorder does not uniquely own audio capture, muxing, finalization,
  publication, or recovery policy. Its unique exposed codec/vendor combinations were
  not hardware-validated product combinations.
- Raw native Annex-B packets have no persistent presentation timestamps. Exact CFR
  intent therefore must be assigned explicitly at the FFmpeg mux boundary and checked
  against finalized MP4 presentation order.
- AAC priming can begin at negative DTS. Fragmented MP4 written with an immediate
  empty movie header cannot describe that relationship correctly and can cause later
  timestamp normalization to shift an otherwise exact video grid.
- A normal finalizer may use bounded stream/format summaries; scanning every frame or
  packet is reserved for dedicated fixtures. Representative one-hour media still must
  demonstrate that this bounded probe meets the latency gate.
- A visual marker drawn cell-by-cell can be captured between writes and decode as a
  plausible but false timestamp. Gray coding alone does not make publication atomic.
  Marker correctness requires an explicit pre-epoch state, one shared audio/visual
  epoch, atomic visual publication, integrity bits, and decoding by actual video PTS.
- Historical recorder A/B evidence remains useful architecture evidence, but the
  retired recorder is not a current acceptance subject and must not be relabeled as a
  supported fallback.

## Chosen design and rationale

### 1. Exact replay coordinate system

Use `REPLAY_TICKS_PER_SECOND = 48,000,000` with a maximum replay duration of 24
hours. This exactly represents 30/1, 60/1, 30000/1001, 48 kHz audio samples, the
current 1/15360 MP4 video time base, milliseconds, and microseconds.

Persist tick, frame, sample, and raw-PTS integers as canonical decimal strings. Rust
uses checked integer values with widened intermediates; TypeScript validates with
`BigInt` and converts to bounded branded numbers only at the DOM edge. Every rational
conversion explicitly chooses exact, floor, ceil, or nearest-ties-to-even behavior.
There is no implicit rounding or saturating timeline conversion. Canonical unsigned
wire integers accept only `0` or `[1-9][0-9]*`; signed affine offsets accept `0` or
`-?[1-9][0-9]*`. Parsers reject `-0`, plus signs, whitespace, exponents, fractions,
leading zeroes, overlong input, values above the 24-hour domain, and arithmetic
overflow before conversion. Distinct types represent `ReplayTick`, `SignedReplayTick`,
`FrameIndex`, `FrameBoundary`, `AudioSampleIndex`, `RawMediaPts`, one-million-Hz
`GameTick`, DOM-only browser seconds, observer-only monotonic milliseconds, and opaque
`MediaId`.

Replay zero is the first validated video presentation boundary. Video coverage is
half-open `[0, video_end)`. Container duration, video coverage, and audio coverage are
separate facts.

### 2. Strict schema-v2 media identity

Each recording generation receives a lowercase hyphenated UUID `media_id` shared by
its initial `game_log.json`, finalized `metadata.json`, playback payloads, and export
requests. Metadata owns exact probed video/audio facts and recorder producer evidence.
The game log stores exact integer-microsecond observations and one optional checked
affine calibration; it stores no derived video milliseconds.

Missing, unknown, malformed, stale, or identity-mismatched schema-v2 data is rejected
without mutating source bytes. Valid media with unavailable game calibration remains
playable while its events are explicitly unmapped.

### 3. Sole native Windows recorder

On Windows, `service.rs` exposes only the native recording session. There is no
`QUEUEBACK_WINDOWS_RECORDER_BACKEND`, external WGC candidate list, or cross-backend
retry. Initialization accepts only the native H.264 High 1920x1080 60-FPS contract and
reports unsupported configuration directly.

The native session owns WGC/D3D11/NVENC and bounded worker/completion resources.
FFmpeg receives encoded H.264 plus selected loopback or silent audio, encodes AAC, and
muxes fragmented MP4. Generic non-Windows FFmpeg recording may remain behind its
platform boundary; it is not a Windows capture backend.

### 4. Exact native mux timeline

The mux boundary assigns the already accepted native packet sequence the rational CFR
presentation grid explicitly: packet `N` receives video PTS/DTS `N` and duration one
in the selected frame-rate time base before MP4 rescaling. The mux invocation disables
negative-timestamp shifting so AAC priming cannot move video replay zero.

Fragmented MP4 uses `delay_moov` with the existing fragment-flush flags. Delaying the
initial movie header lets FFmpeg represent AAC priming before it fixes the initial
track metadata. This choice is accepted only with dedicated evidence for clean stop,
target close, injected NVENC failure, mux failure, and interruption before the first
completed fragment. An unsuccessful candidate remains noncanonical; preservation of
bytes never becomes a false recoverability or playability claim.

Offline verification scans video packets/frames in presentation order and proves the
selected rational rate, first PTS at replay zero, exact adjacent boundaries, exact
frame count/one-past-last boundary, and PTS semantics independent of decode-order DTS.

### 5. Bounded common finalization

Every recorder stop yields a private `CompletedVideoCandidate`. The common finalizer:

1. waits for native/mux ownership to close and flush;
2. runs packaged ffprobe with a fixed stream/format-summary argument list, five-second
   timeout, and one-MiB combined-output cap;
3. validates stream cardinality, codec/profile, bounded rationals/integers, normalized
   video start, exact producer rate/count evidence, and coherent video/audio coverage;
4. builds `MediaTimelineV2` with checked conversions;
5. writes and fsyncs `metadata.pending.json` through an atomically replaced temporary;
6. renames the private candidate to canonical `video.mp4`;
7. atomically renames the validated pending metadata to canonical `metadata.json`;
8. classifies termination between the two canonical renames as an incomplete,
   recoverable directory rather than bundle-atomic success. Library discovery rejects
   canonical video without matching schema-v2 metadata/media identity, startup never
   resumes or overwrites that directory, and cleanup preserves its bytes for explicit
   recovery/disposition. Each file rename is atomic; the two-file publication is
   deliberately ordered, not falsely described as bundle-atomic. Probe or pre-rename
   failure leaves only the documented private partial candidate.

A representative harness invokes this production finalizer path over at least five
short/medium/long dedicated fixtures including one hour. It records elapsed time,
probe output size, result, fixture identity, and repeated measurements without reading
or mutating a user library.

### 6. Game, browser, and export boundaries

Live Client samples use request start/finish in one monotonic domain and their
midpoint. The reducer requires at least five accepted samples spanning at least 750 ms;
it rejects nonfinite, backward, restarted, or frozen game clocks, excessive RTT, and a
fit whose residual or uncertainty exceeds 250 ms. It persists accepted sample count,
window, RTT/residual/uncertainty bounds, status, and the checked integer affine map
`replay_tick = game_tick * 48 + replay_tick_at_game_zero`. Invalidated calibration
requires a fresh sample window and never becomes a guessed zero offset. Events and
snapshots use the same mapping and report before/inside/after/unavailable rather than
saturating or dropping.

Viewer state separates preview intent, dispatched seek, browser `seeked`, first
qualifying presented frame, and current authoritative presented position. Every media
load has a generation and every dispatch a monotonically increasing epoch. A later
dispatch supersedes every earlier epoch in the same generation. `seeked` and RVFC may
settle only the current epoch and only within the exact target/frame tolerance; stale
or off-target callbacks remain telemetry, never authority. Browser `currentTime` equals
validated media presentation origin plus replay tick divided by 48,000,000. RVFC
`mediaTime` is presented-frame evidence when available. Without RVFC, the adapter emits
an explicit `media-clock-approximate` observation that cannot prove frame presentation.
Recovery cancels pending nudges/observations, restores transient playback state, and
increments the media generation before redispatch so no old callback can settle it.

Clip ranges are media-bound half-open `ClipRange { media_id, start_frame,
end_frame_exclusive }` intervals. UI projection floors the start to its containing
frame boundary, ceils the end to the first excluded boundary, clamps only to
`[0, frame_count]`, permits `end_frame_exclusive == frame_count`, and rejects empty or
minimum-duration-violating ranges. Export reloads schema v2, rechecks identity/facts,
derives precise seek/trim values from exact rationals, uses accurate input seek only as
an optimization, applies exact decoded video selection/trim plus corresponding audio
trim, resets both output PTS, and validates zero-based output, exact first source frame,
exact output frame count/coverage, decode, and A/V markers. Source media is opened
read-only and hash-checked before and after.

### 7. Trustworthy native A/V marker protocol

The controlled fixture owns one monotonic epoch established when the 48 kHz PCM source
commits sample zero. Before that commit, the visual surface displays a reserved
pre-epoch word that cannot decode as timestamp zero. The PCM manifest records the
same epoch generation and the exact early/middle/late impulse sample positions.

Each visual update is rendered completely into an off-screen DIB and published to the
window in one BitBlt. The word contains a protocol/version prefix, elapsed timestamp,
generation/integrity bits, and a commit relation duplicated across spatially separated
cells. The decoder accepts only a complete matching word; it never repairs, coerces,
or carries forward a corrupt payload. Valid timestamps must be monotonic.

The verifier uses decoded video presentation PTS, not decoded-frame ordinal, to locate
the visual observations surrounding each scheduled marker. It compares those
observations with impulses detected in decoded 48 kHz PCM. Every post-epoch frame must
have a valid payload; the strict invalid-payload bound is zero. Early, middle, and late
absolute disagreement must each be at most 50 ms, and later disagreement may not grow
by more than 5 ms over the early baseline.

The same native run must also pass exact PTS-grid scanning, ffprobe stream facts, full
decode, changing-frame hashes, and reconciled bounded native telemetry. A fixture or
verifier failure is not attributed to production timing until protocol integrity and
source epoch checks pass.

### 8. Future segment reservation

Reserve `SegmentTimeReference { media_id, match_time_at_replay_zero }` without
implementing segmentation. Independent zero-based MP4 segments cannot be reinterpreted
as one match timeline without both values and a future explicit gap/overlap policy.

## Rejected alternatives and planning decisions

- Retain the FFmpeg WGC recorder for compatibility: rejected because it duplicates a
  load-bearing capture lifecycle while its unique codec/vendor combinations lack
  validated production obligations.
- Keep a backend environment selector for diagnostics: rejected because it preserves
  two production-shaped code paths and lets historical comparison tooling define
  product support.
- Silently map HEVC/non-High/AMF/QSV configuration to native H.264 NVENC: rejected as
  dishonest configuration behavior.
- Remove FFmpeg entirely: rejected because it remains the correct bounded mux/probe,
  export/transcode, and fixture implementation.
- Rewrite the pinned runtime immediately: rejected because immutable runtime provenance
  must remain truthful; unused compiled capability removal requires a new lock/build.
- Accept torn visual words or a percentage invalid allowance: rejected because a
  relaxed verifier can hide fixture races and produce false timing conclusions.
- Scan every packet during normal finalization: rejected as avoidable duration-scaled
  work; full scans belong to dedicated verification.
- Use milliseconds or floating seconds as canonical storage: rejected because common
  frame/audio grids and exact cross-language boundaries are not representable safely.
- Keep schema-v1 compatibility or approximate timing: rejected by the explicit
  pre-release clean-break decision.
- Treat assigned `currentTime` or `seeked` as presented-frame proof: rejected because
  neither proves which frame reached composition.

## Milestones

1. Establish the pure Rust/TypeScript exact contract, golden fixtures, strict schema-v2
   bundle identity, and checked game calibration.
2. Consolidate Windows recording on the native backend; remove external selection,
   active runner, support claims, and backend-specific verifier gates while preserving
   FFmpeg's non-capture responsibilities.
3. Make native mux timestamps exact and validate `delay_moov` across clean and failed
   lifecycle boundaries.
4. Complete bounded finalizer/publication, typed Tauri/frontend boundaries, viewer seek
   authority, and frame-addressed export.
5. Replace the experimental marker path with the single-epoch, atomically published,
   strict native marker protocol and bind it into the native runner/verifier.
6. Run short exact-grid/A/V proof, native failure scenarios, 30-minute resource soak,
   representative finalizer timing, production WebView/export checks, project-wide
   verification and final review.

Each milestone ends with focused verification. Old selectors, aliases, fallback
branches, backend-specific runner contracts, and relaxed verifier tolerances are
removed in the same cutover rather than deprecated.

## Verification design

Focused contract and application checks:

```powershell
cargo test --manifest-path replay-time/Cargo.toml
cargo test --manifest-path recorder/Cargo.toml --all-features
cargo test --manifest-path app/src-tauri/Cargo.toml
npm run test --prefix app
```

Native recorder and media scenarios, all under ignored sentinel-owned roots:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 6 -ReplayTimeMarkers
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario close_window -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption nvenc_failure -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption mux_failure -DurationSeconds 10
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -Interruption pre_first_fragment -DurationSeconds 2
powershell -NoProfile -ExecutionPolicy Bypass -File tools/native_backend/run_native_fixture.ps1 -Scenario steady -DurationSeconds 1800 -CollectResources -KeepTargetVisible -ResourceSampleSeconds 5
cargo run --manifest-path recorder/Cargo.toml --example finalizer_benchmark --features replay-time-fixture -- --manifest build/replay-time/qb-replay-012/finalizer-benchmark.json --result build/replay-time/qb-replay-012/finalizer-benchmark.result.json
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_time/verify.ps1 -RuntimeRoot build/media-runtime/windows-x86_64 -FixtureRoot build/replay-time/qb-replay-012
```

Milestone 5 adds the runner switch `-ReplayTimeMarkers`; it passes the fixture's PCM
endpoint into the native mux as `AudioSource::ReplayTimeFixturePcm`, persists the marker
source manifest, and invokes the strict PTS-based decoder. The short run emits a
manifest/result binding fixture generation, native runner result, source/copy hashes,
exact packet/frame grid, zero-invalid marker proof, three A/V measurements, decode/hash
facts, and terminal native counters. No synthetic source may be relabeled as native
recorder evidence.

Milestone 3 extends the runner `Interruption` set with `mux_failure` and
`pre_first_fragment`. The probe exposes `--fail-mux-after-writes <N>` only under
`native-failure-injection`; the session kills/reaps its owned FFmpeg child once the
writer-call counter reaches N. `mux_failure` fires after 120 writes so a completed
fragment exists and must remain a probeable partial without canonical publication.
`pre_first_fragment` fires after the first write, before a fragment boundary; bytes may
be absent or unprobeable and are preserved, but must never be labeled recoverable or
canonical. Target-close and NVENC-failure retain their exact terminal reason. Every
scenario must reap fixture, native worker, completion thread, pipes, and FFmpeg under
the existing finite deadlines.

Milestone 6 adds `recorder/examples/finalizer_benchmark.rs` plus a
`replay-time-fixture`-gated library wrapper that calls the same private
`finalizer::validate_candidate` used by production. Its strict manifest lists at least
five read-only source paths, expected rate/frame count/codecs/runtime ID, a dedicated
sentinel copy root, and repetitions. The harness hash-checks each source before/after,
copies it to a unique `.partial.mp4`, constructs the real candidate/expectations, times
validation without publication, deletes no source, and writes a bounded create-new JSON
result containing every elapsed time/output/result plus p95. The set includes one-hour
media and at least five repetitions per fixture; p95 above two seconds or an observation
outside the replay-benchmark repeatability band plus 5% requires a recorded disposition.

Run applicable project checks from `docs/development/VERIFICATION.md`, including:

```powershell
cargo fmt --manifest-path replay-time/Cargo.toml -- --check
cargo clippy --manifest-path replay-time/Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo check --manifest-path app/src-tauri/Cargo.toml --all-targets
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run check --prefix app
npm run build --prefix app
npm run desktop:build --prefix app
python -m unittest discover -s tools/replay_benchmark/tests -v
```

Also run packaged-runtime verification, release checks applicable to changed recorder
packaging, feature-list schema/invariant/linked-artifact validation, `git diff --check`,
and a final evidence review.

## Performance and reliability gates

- Recording hot path: no replay-time serialization, probe, full-media scan, unbounded
  allocation, or frontend work per capture frame. WGC callback, latest-frame handoff,
  encoder slots, and mux pipes remain explicitly finite.
- Native exactness: every accepted 60 Hz tick maps to one finalized presentation frame;
  first PTS is replay zero, adjacent boundaries are exact, and terminal
  scheduled/submitted/completed/muxed accounting reconciles.
- Native long run: 30 minutes of continually advancing changing media, finite declared
  texture/queue/encoder depths, no sustained process/GPU-memory growth, no output
  stall, exact terminal accounting, full decode, and no A/V drift-growth violation.
- Marker A/V: zero invalid post-epoch words, each early/middle/late marker within 50 ms,
  and later absolute disagreement no more than 5 ms above the early baseline.
- Finalization: fixed summary probe, at most one MiB combined output, five-second
  timeout, partial preservation on failure, p95 no more than two seconds across the
  representative set, and disposition for any result outside the existing
  repeatability band plus 5%.
- Viewer/export: no unresolved current epoch, old-generation authority, seek storm,
  source mutation, canonical output on failure, media decode error, incorrect source
  frame, or A/V marker violation.
- Safety: destructive, stale, corrupt, interruption, and publication-failure fixtures
  remain under dedicated sentinels. No user recording/library/configuration or retained
  benchmark evidence is overwritten or deleted.
