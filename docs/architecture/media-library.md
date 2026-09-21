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

The `refresh_library` backend builds a `LibrarySnapshot` with an opaque token,
games, lightweight clips with unknown duration, and storage usage from one scan.
Each accepted game summary and video size is read once; clip source labels and
game totals reuse those summaries. Clips are enumerated once, including MP4s with
unrecognized names in storage totals while omitting them from displayed cards.
The core scan never invokes a media probe. The frontend consumes this snapshot
through one `LibraryController`, which owns the root, token, selections,
mutations, refresh coalescing, and optional duration enrichment.

AppState owns a disposable `LibraryCoordinator`: one blocking scan slot, immediate
Busy rejection of excess requests, and one completed snapshot. The blocking
worker retains its permit after caller cancellation. Every admitted refresh gets
a fresh token and revision; cancellation, root publication or shutdown rejects old
results. Same-root scan errors retain the complete prior view and return an error
with its token; root changes clear that view. The coordinator guard precedes the
MediaRoots guard for paired logical-path/ApprovedRoot capture and revision
publication. Settings validation and persistence finish before publication, with
no filesystem I/O or await under these guards. Snapshot identity never authorizes
media delivery; the existing opened-handle boundary remains authoritative.

Optional `resolve_clip_durations` accepts one to eight distinct current-snapshot
clip IDs. Its one global blocking batch/child slot rejects excess requests without
queuing. A caller drop, invalidation or shutdown cancels remaining work; the worker
owns its permit until its child is killed/reaped and its bounded reader joins.
Each packaged ffprobe has a ten-second deadline and at most 4 KiB of stdout;
stderr is discarded because it can contain bearer URLs. Probes use only the existing
capability-protected clip HTTP route, forced MOV demuxing, disabled external data
references/absolute paths and the `http,tcp` protocol whitelist. Opened-handle file
identity, length and modification facts are checked before/after probing and on
cache reuse. Replacement/deletion produces local unavailable state; explicit
`retry_unavailable` retries failure. All optional results disappear on refresh or
invalidation. They never authorize playback/export or identify immutable bytes.

Production requests are limited to visible or active Clips work. The
benchmark-only compatibility path drains the same command sequentially in
eight-ID batches after the Games paint milestone so the historical
`library_useful` event keeps its clips-ready meaning; normal navigation never
starts an all-library duration walk.

Save/delete/retention commands require the selected snapshot token and validate
root and item membership at admission. They share one owned blocking mutation slot
with settings preparation/persistence/publication. Slots survive dropped waiters;
no configuration or publication lock spans filesystem work or await. Every admitted
mutation attempt invalidates through a completion guard, including partial errors
and unwinding; errors remain visible. Failed settings persistence retains the
published pair and revision. Refresh-invalidated retained views are display-only
until a successful refresh supplies an admissible token.

Exports retain their admitted destination and strict source/frame contract. Their
completion invalidates the current view only when its logical path and approved
root match that destination, including a return to the same root after A/B/A. A
root epoch and selected token separately reject stale export responses. Completion
in another root does not invalidate that root. Frontend actions carry the
originating snapshot token through dispatch and reconcile the captured root
after every admitted mutation attempt, including stale or partial completions.
Response admission additionally checks root, request, token, and navigation
identity, so late work cannot publish into a newer view.

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
