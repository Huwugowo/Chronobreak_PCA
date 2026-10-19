import type { LeagueMatch } from "./types";
import { parseMediaId, type MediaId } from "./replayTime";

export type Outcome = "win" | "loss" | "unknown";
export type Coverage = "present" | "partial" | "missing";
export type LeagueResultSummary = { outcome: Outcome; ended_early: boolean; partial: boolean };
// Fixed allowlist matches chronobreak-league-data::Stats.
export const STAT_KEYS = ["kills", "deaths", "assists", "level", "minions", "jungle_minions", "ally_jungle_minions", "enemy_jungle_minions", "gold_earned", "gold_spent", "damage", "champion_damage", "objective_damage", "turret_damage", "physical_damage", "physical_champion_damage", "magic_damage", "magic_champion_damage", "true_damage", "true_champion_damage", "damage_taken", "physical_damage_taken", "magic_damage_taken", "true_damage_taken", "mitigated", "shielding", "healing", "teammate_healing", "crowd_control", "total_crowd_control", "time_dead", "units_healed", "vision", "wards_killed", "wards_placed", "control_wards_bought", "detector_wards_placed", "critical_strike", "killing_spree", "multi_kill", "double_kills", "triple_kills", "quadra_kills", "penta_kills", "turrets", "inhibitors", "barons", "dragons", "objectives_stolen", "objectives_stolen_assists", "first_blood", "first_blood_assist", "first_tower", "first_tower_assist", "spell1_casts", "spell2_casts", "spell3_casts", "spell4_casts", "summoner1_casts", "summoner2_casts"] as const;
export type FinalStats = Record<(typeof STAT_KEYS)[number], number | null>;
const STAT_SOURCE_ALIASES: Record<string, readonly string[]> = {
  kills: ["CHAMPIONS_KILLED", "kills"],
  deaths: ["NUM_DEATHS", "deaths"],
  assists: ["ASSISTS", "assists"],
  level: ["LEVEL", "champLevel"],
  minions: ["MINIONS_KILLED", "totalMinionsKilled"],
  jungle_minions: ["NEUTRAL_MINIONS_KILLED", "neutralMinionsKilled"],
  ally_jungle_minions: ["NEUTRAL_MINIONS_KILLED_YOUR_JUNGLE", "totalAllyJungleMinionsKilled"],
  enemy_jungle_minions: ["NEUTRAL_MINIONS_KILLED_ENEMY_JUNGLE", "totalEnemyJungleMinionsKilled"],
  gold_earned: ["GOLD_EARNED", "goldEarned"],
  gold_spent: ["goldSpent"],
  damage: ["TOTAL_DAMAGE_DEALT", "totalDamageDealt"],
  champion_damage: ["TOTAL_DAMAGE_DEALT_TO_CHAMPIONS", "totalDamageDealtToChampions"],
  objective_damage: ["TOTAL_DAMAGE_DEALT_TO_OBJECTIVES", "damageDealtToObjectives"],
  turret_damage: ["TOTAL_DAMAGE_DEALT_TO_TURRETS", "damageDealtToTurrets"],
  physical_damage: ["PHYSICAL_DAMAGE_DEALT_PLAYER", "physicalDamageDealt"],
  physical_champion_damage: ["PHYSICAL_DAMAGE_DEALT_TO_CHAMPIONS", "physicalDamageDealtToChampions"],
  magic_damage: ["MAGIC_DAMAGE_DEALT_PLAYER", "magicDamageDealt"],
  magic_champion_damage: ["MAGIC_DAMAGE_DEALT_TO_CHAMPIONS", "magicDamageDealtToChampions"],
  true_damage: ["TRUE_DAMAGE_DEALT_PLAYER", "trueDamageDealt"],
  true_champion_damage: ["TRUE_DAMAGE_DEALT_TO_CHAMPIONS", "trueDamageDealtToChampions"],
  damage_taken: ["TOTAL_DAMAGE_TAKEN", "totalDamageTaken"],
  physical_damage_taken: ["PHYSICAL_DAMAGE_TAKEN", "physicalDamageTaken"],
  magic_damage_taken: ["MAGIC_DAMAGE_TAKEN", "magicDamageTaken"],
  true_damage_taken: ["TRUE_DAMAGE_TAKEN", "trueDamageTaken"],
  mitigated: ["TOTAL_DAMAGE_SELF_MITIGATED", "damageSelfMitigated"],
  shielding: ["TOTAL_DAMAGE_SHIELDED_ON_TEAMMATES", "totalDamageShieldedOnTeammates"],
  healing: ["TOTAL_HEAL", "totalHeal"],
  teammate_healing: ["TOTAL_HEAL_ON_TEAMMATES", "totalHealsOnTeammates"],
  crowd_control: ["TIME_CCING_OTHERS", "timeCCingOthers"],
  total_crowd_control: ["totalTimeCCDealt"],
  time_dead: ["totalTimeSpentDead"],
  units_healed: ["totalUnitsHealed"],
  vision: ["VISION_SCORE", "visionScore"],
  wards_killed: ["WARD_KILLED", "wardsKilled"],
  wards_placed: ["WARD_PLACED", "wardsPlaced"],
  control_wards_bought: ["VISION_WARDS_BOUGHT_IN_GAME", "visionWardsBoughtInGame"],
  detector_wards_placed: ["detectorWardsPlaced"],
  critical_strike: ["LARGEST_CRITICAL_STRIKE", "largestCriticalStrike"],
  killing_spree: ["LARGEST_KILLING_SPREE", "largestKillingSpree"],
  multi_kill: ["LARGEST_MULTI_KILL", "largestMultiKill"],
  double_kills: ["doubleKills"],
  triple_kills: ["tripleKills"],
  quadra_kills: ["quadraKills"],
  penta_kills: ["pentaKills"],
  turrets: ["TURRETS_KILLED", "turretKills"],
  inhibitors: ["BARRACKS_KILLED", "inhibitorKills"],
  barons: ["baronKills"],
  dragons: ["dragonKills"],
  objectives_stolen: ["objectivesStolen"],
  objectives_stolen_assists: ["objectivesStolenAssists"],
  first_blood: ["firstBloodKill"],
  first_blood_assist: ["firstBloodAssist"],
  first_tower: ["firstTowerKill"],
  first_tower_assist: ["firstTowerAssist"],
  spell1_casts: ["spell1Casts"],
  spell2_casts: ["spell2Casts"],
  spell3_casts: ["spell3Casts"],
  spell4_casts: ["spell4Casts"],
  summoner1_casts: ["summoner1Casts"],
  summoner2_casts: ["summoner2Casts"],
};
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
  schema_version: 2; source: "lcu_eog"; confirmed: true; media_id: MediaId; game_id: string;
  source_facts: SourceFinalFacts; evidence: ResultEvidence | null;
  outcome: Outcome; ended_early: boolean; coverage: Record<(typeof FAMILY_KEYS)[number], Coverage>;
  duration_seconds: number | null; end_timestamp_ms: number | null;
  queue_id: number | null; map_id: number | null; game_version: string | null;
  game_mode: string | null; game_type: string | null; queue_type: string | null; label_mismatch: boolean;
  local_player: FinalPlayer | null;
  teams: { team_id: number | null; is_winning: boolean | null; stats: FinalStats; players: FinalPlayer[] }[];
};
export type FieldIssue = { state: "unavailable" | "source_error" | "invalid" | "conflict" } | { state: "not_applicable"; evidence: number };
export type FieldIssues = Record<string, FieldIssue>;
export type SourceFinalStats = { stats: FinalStats; extras: Record<string, number>; aliases: Record<string, string[]>; issues: FieldIssues };
export type SourceFinalPlayer = {
  game_id?: string; puuid?: string; riot_id_game_name?: string; riot_id_tag_line?: string;
  team_id?: number; champion_id?: number; champion_name?: string; is_local_player?: boolean;
  spell1_id?: number; spell2_id?: number; selected_position?: string; detected_team_position?: string;
  items?: (number | null)[]; stats?: SourceFinalStats; issues?: FieldIssues;
};
export type SourceFinalTeam = { team_id?: number; is_winning_team?: boolean; players?: SourceFinalPlayer[]; stats?: SourceFinalStats; issues?: FieldIssues };
export type SourceFinalFacts = {
  game_id: string; local_player?: SourceFinalPlayer; teams?: SourceFinalTeam[];
  game_length?: number; end_of_game_timestamp?: number; game_mode?: string; game_version?: string;
  game_type?: string; queue_type?: string; queue_id?: number; map_id?: number;
  invalid?: boolean; game_ended_in_early_surrender?: boolean; team_early_surrendered?: boolean;
  caused_early_surrender?: boolean; early_surrender_accomplice?: boolean; issues?: FieldIssues;
};
export type ResultEvidence = { run_id: MediaId; timing: { start_us: number; end_us: number; received_at: string } };
const own = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
function record(value: unknown, keys: readonly string[]): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("result object");
  const object = value as Record<string, unknown>;
  if (Object.keys(object).length !== keys.length || keys.some(key => !own(object, key))) throw new Error("result keys");
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
const sourceText = (v: unknown) => typeof v === "string" && new TextEncoder().encode(v).length <= 1024 && !/[\u0000-\u001f\u007f-\u009f]/u.test(v);
const sourceNumber = (v: unknown) => v !== null && numeric(v);
function object(v: unknown): Record<string, unknown> {
  if (v === null || typeof v !== "object" || Array.isArray(v)) throw new Error("source object");
  return v as Record<string, unknown>;
}
function issues(v: unknown): void {
  for (const [key, value] of Object.entries(object(v))) {
    if (!key || new TextEncoder().encode(key).length > 128 || !sourceText(key)) throw new Error("issue key");
    const issue = object(value);
    // EOG supplies no observation-ID namespace for applicability references.
    record(issue, ["state"]);
    if (!["unavailable", "source_error", "invalid", "conflict"].includes(issue.state as string)) throw new Error("issue");
  }
}
function fields(v: unknown, rules: Record<string, (value: unknown) => unknown>, recordIssue = false, itemIssues = false): Record<string, unknown> {
  const out = object(v);
  for (const [key, value] of Object.entries(out)) {
    if (key === "issues") issues(value);
    else if (!own(rules, key) || rules[key](value) === false) throw new Error("source field");
  }
  for (const [key, issue] of Object.entries(object(out.issues ?? {}))) {
    if (object(issue).state === "conflict") throw new Error("source issue conflict");
    if (key === "$" && recordIssue && Object.keys(out).length === 1 && Object.keys(object(out.issues)).length === 1) continue;
    if (itemIssues && /^items\/[0-7]$/.test(key) && Array.isArray(out.items) && out.items[Number(key.slice(6))] === null) continue;
    if (!own(rules, key) || own(out, key)) throw new Error("source issue value");
  }
  return out;
}
function array(v: unknown, cap: number, validate: (value: unknown) => unknown): void {
  if (!Array.isArray(v) || v.length > cap) throw new Error("source array");
  for (const value of v) if (validate(value) === false) throw new Error("source array value");
}
function special(key: string): boolean {
  return ["WIN", "PERK_PRIMARY_STYLE", "PERK_SUB_STYLE", "PLAYER_SUBTEAM", "SUBTEAM_PLACEMENT"].includes(key)
    || /^PERK[0-5](?:_VAR[1-3])?$/.test(key) || /^PLAYER_AUGMENT[1-4]$/.test(key);
}
function sourceStats(v: unknown): void {
  const s = record(v, ["stats", "extras", "aliases", "issues"]);
  stats(s.stats); issues(s.issues);
  for (const [key, value] of Object.entries(object(s.extras))) if (!special(key) || !sourceNumber(value) || (key === "WIN" && (value as number) > 1)) throw new Error("extra");
  for (const [key, aliases] of Object.entries(object(s.aliases))) {
    if (!(STAT_KEYS as readonly string[]).includes(key) && !special(key)) throw new Error("alias key");
    array(aliases, 2, alias => sourceText(alias) && (special(key) ? alias === key : STAT_SOURCE_ALIASES[key]?.includes(alias as string)));
    if ((aliases as unknown[]).length < 1 || new Set(aliases as unknown[]).size !== (aliases as unknown[]).length) throw new Error("aliases");
  }
  for (const key of Object.keys(object(s.issues))) {
    if (!(STAT_KEYS as readonly string[]).includes(key) && !special(key)) throw new Error("stat issue");
    if (object(s.stats)[key] != null || key in object(s.extras)) throw new Error("stat issue value");
  }
}
function sourcePlayer(v: unknown, game: string): void {
  const p = fields(v, { game_id: sourceGameId, puuid: sourceText, riot_id_game_name: sourceText, riot_id_tag_line: sourceText,
    team_id: sourceNumber, champion_id: sourceNumber, champion_name: sourceText, is_local_player: v => typeof v === "boolean",
    spell1_id: sourceNumber, spell2_id: sourceNumber, selected_position: sourceText, detected_team_position: sourceText,
    items: v => array(v, 8, numeric), stats: sourceStats }, true, true);
  if (p.game_id !== undefined && p.game_id !== game) throw new Error("player game binding");
}
function sourceGameId(v: unknown): boolean { return typeof v === "string" && /^[1-9][0-9]*$/.test(v) && BigInt(v) <= 18446744073709551615n; }
function sourceFacts(v: unknown, game: string): void {
  let players = 0;
  const facts = fields(v, { game_id: sourceGameId, local_player: v => sourcePlayer(v, game),
    teams: v => array(v, 16, t => fields(t, { team_id: sourceNumber, is_winning_team: v => typeof v === "boolean", stats: sourceStats,
      players: v => { array(v, 64, p => sourcePlayer(p, game)); players += (v as unknown[]).length; } }, true)),
    game_length: sourceNumber, end_of_game_timestamp: sourceNumber, game_mode: sourceText, game_version: sourceText,
    game_type: sourceText, queue_type: sourceText, queue_id: sourceNumber, map_id: sourceNumber,
    invalid: v => typeof v === "boolean", game_ended_in_early_surrender: v => typeof v === "boolean", team_early_surrendered: v => typeof v === "boolean",
    caused_early_surrender: v => typeof v === "boolean", early_surrender_accomplice: v => typeof v === "boolean" });
  if (facts.game_id !== game || players > 64) throw new Error("source binding");
}
export function receipt(v: unknown): boolean {
  if (typeof v !== "string" || !textual(v)) return false;
  const m = /^(\d{4})-(\d{2})-(\d{2})[Tt](\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:[Zz]|[+-](\d{2}):(\d{2}))$/.exec(v);
  if (!m) return false;
  const [year, month, day, hour, minute, second] = m.slice(1, 7).map(Number);
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]
    && hour <= 23 && minute <= 59 && second <= 60 && Number(m[7] ?? 0) <= 23 && Number(m[8] ?? 0) <= 59;
}
export function resultEvidence(v: unknown): void {
  const e = record(v, ["run_id", "timing"]); parseMediaId(e.run_id);
  const t = record(e.timing, ["start_us", "end_us", "received_at"]);
  if (!sourceNumber(t.start_us) || !sourceNumber(t.end_us) || (t.start_us as number) > (t.end_us as number) || !receipt(t.received_at)) throw new Error("result timing");
}
export function decodeLeagueResultSummary(v: unknown): LeagueResultSummary | null {
  try { const s = record(v, ["outcome", "ended_early", "partial"]); return ["win", "loss", "unknown"].includes(s.outcome as string) && typeof s.ended_early === "boolean" && typeof s.partial === "boolean" ? s as LeagueResultSummary : null; } catch { return null; }
}
export function decodeLeagueResult(v: unknown, media: MediaId, candidate: LeagueMatch | null): LeagueResult | null {
  try {
    const r = record(v, ["schema_version", "source", "confirmed", "media_id", "game_id", "source_facts", "evidence", "outcome", "ended_early", "coverage", "duration_seconds", "end_timestamp_ms", "queue_id", "map_id", "game_version", "game_mode", "game_type", "queue_type", "label_mismatch", "local_player", "teams"]);
    if (!candidate || r.schema_version !== 2 || r.source !== "lcu_eog" || r.confirmed !== true || r.media_id !== media || r.game_id !== candidate.game_id
        || !["win", "loss", "unknown"].includes(r.outcome as string) || typeof r.ended_early !== "boolean" || typeof r.label_mismatch !== "boolean"
        || ![r.duration_seconds,r.end_timestamp_ms,r.queue_id,r.map_id].every(numeric) || ![r.game_version,r.game_mode,r.game_type,r.queue_type].every(textual)) return null;
    if (!Object.values(record(r.coverage, FAMILY_KEYS)).every(v => ["present", "partial", "missing"].includes(v as string))) return null;
    sourceFacts(r.source_facts, candidate.game_id);
    if (r.evidence !== null) resultEvidence(r.evidence);
    if (r.local_player !== null) player(r.local_player);
    if (!Array.isArray(r.teams) || r.teams.length > 16) return null;
    let count = 0;
    for (const team of r.teams) { const t = record(team, ["team_id", "is_winning", "stats", "players"]); if (!numeric(t.team_id) || (t.is_winning !== null && typeof t.is_winning !== "boolean") || !Array.isArray(t.players)) return null; stats(t.stats); count += t.players.length; for (const p of t.players) player(p); }
    if (count > 64) return null;
    return r as LeagueResult;
  } catch { return null; }
}
