import type { IndexedEvent, OverlaySample, ReplayIdentity, ReplayRoster, SourceScope } from "./leagueIndex";
import type { ReplayParticipant, ViewerEvent } from "./types";
import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "./replayTime";
import { participantKey } from "./leagueSource";

export function recordedGameClock(tick: string): string {
  const value = BigInt(tick);
  const seconds = (value < 0n ? -value : value) / 1_000_000n;
  return `${value < 0n ? "−" : ""}${seconds / 60n}:${String(seconds % 60n).padStart(2, "0")}`;
}

export type RecordedReplayState = { kills?: number; deaths?: number; assists?: number; creep_score?: number; level?: number; clock: string };

export type RecordedParticipantState = {
  level?: number;
  dead?: boolean;
  respawnSeconds?: number;
  spells: readonly [string | null, string | null];
  spellsAreInitial?: true;
};
/** First recorded loadout context; deliberately contains no temporal status. */
export type InitialSpellLoadout = {
  replay_tick: ReplayTick;
  players: readonly { participant: number; spells: readonly [string | null, string | null] }[];
};
/** Asset availability is source context, never a temporal spell-selection fact. */
export type ParticipantSpellAssets = ReadonlyMap<string, readonly [readonly string[], readonly string[]]>;

export function overlayParticipantStates(scope: SourceScope, sample: OverlaySample | null, position: ReplayTick | null,
  initialLoadout: InitialSpellLoadout | null = null): ReadonlyMap<string, RecordedParticipantState> {
  const states = new Map<string, RecordedParticipantState>();
  if (!sample || position === null || sample.replay_tick > position) {
    // Opening context is never a substitute for a later unavailable sample.
    if (initialLoadout && (position === null || position < initialLoadout.replay_tick)) {
      for (const player of initialLoadout.players) states.set(participantKey({ source: scope.source, participant: player.participant }),
        { spells: player.spells, spellsAreInitial: true });
    }
    return states;
  }
  const elapsed = (position - sample.replay_tick) / REPLAY_TICKS_PER_SECOND;
  for (const player of sample.players) {
    states.set(participantKey({ source: scope.source, participant: player.participant }), {
      level: player.level ?? undefined, dead: player.is_dead ?? undefined,
      // Only a later recorded alive sample restores color, never a deadline.
      respawnSeconds: player.is_dead === true && player.respawn_timer !== null
        ? Math.max(0, Math.ceil(player.respawn_timer - elapsed)) : undefined,
      spells: player.spells,
    });
  }
  return states;
}

export function replayParticipants(scope: SourceScope, identities: readonly ReplayIdentity[]): ReplayParticipant[] {
  const local = identities.find(player => player.participant === scope.local_participant);
  // Timed roster samples can arrive in different orders; filter slots belong to source identities.
  return [...identities].sort((left, right) => left.participant - right.participant).map(player => ({
    identity: { source: scope.source, participant: player.participant },
    is_local: player.participant === scope.local_participant,
    summoner_name: player.riot_id ?? "Unresolved player",
    champion: player.champion_name ?? "Unknown champion",
    relation: player.team && local?.team ? player.team === local.team ? "ally" : "enemy" : "neutral",
  }));
}

export function replayEvent(row: IndexedEvent, scope: SourceScope, roster: ReplayRoster | null, identities: readonly ReplayIdentity[] = []): ViewerEvent {
  const event = row.revision.event;
  const teamOf = (participant: number | null) => roster?.players.find(player => player.participant === participant)?.team
    ?? identities.find(player => player.participant === participant)?.team;
  const localTeam = teamOf(scope.local_participant);
  const roles = ["killer", "recipient", "acer"] as const;
  const actors = roles.flatMap(role => row.effective_actors?.[role] ?? [])
    .filter(reference => reference.status === "resolved" && reference.participant !== null);
  const actorTeam = actors.length === 1 ? teamOf(actors[0].participant) : undefined;
  return {
    source: scope.source, source_id: event.source_id, actors: row.effective_actors ?? undefined,
    local_participant: scope.local_participant, evidence: row.evidence,
    event_type: event.name ?? "UnknownEvent", game_tick: event.game_tick, replay_tick: row.replay_tick,
    killer: event.killer ?? null, victim: event.victim ?? null, assisters: event.assisters ?? [],
    dragon_type: event.dragon_type ?? null, stolen: event.stolen ?? null, recipient: event.recipient ?? null,
    kill_streak: event.kill_streak ?? null, acer: event.acer ?? null, acing_team: event.acing_team ?? null,
    turret: event.turret ?? null, inhibitor: event.inhibitor ?? null, result: event.result ?? null,
    relation: actorTeam && localTeam ? actorTeam === localTeam ? "ally" : "enemy" : "neutral",
  };
}
