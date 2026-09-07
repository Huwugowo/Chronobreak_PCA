# QB-REPLAY-009: Playback boundary and speed ladder

## Purpose

Give the replay viewer one concrete owner for browser playback and expose the six
replay speeds in both layouts. A user can seek, pause, change speed, adjust sound,
and recover a failed preview without losing the replay position or replacing the
video element. Report what the WebView actually presents, including a rate it
cannot sustain and a decoder that falls back to software.

`feature-list.json` owns acceptance. `docs/execution/qb-replay-009.md` owns execution
facts. This document defines implementation design, including the verification
that must precede a hardware-decoded performance claim.

## Relevant planning-time architecture

All three dependencies, QB-REPLAY-006, QB-REPLAY-008, and QB-REPLAY-012, are recorded
as done. Use their current implementation and durable architecture; their earlier
ExecPlans are provenance and are not prerequisite reads.

| Owner | Existing implementation and boundary to preserve |
| --- | --- |
| Playback payload | `app/src-tauri/src/library.rs`, `lib.rs`, `app/src/api.ts`, and `types.ts` validate schema-v2 recording identity and expose the video URL and validated timeline. No whole-file video crosses IPC. |
| Video transport | `app/src-tauri/src/playback_server.rs` serves validated loopback byte ranges with bounded streaming and cheap production counters. |
| Replay time | `app/src/replayTime.ts` and `replay-time/` define the same exact 48,000,000-tick timeline. Browser seconds are converted with `browserSecondsForReplayTick` and `replayTickForBrowserSeconds` using the video PTS origin. |
| Seek facts | `app/src/viewerSeekState.ts` separates requested preview, dispatched request with generation/epoch, seeked observation, and RVFC presentation. Recovery invalidates observations. Preserve and extend this module instead of constructing a second clock. |
| Viewer | `PlaybackSurface` in `app/src/components/ViewerScreen.tsx` owns one persistent `<video>`, the scheduler, listeners, frame callbacks, recovery, metrics, and benchmark actions. Its outer `ViewerScreen` owns probe loading. |
| Layout | `FullscreenOverlay.tsx` receives values and callbacks. It never owns the media element. Fullscreen conditionally mounts an overlay around the same persistent viewer video. |
| Clip editing | `viewerUtils.ts` owns frame-bound half-open clip intervals. Viewer clip looping uses the primary video. `ClipExporterScreen.tsx` and `clipPreviewSync.ts` own a separate multi-element editor preview, outside this extraction. |
| Benchmark | `app/src/benchmark.ts`, `app/src-tauri/src/benchmark.rs`, and `tools/replay_benchmark/` own the sentinel-only production measurement path. `App.tsx` handles benchmark viewer disposal/remount cycles. |

At planning time the relevant viewer units are `observePresentedFrame`,
`syncPresentedTime`, `onVideoFrame`, `dispatchPendingSeek`, `seekTo`,
`attemptRecovery`, `playNativeVideo`, `updateMetrics`, the `onMount` media
listeners/cleanup, and `runBenchmarkAction`'s rate/play/seek operations.
The windowed and fullscreen control bars receive the same actions after extraction.

The normal recorder now produces native NVIDIA H.264 High 1920x1080 at 60 FPS in
MP4 with AAC; HEVC and the retired external capture backend are not supported
recording paths. Playback capability fixtures may still cover HEVC when the real
WebView supports it. This feature does not add recorder codecs or vendors. See
`docs/architecture/windows-capture.md`, `desktop-replay.md`, and `replay-time.md`.

## Scope and non-goals

Change primary replay playback ownership, rate/audio controls, diagnostics, and
the measurement needed to prove these changes. Keep one HTMLVideoElement-backed
implementation and a small injected adapter for deterministic tests.

Do not introduce a backend registry, MSE, libmpv, WebCodecs, native playback,
additional recording stems, an export strategy, ReplayIndex, schema migration,
recorder GOP/profile tuning, or a second video for fullscreen. Do not migrate the
clip editor's backdrop/music synchronization. Preserve dirty working-tree changes
from the replay-time and recorder work.

## Exploration findings

- Native seeks are limited to one in flight and one replaceable pending request,
  with a 100 ms dispatch interval, half-frame dispatch deduplication, and a 1,500 ms
  native-seek timeout. A single separate observation slot awaits presentation
  after `seeked`; it is not an additional queued native seek.
- A paused seek uses a bounded temporary 1/16x play nudge to obtain a presented
  frame, for at most 750 ms. Its saved raw playback rate must not overwrite a newer
  user selection.
  Native completion and first presentation need separate deadlines: clearing the
  native timeout at `seeked` does not prove a frame was presented.
- Existing recovery permits two automatic attempts within ten seconds, pauses,
  clears scheduling, increments the generation, calls `load()` on the same
  element, and seeks back after metadata. A finite consecutive-failure budget
  must also prevent an endless sequence of slow failed reloads.
- The pure seek state has generation checks, but some viewer callbacks read the
  current generation when they run. Extraction must capture observation ownership
  when callbacks are armed, cancel old work, and reject late completions.
- `replayPosition` remains a display cursor that can contain optimistic preview
  time. Consumers needing actual replay state must receive presented time
  explicitly. A `currentTime` fallback is approximate and cannot settle an
  authoritative frame seek.
- Existing Vitest tests run in Node. Pure time/seek and benchmark-action tests
  exist, but do not test the integrated media lifecycle. A fake media adapter and
  fake monotonic clock can prove scheduling and cleanup without pretending to
  prove codecs, audio output, or WebView behavior.
- The accepted QB-REPLAY-008 report recorded accurate 0.25x through 4x, then about
  0.00296x effective advancement after assigning 8x, followed by a failed rate
  transition. Assignment and `ratechange` are not successful playback evidence.
- That report used older recording bundles and a specific binary/WebView/runtime
  identity. It supplies historical findings, not an identity-matched numerical
  before arm for the schema-v2 viewer. The ten-minute torture arm had no sustained
  growth flag, while the short layout arm needs focused repeat measurement.
- The earlier Windows collector deliberately disabled unbounded GPU CIM queries.
  Preserve that decision. Actual media-decoder identity needs a separate bounded
  WebView diagnostic, not another synchronous GPU polling loop.

## Chosen design and rationale

### Concrete controller and adapter

Add `app/src/playbackController.ts`, `htmlVideoPlaybackAdapter.ts`, and their
focused tests. Reuse `viewerSeekState.ts` and `replayTime.ts`; small supporting
rate/diagnostic types may be colocated with the controller. The adapter is a test
seam for the browser object, not a public alternate-backend contract.

Create the adapter and controller once when `PlaybackSurface` mounts and give
them the existing element. `open` supplies the URL, immutable `mediaId`, validated
timeline, and initial preferences. Only this boundary may mutate the primary
video's source, currentTime, playbackRate, muted, volume, play/pause state, or
reload it. It owns all primary-media event listeners, RVFC/fallback handles,
timeouts, nudge state, readiness snapshots, and video-quality counters.

Expose typed commands for `open`, `play`, `pause`, `seek`, `setRate`, `setMuted`,
`setVolume`, `setLoopRange`, `retry`, and `dispose`; expose immutable snapshots and
subscriptions. Use a concrete union for readiness (`closed`, `loading`, `ready`,
`recovering`, `degraded`, `disposed`) alongside native readyState/networkState and
the original MediaError code/message. Loading metadata is not proof of first
presentation or successful play.

State includes media identity, generation, seek epoch, desired play/pause state,
requested preview, dispatched target, seeked observation, authoritative presented
tick with authority, approximate media clock, rate state, audio preferences,
recovery count, and bounded diagnostics. Native seconds exist only at adapter
conversion and explicitly named diagnostic/benchmark serialization boundaries.

Hot presentation subscribers should update only the timeline cursor and replay
state that needs the frame clock. Publish slow metrics no faster than the existing
one-second cadence. Keep local video metrics in the controller. The viewer can
combine them with `loadServerMetrics()`; the controller must not depend on Tauri
IPC or benchmark scenario policy.

Every public asynchronous operation returns a typed outcome or settles on
supersession/disposal. A seek distinguishes `presented`, `deduplicated`,
`superseded`, `unavailable`, and `failed`; deduplication is not fabricated
presentation. Observer/action IDs travel as optional correlation data through a
bounded event sink. Scenario execution remains outside the controller.

### Seek, presentation, and lifecycle

Preserve the existing frame-grid projection and clip bounds: preview intent can
reach an end boundary, but dispatch targets the last valid included frame. Carry
requested preview and dispatched frame tick separately. Keep one native seek in
flight, one latest pending request, the 100 ms throttle, half-frame dedupe, and
one current presentation waiter. New intent supersedes old completion ownership;
it does not start parallel `currentTime` assignments.

`seeked` records a qualifying browser observation and releases the native slot.
When another request is pending, dispatch that request under the existing throttle
and cancel the older presentation waiter. When it is the latest request, wait for
a qualifying RVFC frame. Retain the 1,500 ms native deadline and add a distinct
1,500 ms presentation deadline after native completion. Bound the paused nudge by
750 ms and always restore the current effective user rate
after it. A nudge is internal behavior, never a seventh selectable speed.

RVFC callbacks, scheduled jobs, async play completions, and diagnostic replies
capture media generation and seek/rate operation tokens when armed. Do not attach
the latest token to a callback originating from earlier work. On open/recovery,
cancel callbacks and pending operations before changing source/load state; await
the new load/metadata readiness barrier before accepting current-source
observations. Native DOM events themselves have no generation field: validate
source/readiness plus active operation and target, and never let `seeked` alone
publish presentation. Capture and check tokens around every await.

If RVFC is unavailable, expose approximate currentTime for display and ordinary
playback diagnostics; explicitly return unavailable for authoritative frame
completion. Do not emit authoritative benchmark success from animation frames.
Clip endpoint frame addressing and export continue to use the validated grid.

Maintain desired play state separately from pauses used internally by a seek,
nudge, or recovery. A direct user play action calls `video.play()` synchronously
before the first await to preserve user activation. `NotAllowedError` exposes a
play action; an intentional `AbortError` is not a decoder error. Only a native
media error or a defined timeout invokes recovery.

Automatic recovery is limited to two attempts within ten seconds and two
consecutive unsuccessful attempts. Reset the consecutive budget only after ten
seconds of healthy advancing playback, not merely metadata. Each reload has a
five-second readiness deadline, uses the same DOM element, increments generation,
and cancels older listeners/jobs/operation outcomes. Restore the latest explicit
target when there is pending intent, otherwise the last presented frame, then
restore selected/effective rate, volume, mute, and desired play state. If play
needs another activation, expose that fact without a reload loop. Exhaustion
enters a stable degraded state with manual retry; retry starts a new bounded
episode. Navigation/disposal releases every owned handle and ignores late work.

### Clip and layout integration

The viewer owns clip selection and editing interactions; it passes the validated
half-open loop range to the controller. The controller handles crossing that end
and `ended` through one coalesced loop seek. Automatic loop seeks cannot replace
a newer explicit endpoint/navigation request. Rate observation windows restart at
each intentional timeline discontinuity. A loop or backward seek is not a
monotonic-playback failure.

Keep the primary video outside layout conditionals. Add a shared
`PlaybackControls.tsx` used by the two control bars for rate and basic sound. It
receives state and callbacks, never the video or controller ownership. Retain the
existing fullscreen, keyboard, scrub, and endpoint interactions. General replay
frame stepping belongs to QB-REPLAY-015 and is not added here.
Keep the new controls keyboard-accessible; focused speed/volume controls consume
their editing keys and remain visible while being used in fullscreen.
Descriptive match state follows presented ticks; drag previews and pending seek
cursors remain visibly responsive using requested ticks. Export receives only
the existing validated `ClipDraft` interval.

### Rate ladder, failure, and sound

The only selectable rates are `0.25`, `0.5`, `1`, `2`, `4`, and `8`. Start a new
viewer at 1x; no persisted settings/migration is needed. Within the viewer, retain
selection through fullscreen, clip mode, and recovery. Keep these separate:

- selected rate: user intent;
- applied rate: accepted native property value;
- effective rate: measured presented-time advancement over a valid wall interval;
- outcome: pending, verified, limited, unsupported, or suspended measurement.

Set the native rate, catch assignment errors, observe ratechange, and verify
advancement. Rate requests have their own monotonically increasing token so
rapid changes and a completing nudge cannot restore an obsolete value. While
paused, selection is applied but remains unverified until real playback resumes.

Use one bounded watchdog, with a two-second settling allowance and three-second
continuous active-playback windows. At least three accepted RVFC samples are
needed. Measure delta replay ticks divided by delta monotonic callback time.
For a verified rate, require a positive monotonic media delta and agreement within
the larger of 10% of requested advancement or two source-frame periods. A slower
positive result is limited; zero advancement for 1,500 ms during eligible active
playback is a stall. Invalid/missing RVFC is unknown, never success.

Pause rate measurement across seeks, nudges, loops, end-of-media, hidden documents,
and loading. Restart at a new first frame. Do not misclassify near-end windows or
clip loops as failed rates. A buffering/waiting state may suspend rate judgement,
but five seconds without readiness/progress enters bounded recovery rather than
suspending forever. Ordinary supported 1x playback must not acquire extra seeks.

On rejected, stalled, or persistently limited selection, record the limitation
and apply the most recent verified usable rate for this media, otherwise 1x. Keep
the requested selection visible with explicit text such as `8x unavailable;
playing at 1x`. Changing effective rate must be visible; it is not successful 8x.
If applying the fallback does not recover advancement, use the same bounded reload
policy once and retain the rate limitation through that recovery. Do not
automatically reapply a known failed rate after reload. A deliberate new selection
or manual retry may start a new capability attempt. If even fallback cannot
advance, pause in degraded state. Never emulate 8x with repeated seeks.

For acceptance, selection persistence means retained requested UI intent;
successful playback at that rate additionally requires matching presentation
evidence. A lower fallback advancing while 8x remains selected cannot satisfy an
8x-success claim. The feature's unsupported-behavior allowance covers a visibly
identified limitation and finite recovery, with its own explicit disposition.
It does not turn that rate's failed capability window into a pass.

Expose mute and a labelled 0-to-100% volume slider in both layouts. Preserve user
mute and volume independently of browser rate behavior and internal nudges.
An HTML `muted: false` observation does not prove audible output. Keep audio state
as user-muted, zero-volume, available-but-unverified, runtime-limited, or unknown;
only claim a verified audible/muted capability for the measured WebView and media
configuration. Retain pitch preservation unless existing runtime behavior proves
it unavailable. No WebAudio graph or second audio clock is needed. Production
checks use deterministic audible fixture signals at all rates; generic browser
documentation does not establish WebView2's audio thresholds.

### Decoder identity and canonical media

Use the native WebView2 DevTools protocol receiver through Tauri's Windows
`with_webview` access to observe the current player's media properties. Add a
small Windows-specific `app/src-tauri/src/playback_diagnostics.rs` with an inert
unavailable result on unsupported platforms. Register only the fixed internal
commands/events it needs; do not expose arbitrary DevTools methods or a remote
debugging port.

The installed stack is Tauri 2.11.5, tauri-runtime-wry 2.11.4, wry 0.55.1, and
webview2-com 0.38.2. `WebviewWindow::with_webview` supplies native Windows access;
the CoreWebView2 object exposes `CallDevToolsProtocolMethod` and
`GetDevToolsProtocolEventReceiver`. Add direct Windows-target dependencies for
the native types/callback wrappers actually imported, matching the existing lock;
do not rely on transitive crate visibility. Register and remove COM handlers on
the WebView thread, retain receiver/event tokens for that lifetime, and send only
bounded parsed data across threads. Reuse Tauri's native access without adding a
second window or COM apartment.

Subscribe to media player creation, load events, properties, and errors. A benchmark launch
must await `Media.enable` before opening its target; normal opening proceeds
without waiting and uses active-player discovery if subscription becomes ready
later. Unobserved playback is unknown. The Media domain is experimental: record
the actual WebView/browser protocol version, handle the installed runtime's
creation-event shape, and report unknown when properties or association are
unavailable. Give the primary video a stable DOM marker and append one opaque
UUID query value, `qb_playback_session`, to its existing loopback URL at each open
or recovery generation. Register the tuple of token, mediaId, and generation with
the diagnostic owner. The server currently routes by path and ignores query
values; add a range-response test proving the token changes no media bytes, path
validation, or route class. It is never a filesystem path or media authorization.

Associate a CDP player only when its load URL contains the exact current token and
expected origin/path. `Media.playerEventsAdded` carries Chromium's `kLoad` URL;
validate the installed runtime's event representation. Where `playerCreated`
includes a DOM node ID, cross-check the primary marker as well. The document
`kFrameUrl` alone is not the video URL. Exclude HEVC-probe/music/exporter players,
old tokens, and ambiguous evidence. Rotate the token and invalidate evidence on
recovery. Keep the URL small enough to avoid media-log URL truncation, sanitize
tokens/paths in reports, and use the same association mechanism in both arms.

Maintain at most eight candidate players, 64 retained properties per player,
bounded strings/events, and a 64 KiB total pending diagnostic budget. Use a
five-second acquisition deadline. Overflow, unsupported protocol, timeout, and
failed association are explicit outcomes. COM callbacks enqueue bounded data;
they do not wait on filesystem, process/GPU counters, or frontend work. Remove
event tokens and disable the domain at session close/disposal. Ordinary playback
may continue when diagnostics are unavailable.

Expose a compact per-media result: expected canonical codec/profile/pixel format,
observed decoder name/platform-decoder flag, observation source/runtime, and
`hardware-confirmed`, `software-fallback`, `unknown`, or `unsupported`. Use an
explicit reviewed decoder-name mapping for the measured runtime: for example,
D3D11VideoDecoder plus the platform-decoder flag and matching player is positive
Windows hardware evidence. A platform flag, Mojo/wrapper name, encoding vendor,
`canPlayType`, MediaCapabilities prediction, or GPU utilization alone is
insufficient. Unknown names remain unknown until their underlying path is proved.
Keep decoder transitions, including a later software fallback, in diagnostics.

Keep the bounded property subscription for the current open media lifetime to
catch later decoder transitions. Normal operation retains only the current
snapshot and a bounded transition history, publishes changes rather than frame
events, and never gates first play on diagnostics. Benchmark validation retains
the transition evidence throughout the observed arm. Its cost belongs in observer
controls. Invalidate a hardware label when association is lost, the media reloads,
or diagnostic events are dropped; an earlier snapshot cannot prove a later path.
Software playback remains available and explicitly labelled as compatibility
fallback; it never joins hardware-normal performance statistics. Do not disable
WebView2 GPU acceleration in normal production or silently force software decode
to satisfy a rate. Record effective launch configuration and runtime/device
identity in verification evidence.

Validate canonical native H.264 High 1080p60 8-bit 4:2:0/AAC MP4 and current H.264
exports against their probed facts. If a canonical fixture unexpectedly uses
software decoding, resolve the diagnosis before claiming the normal path; do not
change the recorder contract in this feature. HEVC gets positive playback arms
only after the production capability probe succeeds; lack of HEVC support is
recorded, not treated as evidence for broader recorder support.

The API feasibility and property meaning come from the official
[WebView2 CDP integration documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol),
[Media protocol](https://chromedevtools.github.io/devtools-protocol/tot/Media/), and
[Chromium media-property definitions](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/media_log_properties.h).
The load-URL binding is defined in Chromium's
[media event contract](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/media_log_events.h).
These references do not prove the installed runtime exposes a particular decoder;
the production acquisition gate below does.

## Rejected alternatives and planning decisions

- Extracting only JSX or a collection of unrelated hooks leaves multiple owners
  of media lifecycle. One controller owns the primary video and time state.
- A generic player backend abstraction adds unsupported implementations and
  obscures native readiness/errors. Inject only the small concrete media/timer
  seam needed to test the browser-backed controller.
- Property assignment, wall-clock extrapolation, or frequent seeks cannot prove
  the selected native rate. Use presentation evidence and visible degradation.
- Browser capability predictions and generic GPU counters cannot prove the
  decoder selected for a player. Use bounded per-player native diagnostics.
- Reusing the historical QB-REPLAY-008 aggregate as a matched before arm would
  combine different time contracts/fixtures and missing decoder proof. Preserve
  it and collect a fresh reference with common instrumentation.
- Keep the feature coherent: rate recovery, presentation ownership, and decoder
  evidence exercise the same lifecycle boundary. Discovery of a recorder codec
  change, native decoder replacement, or wider vendor support is separately owned
  work and does not expand this plan.

## Milestones

### M1: Establish the production reference and diagnostic contract

Before changing playback scheduling, implement the bounded native diagnostic
adapter and versioned benchmark observation changes needed for both arms. Add
only passive observation seams to the existing viewer. Record exact reference
source/binary hashes and preserve the reference executable with its packaged
resources in a dedicated ignored directory. This isolates the controller/rate
change from observer changes without reverting the dirty working tree.

Prove primary-player association and decoder identity with a fresh canonical v2
fixture. Exercise missing/ambiguous/late diagnostic cases in parser tests. Validate
that normal GPU acceleration remains enabled. Prepare the reviewed v2 corpus,
run the four minimal/full observer pairs, then collect the full pre-extraction
matrix and explicit rate-capability failures. Freeze protocol/fixtures/observer
identity for comparison. Missing decoder evidence prevents a hardware-normal
comparison claim; resolve that concrete gate before optimizing the controller.

Concrete tooling edits belong in `app/src/benchmark.ts`,
`app/src-tauri/src/benchmark.rs`, `tools/replay_benchmark/analyze.py`, `matrix.py`,
the manifest/event/report schemas and runners, their tests, and the durable replay
protocol. `analyze.py::compare_reports` already implements `--compare`; extend it
rather than introducing a parallel comparator. It currently skips absent summary
keys. Require coverage of all declared comparison strata and report missing or
ineligible evidence as incomplete, never a passing empty comparison.

### M2: Implement the injected playback controller

Add controller/HTML adapter and deterministic tests, preserving the current viewer
until integration. Reuse the exact time helpers and seek state. Prove open,
readiness, seek scheduling, separate timing facts, token invalidation, play
activation, native/presentation deadlines, finite recovery, nudge restoration,
loop precedence, and complete cleanup through a fake adapter and clock.

### M3: Integrate the persistent viewer at 1x

Move primary media ownership into the controller. Route viewer and benchmark
actions through it. Keep layout/clip policy in their existing owners and project
authoritative versus requested time to the appropriate consumers. Remove the
superseded primary-video listeners/scheduler/metric mutation code. Run production
1x open/seek/scrub/loop/fullscreen/recovery scenarios before adding user rate UI.
Require persistent element identity and no new ordinary-playback recovery/seek
amplification. Document the durable ownership change.

Benchmark scenario code must use controller commands, snapshots, and the event
sink exclusively for media access. It must not retain a video reference or read
or write primary currentTime, playbackRate, play/pause, source, or quality directly.
The viewer can serialize controller observations into the benchmark schema.

### M4: Expose rates, audio, and truthful limitations

Add shared rate/mute/volume controls and the rate watchdog/outcomes. Exercise all
six selections, paused selection, rapid selection during a nudge, recovery,
fullscreen, short clip loops, end-of-file, buffering, and unavailable RVFC. Show
native decoder and rate limitations in the existing diagnostic/error surfaces.
Keep software/unknown decoder lanes out of normal performance statistics.

### M5: Verify the completed production tree and record dispositions

Run applicable automated checks, freshly build the production application, and
run the matching after matrix plus the manual audiovisual checks. Revalidate
observer overhead for any instrumentation changes since M1. Analyze matching
strata, inspect the transition-heavy layout arm and ten-minute torture, and
resolve every regression or explicit rate limitation. Update current durable
architecture, the execution checkpoint, and concise feature evidence. The
completion gate remains every criterion in `feature-list.json`; unavailable
required evidence is not a pass.

## Verification design

### Deterministic checks

Use Vitest fake time plus a controllable adapter that can deliver old callbacks,
reject play/rate/seek operations, omit metadata/RVFC, and emit out-of-order events.
Test observable outcomes and ownership bounds, not private implementation layout:

- no more than one native seek and one replaceable pending request; half-frame
  dedupe and rational/nonzero-origin target conversion;
- seeked without presentation, presentation before seeked, superseded targets,
  disposal/navigation/recovery during pending work, and exactly-once outcomes;
- each timeout, two-attempt recovery exhaustion, slow repeated failures, a denied
  user-activation play, intentional AbortError, and callbacks after disposal;
- selected/applied/effective rate distinction, slow positive advancement, a frozen
  8x sample, rapid changes, rate reset on reload, nudge restoration, and audio
  preference preservation;
- loop/endpoint precedence, final frame handling, no-RVFC approximate display, and
  zero listeners/timers/RVFC callbacks after cleanup;
- diagnostic association, contradictory hardware properties, software fallback,
  unknown names, protocol absence, overflow, stale sessions, and parser failures;
- benchmark event/version validation, invalid timing authority, rate limitations
  excluded from successful rate statistics, and unmatched comparison rejection.

Run these commands from the root against the final tree:

```powershell
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
python -m unittest discover -s tools/replay_benchmark/tests -v
```

Also run the replay-time/shared-runtime tests when their consumers or contracts
change. Follow `docs/development/VERIFICATION.md` for packaged-runtime preparation,
production staging, relevant broader checks, and canonical schema/linked-file
validation. Recorder product code is outside this feature; an unexpected required
recorder change must be scoped explicitly before it is made. `git diff --check`
must not introduce whitespace errors; preserve unrelated pre-existing changes.

### Production fixtures and matrix

Use a fresh sentinel at
`build/perf/qb-replay-009/.chronobreak-replay-benchmark`. Generate or explicitly
copy only dedicated fixtures. Build schema-v2 bundles with
`tools/replay_benchmark/build_corpus.py` and `schemas/corpus-v2.schema.json`, then
prepare with packaged full single-thread decode and unchanged source hashes.
Include short native H.264 (2-5 minutes), representative native H.264 (20-35
minutes), long H.264 (at least 45 minutes, labelled derived when applicable),
current exported H.264, a nonzero-origin timing fixture, and conditional HEVC.
The retired external recorder is not a required positive media class. Negative
media copies belong in separate sentinel scenarios.

Concrete existing entry points, with reviewed absolute paths substituted for the
parameters, are:

```powershell
python tools/replay_benchmark/build_corpus.py --spec <corpus-v2-spec> --media-runtime-root <packaged-runtime> --preflight-only
python tools/replay_benchmark/build_corpus.py --spec <corpus-v2-spec> --media-runtime-root <packaged-runtime>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <prepare-spec> -Manifest <template-manifest> -MediaRuntimeRoot <packaged-runtime> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/prepare.ps1 -Spec <prepare-spec> -Manifest <template-manifest> -MediaRuntimeRoot <packaged-runtime>
npm run desktop:build --prefix app
python tools/replay_benchmark/matrix.py plan --template <template-manifest> --spec <matrix-spec>
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <ordered-launch-manifest> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <ordered-launch-manifest>
python tools/replay_benchmark/matrix.py verify --plan <matrix-plan> --require-results
python tools/replay_benchmark/analyze.py --input <immutable-run-root> --output-dir <fresh-report-directory>
python tools/replay_benchmark/analyze.py --input <after-run-root> --output-dir <after-report-directory> --compare <before-report-json>
```

Repeat `--input` for every explicitly reviewed immutable run root in an arm. Use
`--observer-control <minimal-report-json>` for the matching full observer report.
Comparator eligibility requires equal trial counts in the two arms as well as
the minimum evidence counts; single-run commands above illustrate CLI entry
points, not sufficient evidence for the complete comparison.

Use all ten scenario classes in `docs/performance/replay-benchmark.md`: idle,
cold/warm open, play/pause, rates, seeks, scrubs, layouts, lifecycle/torture, and
exports. Preserve prescribed durations, five-trial minima for comparable arms,
40 observations per near/far-forward/backward seek stratum, 20 reopen cycles,
the 600-second torture arm, and independent export cases. Rate capability uses
all six 30-second windows and the rapid 0.25x/8x sequence; a failure must not
silently eliminate the remaining rates. Separate fresh capability runs can
continue after a failed process, with every failure retained.

Map controller events into the benchmark boundary once. Introduce schema-v2
launch/event/report handling for this comparison with the explicit
`event_contract_id: qb-replay-009-v1`. Retain the QB-REPLAY-008 benchmark family
identifier and its versioned comparison rules. Keep a version-dispatched v1
reader for historical benchmark evidence; this is not a schema-v1 recording
reader. Recording bundles remain strictly v2. M1's reference and M5's after arm
must both use the same new observer contract.

Add explicit replay ticks, media identity, seek epoch, rate outcome, and decoder
association beside observer milliseconds. Preserve the meaning of old `_ms`
fields. Include event contract and schema version as explicit comparison identity
requirements in `_comparison_identity` and `compare_reports`, outside the
implementation-subject fields intentionally excluded for before/after matching.
Reject cross-contract reports and missing required metrics instead of quietly
comparing only their intersection. Keep the historical report immutable.

Declare `scenario.purpose` as `performance`, `capability`, or `fault` in the new
manifest schema. Performance trials keep the existing fatal-error and full
success gates. Capability trials record one explicit outcome for every declared
rate, including assignment rejection, limited advancement, stall, supported
advancement, and subsequent recovery/return behavior. A capability trial can
complete its bounded evidence collection with an unsupported rate, but contributes
no successful performance samples for that rate. A missing outcome, lost event,
unknown media identity, or unfinished collection is still invalid. Fault trials
declare their exact injected failure/expected terminal behavior and remain outside
normal statistics. Do not add a general `allow_errors` switch. Extend both matrix
verification and analysis for these purposes; preserve every failed root and
emit separate capability dispositions alongside the positive comparison.

In the real production WebView, verify each layout's controls, audible fixture
signals at each rate, recovery target/play/audio restoration, clip loop boundaries,
nonzero PTS origin, and transitions while paused/seeking. Prove the same primary
DOM node survives layout changes. Record observed sound and effective speed, not
only assigned properties. Use scoped diagnostics and reviewed generated media;
never destructive tests on user recordings.

Retain manifest/spec/protocol identities, binary/resource/source hashes, fixture
hashes, runtime/OS/driver/display identity, decoder association and transitions,
all event and loss counters, original failed roots, exact commands, and explicit
manual observations under the ignored sentinel. Commit only sanitized reports to
`docs/performance/evidence/` and concise canonical evidence to the feature entry.

## Performance and reliability gates

The versioned QB-REPLAY-008 rules remain the comparison authority:

- Four interleaved 60-second minimal/full observer pairs must pass before full
  measurement: equivalent semantic outcomes, zero required loss, median latency
  overhead at most 5%, p95 at most 10%, memory increase at most 16 MiB, and CPU
  increase at most max(2%, one accounting quantum per process). Preserve the
  protocol's MAD/resolution and count/byte/drop gates as well.
- Compare at least five identity-matched trials per arm. P95/P99 need at least
  40 observations in the exact stratum. The repeatability band is
  `max(resolution floor, 3 * baseline MAD)`, with floors of 5 ms latency,
  0.1 normalized CPU percentage point, 8 MiB memory, and 1% for byte/count metrics.
  An unfavorable change requires disposition when it exceeds both the band and
  5%. Do not turn an unpaired historical comparison into a passing result.
- New media errors, timeouts, unexpected recoveries, stale updates, source
  mutation, required loss, seek amplification, and sustained resource growth
  always need disposition. An expected negative scenario does not waive a new
  failure in the positive matrix.
- Preserve bounded memory independent of recording size, one persistent decoder
  element, bounded event/request buffers, one active plus latest native seek,
  finite shutdown, and released media/diagnostic resources on reopen/dispose.
- Normal-path performance evidence requires positive actual hardware decoder
  association for the canonical fixture and no observed transition to software.
  Unknown/unsupported/software runs remain useful separate diagnostic evidence;
  they cannot satisfy this gate.
- An 8x limitation can be explicitly characterized with finite visible fallback
  and successful return to supported playback. It cannot be called a passing 8x
  performance window. If an observed limitation contradicts a feature acceptance
  criterion beyond its explicit unsupported-behavior allowance, keep the feature
  non-done and resolve the requirement explicitly rather than weakening it.

Challenge the plan at review for controller ownership, stale-event attribution,
seek/presentation ordering, activation-sensitive play, failed-rate restoration,
decoder association, benchmark comparability, bounded observer overhead,
resource cleanup, and scope. If implementation contradicts this design
materially, preserve this file and follow the versioned superseding-plan workflow.
