# QB-DATA-002: Exact post-game confirmation and final League facts

## Purpose

Confirm a recording's provisional League candidate after the match, then expose
available final results and useful player/team statistics with explicit coverage.
The recorder performs bounded post-game collection while the viewer is closed.
Playback remains available when post-game data is delayed, absent, contradictory,
or expired.

## Relevant planning-time architecture

- QB-DATA-003 writes an immutable schema-v1 `league_match.json` before canonical
  publication. It contains the media UUID, canonical positive decimal `game_id`,
  queue ID, local identity, compatible map/mode and two-round provenance. It is
  provisional and is never rewritten by this feature.
- `recorder/src/service.rs` owns capture closure, media validation and publication.
  The Live poller remains the only Live request owner; LCU state and Live `GameEnd`
  never drive capture shutdown or establish Victory/Defeat.
- `recorder/src/league_client/transport.rs` already validates the LeagueClient
  process/path and adjacent lockfile, uses opaque credentials over literal
  loopback HTTPS, finite GET allowlists, bounded bodies/deadlines and joined
  discovery workers.
- The sidecar writer retains Windows directory identity and performs handle-relative
  exclusive creation and no-replace rename. This protects a recording generation,
  while app deletion currently lacks coordination with late writers.
- `app/src-tauri/src/library.rs` reads optional sidecars inside the existing owned
  replay slot. Core summaries, descriptor-first playback, saved state and canonical
  media/timing schemas must remain compatible.

## Scope and non-goals

Implement recorder-owned post-game acquisition and restart reconciliation, exact
candidate confirmation, typed allowlisted final facts, separate immutable result
persistence, deletion coordination, optional replay projection and a compact
authoritative library result. Keep all work read-only and loopback bounded.

This feature does not change provisional context, metadata.saved, video, game logs,
historical observations, replay time, audio, Live polling, Match-V5/RSO, account
history crawling, arbitrary LCU automation, or a broad analytics UI. Recording
duration is not an acceptance condition.

## Exploration findings

- Three completed SWIFTPLAY captures show agreeing top-level, local and participant
  EOG game IDs, two teams and ten players. The local result is supported by binary
  `localPlayer.stats.WIN`, the identified team's `isWinningTeam`, and any matching
  local roster WIN. `myTeamStatus` is empty; only wins were observed, so LOSS must be
  covered synthetically and with a real session when available.
- EOG supplies duration, end timestamp, mode/type/queue labels, identity/loadout,
  player/team counters and rune data. Numeric queue/map, version and platform data
  may be absent. The bounded current-summoner history endpoint can enrich those
  fields but returns only one participant and is never scoreboard authority.
- Aborted Practice Tool sessions can return no EOG. EOG aliases may coexist and
  unrelated credentials/rewards/account fields must be discarded at ingestion.
- App-local mutation slots cannot coordinate a recorder late writer. A directory
  pin alone can still race recursive deletion; deletion must exclude the writer
  before namespace mutation and retain filesystem identity through cleanup.

## Chosen design

### Confirmation and result contract

Keep `league_match.json` schema-v1 immutable. Add a separate versioned
`league_result.json` bound to its `media_id` and candidate `game_id`. Its state is
`confirmed` with independent field-family coverage, or terminal `unavailable` with
a stable reason. Only confirmed data can project WIN/LOSS or authoritative totals.
Missing results preserve the provisional projection; unavailable results remain
explicitly unconfirmed.

Confirmation requires all of the following:

1. EOG top-level `gameId` is a positive canonical integer equal to the candidate;
2. EOG `localPlayer.gameId` agrees, and local PUUID plus full Riot ID exactly match
   the candidate and a fresh current-summoner identity;
3. explicitly flagged local roster entries agree with `localPlayer`; absent local
   flags leave corroboration unavailable, while duplicate/conflicting identities
   reject confirmation;
4. every retained participant game ID agrees, and comparable EOG/history mode,
   map and queue facts do not contradict the provisional source values; and
5. the validated credential epoch remains unchanged across acquisition.

Latest-history position, display name, timestamps, account-only matching, process
inspection, roster fingerprints and Live GameEnd are never association proof.

Result is WIN or LOSS only when local EOG WIN is a binary integer and agrees with
the identified team's boolean winning flag and any local roster WIN. Empty strings,
account counters and surrender flags cannot establish a result. Conflicts remain
unknown; mode-specific and aborted coverage is preserved.

Use a fixed allowlist for KDA/CS/level, gold, damage, healing/shielding/mitigation,
vision/wards, multikills/objectives, spells/items/positions, rune/perk IDs and
counters, plus observed mode augment/subteam fields. Bound identities to 64
participants, 16 teams, 8 items per player and fixed rune/counter limits. Serialize
nonnegative counters as canonical decimal strings and checked IDs/levels as `u32`.
Reject overflow, negative, noninteger and error-object values for each field.
Identical aliases collapse; conflicting aliases mark that field conflicted and omit
its value. Coverage records source, family, present/missing/invalid/conflicted
counts and confirmed identity. Unlisted keys and raw bodies are discarded.

### Sources and bounds

Add only these GETs to the finite LCU allowlist:

- `/lol-end-of-game/v1/eog-stats-block` as the result, roster and final-stat authority;
- `/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex=0`
  as optional enrichment after EOG confirmation.

Map history participants through `participantId` to
`participantIdentities[].player` PUUID/full Riot ID. A different newest history
entry supplies nothing and cannot select a match. Preserve the existing two-second
deadline, streamed 256-KiB body cap, one in-flight request and one joined discovery
worker. One coordinator owns at most eight pending jobs, one acquisition round and
one result writer. Each job has at most 18 EOG attempts, at least ten seconds
between attempts and a 180-second expiry; one history request may follow
confirmation. Persist consumed attempts and the next retry before HTTP. A shared
two-second round floor prevents queued jobs from bursting. Work ends after
confirmation, terminal failure, deletion, expiry or shutdown cancellation.

### Lifecycle, restart and publication

At process/HWND-driven closure, close 003 admission and snapshot its immutable
candidate. Start optional post-game acquisition alongside video/poller shutdown;
do not wait for expensive media validation before the first attempt. Retain facts in
bounded memory until healthy canonical media publication is proven. Failed or
partial video never receives a canonical result sidecar.

Per configured root, retain an exclusive OS lease on `.league-job-owner.lock` and
an atomically replaced, bounded `pending_league_jobs.json` journal. The journal
stores only the recognized bundle leaf, media UUID, creation/expiry, attempts and
retry times. Stage the job after provisional sidecar installation and before
canonical publication. On restart, read at most 64 KiB/eight jobs and revalidate
the provisional sidecar, metadata, game-log and healthy canonical video before
retrying. Never scan account history or the full library. Attempts and expiry do
not reset on restart; malformed/future/rolled-back times expire explicitly.
Duplicate notifications coalesce by media UUID and candidate identity. Missing,
deleted or wrong-generation jobs are dropped without recreating directories.

After healthy publication, install at most one bounded result sidecar through a
generation-pinned handle-relative writer with exclusive temporary creation, flush
and no-replace rename. It never rewrites metadata.saved or provisional context.
Optional journal/result failure preserves healthy media and records degraded
restart coverage.

### Late writer and app deletion protocol

Change explicit game deletion and retention to a namespace-first Windows protocol.
Open the source directory with DELETE access and without FILE_SHARE_DELETE; the
late writer's retained traverse handle uses the same exclusion. A busy delete is a
retryable error with the original bundle intact. Under retained source and games-root
handles, atomically rename the source to a reserved hidden `.league-delete-<UUID>`
leaf with no replacement, then enumerate and delete children relative to owned
handles without following reparse targets. Mark the empty owned root for deletion
through its handle before closing it. Do not drop the pin and call pathname
`remove_dir_all`.

Bound depth/count and preserve a tagged tombstone for failed cleanup and explicit
retry. Tombstones are excluded from game IDs and pending jobs. Existing saved
protection is checked under the mutation owner before deletion. Reuse existing
Windows helpers; introduce no second library authority, watcher or background
deletion service.

### Consumer compatibility

The full probe reads both bounded optional sidecars inside its existing replay slot.
A result must match core media UUID and provisional game ID before status or facts
are projected. A bare confirmed marker is invalid. Use typed final result/coverage,
player and team fields and retain IDs as strings. The viewer distinguishes
provisional, confirmed, partial and unknown. `GameSummary` exposes only a compact
authoritative result/coverage summary, read once per accepted bundle. Missing or
invalid results remain unknown and never override observed totals. ReplayDescriptor,
video mounting, replay-time and metadata/game-log schemas remain unchanged.

## Rejected alternatives and planning decisions

- Never overwrite provisional context or metadata to enrich results; capture,
  result and saved state have independent owners.
- EOG `myTeamStatus`, Live GameEnd, latest history, timestamps, process command
  lines, memory, roster fingerprints and global uniqueness claims are not proof.
- A bare status marker, unchecked JSON archive or one-participant history response
  cannot represent authoritative full-scoreboard coverage.
- Process-local app locks and directory identity alone cannot coordinate recursive
  deletion; pathname cleanup after dropping a tombstone handle is rejected.
- A bounded disposable journal provides restart recovery without a full-library
  scanner or competing match-fact database.

## Milestones

1. Define and test exact confirmation, result/coverage types and fixed field mappings
   with synthetic WIN, LOSS, aborted, mode and conflict fixtures.
2. Implement bounded EOG/history transport and the coordinator with fake time,
   restart journal validation, persisted budgets and duplicate/account/game isolation.
3. Implement the generation-bound result writer and namespace-first app
   deletion/retention protocol; exercise cross-process races in temporary libraries.
4. Integrate capture closure/publication and optional full-probe/library projections;
   preserve media-first playback, saved state and old bundles.
5. Run actual app-closed recording/EOG confirmation, delayed/absent/aborted,
   restart and consecutive-game checks; update coordinated documentation and
   completion evidence.

## Verification design

Synthetic confirmation tests cover exact large IDs, zero/missing IDs, local identity,
credential rotation, duplicate/conflicting local rows, participant mismatches,
WIN/team disagreement, missing/error/alias conflict and optional history mismatch.
Virtual-time/restart fixtures prove budgets survive restart and rollback, the
eight-job cap, global cadence, expiry, one HTTP/writer/worker and no work after
cancellation/deletion.

Dedicated Windows fixtures exercise writer/deleter exclusion in both orderings,
atomic no-clobber, directory replacement/reparse, journal/root replacement, failed
cleanup, saved preservation and deletion without recreation. Producer/consumer
fixtures verify matching media/provisional game IDs and compatibility with absent or
corrupt results. Inspect endpoint, field and logging allowlists; diagnostics remain
aggregate and sanitized.

Run the recorder all-target/all-feature check, test, fmt and Clippy; app Rust tests,
fmt and Clippy; frontend tests, check and build; and the canonical validation from
`docs/development/VERIFICATION.md`. Compare representative generated bundles with
and without bounded result sidecars for scan time, bytes, summary/IPC size and peak
memory. No media probe, network call, new scan task or post-game wait may be added
to core playback. Inject a restart during HTTP and prove its attempt remains
debited. Use dedicated temporary libraries and real recordings with the viewer
closed; record actual WIN/LOSS/aborted coverage when available without imposing a
minimum recording duration. Exact EOG equality must precede final facts, and video,
event playback and saved state must remain unchanged.

## Performance and reliability gates

Record requests, bytes, latency, peak pending jobs, journal/result sizes, worker
counts, retries and writes as aggregates. A virtual hour verifies finite memory and
work with no burst; a generated capture verifies healthy media finalization when
post-game data is missing or delayed. Optional acquisition, serialization or storage
failure cannot invalidate healthy media or block readiness. Normal video shutdown
runs in parallel with post-game work, and all ownership survives waiter cancellation.
