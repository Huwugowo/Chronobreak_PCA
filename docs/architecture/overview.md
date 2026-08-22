# Runtime overview

Chronobreak is a local-first, two-process desktop application.

## Process boundary

The standalone Rust recorder (`recorder/`) is the capture process. It watches for the League game process, launches ffmpeg, polls Riot's local Live Client API, and writes game bundles. It is designed to remain idle in the tray and does not need the review app to be open.

The Tauri 2 application (`app/src-tauri/` plus `app/src/`) is the post-game library, replay, and export process. Rust scans and mutates the local library, serves local media, initializes Data Dragon assets, and launches ffmpeg exports. SolidJS owns navigation and presentation. It does not capture gameplay.

In the current development architecture neither process supervises or launches the other. Installed recorder lifecycle, login autostart, and exactly-one-instance behavior are distribution work, not current invariants.

## Shared contracts

The processes communicate through ordinary files and shared configuration rather than IPC or a database:

- the configured output root contains `games/` and `clips/`;
- one game directory contains `video.mp4`, `game_log.json`, and `metadata.json`;
- app and recorder share one TOML schema for output location, retention, recording profile/codec, autostart intent, and HEVC playback capability; the recorder reads it at startup while the app atomically preserves/writes the shared document;
- app and recorder share the exact packaged FFmpeg/ffprobe pair described in [media-runtime.md](media-runtime.md); PATH and single-tool overrides are not production discovery paths;
- the app scans filesystem bundles each time it lists the library;
- clip filenames encode the source game identifier used by the current clip library contract; the paired JPEG is presentation-only.

The filesystem is the durable authority. The loopback playback server, in-memory Tauri state, frontend signals, and Data Dragon cache can be recreated.

## Trust and network boundaries

The recorder reads League process state and connects only to Riot's local Live Client endpoint at `127.0.0.1:2999`. The app may contact Riot Data Dragon for versioned static manifests/icons. Playback and imported music are served only from an ephemeral `127.0.0.1` port.

All path-like identifiers accepted by the playback and library layers are validated before joining them to configured roots. Destructive operations are restricted to recognized game/clip entries. Tests for deletion or retention use temporary directories.

## Product boundary

Recorded events and snapshots are descriptive navigation data. Missing data stays missing; neither process fabricates unavailable team gold, outcomes, or coaching judgments. `docs/product/PRODUCT.md` is authoritative for this boundary.
