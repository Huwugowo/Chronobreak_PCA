import { parseMediaId, type MediaId } from "./replayTime";
import { receipt } from "./leagueResult";

export type SourceEpoch = { media_id: MediaId; run_id: MediaId; epoch: number };
export type ParticipantReference = { source: SourceEpoch; participant: number };
export type ActorRole = "killer" | "victim" | "assister" | "recipient" | "acer";
export type ActorReference = { participant: number | null; status: "resolved" | "unresolved" | "conflict" };
export type EventActors = Partial<Record<ActorRole, ActorReference[]>>;
export type Evidence = { id: number; source: SourceEpoch; endpoint: string;
  timing: { start_us: number; end_us: number; received_at: string }; game_stats: number | null; calibration: number | null };
export const sourceKey = (source: SourceEpoch): string => `${source.media_id}/${source.run_id}/${source.epoch}`;
export const participantKey = (reference: ParticipantReference): string => `${sourceKey(reference.source)}/${reference.participant}`;
export const sameSource = (a: SourceEpoch, b: SourceEpoch): boolean => sourceKey(a) === sourceKey(b);
export function actorKeys(event: { source?: SourceEpoch; actors?: EventActors }, role?: ActorRole): string[] {
  if (!event.source) return [];
  return Object.entries(event.actors ?? {}).flatMap(([key, actors]) => role && key !== role ? [] : (actors ?? []).flatMap(actor =>
    actor.status === "resolved" && actor.participant !== null ? [participantKey({ source: event.source!, participant: actor.participant })] : []));
}
export function sameActor(a: { source?: SourceEpoch; actors?: EventActors }, aRole: ActorRole, b: { source?: SourceEpoch; actors?: EventActors }, bRole: ActorRole): boolean {
  const left = actorKeys(a, aRole), right = actorKeys(b, bRole);
  return left.length === 1 && right.length === 1 && left[0] === right[0];
}

export function object(value: unknown, label = "source"): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error(`${label} must be an object`);
  return value as Record<string, unknown>;
}
export function exact(value: unknown, keys: readonly string[], label = "source"): Record<string, unknown> {
  const row = object(value, label);
  if (Object.keys(row).length !== keys.length || keys.some(key => !Object.prototype.hasOwnProperty.call(row, key))) throw new Error(`${label} shape`);
  return row;
}
export function integer(value: unknown, minimum = 0, maximum = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum || value > maximum) throw new Error("source integer");
  return value;
}
export const nullableInteger = (value: unknown): number | null => value === null ? null : integer(value);
export function text(value: unknown, maximum = 1024): string {
  if (typeof value !== "string" || new TextEncoder().encode(value).length > maximum || /[\u0000-\u001f\u007f-\u009f]/u.test(value)) throw new Error("source text");
  return value;
}
export function gameTick(value: unknown): string {
  if (typeof value !== "string" || !/^(0|-?[1-9][0-9]{0,19})$/.test(value) || BigInt(value) < -86_400_000_000n || BigInt(value) > 86_400_000_000n) throw new Error("source game tick");
  return value;
}
export function array(value: unknown, maximum: number): unknown[] {
  if (!Array.isArray(value) || value.length > maximum) throw new Error("source array capacity"); return value;
}
export function decodeSource(value: unknown): SourceEpoch {
  const row = exact(value, ["media_id", "run_id", "epoch"]);
  return { media_id: parseMediaId(row.media_id), run_id: parseMediaId(row.run_id), epoch: integer(row.epoch) };
}
export function decodeParticipantReference(value: unknown): ParticipantReference | null {
  if (value === null) return null;
  const row = exact(value, ["source", "participant"]);
  return { source: decodeSource(row.source), participant: integer(row.participant, 1) };
}
export function decodeActors(value: unknown): EventActors {
  const row = object(value); const output: EventActors = {};
  for (const [role, values] of Object.entries(row)) {
    if (!["killer", "victim", "assister", "recipient", "acer"].includes(role)) throw new Error("source actor role");
    output[role as ActorRole] = array(values, 64).map(value => {
      const ref = exact(value, ["participant", "status"]);
      if (!["resolved", "unresolved", "conflict"].includes(ref.status as string)) throw new Error("source actor status");
      const participant = ref.participant === null ? null : integer(ref.participant, 1);
      if ((ref.status === "resolved") !== (participant !== null)) throw new Error("source actor identity/status");
      return { participant, status: ref.status as ActorReference["status"] };
    });
  }
  return output;
}
export function decodeEvidence(value: unknown): Evidence {
  const row = exact(value, ["id", "source", "endpoint", "timing", "game_stats", "calibration"]);
  const timing = exact(row.timing, ["start_us", "end_us", "received_at"]);
  const start = integer(timing.start_us), end = integer(timing.end_us);
  if (start > end || !receipt(timing.received_at) || !["game_stats", "active_player", "player_list", "events", "gameflow", "summoner", "end_of_game"].includes(row.endpoint as string)) throw new Error("source evidence");
  return { id: integer(row.id, 1), source: decodeSource(row.source), endpoint: row.endpoint as string,
    timing: { start_us: start, end_us: end, received_at: timing.received_at as string },
    game_stats: row.game_stats === null ? null : integer(row.game_stats, 1), calibration: row.calibration === null ? null : integer(row.calibration, 1) };
}
export function issues(value: unknown): void {
  const entries = Object.entries(object(value)); if (entries.length > 256) throw new Error("source field issue capacity");
  for (const [path, value] of entries) {
    text(path); const issue = object(value);
    if (issue.state === "not_applicable") { exact(value, ["state", "evidence"]); integer(issue.evidence, 1); }
    else if (["unavailable", "source_error", "invalid", "conflict"].includes(issue.state as string)) exact(value, ["state"]);
    else throw new Error("source field issue");
  }
}
