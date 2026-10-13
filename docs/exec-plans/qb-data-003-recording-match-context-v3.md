# QB-DATA-003: Provisional in-game League match context (v3)

Supersedes `docs/exec-plans/qb-data-003-recording-match-context-v2.md`.
The v2 probe established that LCU team arrays and champion selections are incomplete
or mode-dependent in an ordinary bot game. This revision moves authoritative match
confirmation to QB-DATA-002 and narrows 003 to a provisional in-game candidate.

## Purpose

Associate an open recording with a provisional League session while the game is in
progress, without treating incomplete roster structures as authoritative identity
proof. Preserve the candidate `gameId` and `queueId` with the recording so
QB-DATA-002 can confirm it after the game through the post-game LCU result. Only the
confirmed result may attach final outcome or statistics.

## Relevant planning-time architecture

- `recorder/src/service.rs` owns recording directories, `media_id`, video readiness,
  poller startup/stop, finalization and canonical publication. LCU availability never
  starts or stops capture.
- `recorder/src/poller.rs` already acquires Live active-player and game data on its
  existing snapshot cadence. The 003 tap may reuse the active-player identity and Live
  game mode/map from that round; it must not add Live requests or alter the replay clock.
- Existing `recorder/src/league_client` probe code is diagnostic evidence. Its roster
  and champion comparison output is retained for historical coverage analysis, but
  those arrays are not part of the v3 production association contract.
- `QB-DATA-002` is the post-game owner for `/lol-end-of-game/v1/eog-stats-block`, exact
  candidate confirmation, final result/statistics and any late attachment protocol.
- App playback remains media-first. Optional provisional/confirmed details must be
  read through bounded existing playback slots; `metadata.json`, `game_log.json`,
  `ReplayDescriptor` and replay-time schemas remain unchanged.

## Scope and non-goals

003 does:

- collect a bounded provisional candidate while LCU reports `InProgress`;
- require exact local full Riot ID agreement between LCU current-summoner and Live
  active-player;
- require compatible map/game-mode values and preserve the source values/provenance;
- retain positive `gameId` and explicit `queueId` attached to `media_id`;
- preserve a provisional status that is visibly distinct from post-game confirmation.

003 does not:

- use `teamOne`, `teamTwo`, `playerChampionSelections`, full-roster equality, champion
  multisets, participant/champion composition or bot/dummy semantics to bind a match;
- claim that a provisional candidate is authoritative, final, or globally unique;
- request the end-of-game endpoint, attach WIN/LOSS, or persist final statistics;
- use history newest-match, dates, display names, timestamps, process command lines,
  process memory, listener ownership, Match-V5, RSO or external services;
- increase Live sampling, change capture lifecycle, or implement QB-DATA-004 telemetry.

## Exploration findings

The first real bot-game probe showed reliable simultaneous signals: LCU `InProgress`,
positive game and queue fields, current-summoner identity, Live active-player identity,
and compatible map/queue mode. It also showed only four `teamOne` entries, no
`teamTwo` entries, five selection entries, and ten Live entries including five bots.
Consequently, roster/champion arrays are useful diagnostic observations but are not a
stable in-game association contract. The post-game LCU result exposes a direct `gameId`
that can confirm the provisional candidate without depending on those incomplete
structures.

The probe remains valuable for field availability and lifecycle timing, but a passing
provisional match requires only exact local identity, `InProgress`, required IDs and
compatible map/mode. A missing optional map/mode value is unavailable for that round;
it never becomes a fabricated match or a fallback to account-only identity.

## Chosen design and rationale

### Provisional candidate contract

Each recording has an unbound, provisional and closed state. While admission is open,
one bounded LCU round reads only `/lol-summoner/v1/current-summoner` and
`/lol-gameflow/v1/session`. One existing Live snapshot round supplies active-player
full Riot ID plus Live map/game-mode values. No LCU roster arrays participate in the
candidate.

A coherent round requires:

1. `phase == InProgress`;
2. positive `gameData.gameId` and present, valid nonnegative `gameData.queue.id`;
3. nonempty local PUUID and exact nonempty current-summoner `gameName#tagLine`;
4. Live active-player full Riot ID exactly equal to that LCU identity;
5. map/game-mode values, when both sources expose comparable values, do not contradict.

Require two distinct fresh rounds with increasing Live sequence and LCU request start,
the same `gameId`, `queueId`, full Riot ID and compatible map/mode before exposing a
provisional candidate. Missing map/mode may leave the candidate unavailable for that
round; it never supplies a weaker roster or account-only fallback. Credential rotation,
stale responses, changed identity, changed game/queue, expired freshness or a newer
contradictory round clears pending confirmation. A later round cannot replace an
already-exposed candidate for that recording. Normal game closure preserves the
provisional candidate; stop closes admission before cancellation.

The candidate stores canonical decimal `gameId`, required numeric `queueId`, local
PUUID/full Riot ID, compatible source map/mode values, media ID, two-round evidence,
credential epoch and observation provenance. It explicitly carries `status: provisional`.
It is not final match identity until 002 confirms its game ID.

### Bounded transport and ownership

Reuse the v2 bounded LeagueClient discovery and lockfile validation: selected
LeagueClient executable/path and adjacent lockfile, literal loopback HTTPS, no proxy or
redirect, redacted in-memory credentials, GET-only allowlist, 2-second request deadline,
256 KiB streamed body cap, one request in flight and one owned discovery worker. Denied,
absent, ambiguous, rotated or non-Windows clients are unavailable. Do not inspect game
or client command lines. Existing Live polling remains the sole Live request owner.

LCU/Live failure never gates video readiness, stops capture, or invalidates healthy
media. Async network work is cancelled and joined; blocking discovery workers remain
owned and joined. No writes or post-publication mutation occur from 003.

### Provisional persistence and confirmation handoff

Before canonical media publication, 003 may install at most one bounded,
recording-scoped `league_match.json` with `status: provisional`, `game_id`, `queue_id`,
local identity and source/provenance fields. A missing candidate writes an explicit
stable unavailable result or omits optional context according to the existing reader
contract; healthy video remains publishable. The writer is owned, joined, no-clobber,
atomic, directory-identity checked and never recreates deleted directories. It never
rewrites `metadata.json` or app-owned `saved` state.

QB-DATA-002 later reads the provisional sidecar/candidate and acquires
`/lol-end-of-game/v1/eog-stats-block` after game closure. It confirms only when the
post-game `gameId` exactly equals the provisional decimal `gameId`; it then owns the
transition to `confirmed` and the final result/statistics attachment. A mismatch,
missing result or expired candidate remains explicitly unconfirmed/partial and cannot
attach final facts. Optional completed participant/champion data may corroborate Live
observations, but it is never required by 003 and never replaces exact `gameId`
confirmation.

### Replay compatibility

Playback exposes optional provisional or confirmed context with a status marker. Missing,
malformed, wrong-media or unsupported sidecars never invalidate an otherwise valid
schema-v2 recording. Old recordings remain playable. The UI makes no LCU calls and no
final result is displayed as authoritative while status is provisional.

## Rejected alternatives and planning decisions

- LCU `teamOne`, `teamTwo`, `playerChampionSelections`, champion mapping and bot-aware
  roster fingerprints are rejected as 003 binding requirements because the first real
  bot-game evidence showed incomplete/mode-dependent structures.
- Account-only, display-name, latest-history, date and timestamp matching remain
  insufficient. Exact local Riot ID is a necessary in-game bridge, while post-game
  exact `gameId` equality is the authoritative confirmation.
- Launch-ID/platform arguments, process memory, listener-owner checks and fresh Live
  endpoints remain out of scope; no hypothetical OS proof machinery is restored.
- 003 must not request EOG or attach final facts. 002 owns post-game confirmation,
  result/statistics precedence, delayed retries and late-write safety.

## Milestones

1. Revise the diagnostic/probe contract and synthetic fixtures to report only the
   provisional identity/map/mode gate; retain roster findings as historical evidence.
2. Implement the provisional reducer, bounded LCU round and existing Live identity tap.
3. Integrate recording admission, immutable provisional candidate and sidecar writer.
4. Extend PlaybackProbe/TypeScript with status-aware optional context compatibility.
5. Plan and implement QB-DATA-002 confirmation separately, then run integrated
   app-closed recording/post-game verification and final project checks.

## Verification design

- Synthetic rounds bind provisional candidates only with exact local Riot ID, positive
  game ID, explicit queue ID, `InProgress` and compatible map/mode; one round, stale
  identity, changed game/queue, missing queue, contradictory modes, duplicate sequence,
  credential rotation and late cancellation cannot bind.
- Fixtures prove incomplete/empty roster arrays and champion mappings do not affect the
  provisional result. The previous bot-game shape is retained as a regression fixture,
  but no roster field is required for success.
- Sidecar tests cover provisional status, canonical decimal game IDs, media identity,
  bounds, no-clobber/atomic installation, deletion/reparse replacement, missing and
  unsupported data, no writes after publication and healthy-video degradation.
- QB-DATA-002 tests exact EOG `gameId` equality, mismatches, delayed/missing EOG,
  duplicate notifications, recorder restart, consecutive matches, optional final
  participant/champion corroboration and deletion-safe late attachment.
- Real-client evidence must include startup, mid-match, consecutive games, recorder
  restart, normal closure and app-closed collection. Integrated verification must show
  provisional status during recording, exact EOG confirmation afterward, correct final
  facts and unchanged playback. The separate global recorder baseline still requires
  a real match of at least 25 minutes.

Applicable commands remain:

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
git diff --check
```

## Performance and reliability gates

At most one provisional candidate, one LCU round, one latest Live identity value, one
discovery worker and one writer exist per recording. No roster history, retry burst,
additional Live request, per-frame work or unbounded queue is introduced. A virtual
60-minute fixture verifies bounded requests/bytes/memory and no submissions after bind,
closure or cancellation. A generated ten-minute capture with unavailable/delayed LCU
must still finalize healthy video. Final completion additionally requires an integrated
app-closed recording and exact EOG confirmation without cross-recording attachment.

## Source provenance

Existing local field evidence and the sanitized first bot-game report are retained in
`docs/product/league-data-capabilities.md` and ignored `build/qb-data-003/` outputs.
They support the boundary decision but contain no committed identifiers or raw bodies.
The v2 plan remains available as superseded design history; this v3 plan is the active
implementation design after the user-requested provisional/confirmed split.
