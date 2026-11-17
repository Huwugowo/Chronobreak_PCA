// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import App from "./App";
import { loadDdragonStatus, loadStorageUsage, refreshLibrary, setGameSaved } from "./api";
import { indexCatalog, indexRecording, queryHistory, resetLeagueIndex, type HistoryPage } from "./leagueIndex";
import { reconcileClips } from "./managedClips";
import { onLibraryCatalogChanged, onPreparedData } from "./preparedData";
import type { GameSummary, LibrarySnapshot } from "./types";

const harness = vi.hoisted(() => ({ root: "A", destination: "B", managed: false }));
vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(), initializeReplayBenchmark: async () => null }));
vi.mock("./preparedData", () => ({ onLibraryCatalogChanged: vi.fn(() => () => {}), onPreparedData: vi.fn(() => () => {}) }));
vi.mock("./managedClips", async original => ({ ...await original<typeof import("./managedClips")>(),
  usesManagedClips: () => harness.managed, reconcileClips: vi.fn(async () => true),
}));
vi.mock("./api", async original => ({ ...await original<typeof import("./api")>(),
  loadStorageUsage: vi.fn(), refreshLibrary: vi.fn(), setGameSaved: vi.fn(),
  loadSettings: async () => ({ output_path: harness.root, auto_delete_days: 0 }),
  loadDdragonStatus: vi.fn(() => new Promise(() => {})), ensureHevcCapability: () => new Promise(() => {}),
  chooseOutputFolder: async () => harness.destination,
  persistSettings: async (value: { output_path: string }) => { harness.root = value.output_path; return value; },
}));
vi.mock("./leagueIndex", async original => ({ ...await original<typeof import("./leagueIndex")>(),
  indexCatalog: vi.fn(async (token: string) => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 })),
  indexRecording: vi.fn(),
  queryHistory: vi.fn(async () => ({ rows: [{ media_id: game.media_id, game, index_status: "ready",
    stats: { source: "unavailable", kills: null, deaths: null, assists: null, items: null, keystone_id: null, secondary_style: null, summoner_spells: null, creep_score: null, duration_seconds: null, evidence: null, final_evidence: null, game_tick: null } }],
    next: null, indexed_recordings: 1, catalog_recordings: 1, unknown_recordings: 1 })),
  resetLeagueIndex: vi.fn(),
}));
vi.mock("./components/ViewerScreen", () => ({ default: (props: { onBack: () => void }) =>
  <button data-testid="replay-back" onClick={props.onBack}>Back to Match History</button> }));
vi.mock("./components/AppHeader", () => ({ default: (props: { onSettings: () => void; onTab: (tab: "games") => void }) => <>
  <button data-testid="settings" onClick={props.onSettings}>Settings</button>
  <button data-testid="games" onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/SettingsScreen", () => ({ default: (props: {
  onBack: () => void; onChooseFolder: () => void; onRebuildLeagueData: () => void; saving: boolean;
  onRecalculateStorage: () => void; canRecalculateStorage: boolean;
}) => <>
  <button data-testid="back" onClick={props.onBack}>Back</button>
  <button data-testid="root" onClick={props.onChooseFolder}>Root</button>
  <button data-testid="rebuild" disabled={props.saving} onClick={props.onRebuildLeagueData}>Rebuild</button>
  <button data-testid="storage" disabled={!props.canRecalculateStorage} onClick={props.onRecalculateStorage}>Recalculate storage</button>
</> }));
const game: GameSummary = { media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311" as GameSummary["media_id"], timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-23T00:00:00Z",
  duration_ms: 10000, kills: 1, deaths: 2, assists: 3, items: [], participants: [], summoner_spells: [],
  keystone_id: null, saved: false, incomplete: false, video_size_bytes: 1, video_available: true, league_result: null, match_group: "a".repeat(64) };
const snapshot = (token: string, saved = false): LibrarySnapshot => ({ token,
  games: [{ ...game, saved }], clips: [], usage: { games_bytes: 1, clips_bytes: 0, game_count: 1, clip_count: 0 } });
const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
let dispose: (() => void) | undefined;
beforeEach(() => { vi.mocked(refreshLibrary).mockReset().mockResolvedValueOnce(snapshot("a")); vi.mocked(setGameSaved).mockReset(); vi.mocked(resetLeagueIndex).mockReset(); vi.mocked(loadStorageUsage).mockReset(); });
afterEach(() => { dispose?.(); document.body.replaceChildren(); localStorage.clear(); vi.clearAllMocks(); harness.root = "A"; harness.managed = false; });
const mount = async () => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.querySelector('[aria-label="Save Ahri recording"]')).not.toBeNull());
  return { host, star: () => host.querySelector<HTMLButtonElement>('[aria-label$="save Ahri recording" i]')!,
    click: (id: string) => host.querySelector<HTMLButtonElement>('[data-testid="'+id+'"]')!.click() };
};
it.each([false, true])("keeps an immediate Star through its save refresh and then uses canonical saved=%s", async canonical => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.star().click();
  expect(h.star().getAttribute("data-saved")).toBe("true"); expect(h.star().getAttribute("aria-disabled")).toBe("true");
  h.star().click(); expect(setGameSaved).toHaveBeenCalledTimes(1);
  expect(setGameSaved).toHaveBeenCalledWith("1", true, "a");
  save.resolve(); await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  expect(h.star().getAttribute("data-saved")).toBe("true"); expect(h.star().getAttribute("aria-disabled")).toBe("true");
  scan.resolve(snapshot("b", canonical)); await vi.waitFor(() => expect(h.star().getAttribute("aria-disabled")).toBe("false"));
  expect(h.star().getAttribute("data-saved")).toBe(String(canonical));
});
it("keeps the indexed match card and star mounted through saving and refreshing", async () => {
  const save = deferred<void>();
  const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise);
  vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount();
  await vi.waitFor(() => expect(queryHistory).toHaveBeenCalled());
  const article = h.host.querySelector("article");
  const spells = [...article!.querySelectorAll('[title="Summoner spell unavailable"]')];
  expect(spells).toHaveLength(2);
  const star = h.star();
  const open = article!.querySelector<HTMLButtonElement>('[aria-label^="Open Ahri recording"]')!;
  star.focus();
  const expectSameCard = () => {
    expect(h.host.querySelector("article")).toBe(article);
    expect(h.star()).toBe(star);
    expect(document.activeElement).toBe(star);
    expect(star.disabled).toBe(false);
    expect(open.disabled).toBe(false);
    expect(spells.every(spell => spell.isConnected)).toBe(true);
    expect(star.getAttribute("data-saved")).toBe("true");
  };
  star.click();
  expectSameCard();
  save.resolve();
  await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  expect(open.getAttribute("aria-disabled")).toBe("true");
  expectSameCard();
  scan.resolve(snapshot("b", true));
  await vi.waitFor(() => expect(queryHistory).toHaveBeenLastCalledWith("b", expect.anything()));
  expectSameCard();
});
it("keeps populated History cards, filters and images through a favorite and replay round trips", async () => {
  harness.managed = true;
  const scan = deferred<LibrarySnapshot>();
  const save = deferred<void>();
  const historyPage = (saved: boolean): HistoryPage => ({
    rows: [{ media_id: game.media_id!, game: { ...game, saved }, index_status: "ready", clip_count: 1, stats: {
      source: "final_eog", kills: 6, deaths: 2, assists: 9, items: [3085],
      keystone_id: 9923, secondary_style: null, summoner_spells: ["4", "6"], creep_score: 136,
      duration_seconds: 985, evidence: null, final_evidence: null, game_tick: null,
    } }], next: null, indexed_recordings: 1, catalog_recordings: 1, unknown_recordings: 0,
  });
  vi.mocked(loadDdragonStatus).mockResolvedValueOnce({ state: "ready", version: "test", asset_base_url: "http://fixture/ddragon",
    item_count: 1, champion_count: 1, cache_directory: "", error: null });
  const readyCatalog = async (token: string) => ({ snapshot_token: token, next: null, catalog_recordings: 1,
    items: [{ game_timestamp: game.timestamp, media_id: game.media_id!, error: null,
      progress: { media_id: game.media_id!, status: "ready", history_ready: true, batches: 0, position: null, tail_bytes: 0 } }],
  });
  vi.mocked(indexCatalog).mockImplementationOnce(readyCatalog).mockImplementationOnce(readyCatalog);
  vi.mocked(queryHistory).mockResolvedValueOnce(historyPage(false))
    .mockResolvedValueOnce(historyPage(true)).mockResolvedValueOnce(historyPage(true));
  vi.mocked(refreshLibrary).mockReset().mockResolvedValueOnce({ ...snapshot("a"), managed_clip_count: 1 })
    .mockReturnValueOnce(scan.promise);
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise);
  const { host, star, click } = await mount();
  await vi.waitFor(() => expect(host.querySelector('img[src$="/item/3085"]')).not.toBeNull());
  await vi.waitFor(() => expect(reconcileClips).toHaveBeenCalledOnce());
  const history = host.querySelector('[aria-label="Match History"]')!;
  const article = history.querySelector("article")!;
  const images = Array.from(article.querySelectorAll("img"));
  const sources = images.map(img => img.getAttribute("src"));
  const stats = [article.querySelector('[aria-label="Final creep score"]')!,
    Array.from(article.querySelectorAll("em")).find(element => element.textContent?.includes("KDA"))!.parentElement!];
  const favorite = star();
  const open = article.querySelector<HTMLButtonElement>('[aria-label^="Open Ahri recording"]')!;
  const favorites = Array.from(history.querySelectorAll("button")).find(button => button.textContent?.trim() === "Favorites")!;
  const mutations: MutationRecord[] = [];
  const observer = new MutationObserver(records => mutations.push(...records));
  observer.observe(history, { childList: true, subtree: true, attributes: true,
    attributeFilter: ["src"], attributeOldValue: true, characterData: true, characterDataOldValue: true });
  const expectPopulated = () => {
    expect(host.querySelector('[aria-label="Match History"]')).toBe(history);
    expect(history.querySelector("article")).toBe(article);
    expect(star()).toBe(favorite);
    expect(article.textContent).toContain("7.50:1 KDA");
    expect(article.textContent).toContain("CS 136");
    expect(images.length).toBeGreaterThan(3);
    Array.from(article.querySelectorAll("img")).forEach((img, i) => expect(img).toBe(images[i]));
    expect(images.map(img => img.getAttribute("src"))).toEqual(sources);
    expect(images.every(img => img.isConnected)).toBe(true);
  };
  try {
    favorite.click(); expectPopulated();
    expect(setGameSaved).toHaveBeenCalledExactlyOnceWith("1", true, "a");
    save.resolve();
    await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
    expectPopulated();
    scan.resolve({ ...snapshot("b", true), managed_clip_count: 1 });
    await vi.waitFor(() => expect(queryHistory).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(reconcileClips).toHaveBeenCalledTimes(2));
    expectPopulated();
    favorites.click();
    await vi.waitFor(() => expect(queryHistory).toHaveBeenCalledTimes(3));
    expect(queryHistory).toHaveBeenLastCalledWith("b", expect.objectContaining({ saved_only: true }));
    expectPopulated();

    for (let round = 0; round < 2; round++) {
      open.click();
      expect(host.querySelector('[data-testid="replay-back"]')).not.toBeNull();
      expect(history.closest("[hidden]")).not.toBeNull();
      expectPopulated();
      click("replay-back");
      expect(history.closest("[hidden]")).toBeNull();
      expect(favorites.getAttribute("aria-pressed")).toBe("true");
      expectPopulated();
    }
    await new Promise(resolve => setTimeout(resolve, 60));
    expect(refreshLibrary).toHaveBeenCalledTimes(2);
    expect(indexCatalog).toHaveBeenCalledTimes(2);
    expect(reconcileClips).toHaveBeenCalledTimes(2);
    expect(indexRecording).not.toHaveBeenCalled();
    expect(queryHistory).toHaveBeenCalledTimes(3);
    const tracked = [article, favorite, ...images, ...stats];
    expect(mutations.flatMap(record => Array.from(record.removedNodes))
      .some(removed => tracked.some(node => removed.contains(node)))).toBe(false);
    expect(mutations.filter(record => record.type === "attributes").every(record =>
      record.oldValue === (record.target as Element).getAttribute("src"))).toBe(true);
    expect(mutations.filter(record => stats.some(stat => stat.contains(record.target))).some(record =>
      record.oldValue?.includes("—") || Array.from(record.removedNodes).some(node => node.textContent?.includes("—")))).toBe(false);
  } finally { observer.disconnect(); }
});
it("refreshes only the affected data when library notifications arrive during replay", async () => {
  const { host, click } = await mount();
  await vi.waitFor(() => expect(queryHistory).toHaveBeenCalledOnce());
  const catalogChanged = vi.mocked(onLibraryCatalogChanged).mock.calls[0][0];
  const prepared = vi.mocked(onPreparedData).mock.calls[0][0];
  host.querySelector<HTMLButtonElement>('[aria-label^="Open Ahri recording"]')!.click();
  prepared(["unrelated-media"]);
  expect(queryHistory).toHaveBeenCalledOnce();
  prepared([game.media_id!]);
  expect(queryHistory).toHaveBeenCalledTimes(2);
  expect(refreshLibrary).toHaveBeenCalledOnce();
  catalogChanged("old-token");
  expect(refreshLibrary).toHaveBeenCalledOnce();
  vi.mocked(refreshLibrary).mockResolvedValueOnce({ ...snapshot("b"), games: [] });
  vi.mocked(queryHistory).mockResolvedValueOnce({ rows: [], next: null, indexed_recordings: 0,
    catalog_recordings: 0, unknown_recordings: 0 });
  catalogChanged("a");
  expect(refreshLibrary).toHaveBeenCalledOnce();
  expect(host.querySelector('[data-testid="replay-back"]')).not.toBeNull();
  click("replay-back");
  await vi.waitFor(() => expect(queryHistory).toHaveBeenLastCalledWith("b", expect.anything()));
  expect(refreshLibrary).toHaveBeenCalledTimes(2);
  expect(host.querySelector("article")).toBeNull();
});
it("rolls back a failed save while retaining cards through the compulsory refresh", async () => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.star().click(); save.reject(new Error("fixture save failure"));
  await vi.waitFor(() => expect(h.host.textContent).toContain("fixture save failure"));
  expect(h.star().getAttribute("data-saved")).toBe("false"); expect(refreshLibrary).toHaveBeenCalledTimes(2);
  scan.resolve(snapshot("b")); await vi.waitFor(() => expect(h.star().getAttribute("aria-disabled")).toBe("false"));
});
it("suppresses stale save notices and stars after navigation while still reconciling the filesystem", async () => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValue(scan.promise);
  const h = await mount(); h.star().click(); h.click("settings");
  save.resolve(); await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  expect(h.host.textContent).not.toContain("Recording saved.");
  scan.resolve(snapshot("b", false)); await Promise.resolve(); h.click("back");
  await vi.waitFor(() => expect(h.star()?.getAttribute("aria-disabled")).toBe("false"));
  expect(h.star().getAttribute("data-saved")).toBe("false"); expect(h.host.textContent).not.toContain("Recording saved.");
});
it("rebuilds searchable data through the admitted settings mutation and refreshes its snapshot", async () => {
  const reset = deferred<{ catalog_recordings: number }>();
  vi.mocked(resetLeagueIndex).mockReturnValueOnce(reset.promise);
  vi.mocked(refreshLibrary).mockResolvedValue(snapshot("b"));
  const h = await mount(); h.click("settings"); h.click("rebuild");
  expect(resetLeagueIndex).toHaveBeenCalledWith("a");
  expect(h.host.querySelector<HTMLButtonElement>('[data-testid="rebuild"]')!.disabled).toBe(true);
  reset.resolve({ catalog_recordings: 1 });
  await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  await vi.waitFor(() => expect(h.host.textContent).toContain("Searchable League data reset."));
});
it("updates the storage totals only after the Settings action without refreshing the library", async () => {
  const scan = deferred<LibrarySnapshot["usage"]>();
  vi.mocked(loadStorageUsage).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.click("settings");
  expect(loadStorageUsage).not.toHaveBeenCalled();
  h.click("storage"); h.click("storage");
  expect(loadStorageUsage).toHaveBeenCalledExactlyOnceWith("a");
  scan.resolve({ games_bytes: 512, clips_bytes: 0, game_count: 2, clip_count: 0 });
  await vi.waitFor(() => expect(h.host.textContent).toContain("Storage totals updated."));
  expect(h.host.textContent).toContain("Recordings 512 B");
  expect(refreshLibrary).toHaveBeenCalledTimes(1);
});
it("silently reconciles a rebuild that finishes after navigation", async () => {
  const reset = deferred<{ catalog_recordings: number }>();
  const scan = deferred<LibrarySnapshot>();
  vi.mocked(resetLeagueIndex).mockReturnValueOnce(reset.promise);
  vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.click("settings"); h.click("rebuild"); h.click("games");
  reset.resolve({ catalog_recordings: 1 });
  await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  scan.resolve(snapshot("b"));
  await vi.waitFor(() => expect(h.star()?.getAttribute("aria-disabled")).toBe("false"));
  expect(h.host.textContent).not.toContain("Searchable League data reset.");
  expect(h.host.textContent).not.toContain("Rebuild request became stale.");
});
