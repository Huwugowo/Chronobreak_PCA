import { actorKeys, participantKey, type ActorRole } from "./leagueSource";
import type { ReplayParticipant, ViewerEvent } from "./types";

export type TimelineParticipantIndex = ReadonlyMap<string, ReplayParticipant>;

export function timelineParticipantIndex(participants: readonly ReplayParticipant[]): TimelineParticipantIndex {
  return new Map(participants.flatMap(player => player.identity
    ? [[participantKey(player.identity), player] as const] : []));
}

export function combatParticipant(event: ViewerEvent, role: ActorRole, participants: TimelineParticipantIndex): ReplayParticipant | undefined {
  const keys = actorKeys(event, role);
  return keys.length === 1 ? participants.get(keys[0]) : undefined;
}

export function combatSide(event: ViewerEvent, participants: TimelineParticipantIndex): "blue" | "red" | "unknown" {
  switch (combatParticipant(event, "killer", participants)?.team?.toUpperCase()) {
    case "ORDER": return "blue";
    case "CHAOS": return "red";
    default: return "unknown";
  }
}

export function combatAssisters(event: ViewerEvent, participants: TimelineParticipantIndex): Array<{ player?: ReplayParticipant; name: string }> {
  const refs = event.actors?.assister ?? [];
  // Display unresolved names without attributing a champion to a string match.
  if (!refs.length) return event.assisters.map(name => ({ name }));
  return refs.map(ref => {
    const player = ref.status === "resolved" && ref.participant !== null && event.source
      ? participants.get(participantKey({ source: event.source, participant: ref.participant })) : undefined;
    return { player, name: player?.summoner_name ?? "Unknown assister" };
  });
}
