import { expect, it } from "vitest";
import { decodeLeagueResult, decodeLeagueResultSummary, STAT_KEYS, type FinalStats, type LeagueResult } from "./leagueResult";
import { parseMediaId } from "./replayTime";
import type { LeagueMatch } from "./types";
const media = parseMediaId("11111111-2222-4333-8444-555555555555");
const candidate: LeagueMatch = { media_id: media, game_id: "9007199254740993", status: "provisional", queue_id: 400, map_id: 11, local_riot_id: "Synthetic#TEST", game_mode: "CLASSIC" };
const fixture = (): LeagueResult => ({
  schema_version: 1, source: "lcu_eog", confirmed: true, media_id: media, game_id: candidate.game_id,
  outcome: "win", ended_early: false, coverage: { combat: "partial", economy: "missing", damage: "missing", support: "missing", vision: "missing", objectives: "missing", loadout: "missing", runes: "missing" },
  duration_seconds: 600, end_timestamp_ms: null, queue_id: null, map_id: null, game_version: null,
  game_mode: "CLASSIC", game_type: null, queue_type: null, label_mismatch: false,
  local_player: { team_id: 100, champion_id: 103, champion_name: "Ahri", riot_id: "Synthetic#TEST", is_local: true,
    spell1_id: 4, spell2_id: 14, position: null, detected_position: null, items: [0, null, 1001], perks: [], primary_style: null, secondary_style: null,
    augments: [null, null, null, null], subteam_id: null, subteam_placement: null,
    stats: { ...Object.fromEntries(STAT_KEYS.map(key => [key, null])) as FinalStats, kills: 6, deaths: 2, assists: 9 } }, teams: [],
});
it("projects typed nullable final facts only with exact media and provisional game binding", () => {
  const result = fixture(); expect(decodeLeagueResult(result, media, candidate)).toEqual(result);
  expect(decodeLeagueResult(result, media, null)).toBeNull();
  expect(decodeLeagueResult({ ...result, game_id: "7" }, media, candidate)).toBeNull();
  expect(decodeLeagueResult({ ...result, media_id: "other" }, media, candidate)).toBeNull();
  expect(decodeLeagueResult({ confirmed: true }, media, candidate)).toBeNull();
  expect(decodeLeagueResult({ ...result, outcome: "unknown", ended_early: true }, media, candidate)?.outcome).toBe("unknown");
});
it("rejects unsupported, corrupt, overflowing and oversized optional facts", () => {
  for (const value of [null, {}, "corrupt", { ...fixture(), schema_version: 2 }, { ...fixture(), teams: Array(17).fill({}) }]) expect(decodeLeagueResult(value, media, candidate)).toBeNull();
  for (const value of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, "3", {}]) { const r = fixture(); r.local_player!.stats.kills = value as number; expect(decodeLeagueResult(r, media, candidate)).toBeNull(); }
  const r = fixture(); r.local_player!.items = Array(9).fill(1); expect(decodeLeagueResult(r, media, candidate)).toBeNull();
});
it("keeps the summary compact and explicit about unknown/partial outcomes", () => {
  expect(decodeLeagueResultSummary({ outcome: "unknown", ended_early: true, partial: true })).toEqual({ outcome: "unknown", ended_early: true, partial: true });
  expect(decodeLeagueResultSummary({ outcome: "remake", ended_early: true, partial: true })).toBeNull();
});
