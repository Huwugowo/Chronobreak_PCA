import { expect, it } from "vitest";
import { decodeLeagueResult, decodeLeagueResultSummary, STAT_KEYS, type FinalStats, type LeagueResult } from "./leagueResult";
import { parseMediaId } from "./replayTime";
import type { LeagueMatch } from "./types";
const media = parseMediaId("11111111-2222-4333-8444-555555555555");
const candidate: LeagueMatch = { media_id: media, game_id: "9007199254740993", status: "provisional", queue_id: 400, map_id: 11, local_riot_id: "Synthetic#TEST", game_mode: "CLASSIC" };
const fixture = (): LeagueResult => ({
  schema_version: 2, source: "lcu_eog", confirmed: true, media_id: media, game_id: candidate.game_id, source_facts: { game_id: candidate.game_id }, evidence: null,
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
  for (const value of [null, {}, "corrupt", { ...fixture(), schema_version: 1 }, { ...fixture(), teams: Array(17).fill({}) }]) expect(decodeLeagueResult(value, media, candidate)).toBeNull();
  for (const value of [-1, 1.5, Number.MAX_SAFE_INTEGER + 1, "3", {}]) { const r = fixture(); r.local_player!.stats.kills = value as number; expect(decodeLeagueResult(r, media, candidate)).toBeNull(); }
  const r = fixture(); r.local_player!.items = Array(9).fill(1); expect(decodeLeagueResult(r, media, candidate)).toBeNull();
});
it("keeps the summary compact and explicit about unknown/partial outcomes", () => {
  expect(decodeLeagueResultSummary({ outcome: "unknown", ended_early: true, partial: true })).toEqual({ outcome: "unknown", ended_early: true, partial: true });
  expect(decodeLeagueResultSummary({ outcome: "remake", ended_early: true, partial: true })).toBeNull();
});

it("retains source identifiers, aliases, missingness and request provenance through the v2 consumer", () => {
  const result = fixture();
  result.source_facts = { game_id: candidate.game_id, invalid: false, team_early_surrendered: true,
    local_player: { game_id: candidate.game_id, puuid: "synthetic", riot_id_game_name: "Synthetic", riot_id_tag_line: "TEST", is_local_player: true,
      items: [0, null], issues: { "items/1": { state: "unavailable" } }, stats: {
        stats: { ...Object.fromEntries(STAT_KEYS.map(key => [key, null])) as FinalStats, gold_earned: 0 },
        extras: { WIN: 0, PLAYER_SUBTEAM: 7 }, aliases: { kills: ["CHAMPIONS_KILLED", "kills"], gold_earned: ["GOLD_EARNED", "goldEarned"] },
        issues: { kills: { state: "conflict" }, deaths: { state: "unavailable" }, assists: { state: "source_error" } } } },
    teams: [{ team_id: 300, players: [{ issues: { $: { state: "invalid" } } }] }],
    issues: { caused_early_surrender: { state: "unavailable" } } };
  result.evidence = { run_id: media, timing: { start_us: 10, end_us: 20, received_at: "2026-10-16T00:00:00Z" } };
  expect(decodeLeagueResult(JSON.parse(JSON.stringify(result)), media, candidate)).toEqual(result);
  const bad = structuredClone(result); bad.source_facts.game_id = "7";
  expect(decodeLeagueResult(bad, media, candidate)).toBeNull();
  for (const invalid of [{ ...result.source_facts, unknown: 1 }, { ...result.source_facts, invalid: null }, { ...result.source_facts, local_player: { items: [-1] } }]) {
    expect(decodeLeagueResult({ ...result, source_facts: invalid }, media, candidate)).toBeNull();
  }
  expect(decodeLeagueResult({ ...result, evidence: { ...result.evidence, timing: { start_us: 20, end_us: 10, received_at: "fixture" } } }, media, candidate)).toBeNull();
});

it("rejects contradictory persisted identity, field issues, alias provenance and malformed receipt times", () => {
  const result = fixture();
  for (const facts of [
    { game_id: candidate.game_id, local_player: { game_id: "7" } },
    { game_id: candidate.game_id, teams: [{ players: [{ game_id: "7" }] }] },
    { game_id: candidate.game_id, local_player: { puuid: "synthetic", issues: { puuid: { state: "invalid" } } } },
    { game_id: candidate.game_id, issues: { fabricated: { state: "invalid" } } },
    { game_id: candidate.game_id, local_player: { items: [0], issues: { "items/0": { state: "invalid" } } } },
    { game_id: candidate.game_id, local_player: { items: [null], issues: { "items/1": { state: "invalid" } } } },
    { game_id: candidate.game_id, teams: [{ team_id: 100, issues: { team_id: { state: "invalid" } } }] },
    { game_id: candidate.game_id, teams: [{ players: [{ champion_id: 103, issues: { $: { state: "invalid" } } }] }] },
    { game_id: candidate.game_id, local_player: { stats: { stats: result.local_player!.stats, extras: {}, aliases: { kills: ["deaths"] }, issues: {} } } },
  ]) expect(decodeLeagueResult({ ...result, source_facts: facts }, media, candidate)).toBeNull();
  for (const received_at of ["fixture", "2026-02-30T12:00:00Z", "2026-01-01T25:00:00Z", "2026-01-01T00:00:00+24:00"]) {
    expect(decodeLeagueResult({ ...result, evidence: { run_id: media, timing: { start_us: 0, end_us: 1, received_at } } }, media, candidate)).toBeNull();
  }
});
