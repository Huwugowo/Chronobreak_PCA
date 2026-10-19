# Filesystem media library

## Layout and authority

The configured output directory is the library root:

```text
<output>/
  games/
    <unix-timestamp>[-<collision-suffix>]/
      video.mp4
      game_log.json
      league_observations.jsonl # optional committed telemetry journal
      league_checkpoint.json   # optional current-state hint
      metadata.json
      league_match.json  # optional provisional context
      league_result.json # optional confirmed EOG facts
  clips/
    <source-timestamp>_<clip-timestamp>.mp4
    <source-timestamp>_<clip-timestamp>.jpg
```

Game-directory allocation in `recorder/src/storage.rs` is collision-safe. Recorder metadata and the small telemetry manifest/checkpoint use atomic replacement. Telemetry file operations retain the allocated directory's Windows identity and use delete-shared, handle-relative creation/rename. Clip export stages `.partial` media/thumbnail files and renames them only after the requested batch succeeds.

Game media/JSON and clip MP4 filenames are authoritative; clip JPEGs are optional presentation sidecars. `app/src-tauri/src/library.rs` reconstructs core game/clip summaries and usage by scanning the configured root. A local SQLite cache projects committed League observations for search and replay queries; it never authorizes media. Data Dragon assets, SQLite and frontend state are disposable caches. Authored bookmarks/recipes are stored separately in the library root's `league_authored_records.json` and survive cache rebuilds.

The `refresh_library` backend builds a `LibrarySnapshot` with an opaque token,
games, lightweight clips with unknown duration, and storage usage from one scan.
Each accepted game summary and video size is read once; clip source labels and
game totals reuse those summaries. Clips are enumerated once, including MP4s with
unrecognized names in storage totals while omitting them from displayed cards.
The core scan never invokes a media probe. The frontend consumes this snapshot
through one `LibraryController`, which owns the root, token, selections,
mutations, refresh coalescing, and optional duration enrichment.

Game summaries obtain optional current roster display data from a bounded manifest
and checkpoint, without scanning the journal. These are disposable hints, never
media authority or verified historical event evidence. Historical queries use the
validated journal's SQL projection. Both preserve exact full identity strings;
missing team knowledge stays neutral. Missing or invalid telemetry keeps healthy
video and saved intent available.

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

Descriptor and small recording-facts commands require the current selected snapshot
token and game membership and share one additional replay-read slot in the same
`LibraryCoordinator`. Excess requests fail Busy immediately. Strict bundle parsing
and filesystem access execute in one owned blocking job; caller cancellation cannot
release its permit before the job exits.
ReplayDescriptor and recording export authority read bounded strict `metadata.json`
(64 KiB maximum) for the exact media timeline. Optional League telemetry, including
an absent, malformed or unsupported `game_log.json`, cannot block these media-only
reads. Descriptor still requires a nonempty regular video file; served bytes and
export retain their existing opened-handle protection.
Library summaries retain validated media metadata and saved intent when telemetry
is unreadable, with an empty optional observation projection. Explicit deletion
reads saved intent directly from strict metadata and fails if that metadata cannot
be validated; automatic retention also skips incomplete or saved recordings.
Admission and completion capture the paired root and root epoch under the existing
guard order. Refresh, mutation, root A/B/A or shutdown invalidates old results.
No lock spans the read or await and no task-per-event fan-out is introduced.
A descriptor is not a media-delivery authorization:
the existing protected route and checked opened handle still control served bytes.

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

The Star uses a separate optimistic display overlay keyed to the originating root,
root epoch, request, token, navigation and canonical snapshot. A successful save
bridges only its own compulsory refresh while old cards remain visible; the next
canonical snapshot wins. Failure, an unrelated refresh, navigation or root change
clears the overlay. Stale completions cannot publish notices or clear newer
overlays. The overlay never mutates the snapshot or authorizes an action.

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

`video.mp4` is a fragmented capture intended to remain seekable after an interrupted process. `metadata.json` stores recording format details, duration, local-player metadata, save protection, and display fields. `game_log.json` is now a small schema-v2, telemetry-version-1 manifest: media/run identity, source epoch, committed journal position/checksum and current calibration. The old accumulated history format has no compatibility reader.

`league_observations.jsonl` is the authoritative optional observation stream. Compact
batches have sequence bounds and chained SHA-256 commits; acknowledgement follows
flush/sync. Recovery accepts validated committed prefixes, rejects interior
corruption and reports incomplete suffix bytes. Replay checks the exact manifest
anchor but may consume later valid commits after a stale manifest. It never exposes
partial projection on validation failure. The SQLite importer consumes at most four
committed batches per request and advances its source position in the same
transaction as projected rows. Only a validated complete source is queryable;
invalid or unsupported telemetry is unavailable. Refresh checks bounded manifests,
sidecars and file generations rather than reparsing every journal.

`league_checkpoint.json` retains only committed current context, full roster payload
dictionary, source epoch and references. Library summaries validate its identity
and position against the manifest, without claiming historical journal validation.
Replay KDA/loadout/local timeline selection uses stored participant references.
Event projection preserves `stolen` and `recipient`; playback reports observed gap
counts, uncommitted tail bytes, unmapped observations and unresolved actors. These
counters do not establish complete coverage.

The app treats malformed or incomplete entries conservatively: recognized bundles are parsed into summaries; playback requires mandatory properties such as a usable recording frame rate. Missing League data is represented as absent rather than inferred.

The small recording-facts command reads optional `league_match.json`, within its
owned replay-read slot after strict core bundle validation. The reader accepts a
regular file with at most 64 KiB of actual bytes, schema version 1, matching media ID,
canonical positive decimal game ID, required u32 queue ID, bounded full Riot ID,
compatible map/mode and ordered two-round provenance. Invalid or absent context
becomes null and never invalidates playback. The projection omits PUUID, epoch nonce
and request evidence. Core scans use this context only to validate optional final
facts; ReplayDescriptor and replay-time schemas remain independent. Version 1 is
provisional; a bare confirmed marker
is rejected: confirmation belongs to a separate `league_result.json`.

The result reader is capped at 256 KiB, validates the shared typed schema and array
bounds, and requires the core media UUID and validated provisional game ID to
agree. Invalid, missing, unsupported or mismatched facts are absent. Replay
details expose nullable final statistics and EOG provenance separately from recorded
totals. The library scan reads optional facts once per valid video bundle and emits
only outcome, ended-early and partial flags (no scoreboard in the core IPC payload).
Confirmed unknown outcomes never become Defeat or Remake. ReplayDescriptor and
video mounting do not read either optional sidecar.

Result writing yields to existing deletion without adding a deletion protocol.
A narrow delete/write race can leave an empty directory or stray sidecar. Existing
scans can show timestamp-named directories without valid core data as incomplete
entries; they cannot supply confirmed facts or playable media from a stray result.
This is the existing library behavior, including the deliberate v2 race acceptance.

## League query cache and consumers

`league_index` stores one schema-versioned SQLite file under the local app cache,
keyed to the approved library root. WAL mode, bounded transactions and prepared
statements support one writer and two independent readers. Each owned blocking job
retains its slot through caller cancellation. Admission and completion require the
accepted snapshot/root epoch, so root A/B/A, refresh and shutdown reject stale work.
Read transactions establish one snapshot before checking catalog identity.

Catalog reconciliation admits unambiguous media IDs from the core snapshot and
removes stale membership. The importer validates journal commits, the manifest
anchor, source generation and calibration before publishing a complete projection.
Event revisions retain evidence and exact `(media_id, run_id, epoch)` scope; actor
joins require stored participant references. Replay ticks are calculated from each
observation's own calibration. A timing change invalidates the projection.

Whole-source coverage totals and observation bounds are materialized per endpoint
once a projection reaches its validated end. The totals, committed position and
ready status publish in the same SQLite transaction. Replay and roster queries
reuse those totals, while requested-range gaps and current event actor revisions
remain interval queries. Partial imports keep their original coverage calculation
and unknown-match semantics. Rebuilds discard the derived totals with the rest of
the disposable cache; a schema-version change rebuilds existing cache files.

History pages contain at most 50 rows and 2 MiB of retained response data, with
keyset pagination and semantic filters. A new accepted snapshot marks cached source
facts unchecked until each bounded catalog import verifies that recording's small
source stamp; unchanged sources do not require another journal read. Semantic
matches and statistics remain pending for unchecked sources. Coverage classes are
materialized during import, with catalog totals maintained in at most 64 buckets;
page reads never aggregate all historical observations. History describes the last
checked projection in its accepted snapshot; selected replay reads also recheck
the current source stamp before returning data.
KDA and inventory use nullable final EOG facts or an explicitly labelled recorded
snapshot. An unknown scalar remains unknown. Negative event queries require
successful observations spanning the requested interval, without relevant gaps,
uncommitted bytes or unresolved/missing mappings; otherwise coverage stays unknown.

The Games screen imports incrementally while idle. The viewer mounts video from
the media descriptor first, then loads small final facts and selected-source replay
windows. It debounces viewport/playhead requests, keeps at most four event pages,
and offers explicit event pagination. A window contains at most 512 events and the
latest mapped roster at or before the requested time, including sample age. It
does not interpolate roster state across gaps or request SQL data every frame.
Clip export obtains exact frame authority from the media descriptor.

Settings can rebuild searchable data. Reset exclusively acquires the index writer
and both readers, quarantines only the generated SQLite/WAL/SHM files, and creates
a fresh catalog. A corrupt cache can therefore recover without touching source
media, observations or durable authored records. Authored records have their own
bounded, revision-checked atomic store; SQL only projects their references and
ordered frame ranges. Cache deletion/rebuild preserves those records.

## Clips

The clip exporter never modifies source recordings. Each completed export normally has MP4 media and a JPEG thumbnail. The filename links the clip to its source game and creation timestamp; the selected source time range and in-range events are not currently persisted as clip metadata. The Clips tab is rebuilt from MP4 files and uses a JPEG when present. Deleting a clip removes only its recognized clip artifacts.

## Retention and deletion safety

The app reports game-video and clip-MP4 bytes/counts and supports explicit game/clip deletion. Age-based `run_auto_delete` skips games whose metadata marks them saved. Settings changes update the app's media root; recorder use of the shared setting takes effect when its configuration is next loaded.

Destructive tests must use `tempfile` or a purpose-created test library. Real user recordings must never be used for retention, corruption, recovery, relocation, or deletion tests.

## Re-indexability limits

Core summaries remain filesystem-derived, and the League SQL projection is
rebuildable from committed sources. Authored records retain media, participant,
event revision and ordered frame references outside the cache. This supports cache
loss and an unchanged library moved to another root. General moved-root relinking,
reduced-metadata external import and richer exported clip metadata remain roadmap
work under `QB-LIB-005`, `QB-LIB-010`, `QB-LIB-011`, and `QB-LIB-002`.
