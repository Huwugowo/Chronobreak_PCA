# QB-DATA-002: Post-game confirmation and final League facts

## Purpose

Confirm a recording's provisional League candidate after the match, then expose
the final result and useful player/team statistics from the League client's
end-of-game (EOG) data. The recorder performs a short, bounded, in-memory
post-game retry while the viewer is closed. Playback is unaffected when post-game
data is delayed, absent, contradictory or never collected. The result is optional
enrichment: a recording with no result is a normal, fully working recording.

Phase boundary: QB-DATA-003 provides a provisional in-game candidate; this
feature confirms it post-game and attaches WIN/LOSS and final stats.

## Relevant planning-time architecture

- QB-DATA-003 writes an immutable schema-v1 `league_match.json` before canonical
  publication: media UUID, canonical positive decimal `game_id`, queue ID, local
  identity, compatible map/mode and two-round provenance. It is provisional and is
  never rewritten by this feature.
- `recorder/src/service.rs` owns capture closure, media validation and publication.
  The Live poller remains the only Live request owner; LCU state and Live `GameEnd`
  never drive capture shutdown or establish Victory/Defeat.
- `recorder/src/league_client/transport.rs` already validates the LeagueClient
  process/path and adjacent lockfile, uses opaque credentials over literal loopback
  HTTPS, finite GET allowlists, bounded bodies/deadlines and joined discovery
  workers.
- The sidecar writer retains Windows directory identity and performs
  handle-relative exclusive creation and no-replace rename.
- `app/src-tauri/src/library.rs` reads optional sidecars inside the existing owned
  replay slot. Core summaries, descriptor-first playback, saved state and canonical
  media/timing schemas must remain compatible.

## Scope and non-goals

In scope: recorder-owned post-game EOG acquisition with a bounded in-memory retry
window, `game_id` confirmation, typed allowlisted final facts, a separate optional
immutable `league_result.json`, a generation-bound result writer, optional replay
projection and a compact library result summary. All work is read-only and
loopback bounded.

Out of scope: provisional context, `metadata.saved`, video, game logs, historical
observations, replay time, audio, Live polling, Match-V5/RSO, account-history
crawling, arbitrary LCU automation, a broad analytics UI, and any redesign of app
deletion or library semantics. There is no persisted job state and no recovery
after a recorder restart. Recording duration is not an acceptance condition.

## Exploration findings

- Three completed SWIFTPLAY captures show agreeing top-level, local and participant
  EOG game IDs, two teams and ten players. The local result is supported by binary
  `localPlayer.stats.WIN`, the identified team's `isWinningTeam`, and any matching
  local roster WIN. `myTeamStatus` is empty. Only wins were observed.
- EOG supplies duration, end timestamp, mode/type/queue labels, identity/loadout,
  player/team counters and rune data. Numeric queue/map, version and platform data
  may be absent.
- Aborted Practice Tool sessions can return no EOG. EOG aliases may coexist, and
  unrelated credentials/rewards/account fields must be discarded at ingestion.
- Open items, none blocking v1: (a) how a loss is encoded (`WIN: 0`, a separate
  loss flag, or an absent field); (b) whether the endpoint returns the previous
  game's block, or nothing, before the new block is ready; (c) how remakes and
  early surrenders appear. Milestone 1 records the answers from real sessions.

## Chosen design

### Confirmation

Exact match is the core requirement:

- EOG top-level `gameId` is a positive canonical integer equal to the candidate
  `game_id`.

Everything else is corroboration. Absent or empty means skipped and never fails
confirmation. Present and semantically comparable means it must agree.

Association corroborators. A contradiction here means the block cannot be trusted
to describe this recording, so the whole block is rejected and no result is
written:

- `localPlayer.gameId` equals the top-level `gameId`;
- `localPlayer` PUUID and full Riot ID equal the candidate's local identity;
- every retained participant game ID equals the top-level `gameId`;
- explicitly flagged local roster entries agree with `localPlayer`; duplicate or
  conflicting local flags reject the block.

Queue, map and mode labels are compared only when both sides use the same
representation, and a difference is an aggregate diagnostic, not a gate. The game
ID already ties the block to the candidate, so a label mismatch more likely means
a mapping bug than a wrong match.

### Result (WIN / LOSS)

`localPlayer.stats.WIN` is the primary source and must be a binary integer. The
identified team's `isWinningTeam` and any local roster WIN are optional
corroborators: missing is fine, and disagreement makes the outcome `unknown` while
keeping the stats. A missing, empty or non-binary primary yields `unknown`. Account
counters cannot establish a result. Early-surrender/remake indicators never
establish WIN or LOSS; when present they set `ended_early` and the outcome stays
`unknown` unless the primary and corroborators clearly agree.

Mapping a non-win to LOSS is enabled only after Milestone 1 records the real loss
encoding, with fixtures built from it. Until then such a result stays `unknown`.

### Result file and typed facts

Add a separate versioned `league_result.json` bound to `media_id` and `game_id`.
It is written only when confirmed; absence means unknown, and no "unavailable"
file exists. `game_id` is a decimal string, matching `league_match.json`. Ordinary
stats and counters are normal typed numeric fields.

Fixed allowlist: KDA/CS/level, gold, damage, healing/shielding/mitigation,
vision/wards, multikills/objectives, spells/items/positions, rune/perk IDs and
counters, plus observed mode augment/subteam fields. Bounds: 64 participants,
16 teams, 8 items per player, fixed rune/counter limits, and a 256 KiB file cap.

Parsing uses a purpose-built DTO with optional fields and lenient per-field
handling. Unlisted keys, credentials, rewards and raw bodies are never read into
the result. An invalid, negative, overflowing or error-object value becomes null
for that field. Identical aliases collapse and conflicting aliases null the field.
Coverage is one coarse flag per field family (present, partial or missing) plus
the confirmed identity; no per-field counts.

### Source and bounds

Add one GET to the finite LCU allowlist: `/lol-end-of-game/v1/eog-stats-block`.
There is no history endpoint in v1. A game-ID-keyed endpoint can be added later if
the client's own API spec shows one that adds real value. The existing two-second
deadline, streamed 256 KiB body cap, one in-flight request and one joined
discovery worker are preserved.

Each recording has one in-memory job: at most 18 attempts, at least ten seconds
apart, 180-second expiry from closure. At most four jobs are pending, oldest
dropped on overflow. A `gameId` mismatch, an empty response or a not-ready error
means "not yet" and is retried until expiry; because association is by exact ID,
a stale block can never attach to the wrong job. Work ends on confirmation,
association rejection, expiry, deletion, shutdown or failed publication.

### Lifecycle and publication

At process/HWND-driven closure, close 003 admission and snapshot its immutable
candidate. Start acquisition alongside video/poller shutdown without waiting for
media validation. Hold confirmed facts in bounded memory until healthy canonical
media publication is proven; a failed or partial video never receives a result
sidecar. If the recorder exits first, the facts are lost and the recording keeps
its provisional context. Nothing is persisted for recovery.

### Result writer and deletion

After healthy publication, install at most one result sidecar through the existing
generation-pinned, handle-relative writer: exclusive temporary creation, flush and
no-replace rename. It never rewrites `metadata.saved` or provisional context.

Deletion wins, and app deletion and retention are unchanged:

- the writer never creates the bundle directory or any parent;
- its directory handle allows share-delete, so it can never block an app delete
  (verify the existing helper's sharing mode);
- immediately before the rename it revalidates directory identity, media UUID,
  the provisional game ID and the presence of healthy core media;
- on any mismatch or error it removes its own temporary file and abandons the
  write, counted in aggregate diagnostics.

A narrow race can leave an empty directory or stray file after a concurrent delete.
This is accepted and documented: the library lists only bundles with valid core
media, and existing delete/retention handles the rest. Any result-write failure
leaves healthy media and readiness untouched.

### Consumer compatibility

The full probe reads the optional result inside its existing replay slot. It must
match the core media UUID and provisional game ID before any status or facts are
projected; a bare marker, a mismatch or invalid JSON is treated as unknown. The
viewer distinguishes provisional, confirmed, partial and unknown. `GameSummary`
exposes only a compact outcome/coverage summary, read once per accepted bundle.
A missing or invalid result never overrides observed totals. ReplayDescriptor,
video mounting, replay time, and the metadata/game-log schemas are unchanged.

## Rejected alternatives and deferrals

- Overwriting provisional context or metadata to enrich results: capture, result
  and saved state keep independent owners.
- A persistent pending-job journal, restart recovery, persisted attempt budgets,
  clock-rollback handling and an eight-job coordinator: disproportionate for a
  180-second optional enrichment whose failure costs one missing result.
- A namespace-first Windows deletion protocol with tombstones: a separate
  library-hardening effort, not required by an optional writer that abandons on
  mismatch. Revisit independently if deletion races prove real.
- "Latest" current-summoner history enrichment: one participant, can select nothing,
  and duplicates data 003 already carries.
- Credential-epoch tracking and a fresh current-summoner fetch: the exact ID check
  and local-identity corroboration already cover account swaps.
- An `unavailable` sidecar: an absent file already means unknown.
- Decimal-string counters and per-field coverage counts: add parsing cost without
  safety. Only `game_id` stays a string.
- EOG `myTeamStatus`, Live `GameEnd`, timestamps, process command lines, roster
  fingerprints and global-uniqueness claims are not proof. A clearly labelled
  provisional Live outcome could be revisited as a separate feature.

## Milestones

1. Contract and parsing: define the DTO, confirmation tiers, outcome rules and
   result types with synthetic fixtures. Record the real loss encoding, stale-block
   behavior and remake/early-surrender shape from live sessions.
2. Transport and retry: add the EOG GET to the allowlist; implement the in-memory
   job with fake time, retry-on-mismatch, caps, expiry, cancellation and one
   in-flight request.
3. Writer and integration: implement the generation-bound result writer, wire
   capture closure and publication ordering, and add the optional full-probe and
   `GameSummary` projections. Preserve media-first playback, saved state and old
   bundles.
4. Real-game validation and documentation: run actual app-closed recordings
   covering WIN, LOSS, remake or aborted, delayed EOG and back-to-back games;
   update coordinated documentation and completion evidence.

## Verification design

Synthetic confirmation tests cover:

- exact large IDs and zero/missing/noncanonical IDs;
- the two contradiction tiers: association rejection versus outcome-unknown;
- absent/empty corroborators skipped, and present-but-disagreeing ones handled;
- WIN, synthetic LOSS (after the encoding is known), remake, aborted and mode
  augment fixtures;
- missing, error-object and alias-conflict fields;
- duplicate/conflicting local roster flags and participant ID mismatches.

Fake-time retry tests prove mismatch and empty responses are retried until expiry,
the attempt and pending-job caps hold, the cadence has no bursts, one request is in
flight, and no work continues after cancellation, deletion or failed publication.

Writer tests, using dedicated temporary libraries, cover: bundle deleted before and
between temporary creation and rename (never recreated, temporary removed);
no-clobber; directory or media-generation mismatch; replaced directory; and the
writer never blocking an app delete. Producer/consumer tests verify matching media
and game IDs, and compatibility with absent, corrupt or mismatched results and with
old bundles. Inspect the endpoint, field and logging allowlists; diagnostics stay
aggregate and sanitized.

Run the recorder all-target/all-feature check, test, fmt and Clippy; app Rust
tests, fmt and Clippy; frontend tests, check and build; and the canonical
validation from `docs/development/VERIFICATION.md`. Compare representative bundles
with and without result sidecars for scan time, bytes, summary/IPC size and peak
memory. No media probe, network call, new scan task or post-game wait may be added
to core playback. Real recordings use the viewer closed; record actual
WIN/LOSS/aborted coverage when available without imposing a minimum duration.
Video, event playback and saved state must remain unchanged.

## Performance and reliability gates

Record requests, bytes, latency, attempts, peak pending jobs, result size and writes
as aggregates. A virtual hour verifies finite memory and work with no burst. A
generated capture verifies healthy media finalization when post-game data is
missing or delayed. Optional acquisition, serialization or write failure cannot
invalidate healthy media or block readiness. Normal video shutdown runs in parallel
with post-game work, and the job's cancellation and cleanup do not depend on any
waiter.
