# Desktop library, replay, and clip flow

## Tauri boundary and startup

The ordinary Windows app also ensures/observes an independent sibling recorder.
[Recorder supervision](recorder-supervision.md) defines ownership, session limits,
test isolation and explicit login registration.

`app/src-tauri/src/lib.rs` loads shared configuration, creates synchronized app state, installs built-in music, starts the local playback server, launches asynchronous Data Dragon initialization, runs retention, and registers Tauri commands. Commands expose settings, library scans/mutations, playback probes, HEVC capability state, assets, music, export, and diagnostics to `app/src/api.ts`.

SolidJS `app/src/App.tsx` owns screen navigation, a single `LibraryController` snapshot for games/clips/storage, and independent settings/Data Dragon resources. The frontend does not receive raw multi-gigabyte video bytes through Tauri IPC.

## Benchmark boundary

The Rust replay harness is compiled only with the `replay-benchmark` Cargo feature
and activated by an explicit validated manifest launch. Sentinel-owned roots
isolate its configuration, library, results and WebView profile. Activated runs
constrain retention, network work and user mutations while observing the same
production playback server and export implementation. Normal builds return no
benchmark session to the frontend.

The frontend currently includes benchmark code in its normal bundle. Its nullable
session observer owns bounded telemetry, clock/action/generation identities and
required-event loss accounting. Production-path markers must retain their real
library admission, controller event and authoritative video presentation sources;
an animation-frame paint opportunity cannot replace RVFC presentation evidence.

Previously `App.tsx` interpreted scenarios and owned benchmark readiness,
export workloads, repetitions, cancellation, timers and animation frames. A global
window event joined viewer-cycle completion to App navigation.

`benchmarkApplication.ts` now owns that experiment policy and its resources. App
retains its production `LibraryController`, `HistoryController`, navigation,
settings, capability probing and asset initialization. A bounded typed host exposes
live state, existing origin/admission checks, guarded replay opening, token-bound
descriptor reads, the existing duration drain, production export and source-root
invalidation. It exposes neither controller ownership nor media/player internals.
The coordinator is created when the nullable observer resolves, before library
admission. Its Solid creation effect tracks observer identity only; a separate
effect forwards live state. With no session there is no coordinator or associated
benchmark cancellation/timer/frame work.

An optional synchronous Viewer callback captures the coordinator and outgoing
viewer context at mount. It replaces the global control event at the same boundary:
release media (including the existing delay), emit required cycle completion,
mark the viewer finished, then notify App. The coordinator admits each viewer once
and rejects stale/disposed sessions and viewer contexts. Asynchronous experiment
continuations recheck captured origin/navigation authority; active-context failures
remain explicit. An attempted export always invalidates its captured source root,
even when its session can no longer publish results.

`benchmarkViewer.ts` owns Viewer workload construction/execution, eligibility,
readiness deadlines, action/evidence waiters, cancellation and completion. One
runner is created for the captured active session after the real controller exists
and before it opens media; an ordinary mount creates no runner or experiment
resources. Viewer retains controller/media/data/layout/clip ownership and emits
authoritative descriptor, details, controller, presentation, decoder and fullscreen
observations before forwarding the original evidence to the runner.

Its typed live host exposes focused playback/clip/layout/metrics operations and
read-only render/resize subjects, without transferring a controller or video.
Ordinary and workload seeks share one production operation: apply the current
clip loop, call the actual controller and update the requested preview. Event
selection and frame-aligned endpoint changes also share production helpers;
action IDs, endpoint alternation and evidence/timeout policy belong to the runner.
The resize helper reads opaque primary-element/load identity and subscribes to
real frames; it drains issued native work, restores geometry and unlistens even
after publication is cancelled. The render helper checks live source/element/load
identity, real projected participants and the actual overlay DOM, decodes its
images and waits for two paint opportunities. Fixed workload preconditions stay
in the harness, never synthesized by the screen.

Completion captures real metrics before releasing media, waits the existing
100 ms, emits the required event, marks finished and then synchronously notifies
App or completes the observer. Intentional release remains eligible; unexpected
active disposal fails once, while replaced mounts/sessions cannot publish late
work. A failure claimed before release retains its original reason through
disposal. Viewer separately cleans up its production resources.

Bounded production telemetry and real search-field/history observations remain
at their authoritative sources. Backend feature/session
isolation and frontend orchestration ownership are distinct from bundle isolation;
the frontend harness still ships in the normal bundle to preserve loading timing.
Cycle returns render the actual Games screen without the additional refresh
performed by the ordinary user `openTab` return. Export benchmark scenarios
explicitly measure the production backend command, not the exporter UI workflow.
After shutdown, the runner recognizes a newly created export staging parent only
when its case-exact `.chronobreak-export` directory contains solely a regular,
reparse-free, empty `.lease` that can be opened with exclusive sharing. This is a
sentinel-scoped idle postcondition, not durable proof of parent provenance. The
runner records it separately from the exact MP4/JPEG output-entry set; every
media, source-integrity and shutdown gate still applies. Preexisting staging
keeps its existing preservation semantics, and validation never runs recovery.
Legacy `library_useful` still drains clip durations through the existing bounded
queue, while versioned opening uses snapshot/paint admission. These retained
compatibility seams preserve measurement contracts; they do not imply user-click
or exporter-UI equivalence. The replay benchmark protocol owns workload validity
and comparison rules; feature lifecycle remains in `feature-list.json`.

## Loopback playback server

`app/src-tauri/src/playback_server.rs` binds an OS-assigned port on IPv4 loopback and serves:

- game recordings and clip assets with HTTP byte-range support;
- an embedded HEVC probe clip;
- bundled and explicitly registered imported-music previews;
- versioned Data Dragon images from the local cache, fetching and caching missing assets when allowed.

Path components are validated before resolution. Range responses stream file slices through Tokio and count requests, bytes, completed streams, and cancellations for diagnostics. This keeps memory use independent of whole-file size and lets the webview's native `<video>` element own decode, buffering, seeking, rate, and audio.

All delivery routes share the capability, Host/Origin, exact-port CSP and opened-file
containment policy described in [local playback security](local-playback-security.md).
The HTTP/1 connection bound also bounds concurrent handlers and file streams.
Imported preview tokens are random, independently revocable and released when the
exporter's source changes or its preview is disposed.

## Library and viewer flow

The Games search field coalesces text for 180 ms; clearing it and changing discrete
filters submit immediately with the latest text. `HistoryController` retains the
last checked page while a replacement loads, with an explicit updating state.
Pagination is disabled during text/query work and after query failure so a retained
page's cursor cannot be combined with a failed query's filters. Page and pagination
position publish together only after query-generation and snapshot admission.
A new library origin clears retained history; pending text timers are cancelled on
origin changes or disposal. Origin comparison uses its values, not object identity.
History queries submitted before initial catalog admission remain queued, with
only the latest input retained. The controller releases that input after the
catalog token is committed, preserving its action identity; per-recording imports
then continue independently of foreground queries.

History initialization spans current catalog/import work and a final query that
observes its latest completed import revision. A current foreground query can
satisfy that barrier; stale results cannot. Success and terminal failures both
settle it. The library shows one stable Loading match data status throughout this
session. Settled limited/unavailable coverage is described only for relevant
League-data filters; ordinary browsing retains incomplete recording cards.
History distinguishes a nonterminal `indexIssue.kind: limited` (per-recording
catalog errors or returned unavailable progress) from `failed` (a rejected or
invalid catalog/import operation that stops initialization). Settled failures
warn even without League filters, retaining their diagnostic message internally;
source limitations remain filter-relevant. A fresh accepted context clears both.

Opening a selected game first requests a token-bound `ReplayDescriptor`: snapshot
token, game ID, protected loopback video URL and validated schema-v2 media timeline.
Rust uses the strict bundle reader and requires a nonempty regular video, but does
not construct semantic projections or run media probes. The existing viewer and
controller mount from this descriptor and the selected snapshot's display summary.
Data Dragon, optional clip durations and full replay details do not gate that mount.
A single persistent video element is shared between windowed and fullscreen layouts;
changing layout does not remount the decoder.

After two animation frames provide a paint opportunity, the viewer loads optional
recording facts, resumes bounded index work, then requests source scopes and bounded
event/overlay pages. Pending work includes scheduled viewport reads. Useful
same-source facts and pages remain visible while replacement work runs. A compact
header outside the picture shows loading, limited details or unavailable details;
stopped work never remains labelled loading. Source choices use segment labels.
The replay loading notice appears only after 500 ms of continuous pending work,
then clears as soon as work settles. Controller/benchmark loading is immediate;
limited/unavailable notices are immediate and the display timer cancels on exit.
Raw exceptions and optional-data retry controls are absent from the product UI;
return/refresh/rebuild provide deliberate recovery. Core recording-open failures
retain actionable safe copy. Media, rate, clip range, seek state and fullscreen
remain independent. Benchmark readiness still requires its complete-data contract.

Optional match context arrives with full details and displays its provisional status
and game/queue IDs in the viewer header. It never gates media or supplies final
result/statistics. Existing Live-derived totals are labeled recorded K/D/A. The
TypeScript projection preserves decimal game IDs as strings and discards malformed
or wrong-media optional context locally.

`ReplayTimeline` supplies the same seek, zoom, pan, clustered-event and clip rail
in windowed and fullscreen layouts. Its viewport and exact frame clip draft live
in the persistent playback surface; pending details do not gate basic media
controls. The consolidated HUD and rail stay mounted across fullscreen changes,
including mid-drag. Each rail releases pointer capture and gesture listeners when
a gesture ends or is cancelled, and releases those resources and its resize
observer on unmount.

The position slider is an empty focusable sibling of event and clip buttons;
the surrounding rail owns pointer geometry and capture. Focused Left/Right seek
one second, Ctrl+Left/Right five seconds, Home/End the replay bounds, and Page
Up/Down thirty seconds. Shift+Left/Right pans, plus/minus zooms and zero restores
the full viewport. Handled keys stop before viewer-wide shortcuts. Pointer
scrubbing focuses the position slider; event and clip controls retain their own
focus and activation behavior.

The HUD hides after 3.2 seconds only during fullscreen playback with no clip
selection or hover/focus/pointer interaction hold. Pause, windowed mode and clip
selection reveal controls immediately. Pointer capture loss and window focus
transitions reconcile interaction holds. Window capture listeners observe activity
while the overlay root remains pointer-transparent, so a hidden HUD does not
consume the first video click. Disposal removes the inactivity timer and listeners.

`LibraryController` owns one active replay read and one replaceable latest intent
across viewer/exporter lifetimes. Admission includes root epoch, request, token,
game membership and navigation; superseded responses reject rather than publish.
App viewer opens, Settings/exporter returns and benchmark remounts capture a fresh
origin. An invalidated viewer selection unmounts. Exporter details use the same
owner; export's existing source/frame and mutation contracts remain unchanged.

`playback_payload_ready` still means the full semantic probe, now potentially after
mount; it never means descriptor readiness. Ordinary benchmark scenarios require
both full details and presented media, regardless of arrival order. Versioned
opening starts media actions independently and gates completion on admitted
details, retaining its opening deadline and phase markers. Historical library
milestone meanings are unchanged. Paint and subsequent duration-drain admission
check root/request/token/navigation identity; a stale Games view fails instead of
emitting library_useful. Old payload-before-mount attribution is not a descriptor
performance comparison.

`playbackController.ts` owns the primary media lifecycle through
`htmlVideoPlaybackAdapter.ts`: source/load, native play/pause, seek scheduling,
presentation callbacks, recovery, preferences, metrics and disposal. The viewer
retains layout and clip-selection policy and routes benchmark actions through the
same controller. Seeks explicitly settle superseded play operations, pause the
element and await natural target presentation. Native seek completion and the
qualifying frame callback may arrive in either order; automatic resumption waits
for both and preserves current pause/rate intent. A synchronous user play during
that wait retains its operation rather than being restarted by seek completion.
Separate 1500 ms native-seek and presentation deadlines enter bounded recovery;
no temporary low-rate play pulse is used. Invalid open inputs are rejected before
the current generation changes.

The primary JSX video carries `data-qb-primary-playback="true"`. The bounded native
WebView2 Media diagnostic associates its exact per-generation loopback load URL
and checks that marker when a DOM node ID is available. Binding generations are
monotonic within each diagnostic owner; equal conflicting bindings are rejected.
Recovery rotates the session query token on the same element and invalidates old
decoder evidence. CDP decoder properties have only player scope: there is no load
identity on a property, and delivery can cross `kLoad` boundaries. `playerCreated`
can also report an already active player. The monitor retains these bounded
player observations separately and never promotes them to current-load evidence.
Even an exact URL/marker association reports `unknown`, with current decoder name
and platform flag absent. A hardware/software claim requires a source that can
prove load ownership; the current CDP source cannot satisfy that requirement.
Disposal closes the native owner and removes its subscriptions.
Hardware remains the required normal production path. Acceptance can establish
it through controlled production load isolation with raw decoder and presentation
evidence; that conclusion is scoped to the observed fixture/runtime/device/load.
It does not change runtime Unknown or label arbitrary reload/recovery generations.
Continuous decoder identification is not required where trustworthy provenance is
absent. No additional normal runtime observer is introduced for acceptance proof.

Shared windowed/fullscreen controls expose 0.25x, 0.5x, 1x, 2x, 4x and 8x,
plus independent mute and volume intent. The controller keeps selected rate,
accepted native property and observed presentation advancement separate. Rate
verification uses settled RVFC windows; intentional seeks, loops, pauses and
hidden documents suspend measurement. Repeated buffering cannot renew an
unverified capability attempt beyond its ten-second deadline. Missing evidence
remains unknown, while measured slow/stalled playback is explicitly limited.
Failure selects the last verified usable rate or 1x, with at most one reload if
fallback also fails; recovery retains the original selection and limitation
without automatically reapplying the failed rate. Rate changes never emulate
speed with seeks. Audio property failures are diagnosed separately from user mute
and zero volume. Snapshot `muted`/`volume` retain recovery intent; the single
snapshot `media` observation supplies applied mute/volume to both control layouts.
Rejected or transformed requests display the element's adopted values, including
resetting the native volume thumb after a no-op request. Successful property
assignments do not prove audible output.

`requestVideoFrameCallback` is the presented-frame authority. Requested, dispatched, seeked, and presented values retain distinct generation/epoch state. Timeline markers, seeks, event cards, champion filters, and recorded player state use mapped replay ticks from the same payload; unavailable/before/after-media observations are explicit. Only hot visual values subscribe to the frame clock. Missing snapshots remain missing and the UI is descriptive, not prescriptive.

HEVC capability likewise requires a presented-frame callback and current video
data. Its callback observation is armed before playback and has a two-second
deadline even if `play()` never settles. Missing callbacks, media errors and
media-clock progress alone cannot establish support. Settlement releases the
callback, timer and error listener; the probe always unloads its video element.
Marker version 2 invalidates earlier clock-only probe results once, then retains
the existing startup cache behavior. Before normal app state is exposed, the
shared recording setting is reconciled to the current marker; a missing, invalid
or older marker clears an earlier positive setting until the new probe succeeds.
Benchmark launches leave their isolated configuration unchanged. An unavailable
presentation API fails closed.

Timeline elapsed-anchor state uses the sorted visible events' inclusive upper
bound, calculated by a binary-search memo. An index selector notifies elapsed
bindings when that boundary changes; individual anchors do not subscribe directly
to each presented frame. Head/fill/overview geometry retains continuous updates.

Data Dragon manifests/icons enrich champion and item presentation but do not gate playback. Cached versions continue to work offline; unavailable images fall back to fixed-layout placeholders.

## Clip flow

A supported replay event or timeline action enters clip mode with heuristic pre/post-roll. The user can adjust a media-bound, half-open source-frame interval subject to the minimum duration, choose output presets, vertical framing, music, and audio levels, then submit a request through Tauri.

The production vertical editor uses a sharp primary video and a muted background
video with CSS blur/brightness/saturation. The primary owns transport and time;
background drift beyond 80ms and explicit seeks are synchronized to it. Both pause
together, and the background releases its source on preset exit/disposal.
This restores the better measured baseline (QB-CLIP-011), not a claim that two
videos are the ideal long-term design. The QB-CLIP-010 single-source owner and GPU
renderer remain isolated research code, absent from the production import graph;
their failed resource measurements and immutable plans are retained. Revisit only
with a materially different credible performance hypothesis.

`app/src-tauri/src/clip_export.rs` validates the source bundle and request, selects
H.264 hardware encoders with software fallback, and invokes ffmpeg directly without
a shell. It emits progress, stages outputs under an owned export lease, verifies
Discord size with corrective retry, and generates thumbnails. Each validated MP4
is published before its optional JPEG with a no-replace same-volume rename.
Failure or process death preserves already published members; recovery removes
only provably owned unpublished staging files. See the full recovery and root
identity contract in [media-library.md](media-library.md). Published MP4s become
visible on the next library scan; source association comes from the MP4 filename.

The app resolves the locked packaged FFmpeg/ffprobe pair during setup. Production
uses `resources/media-runtime`; a whole-directory development override must satisfy
the same embedded lock. There is no PATH fallback. See
[media-runtime.md](media-runtime.md) for identity and failure behavior. Tauri's
Windows resource mapping includes this runtime; inclusion and supervision of the
standalone recorder and clean-machine installer certification remain distribution
work.
