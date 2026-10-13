import type { LeagueMatch } from "./types";
import type { MediaId } from "./replayTime";

export type Outcome = "win" | "loss" | "unknown";
export type Coverage = "present" | "partial" | "missing";
export type LeagueResultSummary = { outcome: Outcome; ended_early: boolean; partial: boolean };
// Fixed allowlist matches chronobreak-league-data::Stats.
export const STAT_KEYS = ["kills", "deaths", "assists", "level", "minions", "jungle_minions", "ally_jungle_minions", "enemy_jungle_minions", "gold_earned", "gold_spent", "damage", "champion_damage", "objective_damage", "turret_damage", "physical_damage", "physical_champion_damage", "magic_damage", "magic_champion_damage", "true_damage", "true_champion_damage", "damage_taken", "physical_damage_taken", "magic_damage_taken", "true_damage_taken", "mitigated", "shielding", "healing", "teammate_healing", "crowd_control", "total_crowd_control", "time_dead", "units_healed", "vision", "wards_killed", "wards_placed", "control_wards_bought", "detector_wards_placed", "critical_strike", "killing_spree", "multi_kill", "double_kills", "triple_kills", "quadra_kills", "penta_kills", "turrets", "inhibitors", "barons", "dragons", "objectives_stolen", "objectives_stolen_assists", "first_blood", "first_blood_assist", "first_tower", "first_tower_assist", "spell1_casts", "spell2_casts", "spell3_casts", "spell4_casts", "summoner1_casts", "summoner2_casts"] as const;
export type FinalStats = Record<(typeof STAT_KEYS)[number], number | null>;
const FAMILY_KEYS = ["combat", "economy", "damage", "support", "vision", "objectives", "loadout", "runes"] as const;
export type FinalPlayer = {
  team_id: number | null; champion_id: number | null; champion_name: string | null;
  riot_id: string | null; is_local: boolean; spell1_id: number | null; spell2_id: number | null;
  position: string | null; detected_position: string | null; items: (number | null)[];
  perks: { id: number | null; counters: (number | null)[] }[];
  primary_style: number | null; secondary_style: number | null; augments: (number | null)[];
  subteam_id: number | null; subteam_placement: number | null; stats: FinalStats;
};
export type LeagueResult = {
  schema_version: 1; source: "lcu_eog"; confirmed: true; media_id: MediaId; game_id: string;
  outcome: Outcome; ended_early: boolean; coverage: Record<(typeof FAMILY_KEYS)[number], Coverage>;
  duration_seconds: number | null; end_timestamp_ms: number | null;
  queue_id: number | null; map_id: number | null; game_version: string | null;
  game_mode: string | null; game_type: string | null; queue_type: string | null; label_mismatch: boolean;
  local_player: FinalPlayer | null;
  teams: { team_id: number | null; is_winning: boolean | null; stats: FinalStats; players: FinalPlayer[] }[];
};
function record(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("result object");
  const object = value as Record<string, unknown>;
  if (Object.keys(object).length !== keys.length || keys.some(key => !(key in object))) throw new Error("result keys");
  return object;
}
const numeric = (v: unknown) => v === null || (typeof v === "number" && Number.isSafeInteger(v) && v >= 0);
const textual = (v: unknown) => v === null || (typeof v === "string" && v.length > 0 && new TextEncoder().encode(v).length <= 256 && !/[\u0000-\u001f\u007f-\u009f]/u.test(v));
function stats(v: unknown): void { if (!Object.values(record(v, STAT_KEYS)).every(numeric)) throw new Error("stats"); }
function player(v: unknown): void {
  const p = record(v, ["team_id", "champion_id", "champion_name", "riot_id", "is_local", "spell1_id", "spell2_id", "position", "detected_position", "items", "perks", "primary_style", "secondary_style", "augments", "subteam_id", "subteam_placement", "stats"]);
  if (!["team_id", "champion_id", "spell1_id", "spell2_id", "primary_style", "secondary_style", "subteam_id", "subteam_placement"].every(key => numeric(p[key]))
      || !["champion_name", "riot_id", "position", "detected_position"].every(key => textual(p[key])) || typeof p.is_local !== "boolean"
      || !Array.isArray(p.items) || p.items.length > 8 || !p.items.every(numeric)
      || !Array.isArray(p.augments) || p.augments.length !== 4 || !p.augments.every(numeric)
      || !Array.isArray(p.perks) || p.perks.length > 6) throw new Error("player");
  for (const perk of p.perks) { const entry = record(perk, ["id", "counters"]); if (!numeric(entry.id) || !Array.isArray(entry.counters) || entry.counters.length !== 3 || !entry.counters.every(numeric)) throw new Error("perk"); }
  stats(p.stats);
}
export function decodeLeagueResultSummary(v: unknown): LeagueResultSummary | null {
  try { const s = record(v, ["outcome", "ended_early", "partial"]); return ["win", "loss", "unknown"].includes(s.outcome as string) && typeof s.ended_early === "boolean" && typeof s.partial === "boolean" ? s as LeagueResultSummary : null; } catch { return null; }
}
export function decodeLeagueResult(v: unknown, media: MediaId, candidate: LeagueMatch | null): LeagueResult | null {
  try {
    const r = record(v, ["schema_version", "source", "confirmed", "media_id", "game_id", "outcome", "ended_early", "coverage", "duration_seconds", "end_timestamp_ms", "queue_id", "map_id", "game_version", "game_mode", "game_type", "queue_type", "label_mismatch", "local_player", "teams"]);
    if (!candidate || r.schema_version !== 1 || r.source !== "lcu_eog" || r.confirmed !== true || r.media_id !== media || r.game_id !== candidate.game_id
        || !["win", "loss", "unknown"].includes(r.outcome as string) || typeof r.ended_early !== "boolean" || typeof r.label_mismatch !== "boolean"
        || ![r.duration_seconds,r.end_timestamp_ms,r.queue_id,r.map_id].every(numeric) || ![r.game_version,r.game_mode,r.game_type,r.queue_type].every(textual)) return null;
    if (!Object.values(record(r.coverage, FAMILY_KEYS)).every(v => ["present", "partial", "missing"].includes(v as string))) return null;
    if (r.local_player !== null) player(r.local_player);
    if (!Array.isArray(r.teams) || r.teams.length > 16) return null;
    let count = 0;
    for (const team of r.teams) { const t = record(team, ["team_id", "is_winning", "stats", "players"]); if (!numeric(t.team_id) || (t.is_winning !== null && typeof t.is_winning !== "boolean") || !Array.isArray(t.players)) return null; stats(t.stats); count += t.players.length; for (const p of t.players) player(p); }
    if (count > 64) return null;
    return r as LeagueResult;
  } catch { return null; }
}
