# Desktop library, replay, and clip flow

## Tauri boundary and startup

`app/src-tauri/src/lib.rs` loads shared configuration, creates synchronized app state, installs built-in music, starts the local playback server, launches asynchronous Data Dragon initialization, runs retention, and registers Tauri commands. Commands expose settings, library scans/mutations, playback probes, HEVC capability state, assets, music, export, and diagnostics to `app/src/api.ts`.

SolidJS `app/src/App.tsx` owns screen navigation and resources for games, clips, storage, settings, and Data Dragon status. The frontend does not receive raw multi-gigabyte video bytes through Tauri IPC.

## Loopback playback server

`app/src-tauri/src/playback_server.rs` binds an OS-assigned port on IPv4 loopback and serves:

- game recordings and clip assets with HTTP byte-range support;
- an embedded HEVC probe clip;
- bundled and explicitly registered imported-music previews;
- versioned Data Dragon images from the local cache, fetching and caching missing assets when allowed.

Path components are validated before resolution. Range responses stream file slices through Tokio and count requests, bytes, completed streams, and cancellations for diagnostics. This keeps memory use independent of whole-file size and lets the webview's native `<video>` element own decode, buffering, seeking, rate, and audio.

## Library and viewer flow

Opening a game asks Rust to strictly parse its schema-v2 bundle into a playback probe containing the loopback video URL, validated media timeline, mapped events, snapshots, and metadata. A single persistent video element is shared between windowed and fullscreen layouts; changing layout does not remount the decoder.

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
decoder evidence. Candidate decoder properties are scoped to each `kLoad` and
cleared before reconciling its URL, including when WebMediaPlayer reuses a player
ID. Candidates remain available across binding/load races, so fresh properties
received after a load but before its binding retain their correct ownership.
Disposal closes the native owner and removes its subscriptions.
Only associated `D3D11VideoDecoder` plus the platform flag proves the supported
Windows hardware path; software and unknown outcomes remain explicit.

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
and zero volume; successful property assignments do not prove audible output.

`requestVideoFrameCallback` is the presented-frame authority. Requested, dispatched, seeked, and presented values retain distinct generation/epoch state. Timeline markers, seeks, event cards, champion filters, and recorded player state use mapped replay ticks from the same payload; unavailable/before/after-media observations are explicit. Only hot visual values subscribe to the frame clock. Missing snapshots remain missing and the UI is descriptive, not prescriptive.

Data Dragon manifests/icons enrich champion and item presentation but do not gate playback. Cached versions continue to work offline; unavailable images fall back to fixed-layout placeholders.

## Clip flow

A supported replay event or timeline action enters clip mode with heuristic pre/post-roll. The user can adjust a media-bound, half-open source-frame interval subject to the minimum duration, choose output presets, vertical framing, music, and audio levels, then submit a request through Tauri.

`app/src-tauri/src/clip_export.rs` validates the source bundle and request, selects H.264 hardware encoders with software fallback, and invokes ffmpeg directly without a shell. It emits progress, stages all outputs, verifies Discord size with corrective retry, generates thumbnails, and atomically exposes the completed batch. Failure removes partial outputs while preserving source media. Successful MP4/JPEG pairs become visible on the next filesystem library scan; source association comes from the MP4 filename.

Development builds currently discover ffmpeg from the environment. Bundled ffmpeg/ffprobe and a unified installed-tool path are future self-contained distribution work.
