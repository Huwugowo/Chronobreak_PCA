# Filesystem media library

## Layout and authority

The configured output directory is the library root:

```text
<output>/
  games/
    <unix-timestamp>[-<collision-suffix>]/
      video.mp4
      game_log.json
      metadata.json
  clips/
    <source-timestamp>_<clip-timestamp>.mp4
    <source-timestamp>_<clip-timestamp>.jpg
```

Game-directory allocation in `recorder/src/storage.rs` is collision-safe. `game_log.json` and recorder metadata are written through temporary files and atomic replacement. Clip export stages `.partial` media/thumbnail files and renames them only after the requested batch succeeds.

Game media/JSON and clip MP4 filenames are authoritative; clip JPEGs are optional presentation sidecars. There is no SQLite or hidden library database. `app/src-tauri/src/library.rs` reconstructs `GameSummary`, playback payloads, clip summaries, and usage by scanning the configured root. Data Dragon assets and frontend state are disposable caches.

## Game bundles

`video.mp4` is a fragmented capture intended to remain seekable after an interrupted process. `game_log.json` stores normalized events, snapshots, derived item/level changes, diagnostics, and video/game clock mapping. `metadata.json` stores recording format details, duration, local-player metadata, save protection, and display fields.

The app treats malformed or incomplete entries conservatively: recognized bundles are parsed into summaries; playback requires mandatory properties such as a usable recording frame rate. Missing League data is represented as absent rather than inferred.

## Clips

The clip exporter never modifies source recordings. Each completed export normally has MP4 media and a JPEG thumbnail. The filename links the clip to its source game and creation timestamp; the selected source time range and in-range events are not currently persisted as clip metadata. The Clips tab is rebuilt from MP4 files and uses a JPEG when present. Deleting a clip removes only its recognized clip artifacts.

## Retention and deletion safety

The app reports game-video and clip-MP4 bytes/counts and supports explicit game/clip deletion. Age-based `run_auto_delete` skips games whose metadata marks them saved. Settings changes update the app's media root; recorder use of the shared setting takes effect when its configuration is next loaded.

Destructive tests must use `tempfile` or a purpose-created test library. Real user recordings must never be used for retention, corruption, recovery, relocation, or deletion tests.

## Re-indexability limits

Today, restarting the app naturally re-indexes valid bundles in place because summaries are filesystem-derived. This supports recovery from ephemeral UI/index loss. Full portability is not yet complete: durable relationship reconstruction, moved-root relinking, reduced-metadata external import, and richer clip metadata remain roadmap work under `QB-LIB-005`, `QB-LIB-010`, `QB-LIB-011`, and `QB-LIB-002`.
