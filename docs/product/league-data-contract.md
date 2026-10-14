# Chronobreak League data contract

Status: authoritative product-to-data contract, accepted by the user.
Consolidated: 2026-10-14.

This document consolidates the agreed feature-to-data and endpoint acquisition
matrices. It owns the requirements, source semantics and collection decisions for
this scope. It supersedes earlier conversational matrices and conflicting older
requirements for this scope, including collection recommendations in the historical
[capability inventory](league-data-capabilities.md). Other repository requirements
files are not updated by this documentation-only task; this document is the accepted
reference for the revised data scope.
It does not claim that proposed behavior is implemented, replace documentation of
current runtime behavior, or select tables, migrations or an implementation plan.

## Scope and reading rules

The scope is League match identity/context, final statistics, match-history search,
replay overlays, event timelines, historical roster states, synchronization and
automatic clips. Sources are local LCU, Live Client Data API and recorder data only.
Match-V5, account-history crawling, gold graphs and continuous detailed active-player
telemetry are outside this contract. Arena may remain functionally unsupported;
teams, participants, objectives and telemetry availability must remain structurally
extensible across modes.

The contract distinguishes:

- **Observed context:** identity, roster and loadout facts observed from a source;
  apparently static values may change and retain their observation evidence.
- **Final facts:** authoritative post-game values associated through the existing
  exact EOG game-ID confirmation contract. They do not reconstruct history.
- **Source-timestamped events:** source event time is distinct from poll receipt.
  Canonical microsecond representation does not guarantee microsecond source accuracy.
- **Sampled states:** values observed at a response, with request timing and coverage.
  Intermediate states may never have been observed.
- **Inferred changes:** differences between observations, bounded by their timing and
  gaps; not fabricated exact purchases, level-ups or respawns.
- **Derived data:** reproducible clusters, tags and clip selections referencing their
  inputs and rules; not substitutes for source facts.

Every retained fact preserves provenance, missingness and actual temporal precision.
Zero, empty, absent, errored, invalid, conflicting and not applicable are distinct.
An absent field alone does not establish that it is not applicable to a mode.

In matrix 1, **P** rows describe product-facing facts/behaviors and **T** rows describe
supporting technical data. IDs remain stable from the accepted matrix. P35-P39 and
P66-P67 are retired from current requirements: active-player gold, health, resources,
combat/penetration/sustain statistics and ability telemetry. AP-derived full runes and
level observations are also removed; final EOG statistics and PL basic runes remain.

Endpoint abbreviations, expanded in matrix 2:

- **GS:** `/liveclientdata/gamestats`.
- **AP:** `/liveclientdata/activeplayer`, retained for bounded identity checks only.
- **PL:** `/liveclientdata/playerlist`.
- **EV:** `/liveclientdata/eventdata`.
- **GF:** `/lol-gameflow/v1/session`.
- **SUM:** `/lol-summoner/v1/current-summoner`.
- **EOG:** `/lol-end-of-game/v1/eog-stats-block`.

Persistence abbreviations: **GL** = `game_log.json`, **LM** = `league_match.json`,
**LR** = `league_result.json`, **M** = `metadata.json`. **E** denotes the source event
envelope `{EventID, EventName, EventTime}`. EOG statistic names below refer to player
or team `stats` objects where supplied; the exact supported uppercase/lowercase alias
mapping is in [the shared stat contract](../../league-data/src/lib.rs).

“Currently acquired/persisted” describes the inspected implementation, not the
proposed contract. **Wire only** means production requests a response containing
those fields but discards them during parsing. **Parsed** means the code supports a
field; it does not prove that every installed client or mode supplies it. A persisted
nullable field is not evidence that a real capture populated it.

## Evidence and current implementation boundary

The requirements were checked against the [Match History UI specification](../ui/match-history.md),
[feature definitions](../../feature-list.json), [recorder poller](../../recorder/src/poller.rs),
[identity association](../../recorder/src/league_client/association.rs),
[LCU ingestion](../../recorder/src/league_client/result.rs),
[post-game coordinator](../../recorder/src/league_client/post_game.rs),
[shared final-stat model](../../league-data/src/lib.rs),
[library projection](../../app/src-tauri/src/library.rs),
[library UI](../../app/src/components/LibraryScreen.tsx),
[viewer](../../app/src/components/ViewerScreen.tsx),
[overlay](../../app/src/components/FullscreenOverlay.tsx), and
[canonical replay-time contract](../architecture/replay-time.md).

The committed `live-client-capture-20260730-110603.json` contains 25 aggregate
Practice Tool observations, including nested error objects for practice dummies.
It establishes observed shapes, not cross-mode guarantees. Its events only establish
`GameStart` and `MinionsSpawning` in that capture. The historical capability inventory
summarizes additional ignored captures from another checkout; those raw files were
not available for this consolidation. Keep identifiers out of logs and new public
fixtures; examples and tests must use sanitized or synthetic identities.

The current production poller still requests `/activeplayer` in joined 10-second
state rounds. Its identity supplies local roster matching and the LCU association tap.
The app reader discards the recorded gold, HP and full-rune fields; those fields have
no current rendered consumer requiring continuous acquisition. Removing the request
from normal state rounds is a future change, not an accomplished result of this document.

Current history searches champion/mode and offers champion/mode/saved filters. Its
KDA comes from recorded events and its inventory from the last sample, despite final
facts being retained separately. The viewer exposes some final facts, while computed
state props are not all rendered by the overlay. These are consumer gaps, not reasons
to add polling. The history UI's two five-player rosters and seven item slots are
presentation assumptions, not universal storage cardinalities.

Public evidence:

- [Riot Game Client documentation](https://developer.riotgames.com/docs/lol?from=20423&from_column=20423)
  documents focused endpoints, the active-player `riotId` field,
  per-player `riotId` queries and local Swagger/OpenAPI retrieval. It recommends
  selecting response subsets when the aggregate is unnecessary. LCU is explicitly
  unsupported for third-party use and has no equivalent stability guarantee.
- [Riot event examples](https://static.developer.riotgames.com/docs/lol/liveclientdata_events.json)
  and [aggregate example](https://static.developer.riotgames.com/docs/lol/liveclientdata_sample.json)
  provide example shapes, not exhaustive current-mode coverage or event-ID sequencing guarantees.
- A [historical captured Swagger schema](https://gist.github.com/xadamxk/768d3fa9844012c809af22d9a9d55af0)
  advertises `eventID` on EV/aggregate and `teamID` on PL. This is historical schema
  evidence, not verification of the installed client's behavior.

## Matrix 1: feature-to-data contract

Normal candidate intervals are **GS 1 s, EV 1 s and PL 2 s**. All useful retained
fields in a response follow that endpoint's cadence. Rates are unmeasured candidates,
not performance guarantees. AP is identity-only bootstrap/association work, not a
continuous telemetry stream. Fast GS calibration is a separate phase, as specified
in matrix 2.

| Feature / behavior | Exact fields | Source / endpoint | Temporal precision and polling target | Currently acquired? | Currently persisted? | Gap / action needed | Feasibility / uncertainty |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **P01 · Match identity** | `gameData.gameId`; EOG `gameId`; recording `media_id`; association status | GF, EOG, recorder | Observed context: GF 10 s during association; final confirmation: EOG 10 s during reconciliation | Yes | LM provisional identity; LR confirmed identity | Expose provisional/confirmed/unknown consistently to queries | Live has no shared game ID. Repeated agreement remains provisional until exact EOG confirmation |
| **P02 · Queue recognition and search** | `gameData.queue.id`; EOG `queueId`, `queueType` | GF, EOG | Context/final fact; respective 10 s windows | Yes | LM, LR | Project queue fields into history/search; preserve conflicting labels with provenance | Do not derive ranked/draft identity from `gameMode` alone |
| **P03 · Game-mode recognition** | GS `gameMode`; GF `gameData.queue.gameMode`, `map.gameMode`; EOG `gameMode`, `gameType` | GS, GF, EOG | GS observed every 1 s normally; bounded LCU context/final observations | Yes | Mode/context in M, LM, LR | Preserve source distinctions and unknown values | Source labels need not be identical |
| **P04 · Map identity** | GS `mapNumber`; GF `map.id`, `gameData.queue.mapId`; EOG `mapId` | GS, GF, EOG | GS 1 s normally; context/final facts | Yes | LM, LR when available | Make map queryable without assuming map 11 | Map identity alone does not establish objective applicability |
| **P05 · Local-player identity** | AP `riotId`; SUM `puuid`, `gameName`, `tagLine`; exact PL `riotId` match and local participant reference | AP, SUM, PL | Identity bootstrap, then fresh AP observations for 10 s association rounds until bound/closed; PL 2 s | Yes: existing AP `riotId` and SUM path work | Full local identity in LM; legacy local name in M/GL | Retain the working endpoint for bounded identity checks; keep two fresh coherent rounds; use identified PL entry for name/champion/team | Reject missing/tagless values; never weaken matching. Endpoint replacement requires verified compatibility and measured benefit |
| **P06 · Participant identity and opponent search** | PL `riotId`, `riotIdGameName`, `riotIdTagLine`, `summonerName`; EOG `puuid`, `riotIdGameName`, `riotIdTagLine` where supplied | PL, EOG | PL 2 s; EOG final | Partial: Live full ID primarily used for local matching | GL legacy names; LR combined Riot IDs; participant PUUIDs not retained | Persist per-match participant references and available source identifiers; replace tag-stripping joins | Some participants/dummies have incomplete identities; never guess joins |
| **P07 · Champion display and champion/opponent filters** | PL `championName`; EOG `championId`, `championName`; participant reference | PL, EOG | PL 2 s; EOG final | Yes | GL, LR | Index local and roster champions separately; resolve display identity consistently | Live names and final numeric IDs have different provenance |
| **P08 · Team rosters and ally/enemy relationships** | PL `team`; EOG `teamId`; participant/team membership; local team reference | PL, EOG | PL 2 s; EOG final | Yes | GL, LR | Preserve arbitrary team collections; derive relations only with known local membership | Current history layout assumes two five-player rosters; storage must not |
| **P09 · Summoner spells and historical loadout** | PL `summonerSpells.{summonerSpellOne,summonerSpellTwo}.{displayName,rawDisplayName,rawDescription}`; EOG `spell1Id`, `spell2Id` | PL, EOG | PL 2 s sampled; EOG final | Partial Live parsing; EOG IDs parsed | Initial GL spell names; final LR IDs | Retain identity fields and observed changes; retry missing initial values | Final spells cannot backfill earlier changes |
| **P10 · Keystone and roster rune styles** | PL `runes.{keystone,primaryRuneTree,secondaryRuneTree}.{id,displayName,rawDisplayName,rawDescription}` | PL | Sampled loadout, 2 s | Keystone ID parsed; other fields wire only | First GL keystone only | Retain styles/observed changes and nested errors | Basic roster runes do not provide everyone's full rune page |
| **P11 · Final runes and rune counters** | EOG `PERK0…5`, `PERK0…5_VAR1…3`, `PERK_PRIMARY_STYLE`, `PERK_SUB_STYLE` | EOG | Final facts/counters; 10 s reconciliation | Parsed | Structured perks/styles in LR | Preserve available groups/styles/counters and missingness | No AP full-rune acquisition requirement. Final counters do not establish activation timestamps |
| **P12 · Recording date and match end date** | Recorder `recorded_at`; EOG `endOfGameTimestamp` | Recorder, EOG | Distinct wall-clock facts | Yes | M, LR | Query and label each separately | Recording start is not authoritative match start |
| **P13 · Match duration versus video duration** | EOG `gameLength`; recorder validated media duration/frame coverage | EOG, recorder | Final match duration; exact media coverage | Yes | LR, M | Keep separate in history/search | Late recording start cannot establish full match duration |
| **P14 · Authoritative outcome** | Local EOG `stats.WIN`; corroborating roster `WIN`, team `isWinningTeam`; normalized outcome | EOG | Final fact; 10 s reconciliation | Yes | LR outcome | Validate real loss encoding; expose unknown explicitly | Code accepts corroborated `WIN:1`; `WIN:0` remains unknown pending evidence |
| **P15 · Early termination / surrender facts** | EOG `invalid`, `gameEndedInEarlySurrender`, `teamEarlySurrendered`, `causedEarlySurrender`, `earlySurrenderAccomplice` | EOG | Final facts | Yes | Collapsed into LR `ended_early` | Preserve individual flags and missingness | These flags do not yet establish a validated remake classification |
| **P16 · Final K/D/A and history ratio** | EOG `CHAMPIONS_KILLED`/`kills`, `NUM_DEATHS`/`deaths`, `ASSISTS`/`assists`; participant reference | EOG stats | Final counters | Yes | LR | Use final counters for final history summary; define zero/missing-death ratio handling | Current history uses recorded events; incomplete capture cannot establish final KDA |
| **P17 · Final build** | EOG player `items[]`, preserving supplied positions/null entries | EOG | Final inventory | Yes | LR | Feed final history build from final inventory when available; distinguish last observed build | UI has seven slots; parser accepts up to eight. Slot meaning needs verification |
| **P18 · Final level and farming totals** | `LEVEL`/`champLevel`, `MINIONS_KILLED`/`totalMinionsKilled`, `NEUTRAL_MINIONS_KILLED`/`neutralMinionsKilled`, `totalAllyJungleMinionsKilled`, `totalEnemyJungleMinionsKilled`; existing uppercase aliases | EOG stats | Final counters | Parsed | LR nullable values | Expose available final facts with field-level missingness | Individual aliases not all capture-verified; no final-level badge required by current history UI |
| **P19 · Final economy totals** | `GOLD_EARNED`/`goldEarned`, `goldSpent` | EOG player/team stats where supplied | Final counters | Parsed | LR | Preserve scope and missingness | These remain in scope; no historical gold reconstruction |
| **P20 · Final damage dealt** | `totalDamageDealt`, `totalDamageDealtToChampions`, `damageDealtToObjectives`, `damageDealtToTurrets`, `physicalDamageDealt`, `magicDamageDealt`, `trueDamageDealt`, `physicalDamageDealtToChampions`, `magicDamageDealtToChampions`, `trueDamageDealtToChampions`; existing uppercase aliases | EOG stats | Final counters | Parsed | LR | Preserve individual values and alias-conflict reasons; expose final details | Complete population is mode/client dependent |
| **P21 · Final damage received and mitigated** | `totalDamageTaken`, `physicalDamageTaken`, `magicDamageTaken`, `trueDamageTaken`, `damageSelfMitigated`; existing uppercase aliases | EOG stats | Final counters | Parsed | LR | Retain scope and field-level missingness | No per-hit history follows from totals |
| **P22 · Final healing and shielding** | `totalHeal`, `totalHealsOnTeammates`, `totalDamageShieldedOnTeammates`, `totalUnitsHealed`; existing uppercase aliases | EOG stats | Final counters | Parsed | LR | Expose values without substituting zero | Individual availability unverified across modes |
| **P23 · Final vision and ward totals** | `visionScore`, `wardsPlaced`, `wardsKilled`, `visionWardsBoughtInGame`, `detectorWardsPlaced`; existing uppercase aliases | EOG stats | Final counters | Parsed | LR | Keep distinct measures and field coverage | No ward positions or placement times |
| **P24 · Final objective totals** | `turretKills`, `inhibitorKills`, `baronKills`, `dragonKills`, `objectivesStolen`, `objectivesStolenAssists`, `firstTowerKill`, `firstTowerAssist`; existing uppercase aliases | EOG stats | Final counters/flags | Parsed | LR | Retain attribution and objective applicability | Missing totals are not zero; objectives differ by mode |
| **P25 · Final streak, control and death totals** | `largestKillingSpree`, `largestMultiKill`, `doubleKills`, `tripleKills`, `quadraKills`, `pentaKills`, `firstBloodKill`, `firstBloodAssist`, `largestCriticalStrike`, `timeCCingOthers`, `totalTimeCCDealt`, `totalTimeSpentDead`; existing aliases | EOG stats | Final counters/flags | Parsed | LR | Preserve distinct metrics; verify units/aliases | Multikill totals do not identify underlying kill timestamps |
| **P26 · Final spell-use counters** | `spell1Casts`…`spell4Casts`, `summoner1Casts`, `summoner2Casts` | EOG stats | Final counters | Parsed | LR | Verify real payload mappings and slot semantics | Not cast events or cooldown history |
| **P27 · Mode-specific final facts** | `PLAYER_AUGMENT1…4`, `PLAYER_SUBTEAM`, `SUBTEAM_PLACEMENT` | EOG stats | Final facts | Parser support | LR when supplied | Keep mode-scoped/nullable; do not impose these shapes globally | Four-augment parser support is not complete Arena support |
| **P28 · Historical roster K/D/A** | PL `scores.{kills,deaths,assists}`; participant reference | PL | Sampled state, 2 s | Wire only | No sampled roster KDA | Retain observations; query latest valid state at playhead with age | Event-derived local KDA is a separate coverage-dependent fact |
| **P29 · Historical creep score** | PL `scores.creepScore` | PL | Sampled state, 2 s | Yes, currently 10 s | GL | Preserve error/missing versus zero; retain roster scope in queries | Current replay projection mainly supplies local CS |
| **P30 · Historical ward score** | PL `scores.wardScore` | PL | Sampled numeric state, 2 s | Wire only | No | Retain numeric value and source semantics | Do not assume integer type or equality with EOG vision score |
| **P31 · Historical level** | PL `level`; participant reference | PL | Sampled state, 2 s | Yes, currently 10 s | GL | Retain observation timing/missingness; use PL as the level source | No additional AP level stream |
| **P32 · Historical inventory** | PL `items[*].{itemID,slot,count}` | PL | Sampled state, 2 s | Yes, currently 10 s | GL | Preserve count in consumers and error/missing/empty distinctions | Nested item errors currently become empty inventory |
| **P33 · Historical alive/dead state** | PL `isDead` | PL | Sampled boolean, 2 s | Wire only | No | Retain for all participants with coverage | Differences bound transitions; they do not establish exact death/respawn time |
| **P34 · Historical respawn estimate** | PL `respawnTimer` | PL | Sampled countdown, 2 s | Wire only | No | Retain observed value/age; label any display extrapolation | Nonzero behavior and units need representative capture verification |
| **P40 · Inventory-change timeline** | Previous/current observation IDs; participant; before/after inventories; observation bounds | Derived from PL | Inferred interval between observations, widened across gaps/request uncertainty | Derived today | GL changes currently use later sample tick | Represent bounded inventory changes, not exact purchase/sale events | Transformation, consumption, undo and slot moves can resemble purchase/sale |
| **P41 · Level-change timeline** | Previous/current PL observation IDs; participant; before/after level; observation bounds | Derived from PL | Inferred interval, not exact level-up time | Yes | GL later-sample tick | Preserve interval/source; no fabricated intermediate timestamps | Multiple changes may occur between samples |
| **P42 · Kills, deaths and assists on timeline** | E + `KillerName`, `VictimName`, `Assisters` | EV | Source-timestamped event; 1 s polling | Yes | GL fields/normalized time; source ID omitted | Persist source ID and actor references; retain unresolved actor identifiers | Poll receipt is not event time; source accuracy is not a microsecond guarantee |
| **P43 · First-blood marker** | E + candidate `Recipient` field | EV | Source event if supplied; 1 s | Generic envelope; recipient discarded | Event may survive without recipient | Verify actual payload and retain recipient if supplied | `Recipient` not verified by available committed capture |
| **P44 · Multikill markers and clip triggers** | E + `KillerName`, `KillStreak`; derived links to contributing kill events | EV + derived links | Source marker; event linking is derived | Event fields parsed | GL marker; no durable contributing-event links | Persist source ID and explicit contributing-event references with uncertainty | Official examples establish marker shape, not a universal linking rule |
| **P45 · Ace markers** | E + `Acer`, `AcingTeam` | EV | Source event; 1 s | Yes | GL | Resolve actors/teams without universal two-team assumption | Availability varies by mode |
| **P46 · Dragon kills and steals** | E + `DragonType`, `KillerName`, `Assisters`, `Stolen` when supplied | EV | Source event; 1 s | Yes | GL; some fields lost in app projection | Carry type/steal/actors through search and clip selection | Missing `Stolen` is not false; objective may not apply |
| **P47 · Baron kills and steals** | E + `KillerName`, `Assisters`, `Stolen` when supplied | EV | Source event; 1 s | Yes | GL; incomplete app projection | Preserve steal attribution for semantic search | Optional field availability needs representative captures |
| **P48 · Herald kills** | E + `KillerName`, `Assisters`, `Stolen` when supplied | EV | Source event; 1 s | Yes | GL | Preserve supported fields through consumers | Objective payloads need not have identical fields |
| **P49 · Void-grub / Horde events** | E + actual returned objective/actor fields | EV | Source event if supplied; 1 s | Generic parser/UI recognize candidate names | Generic fields only | Verify exact event name/payload before promising support | `Horde` / `HordeKill` behavior remains unverified |
| **P50 · Turret destruction** | E + `TurretKilled`, `KillerName`, `Assisters` | EV | Source event; 1 s | Yes | GL | Preserve structure identifier and actor roles | Structure-name interpretation must be map-specific |
| **P51 · First turret marker** | E + `KillerName` for `FirstBrick` | EV | Source event; 1 s | Yes | GL | Link related marker without double-counting destruction | Marker may lack complete structure identity |
| **P52 · Inhibitor destruction** | E + `InhibKilled`, `KillerName`, `Assisters` | EV | Source event; 1 s | Yes | GL | Preserve structure identity and actors | Map/mode dependent |
| **P53 · Inhibitor respawn markers** | E + actual structure identifier for `InhibRespawningSoon` / `InhibRespawned` | EV | Source event if supplied; 1 s | Parser support | GL when accepted | Verify exact payload keys and availability | No representative runtime payload available |
| **P54 · Game lifecycle markers** | E for `GameStart`, `MinionsSpawning`, `GameEnd`; `Result` when supplied | EV | Source event; 1 s | Yes | GL | Retain timeline facts with coverage | `GameEnd.Result` neither replaces EOG outcome nor controls recorder lifecycle |
| **P55 · Participant/category timeline filtering** | Event type; actor references/roles; objective identifier/type; selected participant/category | Existing event data | Source event time | Partial | Source events persist; selection is UI state | Keep killer/victim/assister roles queryable; support independent category filtering | Current matching strips tags; current multi-selection also differs from intended single-participant behavior |
| **P56 · Teamfight / kill-sequence clusters** | Cluster ID; member event IDs; first/last game time; participants; rule parameters/version | Derived retained events | Derived interval | Nearby-event heuristics only | No durable reproducible cluster record | Define deterministic membership and retain input references | Without positions, temporal proximity cannot prove a spatially connected fight |
| **P57 · Automatic clip boundaries** | Trigger/cluster references; first/last relevant event; pre/post roll; mapped `[start_frame,end_frame_exclusive)`; `media_id` | Events + recorder mapping | Derived selection; exact frame interval after mapping | Heuristic ranges exist | No durable selection recipe | Retain reason/range; bound to media coverage; merge overlaps deterministically | Current defaults do not guarantee full multikill coverage |
| **P58 · Selected-player kill montage** | Participant; ordered kill/cluster IDs; ordered non-overlapping source frame ranges | Derived events + media | Derived sequence of exact media intervals | No complete pipeline | No | Retain deterministic selection/ordering inputs | Limited to captured events and mapped media |
| **P59 · Automatic recap** | Event/bookmark references; rule/version; inclusion reasons; ordered frame ranges | Events/bookmarks + recorder | Derived sequence | No | No | Make every segment traceable to a supported rule | Recap policy remains unresolved; no subjective quality/coaching inference |
| **P60 · Live bookmarks** | Bookmark ID; `media_id`; capture monotonic time; mapped replay/game time where available; label | Recorder input/time mapping | User-action time with mapping uncertainty | No | No | Preserve action timing and unavailable mappings honestly | No League endpoint needed |
| **P61 · Clip provenance and semantic search** | Clip ID; ordered source media IDs/frame ranges; trigger/event IDs; derived participant/champion/objective tags | Recorder/export + retained facts | Exact media ranges; derived tags | Filename source linkage only | No complete recipe/event sidecar | Retain recipe/source references for restart and rebuild | Tag absence cannot prove event absence with incomplete coverage |
| **P62 · Map labels and terrain** | GS `mapName`, `mapTerrain` | GS | Observed context, 1 s normally | Wire only | No | Retain initial value and subsequent changes with observation evidence | Observed in Practice Tool; terrain need not be immutable |
| **P63 · Participant role and bot context** | PL `position`, `isBot`; EOG `selectedPosition`, `detectedTeamPosition` | PL, EOG | PL 2 s; EOG final | Live wire only; EOG parsed | Final positions in LR | Keep selected/detected roles and Live observations distinct | Empty role is not inferred lane; dummies/bots need explicit treatment |
| **P64 · Champion/skin rendering identity** | PL `rawChampionName`, `skinID`, `skinName`, `rawSkinName` | PL | Observed context, 2 s | Wire only | No | Retain returned identifiers/labels and observed changes | Capture has these keys, including empty values; not guaranteed across versions/modes |
| **P65 · Inventory metadata** | PL `items[*].{canUse,consumable,price,displayName,rawDisplayName,rawDescription}` alongside P32 | PL | Sampled response, 2 s | Wire only | No | Retain metadata with item observations; share repeated payloads | `canUse` is not verified as off-cooldown; price is not earned gold |
| **P68 · Match version and final labels** | EOG `gameVersion`, `gameType`, `queueType`, `gameMode`, `queueId`, `mapId` | EOG | Final context | Yes | LR | Expose version for interpretation/query provenance | No equivalent verified in-game patch field established here |
| **P69 · Final team scoreboard** | EOG `teams[*].teamId`, `isWinningTeam`, `stats` using the supported counters above | EOG | Final facts | Parsed | LR | Preserve reported team totals separately from calculated player sums | Team stats may be absent; player sums are derived and may differ |
| **P70 · Match-history semantic search** | Match/media identity; local/roster champion and participant references; team relation; queue/mode/map; authoritative outcome; event type/actor role/multikill count/steal flag; recording date | Facts above | Mixed facts retaining their source semantics | Inputs partial; basic champion/mode search/filter exists | Partial source files; no complete semantic index | Support opponent/participant, queue, outcome and event predicates with unknown coverage | No matching event in an incomplete recording does not prove none occurred |
| **T01 · Recording-to-match ownership** | `media_id`; provisional/confirmed game ID; association evidence/source scope | Recorder + LM/LR | Technical identity | Yes | M/LM/LR | Keep recording identity distinct from match identity; do not globally key by an unqualified numeric game ID | Platform/region namespace is not established by current allowlisted sources |
| **T02 · Durable event identity** | Source session/epoch; `EventID`, `EventName`, source `EventTime`; normalized `GameTick`; ingestion status | EV | Source time plus receipt timing | Event ID exists in memory | GL loses EventID | Retain source ID/time before advancing durable retrieval progress | Reset, consecutiveness, corrections and cursor edge semantics unverified |
| **T03 · Observation timing** | Observation ID; endpoint; request-start/end monotonic times; clock-domain/run ID; receipt wall time; GS observation/calibration reference | Each acquisition, including identity-only AP | Request interval; mapped game-time estimate with uncertainty | Timing partly measured transiently | Not durably attached to observations | Retain per-response timing; do not assign PL the GS receipt as if simultaneous | PL has no verified independent source game timestamp; AP is not a historical-state stream |
| **T04 · Coverage and gaps** | Endpoint activation/stop; successful observation times; errors/missing status; skipped polls; last durable observation; gap bounds/reason | Acquisition + recorder | Observed coverage | Failures logged | No complete durable coverage | Retain interruption/recovery boundaries and stale/unknown state; AP only records identity coverage | Current loops stop after three failures; missed roster states cannot be backfilled |
| **T05 · Field provenance and missingness** | Source/endpoint; source schema/client version when known; field presence/value; unavailable/error/invalid/conflicting/not-applicable reason | All sources | Inherits fact precision | Partial | Coarse LR family coverage only | Preserve distinct field states, including zero and empty | Not-applicable requires mode evidence |
| **T06 · Game/video calibration** | Existing sample count; first/last game tick; intercept; RTT; residual; uncertainty; validity | GS + recorder | Existing fast 250 ms calibration phase; separate normal 1 s polling | Yes | GL calibration | Preserve five-sample validation, tolerances, retries and invalidation; no startup slowdown | Probe spacing is a target, not guaranteed calibration latency |
| **T07 · Media/frame authority** | `media_id`; video time base; first/last PTS; exact rate/count; replay coverage; capture anchor; presented-frame time | Recorder + player | Exact validated media coordinates | Yes | M; presented-frame observations transient | Reuse existing time types and half-open frame ranges | Wall clock and seek requests are not presented-video authority |
| **T08 · Historical-state lookup** | Participant; requested game/replay time; selected observation ID/time; value; age; coverage/missingness | Retained samples + calibration | Latest valid observation at/before playhead; no interpolation by default | Partial local-state lookup | Source samples only | Extend roster queries and expose staleness/gaps | Computed overlay props are not all actually rendered |
| **T09 · Mode-dependent applicability** | Raw mode/queue/map; arbitrary participant/team IDs; supported/unknown/not-applicable field/event coverage | Context + observed source behavior | Context/coverage | Partial | Arrays/context exist | Extensible collections and identifiable unknown kinds | Arena may remain unsupported; valid unfamiliar structure must not be coerced to standard mode |
| **T10 · Reproducible derivation and indexing** | Source artifact identity/fingerprint; schema/normalizer version; rule version/parameters/input references | Durable source data | Technical provenance | Fragmented | No unified rebuild contract | Reproducible projections/clusters; invalidate when inputs change | Derived values must not overwrite source facts |
| **T11 · Truthful negative queries** | Queried interval; relevant endpoint coverage; supported field/event set; missingness/uncertainty | Coverage + facts | Interval-aware query result | No complete support | No | Distinguish absent, unknown and not applicable | Complete HTTP coverage does not prove the API exposes every real action |
| **T12 · Durable acquisition progress** | Last durable event retrieval boundary; observation/checkpoint boundary; pending EOG association/expiry/completion | Acquisition persistence | Commit progress, not event time | Memory state exists | No restart-safe acquisition checkpoint | Recover from committed state without duplicates or silently lost pending reconciliation | Data no longer served by the client may be unrecoverable |
| **T13 · Resource-budget evidence** | Requests/bytes per endpoint; response latency; parse/write time; queue depth; memory high-water mark; disk growth; League frame-time/FPS; recorder drops | Acquisition/recorder measurements | Measurement windows in separate clock domain | Some diagnostics | No complete candidate comparison | Measure candidate rates, long matches and failures before accepting defaults | Available captures establish payload sizes, not FPS/CPU impact |

## Matrix 2: endpoint acquisition and persistence contract

All Live paths below use local HTTPS on `127.0.0.1:2999`; LCU uses the existing
authenticated loopback transport. There is one normal cadence per endpoint, driven
by its shortest retained-field freshness requirement. All retained response fields
follow that cadence. The explicit exception is the existing fast GS calibration
phase, which is separate from normal polling and must not regress.

Keep the existing working `/activeplayer` endpoint for bounded identity checks. Its
`riotId` is sufficient: PL supplies local name/champion/team, GS supplies map/mode,
and LCU supplies PUUID/game/queue and comparison identity. `/activeplayername` remains
an unused alternative; replacing the working path requires verified installed-client
compatibility and measured benefit. The current association requires two fresh
coherent rounds; the same cached AP response cannot count twice. Its first
successful exact local roster mapping remains useful if LCU association fails. A
failed/unavailable identity must not block ordinary roster acquisition or video.

| Endpoint | Useful response fields | Cadence / active phase | Acquisition method / current position | API optimization and verification status | Persistence contract | Interruption / recovery behavior |
| --- | --- | --- | --- | --- | --- | --- |
| **GS** `/liveclientdata/gamestats` | `gameTime`, `gameMode`, `mapNumber`, `mapName`, `mapTerrain` | **250 ms fast calibration**; **1 s normal polling** candidate | Existing GET. Preserve current five strictly advancing samples, minimum 750 ms window, 250 ms tolerances, midpoint/RTT evidence and 1 s failure retry. Current normal sampling is 10 s | Focused endpoint documented; no verified field filter/cursor/conditional retrieval. One request owner switches phases without overlapping calibration/normal pollers | Retain clock/request timing and calibration evidence; share unchanged context values while preserving observations | Preserve existing fast recalibration behavior after invalidation; mark discontinuities. Never silently join data across a reset |
| **AP** `/liveclientdata/activeplayer` | `riotId` for identity; detailed telemetry is outside current requirements | **Identity bootstrap; fresh checks at the existing 10 s association cadence while needed; stop on binding or admission closure** | Reuse the existing working endpoint/identity extraction; remove its dependency from normal state rounds | No cursor/filter needed. Keep the established path; an alternative must demonstrate compatible behavior and measured benefit before replacement | Retain valid full identity with request timing/provenance; exact PL mapping supplies local summary. No detailed AP telemetry archive | Missing/tagless response leaves identity unavailable; preserve fresh-round rules and bounded identity lifecycle. Cached identity may support roster display, not fresh association proof |
| **EV** `/liveclientdata/eventdata` | E plus supported event-specific fields P42-P54 | **1 s** | Existing cumulative GET and RAM EventID dedup | Historical schema advertises `eventID` as the next expected event ID. Enable only after installed-client schema and behavioral verification: inclusivity, empty/stale/future cursor responses, reset and corrections. Current code sends no cursor | Persist source-scoped ID, original time and supported payload once; retain poll timing/coverage. Advance durable cursor only after corresponding data/status is durably accounted for | Resume from durable boundary with verified safe overlap/dedup; reconcile without cursor on uncertain session/reset behavior. If history is unavailable, retain a gap |
| **PL** `/liveclientdata/playerlist` | Full roster identity/context; levels; KDA/CS/ward score; inventory/metadata; dead/respawn state; spells/basic runes | **2 s** | Existing GET currently required in joined 10 s rounds; proposed roster acquisition independent of AP success | Historical `teamID` filter uses strings such as `ORDER`/`CHAOS`; current behavior unverified. Omit filter for all teams. Splitting teams increases request count/skew; no measured benefit | Initial complete retained state then changed payloads; retain every observation's timing/payload reference, including equality and nested errors | Resume with complete roster state; retain gaps. Never backfill intermediate states or compare across uncertain identity/session continuity |
| **GF** `/lol-gameflow/v1/session` | `phase`, `gameData.gameId`, `gameData.queue.{id,mapId,gameMode}`, `map.{id,gameMode}` | **10 s**, only while association admission is open | Existing LCU collector and coherent paired SUM/Live rounds | No verified incremental/filter capability needed. Reuse collector; no duplicate context poller | Preserve accepted context and evidence timing/status; share repeated values with observation evidence | Reuse valid sealed LM; incomplete association requires fresh coherent rounds. Phase is not recorder lifecycle authority |
| **SUM** `/lol-summoner/v1/current-summoner` | `puuid`, `gameName`, `tagLine` | **10 s**, same association window as GF | Existing authenticated GET paired with GF | No verified cursor/filter. Local payload dedup is a client optimization, not fresh evidence | Retain exact local identity/accepted observations; identifiers remain local and out of logs | Rediscover credentials without persisting secrets. Account changes invalidate pending assumptions; sealed identity remains immutable |
| **EOG** `/lol-end-of-game/v1/eog-stats-block` | Exact game ID; final context/outcome/early flags; local/roster/team loadouts and supported statistics | **10 s**, existing maximum 180 s / 18 attempts | Existing bounded coordinator, at most four jobs; pending work currently memory-only | No verified cursor/field filter. Sharing an acquired response among pending exact-ID checks is a client proposal, not an API feature; one endpoint request owner | Reuse LR atomic deletion-aware attachment; retain facts/missingness. Persist pending association/expiry for restart recovery. Do not replace confirmed facts with worse partial data | Resume only unexpired durable pending work and revalidate exact game ID. Expired/unavailable remains unknown; never attach a newer unrelated match |
| `/liveclientdata/activeplayername` | Full Riot ID JSON string, available as a documented alternative | **0 in the proposed contract** | Not used; retain the existing AP identity path | Smaller response is documented, but installed-client compatibility and practical benefit are not verified/measured. Replacement is not required | No acquisition or persistence requirement for this endpoint | Not a recovery dependency or automatic fallback |
| `/liveclientdata/allgamedata` | Aggregate GS/PL/identity/events, plus out-of-scope AP telemetry | **0 in proposed production acquisition** | Used by diagnostic captures; production uses focused routes | Aggregate documented; historical schema also has `eventID`, which affects only the event portion. A benchmark alternative must replace overlapping requests, not add them | Explicit diagnostic use only; no requirement to retain out-of-scope AP data | No automatic overlapping aggregate fallback; changing acquisition requires revising the measured endpoint choice |
| `/liveclientdata/activeplayerabilities` | Out-of-scope local ability telemetry | **0** | Documented focused route, not needed | Not a replacement telemetry route under this contract | No new acquisition requirement | No historical backfill |
| `/liveclientdata/activeplayerrunes` | Out-of-scope local full Live rune page | **0** | Documented focused route, not needed | PL basic runes and EOG final rune facts remain sufficient for current scope | No new acquisition requirement | Final facts cannot reconstruct historical loadout changes |
| `/liveclientdata/playerscores?riotId=…` | One player's KDA, CS and ward score | **0** | Documented per-player GET | Full-roster fan-out duplicates PL work; no measured benefit | Use PL observations | No historical backfill |
| `/liveclientdata/playeritems?riotId=…` | One player's inventory/item metadata | **0** | Documented per-player GET | Redundant with PL for this contract | Use PL observations | No historical backfill |
| `/liveclientdata/playersummonerspells?riotId=…` | One player's spell identities/labels | **0** | Documented per-player GET | Redundant with PL | Use PL observations | No historical spell-change reconstruction |
| `/liveclientdata/playermainrunes?riotId=…` | One player's keystone and rune styles | **0** | Documented per-player GET | Redundant with PL; does not supply full rune pages | Use PL observations | Preserve missing until a later valid observation |
| `/swagger/v3/openapi.json` | Installed-client endpoint/query schemas | **0 periodic polling; explicit diagnostic read** | Riot documents local schema retrieval | Establishes advertised parameters; runtime probes still needed for cursor semantics | Retain versioned diagnostic schema evidence without player data/credentials | Recheck when compatibility evidence changes; not a gameplay-history stream |

No conditional GET, changed-since, long-poll or push mechanism for these Live streams
has been verified. Local payload hashing, deduplication, response sharing and
change-aware storage are client-side proposals, not API capabilities.

## Timing, persistence and recovery requirements

- **Bound work:** one owner per endpoint, at most one request in flight per endpoint,
  bounded total concurrency/response sizes/timeouts and no catch-up bursts. Failed or
  skipped slots become coverage evidence. Capture remains independent of League API
  availability; network, parsing and disk work must not enter the capture-critical path.
- **Preserve observations:** store identical payloads once if useful, but preserve each
  successful observation's actual timing and reference. First/last/count alone loses
  jitter and gaps. A missing/error transition is a change. Equality at two observations
  does not prove no intervening change occurred.
- **Keep responses independent:** PL and GS are not an atomic snapshot. Retain each
  response's request interval and mapping uncertainty. Event source time, observation
  time, wall time and recording/video time remain distinct.
- **Respect clock authority:** reuse existing game ticks, calibration and exact media
  frame ranges. Keep fast calibration separate from normal GS polling. Preserve its
  current five-sample behavior, failure retries and invalidation/recalibration checks.
  No slower startup calibration or second timing model is authorized.
- **Commit cursor progress with data:** a received maximum EventID is insufficient.
  Persist accepted event data or explicit ingestion status before advancing a durable
  retrieval boundary. Retain conflict/invalid evidence instead of silently discarding
  an event and making it unrecoverable. Verify correction behavior before relying on
  incremental retrieval. Numeric ID gaps alone do not establish loss.
- **Scope event identity:** deduplication and retrieval state belong to a verified
  source session/epoch, not a global EventID. Preserve/reset boundaries deliberately
  after restart, game change or uncertain clock continuity. Never merge solely by time.
- **Recover honestly:** cumulative events may be recoverable while the same game is
  still served. Intermediate roster states generally are not. Restart from the last
  durable checkpoint, retain a crash/unobserved tail gap, and never fabricate backfill.
- **Preserve identity semantics:** match association still needs exact full Live/LCU
  Riot ID agreement, compatible map/mode and two distinct fresh coherent rounds.
  Retain the working AP identity source for bounded checks; do not turn its cached
  identity into fresh evidence or authoritative match identity. EOG confirmation remains separate.
- **Retain local presentation independently:** one valid AP-to-PL identity match may
  supply local name/champion/team even if LCU association never succeeds. Missing local
  identity leaves that summary unavailable; it does not stop roster/events/video.
- **Bound post-game recovery:** preserve existing expiry, attempt and job limits when
  adding durable pending EOG state. Resume only while valid, require exact game ID,
  and retain deletion-safe writer ownership; never recreate a deleted recording.
- **Keep compatibility:** existing recordings remain usable. New observations must not
  retroactively replace unknown historical states or silently remap participants.
  Secrets and unrelated account/social/reward data are never part of this contract.

## Durable storage and rebuildable SQLite implications

Reuse the recording bundle, `media_id`, canonical replay-time types, LM and LR
sidecars and existing writer ownership. Durable source data must retain source-scoped
events, timed roster observations, coverage/missingness, identity evidence and recovery
progress. An allowlisted source record with provenance is sufficient; a blanket raw
API archive is not required. SQLite must not be the only copy of those facts.

The current GL writer clones and rewrites the whole growing document. At the proposed
cadences, bounded incremental persistence with occasional self-contained checkpoints
is a candidate to measure. The physical format, flush policy and reader compatibility
remain undecided. A checkpoint must preserve a recoverable committed prefix and a
truthful durability boundary; an uncommitted tail is not guaranteed recoverable.

SQLite is a rebuildable query projection of match/recording identity, participants,
final facts, events, roster observations, coverage, recording mappings and reproducible
derived selections. Index match filters, event/participant roles and participant-time
lookups. Input fingerprints and normalizer/rule versions determine invalidation.
Authored bookmarks and clip recipes need durable ownership outside a disposable index.
No tables, migrations, alternate provider framework or implementation sequence is
selected here.

## Outstanding verification and decisions

These are the remaining limits of the accepted contract, not a new roadmap:

1. **Identity scope:** preserve the working AP `riotId` comparison, PL mapping and
   fresh-round behavior when limiting requests to bounded identity work. A tagless,
   malformed or missing value is unavailable; no name-only inference. Endpoint replacement
   is not required; reconsider `/activeplayername` only with verified compatibility
   and measured benefit.
2. **EV cursor:** verify installed schema plus real cursor behavior, including overlap,
   empty/stale/future requests, ID reset and corrections. Keep full retrieval/dedup
   until that evidence supports enabling `eventID`. Do not assume `max_id + 1` semantics
   solely from historical next-event wording.
3. **Cadence/resource measurements:** GS 1 s, EV 1 s and PL 2 s imply an average
   **2.5 steady Live requests/second**, excluding calibration, identity and LCU work.
   This is arithmetic, not a measured resource claim. Compare focused/aggregate
   alternatives, response bytes/latency, parsing and write cost, memory/queue bounds,
   storage growth, missed transitions, League frame-time/FPS and recorder drops.
   Include long matches and interruption/recovery. Numeric acceptance budgets and
   production rates remain subject to those measurements; no FPS guarantee is made.
4. **Field/mode evidence:** verify real loss encoding, final-item slots and stat units,
   FirstBlood recipient, Horde variants, inhibitor respawn fields, nonzero respawn
   timers and mode applicability. Supported parser keys are not availability guarantees.
5. **Persistence details:** select the bounded record/checkpoint format, durability
   window and compatibility behavior from measurements; define source-session reset
   behavior without weakening provenance or creating a new clock.
6. **Derived selection policy:** deterministic cluster membership, recap inclusion and
   clip-boundary parameters remain explicit product decisions. Preserve source inputs
   so changing those rules does not require reacquiring gameplay data.

This consolidation changes documentation only. The matrices distinguish existing
implementation from agreed requirements, candidate rates and unverified capabilities.
