import type { IndexedEvent, OverlayPage, OverlayQuery, ReplayWindow, ReplayWindowQuery, QueryCoverage } from "../leagueIndex";
import type { Evidence, EventActors } from "../leagueSource";
import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "../replayTime";
import type { GameSummary } from "../types";

function mockPlayers(game: GameSummary, observedSeconds: number, riotId: string) {
  return game.participants.map((player, index) => ({
    participant: index + 1,
    riot_id: index === 0 ? riotId : player.summoner_name,
    champion_name: index === 0 ? game.champion : player.champion,
    team: index < 5 ? "ORDER" : "CHAOS",
    level: 1 + Math.min(17, Math.floor(observedSeconds / 90)),
    scores: { kills: index === 0 ? Math.floor(game.kills * observedSeconds / 1800) : index,
      deaths: index === 0 ? Math.floor(game.deaths * observedSeconds / 1800) : 2,
      assists: index === 0 ? Math.floor(game.assists * observedSeconds / 1800) : 3,
      creep_score: Math.floor(observedSeconds / 8) },
    items: game.items.map(item => ({ ...item })),
  }));
}

/** Fixed observations: moving the viewport never changes event identity or time. */
export function mockReplayWindow(game: GameSummary, query: ReplayWindowQuery, riotId: string): ReplayWindow {
  const observedSeconds = Math.floor(query.at / REPLAY_TICKS_PER_SECOND / 10) * 10;
  const observed = observedSeconds * REPLAY_TICKS_PER_SECOND as ReplayTick;
  const players = mockPlayers(game, observedSeconds, riotId);
  const coverage: QueryCoverage = { index_status: "ready", committed: null, uncommitted_tail_bytes: 0,
    gap_attempts: 0, unresolved_actors: 0, unmapped_observations: 0, successful_observations: 181,
    observed_from: "0", observed_through: "1800000000" };
  const evidence = (id: number, seconds: number, endpoint = "events"): Evidence => ({
    id, source: query.scope, endpoint,
    timing: { start_us: seconds * 1_000_000, end_us: seconds * 1_000_000, received_at: "2026-10-19T00:00:00Z" },
    game_stats: 1, calibration: 1,
  });
  const moments = [
    [60, "ChampionKill"], [140, "ChampionKill"], [240, "DragonKill"],
    [350, "ChampionKill"], [480, "TurretKilled"], [580, "ChampionKill"],
    [650, "DragonKill"], [760, "ChampionKill"], [880, "TurretKilled"],
    [990, "ChampionKill"], [1100, "DragonKill"], [1210, "BaronKill"],
    [1320, "ChampionKill"], [1440, "InhibKilled"], [1560, "ChampionKill"],
    [1680, "BaronKill"], [1760, "TurretKilled"],
  ] as const;
  const events: IndexedEvent[] = moments.map(([seconds, name], index) => {
    const killer = index === 0 || index % 3 !== 1 ? 1 : 8;
    const victim = killer === 1 ? 8 : 1;
    const actors: EventActors = { killer: [{ status: "resolved", participant: killer }] };
    if (name === "ChampionKill") actors.victim = [{ status: "resolved", participant: victim }];
    return { sequence: index + 1, ordinal: 0, evidence: evidence(index + 1, seconds), effective_actors: actors,
      replay_tick: seconds * REPLAY_TICKS_PER_SECOND as ReplayTick,
      revision: { fingerprint: Array(32).fill(index + 1), response_fingerprint: Array(32).fill(index + 1), ingestion: "accepted",
        event: { source_id: index + 1, game_tick: String(seconds * 1_000_000), name, actors,
          killer: players[killer - 1].riot_id,
          ...(name === "ChampionKill" ? { victim: players[victim - 1].riot_id } : {}),
          ...(name === "DragonKill" ? { dragon_type: "Fire", stolen: index === 10 } : {}),
          ...(name === "TurretKilled" ? { turret: "Turret_T200_L0_P3" } : {}),
          ...(name === "InhibKilled" ? { inhibitor: "Barracks_T200_L1" } : {}),
        } },
    };
  }).filter(row => row.replay_tick! >= query.start && row.replay_tick! <= query.end &&
    (!query.after || row.replay_tick! > query.after.replay_tick ||
      (row.replay_tick === query.after.replay_tick && row.revision.event.source_id > query.after.source_id)));
  const page = events.slice(0, Math.min(Math.max(query.limit, 1), 512));
  const last = page[page.length - 1];
  return { source: query.scope, local_participant: 1, coverage, events: page,
    identities: players.map(({ participant, team, riot_id, champion_name }) => ({ participant, team, riot_id, champion_name })),
    next: events.length > page.length && last ? { replay_tick: last.replay_tick!, source_id: last.revision.event.source_id } : null,
    roster: { source: query.scope, local_participant: 1, game_tick: String(observedSeconds * 1_000_000),
      observed_replay_tick: observed, age_replay_ticks: query.at - observed as ReplayTick,
      evidence: evidence(1000 + observedSeconds, observedSeconds, "player_list"), players, coverage },
  };
}

/** Browser preview uses the same fixed ten-second observations as its event fixture. */
export function mockOverlayPage(game: GameSummary, query: OverlayQuery, riotId: string): OverlayPage {
  const revision = { end_offset: 1, last_sequence: 1, checksum: Array(32).fill(0) };
  if (Boolean(query.after) !== Boolean(query.revision) || (query.revision && JSON.stringify(query.revision) !== JSON.stringify(revision)))
    throw new Error("Mock overlay revision changed");
  const first = query.after?.observation_id ?? 0;
  const samples = Array.from({ length: Math.min(Math.max(query.limit, 1), 128, Math.max(0, 181 - first)) }, (_, offset) => {
    const index = first + offset, seconds = index * 10;
    const players = mockPlayers(game, seconds, riotId);
    return { replay_tick: seconds * REPLAY_TICKS_PER_SECOND as ReplayTick, observation_id: index + 1,
      game_tick: String(seconds * 1_000_000), local_scores: players[0]?.scores ?? null,
      players: players.map(player => ({ participant: player.participant, level: player.level,
        is_dead: null, respawn_timer: null, spells: [null, null] as [null, null] })) };
  });
  const last = samples[samples.length - 1];
  return { source: query.scope, local_participant: 1, revision, samples,
    identities: query.after ? null : mockPlayers(game, 0, riotId).map(({ participant, team, riot_id, champion_name }) => ({ participant, team, riot_id, champion_name })),
    next: last && last.observation_id < 181 ? { replay_tick: last.replay_tick, observation_id: last.observation_id } : null };
}
