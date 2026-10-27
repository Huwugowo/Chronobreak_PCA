// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ViewerScreen from "./ViewerScreen";
import { loadReplayDescriptor, loadServerMetrics } from "../api";
import { indexRecording, loadRecordingFacts, queryOverlayPage, queryReplayWindow, sourceScopes } from "../leagueIndex";
import type { OverlayPage, ReplayWindow } from "../leagueIndex";
import type { GameSummary, LeagueMatch, ReplayDescriptor, ReplayParticipant, ViewerEvent } from "../types";
import type { LeagueResult } from "../leagueResult";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";
import { buildScenarioActions, emitReplayBenchmarkEvent, replayBenchmarkObserver } from "../benchmark";
import { STAT_KEYS, type FinalStats } from "../leagueResult";
import { sourceKey } from "../leagueSource";
import { runBenchmarkDesktopResize } from "../benchmarkResize";
import timelineStyles from "./ReplayTimeline.module.css";
import * as playbackModule from "../playbackController";
import * as renderBenchmark from "../benchmarkRender";
import type { PlaybackMediaAdapter } from "../htmlVideoPlaybackAdapter";

const diagnosticHarness = vi.hoisted(() => ({ held: false }));
vi.mock("../benchmarkResize", () => ({ runBenchmarkDesktopResize: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../api", async original => ({ ...await original<typeof import("../api")>(),
  loadReplayDescriptor: vi.fn(), loadServerMetrics: vi.fn() }));
vi.mock("../leagueIndex", async original => ({ ...await original<typeof import("../leagueIndex")>(),
  loadRecordingFacts: vi.fn(), indexRecording: vi.fn(), sourceScopes: vi.fn(), queryOverlayPage: vi.fn(), queryReplayWindow: vi.fn() }));
vi.mock("../playbackDiagnostics", () => ({ createPlaybackDiagnostics: () => ({
  ready: diagnosticHarness.held ? new Promise(() => {}) : Promise.resolve(false), bind: vi.fn(), dispose: vi.fn(),
}) }));
vi.mock("../benchmark", async original => ({ ...await original<typeof import("../benchmark")>(),
  emitReplayBenchmarkEvent: vi.fn(), replayBenchmarkObserver: vi.fn(() => null),
  buildScenarioActions: vi.fn() }));

afterEach(() => { diagnosticHarness.held = false; vi.restoreAllMocks(); vi.mocked(loadRecordingFacts).mockReset(); vi.mocked(indexRecording).mockReset(); vi.mocked(sourceScopes).mockReset(); vi.mocked(queryOverlayPage).mockReset(); vi.mocked(queryReplayWindow).mockReset(); vi.mocked(emitReplayBenchmarkEvent).mockClear(); vi.mocked(replayBenchmarkObserver).mockReset().mockReturnValue(null); vi.mocked(buildScenarioActions).mockReset(); document.body.replaceChildren(); });

type ReplayFixture = { game: GameSummary; video_url: string; media_timeline: ReplayDescriptor["media_timeline"]; local_player_name: string | null; league_match: LeagueMatch | null; league_result: LeagueResult | null; participants: ReplayParticipant[]; events: ViewerEvent[] };
const replayProbe = (): ReplayFixture => ({
    game: { media_id: parseMediaId("11111111-2222-4333-8444-555555555555"), timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-08T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true, league_result: null },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
    local_player_name: null, league_match: null, league_result: null, participants: [], events: [],
});

const coverage = { index_status: "ready", committed: null, uncommitted_tail_bytes: 0, gap_attempts: 0,
  unresolved_actors: 0, unmapped_observations: 0, successful_observations: 1, observed_from: "0", observed_through: "14400000000" } as const;
const resolvedWindow = (probe: ReplayFixture, selected?: { media_id: typeof probe.media_timeline.mediaId; run_id: typeof probe.media_timeline.mediaId; epoch: number }) => {
  const source = selected ?? { media_id: probe.media_timeline.mediaId, run_id: parseMediaId("123e4567-e89b-42d3-a456-426614174001"), epoch: 0 };
  const evidence = { id: 1, source, endpoint: "player_list", timing: { start_us: 0, end_us: 0, received_at: "2026-10-16T00:00:00Z" }, game_stats: 1, calibration: 1 };
  const eventEvidence = { ...evidence, id: 2, endpoint: "events" };
  const events = probe.events.map((event, index) => ({ sequence: index + 1, ordinal: 0, evidence: eventEvidence, replay_tick: event.replay_tick!, revision: { event: { source_id: index + 1, game_tick: event.game_tick, name: event.event_type, killer: event.killer ?? undefined, victim: event.victim ?? undefined, assisters: event.assisters, actors: { killer: [{ status: "resolved", participant: 1, raw: "player1" }] } }, fingerprint: Array(32).fill(0), response_fingerprint: Array(32).fill(0), ingestion: "accepted" } }));
  return { source, local_participant: 1, events, next: null, coverage, identities: [{ participant: 1, team: "ORDER", riot_id: "player1", champion_name: "Ahri" }, { participant: 2, team: "CHAOS", riot_id: "enemy2", champion_name: "Zed" }], roster: { source, local_participant: 1, game_tick: "0", observed_replay_tick: 0 as ReplayTick,
    age_replay_ticks: 0 as ReplayTick, evidence, coverage, players: [
      { participant: 1, riot_id: "player1", champion_name: "Ahri", team: "ORDER", level:7, scores: { kills: 1, deaths: 2, assists: 3, creep_score:43 } },
      { participant: 2, riot_id: "enemy2", champion_name: "Zed", team: "CHAOS", scores: { kills: 4, deaths: 5, assists: 6 } },
    ] } } as any;
};
const overlayFor = (window: ReplayWindow): OverlayPage => ({ source: window.source, local_participant: window.local_participant,
  revision: { end_offset: 10, last_sequence: 1, checksum: Array(32).fill(0) }, identities: window.identities, next: null,
  samples: window.roster ? [{ replay_tick: window.roster.observed_replay_tick, observation_id: 1, game_tick: window.roster.game_tick,
    local_scores: { kills: 1, deaths: 2, assists: 3, creep_score: 43 }, players: window.roster.players.flatMap(p => p.participant === undefined ? [] : [{
      participant: p.participant, level: p.level ?? null, is_dead: p.is_dead ?? null, respawn_timer: p.respawn_timer ?? null,
      spells: [p.summoner_spells?.one?.display_name ?? null, p.summoner_spells?.two?.id ? String(p.summoner_spells.two.id) : null] }]) }] : [] });
const setFixture = (probe: ReplayFixture, epochs = [0]) => {
  const scopes = epochs.map(epoch => ({ source: { media_id: probe.media_timeline.mediaId,
    run_id: parseMediaId(`123e4567-e89b-42d3-a456-42661417400${epoch + 1}`), epoch }, local_participant: 1 }));
  const facts = { media_id: probe.media_timeline.mediaId, local_player_name: probe.local_player_name,
    league_match: probe.league_match, league_result: probe.league_result };
  vi.mocked(loadRecordingFacts).mockResolvedValue(facts);
  vi.mocked(indexRecording).mockResolvedValue({ media_id: probe.media_timeline.mediaId, status: "ready", batches: 1, position: null, tail_bytes: 0 });
  vi.mocked(sourceScopes).mockResolvedValue(scopes);
  vi.mocked(queryOverlayPage).mockImplementation(async (_token, _game, query) => ({ ...overlayFor(resolvedWindow(probe)), source: query.scope }));
  vi.mocked(queryReplayWindow).mockImplementation(async (_token, _game, query) => {
    const source = query.scope;
    const roster = { source, local_participant: 1, game_tick: "0", observed_replay_tick: query.at,
      age_replay_ticks: 0 as ReplayTick, evidence: { id: 1, source, endpoint: "player_list", timing: { start_us: 0, end_us: 0, received_at: "2026-10-16T00:00:00Z" }, game_stats: 1, calibration: 1 },
      coverage, players: [
        { participant: 1, riot_id: "player1", champion_name: "Ahri", team: "ORDER", scores: { kills: 1, deaths: 2, assists: 3 } },
        { participant: 2, riot_id: "enemy2", champion_name: "Zed", team: "CHAOS", scores: { kills: 4, deaths: 5, assists: 6 } },
      ] };
    const evidence = { id: 2, source, endpoint: "events", timing: { start_us: 0, end_us: 0, received_at: "2026-10-16T00:00:00Z" }, game_stats: 1, calibration: 1 };
    const events = probe.events.filter(event => event.replay_tick !== undefined && event.replay_tick >= query.start && event.replay_tick <= query.end)
      .map((event, index) => ({ sequence: index + 1, ordinal: 0, evidence, replay_tick: event.replay_tick!, revision: { event: { source_id: index + 1, game_tick: event.game_tick, name: event.event_type,
        killer: event.killer ?? undefined, victim: event.victim ?? undefined, assisters: event.assisters,
        actors: { killer: [{ status: "resolved", participant: 1, raw: "player1" }] } }, fingerprint: Array(32).fill(0), response_fingerprint: Array(32).fill(0), ingestion: "accepted" } }));
    return { source, local_participant: 1, events, next: null, roster, coverage, identities: roster.players.map(({ participant, team, riot_id, champion_name }) => ({ participant, team, riot_id, champion_name })) } as any;
  });
  return { scopes, facts };
};

it("shows opening spell icons without a video or timeline interaction and omits overlay team headings", async () => {
  const probe = replayProbe();
  setFixture(probe);
  const overlay = overlayFor(resolvedWindow(probe));
  overlay.samples[0].replay_tick = 25 * 48_000_000 as ReplayTick;
  overlay.samples[0].players[0].spells = ["Flash", "Ghost"];
  overlay.samples[0].players[0].is_dead = true;
  overlay.samples[0].players[0].respawn_timer = 10;
  overlay.samples[0].players[1].spells = ["Teleport", null];
  vi.mocked(queryOverlayPage).mockResolvedValue(overlay);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    ddragonAssetBaseUrl="http://assets.test/ddragon/16.20.1"
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(2));
    await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
    const rails = host.querySelector('[data-testid="champion-filter-fullscreen"]')!;
    expect(rails.querySelectorAll("h3")).toHaveLength(0);
    expect(rails.querySelector('[aria-label="ALLIES"]')).not.toBeNull();
    expect(rails.querySelector('[aria-label="ENEMIES"]')).not.toBeNull();
    const selectedImages = [...rails.querySelectorAll<HTMLImageElement>('[class*="spell"] img')].filter(img => !img.hidden);
    expect(selectedImages.map(img => img.src)).toEqual(["Flash", "Ghost", "Teleport"].map(label => `http://assets.test/ddragon/16.20.1/spell/${label}`));
    expect(selectedImages.every(img => img.parentElement?.title.startsWith("First recorded: "))).toBe(true);
    expect(rails.querySelectorAll('[data-testid="champion-level"], [data-testid="champion-respawn"], [data-dead="true"]')).toHaveLength(0);
    expect(host.querySelector("video")!.currentTime).toBe(0);
    expect([...host.querySelectorAll('[aria-label="Recorded state"] dd')].map(node => node.textContent?.replace(/\s+/g, " ").trim()))
      .toEqual(["— / — / —", "—", "—", "—"]);
  } finally { dispose(); }
});

it("shows confirmed partial EOG facts alongside recorded totals without replacing the video", async () => {
  const probe=replayProbe();
  probe.league_match={media_id:probe.media_timeline.mediaId,status:"provisional",game_id:"9007199254740993",queue_id:400,map_id:11,local_riot_id:"Synthetic#TEST",game_mode:"CLASSIC"};
  probe.league_result={schema_version:2,source:"lcu_eog",confirmed:true,media_id:probe.media_timeline.mediaId,game_id:probe.league_match.game_id,source_facts:{game_id:probe.league_match.game_id},evidence:null,outcome:"win",ended_early:false,
    coverage:{combat:"partial",economy:"missing",damage:"missing",support:"missing",vision:"missing",objectives:"missing",loadout:"missing",runes:"missing"},duration_seconds:600,end_timestamp_ms:null,queue_id:null,map_id:null,game_version:null,game_mode:"CLASSIC",game_type:null,queue_type:null,label_mismatch:false,
    local_player:{team_id:100,champion_id:103,champion_name:"Ahri",riot_id:"Synthetic#TEST",is_local:true,spell1_id:null,spell2_id:null,position:null,detected_position:null,items:[],perks:[],primary_style:null,secondary_style:null,augments:[null,null,null,null],subteam_id:null,subteam_placement:null,stats:{...Object.fromEntries(STAT_KEYS.map(k=>[k,null])) as FinalStats,kills:6,deaths:2,assists:9}},teams:[]};
  setFixture(probe);vi.mocked(loadReplayDescriptor).mockResolvedValue({snapshot_token:"snapshot",game_timestamp:"1",video_url:probe.video_url,media_timeline:probe.media_timeline});vi.mocked(loadServerMetrics).mockReturnValue(new Promise(()=>{}));
  vi.spyOn(HTMLMediaElement.prototype,"pause").mockImplementation(()=>{});vi.spyOn(HTMLMediaElement.prototype,"load").mockImplementation(()=>{});
  const host=document.createElement("div");document.body.append(host);const dispose=render(()=><ViewerScreen gameTimestamp="1" game={probe.game} readReplay={work=>work("snapshot")} onBack={()=>{}} onExportClip={()=>{}}/>,host);
  try{await vi.waitFor(()=>expect(host.querySelector('[data-testid="league-result"]')?.textContent).toContain("WIN · Partial final facts"));expect(host.querySelector('[data-testid="replay-coverage"]')).toBeNull();expect(host.textContent).toContain("RECORDED K / D / A— / — / —");expect(host.textContent).toContain("Match confirmed");expect(host.textContent).toContain("FINAL EOG K / D / A6 / 2 / 9");expect(host.querySelectorAll("video")).toHaveLength(1);}finally{dispose();}
});

it("marks only the persistent primary video across layout and recovery, then releases it", async () => {
  const probe = replayProbe();
  probe.league_match = { media_id: probe.media_timeline.mediaId, status: "provisional", game_id: "9007199254740993",
    queue_id: 0, local_riot_id: "Player#EUW", map_id: 11, game_mode: "CLASSIC" };
  setFixture(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  const loads = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const probeVideo = document.createElement("video"); document.body.append(probeVideo);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelectorAll("video")).toHaveLength(1));
    const primary = host.querySelector("video")!;
    await vi.waitFor(() => expect(host.querySelector('[data-testid="league-match-context"]')?.textContent)
      .toContain("Match provisional · Result unknown · GAME 9007199254740993 · QUEUE 0"));
    expect(host.textContent).toContain("RECORDED K / D / A");
    expect(host.textContent).not.toContain("FINAL K / D / A");
    expect(primary.closest('[data-testid="video-frame"]')).not.toBeNull();
    expect([...document.querySelectorAll('[data-qb-primary-playback="true"]')]).toEqual([primary]);
    expect(probeVideo.hasAttribute("data-qb-primary-playback")).toBe(false);
    const source = primary.src;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 },
      error: { get: () => ({ code: 3, message: "fixture decode error" }) } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    const selectSpeed = (value: string) => {
      const select = host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!;
      expect([...select.options].map((option) => option.value)).toEqual(["0.25", "0.5", "1", "2", "4", "8"]);
      select.value = value; select.dispatchEvent(new Event("change", { bubbles: true }));
    };
    selectSpeed("4");
    host.querySelector<HTMLButtonElement>('[aria-label="Mute replay"]')!.click();
    const volume = host.querySelector<HTMLInputElement>('[aria-label="Replay volume"]')!;
    volume.value = "37"; volume.dispatchEvent(new Event("input", { bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(host.querySelector('[data-testid="video-frame"]')?.getAttribute("data-fullscreen")).toBe("true");
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!.value).toBe("4");
    expect(host.querySelector<HTMLInputElement>('[aria-label="Replay volume"]')!.value).toBe("37");
    expect(host.querySelector('[aria-label="Mute replay"]')).not.toBeNull();
    primary.dispatchEvent(new Event("error"));
    expect(primary.src).not.toBe(source);
    expect(loads).toHaveBeenCalledTimes(2);
    expect(primary.playbackRate).toBe(4); expect(primary.muted).toBe(false); expect(primary.volume).toBe(0.37);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(host.querySelector("video")).toBe(primary);
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!.value).toBe("4");
    expect([...document.querySelectorAll('[data-qb-primary-playback="true"]')]).toEqual([primary]);
  } finally { dispose(); }
  expect(host.querySelector("video")).toBeNull();
  expect(document.querySelector('[data-qb-primary-playback="true"]')).toBeNull();
});

it("shows champion filters and event colors without a seek or a fabricated roster sample", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1000, height: 50, left: 0, right: 1000, top: 0, bottom: 50, x: 0, y: 0, toJSON: () => ({}) });
  const probe = replayProbe();
  probe.events = [{ event_type: "ChampionKill", game_tick: "1000000", replay_tick: 48_000_000 as ReplayTick,
    killer: "player1", victim: "enemy2", assisters: [], dragon_type: null, stolen: null, recipient: null,
    kill_streak: null, acer: null, acing_team: null, turret: null, inhibitor: null, result: null, relation: "ally" }];
  setFixture(probe);
  const initial = resolvedWindow(probe);
  initial.roster = null;
  initial.events[0].effective_actors = initial.events[0].revision.event.actors;
  vi.mocked(queryReplayWindow).mockResolvedValue(initial);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game} readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector('[data-event-icon="swords"]')?.classList.contains(timelineStyles.eventally)).toBe(true));
    expect(host.querySelector("video")!.currentTime).toBe(0);
    expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(2);
    expect(host.querySelector('[aria-label="Add Ahri, player1, from timeline filter"]')).not.toBeNull();
    host.querySelector<HTMLButtonElement>('[data-testid="champion-filter-option"]')!.click();
    expect(host.querySelector('[data-testid="champion-filter-option"]')?.getAttribute("aria-pressed")).toBe("true");
    expect(host.textContent).toContain("RECORDED K / D / A— / — / —");
    expect(host.querySelector('[data-testid="replay-coverage"]')).toBeNull();
    expect(queryReplayWindow).toHaveBeenCalledWith("snapshot", "1", expect.objectContaining({ at: 0 }));
  } finally { dispose(); }
});

it("switches recorded epochs without retaining stale participants or a stale window", async () => {
  const probe = replayProbe();
  const fixture = setFixture(probe, [0, 1]);
  let releaseFirst!: (value: unknown) => void;
  const first = new Promise<unknown>(resolve => { releaseFirst = resolve; });
  vi.mocked(queryReplayWindow).mockImplementation((_token, _game, query) =>
    query.scope.epoch === 0 ? first : Promise.resolve(resolvedWindow(probe, query.scope)));
  vi.mocked(queryOverlayPage).mockImplementation(async (_token, _game, query) =>
    overlayFor(query.scope.epoch === 0 ? await first as ReplayWindow : resolvedWindow(probe, query.scope)));
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game} readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    const select = await vi.waitFor(() => {
      const node = host.querySelector<HTMLSelectElement>('[aria-label="Recorded source"]');
      expect(node).not.toBeNull(); return node!;
    });
    await vi.waitFor(() => expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(2));
    select.value = sourceKey(fixture.scopes[0].source);
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.waitFor(() => expect(vi.mocked(queryReplayWindow).mock.calls.some(([, , query]) => query.scope.epoch === 0)).toBe(true));
    expect(host.textContent).toContain("RECORDED K / D / A— / — / —");
    expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(0);
    select.value = sourceKey(fixture.scopes[1].source);
    select.dispatchEvent(new Event("change", { bubbles: true }));
    releaseFirst(resolvedWindow(probe, fixture.scopes[0].source));
    await vi.waitFor(() => expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(2));
    expect(select.value).toBe(sourceKey(fixture.scopes[1].source));
    expect(host.textContent).toContain("RECORDED K / D / A— / — / —");
    await vi.waitFor(() => expect(vi.mocked(queryReplayWindow).mock.calls.some(([, , query]) => query.scope.epoch === 1)).toBe(true));
  } finally { dispose(); }
});

it.each([false, true])("keeps descriptor playback and clip state while details are held or fail (failure=%s)", async failure => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1000, height: 50, left: 0, right: 1000, top: 0, bottom: 50, x: 0, y: 0, toJSON: () => ({}) });
  const probe = replayProbe();
  probe.events = [{ event_type: "ChampionKill", game_tick: "1000000", replay_tick: 48_000_000 as ReplayTick,
    killer: "player", victim: "enemy", assisters: [], dragon_type: null, stolen: null, recipient: null, kill_streak: null,
    acer: null, acing_team: null, turret: null, inhibitor: null, result: null, relation: "ally" }];
  probe.participants = [{ summoner_name: "player", champion: "Ahri", relation: "ally" }];
  let resolveDetails!: (value: ReplayWindow) => void;
  let rejectDetails!: (reason: Error) => void;
  const pending = new Promise<ReplayWindow>((resolve, reject) => { resolveDetails = resolve; rejectDetails = reject; });
  setFixture(probe);
  vi.mocked(queryReplayWindow).mockReset().mockReturnValue(pending);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  let paused = true;
  vi.mocked(HTMLMediaElement.prototype.pause).mockImplementation(() => { paused = true; });
  const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(async () => { paused = false; });
  const loads = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const exported = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  const draft = { gameTimestamp: "1", mediaId: probe.media_timeline.mediaId,
    startFrame: 0 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} initialClipDraft={draft} onBack={() => {}} onExportClip={exported} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")).not.toBeNull());
    const primary = host.querySelector("video")!;
    const source = primary.src;
    const loadCount = loads.mock.calls.length;
    expect(host.querySelector('[data-testid="replay-details-state"]')?.textContent).toContain("Loading replay details");
    expect(host.querySelectorAll('[aria-label^="Seek to"]')).toHaveLength(0);
    expect(host.querySelector('[data-testid="fullscreen-hud"]')).not.toBeNull();
    // Source identities are ready independently of the held event page.
    await vi.waitFor(() => expect(host.querySelectorAll('[data-testid="champion-filter-option"]')).toHaveLength(2));
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "viewer_mounted")).toBe(true);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "playback_payload_ready")).toBe(false);
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 }, paused: { get: () => paused } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    primary.dispatchEvent(new Event("canplay"));
    primary.dispatchEvent(new Event("seeked"));
    const speed = host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!;
    speed.value = "4"; speed.dispatchEvent(new Event("change", { bubbles: true }));
    const preview = host.querySelector<HTMLButtonElement>('[aria-label="Preview selected clip"]')!;
    expect(preview.disabled).toBe(false);
    preview.click();
    primary.dispatchEvent(new Event("seeked"));
    await vi.waitFor(() => expect(play).toHaveBeenCalled());
    const rail = () => host.querySelector<HTMLElement>('[data-testid="replay-timeline"]')!;
    rail().querySelector<HTMLElement>('[role="slider"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "+", bubbles: true }));
    const viewport = () => [...host.querySelectorAll('[data-testid="timeline-viewport-readout"] > span')]
      .map(boundary => boundary.textContent).join(" - ");
    const zoomed = viewport();
    expect(zoomed).toContain("0:00 - 3:12");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(host.querySelector('[data-testid="video-frame"]')?.getAttribute("data-fullscreen")).toBe("true");
    expect(viewport()).toBe(zoomed);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    primary.currentTime = 3;
    primary.dispatchEvent(new Event("timeupdate"));
    const playCount = play.mock.calls.length;
    expect(paused).toBe(false);
    await vi.waitFor(() => expect(queryReplayWindow).toHaveBeenCalledTimes(1));
    if (failure) {
      rejectDetails(new Error("fixture details failed"));
      await vi.waitFor(() => expect(host.textContent).toContain("Replay details unavailable. Playback remains available."));
      expect(host.querySelector("video")).toBe(primary);
      vi.mocked(queryReplayWindow).mockResolvedValue(resolvedWindow(probe));
      [...host.querySelectorAll("button")].find(button => button.textContent === "Retry replay details")!.click();
    } else resolveDetails(resolvedWindow(probe));
    await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
    expect(host.querySelectorAll('[aria-label^="Seek to"]')).toHaveLength(1);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.filter(([kind]) => kind === "playback_payload_ready")).toHaveLength(1);
    expect(host.querySelector("video")).toBe(primary);
    expect(primary.src).toBe(source);
    expect(loads).toHaveBeenCalledTimes(loadCount);
    expect(primary.playbackRate).toBe(4);
    expect(primary.currentTime).toBe(3);
    expect(paused).toBe(false);
    expect(play).toHaveBeenCalledTimes(playCount);
    expect(viewport()).toBe(zoomed);
    expect(host.querySelector('[aria-label="Clip starts at 0:00"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Clip ends at 0:10"]')).not.toBeNull();
    [...host.querySelectorAll("button")].find(button => button.textContent?.includes("Export clip"))!.click();
    expect(exported).toHaveBeenCalledWith(draft);
    host.querySelector<HTMLButtonElement>('[data-testid="champion-filter-option"]')!.click();
    const handle = host.querySelector<HTMLButtonElement>('[aria-label="Clip ends at 0:10"]')!;
    const released = vi.fn();
    Object.assign(handle, { setPointerCapture: vi.fn(), hasPointerCapture: () => true, releasePointerCapture: released });
    const removed = vi.spyOn(handle, "removeEventListener");
    const pointer = (target: HTMLElement, kind: string, clientX: number) => {
      const event = new MouseEvent(kind, { bubbles: true, button: 0, clientX });
      Object.defineProperty(event, "pointerId", { value: 7 });
      target.dispatchEvent(event);
    };
    pointer(handle, "pointerdown", 52);
    pointer(handle, "pointermove", 100);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(handle.isConnected).toBe(true);
    expect(host.querySelector('[aria-label^="Clip ends at"]')).toBe(handle);
    expect(released).not.toHaveBeenCalled();
    pointer(handle, "pointercancel", 100);
    expect(released).toHaveBeenCalledWith(7);
    for (const kind of ["pointermove", "pointerup", "pointercancel", "lostpointercapture"]) {
      expect(removed.mock.calls.some(([event]) => event === kind)).toBe(true);
    }
    const endpoint = () => host.querySelector('[aria-label^="Clip ends at"]')?.getAttribute("aria-label");
    const editedEndpoint = endpoint();
    pointer(handle, "pointermove", 900);
    pointer(handle, "pointerup", 900);
    expect(endpoint()).toBe(editedEndpoint);
    expect(host.querySelector('[data-testid="champion-filter-option"]')?.getAttribute("aria-pressed")).toBe("true");
    expect(viewport()).toBe(zoomed);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(endpoint()).toBe(editedEndpoint);
    expect(host.querySelector('[data-testid="champion-filter-option"]')?.getAttribute("aria-pressed")).toBe("true");
    expect(host.querySelector("video")).toBe(primary);
    expect(primary.src).toBe(source);
    expect(loads).toHaveBeenCalledTimes(loadCount);
  } finally { dispose(); }
});

it("keeps descriptor errors local and retries without requesting details first", async () => {
  const probe = replayProbe();
  setFixture(probe);
  vi.mocked(loadReplayDescriptor).mockRejectedValueOnce(new Error("fixture unavailable"))
    .mockResolvedValueOnce({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url,
      media_timeline: probe.media_timeline });
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.textContent).toContain("Recording could not be opened."));
    expect(host.querySelector("video")).toBeNull();
    expect(queryReplayWindow).not.toHaveBeenCalled();
    [...host.querySelectorAll("button")].find(button => button.textContent === "Retry recording")!.click();
    await vi.waitFor(() => expect(host.querySelector("video")).not.toBeNull());
  } finally { dispose(); }
});

it("does not mount a late descriptor or request details after the viewer is disposed", async () => {
  const probe = replayProbe();
  let resolveDescriptor!: (value: import("../types").ReplayDescriptor) => void;
  vi.mocked(loadReplayDescriptor).mockReturnValueOnce(new Promise(resolve => { resolveDescriptor = resolve; }));
  setFixture(probe);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  dispose();
  resolveDescriptor({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url,
    media_timeline: probe.media_timeline });
  await Promise.resolve(); await Promise.resolve();
  expect(host.querySelector("video")).toBeNull();
  expect(queryReplayWindow).not.toHaveBeenCalled();
});


it.each(["media-first", "details-first"] as const)("starts the benchmark once after both readiness gates (%s)", async order => {
  const probe = replayProbe();
  const details = resolvedWindow(probe);
  Object.assign(details.roster.players[0], { is_dead:true, respawn_timer:12.4,
    summoner_spells:{one:{display_name:"Flash"},two:{id:6}} });
  let resolveDetails!: (value: ReplayWindow) => void;
  setFixture(probe);
  vi.mocked(queryOverlayPage).mockResolvedValue(overlayFor(details));
  vi.mocked(queryReplayWindow).mockReturnValue(new Promise<ReplayWindow>(resolve => { resolveDetails = resolve; }));
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  const observer = { nextMediaGeneration: () => 1, nextActionId: () => "seek-1", scenario: () => ({ kind: "cold_open" }),
    emit: vi.fn(), complete: vi.fn() };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  vi.mocked(buildScenarioActions).mockReturnValue([]);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const prototype = HTMLVideoElement.prototype;
  const originals = ["requestVideoFrameCallback", "cancelVideoFrameCallback"].map(name => Object.getOwnPropertyDescriptor(prototype, name));
  let frame!: VideoFrameRequestCallback;
  Object.defineProperty(prototype, "requestVideoFrameCallback", { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
  Object.defineProperty(prototype, "cancelVideoFrameCallback", { configurable: true, value: () => {} });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    ddragonAssetBaseUrl="http://assets.test/ddragon/16.20.1"
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
    const primary = host.querySelector("video")!;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
    const readyMedia = () => {
      primary.dispatchEvent(new Event("loadstart"));
      primary.dispatchEvent(new Event("loadedmetadata"));
      primary.dispatchEvent(new Event("canplay"));
      expect(frame).toBeTypeOf("function");
      frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
    };
    if (order === "media-first") {
      readyMedia();
      await Promise.resolve();
      expect(buildScenarioActions).not.toHaveBeenCalled();
      resolveDetails(details);
      await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
    } else {
      resolveDetails(details);
      await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
      expect(buildScenarioActions).not.toHaveBeenCalled();
      readyMedia();
    }
    await vi.waitFor(() => expect(buildScenarioActions).toHaveBeenCalledTimes(1));
    expect(host.querySelector('[aria-label="Recorded state"]')?.textContent).toContain("CS43Level7Game clock0:00");
    const champion = host.querySelector('[data-player-name="player1"]')!;
    expect(champion.querySelector('[data-testid="champion-level"]')?.textContent).toBe("7");
    expect(champion.getAttribute("data-dead")).toBe("true");
    expect(champion.querySelector('[data-testid="champion-respawn"]')?.textContent).toBe("13");
    expect([...champion.querySelectorAll('[class*="spell"] img')].map(img=>img.getAttribute("src")))
      .toEqual(["http://assets.test/ddragon/16.20.1/spell/Flash","http://assets.test/ddragon/16.20.1/spell/6"]);
    primary.dispatchEvent(new Event("canplay"));
    frame(performance.now(), { mediaTime: 1 } as VideoFrameCallbackMetadata);
    await Promise.resolve();
    expect(buildScenarioActions).toHaveBeenCalledTimes(1);
    expect(champion.querySelector('[data-testid="champion-respawn"]')?.textContent).toBe("12");
    const spellImages = [...champion.querySelectorAll('[class*="spell"] img')];
    const queryCount = vi.mocked(queryReplayWindow).mock.calls.length;
    // A paused seek may present before seeked. Queue publications must neither
    // erase that frame nor reset the roster query to zero and remount assets.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
    expect(primary.paused).toBe(true);
    expect(primary.currentTime).toBe(16);
    expect(champion.querySelector('[data-testid="champion-respawn"]')?.textContent).toBe("12");
    expect(spellImages.every(image => image.isConnected)).toBe(true);
    frame(performance.now(), { mediaTime: 16 } as VideoFrameCallbackMetadata);
    primary.dispatchEvent(new Event("seeked"));
    primary.dispatchEvent(new Event("canplay"));
    expect(champion.querySelector('[data-testid="champion-respawn"]')?.textContent).toBe("0");
    expect(champion.getAttribute("data-dead")).toBe("true");
    expect([...champion.querySelectorAll('[class*="spell"] img')]).toEqual(spellImages);
    await new Promise(resolve => setTimeout(resolve, 150));
    expect(vi.mocked(queryReplayWindow).mock.calls.length).toBe(queryCount);
    expect(queryOverlayPage).toHaveBeenCalledTimes(1);
  } finally {
    dispose();
    ["requestVideoFrameCallback", "cancelVideoFrameCallback"].forEach((name, index) => {
      const descriptor = originals[index];
      if (descriptor) Object.defineProperty(prototype, name, descriptor);
      else Reflect.deleteProperty(prototype, name);
    });
  }
});

it.each(["warm_open", "whole_app", "lifecycle"])("notifies %s synchronously after release and required cycle telemetry", async kind => {
  const probe = replayProbe(); setFixture(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockResolvedValue({ requests: 1, range_requests: 1, response_bytes: 1,
    completed_streams: 1, cancelled_streams: 0, active_connections: 0, peak_connections: 1,
    rejected_connections: 0, rejected_requests: 0, active_streams: 0, peak_streams: 1 });
  const trace: string[] = [];
  const observer = { nextMediaGeneration: () => 1, nextActionId: () => "play-1", scenario: () => ({ kind }),
    emit: vi.fn((event: string) => { if (event === "viewer_cycle_completed") trace.push("cycle"); }), complete: vi.fn() };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  vi.mocked(buildScenarioActions).mockReturnValue([]);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(function(this: HTMLMediaElement) {
    if (!this.getAttribute("src")) trace.push("released");
  });
  let releaseDelay: (() => void) | undefined;
  const nativeTimeout = window.setTimeout.bind(window);
  vi.spyOn(window, "setTimeout").mockImplementation(((callback: TimerHandler, ms?: number) => {
    if (ms === 100 && vi.mocked(buildScenarioActions).mock.calls.length) { releaseDelay = callback as () => void; return 999; }
    return nativeTimeout(callback, ms);
  }) as typeof window.setTimeout);
  const prototype = HTMLVideoElement.prototype;
  const names = ["requestVideoFrameCallback", "cancelVideoFrameCallback"];
  const originals = names.map(name => Object.getOwnPropertyDescriptor(prototype, name));
  let frame!: VideoFrameRequestCallback;
  Object.defineProperty(prototype, names[0], { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
  Object.defineProperty(prototype, names[1], { configurable: true, value: () => {} });
  const host = document.createElement("div"); document.body.append(host);
  let dispose = () => {};
  const completed = vi.fn(() => { trace.push("callback"); dispose(); trace.push("unmounted"); });
  dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}}
    onBenchmarkViewerCycleCompleted={completed} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
    const primary = host.querySelector("video")!;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
    primary.dispatchEvent(new Event("loadstart")); primary.dispatchEvent(new Event("loadedmetadata")); primary.dispatchEvent(new Event("canplay"));
    frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
    await vi.waitFor(() => expect(releaseDelay).toBeTypeOf("function"));
    expect(trace).toEqual(["released"]); expect(completed).not.toHaveBeenCalled();
    releaseDelay!();
    await vi.waitFor(() => expect(completed).toHaveBeenCalledOnce());
    expect(trace.slice(0, 4)).toEqual(["released", "cycle", "callback", "unmounted"]);
    expect(observer.emit).toHaveBeenCalledWith("viewer_cycle_completed", expect.objectContaining({ kind }), { generation: expect.any(Number), required: true });
    expect(observer.complete).not.toHaveBeenCalled();
    expect(host.querySelector("video")).toBeNull();
  } finally {
    dispose();
    names.forEach((name, index) => {
      if (originals[index]) Object.defineProperty(prototype, name, originals[index]!);
      else Reflect.deleteProperty(prototype, name);
    });
  }
});

it.each(["presented-first", "seeked-first", "timeout", "disposed", "superseded", "layout-disposed"] as const)(
  "retains the final scrub until complete native/presentation evidence (%s)", async outcome => {
    const probe = replayProbe();
    setFixture(probe);
    vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
      video_url: probe.video_url, media_timeline: probe.media_timeline });
    vi.mocked(loadServerMetrics).mockResolvedValue({ requests: 0, range_requests: 0, response_bytes: 0,
      completed_streams: 0, cancelled_streams: 0, active_connections: 0, peak_connections: 0,
      rejected_connections: 0, rejected_requests: 0, active_streams: 0, peak_streams: 0 });
    let action = 0;
    const observer = { nextMediaGeneration: () => 1, nextActionId: () => `seek-${++action}`,
      scenario: () => ({ kind: "scrub" }), emit: vi.fn(), complete: vi.fn() };
    vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
    vi.mocked(buildScenarioActions).mockReturnValue(outcome === "layout-disposed"
      ? [{ kind: "fullscreen", enabled: true, seekTargetMs: 5000 }]
      : [{ kind: "scrub", targetsMs: [5000], intervalMs: 0 }]);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
    vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
    let timeout!: () => void;
    const nativeTimeout = window.setTimeout.bind(window);
    vi.spyOn(window, "setTimeout").mockImplementation((handler, delay, ...args) => {
      if (delay === 6000 && typeof handler === "function") timeout = handler as () => void;
      return nativeTimeout(handler, delay, ...args);
    });
    const prototype = HTMLVideoElement.prototype;
    const names = ["requestVideoFrameCallback", "cancelVideoFrameCallback"];
    const originals = names.map(name => Object.getOwnPropertyDescriptor(prototype, name));
    let frame!: VideoFrameRequestCallback;
    Object.defineProperty(prototype, names[0], { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
    Object.defineProperty(prototype, names[1], { configurable: true, value: () => {} });
    const host = document.createElement("div"); document.body.append(host);
    const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
      readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
    let disposed = false;
    const seen = (kind: string) => vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(call => call[0] === kind);
    try {
      await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
      const primary = host.querySelector("video")!;
      Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
      primary.dispatchEvent(new Event("loadstart"));
      primary.dispatchEvent(new Event("loadedmetadata"));
      primary.dispatchEvent(new Event("canplay"));
      frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
      await vi.waitFor(() => expect(seen("seek_dispatched")).toBe(true));
      // Reproduce the last scrub request whose RVFC can precede native seeked.
      if (outcome === "seeked-first") primary.dispatchEvent(new Event("seeked"));
      else frame(performance.now(), { mediaTime: 5 } as VideoFrameCallbackMetadata);
      await Promise.resolve(); await Promise.resolve();
      expect(seen("scrub_settled")).toBe(false);
      expect(seen("viewer_disposed")).toBe(false);
      expect(observer.complete).not.toHaveBeenCalled();
      if (outcome === "disposed" || outcome === "layout-disposed") { dispose(); disposed = true; }
      else if (outcome === "timeout") timeout();
      else if (outcome === "superseded") window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
      else if (outcome === "seeked-first") frame(performance.now(), { mediaTime: 5 } as VideoFrameCallbackMetadata);
      else primary.dispatchEvent(new Event("seeked"));
      await vi.waitFor(() => expect(observer.complete).toHaveBeenCalled());
      if (outcome === "presented-first" || outcome === "seeked-first") {
        expect(seen("scrub_settled")).toBe(true);
        expect(observer.complete).toHaveBeenCalledWith("complete", null, expect.anything());
        const kinds = vi.mocked(emitReplayBenchmarkEvent).mock.calls.map(call => call[0]);
        expect(kinds.indexOf("viewer_disposed")).toBeGreaterThan(kinds.indexOf("seeked"));
        expect(kinds.indexOf("viewer_disposed")).toBeGreaterThan(kinds.indexOf("seek_presented"));
      } else {
        expect(seen("scrub_settled")).toBe(false);
        expect(observer.complete).toHaveBeenCalledWith("failed", expect.any(String));
      }
    } finally {
      if (!disposed) dispose();
      names.forEach((name, index) => {
        if (originals[index]) Object.defineProperty(prototype, name, originals[index]!);
        else Reflect.deleteProperty(prototype, name);
      });
    }
  },
);

it("passes the mounted primary video to native resize after playback takes ownership", async () => {
  const probe = replayProbe();
  setFixture(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  const observer = { nextMediaGeneration: () => 1, scenario: () => ({ kind: "desktop_resize" }),
    emit: vi.fn(), complete: vi.fn() };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  vi.mocked(buildScenarioActions).mockReturnValue([{ kind: "desktop-resize" }]);
  vi.mocked(runBenchmarkDesktopResize).mockClear();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const prototype = HTMLVideoElement.prototype;
  const names = ["requestVideoFrameCallback", "cancelVideoFrameCallback"];
  const originals = names.map(name => Object.getOwnPropertyDescriptor(prototype, name));
  let frame!: VideoFrameRequestCallback;
  Object.defineProperty(prototype, names[0], { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
  Object.defineProperty(prototype, names[1], { configurable: true, value: () => {} });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
    const primary = host.querySelector("video")!;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    primary.dispatchEvent(new Event("canplay"));
    frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
    await vi.waitFor(() => expect(runBenchmarkDesktopResize).toHaveBeenCalledTimes(1));
    expect(vi.mocked(runBenchmarkDesktopResize).mock.calls[0][2]).toBe(primary);
    expect(primary.isConnected).toBe(true);
  } finally {
    dispose();
    names.forEach((name, index) => {
      if (originals[index]) Object.defineProperty(prototype, name, originals[index]!);
      else Reflect.deleteProperty(prototype, name);
    });
  }
});

it.each(["media-first", "details-first", "details-fail", "details-timeout", "disposed"] as const)(
  "versioned opening plays from paused RVFC and admits a positive terminal only with details (%s)", async outcome => {
    diagnosticHarness.held = true;
    const probe = replayProbe();
    let resolveDetails!: (value: ReplayWindow) => void;
    let rejectDetails!: (error: Error) => void;
    setFixture(probe);
    const heldWindow = new Promise<ReplayWindow>((resolve, reject) => { resolveDetails = resolve; rejectDetails = reject; });
    // The controller observes this asynchronously; retain a handler so intentional failure cases do not become a test-harness rejection.
    void heldWindow.catch(() => {});
    vi.mocked(queryReplayWindow).mockReturnValue(heldWindow);
    vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
      video_url: probe.video_url, media_timeline: probe.media_timeline });
    vi.mocked(loadServerMetrics).mockResolvedValue({ requests: 1, range_requests: 1, response_bytes: 1,
      completed_streams: 1, cancelled_streams: 0, active_connections: 0, peak_connections: 1,
      rejected_connections: 0, rejected_requests: 0, active_streams: 0, peak_streams: 1 });
    const opening = Object.freeze({ cycle: 0, fixture_id: "S", gameTimestamp: "1" });
    const observer = { nextMediaGeneration: () => 1, nextActionId: () => "play-1",
      scenario: () => ({ id: "qb010-opening-v1-s-cold", kind: "cold_open" }),
      openingCycle: () => opening, emitOpening: vi.fn(), emit: vi.fn(), complete: vi.fn() };
    vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
    vi.mocked(buildScenarioActions).mockReturnValue([{ kind: "play" }]);
    let paused = true;
    vi.spyOn(HTMLMediaElement.prototype, "paused", "get").mockImplementation(() => paused);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(function(this: HTMLMediaElement) { paused = true; this.dispatchEvent(new Event("pause")); });
    const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(async function(this: HTMLMediaElement) { paused = false; this.dispatchEvent(new Event("play")); });
    vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
    let readinessTimeout: (() => void) | undefined;
    const nativeTimeout = window.setTimeout.bind(window);
    vi.spyOn(window, "setTimeout").mockImplementation(((callback: TimerHandler, ms?: number) => {
      if (ms === 30_000 && !readinessTimeout) readinessTimeout = callback as () => void;
      return nativeTimeout(callback, ms);
    }) as typeof window.setTimeout);
    const prototype = HTMLVideoElement.prototype;
    const names = ["requestVideoFrameCallback", "cancelVideoFrameCallback"];
    const originals = names.map(name => Object.getOwnPropertyDescriptor(prototype, name));
    let frame!: VideoFrameRequestCallback;
    Object.defineProperty(prototype, names[0], { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
    Object.defineProperty(prototype, names[1], { configurable: true, value: () => {} });
    const host = document.createElement("div"); document.body.append(host);
    const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
      readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
    try {
      await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
      await vi.waitFor(() => expect(resolveDetails).toBeTypeOf("function"));
      const primary = host.querySelector("video")!;
      Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
      if (outcome === "details-first") {
        resolveDetails(resolvedWindow(probe));
        await vi.waitFor(() => expect(observer.emitOpening).toHaveBeenCalledWith("opening_details_ready", opening));
      }
      primary.dispatchEvent(new Event("loadstart"));
      primary.dispatchEvent(new Event("loadedmetadata"));
      primary.dispatchEvent(new Event("canplay"));
      expect(paused).toBe(true);
      expect(play).not.toHaveBeenCalled();
      frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
      await vi.waitFor(() => expect(play).toHaveBeenCalledTimes(1));
      expect(observer.emitOpening.mock.calls.map(([kind]) => kind).slice(0, 3)).toEqual([
        "opening_descriptor_requested", "opening_descriptor_ready", "opening_media_open_requested",
      ]);
      const load = vi.mocked(emitReplayBenchmarkEvent).mock.calls.find(([kind]) => kind === "media_loadstart")!;
      expect(observer.emitOpening).toHaveBeenCalledWith("opening_media_open_bound", opening, { generation: load[2]!.generation });
      expect(emitReplayBenchmarkEvent).toHaveBeenCalledWith("play_requested", expect.objectContaining({
        measurement_contract: "replay-opening-v1", cycle: 0, fixture_id: "S",
      }), expect.objectContaining({ generation: load[2]!.generation }));
      if (outcome !== "details-first") {
        expect(observer.complete).not.toHaveBeenCalled();
        expect(observer.emit).not.toHaveBeenCalledWith("scenario_completed", expect.anything(), expect.anything());
      }
      if (outcome === "disposed") dispose();
      else if (outcome === "details-fail") rejectDetails(new Error("details unavailable"));
      else if (outcome === "details-timeout") readinessTimeout!();
      else {
        resolveDetails(resolvedWindow(probe));
        await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
      }
      await vi.waitFor(() => expect(observer.complete).toHaveBeenCalledTimes(1));
      expect(observer.complete.mock.calls[0][0]).toBe(outcome === "media-first" || outcome === "details-first" ? "complete" : "failed");
      if (["disposed", "details-fail", "details-timeout"].includes(outcome)) {
        expect(observer.emit).toHaveBeenCalledWith("scenario_failed", expect.objectContaining({
          measurement_contract: "replay-opening-v1", cycle: 0, fixture_id: "S",
        }), expect.objectContaining({ generation: load[2]!.generation }));
        resolveDetails(resolvedWindow(probe));
        await Promise.resolve(); await Promise.resolve();
        expect(observer.emit.mock.calls.some(([kind]) => kind === "scenario_completed")).toBe(false);
        expect(observer.complete).toHaveBeenCalledTimes(1);
      } else {
        expect(observer.emit).toHaveBeenCalledWith("scenario_completed", expect.objectContaining({
          measurement_contract: "replay-opening-v1", cycle: 0, fixture_id: "S",
        }), expect.objectContaining({ generation: expect.any(Number) }));
      }
    } finally {
      dispose();
      names.forEach((name, index) => {
        const original = originals[index];
        if (original) Object.defineProperty(prototype, name, original);
        else Reflect.deleteProperty(prototype, name);
      });
    }
  },
);


// Frozen before QB-DEV-007 extraction. These traces exercise the actual Viewer
// entry point and real action builder; transport evidence is delivered inside
// seek(), before its promise returns. Real controller/media integration remains
// covered above, and native render/resize behavior has separate helper tests.
it.each([
  { id: "whole_app", kind: "whole_app" },
  { id: "layout", kind: "layout" },
  { id: "lifecycle", kind: "lifecycle" },
  { id: "timed-life", kind: "lifecycle", duration_seconds: 30 },
  { id: "settled-memory", kind: "lifecycle" },
  { id: "rate", kind: "rate" },
  { id: "event", kind: "seek", target_times_ms: [10000, 30000, 90000], seek_reason: "event-jump" },
  { id: "endpoint", kind: "seek", target_times_ms: [10000, 30000, 90000], seek_reason: "endpoint-edit" },
  { id: "paused-seek", kind: "seek", target_times_ms: [10000, 30000, 90000], seek_playback_mode: "paused" },
  { id: "qb010-opening-v1-cold-open", kind: "cold_open" },
  { id: "qb010-opening-v1-warm-open", kind: "warm_open", iterations: 6 },
  { id: "resize", kind: "desktop_resize" },
  { id: "scrub", kind: "scrub" },
  { id: "play-pause", kind: "play_pause" },
])("freezes the ordered Viewer executor contract ($id)", async selected => {
  vi.useFakeTimers();
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => window.setTimeout(() => callback(performance.now()), 16));
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(id => window.clearTimeout(id));
  const actualBenchmark = await vi.importActual<typeof import("../benchmark")>("../benchmark");
  vi.mocked(buildScenarioActions).mockImplementation(actualBenchmark.buildScenarioActions);
  const probe = replayProbe();
  probe.events = [10, 30, 90].map(seconds => ({ event_type: "ChampionKill", replay_tick: seconds * 48_000_000 as ReplayTick,
    game_tick: String(seconds * 1_000_000), killer: "player1", victim: "enemy2", assisters: [], dragon_type: null,
    stolen: null, recipient: null, kill_streak: null, acer: null, acing_team: null, turret: null, inhibitor: null,
    result: null, relation: "ally" }));
  setFixture(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  const trace: string[] = [];
  const record = (kind: string, value?: unknown) => trace.push(value === undefined ? kind : `${kind} ${JSON.stringify(value)}`);
  let done = false, sequence = 0;
  const scenario = { trial_id: "frozen", seed: 20261027, ...selected };
  const opening = { cycle: 0, fixture_id: "S", gameTimestamp: "1" };
  const emit = (kind: string, payload: Record<string, unknown> = {}, meta: unknown = {}) => {
    record(`event:${kind}`, { payload, meta });
    if (kind === "viewer_cycle_completed" || kind === "scenario_completed") done = true;
    return true;
  };
  const observer = { nextMediaGeneration: () => 1, nextActionId: (kind: string) => `${kind}-${++sequence}`,
    scenario: () => scenario, openingCycle: () => opening,
    emitOpening: (kind: string, value: typeof opening, meta = {}) => emit(kind, actualBenchmark.openingPayload(value), meta),
    emit, complete: vi.fn(async (...args: unknown[]) => { record("terminal", args); done = true; }) };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  vi.mocked(emitReplayBenchmarkEvent).mockImplementation(emit);
  const metrics = { requests: 1, range_requests: 1, response_bytes: 1, completed_streams: 1, cancelled_streams: 0,
    active_connections: 0, peak_connections: 1, rejected_connections: 0, rejected_requests: 0, active_streams: 0, peak_streams: 1 };
  vi.mocked(loadServerMetrics).mockImplementation(async () => { record("server-metrics"); return metrics; });
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  const realCreate = playbackModule.createPlaybackController;
  const { createHtmlVideoPlaybackAdapter } = await import("../htmlVideoPlaybackAdapter");
  const baseController = realCreate(createHtmlVideoPlaybackAdapter(document.createElement("video")));
  const base = baseController.snapshot(); baseController.dispose();
  vi.spyOn(playbackModule, "createPlaybackController").mockImplementation((adapter: PlaybackMediaAdapter, options = {}) => {
    let paused = true, seconds = 0, at = performance.now(), rate = 1;
    let readiness: playbackModule.PlaybackSnapshot["readiness"] = "closed", generation = options.generation ?? 0;
    let sourceUrl: string | null = null, preview: ReplayTick | null = null;
    const subscribers = new Set<(state: playbackModule.PlaybackSnapshot) => void>();
    const frames = new Set<(tick: ReplayTick, authority: "rvfc" | "media-clock-approximate") => void>();
    const sync = () => { const now = performance.now(); if (!paused) seconds += (now - at) * rate / 1000; at = now; };
    const snapshot = (): playbackModule.PlaybackSnapshot => {
      sync();
      return { ...base, readiness, generation, mediaId: probe.media_timeline.mediaId, sourceUrl, sessionToken: "load",
        authority: "rvfc", queue: "IDLE", desiredPlaying: !paused,
        seek: { ...base.seek, generation, requestedPreview: preview, presented: preview },
        media: { ...base.media, source: sourceUrl ?? "", readyState: 4, networkState: 1, paused, ended: false,
          browserSeconds: seconds, tick: Math.round(seconds * 48_000_000) as ReplayTick },
        rate: { ...base.rate, selected: rate as playbackModule.PlaybackRate, applied: rate },
        quality: { totalFrames: 100, droppedFrames: 0, heapBytes: null } };
    };
    const publish = () => subscribers.forEach(callback => callback(snapshot()));
    const event = (kind: string, payload: Record<string, unknown> = {}, actionId?: string) =>
      options.eventSink?.({ kind, payload, actionId, generation, atMs: performance.now() });
    return {
      snapshot, sampleMetrics: () => { record("sample"); return snapshot(); },
      open: async input => {
        sourceUrl = input.url; generation++; readiness = "ready";
        adapter.open(input.url, input.timeline); publish();
        event("media_canplay"); event("first_presented_frame");
        frames.forEach(callback => callback(0 as ReplayTick, "rvfc"));
        return { status: "ready" };
      },
      play: async () => { sync(); paused = false; record("play"); publish(); return { status: "playing" }; },
      pause: () => { sync(); paused = true; record("pause"); publish(); return { status: "paused" }; },
      setRate: value => { sync(); rate = value; record("rate", value); publish(); },
      setLoopRange: range => record("loop", range),
      seek: (target, request = {}) => {
        record("seek", { target, ...request }); preview = target; seconds = target / 48_000_000; at = performance.now();
        event("seek_requested", { target_ms: target / 48_000 }, request.actionId);
        event("seek_dispatched", { target_ms: target / 48_000 }, request.actionId);
        event("seek_presented", { target_ms: target / 48_000, authoritative: true }, request.actionId);
        event("seeked", { target_ms: target / 48_000 }, request.actionId);
        frames.forEach(callback => callback(target, "rvfc")); publish();
        return Promise.resolve({ status: "presented", target, presented: target });
      },
      subscribe: callback => { subscribers.add(callback); callback(snapshot()); return () => subscribers.delete(callback); },
      subscribeFrames: callback => { frames.add(callback); return () => frames.delete(callback); },
      dispose: () => {
        if (readiness === "disposed") return;
        record("release"); readiness = "disposed"; event("viewer_disposed"); publish(); adapter.release();
      },
      retry: vi.fn(), setMuted: vi.fn(), setVolume: vi.fn(),
    } as playbackModule.PlaybackController;
  });
  vi.mocked(runBenchmarkDesktopResize).mockImplementation(async () => { record("native-resize"); });
  vi.spyOn(renderBenchmark, "waitForBenchmarkRenderedReplay").mockImplementation(async () => { record("render-ready"); });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game} ddragonAssetBaseUrl="http://assets.test/ddragon/16.20.1"
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}}
    onBenchmarkViewerCycleCompleted={() => record("cycle-callback")} />, host);
  try {
    for (let step = 0; step < 3000 && !done; step++) {
      for (let micro = 0; micro < 30; micro++) await Promise.resolve();
      if (!done) await vi.advanceTimersToNextTimerAsync();
    }
    for (let micro = 0; micro < 30; micro++) await Promise.resolve();
    expect(done).toBe(true);
    expect(trace.some(value => value.startsWith("event:scenario_failed"))).toBe(false);
    expect(trace).toMatchSnapshot();
  } finally { dispose(); vi.useRealTimers(); }
});
