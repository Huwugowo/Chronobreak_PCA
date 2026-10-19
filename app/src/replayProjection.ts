import type { IndexedEvent, ReplayRoster, SourceScope } from "./leagueIndex";
import type { ReplayParticipant, ViewerEvent } from "./types";

export function replayParticipants(roster: ReplayRoster | null): ReplayParticipant[] {
  if (!roster) return [];
  const local = roster.players.find(player => player.participant === roster.local_participant);
  return roster.players.map(player => ({
    identity: player.participant === undefined ? null : { source: roster.source, participant: player.participant },
    is_local: player.participant !== undefined && player.participant === roster.local_participant,
    summoner_name: player.riot_id ?? "Unresolved player",
    champion: player.champion_name ?? "Unknown champion",
    relation: player.team && local?.team ? player.team === local.team ? "ally" : "enemy" : "neutral",
  }));
}

export function replayEvent(row: IndexedEvent, scope: SourceScope, roster: ReplayRoster | null): ViewerEvent {
  const event = row.revision.event;
  const local = roster?.players.find(player => player.participant === scope.local_participant);
  const roles = ["killer", "recipient", "acer"] as const;
  const actors = roles.flatMap(role => event.actors?.[role] ?? [])
    .filter(reference => reference.status === "resolved" && reference.participant !== null);
  const player = actors.length === 1
    ? roster?.players.find(player => player.participant === actors[0].participant) : undefined;
  return {
    source: scope.source, source_id: event.source_id, actors: event.actors,
    local_participant: scope.local_participant, evidence: row.evidence,
    event_type: event.name ?? "UnknownEvent", game_tick: event.game_tick, replay_tick: row.replay_tick,
    killer: event.killer ?? null, victim: event.victim ?? null, assisters: event.assisters ?? [],
    dragon_type: event.dragon_type ?? null, stolen: event.stolen ?? null, recipient: event.recipient ?? null,
    kill_streak: event.kill_streak ?? null, acer: event.acer ?? null, acing_team: event.acing_team ?? null,
    turret: event.turret ?? null, inhibitor: event.inhibitor ?? null, result: event.result ?? null,
    relation: player?.team && local?.team ? player.team === local.team ? "ally" : "enemy" : "neutral",
  };
}
