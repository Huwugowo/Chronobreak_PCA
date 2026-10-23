// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import LibraryScreen from "./components/LibraryScreen";
import { HistoryController } from "./historyController";
import { ReplayBenchmarkObserver, replayBenchmarkObserver } from "./benchmark";
import { observeBenchmarkHistory, runBenchmarkIndexSearch, runBenchmarkSearchSequence, waitForBenchmarkLibraryReturn, waitForBenchmarkMemoryWindow, waitForBenchmarkSearchReady } from "./benchmarkSearch";
import type { HistoryPage } from "./leagueIndex";

vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  replayBenchmarkObserver: vi.fn(() => null) }));
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.mocked(replayBenchmarkObserver).mockReset(); document.body.replaceChildren(); });

const setup = async (scenarioId = "search") => {
  vi.useFakeTimers();
  const observer = new ReplayBenchmarkObserver({ schema_version: 1, run_id: "search-test", observer_profile: "minimal",
    harness_initialization_ms: 0, session_elapsed_ms: 0, fixtures: [{ id: "S", alias: "S", game_timestamp: "1" }],
    scenarios: [{ id: scenarioId, trial_id: "t1", kind: "library_search", fixture_ids: ["S"] }],
    ddragon_mode: "offline", ddragon_cache_fingerprint: "fixture",
    event_queue: { capacity: 256, high_water_mark: 0, accepted_records: 0, dropped_records: 0 } }, 0);
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer);
  const events: Parameters<Parameters<typeof observer.subscribeEvents>[0]>[0][] = [];
  observer.subscribeEvents(event => events.push(event));
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { frames.push(callback); return frames.length; });
  const page: HistoryPage = { rows: [], next: null, indexed_recordings: 1, catalog_recordings: 1, unknown_recordings: 0 };
  const api = { current: () => true, catalog: vi.fn(async (token: string) => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 })),
    recording: vi.fn(), history: vi.fn(async () => page),
    observeQuery: (event: Parameters<typeof observeBenchmarkHistory>[0]) => observeBenchmarkHistory(event, generation => history.isQueryCurrent(generation)) };
  const history = new HistoryController(api);
  history.setContext({ root: "fixture", rootEpoch: 1, request: 1, token: "scan", navigation: 1 });
  const [state, setState] = createSignal(history.value);
  history.subscribe(setState);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <LibraryScreen tab="games" games={[]} clips={[]} history={state()}
    ddragon={{ state: "offline", version: null, asset_base_url: null, item_count: 0, champion_count: 0, cache_directory: "", error: null }}
    actionable snapshotOrigin={null} busyId={null} onHistoryQuery={(query, actionId) => history.setQuery(query, actionId)}
    onOpenGame={() => {}} onToggleSaved={() => {}} onDeleteGame={() => {}} onOpenClip={() => {}}
    onDeleteClip={() => {}} onOpenClipsFolder={() => {}} />, host);
  await vi.advanceTimersByTimeAsync(10);
  events.length = 0; api.history.mockClear();
  return { observer, events, frames, history, api, input: host.querySelector<HTMLInputElement>("input")!, dispose };
};

it("measures real input debounce, backend admission and two paint frames without recording text", async () => {
  const f = await setup();
  const signal = new AbortController();
  try {
    const run = runBenchmarkSearchSequence(f.observer, f.input, ["A", "Ah", "Ahr", "Ahri"], "typing", signal.signal);
    let complete = false; void run.then(() => { complete = true; });
    await vi.advanceTimersByTimeAsync(360);
    expect(f.api.history).toHaveBeenCalledExactlyOnceWith("scan", expect.objectContaining({ search: "Ahri" }));
    expect(f.events.filter(event => event.kind === "library_search_coalesced")).toHaveLength(3);
    expect(f.events.filter(event => event.kind === "history_query_admitted")).toHaveLength(1);
    expect(complete).toBe(false);
    f.frames.shift()!(0); await Promise.resolve(); expect(complete).toBe(false);
    f.frames.shift()!(1); await run;
    expect(complete).toBe(true);
    expect(f.events[f.events.length - 1]).toMatchObject({ kind: "library_search_painted", payload: { authority: "two-animation-frame-paint-opportunity" } });
    expect(JSON.stringify(f.events)).not.toContain("Ahri");
    const clear = runBenchmarkSearchSequence(f.observer, f.input, [""], "clear", signal.signal);
    await vi.advanceTimersByTimeAsync(0);
    expect(f.api.history).toHaveBeenLastCalledWith("scan", expect.objectContaining({ search: null }));
    f.frames.shift()!(2); f.frames.shift()!(3); await clear;
  } finally { signal.abort(); f.dispose(); f.history.close(); }
});

it("waits through the transient final import state until its refresh query completes", async () => {
  vi.useFakeTimers();
  let finishQuery!: (page: HistoryPage) => void;
  const page: HistoryPage = { rows: [], next: null, indexed_recordings: 1, catalog_recordings: 1, unknown_recordings: 0 };
  const history = new HistoryController({ current: () => true,
    catalog: async token => ({ snapshot_token: token, items: [], next: null, catalog_recordings: 1 }),
    recording: vi.fn(), history: () => new Promise(resolve => { finishQuery = resolve; }) });
  history.setContext({ root: "fixture", rootEpoch: 1, request: 1, token: "scan", navigation: 1 });
  let ready = false;
  const abort = new AbortController();
  const wait = waitForBenchmarkSearchReady(history, abort.signal).then(() => { ready = true; });
  await vi.advanceTimersByTimeAsync(10);
  expect(history.value.indexing).toBe(false);
  expect(history.value.loading).toBe(true);
  expect(ready).toBe(false);
  finishQuery(page); await wait;
  expect(ready).toBe(true);
  history.close();
});

it("ignores untagged index refreshes in benchmark telemetry", async () => {
  const f = await setup();
  try {
    observeBenchmarkHistory({ kind: "history_query_completed", generation: 1, rowCount: 0 }, () => true);
    expect(f.events).toHaveLength(0);
  } finally { f.dispose(); f.history.close(); }
});

it.each([true, false])("runs bounded real search and truthfully reports indexing overlap (%s)", async indexing => {
  const f = await setup("cold-index-search");
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  const control = new AbortController();
  try {
    if (indexing) f.history.setContext({ root: "fixture", rootEpoch: 1, request: 2, token: "scan2", navigation: 1 });
    const run = runBenchmarkIndexSearch(f.observer, f.history, "Ahri", 1, control.signal);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(await run).toMatchObject({ indexing_observed: indexing, import_completed: true, search_sequences: 3 });
    expect(f.events.filter(event => event.kind === "library_search_sequence")).toHaveLength(3);
    expect(f.events.some(event => event.kind === "indexing_diagnostic_progress" && event.payload?.indexing === indexing)).toBe(true);
  } finally { control.abort(); f.dispose(); f.history.close(); }
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["settled", "changed-page", "aborted", "timeout"])("checks the mounted library and stable admitted page before return (%s)", async outcome => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  const page = { rows: [{}, {}] } as HistoryPage;
  const state = { page, loading: false, indexing: false, error: null, indexError: null };
  const history = { get value() { return state; } } as Pick<HistoryController, "value">;
  const input = document.createElement("input"); input.setAttribute("aria-label", "Search matches"); document.body.append(input);
  const abort = new AbortController();
  const wait = waitForBenchmarkLibraryReturn(history, abort.signal);
  const expectation = outcome === "aborted" || outcome === "timeout" ? expect(wait).rejects.toThrow() : expect(wait).resolves.toBe(2);
  if (outcome === "timeout") input.remove();
  if (outcome === "aborted") abort.abort();
  if (outcome === "changed-page") { await vi.advanceTimersByTimeAsync(17); state.page = { ...page }; }
  await vi.advanceTimersByTimeAsync(outcome === "timeout" ? 10_001 : 100); await expectation;
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["settled", "video", "changed-page", "aborted"])("only samples a quiet mounted library (%s)", async outcome => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  const state = { page: { rows: [{}, {}] } as HistoryPage, loading: false, indexing: false, error: null, indexError: null };
  const history = { get value() { return state; } } as Pick<HistoryController, "value">;
  const input = document.createElement("input"); input.setAttribute("aria-label", "Search matches"); document.body.append(input);
  const observer = { emit: vi.fn(), flush: vi.fn(async () => {}) };
  const abort = new AbortController();
  const run = waitForBenchmarkMemoryWindow(observer as unknown as ReplayBenchmarkObserver, history, 2, abort.signal);
  const result = outcome === "settled" ? expect(run).resolves.toBeUndefined() : expect(run).rejects.toThrow();
  await vi.advanceTimersByTimeAsync(5_100);
  expect(observer.emit).toHaveBeenCalledWith("memory_window_started", { cycle: 2, settle_ms: 5000, sample_ms: 10000 }, { required: true });
  if (outcome === "video") document.body.append(document.createElement("video"));
  if (outcome === "changed-page") state.page = { ...state.page };
  if (outcome === "aborted") abort.abort();
  await vi.advanceTimersByTimeAsync(10_000); await result;
  expect(observer.emit.mock.calls.some(call => call[0] === "memory_window_completed")).toBe(outcome === "settled");
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["origin", "timeout", "dispose"])("rejects missing or stale paint (%s)", async failure => {
  const f = await setup();
  const signal = new AbortController();
  try {
    const run = runBenchmarkSearchSequence(f.observer, f.input, ["Ahri"], "settled", signal.signal);
    const rejected = expect(run).rejects.toThrow(/search/);
    await vi.advanceTimersByTimeAsync(180);
    if (failure === "origin") {
      f.frames.shift()!(0);
      f.history.setContext(null);
      f.frames.shift()!(1);
    } else if (failure === "timeout") await vi.advanceTimersByTimeAsync(10_000);
    else signal.abort();
    await rejected;
    expect(f.events.some(event => event.kind === "library_search_painted")).toBe(false);
  } finally { signal.abort(); f.dispose(); f.history.close(); }
});
