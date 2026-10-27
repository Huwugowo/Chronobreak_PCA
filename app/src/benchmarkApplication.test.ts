// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import appSource from "./App.tsx?raw";
import viewerSource from "./components/ViewerScreen.tsx?raw";
import coordinatorSource from "./benchmarkApplication.ts?raw";
import { createBenchmarkApplication, type BenchmarkApplicationHost, type BenchmarkApplicationState } from "./benchmarkApplication";
import type { BenchmarkEventMeta, BenchmarkFixture, BenchmarkScenario, ReplayBenchmarkObserver } from "./benchmark";
import type { ClipExportResult, GameSummary, ReplayDescriptor } from "./types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "./replayTime";
import * as search from "./benchmarkSearch";
import { runDeliveryRouteProbe } from "./deliveryRouteProbe";

vi.mock("./benchmarkSearch", () => ({
  waitForBenchmarkSearchReady: vi.fn(), runBenchmarkSearchSequence: vi.fn(),
  runBenchmarkIndexSearch: vi.fn(), waitForBenchmarkLibraryReturn: vi.fn(), waitForBenchmarkMemoryWindow: vi.fn(),
}));
vi.mock("./deliveryRouteProbe", () => ({ runDeliveryRouteProbe: vi.fn() }));

const deferred = <T,>() => {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const settle = async () => { for (let i = 0; i < 100; i++) await Promise.resolve(); };
let frames: Map<number, FrameRequestCallback>;
let frameId: number;
const frame = () => {
  const [id, callback] = [...frames][0] ?? [];
  if (callback) { frames.delete(id); callback(1); }
};
beforeEach(() => {
  vi.useFakeTimers(); frames = new Map(); frameId = 0;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.set(++frameId, callback); return frameId; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(id => { frames.delete(id); });
  vi.mocked(search.waitForBenchmarkSearchReady).mockResolvedValue();
  vi.mocked(search.runBenchmarkSearchSequence).mockResolvedValue();
  vi.mocked(search.runBenchmarkIndexSearch).mockResolvedValue({ search_sequences: 3 } as never);
  vi.mocked(search.waitForBenchmarkLibraryReturn).mockResolvedValue(2);
  vi.mocked(search.waitForBenchmarkMemoryWindow).mockResolvedValue();
  vi.mocked(runDeliveryRouteProbe).mockResolvedValue([]);
  const input = document.createElement("input"); input.setAttribute("aria-label", "Search matches"); document.body.append(input);
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.resetAllMocks(); document.body.replaceChildren(); });

const setup = (scenario: Partial<BenchmarkScenario> = {}) => {
  const selected: BenchmarkScenario = { id: "historical", kind: "app_idle", trial_id: "1", ...scenario };
  const fixture: BenchmarkFixture = { id: "S", alias: "S", game_timestamp: "1", codec: "h264" };
  const game = { timestamp: "1", champion: "Ahri", duration_ms: 240000, video_available: true } as GameSummary;
  let state: BenchmarkApplicationState = {
    origin: { root: "A", rootEpoch: 1, request: 1, token: "snapshot-1", navigation: 1 },
    navigation: { screen: "library", tab: "games" }, loading: false, error: null,
    capability: { tested: true, supported: false, probe_url: "" },
    history: { page: null, loading: false, indexing: false, error: null, indexError: null, laterPage: false },
    library: { root: "A", request: 1, refreshing: false, actionable: true, error: null,
      snapshot: { token: "snapshot-1", games: [game, { ...game, timestamp: "2" }], clips: [],
        usage: { game_count: 2, clip_count: 0, games_bytes: 1, clips_bytes: 0 } },
      selectedGame: null, activeClip: null, deletion: null, durations: {}, busy: null, mutationRefresh: null },
  };
  const records: { kind: string; payload: Record<string, unknown>; meta?: BenchmarkEventMeta }[] = [];
  const trace: string[] = [];
  const observer = {
    scenario: () => selected,
    fixtureForScenario: vi.fn(() => fixture),
    beginOpeningCycle: vi.fn((_fixture, cycle) => trace.push(`opening:${cycle}`)),
    emit: vi.fn((kind: string, payload: Record<string, unknown> = {}, meta?: BenchmarkEventMeta) => records.push({ kind, payload, meta })),
    complete: vi.fn(async () => {}),
  };
  const descriptor: ReplayDescriptor = { snapshot_token: "snapshot-1", game_timestamp: "1", video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14400 as FrameBoundary,
        onePastLastPts: 14400n, replayEnd: 11520000000 as ReplayTick }, audio: { present: true } } };
  const result = { elapsed_ms: 10, setup_elapsed_ms: 1, source_probe_elapsed_ms: 1, source_probe_strategy: "recording_metadata",
    finalize_elapsed_ms: 1, total_file_size_bytes: 100, outputs: [] } satisfies ClipExportResult;
  const host: BenchmarkApplicationHost = {
    readState: () => state,
    captureContext: () => ({ origin: state.origin && { ...state.origin }, navigation: state.navigation }),
    isContextCurrent: context => JSON.stringify(context.origin) === JSON.stringify(state.origin) &&
      state.library.actionable && !state.library.refreshing && context.navigation === state.navigation,
    history: { get value() { return state.history; }, subscribe: () => () => false },
    openViewer: vi.fn((timestamp, context) => {
      expect(host.isContextCurrent(context)).toBe(true);
      trace.push(`open:${timestamp}`);
      state = { ...state, origin: { ...state.origin!, navigation: state.origin!.navigation + 1 }, navigation: { screen: "viewer", gameTimestamp: timestamp } };
      return true;
    }),
    publishBenchmarkGamesReturn: vi.fn(context => {
      expect(host.isContextCurrent(context)).toBe(true); trace.push("games");
      state = { ...state, navigation: { screen: "library", tab: "games" } }; return true;
    }),
    readReplayDescriptor: vi.fn(async () => descriptor), drainDurations: vi.fn(async () => true),
    exportClip: vi.fn(async () => result), invalidateRoot: vi.fn(root => { trace.push(`invalidate:${root}`); }),
  };
  const app = createBenchmarkApplication(observer as unknown as ReplayBenchmarkObserver, host);
  return { app, host, observer, fixture, descriptor, result, selected, records, trace, kinds: () => records.map(value => value.kind),
    state: () => state, change: (patch: Partial<BenchmarkApplicationState>) => { state = { ...state, ...patch }; },
    start: async () => { app.update(); frame(); frame(); await settle(); },
  };
};

it.each([
  [{}, 60000], [{ idle_seconds: 2 }, 2000], [{ parameters: { duration_ms: -3 } }, 1000],
  [{ parameters: { duration_ms: 900000 } }, 600000], [{ parameters: { duration_ms: Infinity } }, 60000],
] as const)("retains idle defaults/clamps %j -> %s", async (scenario, duration) => {
  const h = setup(scenario); await h.start(); h.app.update();
  await vi.advanceTimersByTimeAsync(duration - 1); expect(h.observer.complete).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1); h.app.update();
  expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { duration_ms: duration });
  expect(h.kinds()).toEqual(["hevc_capability", "library_useful", "scenario_started", "scenario_completed"]);
  h.app.dispose(); h.app.dispose();
});

it.each(["root", "rootEpoch", "request", "token", "navigation", "screen", "refresh"] as const)(
  "rejects %s invalidation at each paint/duration boundary", async field => {
    for (const stage of [0, 1, 2]) {
      const h = setup({ id: "qb010-library-v2-n-cold" });
      const drain = deferred<boolean>(); vi.mocked(h.host.drainDurations).mockReturnValue(drain.promise);
      h.app.update(); if (stage > 0) frame(); if (stage > 1) frame();
      if (field === "screen") h.change({ navigation: { screen: "library", tab: "clips" } });
      else if (field === "refresh") h.change({ library: { ...h.state().library, refreshing: true } });
      else h.change({ origin: { ...h.state().origin!, [field]: typeof h.state().origin![field] === "number" ? 9 : "changed" } });
      frame(); frame(); drain.resolve(true); await settle(); h.app.update();
      expect(h.observer.complete).toHaveBeenCalledTimes(1);
      expect(h.records.filter(value => value.kind === "scenario_failed")[0].payload.phase).toBe(stage === 2 ? "library_details" : "library_paint");
      expect(h.kinds()).not.toContain("scenario_started"); h.app.dispose();
    }
  },
);

it.each([0, 1, 2])("disposal suppresses queued paint/duration continuation at stage %s", async stage => {
  const h = setup(); const drain = deferred<boolean>(); vi.mocked(h.host.drainDurations).mockReturnValue(drain.promise);
  h.app.update(); if (stage > 0) frame(); if (stage > 1) frame();
  h.app.dispose(); const before = [...h.records]; frame(); frame(); drain.resolve(true); await settle(); h.app.update();
  expect(h.records).toEqual(before); expect(frames.size).toBe(0); expect(h.observer.complete).not.toHaveBeenCalled();
});

it("retains contract payloads and duration ordering across repeated live updates", async () => {
  const h = setup({ id: "qb010-library-v2-n-cold" }); const drain = deferred<boolean>();
  vi.mocked(h.host.drainDurations).mockReturnValue(drain.promise);
  h.app.update(); h.change({ origin: { ...h.state().origin! } }); h.app.update(); frame(); h.app.update(); frame();
  expect(h.kinds()).toEqual(["hevc_capability", "library_view_admitted", "games_library_usable"]);
  expect(h.records[1]).toEqual({ kind: "library_view_admitted", meta: { required: true }, payload: {
    measurement_contract: "games-library-usable-v1", refresh_request_token: "1", snapshot_token: "snapshot-1", view_token: "snapshot-1",
    game_count: 2, clip_count: 0, storage_game_count: 2, storage_clip_count: 0,
  } });
  drain.resolve(true); await settle(); h.app.update();
  expect(h.kinds().slice(-2)).toEqual(["library_useful", "scenario_started"]);
  expect(h.host.drainDurations).toHaveBeenCalledTimes(1); h.app.dispose();
});

it("admits versioned opening before optional capability/history/duration work", async () => {
  const h = setup({ id: "qb010-opening-v1-n-cold", kind: "cold_open" }); h.change({ capability: null }); await h.start();
  expect(h.kinds()).toEqual(["opening_library_usable", "scenario_started", "replay_requested"]);
  expect(h.records[0].payload).toEqual({ measurement_contract: "replay-opening-v1", gate: "snapshot", after_library_paint: true, fixture_id: "S", game_count: 2, clip_count: 0 });
  expect(h.trace).toEqual(["opening:0", "open:1"]); expect(h.host.drainDurations).not.toHaveBeenCalled(); h.app.dispose();
});

it.each(["library_load", "hevc_capability", "media_capability", "fixture_not_found"])("keeps %s failure observable", async phase => {
  const h = setup({ kind: "cold_open" });
  if (phase === "library_load") h.change({ error: new Error("fixture scan failed") });
  if (phase === "hevc_capability") h.change({ capability: { tested: false, supported: false, probe_url: "" } });
  if (phase === "media_capability") h.fixture.codec = "hevc";
  if (phase === "fixture_not_found") h.observer.fixtureForScenario.mockReturnValue(null as never);
  await h.start(); h.app.update();
  expect(h.kinds()).toContain("scenario_failed"); expect(h.host.openViewer).not.toHaveBeenCalled();
  expect(h.observer.complete).toHaveBeenCalledOnce(); h.app.dispose();
});

it.each(["library_search", "whole_app"])("retains %s prefix/empty/clear sequences and terminal ordering", async kind => {
  const h = setup({ kind }); await h.start();
  const sequences = vi.mocked(search.runBenchmarkSearchSequence).mock.calls;
  expect(sequences).toHaveLength(24);
  expect(sequences.slice(0, 3).map(call => [call[2], call[3]])).toEqual([
    [["A", "Ah", "Ahr", "Ahri"], "typing"], [["zzzzzz"], "settled"], [[""], "clear"],
  ]);
  expect(h.kinds().slice(2, 5)).toEqual(["scenario_started", "search_preconditioning_started", "search_preconditioning_ready"]);
  if (kind === "whole_app") {
    expect(h.kinds().slice(-2)).toEqual(["search_workload_completed", "replay_requested"]);
    const context = h.host.captureContext(); h.app.viewerCycleCompleted(context); h.app.viewerCycleCompleted(context);
    await vi.advanceTimersByTimeAsync(0);
    expect(h.kinds().slice(-3)).toEqual(["viewer_close_requested", "library_return_ready", "scenario_completed"]);
    expect(h.records[h.records.length - 1].payload).toEqual({ kind, viewer_cycles: 1, search_sequences: 24 });
    expect(h.trace).toEqual(["open:1", "games"]);
  } else expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { search_sequences: 24 });
  h.app.dispose();
});

it.each([0, 101, 1.1, "8"])("rejects invalid search iterations %s", async iterations => {
  const h = setup({ kind: "library_search", iterations }); await h.start();
  expect(h.records[h.records.length - 1]).toMatchObject({ kind: "scenario_failed", payload: { phase: "library_search" } });
  expect(search.runBenchmarkSearchSequence).not.toHaveBeenCalled(); h.app.dispose();
});

it.each(["iterations", "catalog"])("keeps frozen whole-app %s admission strict", async invalid => {
  const h = setup({ kind: "whole_app", iterations: invalid === "iterations" ? 7 : 8 });
  if (invalid === "catalog") h.state().library.snapshot!.games.pop();
  await h.start(); expect(h.host.openViewer).not.toHaveBeenCalled();
  expect(h.observer.complete).toHaveBeenCalledWith("failed", "whole-app workload requires eight search iterations and the settled two-game library"); h.app.dispose();
});

it("starts cold indexing diagnostic before optional work and preserves its result", async () => {
  const h = setup({ kind: "library_search", id: "cold-index-search", iterations: 1 }); h.change({ capability: null }); await h.start();
  expect(frames.size).toBe(0); expect(search.waitForBenchmarkSearchReady).not.toHaveBeenCalled();
  expect(search.runBenchmarkIndexSearch).toHaveBeenCalledWith(expect.anything(), h.host.history, "Ahri", 2, expect.any(AbortSignal));
  expect(h.kinds()).toEqual(["scenario_started", "scenario_completed"]); h.app.dispose();
});

it.each([true, false])("delivery route probe preserves success/failure (%s)", async success => {
  const h = setup({ kind: "cold_open", id: "delivery-route-probe" });
  if (!success) vi.mocked(runDeliveryRouteProbe).mockRejectedValue(new Error("fixture route failure"));
  await h.start();
  if (success) { expect(h.kinds().slice(-2)).toEqual(["delivery_route_probe", "replay_requested"]); expect(h.trace).toEqual(["open:1"]); }
  else { expect(h.observer.complete).toHaveBeenCalledWith("failed", "delivery route probe failed"); expect(h.trace).toEqual([]); }
  h.app.dispose();
});

it.each([["warm_open", undefined, 6], ["lifecycle", undefined, 20], ["warm_open", 0, 1], ["warm_open", 2.1, 6]] as const)(
  "preserves %s iteration default/clamp %s -> %s", async (kind, iterations, expected) => {
    const h = setup({ kind, iterations }); await h.start();
    let oldContext = h.host.captureContext();
    for (let cycle = 1; cycle <= expected; cycle++) {
      const context = h.host.captureContext(); h.app.viewerCycleCompleted(context); h.app.viewerCycleCompleted(context);
      if (cycle > 1) h.app.viewerCycleCompleted(oldContext);
      oldContext = context;
      if (cycle < expected) {
        expect(h.state().navigation.screen).toBe("library");
        await vi.advanceTimersByTimeAsync(99); expect(h.trace.filter(value => value.startsWith("open:"))).toHaveLength(cycle);
        await vi.advanceTimersByTimeAsync(1);
      } else await vi.advanceTimersByTimeAsync(0);
    }
    expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { viewer_cycles: expected });
    expect(h.host.openViewer).toHaveBeenCalledTimes(expected);
    expect(h.host.publishBenchmarkGamesReturn).toHaveBeenCalledTimes(kind === "lifecycle" ? expected : expected - 1);
    h.app.dispose();
  },
);

it("clamps the maximum cycle count and rejects obsolete reopen context", async () => {
  const h = setup({ kind: "warm_open", iterations: 99999 }); await h.start(); h.app.viewerCycleCompleted(h.host.captureContext());
  expect(h.records[h.records.length - 1].payload).toEqual({ completed_cycles: 1, expected_cycles: 10000 });
  h.change({ navigation: { screen: "library", tab: "clips" } }); await vi.advanceTimersByTimeAsync(100);
  expect(h.host.openViewer).toHaveBeenCalledTimes(1); expect(h.observer.complete).toHaveBeenCalledOnce(); h.app.dispose();
});

it.each(["root", "rootEpoch", "request", "token", "navigation", "screen", "refresh", "missing_origin"] as const)(
  "fails an active viewer cycle explicitly after %s context loss", async field => {
    const h = setup({ kind: "warm_open" }); await h.start();
    const context = h.host.captureContext();
    if (field === "screen") h.change({ navigation: { screen: "library", tab: "clips" } });
    else if (field === "refresh") h.change({ library: { ...h.state().library, refreshing: true } });
    else if (field === "missing_origin") h.change({ origin: null });
    else h.change({ origin: { ...h.state().origin!, [field]: typeof h.state().origin![field] === "number" ? 9 : "changed" } });
    h.app.viewerCycleCompleted(context); h.app.viewerCycleCompleted(context); h.app.update();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.records.slice(-1)).toEqual([{ kind: "scenario_failed", payload: {
      phase: "viewer_cycle", reason: "Benchmark application context changed",
    }, meta: { required: true } }]);
    expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("failed", "Benchmark application context changed", { phase: "viewer_cycle" });
    expect(h.kinds().filter(kind => kind === "scenario_failed")).toHaveLength(1);
    expect(h.host.publishBenchmarkGamesReturn).not.toHaveBeenCalled();
    expect(h.host.openViewer).toHaveBeenCalledTimes(1); h.app.dispose();
  },
);

it.each(["stale", "disposed"] as const)("ignores %s viewer callbacks even after context loss", async callback => {
  const h = setup({ kind: "warm_open" }); await h.start();
  const context = h.host.captureContext();
  if (callback === "stale") {
    h.app.viewerCycleCompleted(context); await vi.advanceTimersByTimeAsync(100);
  } else { h.app.dispose(); h.app.dispose(); }
  h.change({ origin: { ...h.state().origin!, root: "B" } });
  const before = [...h.records], trace = [...h.trace];
  h.app.viewerCycleCompleted(context);
  expect(h.records).toEqual(before); expect(h.trace).toEqual(trace);
  expect(h.observer.complete).not.toHaveBeenCalled(); h.app.dispose();
});

it.each(["initial", "reopen", "memory_reopen"] as const)("fails a refused %s viewer open explicitly", async stage => {
  const h = setup(stage === "memory_reopen"
    ? { kind: "lifecycle", id: "settled-memory", iterations: 6 } : { kind: "warm_open" });
  if (stage === "initial") vi.mocked(h.host.openViewer).mockReturnValue(false);
  await h.start();
  if (stage !== "initial") {
    vi.mocked(h.host.openViewer).mockReturnValue(false);
    h.app.viewerCycleCompleted(h.host.captureContext());
    await vi.advanceTimersByTimeAsync(stage === "memory_reopen" ? 0 : 100);
  }
  h.app.update(); h.app.viewerCycleCompleted(h.host.captureContext()); await vi.advanceTimersByTimeAsync(1000);
  expect(h.records.slice(-1)).toEqual([{ kind: "scenario_failed", payload: {
    phase: "replay_open", reason: "Benchmark replay could not be opened",
  }, meta: { required: true } }]);
  expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("failed", "Benchmark replay could not be opened", { phase: "replay_open" });
  expect(h.kinds().filter(kind => kind === "scenario_failed")).toHaveLength(1);
  expect(h.host.openViewer).toHaveBeenCalledTimes(stage === "initial" ? 1 : 2); h.app.dispose();
});

it("suppresses failure when a refused viewer open disposes its coordinator", async () => {
  const h = setup({ kind: "warm_open" });
  vi.mocked(h.host.openViewer).mockImplementation(() => { h.app.dispose(); return false; });
  await h.start(); h.app.update();
  expect(h.kinds()).not.toContain("scenario_failed"); expect(h.observer.complete).not.toHaveBeenCalled();
});

it("preserves all seven settled-memory windows and six actual viewer cycles", async () => {
  const h = setup({ kind: "lifecycle", id: "settled-memory", iterations: 6 }); await h.start();
  for (let cycle = 1; cycle <= 6; cycle++) { h.app.viewerCycleCompleted(h.host.captureContext()); await vi.advanceTimersByTimeAsync(0); }
  expect(vi.mocked(search.waitForBenchmarkMemoryWindow).mock.calls.map(call => call[2])).toEqual([0, 1, 2, 3, 4, 5, 6]);
  expect(h.host.openViewer).toHaveBeenCalledTimes(6); expect(h.host.publishBenchmarkGamesReturn).toHaveBeenCalledTimes(6);
  expect(h.observer.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { viewer_cycles: 6 }); h.app.dispose();
});

it.each(["ready", "sequence", "index", "delivery", "memory_baseline", "memory_return", "library_return"])(
  "cancels publication during %s wait", async wait => {
    const gate = deferred<never>();
    const kind = wait === "index" ? "library_search" : wait.startsWith("memory") ? "lifecycle" : wait === "library_return" ? "whole_app" : wait === "delivery" ? "cold_open" : "library_search";
    const h = setup({ kind, id: wait === "index" ? "cold-index-search" : wait.startsWith("memory") ? "settled-memory" : wait === "delivery" ? "delivery-route-probe" : "historical", iterations: wait === "index" ? 1 : wait.startsWith("memory") ? 6 : 8 });
    const method = wait === "ready" ? search.waitForBenchmarkSearchReady : wait === "sequence" ? search.runBenchmarkSearchSequence : wait === "index" ? search.runBenchmarkIndexSearch : wait === "delivery" ? runDeliveryRouteProbe : wait === "library_return" ? search.waitForBenchmarkLibraryReturn : search.waitForBenchmarkMemoryWindow;
    if (wait !== "memory_return") vi.mocked(method).mockReturnValue(gate.promise);
    await h.start();
    if (wait === "memory_return" || wait === "library_return") {
      if (wait === "memory_return") vi.mocked(search.waitForBenchmarkMemoryWindow).mockReturnValue(gate.promise);
      h.app.viewerCycleCompleted(h.host.captureContext()); await vi.advanceTimersByTimeAsync(0);
    }
    h.app.dispose(); const before = [...h.records]; gate.resolve(undefined as never); await settle(); await vi.advanceTimersByTimeAsync(1000);
    expect(h.records).toEqual(before); expect(h.observer.complete).not.toHaveBeenCalled();
    if (wait !== "delivery") expect(vi.mocked(method).mock.calls.some(call => call.some(value => value instanceof AbortSignal && value.aborted))).toBe(true);
  },
);

it.each([false, true])("export retains action/result ordering and reconciliation (failure=%s)", async failure => {
  const h = setup({ kind: "export", export_presets: ["invalid", "vertical"], music_modes: ["built_in"], gain_modes: ["non_unity"], clip_start_ms: -200, clip_end_ms: 1 });
  if (failure) vi.mocked(h.host.exportClip).mockRejectedValue(new Error("fixture encoder failed"));
  await h.start();
  expect(vi.mocked(h.host.exportClip).mock.calls[0]).toEqual([{
    game_timestamp: "1", media_id: h.descriptor.media_timeline.mediaId, start_frame: "0", end_frame_exclusive: "300",
    presets: ["vertical"], vertical_focus: 0.72, vertical_position: 0.5, music: { kind: "builtin", filename: "momentum.mp3" }, game_audio_volume: 0.8, music_volume: 1,
  }, expect.any(Function), "snapshot-1"]);
  expect(h.kinds().slice(-3)).toEqual(failure ? ["export_requested", "action_failed", "scenario_failed"] : ["export_requested", "export_completed", "scenario_completed"]);
  expect(h.records.find(value => value.kind === "export_requested")?.meta).toEqual({ actionId: "export-1", required: true });
  expect(h.trace).toEqual(["invalidate:A"]); expect(h.observer.complete).toHaveBeenCalledOnce(); h.app.dispose();
});

it.each(["descriptor", "backend"])("suppresses late %s export publication while retaining attempted-command cleanup", async stage => {
  const h = setup({ kind: "export" }); const gate = deferred<never>();
  if (stage === "descriptor") vi.mocked(h.host.readReplayDescriptor).mockReturnValue(gate.promise);
  else vi.mocked(h.host.exportClip).mockReturnValue(gate.promise);
  await h.start(); h.app.dispose(); const before = [...h.records];
  if (stage === "backend") vi.mocked(h.host.exportClip).mock.calls[0][1]({ stage: "encoding", percent: 50, preset: "horizontal", completed_outputs: 0, total_outputs: 1 });
  gate.resolve((stage === "descriptor" ? h.descriptor : h.result) as never); await settle();
  expect(h.records).toEqual(before); expect(h.observer.complete).not.toHaveBeenCalled();
  expect(h.trace).toEqual(stage === "backend" ? ["invalidate:A"] : []);
});

it("active export context loss reports action failure once and still reconciles its captured root", async () => {
  const h = setup({ kind: "export" }); const gate = deferred<ClipExportResult>(); vi.mocked(h.host.exportClip).mockReturnValue(gate.promise);
  await h.start(); h.change({ origin: { ...h.state().origin!, root: "B" } }); h.app.update();
  gate.resolve(h.result); await settle();
  expect(h.kinds().slice(-2)).toEqual(["action_failed", "scenario_failed"]); expect(h.observer.complete).toHaveBeenCalledOnce();
  expect(h.trace).toEqual(["invalidate:A"]); h.app.dispose();
});

it("keeps screen orchestration and playback ownership outside each other's dependency cone", () => {
  expect(appSource).not.toMatch(/scenario\(\)|scenario\.kind|scenario\.id|benchmark\w*Timer|benchmark\w*Frame/);
  for (const source of [appSource, viewerSource]) expect(source).not.toMatch(/REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT|chronobreak:replay-benchmark-viewer-cycle/);
  expect(coordinatorSource).not.toMatch(/from ["'][^"']*(?:playbackController|ViewerScreen|video)[^"']*["']/i);
});
