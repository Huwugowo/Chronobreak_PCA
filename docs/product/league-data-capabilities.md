# League data capabilities and collection boundaries

Evidence date: 2026-09-29. This is a source/consumer inventory, not a feature-status
ledger or a promise that every field works in every League mode. `feature-list.json`
owns scope and lifecycle; each finalized ExecPlan owns its implementation design.

## Product intent

Preserve useful gameplay facts for recording, replay navigation, match/clip search,
descriptive state and possible later graphs/cross-match statistics. Collection must
work with the viewer closed. Do not implement an analytics UI, coaching, competitive
overlay, arbitrary client automation, Match-V5, RSO, or an account-history crawler in
this work. Audio is explicitly deferred. Future external sources may attach facts to
the same match identity; no speculative provider framework is required now.

Completeness means documented coverage, provenance and honest missingness, not an
unfiltered archive of every client response. Credentials, chat, social/presence data,
server connection details, client rewards and unrelated account state are excluded.
Riot IDs/PUUIDs used to bind participants are personal identifiers: keep them local,
out of logs and public fixtures, and delete their attachments with the recording.

## Evidence and limits

The ignored captures under `build/client-api-captures/` were inspected structurally
and read-only; no raw response bodies or real account identifiers belong in Git.
These observations describe the captured client versions, not API guarantees.

| Capture | Observed match classes | Useful evidence |
| --- | --- | --- |
| `20260928T102458Z-bc38ea44` | Completed queue 890 / SWIFTPLAY, 711 seconds | Live events/state, gameflow session, end-of-game block, history summary |
| `20260928T112402Z-e19798d4` | Aborted Practice Tool, 86 seconds | Live state; history exists without end-of-game block |
| `20260928T125731Z-d0d63913` | Aborted Practice Tool, 89 seconds | Same partial/post-game availability distinction |
| `20260928T141915Z-710ae41f` | Completed queue 890 / SWIFTPLAY, 694 seconds | Full end-of-game player/team statistics |
| `20260929T082237Z-90fdbbde` | Aborted 37-second practice plus completed 536-second queue 890 match | Consecutive sessions in one capture, with distinct match IDs |

The diagnostics save changed successful responses and HTTP-status transitions, not
one row per request. Use `summary.json` for total polls when present. Existing
10-second state samples cannot establish the benefit or cost of a 1-second cadence.

Across the three completed matches, end-of-game responses contained two teams and
ten players, with at least 183 stat keys per player (including upper/lower-case
aliases, not 183 distinct guaranteed statistics). The captured history endpoint
contained only one participant and two team entries. Do not treat history as a
complete substitute for the end-of-game roster/statistics. Both sources' game IDs
matched gameflow observations in these captures.

Successful response sizes observed: gameflow session roughly 6-9 KiB, end-of-game
roughly 63 KiB, history roughly 4 KiB, Live aggregate roughly 6-55 KiB, and cumulative
events up to roughly 16 KiB. These are payload observations, not CPU or polling-budget
measurements. Synthetic long-match/resource checks remain necessary.

## Consumer-first inventory

| Consumer / future use | Available source and semantics | Currently retained / gap | Collection class |
| --- | --- | --- | --- |
| Match association, queue/map filters | LCU gameflow `gameData.gameId`, queue ID/mode/map; current summoner PUUID/Riot ID; history platform/version | Recorder retains mode and local name/champion/team, not durable LCU match/account identity | Static session identity; validate during recording, never attach merely the latest history entry |
| Participant display, matchup/search | Live roster has full Riot ID, champion/team, position, bot/death flags; LCU session/post-game supplies participant IDs/PUUIDs | Persisted snapshots lose explicit full Riot ID; app legacy name normalization strips tags | Static/observed roster, with source-specific IDs and partial coverage |
| Loadouts | Live spells/basic runes for roster and full runes/ability levels for active player; LCU post-game spell/rune IDs | Existing first snapshot retains spells/runes partially; discarded fields need inventory-based selection | Store stable fields once, retain meaningful changes; post-game facts are not historical observations |
| Event timeline, navigation and clipping | Live cumulative `EventID`, `EventName`, `EventTime`, event-specific actors/objectives | Events are polled each second, deduplicated and mapped through canonical game/replay time; `FirstBlood.Recipient` is not in normalized event contract | Incremental event ingestion preserving source identity and actual event time |
| State at playhead, progression | Live roster scores (KDA/CS/ward score), level, inventory, death/respawn state | 10-second snapshots keep CS/level/items; many other available fields are discarded. Item/level changes are only snapshot-time observations | Sampled state with observation intervals/gaps; not precise purchase/sale events |
| Local-player descriptive graphs | Live active-player current gold, health/resources, ability levels and combat stats | Only current gold and health/max health are kept, at current snapshot cadence | Candidate faster local-state stream; measure benefit/cost before fixing cadence |
| Final scoreboards and future per-match aggregates | LCU end-of-game result/team flags, roster, damage dealt/taken, healing/shielding, gold earned/spent, vision/wards, CS/KDA, multikills, objective totals, rune/stat counters | Not ingested in production; useful broad post-game statistic set is empirically present | Acquire after game, validate match ID, retain available facts once |
| Local cross-match statistics | Derivable from the user's collected recordings and validated final facts | No new account-history/network crawler is needed to retain inputs | Later rebuildable consumer; no persistent analytics database now |
| Clip tags/source linkage | Existing event log plus `media_id` and canonical clip frame interval | Clip-side metadata is separately owned by QB-LIB-002/003 | No additional League polling |

## Important distinctions

- LCU account identity is not authoritative recording-to-match proof. QB-DATA-003 may
  retain exact local Riot ID plus InProgress/game/queue/map-mode agreement as a
  provisional in-game candidate, but QB-DATA-002 must confirm its gameId through the
  post-game LCU result before final facts are attached. Follow-up review inspected
  37 session bodies and 213 Live aggregates: LCU exposes `gameData.gameId`, but Live
  game data exposes only mode/time/map fields. LCU `gameClient` has running/visible
  and server/observer address/port fields, not a game PID or process generation.
  Fresh Live data for B and stale LCU data for A can therefore agree on the same
  account and roster. Repeated agreement does not make that match link authoritative.
  A Windows listener-owner check can separately associate Live with the selected
  process, but cannot supply the missing shared LCU/Live match key. The selected-game
  launch identity research below is superseded history. The active user-directed
  contract is provisional in-game identity followed by exact post-game EOG gameId
  confirmation; unconfirmed candidates cannot authorize final facts in QB-DATA-002.
- Current gold is spendable gold, not gold earned. Inventory price is not an exact
  substitute for total/team gold. Do not synthesize missing enemy/team gold histories.
- Detailed active-player health/combat stats do not imply equivalent data for every
  player. Our roster responses do not expose complete per-player health/gold/combat
  time series.
- Final damage/gold/ward/spell-cast totals cannot reconstruct when each action occurred.
  History `timeline` has named delta/lane/role fields in our responses; field presence
  alone does not prove useful interval values or a full Match-V5-style timeline.
- No verified source here provides a complete movement/position trace, ward locations,
  per-hit damage stream, ability cooldown/cast history, or all-player gold graph.
  Mark these unavailable/unverified; do not infer them from video, memory, or hooks.
- An inventory difference establishes an observed change, not necessarily a purchase:
  upgrades, transformations, undo, slot moves and consumables require truthful labels.
- Practice dummies can have HTTP-200 player entries with nested error objects. Missing,
  errored and zero are different facts. Static field names are not guarantees of values.
- Different spell/rune/role forms may need normalization; unknown values must not become
  plausible defaults. Mode-specific stats and duplicate aliases require tested mapping.

## Historical superseded process-proof research

The following describes v1-era research. It is not an active collection mechanism:
v3 excludes process command lines, platform endpoints and listener-owner proof.

Date/start-time correlation was checked across the six captured games. History has
`gameCreation` (epoch milliseconds), matching `gameCreationDate`, and `gameDuration`
(seconds); end-of-game has `endOfGameTimestamp` and `gameLength`. Those observations
were post-game, not an in-progress session start timestamp. Median local response
receipt minus Live `gameTime` was 4.995-21.127 seconds later than history creation.
Two early samples shifted their inferred origin by 5.349 and 13.375 seconds. After
gameTime reached one second, per-game origin spread was 0.001-0.013 seconds. In the
three completed games, EOG end time minus inferred origin and game length was
0.334-0.826 seconds. These are useful retrospective consistency observations, not
universal tolerance bounds or match-instance proof across pauses/clock adjustments.

Seven recent installed-client r3d logs each contained one direct non-command-line
numeric GameID and exactly one `-GameID=<decimal>` launch argument; they agreed in
all seven. Six IDs matched the six games in existing LCU captures; the seventh log
had no matching capture coverage. Each launch record also had exactly one
`-PlatformID=<ASCII alphanumeric>`. No ID, token, address, player name or raw log line
was exported. There was no proven selected-game-PID-to-log mapping; newest-log/date
selection must not become a production identity source.

The proposed direct link is the already-captured process's launch GameID/PlatformID,
read through a narrowly PID-scoped Windows process-metadata query, compared with LCU.
A separate acquisition-time TCP owner check ties Live observations to that same
process generation; identity requests cannot reuse an old pooled connection.
Microsoft documents the read-only process properties and owner-PID table mechanisms:
[Win32_Process](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-process),
[GetExtendedTcpTable](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable).
This is a narrow expansion of local identity evidence, not gameplay memory inspection
or broad command-line collection. Raw arguments are sensitive and never persisted.

A self-process WMI query was denied in the sandbox and succeeded outside it; only
availability booleans were printed. No League process was running, so ordinary-user
League/Vanguard accessibility is not yet proven. The proposed scalar LCU platform
route `/lol-platform-config/v1/namespaces/LoginDataPacket/platformId` has original
[client-source usage](https://github.com/vickz84259/lol-highlights-enhancer/blob/master/data_manager.py),
but was not captured or verified against this installed client's API. Its literal
key was not found in two inspected client logs; absence there is not endpoint absence.
The v2 design therefore requires an actual-client feasibility gate before integration,
not a claim that these mechanisms already work in production.

## Existing architecture and work ownership

`recorder/src/poller.rs` already owns calibrated events, snapshots, derived changes and
the coalesced atomic game-log writer. `QB-REPLAY-012` owns the exact schema-v2 clocks;
do not introduce a second timing model. The app already consumes events and some
derived state. Inspection also found state computed but not visibly used by current
viewer components; QB-REPLAY-004 needs a scoped consumer review, not more polling to
compensate for a UI gap.

`metadata.json` and `game_log.json` have strict app readers. The app rewrites metadata
when saving/unsaving, so asynchronous LCU enrichment must not become a second metadata
writer. App deletion is not synchronized with the recorder. The foundation therefore
uses a separate versioned `league_match.json`, bound to `media_id`, and seals it before
canonical publication. Post-game updates require their own deletion-safe ownership
design before implementation. Optional League enrichment must not gate video playback.

Implementation order:

1. **QB-DATA-003:** reliable recording/match identity and optional static context.
2. **QB-DATA-002:** authoritative post-game result and statistics, explicitly expanded
   from its original result-only scope; safe late attachment and partial reconciliation.
3. **QB-DATA-004:** richer timed Live observations, event completeness and measured
   cadence/persistence, using the existing canonical clocks.

These are collection/data contracts. Analytics UI, category lanes, clip metadata and
playhead presentation remain separate consumers. QB-DATA-001 remains the baseline;
its original evidence predates the current schema-v2 timing implementation.

## Collection design rules

- Use finite read-only endpoint allowlists, loopback-only authentication, no proxy or
  redirects, bounded response sizes/timeouts/concurrency, and no raw-body logging.
- Prefer stable metadata once, cumulative events incrementally, sampled state only as
  frequently as its fidelity requires, and bounded post-game reconciliation.
- Never fetch overlapping aggregate and focused endpoints for the same steady-state
  facts without measured justification. Compare bytes, latency, request count, parsing,
  persistence cost and missed transitions rather than optimizing only request count.
- Track source, phase/observation time, coverage and stable missing/error reasons.
  Preserve useful allowlisted gameplay facts for future consumers; discard secrets and
  unrelated fields at ingestion. Raw capture remains an explicit diagnostic tool.
- A failed API, identity match or optional write cannot stop otherwise healthy video.
  Do not use LCU phase or Live events as replacement recorder start/stop authority.
- Existing recordings remain usable. New sources may enrich missing facts but cannot
  invent historical observations, rewrite media, or silently remap participant identity.

## Public-source boundary

Riot documents Live Client data and recommends subset endpoints when aggregate data is
unnecessary. It explicitly describes LCU as unsupported for third-party use, without
stability guarantees. Revalidate empirical behavior and track applicable distribution
requirements before release; no API account/key or registration change is made here.
Source: [Riot League API documentation](https://developer.riotgames.com/docs/lol).
