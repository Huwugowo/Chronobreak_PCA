# QB-DATA-003: Recording-bound League match context (v2)

Supersedes `docs/exec-plans/qb-data-003-recording-match-context.md`.
This user-requested revision changes only this ExecPlan. Before implementation,
reconcile the feature entry and execution checkpoint with this revised design;
their previous process-proof requirements are not part of this design.

## Purpose

Associate each recording, while its match is in progress, with the League match
that produced it. Successful 003 context includes both `gameId` and `queueId`.
Freeze that association and basic context alongside the video,
then expose it through replay details even when the app was closed during capture.
The captured LCU `gameId` is the durable correlation key for QB-DATA-002; a later
phase must use this identity rather than select whichever match is newest.

## Relevant planning-time architecture

- `recorder/src/service.rs` allocates the directory and UUID `media_id`, starts
  `PollerSession` after video readiness, joins collection/capture on stop, validates
  media, then publishes canonical video/metadata. Process/window lifecycle remains
  the capture authority; API availability does not start or stop recording.
- `recorder/src/poller.rs` already reads Live active-player and roster full Riot IDs,
  champion names and teams. It takes an initial joined snapshot after calibration
  and focused snapshots every ten seconds; events use a separate one-second loop.
  Its writer owns `game_log.json`. Raw full identities must be used before legacy
  display-name normalization. Current snapshot timing follows gamestats, so identity
  freshness needs the actual identity request group's monotonic start/finish.
- `recorder/src/watcher.rs` retains its minimal two-second process scan.
  `tools/client_api_capture.py` is diagnostic precedent, not a production collector;
  its broad endpoint set must not be inherited by this feature or its probe.
- `app/src-tauri/src/library.rs` strictly reads schema-v2 bundles; `save_game`
  rewrites metadata and deletion removes directories. `PlaybackProbe` supplies
  optional details through an owned bounded read slot. `ReplayDescriptor` and core
  scans stay media-first. The app coordinator does not serialize recorder writes.

## Scope and non-goals

| Phase | Responsibility |
| --- | --- |
| QB-DATA-003 | Identity feasibility probe, immutable recording association, static/basic match/player/roster context, optional sidecar and replay-details projection. |
| QB-DATA-002 | Authoritative post-game results and detailed final player/team facts, attached using the identity established by 003. |
| QB-DATA-004 | Richer timestamped Live events/player state and its polling/storage strategy. |

003 does not retrieve WIN/LOSS, end-of-game statistics or match history; design EOG
fallbacks, post-game source precedence, retries or reconciliation; increase Live
sampling; persist evolving KDA/items/gold/CS/scores; or design event-history storage.
Analytics UI and audio are outside all three phases. No Match-V5, RSO, external
backend, daemon, general database/provider framework or historical import is added.
Non-Windows context remains explicitly unavailable. Existing capture, Live history
and replay clocks retain their contracts.

## Exploration findings

LCU gameflow supplies `gameId`, queue/session context and roster PUUIDs;
current-summoner bridges the local PUUID to its full Riot ID. Live supplies independent
active-player/roster/champion observations but no `gameId`. The question is whether
these sources describe the same current match, not whether both expose its number.

Account agreement alone is insufficient. Combine it with champion/roster agreement
and repeated observations. LCU champion IDs and Live champion names need a verified
semantic mapping; field availability, bots and partial Practice Tool rosters belong
in the first probe. Do not assume raw numeric IDs and display strings are comparable.
Existing diagnostic evidence establishes field availability, not lifecycle acceptance.

The main unproven risk is stale LCU match A still reporting `InProgress` while Live
serves B for the same account. Different static fingerprints must reject. Identical
fingerprints across distinct matches remain a theoretical residual ambiguity, not
a demonstrated failure or a reason to prebuild process ownership machinery.

## Chosen design and rationale

### Identity feasibility probe first

Build a small read-only probe using typed LCU observations and an in-memory tap of
existing poller observations. Do not integrate persistence/service association before
this gate. Run under ordinary user permissions; no elevation or anti-cheat bypass.
The tap adds no Live endpoint calls, sampling or historical snapshot storage.

For ordinary real games, establish availability and comparability of gameflow phase,
nonzero `gameId`, required `queueId`, local PUUID/participant, current-summoner
full Riot identity, Live active identity/roster/champions and corresponding LCU roster
and champions. Verify the champion-key mapping from already available static context
or a narrow local static mapping; no fuzzy name matching or new Live requests.
Unknown/ambiguous mapping leaves context unavailable. Before implementing the reducer,
record the exact fingerprint, required fields, minimum roster coverage, champion
mapping and supported mode/bot/dummy semantics demonstrated by the probe. Use exact
full-roster count/champion-multiset equality where both sources reliably expose
comparable complete rosters. Otherwise select an explicit static roster/participant/
champion fingerprint supported by observed coverage and lifecycle evidence. It must
corroborate match composition in addition to local account and champion agreement;
account-only or account-plus-local-champion matching is not a fallback. If the probe
cannot establish sufficient corroboration, context remains unavailable and the design
needs review. Never drop required fingerprint fields dynamically to obtain a match.
Missing `queueId` likewise cannot pass the successful-context feasibility gate.

Exercise normal startup, recording mid-match, consecutive games, recorder restart
within one match, normal closure and collection with the app closed. Practice Tool
and reconnect are additional checks only where reproducible and supported. Observe
lifecycle transitions for stale cross-source state; do not require manufacturing it.
If it occurs naturally, compare static fingerprints and retain a sanitized regression
fixture. Inspect no mutable gameplay values for the production association contract.

The gate answers: is required LCU context reliably available during `InProgress`;
do existing Live observations corroborate it; do both remain coherent in exercised
ordinary lifecycles; and is any unresolved ambiguity actually observed? Report field
coverage, scenario coverage, comparison booleans and timing aggregates, not personal
identifiers/raw payloads. Reliable context, corroboration and lifecycle coherence,
with no unresolved observed ambiguity, permit the simplified design to proceed.
Reproducible ambiguity requires documenting the exact observations
first, then proposing only the smallest guard that resolves it. Unavailable real-client
evidence does not count as a pass; synthetic tests remain separate.

### Recording ownership and existing Live reuse

Add recorder-owned `league_client` with typed DTOs, a pure association reducer and
`MatchContextSession`. Start it after video readiness alongside the poller, off all
GPU/WGC/capture hot paths. The service supplies `media_id` and a recording-session
admission token; this rejects late work, not proves API/process ownership. No new
selected-game process handle or process-generation proof is needed for association.

Publish one latest-value Live observation containing full active Riot ID, bounded
roster identities/champions/source-native teams, sequence number, `media_id` and
monotonic request-group start/finish. Require active-player and roster success from
the same existing snapshot round; never splice cached parts from different rounds.
Malformed/missing identity marks that observation unusable but leaves existing
snapshot/event collection healthy. Keep only one current observation, no history.
A reused sequence cannot count as a second confirmation.

While unbound, attempt one LCU round per fresh Live observation, with at least ten
seconds between round starts, one request in flight and no queued rounds. Each round
reads current-summoner then gameflow. Use the latest observation after delays; never
catch up missed rounds. Both sources' request intervals must begin after admission
and be at most fifteen seconds old at comparison, measured from the oldest request
start. Two confirming rounds must have distinct increasing Live sequences and request
starts, separated by at least two and at most thirty seconds. These conservative
bounds fit the existing ten-second snapshots; the probe checks them without tuning
Live frequency. Expiration or failed/conflicting rounds clear pending confirmation.
If a newer Live observation contradicts a pair while LCU is in flight, discard that
pair rather than accept superseded evidence.

### Exact sufficient association rule

Bind only after **two consecutive coherent LCU/Live rounds** for the same open
recording and LCU credential epoch. Each round must satisfy all of these conditions:

1. Gameflow is `InProgress`, `gameId` is a valid positive integer, `queueId` is
   present and a valid nonnegative integer, local PUUID is nonempty, and exactly
   one relevant session participant has that PUUID. An explicitly supplied zero
   queue ID is allowed; missing/malformed values must never default to zero.
2. Current-summoner has that same PUUID and a nonempty full Riot ID
   (`gameName#tagLine`). Any comparable identity on the local session participant
   agrees. That full Riot ID exactly equals Live's active-player full Riot ID and
   occurs exactly once in the same Live roster. Never strip the tag or fuzzy-match.
3. The local champion agrees using the probe-verified champion keys, and the exact
   static roster/participant/champion fingerprint selected at the feasibility gate
   agrees with its required coverage. Full-roster count/champion-multiset equality,
   including duplicate multiplicities, is required only where the probe validates
   that contract. Compare full participant identities and champion assignments
   wherever both sources expose comparable information; contradictions reject.
   Missing required coverage is unavailable, never implicit agreement or a switch
   to weaker account-only/local-champion matching. Bots/dummies follow the recorded
   probe-verified semantics, not guessed player mappings.
4. The two rounds agree on `gameId`, `queueId`, local PUUID/full Riot ID and this static
   fingerprint. Map/mode values, when present and semantically comparable,
   must not contradict each other. Optional absence does not invent a value.
5. The observation freshness/order rules above hold and admission remains open.
   No malformed, conflicting or newer candidate may be mixed into confirmation.

This establishes sufficiently strong application-level agreement that the LCU
session describes the observed Live match. Live need not supply the same `gameId`.
Mutable KDA, items, gold, CS, scores and player state never participate in equality.
A stale LCU A paired with a different Live B fingerprint rejects, even for the same
account. A fully identical static fingerprint cannot prove those are different
games; the real-client probe must investigate that residual without assuming it occurs.

The reducer has unbound, pending and bound states, plus a closed admission boundary.
A changed candidate starts confirmation afresh. Older/duplicate sequences, expired
responses and results from a previous recording/credential epoch cannot advance it.
Stop closes admission before cancellation. Once bound, freeze all accepted context,
stop 003 network association work and release its observation subscription. A newer
LCU session never replaces the association; normal game closure preserves it.
Different `media_id` values may bind to the same `gameId` after a recorder restart.
No unconfirmed candidate is exposed as bound context.

### Bounded LCU discovery and transport

Discover `LeagueClient.exe` by process name/executable path and read the adjacent
lockfile outside the watcher hot loop. Validate its PID against that client; absent,
denied, stale or multiple valid candidates mean unavailable. Use one owned blocking
discovery worker, no recursive drive scans and no overlapping rediscovery; rediscover
at most every ten seconds. Do not inspect game/client command lines for credentials.
A development-only absolute `QUEUEBACK_LCU_LOCKFILE` override still requires the same
PID/path/format checks; there is no URL override.

Cap lockfiles at 4 KiB; require the recognized LeagueClient token, PID, valid port,
secret and HTTPS fields. Keep credentials only in a redacted in-memory type. Compare
parsed credentials before/after each LCU round; any change discards pending evidence.
Use private authenticated HTTPS to literal `127.0.0.1` at that port, with proxies and
redirects disabled and self-signed-certificate acceptance confined to that client.
Allow only GET `/lol-gameflow/v1/session` and
`/lol-summoner/v1/current-summoner`. Each request has a two-second total timeout and
a streamed 256 KiB body cap. Decode allowlisted typed fields; never log credentials,
raw bodies, PUUIDs or Riot IDs. No platform endpoint or broader fallback is needed.

Cancel network work alongside poller/video shutdown; abort and await the async task
if cooperative joining exceeds two seconds. Blocking discovery/filesystem workers
remain owned and joined, never detached or replaced while busy. The network deadline
does not promise forced interruption of blocked OS I/O. Optional collection failure
cannot gate video readiness, drive shutdown or emit a healthy-video capture error.

### Narrow static context and persistence

Persist one recording-scoped association: `media_id` -> required `game_id` -> required
`queue_id` -> local PUUID/full Riot ID/champion -> relevant static roster/context.
Both IDs must survive sidecar validation and replay-details projection. Missing or
invalid `queue_id` means unavailable context, not successful 003 completion; healthy
video remains valid. Never infer the queue from mode/map or defer it to DATA-002.
Represent the positive game ID as a canonical decimal string to avoid JavaScript
precision loss. It is the captured LCU correlation key, not a globally deduplicating
library key; `media_id` remains the attachment authority. PlatformID is not required.

Allow only local identity/champion, participant PUUID/full identity where available,
champion keys, source-native team membership and participant slot needed to explain
the fingerprint, plus useful available map ID/game mode. Preserve LCU `team_one`/
`team_two` and Live `ORDER`/`CHAOS` separately; do not invent a cross-source numeric
team mapping. Optional values remain absent. Cap rosters at 64 entries and strings
at 256 UTF-8 bytes; reject excess, do not truncate identity. Do not retain fields
merely because they are exposed.

Provenance records LCU gameflow/current-summoner plus existing Live identity/roster
sources, fingerprint/mapping version, successful two-round comparison and confirming
UTC time. This is application-level corroboration, not process ownership proof;
wall time is provenance, never a replay clock. Store the accepted static evidence
once, not repeated observations or mutable history. No final statistics enter this schema.

Use optional `league_match.json`, independent `schema_version: 1`, matching
`media_id`, and tagged `bound` context or `unavailable` with a stable non-sensitive
reason (discovery, transport, missing/ambiguous/conflicting identity or cancellation).
Keep the result in memory until stop. One owned writer installs at most one sidecar
before canonical publication: cap serialization at 64 KiB, write/flush a unique
sibling temporary file and atomically install without overwriting an existing target.
Never create a removed recording directory or asynchronously rewrite `metadata.json`.

Retain the existing plan's directory safety: hold a Windows directory handle denying
deletion/rename through installation, compare allocation-time volume/file identity,
reject replacement/reparse targets and revalidate identity after installation. This
pins the directory, not sibling media; the normal finalizer still validates video.
Await the writer before publication even on cancellation. A blocked write stays owned
with truthful finalizing/error state; a completed optional write failure leaves healthy
video publishable under existing checks. No writes occur after publication.

### Replay-details compatibility

Extend full `PlaybackProbe` and its TypeScript contract with optional validated static
`match_context` and availability reason. Read one capped sidecar in the existing
owned read slot after mandatory bundle validation; check its version and `media_id`.
Missing, malformed, wrong-media or unsupported sidecars cannot invalidate video.
Old recordings remain playable. Preserve selected-token/root-generation rejection.
Do not change `ReplayDescriptor`, core library scans, `metadata.json`, `game_log.json`
or timing schemas. Save/unsave never rewrites context. The UI makes no LCU calls and
receives no analytics controls. QB-DATA-002 can later read the immutable `gameId`;
its retrieval and attachment design are outside this plan.

## Rejected alternatives and planning decisions

- Mandatory WMI command-line/launch `-GameID`/`-PlatformID` parsing, native/WMI
  creation-time equality, association-only retained game handles, TCP owner tables,
  fresh Live connections and pre/post listener checks: excessive prerequisites
  without a demonstrated ambiguity. Remove their dedicated workers, parsers,
  deadlines, privacy/test machinery and mandatory scalar PlatformID equality.
  Earlier local logs showed matching launch/direct GameIDs and PlatformID tokens;
  that historical finding no longer drives production design.
- Account-only, latest-match, name-only or timestamp matching: insufficient context.
  Additional identical-account rounds do not substitute for static fingerprints.
- UI-owned/Python-child collection, broad raw archives and canonical metadata rewrites:
  conflict with recorder ownership, privacy or saved-state compatibility.
- Escalation order is application-level association -> real feasibility/lifecycle
  testing -> concrete reproducible unresolved ambiguity -> smallest resolving guard.
  Hypothetical collisions do not authorize preemptive OS proof machinery.

## Milestones

1. Implement only the scoped read-only identity probe and existing-observation tap.
   Establish required queue availability, the exact fingerprint/coverage contract
   and ordinary lifecycle feasibility before service
   integration. Preserve sanitized evidence and unresolved limitations.
2. Implement DTOs/reducer and deterministic synthetic association tests, then bounded
   LCU discovery/transport. Verify ordering, cancellation and existing Live reuse.
3. Integrate `MatchContextSession`, immutable result and single sidecar installation;
   verify optional-data failure isolation and normal closure.
4. Add optional full-probe/TypeScript projection and temporary-library compatibility.
5. Run automated, bounded-resource and integrated real-client acceptance checks;
   update durable architecture and canonical evidence only for verified behavior.

## Verification design

### Deterministic synthetic tests

Use fake LCU/Live observations, clocks and dedicated temporary libraries:

- Coherent rounds bind the expected ID; one round or a reused Live sequence cannot;
  two coherent rounds bind once. Duplicate full names with different tags stay distinct.
- Required `queueId` agrees across rounds and survives persistence/projection; missing,
  malformed or conflicting queue IDs cannot produce bound context. Explicit zero is
  distinct from missing, and queue failure leaves video healthy.
- Reject local identity/champion and roster/champion fingerprint mismatches, including
  injected stale LCU A with Live B. Test the probe-selected fingerprint's exact fields
  and coverage, full-roster equality where selected, and rejection of account-only
  fallback. Cover missing/malformed required fields, insufficient roster coverage
  and unknown champion mappings without guessing. Fully identical static
  fingerprints are the documented limit, not a fictitious distinguishable fixture.
- Delayed/out-of-order/expired responses, credential-epoch changes and late results
  after stop cannot bind incorrectly. A newer session cannot replace bound context;
  two recording IDs can bind the same match. Closure preserves accepted context.
- Transport covers lockfile validation/rotation, ambiguous discovery, authentication,
  endpoint/loopback restrictions, time/byte limits, cancellation and secret-safe errors.
  These remain relevant production paths; no removed OS-proof fixtures remain.
- Healthy video starts/finalizes without context. Sidecar tests cover absent/corrupt/
  unsupported/wrong-media data, bounds, no-clobber/atomic failures, deleted/replaced/
  reparse directories, sibling deletion, save-state preservation and no writes after
  publication. Controlled workers prove cancellation does not detach pending writes.

### Real League feasibility and acceptance

Use the small ordinary scenario set from the probe, then repeat relevant scenarios
through the integrated recorder with the app closed. Check required fields, cross-source
agreement under the probe-selected fingerprint, frozen `gameId` and `queueId` in the
sidecar and replay details, restart/same-match behavior, publication and unchanged
playback. The 003 probe and association scenarios have no feature-specific minimum
game duration; observe long enough to confirm the association and relevant transitions.
Separately, the existing global full-recorder baseline in
`docs/development/VERIFICATION.md`, under "Interactive League and Windows validation",
requires a real match of at least 25 minutes, including timing/event health checks.
Retain that requirement for integrated recorder verification; it is not a minimum
for each 003 scenario or its feasibility probe. Reuse that session for relevant 003
acceptance evidence rather than require a second duration-based run.

Do not require manual reproduction of stale A/Live B, multiple simultaneous clients,
out-of-order responses or precisely timed credential rotation. These are fixtures;
naturally observed cases become regression evidence. Record scenario/field coverage,
pass/fail comparisons, limits and resource aggregates without identifiers. Real-client
checks not exercised remain not run; synthetic passes do not substitute for them.

Run the applicable baseline after implementation:

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

Run canonical schema/graph/plan/checkpoint validation from VERIFICATION.md. Required
real-client evidence and applicable checks must pass before feature completion.

## Performance and reliability gates

One context task, one LCU request, one pending candidate/result, one latest Live value,
one discovery worker and one final writer; no queues or growing observation history.
No extra Live requests, faster sampling, per-frame work or HTTP waits on capture paths.
Existing Live polling continues after 003 association work stops.

A 60-minute virtual-clock fixture checks cadence/byte/memory bounds, no retry bursts
and no submissions after binding/cancellation. A ten-minute generated capture with
failed/delayed LCU verifies owned tasks, frame reconciliation and successful media
finalization. Record request/latency/byte/write/resource aggregates. Join already-owned
workers and reject late results. This tests collection overhead/lifecycle; it does not
replace QB-PERF-002 or establish negligible League performance impact.

## Source provenance

- [LeagueRecorder's InProgress handler](https://git.vhaudiquet.fr/vhaudiquet/leaguerecorder/commit/f90e549b1e39ae84aec943eb03f3f3e11d2c2d62?files=record-daemon/src/lqp)
  extracts gameflow `gameId`, queue and static player context in the inspected path
  without launch-ID/TCP-owner proof. This is architectural precedent, not evidence
  of Chronobreak lifecycle correctness or a recommendation to copy its broader scope.
- [LoLProxChat architecture](https://github.com/danthi123/LoLProxChat/blob/main/docs/architecture.md)
  describes deterministic room IDs derived from sorted player names, demonstrating
  roster-based match grouping. Chronobreak adds LCU's durable ID, champion/context
  corroboration and repeated confirmation; a roster fingerprint is not a universal ID.

Existing local field evidence is summarized in
`docs/product/league-data-capabilities.md`; its earlier process-proof prescriptions
are superseded by this design. Raw personal captures are not dependencies or committed
fixtures. Neither precedent replaces the real-client feasibility gate.
