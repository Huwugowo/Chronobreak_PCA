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

Game-directory allocation in `recorder/src/storage.rs` is collision-safe. Recorder metadata and the small telemetry manifest/checkpoint use atomic replacement. Telemetry file operations retain the allocated directory's Windows identity and use delete-shared, handle-relative creation/rename. Clip export stages media/thumbnail pairs in an owned batch directory and publishes only after all requested members pass validation.

Game media/JSON and clip MP4 filenames are authoritative; clip JPEGs are optional presentation sidecars. `app/src-tauri/src/library.rs` reconstructs core game/clip summaries and usage by scanning the configured root. A local SQLite cache projects committed League observations for search and replay queries; it never authorizes media. Data Dragon assets, SQLite and frontend state are disposable caches. Authored bookmarks/recipes are stored separately in the library root's `league_authored_records.json` and survive cache rebuilds.

Normal startup accepts an optional absolute `LEAGUE_REPLAY_APP_DATA_DIR` for app-owned files/caches; an invalid override fails instead of falling back. Its absence preserves Tauri's normal app-local-data directory. Library/config and WebView profile selection remain independent.

Index schema v5 keeps producer event revisions/actor references unchanged. After complete journal validation, it atomically publishes separate derived actor links, effective coverage and ready state. A raw unresolved tagless actor can use exactly one validated explicit roster `riot_id_game_name` in the same media/run/epoch, with ASCII case-insensitive equality for both matching and collisions. Missing/conflicting identities and non-player labels stay unresolved; resolved/conflict producer references are retained. SQL semantic queries use the effective-actor view; IPC returns raw `revision` and separate `effective_actors` for presentation. The v5 cache filename triggers the normal disposable rebuild from filesystem evidence; no recording migration occurs.

The `refresh_library` backend builds a `LibrarySnapshot` with an opaque token,
games, lightweight clips with unknown duration, and storage usage from one scan.
Each accepted game summary and video size is read once; clip source labels and
game totals reuse those summaries. Clips are enumerated once, including MP4s with
unrecognized names in storage totals while omitting them from displayed cards.
The core scan never invokes a media probe. The frontend consumes this snapshot
through one `LibraryController`, which owns the root, token, selections,
mutations, refresh coalescing, and optional duration enrichment.

Core game summaries carry no inferred roster. Checked history rows add roster
display data from validated final facts, or one complete indexed observation from
a single source when final rosters are unavailable. Cards consume this history
projection rather than the core snapshot's empty compatibility participant list.
Exact recorded names are preserved; missing team knowledge stays neutral. No
journal scan or additional file read occurs on a history page. Missing or invalid
telemetry keeps healthy video and saved intent available.
Unfiltered first pages also include incomplete filesystem entries alongside the
bounded SQL page; subsequent SQL pages do not repeat those entries. Summoner
spells come from checked final facts or the selected recorded snapshot, preserving
missing slots. The versioned Data Dragon summoner manifest resolves numeric IDs,
internal keys and recorded display names to validated image filenames. An older
offline cache still serves its other assets when that manifest is unavailable.

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
After an export attempt, only the same editor, source, root epoch and navigation
can adopt its completed refresh's new token. Retry waits for descriptor validation
against that token; refresh failure or changed source blocks stale dispatch.
The admitted export worker owns completion, staging and child cleanup even when
its waiter disappears. Encoding has a duration-derived deadline (5 minutes to
6 hours) and a 120-second progress stall limit; thumbnails have a 30-second limit.
Progress records retain at most 8 KiB and diagnostics 64 KiB. Interruption kills
and reaps the child before admission releases and never triggers encoder fallback.

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
snapshot. History also projects final keystone/secondary rune-tree IDs and total CS
(lane plus neutral minions). CS requires both components; CS/min uses positive
final game duration, independently of recording length or discontinuous replay
clocks. These fields remain unavailable for provisional snapshots. An unknown
scalar remains unknown. Negative event queries require
successful observations spanning the requested interval, without relevant gaps,
uncommitted bytes or unresolved/missing mappings; otherwise coverage stays unknown.

The Games screen imports incrementally while idle. The viewer mounts video from
the media descriptor first, then loads small final facts and selected-source replay
windows. It debounces viewport requests, keeps at most four event pages,
and offers explicit event pagination. A window contains at most 512 events;
its legacy single-roster field is not the temporal overlay's state owner.
The overlay independently loads compact complete snapshots from mapped SQLite
rosters, in pages of at most 128 samples / 2 MiB. Continuations carry an exclusive
replay-tick/observation-ID cursor and the same committed journal position, rejecting
mixed revisions. The first page supplies consistent source-wide participant
identities. Each sample contains only its mapped replay tick, original game clock,
participant level/dead/timer/spells and local KDA/CS. Raw roster payloads, items,
descriptions, event data and repeated observation evidence are not retained.
One selected source retains at most 131,072 samples / 64 MiB serialized sample
data; overflow is an explicit unavailable error, never silent truncation.
Video-driven binary lookup requires a proven predecessor: while continuation is
pending, the last loaded replay tick and later positions remain unavailable,
including equal-tick samples split between pages. Event paging cannot evict this
timeline. Source change/disposal releases it. The shared library reader retains
one active read overall and one latest pending intent in each of two explicit
lanes, details and overlay; navigation/root/refresh invalidates both.
Replay details remain loading until the selected overlay timeline is complete;
video playback stays available during loading or an optional-data failure.
Only after completion, the frontend derives distinct first-recorded spell-loadout
context from the earliest mapped sample. Before that sample, it supplies two
nullable spell labels per recorded participant, without any temporal status.
Normal samples replace it completely; source changes/errors clear it. No extra
query, persisted data or final-result substitution is involved.
It does not interpolate roster state across gaps or request SQL data every frame.
Clip export obtains exact frame authority from the media descriptor.

History queries and imports use independent single active lanes. Import completion
marks an outstanding query dirty without changing its admission generation. On
settlement, pending user input/pagination runs first; otherwise one coalesced
automatic refresh incorporates completed imports. Dispatch consumes the dirty
bit. Context replacement/disposal clears it, while a stale query releasing the
global lane still permits current-context work. Explicit newer input keeps its
existing generation-based supersession and benchmark observation semantics.

Settings can rebuild searchable data. Reset exclusively acquires the index writer
and both readers, quarantines only the generated SQLite/WAL/SHM files, and creates
a fresh catalog. A corrupt cache can therefore recover without touching source
media, observations or durable authored records. Authored records have their own
bounded, revision-checked atomic store; SQL only projects their references and
ordered frame ranges. Cache deletion/rebuild preserves those records.

## Clips

The clip exporter never modifies source recordings. Each completed export normally has MP4 media and a JPEG thumbnail. The filename links the clip to its source game and creation timestamp; the selected source time range and in-range events are not currently persisted as clip metadata. The Clips tab is rebuilt from MP4 files and uses a JPEG when present. Deleting a clip removes only its recognized clip artifacts.

`export_staging` owns `clips/.chronobreak-export/<32-hex-id>/`: one strict,
versioned ownership record (8 KiB maximum, at most three destinations) and fixed
numbered MP4/JPEG files. A persistent `.lease` holds a nonblocking OS lock through
export/recovery. Process death releases that lock; active ownership makes recovery
skip all batches. Retained Windows directory handles reject reparse points and
deny deletion/renaming of the approved root and staging chain during work.
The approved root includes its directory generation; coordinator admission compares
that identity, and a refresh retains a root guard through recovery and scanning.

Publication uses same-volume, atomic, no-replace Windows moves: validated MP4 first,
then optional JPEG. Neither ordinary failure nor restart cleanup removes final
files. A failed or interrupted multi-preset publication may leave a playable
completed subset, including an MP4 without its JPEG. Normal refresh reclaims only
strictly owned unpublished staging before calculating cards and MP4 usage;
explicit export also reconciles while holding the lease. Cleanup validates the
flat directory and removes only enumerated regular files, then the ownership
record and empty directory. Unknown, malformed, foreign, reparse or locked entries
are retained with bounded diagnostics; legacy unowned `.part` files are untouched.
Recovery never reads source media or starts media tools. It streams directory
names, processes at most 64 batches per attempt and rotates through retained IDs;
the report retains at most four bounded diagnostics. This is process-interruption
recovery, with no claim of power-loss durability.

Benchmark background refresh performs no recovery mutation. Explicit benchmark
exports keep their isolated-fixture write behavior but never reclaim earlier
staging. Filesystem-derived clip storage counts final MP4 bytes only; staging and
JPEGs are outside that metric.

Every preset exports H.264/AAC, including a valid recording whose media timeline
declares no source audio. The exporter derives audio presence from that validated
timeline and supplies finite stereo 48 kHz silence only for absent source audio.
Selected music uses the same two-input mix with that silent base, preserving the
existing volume/normalization semantics. Silence is bounded by the selected clip
duration; output frame and audio-alignment validation remain mandatory.

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
