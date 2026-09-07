# QB-REPLAY-009: Playback boundary and speed ladder (v2)

This version supersedes `docs/exec-plans/qb-replay-009-playback-boundary-and-speed-ladder.md`
for the user-requested proportionality correction. Preserve that file as immutable
history. This plan is self-contained; the superseded plan is not a prerequisite read.

## Purpose

Give the replay viewer one concrete owner for browser playback and expose the six
replay speeds in both layouts. A user can seek, pause, change speed, adjust sound,
and recover a failed preview without losing the replay position or replacing the
video element. Report what the WebView actually presents, including a rate it
cannot sustain and a decoder that falls back to software.

`feature-list.json` owns acceptance. `docs/execution/qb-replay-009.md` owns execution
facts. This document defines implementation design and a targeted controller
validation suite. Accepted QB-REPLAY-008 and QB-REPLAY-012 evidence is authoritative
input, including its limitations; do not reopen or reproduce it by default.

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
the focused verification needed to decide whether these changes meet their contract. Keep one HTMLVideoElement-backed
implementation and a small injected adapter for deterministic tests.

Do not introduce a backend registry, MSE, libmpv, WebCodecs, native playback,
additional recording stems, an export strategy, ReplayIndex, schema migration,
recorder GOP/profile tuning, or a second video for fullscreen. Do not migrate the
clip editor's backdrop/music synchronization. Preserve dirty working-tree changes
from the replay-time and recorder work.

A complete QB-REPLAY-008 baseline rerun, a broad benchmark corpus rebuild, new
benchmark schema/event/report versions, comparator redevelopment, observer-control
campaigns, export benchmarking, and long-duration torture matrices are outside the
default scope. Reuse existing fixtures, checks, and observation seams. Broader
measurement or infrastructure changes require a concrete material finding from
the targeted suite that could change implementation or architecture, under the
escalation and stopping rule below.

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
- That accepted report and QB-REPLAY-012's exact-time evidence remain authoritative.
  They are not numerical matched before/after measurements of this extraction,
  and do not need to become such measurements. The ten-minute torture result had
  no sustained growth flag; accept it without repeating the soak. The short layout
  arm's growth flag is a focused transition/cleanup comparison target, not a proven
  leak or a reason to rebuild the broad baseline.
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
- observed (effective) rate: measured presented-time advancement over a valid wall interval;
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
checks listen to a known audible section of an existing dedicated fixture at the
supported rates and record the failed-rate/fallback audio behavior. Do not build an
audio corpus. Generic browser documentation does not establish WebView2's audio
thresholds.

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

Subscribe to media player creation, load events, properties, and errors. The
targeted decoder check awaits `Media.enable` within the acquisition deadline before
opening its target; normal opening proceeds without waiting and uses active-player
discovery if subscription becomes ready later. Unobserved playback is unknown. The Media domain is experimental: record
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
tokens/paths in evidence. Pre-extraction playback need not gain this new diagnostic
implementation merely to create a matched observer arm.

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
events, and never gates first play on diagnostics. Targeted validation retains
decoder observations and transitions through the exercised open/recovery/dispose
sequence. Check its cost with the same focused resource observations as the
controller; add an isolated observer-cost check only if those observations give a
material reason to suspect instrumentation. Invalidate a hardware label when association is lost, the media reloads,
or diagnostic events are dropped; an earlier snapshot cannot prove a later path.
Software playback remains available and explicitly labelled as compatibility
fallback; it never joins hardware-normal performance statistics. Do not disable
WebView2 GPU acceleration in normal production or silently force software decode
to satisfy a rate. Record effective launch configuration and runtime/device
identity in verification evidence.

Validate the expected path on one existing validated canonical native H.264 High
1080p60 8-bit 4:2:0/AAC MP4 fixture. Reuse an existing current H.264 export for the
ordinary clip playback smoke when applicable; do not run export performance arms.
If canonical playback unexpectedly uses software decoding, resolve that focused
diagnosis before claiming the required normal path; do not change the recorder
contract here. Unknown diagnostics must stay unknown. Perfecting telemetry is not
a separate completion goal, but missing evidence needed to establish the required
decoder path cannot be declared a pass. HEVC support was unavailable in the
accepted environment; do not rebuild or repeat that capability investigation.
Add a focused HEVC check only if the target runtime now reports support and the
changed controller actually exercises that path.

The API feasibility and property meaning come from the official
[WebView2 CDP integration documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/chromium-devtools-protocol),
[Media protocol](https://chromedevtools.github.io/devtools-protocol/tot/Media/), and
[Chromium media-property definitions](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/media_log_properties.h).
The load-URL binding is defined in Chromium's
[media event contract](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/base/media_log_events.h).
These references do not prove the installed runtime exposes a particular decoder;
the focused production decoder check below does.

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
- Treating this extraction as a second QB-REPLAY-008 campaign would delay the
  playback contract without resolving a new decision. Accept 008/012 evidence as
  input, compare only changed controller behavior with a small current before
  observation, and verify new UI/diagnostics on the completed implementation.
  Do not claim a cross-version numerical comparison with the historical report.
- Keep the feature coherent: rate recovery, presentation ownership, and decoder
  evidence exercise the same lifecycle boundary. Discovery of a recorder codec
  change, native decoder replacement, or wider vendor support is separately owned
  work and does not expand this plan.

## Milestones

### M1: Capture the small controller reference

Use one existing validated canonical schema-v2 H.264 fixture in a dedicated test
library. Record the current source/binary, fixture, and WebView/runtime identities
and run the shared-behavior rows of the targeted suite below once before extracting
the controller. A new production build, when needed, uses
`npm run desktop:build --prefix app`. Preserve the concise observations and,
when useful for a focused repeat, a copy of that binary and its packaged resources
under ignored `build/perf/qb-replay-009/`; never reset the dirty tree to recreate it.

Reuse existing benchmark actions/counters where they already answer the question,
or record the procedure and observations directly. Missing automated report
support is not a reason to add a schema or comparator. Do not implement diagnostics
in the old viewer first, build a corpus, run observer pairs, or reproduce the known
8x stall. The accepted 008 capability findings are the before context for rate
limitations. New controls, finite rate fallback, and decoder diagnostics receive
their own after checks.

Deliverable: a small case/result table identifying the comparison fixture,
observations, and any actual uncertainty. Then proceed to M2. An unusable required
observation calls for the smallest repair or substitute that can answer that case,
not a preliminary benchmark project.

### M2: Implement the injected playback controller

Add `playbackController.ts`, `htmlVideoPlaybackAdapter.ts`, and focused tests while
preserving the existing viewer until integration. Reuse the exact time helpers
and seek state. Prove open/readiness, scheduling, distinct timing facts, token
invalidation, play activation, native/presentation deadlines, finite recovery,
nudge restoration, loop precedence, and cleanup with a fake adapter and clock.

### M3: Integrate primary playback at 1x and bounded decoder diagnostics

Move primary media ownership into the controller. Route viewer and existing
benchmark actions through its commands, snapshots, and bounded event sink; they
must not retain a video reference or directly access primary currentTime, rate,
play/pause, source, or quality. Preserve external benchmark field meanings.
Routing existing actions through the new owner is integration work, not a mandate
to upgrade the benchmark data model.

Keep layout/clip policy in its current owners and project requested versus
presented time to the appropriate consumers. Remove superseded media listeners,
scheduler, and metric mutations. Implement the bounded Windows diagnostic adapter,
generation association, parser/lifetime tests, and inert unsupported-platform
result described above. Target `app/src-tauri/src/playback_diagnostics.rs`,
Windows-specific dependencies, `lib.rs` registration, and the range-response test
only as needed for that product diagnostic.

Run focused production 1x seek/scrub/presentation, loop, fullscreen, recovery, and
decoder checks. Require persistent element identity, released resources, and no
new ordinary-playback seek/recovery amplification. Diagnostics are not a gate
before controller implementation. Document the durable ownership change.

### M4: Expose rates, audio, and truthful limitations

Add shared rate/mute/volume controls and the rate watchdog/outcomes. Exercise all
six selections, paused selection, rapid changes during a nudge, recovery,
fullscreen, short loops, end-of-file, buffering, and unavailable RVFC. Show decoder
and rate limitations in existing diagnostic/error surfaces. Measure selected,
applied, and observed rate separately. An explicitly limited 8x selection with
finite return to supported playback satisfies limitation handling, never 8x
capability success. Do not reapply failed 8x automatically or emulate it with seeks.

### M5: Make the completion decision and stop

Run applicable automated checks and the targeted after suite in the production
WebView. Compare shared behavior with M1, use accepted 008/012 facts as context,
and record separate outcomes for new behavior and known runtime limitations.
Investigate only material findings under the escalation rule below. Update durable
architecture, execution evidence, and canonical status when all criteria pass.
Apply the stopping rule; a complete baseline, better reports, or extra fixtures
are not finishing tasks.

## Verification design

### Deterministic checks and applicable project checks

Use Vitest fake time and an injected adapter to deliver late callbacks, reject
play/rate/seek operations, omit metadata/RVFC, and reorder events. Verify observable
outcomes and resource bounds:

- one native seek plus one replaceable pending request, half-frame dedupe,
  rational/nonzero-origin dispatch conversion, and explicit target/loop precedence;
- requested/dispatched/seeked/presented distinction, seeked without presentation,
  presentation before seeked, stale generations/epochs, and exactly-once outcomes;
- native and presentation deadlines, nudge cancellation/restoration, bounded
  recovery/exhaustion, denied user activation, intentional AbortError, and disposal;
- selected/applied/observed rate, slow or frozen 8x, rapid changes, paused selection,
  reload persistence without retrying a failed rate, and audio preference retention;
- no-RVFC approximate display without fabricated frame authority, loop/end handling,
  and no owned listener/timer/RVFC handle after disposal;
- decoder association, software/unknown/contradictory properties, missing protocol,
  overflow, stale sessions, bounded acquisition, and native subscription cleanup.

Keep existing replay-time tests as regression coverage; do not re-prove
QB-REPLAY-012's recording/time architecture or reproduce its media campaign.
Run these commands against the completed app changes:

```powershell
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run desktop:build --prefix app
```

Follow `docs/development/VERIFICATION.md` for the applicable packaged-runtime,
viewer/clip/export-playback smoke, and canonical schema/link checks. Keep those
integration checks brief and scoped to the affected app flow; an export-playback
smoke is not an export benchmark matrix. Run
`python -m unittest discover -s tools/replay_benchmark/tests -v` only if benchmark
tooling is changed; its expansion is conditional below. Shared-runtime/replay-time
contract changes require their corresponding checks and explicit scope review.
Recorder code and its validation campaign are outside this feature.
Check whitespace without overwriting unrelated working-tree changes.

### Targeted production before/after suite

The default is one bounded before pass and one after pass on the same existing
validated schema-v2 canonical H.264 fixture, machine, runtime, layout configuration,
and available observation method. Record intentional binary/source differences
and any environment mismatch; never manufacture a matched comparison. Use an
existing audible section and known seek/frame targets. Existing nonzero-origin
fixture coverage or deterministic conversion tests suffice unless changed browser
conversion behavior leaves a concrete production question.

No short/medium/long/export/codec corpus, minimum five-trial matrix, p95/p99 sample
quota, full decoder sweep, four observer pairs, or ten-minute torture is required.
Do not generate/copy new media unless an essential changed behavior lacks an
existing safe fixture; then create only that smallest missing fixture. Faults use
dedicated disposable copies or adapter injection, never user recordings.

| Case | Bounded procedure | Decision evidence |
| --- | --- | --- |
| Seek, scrub, presentation (before/after) | Open, play/pause, seek to known early/middle/late targets in both directions while paused and playing; issue one rapid scrub burst then release. | Latest target wins; requested, dispatched, seeked, and presented facts stay distinct; frame-driven state follows RVFC; native requests remain bounded; no new visible delay, stale update, error, or seek amplification. Compare request counts and request-to-first-presentation timing when available; do not infer presentation from currentTime. |
| Clip loops and layouts (before/after where affected) | Exercise one short half-open loop, endpoint adjustment, paused and playing fullscreen entry/exit, and a layout transition during a pending seek. | Same primary DOM element/decoder lifetime, correct loop boundary and explicit-seek precedence, no lost position/play/audio intent or duplicated listeners. New rate selection survives these transitions in the after pass. |
| Rates and audio (after, with accepted 008 capability context) | Select 0.25x, 0.5x, 1x, 2x, 4x, and 8x; allow a settling interval and one qualifying observation window per supported rate. Check paused selection, one rapid transition sequence, layout/recovery persistence, mute and volume, and return from failed 8x. | Record selected/applied/observed/outcome and audible behavior. Supported rates advance within the design tolerance. Known 8x may stall/be limited: the visible diagnosis and finite fallback/return must work, with no fake seeks, reload loop, or success claim. Do not repeat failed 8x just to characterize it again; later selections continue independently after bounded recovery. |
| Recovery and lifetime (before for existing behavior, after for the new contract) | Use one controlled recoverable interruption on disposable media and one terminal/unavailable case. Adapter tests cover repeated slow failures and rare event orderings. Dispose/reopen with work pending. | Same-element reload, new generation, correct target/preferences, bounded attempt/readiness/presentation deadlines, explicit manual retry when exhausted, no late mutation after navigation. Injected failures stay separate from normal playback outcomes. |
| Expected decoder path (after) | Observe the associated canonical player at open, through exercised rate/layout/recovery changes, and disposal. Record effective GPU-enabled configuration. | Expected codec/profile and actual per-player decoder result; distinguish hardware-confirmed, software-fallback, unknown, unsupported, and stale evidence. Generic GPU activity or assigned rate proves neither decoding nor successful playback. No broad decoder/vendor survey or replay of 008's unavailable GPU collector. |
| Focused resources and regressions (before/after) | After warm-up, observe 60 seconds of ordinary 1x playback, then two equal small blocks of five viewer open/close cycles and ten layout transitions, with the seek/scrub actions above. Allow 30 seconds of settling after each block. Sample existing process-tree CPU/private memory/working set/handles/threads, video drops, server request counts, and available owned-handle diagnostics. | No new steady-playback seeks/recoveries; bounded outstanding work; no retained listener/timer/subscription growth; no persistent increase across equal transition blocks or material worsening of stalls/drops/seek responsiveness relative to the before observations. The historical layout flag is a review target, not an assumed new leak. |

Use existing sentinel benchmark actions/counters only where they support these
cases without infrastructure expansion. When suitable, the existing runner entry
is:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <targeted-existing-schema-manifest> -PreflightOnly
powershell -NoProfile -ExecutionPolicy Bypass -File tools/replay_benchmark/run.ps1 -Manifest <targeted-existing-schema-manifest>
```

These commands are optional collection aids, not a requirement to build a manifest
matrix. A direct documented production procedure plus existing logs/metrics is
valid evidence when it answers the same case. If an existing verifier rejects a
known unsupported rate or lacks a new diagnostic field, retain the raw outcome
and explicit disposition; do not relabel the run as passing or rebuild that
verifier merely to obtain a green report. Missing essential observations still
need a focused substitute; missing report formatting does not.

### Evidence shape and material findings

Keep a concise case table in the checkpoint or a linked sanitized report: case,
before observation or accepted input reference, after observation, result
(`passed`, `failed`, `environment-blocked`, `not run`), and disposition. Record
commands/manual steps, source/binary and fixture identity, runtime/device identity,
observed timings/counts/resources needed for the decision, rate outcomes, decoder
association, and relevant failures. Raw generated observations can remain in ignored
`build/perf/qb-replay-009/` under the existing sentinel ownership convention.
Do not require a new report schema, corpus spec, event contract, or archive format.

A failed capability result can coexist with passing limitation handling; keep the
two conclusions explicit. A before pass is a focused regression reference, not a
statistical performance certification or a replacement QB-REPLAY-008 baseline.
Compare only shared, adequately observed behavior; absent pre-change diagnostic
fields do not make the whole before reference ineligible.

## Performance and reliability gates

The functional bounds above are hard gates: correct presentation attribution,
one persistent element, one active plus latest seek, finite recovery, bounded
callbacks/buffers, and cleanup on dispose. No new normal-playback errors, stale
state, recovery loops, seek amplification, or sustained resource growth may remain
unresolved. Preserve bounded memory independent of recording size and normal
WebView GPU acceleration. Positive hardware-normal claims require actual associated
decoder evidence; software and unknown observations stay separately diagnosed.

The focused comparison is a regression decision, not a new absolute performance
budget. Assess latency/drops/CPU/memory against the matching before observations,
counter resolution, warm-up, and the two equal transition blocks. Any functional
contract breach is material. A new visible stall, repeated responsiveness loss,
persistent resource growth after settling, or unexpected decoder fallback is a
material trigger even without a percentile report. An isolated noisy sample is
not a proven regression: repeat only that case with the same conditions when it
could change the conclusion. Do not hide a persistent trend as noise.

### Escalation and stopping rule

1. When a targeted result is material or leaves architectural uncertainty, record
   the observation, the concrete implementation/architecture decision it could
   change, and the smallest additional check that can resolve it.
2. Repeat or extend only the affected scenario first. Add a fixture, observation,
   or benchmark-tool change only when existing evidence cannot answer that decision.
   A full QB-REPLAY-008 rerun or broad infrastructure/corpus work is exceptional:
   it requires a recorded reason that narrower checks are insufficient and the
   result could change implementation or architecture. Material design changes
   follow the versioned superseding-plan workflow.
3. Once the implementation satisfies its functional contract and targeted
   validation shows no material regression or unresolved architectural uncertainty,
   stop after the applicable project checks, documentation, and canonical completion
   update. Do not keep measuring to improve confidence without a decision to resolve.
4. Imperfections in benchmark, verifier, or evidence infrastructure are non-blocking
   unless they could invalidate that conclusion. Report them as limitations; repair
   only conclusion-critical gaps. A missing required functional observation, an
   unresolved decoder-path requirement, or an unavailable required check is not a
   pass and cannot be waived by this stopping rule.

The known 8x WebView limitation is not unresolved architectural uncertainty when
the controller truthfully reports it and restores supported playback within the
contract. It does not trigger native playback, frame stepping (QB-REPLAY-015), or
a renewed capability campaign.

Challenge the plan at review for ownership, stale attribution, seek/presentation
ordering, activation-sensitive play, failed-rate restoration, decoder association,
resource bounds, and whether each proposed check can change a decision. Preserve
this plan and use the versioned superseding mechanism for a material contradiction.
