import { invoke } from "@tauri-apps/api/core";
import { mockOverlayPage, mockReplayWindow } from "./dev/mockReplay";
import { decodeGameSummary, decodeLeagueMatch, decodeMappedReplayTick, loadGames, useMockApi } from "./api";
import { decodeLeagueResult, resultEvidence, type LeagueResult, type ResultEvidence } from "./leagueResult";
import { parseMediaId, parseReplayTick, type MediaId, type ReplayTick } from "./replayTime";
import type { GameSummary, LeagueMatch } from "./types";
import { array, decodeActors, decodeEvidence, decodeSource, exact, gameTick, integer, issues, nullableInteger, object, sameSource, text,
  type ActorRole, type EventActors, type Evidence, type SourceEpoch } from "./leagueSource";

const MOCK_IDENTITIES: Record<string, { riot_id: string; run_id: MediaId }> = {
  "9c82b929-7c85-4c27-a6d0-001786005616": { riot_id: "SUPERSTAR#VOID", run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174001") },
  "9c82b929-7c85-4c27-a6d0-001786004189": { riot_id: "Viktor Main#EUW", run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174002") },
  "9c82b929-7c85-4c27-a6d0-001785999000": { riot_id: "Jinx Enjoyer#EUW", run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174003") },
  "9c82b929-7c85-4c27-a6d0-001785900000": { riot_id: "Orianna Player#EUW", run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174004") },
  "9c82b929-7c85-4c27-a6d0-001785800000": { riot_id: "Ahri Player#EUW", run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174005") },
};
const mockIdentity = (media: MediaId) => {
  const identity = MOCK_IDENTITIES[media];
  if (!identity) throw new Error("Mock recording identity missing");
  return identity;
};

export type HistoryCursor = { recorded_at: string; media_id: MediaId };
export type HistoryQuery = { after?: HistoryCursor | null; limit: number; queue_id?: number | null; outcome?: "win" | "loss" | "unknown" | null;
  champion?: string | null; mode?: string | null; saved_only?: boolean | null; search?: string | null;
  participant_riot_id?: string | null; event_kind?: string | null; stolen?: boolean | null;
  minimum_kill_streak?: number | null; role?: ActorRole | null };
export type HistoryStats = { source: "final_eog" | "recorded_snapshot" | "unavailable"; kills: number | null; deaths: number | null; assists: number | null;
  items: Array<number | null> | null; summoner_spells: Array<string | null> | null; keystone_id: number | null; secondary_style: number | null; creep_score: number | null; duration_seconds: number | null;
  evidence: Evidence | null; final_evidence: ResultEvidence | null; game_tick: string | null };
export type HistoryRow = { media_id: MediaId; game: GameSummary; stats: HistoryStats; index_status: string };
export type HistoryPage = { rows: HistoryRow[]; next: HistoryCursor | null; indexed_recordings: number; catalog_recordings: number; unknown_recordings: number };
export type ImportProgress = { media_id: MediaId; status: string; batches: number; position: Position | null; tail_bytes: number };
type Position = { end_offset: number; last_sequence: number; checksum: number[] };
export type CatalogPage = { snapshot_token: string; items: Array<{ game_timestamp: string; media_id: MediaId | null; progress: ImportProgress | null; error: string | null }>;
  next: string | null; catalog_recordings: number };
export type SourceScope = { source: SourceEpoch; local_participant: number | null };
export type EventCursor = { game_tick: string; source_id: number };
export type WindowQuery = { scope: SourceEpoch; start: string; end: string; after?: EventCursor | null; limit: number; kind?: string | null;
  stolen?: boolean | null; minimum_kill_streak?: number | null; participant?: number | null; role?: ActorRole | null };
export type QueryCoverage = { index_status: string; committed: Position | null; uncommitted_tail_bytes: number; gap_attempts: number;
  unresolved_actors: number; unmapped_observations: number; successful_observations: number; observed_from: string | null; observed_through: string | null };
export type SourceEvent = { source_id: number; source_time?: number; game_tick: string; name?: string; killer?: string; victim?: string;
  assisters?: string[]; recipient?: string; kill_streak?: number; acer?: string; acing_team?: string; dragon_type?: string; stolen?: boolean;
  turret?: string; inhibitor?: string; result?: string; actors?: EventActors; issues?: Record<string, unknown> };
export type IndexedEvent = { revision: { event: SourceEvent; fingerprint: number[]; response_fingerprint: number[]; ingestion: string };
  effective_actors: EventActors | null;
  sequence: number; ordinal: number; evidence: Evidence; replay_tick?: ReplayTick };
export type EventWindow = SourceScope & { events: IndexedEvent[]; next: EventCursor | null; match_state: "observed_match" | "no_observed_match" | "unknown_coverage"; coverage: QueryCoverage };
export type RosterSpell = { id?: number; display_name?: string; raw_display_name?: string; raw_description?: string; issues?: Record<string, unknown> };
export type RosterPlayer = { participant?: number; riot_id?: string; champion_name?: string; team?: string; level?: number;
  is_dead?: boolean; respawn_timer?: number; summoner_spells?: { one?: RosterSpell; two?: RosterSpell; issues?: Record<string, unknown> };
  scores?: { kills?: number; deaths?: number; assists?: number; creep_score?: number; ward_score?: number };
  items?: Array<{ item_id?: number; slot?: number; [key: string]: unknown }>; [key: string]: unknown };
export type RosterAt = SourceScope & { game_tick: string | null; age_us: number | null; evidence: Evidence | null; players: RosterPlayer[]; coverage: QueryCoverage };
export type RecordingFacts = { media_id: MediaId; local_player_name: string | null; league_match: LeagueMatch | null; league_result: LeagueResult | null };
export type ReplayCursor = { replay_tick: ReplayTick; source_id: number };
export type ReplayWindowQuery = { scope: SourceEpoch; start: ReplayTick; end: ReplayTick; at: ReplayTick; after?: ReplayCursor | null; limit: number };
export type ReplayRoster = SourceScope & { game_tick: string; observed_replay_tick: ReplayTick; age_replay_ticks: ReplayTick; evidence: Evidence; players: RosterPlayer[]; coverage: QueryCoverage };
export type ReplayIdentity = { participant: number; riot_id: string | null; champion_name: string | null; team: string | null };
export type ReplayWindow = SourceScope & { events: IndexedEvent[]; next: ReplayCursor | null; roster: ReplayRoster | null; identities: ReplayIdentity[]; coverage: QueryCoverage };
export type OverlayCursor = { replay_tick: ReplayTick; observation_id: number };
export type OverlayQuery = { scope: SourceEpoch; after: OverlayCursor | null; revision: Position | null; limit: number };
export type OverlayPlayer = { participant: number; level: number | null; is_dead: boolean | null; respawn_timer: number | null;
  spells: [string | null, string | null] };
export type OverlayScores = { kills: number | null; deaths: number | null; assists: number | null; creep_score: number | null };
export type OverlaySample = OverlayCursor & { game_tick: string; players: OverlayPlayer[]; local_scores: OverlayScores | null };
export type OverlayPage = SourceScope & { revision: Position; identities: ReplayIdentity[] | null; samples: OverlaySample[]; next: OverlayCursor | null };

export const compareOverlayCursor = (a: OverlayCursor, b: OverlayCursor) =>
  a.replay_tick - b.replay_tick || a.observation_id - b.observation_id;

const status = (value: unknown): string => { if (!["ready", "pending", "unavailable"].includes(value as string)) throw new Error("index status"); return value as string; };
const digest = (value: unknown): number[] => { const bytes = array(value, 32); if (bytes.length !== 32) throw new Error("index digest"); return bytes.map(b => integer(b, 0, 255)); };
function position(value: unknown): Position | null {
  if (value === null) return null; const r = exact(value, ["end_offset", "last_sequence", "checksum"]);
  return { end_offset: integer(r.end_offset), last_sequence: integer(r.last_sequence), checksum: digest(r.checksum) };
}
function sourceScope(value: unknown): SourceScope {
  const r = object(value); return { source: decodeSource(r.source), local_participant: r.local_participant === null ? null : integer(r.local_participant, 1) };
}
function eventCursor(value: unknown): EventCursor | null {
  if (value === null) return null; const r = exact(value, ["game_tick", "source_id"]); return { game_tick: gameTick(r.game_tick), source_id: integer(r.source_id) };
}
function coverage(value: unknown): QueryCoverage {
  const r = exact(value, ["index_status", "committed", "uncommitted_tail_bytes", "gap_attempts", "unresolved_actors", "unmapped_observations", "successful_observations", "observed_from", "observed_through"]);
  return { index_status: status(r.index_status), committed: position(r.committed), uncommitted_tail_bytes: integer(r.uncommitted_tail_bytes), gap_attempts: integer(r.gap_attempts),
    unresolved_actors: integer(r.unresolved_actors), unmapped_observations: integer(r.unmapped_observations), successful_observations: integer(r.successful_observations),
    observed_from: r.observed_from === null ? null : gameTick(r.observed_from), observed_through: r.observed_through === null ? null : gameTick(r.observed_through) };
}
export function decodeImportProgress(value: unknown): ImportProgress {
  const r = exact(value, ["media_id", "status", "batches", "position", "tail_bytes"]);
  return { media_id: parseMediaId(r.media_id), status: status(r.status), batches: integer(r.batches, 0, 4), position: position(r.position), tail_bytes: integer(r.tail_bytes) };
}
export function decodeCatalogPage(value: unknown, token: string): CatalogPage {
  const r = exact(value, ["snapshot_token", "items", "next", "catalog_recordings"]); if (r.snapshot_token !== token) throw new Error("stale index catalog");
  return { snapshot_token: token, items: array(r.items, 4).map(value => {
    const item = exact(value, ["game_timestamp", "media_id", "progress", "error"]); const media = item.media_id === null ? null : parseMediaId(item.media_id);
    const progress = item.progress === null ? null : decodeImportProgress(item.progress);
    if (progress && progress.media_id !== media) throw new Error("catalog media identity");
    return { game_timestamp: text(item.game_timestamp, 64), media_id: media, progress, error: item.error === null ? null : text(item.error, 4096) };
  }), next: r.next === null ? null : text(r.next, 64), catalog_recordings: integer(r.catalog_recordings) };
}
export function decodeHistoryPage(value: unknown): HistoryPage {
  const r = exact(value, ["rows", "next", "indexed_recordings", "catalog_recordings", "unknown_recordings"]);
  const rows = array(r.rows, 50).map(value => {
    const row = exact(value, ["media_id", "game", "stats", "index_status"]); const media = parseMediaId(row.media_id); const game = decodeGameSummary(row.game, "history.game");
    if (media !== game.media_id) throw new Error("history media identity");
    const s = exact(row.stats, ["source", "kills", "deaths", "assists", "items", "summoner_spells", "keystone_id", "secondary_style", "creep_score", "duration_seconds", "evidence", "final_evidence", "game_tick"]);
    if (!["final_eog", "recorded_snapshot", "unavailable"].includes(s.source as string)) throw new Error("history stats source");
    const evidence = s.evidence === null ? null : decodeEvidence(s.evidence); if (evidence && evidence.source.media_id !== media) throw new Error("history evidence media");
    if (s.final_evidence !== null) resultEvidence(s.final_evidence);
    const stats: HistoryStats = { source: s.source as HistoryStats["source"], kills: nullableInteger(s.kills), deaths: nullableInteger(s.deaths), assists: nullableInteger(s.assists),
      keystone_id: nullableInteger(s.keystone_id), secondary_style: nullableInteger(s.secondary_style), creep_score: nullableInteger(s.creep_score), duration_seconds: nullableInteger(s.duration_seconds),
      summoner_spells: s.summoner_spells === null ? null : array(s.summoner_spells, 2).map(v => v === null ? null : text(v)),
      items: s.items === null ? null : array(s.items, 32).map(nullableInteger), evidence, final_evidence: s.final_evidence as ResultEvidence | null,
      game_tick: s.game_tick === null ? null : gameTick(s.game_tick) };
    return { media_id: media, game, stats, index_status: status(row.index_status) };
  });
  let next: HistoryCursor | null = null; if (r.next !== null) { const c = exact(r.next, ["recorded_at", "media_id"]); next = { recorded_at: text(c.recorded_at, 64), media_id: parseMediaId(c.media_id) }; }
  const indexed = integer(r.indexed_recordings), catalog = integer(r.catalog_recordings), unknown = integer(r.unknown_recordings);
  if (indexed > catalog || unknown > catalog) throw new Error("history catalog counts");
  return { rows, next, indexed_recordings: indexed, catalog_recordings: catalog, unknown_recordings: unknown };
}
function sourceEvent(value: unknown): SourceEvent {
  const r = object(value); const allowed = ["source_id", "source_time", "game_tick", "name", "killer", "victim", "assisters", "recipient", "kill_streak", "acer", "acing_team", "dragon_type", "stolen", "turret", "inhibitor", "result", "actors", "issues"];
  if (!("source_id" in r) || !("game_tick" in r)) throw new Error("indexed event identity/time");
  for (const [key, v] of Object.entries(r)) {
    if (!allowed.includes(key)) throw new Error("event source field");
    if (["source_id", "kill_streak"].includes(key)) integer(v);
    else if (key === "source_time") { if (typeof v !== "number" || !Number.isFinite(v)) throw new Error("source event time"); }
    else if (key === "game_tick") gameTick(v);
    else if (key === "actors") decodeActors(v);
    else if (key === "issues") issues(v);
    else if (key === "assisters") array(v, 64).forEach(v => text(v));
    else if (key === "stolen") { if (typeof v !== "boolean") throw new Error("event stolen field"); }
    else text(v);
  }
  return r as SourceEvent;
}
export function decodeEventWindow(value: unknown, requested: SourceEpoch): EventWindow {
  const r = exact(value, ["source", "local_participant", "events", "next", "match_state", "coverage"]); const scope = sourceScope(r);
  if (!sameSource(scope.source, requested)) throw new Error("event source scope changed");
  if (!["observed_match", "no_observed_match", "unknown_coverage"].includes(r.match_state as string)) throw new Error("event match state");
  const events = array(r.events, 512).map(value => {
    const row = exact(value, ["revision", "effective_actors", "sequence", "ordinal", "evidence", "mapped_replay_time"]); const revision = exact(row.revision, ["event", "fingerprint", "response_fingerprint", "ingestion"]);
    const evidence = decodeEvidence(row.evidence); if (!sameSource(evidence.source, scope.source) || evidence.endpoint !== "events") throw new Error("event observation scope");
    if (!["accepted", "unknown_kind", "invalid", "correction", "conflict"].includes(revision.ingestion as string)) throw new Error("event ingestion state");
    return { revision: { event: sourceEvent(revision.event), fingerprint: digest(revision.fingerprint), response_fingerprint: digest(revision.response_fingerprint), ingestion: revision.ingestion as string },
      effective_actors: row.effective_actors === null ? null : decodeActors(row.effective_actors),
      sequence: integer(row.sequence, 1), ordinal: integer(row.ordinal), evidence, replay_tick: decodeMappedReplayTick(row.mapped_replay_time, "indexed_event") };
  });
  return { ...scope, events, next: eventCursor(r.next), match_state: r.match_state as EventWindow["match_state"], coverage: coverage(r.coverage) };
}
export function decodeRosterAt(value: unknown, requested: SourceEpoch): RosterAt {
  const r = exact(value, ["source", "local_participant", "game_tick", "age_us", "evidence", "players", "coverage"]); const scope = sourceScope(r);
  if (!sameSource(scope.source, requested)) throw new Error("roster source scope changed");
  const evidence = r.evidence === null ? null : decodeEvidence(r.evidence); if (evidence && (!sameSource(evidence.source, scope.source) || evidence.endpoint !== "player_list")) throw new Error("roster observation scope");
  const players = array(r.players, 64).map(value => {
    const p = object(value); const allowed = ["participant", "riot_id", "riot_id_game_name", "riot_id_tag_line", "summoner_name", "champion_name", "raw_champion_name", "skin_id", "skin_name", "raw_skin_name", "team", "position", "is_bot", "level", "is_dead", "respawn_timer", "scores", "items", "summoner_spells", "runes", "issues"];
    if (Object.keys(p).some(key => !allowed.includes(key))) throw new Error("roster source field");
    for (const key of ["participant", "level", "skin_id"]) if (key in p) integer(p[key], key === "participant" ? 1 : 0);
    for (const key of ["riot_id", "riot_id_game_name", "riot_id_tag_line", "summoner_name", "champion_name", "raw_champion_name", "skin_name", "raw_skin_name", "team", "position"]) if (key in p) text(p[key]);
    if (p.is_dead !== undefined && typeof p.is_dead !== "boolean") throw new Error("roster death state");
    if (p.respawn_timer !== undefined && (typeof p.respawn_timer !== "number" || !Number.isFinite(p.respawn_timer) || p.respawn_timer < 0)) throw new Error("roster respawn timer");
    if (p.summoner_spells !== undefined) {
      const spells = object(p.summoner_spells);
      for (const [key, value] of Object.entries(spells)) {
        if (key === "issues") { issues(value); continue; }
        if (key !== "one" && key !== "two") throw new Error("roster spell slot");
        const label = object(value);
        for (const [field, fact] of Object.entries(label)) {
          if (field === "issues") issues(fact);
          else if (field === "id") integer(fact);
          else if (["display_name", "raw_display_name", "raw_description"].includes(field)) text(fact);
          else throw new Error("roster spell label");
        }
      }
    }
    if (p.issues !== undefined) issues(p.issues);
    if (p.scores !== undefined) { const scores = object(p.scores); for (const [key, v] of Object.entries(scores)) { if (key === "issues") issues(v); else if (["kills", "deaths", "assists", "creep_score"].includes(key)) integer(v); else if (key !== "ward_score" || typeof v !== "number" || !Number.isFinite(v) || v < 0) throw new Error("roster scores"); } }
    if (p.items !== undefined) array(p.items, 32).forEach(value => { const item = object(value); for (const key of ["item_id", "slot", "count", "price"]) if (key in item) integer(item[key]); });
    return p as RosterPlayer;
  });
  return { ...scope, game_tick: r.game_tick === null ? null : gameTick(r.game_tick), age_us: nullableInteger(r.age_us), evidence, players, coverage: coverage(r.coverage) };
}

export async function indexRecording(token: string, game: string, rebuild = false): Promise<ImportProgress> {
  if (useMockApi()) { const recording = (await loadGames()).find(g => g.timestamp === game); if (!recording?.media_id) throw new Error("Recording not found"); return { media_id: recording.media_id, status: "ready", batches: 0, position: null, tail_bytes: 0 }; }
  return decodeImportProgress(await invoke("index_league_recording", { snapshotToken: token, gameTimestamp: game, rebuild }));
}
export async function indexCatalog(token: string, after: string | null): Promise<CatalogPage> {
  if (useMockApi()) return { snapshot_token: token, items: [], next: null, catalog_recordings: (await loadGames()).length };
  return decodeCatalogPage(await invoke("index_league_catalog", { snapshotToken: token, after, limit: 4 }), token);
}
export async function resetLeagueIndex(token: string): Promise<{ catalog_recordings: number }> {
  if (useMockApi()) return { catalog_recordings: (await loadGames()).length };
  const result = exact(await invoke("reset_league_index", { snapshotToken: token }), ["catalog_recordings"]);
  return { catalog_recordings: integer(result.catalog_recordings) };
}
export async function queryHistory(token: string, query: HistoryQuery): Promise<HistoryPage> {
  if (useMockApi()) {
    const catalog = (await loadGames()).filter(game => game.media_id);
    let games = catalog.filter(game => (!query.champion || game.champion === query.champion) && (!query.mode || game.game_mode === query.mode) && (!query.saved_only || game.saved) && (!query.search || [game.champion, game.game_mode, game.timestamp].some(value => value.toLowerCase().includes(query.search!.toLowerCase()))));
    const unsupportedSemantic = !!(query.outcome || query.queue_id != null || query.participant_riot_id || query.event_kind || query.stolen != null || query.minimum_kill_streak != null || query.role);
    if (unsupportedSemantic) games = [];
    games.sort((a, b) => b.recorded_at.localeCompare(a.recorded_at) || a.media_id!.localeCompare(b.media_id!));
    if (query.after) games = games.filter(game => game.recorded_at < query.after!.recorded_at || (game.recorded_at === query.after!.recorded_at && game.media_id! > query.after!.media_id));
    const page = games.slice(0, Math.min(Math.max(query.limit, 1), 50));
    const last = page[page.length - 1];
    return { rows: page.map(game => ({ media_id: game.media_id!, game, index_status: "ready", stats: { source: "recorded_snapshot", kills: game.kills, deaths: game.deaths, assists: game.assists, items: game.items.map(i => i.item_id), keystone_id: game.keystone_id, secondary_style: null, summoner_spells: game.summoner_spells, creep_score: null, duration_seconds: null, evidence: null, final_evidence: null, game_tick: null } })), next: page.length === Math.min(Math.max(query.limit, 1), 50) && games.length > page.length && last ? { recorded_at: last.recorded_at, media_id: last.media_id! } : null, indexed_recordings: catalog.length, catalog_recordings: catalog.length, unknown_recordings: unsupportedSemantic ? catalog.length : 0 };
  }
  return decodeHistoryPage(await invoke("query_league_history", { snapshotToken: token, query }));
}
export async function sourceScopes(token: string, game: string): Promise<SourceScope[]> {
  if (useMockApi()) { const media = (await loadGames()).find(g => g.timestamp === game)?.media_id; return media ? [{ source: { media_id: media, run_id: mockIdentity(media).run_id, epoch: 0 }, local_participant: 1 }] : []; }
  return array(await invoke("league_source_scopes", { snapshotToken: token, gameTimestamp: game }), 256).map(value => { exact(value, ["source", "local_participant"]); return sourceScope(value); });
}

export function decodeRecordingFacts(value: unknown, media: MediaId): RecordingFacts {
  const r = exact(value, ["media_id", "local_player_name", "league_match", "league_result"]);
  if (parseMediaId(r.media_id) !== media) throw new Error("Recording facts identity changed");
  const candidate = decodeLeagueMatch(r.league_match, media);
  return { media_id: media, local_player_name: r.local_player_name === null ? null : text(r.local_player_name), league_match: candidate,
    league_result: decodeLeagueResult(r.league_result, media, candidate) };
}
export async function loadRecordingFacts(token: string, game: string, media: MediaId): Promise<RecordingFacts> {
  if (useMockApi()) return { media_id: media, local_player_name: mockIdentity(media).riot_id, league_match: null, league_result: null };
  return decodeRecordingFacts(await invoke("league_recording_facts", { snapshotToken: token, gameTimestamp: game }), media);
}
export function decodeReplayWindow(value: unknown, requested: SourceEpoch): ReplayWindow {
  const r = exact(value, ["source", "local_participant", "events", "next", "roster", "identities", "coverage"]);
  const window = decodeEventWindow({ source: r.source, local_participant: r.local_participant, events: r.events, next: null, match_state: "unknown_coverage", coverage: r.coverage }, requested);
  let next: ReplayCursor | null = null;
  if (r.next !== null) { const cursor = exact(r.next, ["replay_tick", "source_id"]); next = { replay_tick: parseReplayTick(cursor.replay_tick), source_id: integer(cursor.source_id) }; }
  let roster: ReplayRoster | null = null;
  if (r.roster !== null) {
    const row = exact(r.roster, ["source", "local_participant", "game_tick", "observed_replay_tick", "age_replay_ticks", "evidence", "players", "coverage"]);
    const observed = parseReplayTick(row.observed_replay_tick), age = parseReplayTick(row.age_replay_ticks);
    const source = decodeRosterAt({ source: row.source, local_participant: row.local_participant, game_tick: row.game_tick, age_us: Math.ceil(age / 48), evidence: row.evidence, players: row.players, coverage: row.coverage }, requested);
    if (source.game_tick === null || source.evidence === null) throw new Error("Replay roster evidence");
    if (source.local_participant !== window.local_participant) throw new Error("Replay local identity changed");
    roster = { ...source, game_tick: source.game_tick, evidence: source.evidence, observed_replay_tick: observed, age_replay_ticks: age };
  }
  const identities = array(r.identities, 64).map(value => {
    const identity = exact(value, ["participant", "riot_id", "champion_name", "team"]);
    return { participant: integer(identity.participant, 1), riot_id: identity.riot_id === null ? null : text(identity.riot_id),
      champion_name: identity.champion_name === null ? null : text(identity.champion_name), team: identity.team === null ? null : text(identity.team) };
  });
  if (new Set(identities.map(identity => identity.participant)).size !== identities.length) throw new Error("Duplicate replay identity participant");
  return { source: window.source, local_participant: window.local_participant, events: window.events, next, roster, identities, coverage: window.coverage };
}
export async function queryReplayWindow(token: string, game: string, query: ReplayWindowQuery): Promise<ReplayWindow> {
  if (useMockApi()) {
    const recording = (await loadGames()).find(candidate => candidate.timestamp === game);
    if (!recording?.media_id) throw new Error("Recording not found");
    const identity = mockIdentity(recording.media_id);
    if (!sameSource(query.scope, { media_id: recording.media_id, run_id: identity.run_id, epoch: 0 })) throw new Error("Mock replay source changed");
    return mockReplayWindow(recording, query, identity.riot_id);
  }
  return decodeReplayWindow(await invoke("query_league_replay_window", { snapshotToken: token, gameTimestamp: game, query: { ...query, start: String(query.start), end: String(query.end), at: String(query.at), after: query.after ? { ...query.after, replay_tick: String(query.after.replay_tick) } : null } }), query.scope);
}
export function decodeOverlayPage(value: unknown, query: OverlayQuery): OverlayPage {
  const r = exact(value, ["source", "local_participant", "revision", "identities", "samples", "next"]);
  const scope = sourceScope(r);
  if (!sameSource(scope.source, query.scope)) throw new Error("Overlay source changed");
  const revision = position(r.revision);
  if (!revision || (query.revision && JSON.stringify(revision) !== JSON.stringify(query.revision))) throw new Error("Overlay revision changed");
  if (Boolean(query.after) !== Boolean(query.revision)) throw new Error("Overlay continuation revision");
  const identities = r.identities === null ? null : array(r.identities, 64).map(value => {
    const p = exact(value, ["participant", "riot_id", "champion_name", "team"]);
    return { participant: integer(p.participant, 1), riot_id: p.riot_id === null ? null : text(p.riot_id),
      champion_name: p.champion_name === null ? null : text(p.champion_name), team: p.team === null ? null : text(p.team) };
  });
  if ((query.after === null) !== (identities !== null) || (identities && new Set(identities.map(p => p.participant)).size !== identities.length))
    throw new Error("Overlay source identities");
  const cursor = (value: unknown): OverlayCursor => {
    const c = exact(value, ["replay_tick", "observation_id"]);
    return { replay_tick: parseReplayTick(c.replay_tick), observation_id: integer(c.observation_id) };
  };
  let previous = query.after;
  const samples = array(r.samples, Math.min(128, integer(query.limit, 1, 128))).map(value => {
    const s = exact(value, ["replay_tick", "observation_id", "game_tick", "players", "local_scores"]);
    const key = cursor({ replay_tick: s.replay_tick, observation_id: s.observation_id });
    if (previous && compareOverlayCursor(key, previous) <= 0) throw new Error("Overlay sample order");
    previous = key;
    const players = array(s.players, 64).map(value => {
      const p = exact(value, ["participant", "level", "is_dead", "respawn_timer", "spells"]);
      if (p.is_dead !== null && typeof p.is_dead !== "boolean") throw new Error("Overlay death state");
      if (p.respawn_timer !== null && (typeof p.respawn_timer !== "number" || !Number.isFinite(p.respawn_timer) || p.respawn_timer < 0)) throw new Error("Overlay respawn timer");
      const spells = array(p.spells, 2).map(v => v === null ? null : text(v));
      if (spells.length !== 2) throw new Error("Overlay spell slots");
      return { participant: integer(p.participant, 1), level: nullableInteger(p.level), is_dead: p.is_dead as boolean | null,
        respawn_timer: p.respawn_timer as number | null, spells: spells as [string | null, string | null] };
    });
    if (new Set(players.map(p => p.participant)).size !== players.length) throw new Error("Duplicate overlay participant");
    const scores = s.local_scores === null ? null : exact(s.local_scores, ["kills", "deaths", "assists", "creep_score"]);
    return { ...key, game_tick: gameTick(s.game_tick), players,
      local_scores: scores === null ? null : { kills: nullableInteger(scores.kills), deaths: nullableInteger(scores.deaths),
        assists: nullableInteger(scores.assists), creep_score: nullableInteger(scores.creep_score) } };
  });
  const next = r.next === null ? null : cursor(r.next);
  if (next && (!samples.length || compareOverlayCursor(next, samples[samples.length - 1]) !== 0)) throw new Error("Overlay continuation cursor");
  if (new TextEncoder().encode(JSON.stringify(value)).length > 2 * 1024 * 1024) throw new Error("Overlay page capacity");
  return { ...scope, revision, identities, samples, next };
}
export async function queryOverlayPage(token: string, game: string, query: OverlayQuery): Promise<OverlayPage> {
  if (useMockApi()) {
    const recording = (await loadGames()).find(candidate => candidate.timestamp === game);
    if (!recording?.media_id) throw new Error("Recording not found");
    const identity = mockIdentity(recording.media_id);
    if (!sameSource(query.scope, { media_id: recording.media_id, run_id: identity.run_id, epoch: 0 })) throw new Error("Mock replay source changed");
    return mockOverlayPage(recording, query, identity.riot_id);
  }
  return decodeOverlayPage(await invoke("query_league_overlay_page", { snapshotToken: token, gameTimestamp: game,
    query: { ...query, after: query.after ? { ...query.after, replay_tick: String(query.after.replay_tick) } : null } }), query);
}
export async function queryEvents(token: string, game: string, query: WindowQuery): Promise<EventWindow> {
  return decodeEventWindow(await invoke("query_league_events", { snapshotToken: token, gameTimestamp: game, query }), query.scope);
}
export async function queryRoster(token: string, game: string, scope: SourceEpoch, tick: string): Promise<RosterAt> {
  return decodeRosterAt(await invoke("query_league_roster", { snapshotToken: token, gameTimestamp: game, scope, tick }), scope);
}
