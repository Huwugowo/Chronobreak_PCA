import { afterEach, expect, it, vi } from "vitest";
import { HistoryController, type HistoryQueryObservation } from "./historyController";
import type { LibraryOrigin } from "./libraryController";
import type { CatalogPage, HistoryPage, ImportProgress } from "./leagueIndex";
import type { GameSummary } from "./types";
const origin = (root: string, rootEpoch: number): LibraryOrigin => ({ root, rootEpoch, request: rootEpoch, token: `scan-${rootEpoch}`, navigation: rootEpoch });
const game = { timestamp: "100", media_id: "9c82b929-7c85-4c27-a6d0-4f4e1d8e3311" } as GameSummary;
const page = (): HistoryPage => ({ rows: [], next: null, indexed_recordings: 0, catalog_recordings: 1, unknown_recordings: 1 });
const catalog = (token: string): CatalogPage => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 });
const deferred = <T>() => { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((r, fail) => { resolve = r; reject = fail; }); return { promise, resolve, reject }; };
const flush = async () => { for (let n = 0; n < 8; n++) await Promise.resolve(); };
afterEach(() => vi.useRealTimers());

it.each([false, true])("settles positive tail coverage without retries (catalog ready: %s)", async catalogReady => {
  vi.useFakeTimers();
  const progress: ImportProgress = { media_id: game.media_id!, status: "ready", batches: 0,
    position: { end_offset: 100, last_sequence: 7, checksum: Array(32).fill(1) }, tail_bytes: 512 };
  const api = { current: () => true,
    catalog: vi.fn(async (token: string): Promise<CatalogPage> => ({ ...catalog(token), items: [{
      game_timestamp: game.timestamp, media_id: game.media_id!, error: null,
      progress: catalogReady ? progress : { ...progress, status: "pending", batches: 4, tail_bytes: 0 },
    }] })),
    recording: vi.fn(async () => progress), history: vi.fn(async () => page()) };
  const controller = new HistoryController(api);
  try {
    controller.setContext(origin("A", 1), [game]);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(api.catalog).toHaveBeenCalledTimes(1);
    expect(api.recording).toHaveBeenCalledTimes(catalogReady ? 0 : 1);
    expect(controller.value).toMatchObject({ indexing: false, loading: false, indexError: null });
    expect(vi.getTimerCount()).toBe(0);
    // New accepted source observations remain the wake mechanism.
    controller.setContext(origin("A", 2), [game]);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(api.catalog).toHaveBeenCalledTimes(2);
    expect(api.recording).toHaveBeenCalledTimes(catalogReady ? 0 : 2);
  } finally { controller.close(); }
});

async function overlappingImports() {
  vi.useFakeTimers();
  const o = origin("A", 1), events: HistoryQueryObservation[] = [];
  const pending = { media_id: game.media_id!, status: "pending", batches: 4 } as ImportProgress;
  const imports: ReturnType<typeof deferred<ImportProgress>>[] = [];
  const queries: ReturnType<typeof deferred<HistoryPage>>[] = [];
  let activeImports = 0, activeQueries = 0, maximumImports = 0, maximumQueries = 0;
  const api = {
    current: () => true,
    catalog: vi.fn(async (token: string) => ({ ...catalog(token), items: [{ game_timestamp: game.timestamp, media_id: game.media_id!, progress: pending }] } as CatalogPage)),
    recording: vi.fn(() => {
      maximumImports = Math.max(maximumImports, ++activeImports);
      const result = deferred<ImportProgress>(); imports.push(result);
      return result.promise.finally(() => activeImports--);
    }),
    history: vi.fn((_token: string, _query: unknown): Promise<HistoryPage> => {
      maximumQueries = Math.max(maximumQueries, ++activeQueries);
      if (api.history.mock.calls.length === 1) return Promise.resolve(page()).finally(() => activeQueries--);
      const result = deferred<HistoryPage>(); queries.push(result);
      return result.promise.finally(() => activeQueries--);
    }),
    observeQuery: (event: HistoryQueryObservation) => events.push(event),
  };
  const controller = new HistoryController(api);
  controller.setContext(o, [game]);
  await vi.advanceTimersByTimeAsync(8);
  controller.setQuery({ limit: 50, search: "foreground" }, "foreground");
  return { controller, api, events, pending, imports, queries,
    bounds: () => ({ maximumImports, maximumQueries }),
    finish: async () => {
      controller.close();
      for (const result of imports) result.resolve(pending);
      for (const result of queries) result.resolve(page());
      await flush();
      expect(vi.getTimerCount()).toBe(0);
    },
  };
}

it("admits the held foreground query despite repeated imports and coalesces later refreshes", async () => {
  const fixture = await overlappingImports();
  const { controller, api, imports, queries, pending, events } = fixture;
  try {
    for (let n = 0; n < 3; n++) {
      imports[n].resolve(pending); await flush();
      await vi.advanceTimersByTimeAsync(4);
    }
    expect(api.history).toHaveBeenCalledTimes(2);
    queries[0].resolve({ ...page(), indexed_recordings: 7 }); await flush();
    expect(events.filter(event => event.actionId === "foreground").map(event => event.kind))
      .toEqual(["history_query_scheduled", "history_query_dispatched", "history_query_completed", "history_query_admitted"]);
    expect(controller.value.page?.indexed_recordings).toBe(7);
    expect(api.history).toHaveBeenCalledTimes(3); // One refresh for all three imports.
    imports[3].resolve(pending); await flush();
    expect(api.history).toHaveBeenCalledTimes(3);
    queries[1].resolve(page()); await flush();
    expect(api.history).toHaveBeenCalledTimes(4); // Import during refresh was retained.
    expect(fixture.bounds()).toEqual({ maximumImports: 1, maximumQueries: 1 });
  } finally { await fixture.finish(); }
});

it("keeps foreground priority when an import completes after the foreground result", async () => {
  const fixture = await overlappingImports();
  try {
    fixture.queries[0].resolve(page()); await flush();
    expect(fixture.events.filter(event => event.actionId === "foreground" && event.kind === "history_query_admitted")).toHaveLength(1);
    expect(fixture.api.history).toHaveBeenCalledTimes(2);
    fixture.imports[0].resolve(fixture.pending); await flush();
    expect(fixture.api.history).toHaveBeenCalledTimes(3);
  } finally { await fixture.finish(); }
});

it("prioritizes newer user input over dirty refresh and consumes commits before dispatch", async () => {
  const fixture = await overlappingImports();
  try {
    fixture.imports[0].resolve(fixture.pending); await flush();
    fixture.controller.setQuery({ search: "latest", limit: 50 }, "latest");
    fixture.queries[0].resolve(page()); await flush();
    expect(fixture.api.history).toHaveBeenCalledTimes(3);
    expect(fixture.api.history).toHaveBeenLastCalledWith("scan-1", expect.objectContaining({ search: "latest" }));
    fixture.queries[1].resolve(page()); await flush();
    expect(fixture.api.history).toHaveBeenCalledTimes(3);
    expect(fixture.events.filter(event => event.actionId === "latest" && event.kind === "history_query_admitted")).toHaveLength(1);
  } finally { await fixture.finish(); }
});

it("releases a stale context query into current-context work and discards its dirty bit", async () => {
  for (const nextOrigin of [origin("B", 2), origin("A", 3)]) {
    const fixture = await overlappingImports();
    try {
      fixture.imports[0].resolve(fixture.pending); await flush();
      fixture.controller.setContext(null);
      fixture.controller.setContext(nextOrigin, [game]);
      fixture.controller.setQuery({ search: "new context", limit: 50 }, "new-context");
      await vi.advanceTimersByTimeAsync(4);
      fixture.queries[0].resolve(page()); await flush();
      expect(fixture.api.history).toHaveBeenLastCalledWith(nextOrigin.token, expect.objectContaining({ search: "new context" }));
      fixture.queries[1].resolve(page()); await flush();
      expect(fixture.api.history).toHaveBeenCalledTimes(3);
      expect(fixture.events.filter(event => event.actionId === "foreground" && event.kind === "history_query_admitted")).toHaveLength(0);
      expect(fixture.events.filter(event => event.actionId === "new-context" && event.kind === "history_query_admitted")).toHaveLength(1);
    } finally { await fixture.finish(); }
  }
});

it("coalesces dirty error recovery once and close prevents any new refresh", async () => {
  const fixture = await overlappingImports();
  try {
    fixture.imports[0].resolve(fixture.pending); await flush();
    fixture.queries[0].reject(new Error("query failed")); await flush();
    expect(fixture.api.history).toHaveBeenCalledTimes(3);
    expect(fixture.controller.value).toMatchObject({ error: null, loading: true });
    await vi.advanceTimersByTimeAsync(4);
    fixture.imports[1].resolve(fixture.pending); await flush();
    fixture.controller.close();
    fixture.queries[1].resolve(page()); await flush();
    expect(fixture.api.history).toHaveBeenCalledTimes(3);
  } finally { await fixture.finish(); }
});

it("distinguishes queued replacements from completed superseded work without cancelling IPC", async () => {
  vi.useFakeTimers();
  const events: HistoryQueryObservation[] = [];
  const active = deferred<HistoryPage>();
  const api = { current: () => true, catalog: async (token: string) => catalog(token), recording: vi.fn(),
    history: vi.fn().mockReturnValueOnce(active.promise).mockResolvedValue(page()), observeQuery: (event: HistoryQueryObservation) => events.push(event) };
  const controller = new HistoryController(api);
  controller.setContext(origin("A", 1), [game]);
  controller.setQuery({ limit: 50, search: "first" }, "search-1");
  await vi.advanceTimersByTimeAsync(4);
  controller.setQuery({ limit: 50, search: "replaced" }, "search-2");
  controller.setQuery({ limit: 50, search: "latest" }, "search-3");
  expect(api.history).toHaveBeenCalledTimes(1);
  active.resolve(page()); await flush();
  expect(api.history).toHaveBeenCalledTimes(2);
  expect(events.filter(event => event.actionId === "search-1").map(event => event.kind))
    .toEqual(["history_query_scheduled", "history_query_dispatched", "history_query_completed", "history_query_superseded"]);
  expect(events.filter(event => event.actionId === "search-2").map(event => event.kind))
    .toEqual(["history_query_scheduled", "history_query_replaced"]);
  expect(events.filter(event => event.actionId === "search-3").map(event => event.kind))
    .toEqual(["history_query_scheduled", "history_query_dispatched", "history_query_completed", "history_query_admitted"]);
  controller.close();
});

it("queues the latest input until catalog admission and preserves its action while recordings import", async () => {
  vi.useFakeTimers();
  const ready = deferred<CatalogPage>(), importing = deferred<ImportProgress>();
  const events: HistoryQueryObservation[] = [];
  const o = origin("A", 1);
  const api = { current: () => true, catalog: vi.fn(() => ready.promise), recording: vi.fn(() => importing.promise),
    history: vi.fn(async () => page()), observeQuery: (event: HistoryQueryObservation) => events.push(event) };
  const controller = new HistoryController(api);
  controller.setContext(o, [game]);
  controller.setQuery({ search: "old", limit: 50 }, "old-input");
  await vi.advanceTimersByTimeAsync(4);
  controller.setQuery({ search: "latest", limit: 50 }, "latest-input");
  expect(api.history).not.toHaveBeenCalled();
  expect(controller.value.error).toBeNull();
  ready.resolve({ ...catalog(o.token), items: [{ game_timestamp: game.timestamp, media_id: game.media_id!, error: null,
    progress: { media_id: game.media_id!, status: "pending", batches: 4 } as ImportProgress }] });
  await flush();
  expect(api.history).toHaveBeenCalledExactlyOnceWith(o.token, expect.objectContaining({ search: "latest" }));
  expect(events.filter(event => event.actionId === "old-input").map(event => event.kind))
    .toEqual(["history_query_scheduled", "history_query_replaced"]);
  expect(events.filter(event => event.actionId === "latest-input").map(event => event.kind))
    .toEqual(["history_query_scheduled", "history_query_dispatched", "history_query_completed", "history_query_admitted"]);
  await vi.advanceTimersByTimeAsync(4);
  expect(api.recording).toHaveBeenCalledTimes(1);
  controller.setQuery({ search: "during import", limit: 50 }, "import-input");
  await flush();
  expect(api.history).toHaveBeenCalledTimes(2);
  expect(controller.value).toMatchObject({ indexing: true, loading: false, error: null });
  controller.close();
  importing.resolve({ media_id: game.media_id!, status: "ready" } as ImportProgress);
  await flush();
});

it("yields between bounded imports while queries run independently and close cancels continuation", async () => {
  vi.useFakeTimers();
  const o = origin("A", 1), first = deferred<ImportProgress>(), second = deferred<ImportProgress>();
  const pending = { media_id: game.media_id!, status: "pending", batches: 4 } as ImportProgress;
  const api = {
    current: () => true,
    catalog: vi.fn(async () => ({ ...catalog(o.token), items: [{ game_timestamp: game.timestamp, media_id: game.media_id!, progress: pending }] } as CatalogPage)),
    recording: vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise),
    history: vi.fn(async () => page()),
  };
  const controller = new HistoryController(api); controller.setContext(o, [game]);
  await flush(); expect(api.catalog).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(4);
  expect(api.catalog).toHaveBeenCalledTimes(1); expect(api.recording).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(4);
  expect(api.recording).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(500);
  expect(api.recording).toHaveBeenCalledTimes(1);
  controller.setQuery({ search: "Lux", limit: 50 }); await flush();
  expect(api.history).toHaveBeenLastCalledWith(o.token, expect.objectContaining({ search: "Lux" }));
  expect(controller.value.loading).toBe(false);
  first.resolve(pending); await flush();
  expect(api.recording).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(4);
  expect(api.recording).toHaveBeenCalledTimes(2);
  controller.close(); second.resolve(pending); await flush();
  await vi.advanceTimersByTimeAsync(500);
  expect(api.recording).toHaveBeenCalledTimes(2);
});

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

it("retains accepted pages and pagination until replacement succeeds, including query errors", async () => {
  vi.useFakeTimers();
  const first = { ...page(), next: { recorded_at: "2026-01-01", media_id: game.media_id! } };
  const second = { ...first, indexed_recordings: 2 };
  const next = deferred<HistoryPage>(), failed = deferred<HistoryPage>(), latest = deferred<HistoryPage>();
  const api = { current: () => true, catalog: async (token: string) => catalog(token), recording: vi.fn(),
    history: vi.fn().mockResolvedValueOnce(first).mockReturnValueOnce(next.promise)
      .mockReturnValueOnce(failed.promise).mockReturnValueOnce(latest.promise) };
  const controller = new HistoryController(api);
  controller.setContext(origin("A", 1), [game]);
  await vi.advanceTimersByTimeAsync(100);
  controller.nextPage();
  expect(controller.value).toMatchObject({ page: first, loading: true, laterPage: false });
  controller.nextPage();
  expect(api.history).toHaveBeenCalledTimes(2);
  next.resolve(second); await flush();
  expect(controller.value).toMatchObject({ page: second, loading: false, laterPage: true });
  controller.setQuery({ limit: 50, search: "Ahri" });
  expect(controller.value).toMatchObject({ page: second, loading: true, laterPage: true });
  failed.reject(new Error("Search failed")); await flush();
  expect(controller.value).toMatchObject({ page: second, loading: false, laterPage: true, error: "Search failed" });
  controller.nextPage(); controller.firstPage();
  expect(api.history).toHaveBeenCalledTimes(3);
  controller.setQuery({ limit: 50, search: "Lux" });
  expect(controller.value.error).toBeNull();
  latest.resolve(page()); await flush();
  expect(controller.value).toMatchObject({ page: page(), loading: false, laterPage: false });
  controller.setContext(origin("B", 2), [game]);
  expect(controller.value.page).toBeNull();
  controller.close();
});

it("rejects superseded same-context queries while retaining the previous page", async () => {
  vi.useFakeTimers();
  const accepted = page(), old = deferred<HistoryPage>(), latest = deferred<HistoryPage>();
  const api = { current: () => true, catalog: async (token: string) => catalog(token), recording: vi.fn(),
    history: vi.fn().mockResolvedValueOnce(accepted).mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise) };
  const controller = new HistoryController(api); controller.setContext(origin("A", 1), [game]);
  await vi.advanceTimersByTimeAsync(100);
  controller.setQuery({ limit: 50, search: "old" }); controller.setQuery({ limit: 50, search: "latest" });
  old.resolve({ ...page(), indexed_recordings: 99 }); await flush();
  expect(controller.value).toMatchObject({ page: accepted, loading: true });
  latest.resolve({ ...page(), indexed_recordings: 3 }); await flush();
  expect(controller.value.page?.indexed_recordings).toBe(3);
  controller.close();
});
