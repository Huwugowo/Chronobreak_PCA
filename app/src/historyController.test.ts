import { afterEach, expect, it, vi } from "vitest";
import { HistoryController } from "./historyController";
import type { LibraryOrigin } from "./libraryController";
import type { CatalogPage, HistoryPage } from "./leagueIndex";
import type { GameSummary } from "./types";
const origin = (root: string, rootEpoch: number): LibraryOrigin => ({ root, rootEpoch, request: rootEpoch, token: `scan-${rootEpoch}`, navigation: rootEpoch });
const game = { timestamp: "100", media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311" } as GameSummary;
const page = (): HistoryPage => ({ rows: [], next: null, indexed_recordings: 0, catalog_recordings: 1, unknown_recordings: 1 });
const catalog = (token: string): CatalogPage => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 });
const deferred = <T>() => { let resolve!: (value: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; };
const flush = async () => { for (let n = 0; n < 8; n++) await Promise.resolve(); };
afterEach(() => vi.useRealTimers());

it("yields between catalog pages and pauses when navigation leaves Games", async () => {
  vi.useFakeTimers(); const first = deferred<CatalogPage>(); const o = origin("A", 1);
  const api = { current: () => true, catalog: vi.fn(() => first.promise), recording: vi.fn(), history: vi.fn(async () => page()) };
  const controller = new HistoryController(api); controller.setContext(o, [game]);
  expect(api.catalog).not.toHaveBeenCalled(); await vi.advanceTimersByTimeAsync(100);
  expect(api.catalog).toHaveBeenCalledTimes(1);
  controller.setContext(null); first.resolve({ ...catalog(o.token), next: "100" }); await flush();
  await vi.advanceTimersByTimeAsync(1000);
  expect(api.catalog).toHaveBeenCalledTimes(1); expect(api.history).not.toHaveBeenCalled(); controller.close();
});

it("drops A/B/A results and keeps one latest history query", async () => {
  vi.useFakeTimers(); const a1 = origin("A", 1), a2 = origin("A", 3);
  const held = deferred<HistoryPage>();
  const api = { current: () => true, catalog: vi.fn(async (token: string) => catalog(token)), recording: vi.fn(), history: vi.fn().mockReturnValueOnce(held.promise).mockResolvedValue(page()) };
  const controller = new HistoryController(api); controller.setContext(a1, [game]); await vi.advanceTimersByTimeAsync(100);
  expect(api.history).toHaveBeenCalledTimes(1);
  controller.setContext(origin("B", 2), [game]); controller.setContext(a2, [game]); await vi.advanceTimersByTimeAsync(100);
  controller.setQuery({ limit: 50, champion: "Ahri" }); controller.setQuery({ limit: 50, champion: "Lux" });
  expect(api.history).toHaveBeenCalledTimes(1);
  held.resolve({ ...page(), unknown_recordings: 0 }); await flush();
  expect(api.history).toHaveBeenCalledTimes(2); expect(api.history).toHaveBeenLastCalledWith(a2.token, expect.objectContaining({ champion: "Lux" }));
  expect(controller.value.page?.unknown_recordings).toBe(1); controller.close();
});

it("rejects cached media outside the accepted snapshot without hiding core cards", async () => {
  vi.useFakeTimers(); const o = origin("A", 1);
  const controller = new HistoryController({ current: () => true, catalog: async token => catalog(token), recording: vi.fn(), history: async () => ({ ...page(), rows: [{ media_id: game.media_id!, game: { ...game, timestamp: "unaccepted" }, stats: {} as never, index_status: "ready" }] }) });
  controller.setContext(o, [game]); await vi.advanceTimersByTimeAsync(100); await flush();
  expect(controller.value.page).toBeNull(); expect(controller.value.error).toContain("changed since the scan"); controller.close();
});
