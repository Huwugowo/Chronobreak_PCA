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

Opening a game asks Rust to parse its bundle into a playback probe containing the loopback video URL, recording FPS, sorted events, snapshots, and metadata. A single persistent video element is shared between windowed and fullscreen layouts; changing layout does not remount the decoder.

`requestVideoFrameCallback` is the preferred presented-frame clock. Timeline markers use stored `video_time_ms`; seeking, event cards, champion filters, and recorded player state derive from the same playback payload. Only hot visual values subscribe to the frame clock. Missing snapshots remain missing and the UI is descriptive, not prescriptive.

Data Dragon manifests/icons enrich champion and item presentation but do not gate playback. Cached versions continue to work offline; unavailable images fall back to fixed-layout placeholders.

## Clip flow

A supported replay event or timeline action enters clip mode with heuristic pre/post-roll. The user can adjust frame-aligned endpoints subject to the minimum duration, choose output presets, vertical framing, music, and audio levels, then submit a request through Tauri.

`app/src-tauri/src/clip_export.rs` validates the source bundle and request, selects H.264 hardware encoders with software fallback, and invokes ffmpeg directly without a shell. It emits progress, stages all outputs, verifies Discord size with corrective retry, generates thumbnails, and atomically exposes the completed batch. Failure removes partial outputs while preserving source media. Successful MP4/JPEG pairs become visible on the next filesystem library scan; source association comes from the MP4 filename.

Development builds currently discover ffmpeg from the environment. Bundled ffmpeg/ffprobe and a unified installed-tool path are future self-contained distribution work.
