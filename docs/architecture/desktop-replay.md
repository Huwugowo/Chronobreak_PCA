# Desktop library, replay, and clip flow

## Tauri boundary and startup

`app/src-tauri/src/lib.rs` loads shared configuration, creates synchronized app state, installs built-in music, starts the local playback server, launches asynchronous Data Dragon initialization, runs retention, and registers Tauri commands. Commands expose settings, library scans/mutations, playback probes, HEVC capability state, assets, music, export, and diagnostics to `app/src/api.ts`.

SolidJS `app/src/App.tsx` owns screen navigation, a single `LibraryController` snapshot for games/clips/storage, and independent settings/Data Dragon resources. The frontend does not receive raw multi-gigabyte video bytes through Tauri IPC.

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

Opening a selected game first requests a token-bound `ReplayDescriptor`: snapshot
token, game ID, protected loopback video URL and validated schema-v2 media timeline.
Rust uses the strict bundle reader and requires a nonempty regular video, but does
not construct semantic projections or run media probes. The existing viewer and
controller mount from this descriptor and the selected snapshot's display summary.
Data Dragon, optional clip durations and full replay details do not gate that mount.
A single persistent video element is shared between windowed and fullscreen layouts;
changing layout does not remount the decoder.

After two animation frames provide a paint opportunity, the viewer requests the
unchanged full playback probe. Its game, URL and decoded media timeline must match
the descriptor before semantic arrays are admitted. Existing event/player/KDA
views update reactively without reopening media or resetting rate, clip range,
seek state or fullscreen. Pending/failed details are explicit, with a local retry;
missing stats and game-time calibration stay unavailable. Strict JSON is read for each request;
there is no persistent cache or claim that aggregate I/O is reduced.

`ReplayTimeline` supplies the same seek, zoom, pan, clustered-event and clip rail
in windowed and fullscreen layouts. Its viewport and exact frame clip draft live
in the persistent playback surface; pending details do not gate basic media
controls. Each disposable rail releases pointer capture, gesture listeners and its
resize observer on cancellation or unmount, including a layout change mid-drag.

`LibraryController` owns one active replay read and one replaceable latest intent
across viewer/exporter lifetimes. Admission includes root epoch, request, token,
game membership and navigation; superseded responses reject rather than publish.
App viewer opens, Settings/exporter returns and benchmark remounts capture a fresh
origin. An invalidated viewer selection unmounts. Exporter details use the same
owner; export's existing source/frame and mutation contracts remain unchanged.

`playback_payload_ready` still means the full semantic probe, now potentially after
mount; it never means descriptor readiness. Benchmark scenarios require both full
details and presented media, regardless of arrival order. Historical library
milestone meanings are unchanged. Paint and subsequent duration-drain admission
check root/request/token/navigation identity; a stale Games view fails instead of
emitting library_useful. Old payload-before-mount attribution is not a descriptor
performance comparison.

`playbackController.ts` owns the primary media lifecycle through
`htmlVideoPlaybackAdapter.ts`: source/load, native play/pause, seek scheduling,
presentation callbacks, recovery, preferences, metrics and disposal. The viewer
retains layout and clip-selection policy and routes benchmark actions through the
same controller. Public play completion and internal presentation nudges have
separate ownership; seeks explicitly settle superseded play operations. Invalid
open inputs are rejected before the current generation changes.

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

Data Dragon manifests/icons enrich champion and item presentation but do not gate playback. Cached versions continue to work offline; unavailable images fall back to fixed-layout placeholders.

## Clip flow

A supported replay event or timeline action enters clip mode with heuristic pre/post-roll. The user can adjust a media-bound, half-open source-frame interval subject to the minimum duration, choose output presets, vertical framing, music, and audio levels, then submit a request through Tauri.

`app/src-tauri/src/clip_export.rs` validates the source bundle and request, selects H.264 hardware encoders with software fallback, and invokes ffmpeg directly without a shell. It emits progress, stages all outputs, verifies Discord size with corrective retry, generates thumbnails, and atomically exposes the completed batch. Failure removes partial outputs while preserving source media. Successful MP4/JPEG pairs become visible on the next filesystem library scan; source association comes from the MP4 filename.

Development builds currently discover ffmpeg from the environment. Bundled ffmpeg/ffprobe and a unified installed-tool path are future self-contained distribution work.
