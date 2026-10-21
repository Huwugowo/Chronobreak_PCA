// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import LibraryScreen from "./LibraryScreen";
import type { DdragonStatus, GameSummary } from "../types";
import type { HistoryState } from "../historyController";

afterEach(() => { document.body.replaceChildren(); vi.useRealTimers(); });

it("renders Games from the core snapshot when Data Dragon enrichment fails", () => {
  const game: GameSummary = {
    media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311" as GameSummary["media_id"],
    timestamp: "100",
    champion: "Ahri",
    game_mode: "CLASSIC",
    duration_ms: 120_000,
    recorded_at: "2026-01-01T00:00:00Z",
    kills: 2,
    deaths: 1,
    assists: 3,
    summoner_spells: [],
    keystone_id: null,
    items: [], participants: [{ summoner_name: "Ally", champion: "Lux", relation: "ally" },
      { summoner_name: "Enemy", champion: "Garen", relation: "enemy" },
      { summoner_name: "Unknown team", champion: "Unknown", relation: "neutral" }],
    saved: false,
    incomplete: false,
    video_size_bytes: 1,
    video_available: true,
    league_result: { outcome: "unknown", ended_early: true, partial: true },
  };
  const ddragon: DdragonStatus = {
    state: "offline",
    version: null,
    asset_base_url: null,
    item_count: 0,
    champion_count: 0,
    cache_directory: "",
    error: "Data Dragon unavailable",
  };
  const host = document.createElement("div");
  document.body.append(host);
  const origin = { root: "A", rootEpoch: 1, request: 1, token: "token-a", navigation: 1 };
  const [actionable, setActionable] = createSignal(true);
  const [assets, setAssets] = createSignal(ddragon);
  const [games, setGames] = createSignal([game]);
  const [history,setHistory]=createSignal<HistoryState>({page:null,loading:false,indexing:false,error:null,indexError:null,laterPage:false});
  const opened = vi.fn(), saved = vi.fn(), deleted = vi.fn();
  const dispose = render(() => <LibraryScreen
    tab="games"
    games={games()}
    history={history()}
    clips={[]}
    ddragon={assets()}
    actionable={actionable()}
    busyId={null}
    snapshotOrigin={origin}
    onOpenGame={opened}
    onToggleSaved={saved}
    onDeleteGame={deleted}
    onOpenClip={() => {}}
    onDeleteClip={() => {}}
    onOpenClipsFolder={() => {}}
  />, host);
  expect(host.textContent).toContain("Ahri");
  expect(host.textContent).toContain("Unknown");
  expect(host.querySelector('[aria-label="Recorded matches"]')?.textContent).not.toContain("Defeat");
  expect(host.textContent).not.toContain("Remake");
  expect(host.querySelector('[aria-label="Match History"]')).not.toBeNull();
  expect(host.textContent).not.toContain("PERFECT KDA");
  const stats={source:"final_eog" as const,kills:6,deaths:null,assists:9,items:[0,null],keystone_id:9923,secondary_style:8000,summoner_spells:["4","6"],creep_score:136,duration_seconds:985,evidence:null,final_evidence:null,game_tick:null};
  setHistory({...history(),page:{rows:[{media_id:game.media_id!,game,stats,index_status:"ready"}],next:null,indexed_recordings:1,catalog_recordings:1,unknown_recordings:0}});
  expect(host.textContent).not.toContain("Final EOG"); expect(host.textContent).not.toContain("PERFECT KDA");
  expect(host.querySelector('[aria-label="Final creep score"]')?.textContent).toBe("CS 1368.3 / min");
  setAssets({...ddragon,state:"ready",asset_base_url:"http://assets.test/ddragon/16.20.1"});
  expect([...host.querySelectorAll("[class*=spells] img")].map(img=>img.getAttribute("src"))).toEqual(["http://assets.test/ddragon/16.20.1/spell/4","http://assets.test/ddragon/16.20.1/spell/6"]);
  const primary = host.querySelector('[title="Keystone rune"] img')!;
  const secondary = host.querySelector('[title="Secondary rune tree"] img')!;
  expect(primary.getAttribute("src")).toContain("/rune/9923");
  expect(secondary.getAttribute("src")).toContain("/rune/8000");
  expect(primary.parentElement!.parentElement).toBe(secondary.parentElement!.parentElement);
  expect(primary.parentElement!.nextElementSibling).toBe(secondary.parentElement);
  secondary.dispatchEvent(new Event("error"));
  expect((secondary as HTMLImageElement).hidden).toBe(true);
  expect(secondary.parentElement!.isConnected).toBe(true);
  setHistory({...history(),page:{...history().page!,rows:[{media_id:game.media_id!,game,stats:{...stats,deaths:0},index_status:"ready"}]}});
  expect(host.textContent).toContain("PERFECT KDA");
  const roster = host.querySelector('[aria-label="Team rosters"]')!;
  expect(roster.querySelector('[data-team="ally"]')?.textContent).toContain("Ally");
  expect(roster.querySelector('[data-team="enemy"]')?.textContent).toContain("Enemy");
  expect(roster.textContent).not.toContain("Unknown team");
  // The core snapshot is deliberately empty after the index migration. The
  // checked history row must remain the roster's source, independent of it.
  const indexedGame = {...game,participants:[{summoner_name:"Indexed ally",champion:"Lux",relation:"ally" as const},{summoner_name:"Indexed enemy",champion:"Garen",relation:"enemy" as const}]};
  setHistory({...history(),page:{...history().page!,rows:[{media_id:game.media_id!,game:indexedGame,stats:{...stats,creep_score:0},index_status:"ready"}]}});
  expect(roster.textContent).toContain("Indexed ally");
  expect(roster.textContent).toContain("Indexed enemy");
  expect(host.querySelector('[aria-label="Final creep score"]')?.textContent).toBe("CS 00.0 / min");
  for (const missing of [{creep_score:null}, {duration_seconds:null}, {duration_seconds:0}]) {
    setHistory({...history(),page:{...history().page!,rows:[{media_id:game.media_id!,game:indexedGame,stats:{...stats,...missing},index_status:"ready"}]}});
    expect(host.querySelector('[aria-label="Final creep score"]')?.textContent).toContain("— / min");
  }
  setHistory({...history(),page:{...history().page!,rows:[{media_id:game.media_id!,game:indexedGame,stats:{...stats,source:"recorded_snapshot"},index_status:"ready"}]}});
  expect(host.querySelector('[aria-label="Final creep score"]')?.textContent).toBe("CS —— / min");
  const open = host.querySelector<HTMLButtonElement>('[aria-label^="Open Ahri"]')!;
  const save = host.querySelector<HTMLButtonElement>('[aria-label="Save Ahri recording"]')!;
  const remove = [...host.querySelectorAll("button")].find(button => button.textContent?.includes("Delete recording"))!;
  open.click(); save.click(); remove.click();
  expect(opened).toHaveBeenCalledWith("100", origin);
  expect(saved).toHaveBeenCalledWith(game, origin);
  expect(deleted).toHaveBeenCalledWith(game, origin);
  setActionable(false);
  for (const button of [open, save, remove]) { expect(button.disabled).toBe(true); button.click(); }
  expect(opened).toHaveBeenCalledTimes(1);
  expect(saved).toHaveBeenCalledTimes(1);
  expect(deleted).toHaveBeenCalledTimes(1);
  // A full SQL page must not displace filesystem entries that cannot be indexed.
  const complete = Array.from({length:50}, (_,i)=>({...game,timestamp:String(1000+i),media_id:`9c82b929-7c85-4c27-a6d0-${String(i).padStart(12,"0")}` as GameSummary["media_id"]}));
  setGames([...complete,{...game,timestamp:"incomplete",incomplete:true}]);
  setHistory({...history(),laterPage:false,page:{...history().page!,rows:complete.map(game=>({game,media_id:game.media_id!,stats,index_status:"ready"})),next:{recorded_at:game.recorded_at,media_id:complete[49].media_id!}}});
  expect(host.querySelectorAll("article")).toHaveLength(51);
  expect(host.textContent).toContain("INCOMPLETE");
  setHistory({...history(),laterPage:true});
  expect(host.querySelectorAll("article")).toHaveLength(50);
  dispose();
});

it("coalesces typing, applies filters and clearing immediately, and cancels pending text on disposal", async () => {
  vi.useFakeTimers();
  const host = document.createElement("div"); document.body.append(host);
  const onQuery = vi.fn();
  const [origin, setOrigin] = createSignal({ root: "A", rootEpoch: 1, request: 1, token: "a", navigation: 1 });
  const dispose = render(() => <LibraryScreen tab="games" games={[]} clips={[]}
    ddragon={{ state: "offline", version: null, asset_base_url: null, item_count: 0, champion_count: 0, cache_directory: "", error: null }}
    actionable snapshotOrigin={{ ...origin() }} busyId={null} onHistoryQuery={onQuery}
    onOpenGame={() => {}} onToggleSaved={() => {}} onDeleteGame={() => {}}
    onOpenClip={() => {}} onDeleteClip={() => {}} onOpenClipsFolder={() => {}} />, host);
  const search = host.querySelector<HTMLInputElement>('[aria-label="Search matches"]')!;
  const type = (text: string) => { search.value = text; search.dispatchEvent(new Event("input", { bubbles: true })); };
  onQuery.mockClear();
  for (const text of ["M", "Master", "Master Yi"]) { type(text); await vi.advanceTimersByTimeAsync(60); }
  expect(onQuery).not.toHaveBeenCalled();
  expect(host.textContent).toContain("Updating matches.");
  await vi.advanceTimersByTimeAsync(120);
  expect(onQuery).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ search: "Master Yi" }));
  onQuery.mockClear(); type("Ahri");
  const event = host.querySelector<HTMLSelectElement>('[aria-label="Filter by event"]')!;
  event.value = "ChampionKill"; event.dispatchEvent(new Event("change", { bubbles: true }));
  expect(onQuery).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ search: "Ahri", event_kind: "ChampionKill" }));
  await vi.advanceTimersByTimeAsync(500);
  expect(onQuery).toHaveBeenCalledTimes(1);
  onQuery.mockClear(); type("");
  expect(onQuery).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ search: null }));
  onQuery.mockClear(); type("new context");
  setOrigin({ root: "B", rootEpoch: 2, request: 2, token: "b", navigation: 2 });
  expect(onQuery).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ search: "new context" }));
  await vi.advanceTimersByTimeAsync(500);
  expect(onQuery).toHaveBeenCalledTimes(1);
  type("unmounted"); dispose(); onQuery.mockClear();
  await vi.advanceTimersByTimeAsync(500);
  expect(onQuery).not.toHaveBeenCalled();
});

it("keeps checked cards and their stats visible while a semantic query updates", () => {
  const host = document.createElement("div"); document.body.append(host);
  const game: GameSummary = { timestamp: "100", media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311" as GameSummary["media_id"],
    champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-01-01", duration_ms: 120_000,
    kills: 2, deaths: 1, assists: 3, summoner_spells: [], keystone_id: null, items: [], participants: [],
    saved: false, incomplete: false, video_size_bytes: 1, video_available: true, league_result: null };
  const [history, setHistory] = createSignal<HistoryState>({ loading: false, indexing: false, error: null, indexError: null, laterPage: false,
    page: { rows: [{ media_id: game.media_id!, game, index_status: "ready", stats: { source: "final_eog", kills: 9, deaths: 1, assists: 2,
      items: null, summoner_spells: null, keystone_id: null, secondary_style: null, creep_score: null, duration_seconds: null,
      evidence: null, final_evidence: null, game_tick: null } }], next: { recorded_at: game.recorded_at, media_id: game.media_id! },
      indexed_recordings: 1, catalog_recordings: 1, unknown_recordings: 0 } });
  const dispose = render(() => <LibraryScreen tab="games" games={[game]} clips={[]} history={history()}
    ddragon={{ state: "offline", version: null, asset_base_url: null, item_count: 0, champion_count: 0, cache_directory: "", error: null }}
    actionable snapshotOrigin={null} busyId={null} onHistoryQuery={() => setHistory(state => ({ ...state, loading: true }))}
    onOpenGame={() => {}} onToggleSaved={() => {}} onDeleteGame={() => {}}
    onOpenClip={() => {}} onDeleteClip={() => {}} onOpenClipsFolder={() => {}} />, host);
  const card = host.querySelector("article");
  const filter = host.querySelector<HTMLSelectElement>('[aria-label="Filter by event"]')!;
  filter.value = "ChampionKill"; filter.dispatchEvent(new Event("change", { bubbles: true }));
  expect(host.querySelector("article")).toBe(card);
  expect(card?.textContent).toContain("11.00:1 KDA");
  expect(host.querySelector('[aria-label="Recorded matches"]')?.getAttribute("aria-busy")).toBe("true");
  expect([...host.querySelectorAll("button")].find(button => button.textContent === "Next 50 recordings")?.disabled).toBe(true);
  setHistory(state => ({ ...state, loading: false, error: "Search failed" }));
  expect(host.querySelector("article")).toBe(card);
  expect([...host.querySelectorAll("button")].find(button => button.textContent === "Next 50 recordings")?.disabled).toBe(true);
  setHistory(state => ({ ...state, loading: false, page: { ...state.page!, rows: [], next: null } }));
  expect(host.querySelector("article")).toBeNull();
  expect(host.textContent).not.toContain("Updating matches.");
  dispose();
});
