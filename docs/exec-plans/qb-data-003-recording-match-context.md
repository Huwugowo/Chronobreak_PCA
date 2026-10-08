# QB-DATA-003: Recording-bound League match context

## Purpose

Give every new recording an optional, validated association to the League match and
local participant that produced it. Later post-game retrieval must attach to that
match, never whichever match happens to be newest. Expose the recorded context through
the existing replay-details path without requiring the app to run during collection.

## Relevant planning-time architecture

- `recorder/src/service.rs` allocates a collision-safe game directory and UUID
  `media_id` before native startup. After video readiness it starts `PollerSession`
  with the same ID and first-frame clock. Stop joins polling and video, validates
  media, then publishes canonical video/metadata.
- `recorder/src/poller.rs` already fetches Live active-player and roster data. Its raw
  structs include `riotId`, but persisted legacy names lose that distinction. Its
  coalesced writer exclusively owns `game_log.json`; keep that schema and ownership.
- `recorder/src/watcher.rs` performs a minimal two-second League process scan. Do not
  add broad command-line/CPU/module inspection to its hot loop for LCU discovery.
- `tools/client_api_capture.py` is the sole existing LCU reader: lockfile discovery,
  authenticated HTTPS GETs, and ignored raw output. It is evidence, not production
  code to run as a child or a persistence contract to import wholesale.
- `app/src-tauri/src/library.rs` strictly parses schema-v2 metadata/game logs. Its
  `save_game` rewrites metadata; its `delete_game` removes entire directories. The
  coordinator serializes app work only, not external recorder writes.
- `PlaybackProbe` is optional full replay detail behind the existing bounded selected
  read slot. `ReplayDescriptor` is the media-first path; do not expand or delay it.

## Scope and non-goals

Implement Windows LCU discovery/read transport, recording-scoped identity association,
static match context, optional sidecar persistence and replay-details projection.
Preserve the existing Live collector, video lifecycle, canonical clocks and playback.

Do not acquire post-game results/statistics, retain lobby/champ-select histories,
increase gameplay sampling, restore historical recordings, introduce a daemon/process,
change audio, add analytics UI, or build Match-V5/RSO integration. No general provider
framework, new database or alternate replay clock is needed. Non-Windows recording
continues with unavailable LCU context; do not claim a macOS LCU implementation.

## Exploration findings

Gameflow observations expose `gameData.gameId`, queue/map/mode and team entries with
PUUIDs. Current-summoner exposes `puuid`, `gameName`, `tagLine`; Live exposes the full
`riotId` of the active player and roster. These suffice for conservative local-session
association, but not for guessing from a truncated name or a timestamp alone.

Completed capture fixtures include matching game IDs in session/post-game/history,
while one diagnostic spans consecutive practice and completed matches. This makes
stale-session isolation essential. Practice dummy entries can be partially errored.
No verified platform-qualified game ID is supplied by every foundation response;
do not label a bare numeric game ID as globally unique.

Adding asynchronous fields to metadata would race the app's saved flag. A separate,
optional sidecar is preferable. A completed sidecar write failure does not invalidate
validated media; a hung OS write can delay publication while its worker stays owned
and must remain truthfully reported as finalizing/error, not successful completion.
The complete capability inventory is summarized in
`docs/product/league-data-capabilities.md`, but this plan contains the foundation's
required design without requiring raw-capture access.

## Chosen design and rationale

### Ownership and integration

Add a recorder `league_client` module containing typed transport DTOs, discovery,
association reducer and a recording-scoped `MatchContextSession`. The session starts
after video readiness alongside the existing poller, never on the GPU worker or WGC
callback. Its task retains at most one pending candidate and one accepted context.

Extend the poller with an in-memory latest-value observation channel containing the
active full Riot ID, roster full Riot IDs/team/champion names and observation instant.
Populate it from existing successful initial/focused snapshot responses; do not add
Live HTTP requests or change persisted snapshots. Consumers must not infer identity
from the truncated display-name normalization in the app.

The service owns an immutable recording-generation token containing `media_id`, the
selected PID/HWND and a generation counter; pass it into the context session rather
than deriving ownership from the latest watcher observation. Only that generation
may admit a result. Stop closes admission before cancellation; already-accepted
context survives ordinary game-window closure. A late round cannot bind after stop
or bind into a replacement recording, even when an OS PID/HWND is reused.

The data task observes LCU until it has accepted context, then stops network work.
It returns that immutable result, or a bounded unavailable reason, on recording stop.
Its drop/cancellation path cannot orphan a request/task. Stop cancels it in parallel
with poller/video cleanup; async network cancellation/join has a two-second deadline,
after which abort and await that async task. Blocking OS discovery or filesystem
workers are separately owned and joined, never detached by cancelling their waiter.
The network deadline is not a promise to forcibly interrupt blocked OS/disk calls.
Collection failure is logged as optional data degradation, not `ServiceEvent::Error`
for healthy video. Match association never authorizes video startup/shutdown.

### Discovery and transport

Discover running `LeagueClient.exe` installations using process name plus executable
path, then read `lockfile` beside the client executable. Inspect PID/executable path
only; never enumerate command lines, process memory or environment for credentials.
At most one process/lockfile pair may qualify. Multiple valid pairs are unavailable,
not resolved by lowest PID; stale/invalid pairs are rejected. A running client whose
identity cannot be inspected is not silently assumed stale to disambiguate another.
Cache the candidate while its process/lockfile generation
is unchanged. Rediscovery is at most once per ten seconds while unbound.
Run discovery/path/lockfile reads on one owned blocking worker, outside the watcher
hot loop. Do not start another while it is in flight; retain and join it on stop.
Use finite process enumeration and capped file reads, not recursive drive searches.

Allow a development-only absolute local-file override `QUEUEBACK_LCU_LOCKFILE`, still
requiring the production format, HTTPS/port/PID validation and loopback-only transport.
There is no arbitrary URL override. Synthetic tests inject discovery and transport
interfaces rather than relaxing production checks. No hardcoded C: installation path
is the production default. If executable-path access is denied, context is unavailable;
video continues.

Read at most 4 KiB from the lockfile; require five fields, a `LeagueClient` token
(or the explicit equivalent `LeagueClient.exe`), valid positive PID and port and
`https`. The PID must identify the discovered `LeagueClient.exe` whose executable
directory owns that lockfile; the override does not bypass that agreement. Reject
unrecognized tokens. The credential generation compares parsed PID, port and secret
plus process identity (including start time and executable path), not merely lockfile
mtime/size. Keep its secret only in a redacted
credential type in memory. Requests target only literal `127.0.0.1` at that port,
accept the client's self-signed certificate only in this private client, disable
proxy/redirects, and allow only GET `/lol-gameflow/v1/session` and
`/lol-summoner/v1/current-summoner`. Maximum one request in flight, two-second total
timeout per request and 256 KiB response cap enforced during streaming, not afterward.
Never include body, authorization, lockfile contents, Riot ID or PUUID in diagnostics.

An observation round fetches current-summoner, then gameflow session; re-read the
lockfile generation before and after the round and reject rotation mid-round. Require
two consecutive matching rounds, at least two seconds apart, for binding. Unbound
rounds run every five seconds using delayed ticks, no burst catch-up; after failures
use ten-second spacing, resetting to five seconds on successful observations. There
are no recursive/immediate retries, per-endpoint queues or growing request history.
These intervals are conservative initial bounds, not measured optimal cadences.

### Association and static field contract

Association requires all of the following:

1. The service's recording generation remains open to admission with its original
   exact League game PID/HWND; no result from another generation is accepted.
2. Two consistent rounds from the same LCU process/credential generation, with
   `phase == InProgress`, positive identical `gameData.gameId` and identical nonempty
   local PUUID.
3. Exactly one session team entry matching that local PUUID.
4. A successful Live observation no older than 15 seconds at the confirming round,
   whose active full Riot ID equals `gameName + '#' + tagLine` from current-summoner,
   and whose active participant occurs unambiguously in the Live roster.

Compare complete identifiers; do not strip tags, lowercase identifiers, equate bots
by champion, or accept a timestamp/champion/queue heuristic as proof. Missing or
conflicting identity keeps collection unbound. Successful later rounds may replace
an unaccepted candidate; never combine fields across candidates or credential epochs.
Once bound, freeze the association to its recording; later account/game changes do
not reassign it. A restarted recorder may bind a new recording generation to the
same match; do not deduplicate different `media_id` values or invent segmentation.

Static fields are allowlisted: local PUUID/full Riot ID, numeric LCU game ID, queue ID,
map ID, game mode, local session team, and session roster entries containing PUUID,
session team, participant slot/ID, champion ID and selected position/role when present.
Preserve session membership as source-native `team_one`/`team_two` from the containing
LCU arrays. Preserve a numeric LCU team ID only if actually supplied. Live team labels
such as `ORDER`/`CHAOS` remain separately source-labelled; do not equate them to LCU
array membership or invent a numeric 100/200 mapping. The exact local PUUID match
proves only the local participant's LCU membership.
Persist absent fields as absent. Retain the Live identity used to confirm the local
mapping; do not claim opponent Live-to-LCU mappings from incomplete names.
Do not synthesize a platform/region value from map fields. A later validated source
may provide platform/version in its own post-game contract.

The immutable association belongs to exactly one `media_id`; this is its attachment
key, with no redundant binding UUID. Preserve `game_id` as a positive decimal
string rather than a JavaScript number; it remains source-scoped, not a universal
match ID. PUUID and full Riot ID remain opaque strings. Source provenance records
the named endpoints and confirming UTC observation time; this wall time is provenance
only and never replaces game/replay calibration. Limit roster entries to 64 and
identifier/display strings to 256 UTF-8 bytes; reject excess rather than truncate.

### Persistence and compatibility

Add optional `league_match.json`, with independent `schema_version: 1`, `media_id`,
and a tagged association: `bound` with context/provenance, or `unavailable`
with a stable reason. Reasons distinguish no client, ambiguous client, denied
discovery, API unavailable/unsupported, missing Live identity, ambiguous identity,
conflicting identity and cancelled-before-binding. No raw endpoint payload is stored.
Use strict typed decoding; unsupported sidecar versions are optional-data unavailability.

Keep the collector result in memory until stop. A single recorder-owned write installs
the final sidecar before canonical video/metadata publication; no task may write it
after publication. Serialize within a 64 KiB limit, write a unique temporary sibling,
flush, and atomically install without replacing an existing sidecar. Never call
`create_dir_all` from this writer or recreate a removed recording directory.

Before validating identity or writing, acquire a Windows directory handle that
disallows directory deletion/rename, and hold it through no-clobber installation.
Compare its volume/file identity with the recording directory captured at allocation;
reject reparse-point/replaced targets and revalidate handle/path identity after the
install. This pins the directory itself, not its siblings: app deletion can still
remove media/game-log files while the directory handle is held. Therefore enrichment
is best effort, and the existing finalizer must still reject missing/invalid media.
Do not describe this guard as a transaction or protection of the whole recording.

Do filesystem work on one owned blocking worker. Await its completion before any
canonical publication, including cancellation/timeout paths; never detach a writer
or publish while it can still mutate the bundle. A blocked filesystem operation
cannot be hard-killed safely: retain truthful finalizing/error state and ownership
rather than claim bounded successful publication. A completed write failure degrades
only enrichment; canonical publication may continue under its existing safety checks.
No late post-game writer is introduced.
When such a writer is added by QB-DATA-002, it must separately coordinate with app
deletion; this foundation is not a general cross-process locking solution.

Old schema-v2 bundles without the sidecar remain valid and playable. Do not modify
`metadata.json`, `game_log.json`, replay-time version numbers, or original recordings.
Full replay details may read one capped optional sidecar using the existing owned
read slot, after mandatory bundle validation. Missing, malformed, wrong-media or
unsupported sidecars yield unavailable match context, never a failed descriptor or
invalid video. The app never rewrites this sidecar, including during save/unsave.

### App interface

Extend `PlaybackProbe` and its TypeScript boundary with `match_context`, a tagged
available/unavailable result; available exposes the validated static context and
provenance, unavailable exposes a non-sensitive reason. Validate `media_id` against
the selected bundle and preserve existing request/token/root-generation rejection.
Do not add LCU calls to the app, read sidecars in the core library scan, change
`ReplayDescriptor`, or add viewer controls. Future result/search features may extend
their own consumer contracts after this boundary is proven.

## Rejected alternatives and planning decisions

- Python helper as production collector: duplicates lifecycle/config/deployment and
  records private unrelated fields; retain it only as a diagnostic tool.
- UI-owned LCU connection: loses data when the app is closed.
- Latest-history/name/time matching: can attach another game/account's facts.
- Updating canonical metadata after finalization: conflicts with app-owned save flag.
- New fields in strict schema-v2 game log: unnecessary breakage for static enrichment.
- General source registry/database: not needed for the two local sources or this slice.
- Post-game writes in the foundation: adds deletion/reconciliation responsibilities
  that belong to QB-DATA-002; freeze identity before publication instead.

## Milestones

1. Implement typed optional context/availability contracts, safe field validation and
   pure association reducer with synthetic fixtures. No service networking yet.
2. Implement bounded discovery/transport, redacted diagnostics, rotation/cancellation
   behavior and deterministic fake transport tests.
3. Connect existing Live observations and the recording-scoped session; implement
   single-owner safe sidecar installation and failure isolation before publication.
4. Add optional full-probe projection and frontend type handling; preserve descriptor,
   old bundles, save state, deletion semantics and media delivery security.
5. Run focused automated, hostile-filesystem and generated recording integration
   checks; verify actual League association during a later ordinary recording and
   document the durable architecture only after it exists.

## Verification design

Pure fixtures cover: positive association; duplicate names/different tags; bots/dummies;
missing/zero IDs; account switch; stale session/Live observation; credential rotation;
multiple clients; delayed availability; consecutive matches; reconnect; recording
startup mid-match; two recording generations for one match; unknown/excess fields.

Transport tests cover loopback-only URL/auth, GET allowlist, redirects/proxy disabled,
malformed/oversized lockfile, wrong token/PID/executable directory, same-size credential
rotation, process identity reuse, 401/404/timeouts, streamed response cap,
partial JSON, request count/concurrency, delayed ticks, cancellation and secret-free
error formatting. Tests must run without League or credentials using synthetic data.

Temporary-library tests cover absent/corrupt/future/wrong-ID sidecar, valid context,
64 KiB boundary, atomic failure, existing target, deleted/replaced/reparse directory,
simultaneous deletion/install (including sibling removal under the directory guard),
saved-state preservation and no writes after canonical publication. A controllable
blocking worker proves cancellation does not detach it or permit early publication.
Never use user recordings as mutation fixtures.

Service tests prove healthy video starts/stops/finalizes with unavailable LCU, async
network cancellation stays bounded, OS workers remain joined/owned, and success
produces one sidecar for the correct media ID. Cover late completion after stop,
replacement recording/PID reuse, and preservation of bound context on normal closure.
Use an existing generated-window fixture plus injected LCU/Live observations to exercise
the full recorder boundary without a new user game. Read existing captures for evidence
only; any committed fixture is a manually sanitized synthetic equivalent.

Run the applicable verification baseline:

```powershell
cargo check --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo test --manifest-path recorder/Cargo.toml --all-targets --all-features
cargo fmt --manifest-path recorder/Cargo.toml -- --check
cargo clippy --manifest-path recorder/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
npm run test --prefix app
npm run check --prefix app
npm run build --prefix app
cargo build --release --locked --manifest-path recorder/Cargo.toml
git diff --check
```

Also run canonical schema/graph/plan/checkpoint validation from VERIFICATION.md. Final
manual evidence confirms app-closed collection, correct match/account association,
normal video publication, unchanged app playback and no secrets in logs. Missing
interactive evidence is reported explicitly and prevents feature-completion claims.

## Performance and reliability gates

Exactly one match-context task, one request in flight, one candidate and one result;
one final sidecar write; no per-frame work, hot-path process command-line scans or
full-video read. No HTTP wait may gate capture readiness or refresh video health.

A virtual-clock 60-minute fixture must prove no queue/history growth, retry bursts or
requests after binding/cancellation; enforce declared byte/roster limits. A generated
10-minute capture (an initial lifecycle gate, not an optimal-duration claim) with
failed/delayed LCU responses must retain bounded task/memory
ownership, normal frame reconciliation and successful media finalization. Record
collection request/latency/byte/write aggregates without identities. This is collection
overhead/lifecycle verification, not a substitute for QB-PERF-002's formal League
performance matrix. No claim of optimal cadence or broad client/mode coverage follows
from the existing short captures.
