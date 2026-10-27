// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createBenchmarkViewer, type BenchmarkViewerHost } from "./benchmarkViewer";
import { buildScenarioActions, replayBenchmarkObserver, type ReplayBenchmarkObserver, type ScenarioAction } from "./benchmark";
import type { PlaybackEvent, PlaybackSnapshot, SeekOutcome } from "./playbackController";
import { createViewerSeekState } from "./viewerSeekState";
import { parseMediaId, type ReplayTick } from "./replayTime";
import { runBenchmarkDesktopResize } from "./benchmarkResize";
import { waitForBenchmarkRenderedReplay } from "./benchmarkRender";
import viewerSource from "./components/ViewerScreen.tsx?raw";
import runnerSource from "./benchmarkViewer.ts?raw";

vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  replayBenchmarkObserver: vi.fn(), buildScenarioActions: vi.fn() }));
vi.mock("./benchmarkResize", () => ({ runBenchmarkDesktopResize: vi.fn(async () => {}) }));
vi.mock("./benchmarkRender", async original => ({ ...await original<typeof import("./benchmarkRender")>(),
  waitForBenchmarkRenderedReplay: vi.fn(async () => {}) }));
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
};
const settle = async () => { for (let i = 0; i < 80; i++) await Promise.resolve(); };
let frames: Map<number, FrameRequestCallback>, nextFrame: number;
const frame = () => { const [id, callback] = [...frames][0] ?? []; if (callback) { frames.delete(id); callback(performance.now()); } };
beforeEach(() => {
  vi.useFakeTimers(); frames = new Map(); nextFrame = 0;
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.set(++nextFrame, callback); return nextFrame; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(id => { frames.delete(id); });
  vi.mocked(runBenchmarkDesktopResize).mockResolvedValue(); vi.mocked(waitForBenchmarkRenderedReplay).mockResolvedValue();
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.resetAllMocks(); document.body.replaceChildren(); });
const setup = (actions: ScenarioAction[] = [], opening = false, detailsReady = true) => {
  vi.mocked(buildScenarioActions).mockReturnValue(actions);
  const mediaId = parseMediaId("11111111-2222-4333-8444-555555555555");
  let snapshot = { readiness: "ready", generation: 7, mediaId, sessionToken: "load", sourceUrl: "fixture", recoveryCount: 0,
    seek: createViewerSeekState(7), media: { paused: true, ended: false, browserSeconds: 0, tick: 0, readyState: 4, networkState: 1 },
    rate: { selected: 1, applied: 1 }, quality: { totalFrames: 100, droppedFrames: 0 }, muted: false, volume: 1 } as unknown as PlaybackSnapshot;
  let details = { ready: detailsReady, error: null as unknown }, mountLive = true, actionId = 0;
  const trace: string[] = [];
  const observer = { scenario: () => ({ id: opening ? "qb010-opening-v1-cold-open" : "unit", kind: "cold_open" }),
    nextActionId: (kind: string) => `${kind}-${++actionId}`, now: () => performance.now(),
    emit: vi.fn((kind: string) => { trace.push(kind); return true; }), complete: vi.fn(async () => { trace.push("terminal"); }) };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as ReplayBenchmarkObserver);
  const root = document.createElement("div"); document.body.append(root);
  const elementToken = {}, mountToken = {}, source = { media_id: mediaId, run_id: mediaId, epoch: 0 };
  const change = (patch: Partial<PlaybackSnapshot>) => { snapshot = { ...snapshot, ...patch }; };
  const host: BenchmarkViewerHost = {
    snapshot: () => snapshot, details: () => details, presentationClock: () => "VIDEO FRAME",
    play: vi.fn<BenchmarkViewerHost["play"]>(async () => { trace.push("play"); change({ media: { ...snapshot.media, paused: false } }); return { status: "playing" }; }),
    pause: vi.fn<BenchmarkViewerHost["pause"]>(() => { trace.push("pause"); change({ media: { ...snapshot.media, paused: true } }); return { status: "paused" }; }),
    setRate: vi.fn(), requestSeek: vi.fn<BenchmarkViewerHost["requestSeek"]>((target, request) => {
      trace.push("seek");
      event("seek_requested", request.actionId); event("seek_dispatched", request.actionId);
      event("seek_presented", request.actionId, { authoritative: true }); event("seeked", request.actionId);
      return Promise.resolve({ status: "presented", target });
    }),
    prepareEventJump: vi.fn(target => ({ target, eventType: "ChampionKill" })),
    editClipEndpoint: vi.fn((_endpoint, target) => ({ target, start: 0 as ReplayTick, end: 480_000_000 as ReplayTick })),
    finishClipEndpointEdit: vi.fn(() => { trace.push("endpoint-finished"); }), setClipSelection: vi.fn(),
    setFullscreen: vi.fn(() => { trace.push("fullscreen"); }), refreshMetrics: vi.fn(async () => { trace.push("refresh"); }),
    sampleMetrics: vi.fn(() => { trace.push("sample"); return snapshot; }),
    serverMetrics: () => ({ requests: 0, range_requests: 0, response_bytes: 0, completed_streams: 0, cancelled_streams: 0,
      active_connections: 0, peak_connections: 0, rejected_connections: 0, rejected_requests: 0, active_streams: 0, peak_streams: 0 }),
    release: vi.fn(() => { trace.push("release"); change({ readiness: "disposed" }); }), cycleCompleted: vi.fn(),
    resize: { snapshot: () => snapshot, play: async () => ({ status: "playing" }), pause: () => ({ status: "paused" }),
      subscribeFrames: () => () => {}, readMediaIdentity: () => ({ elementToken, connected: true, currentSrc: "fixture" }) },
    render: { root: () => root, read: () => ({ elementToken, connected: true, assetsAvailable: true, generation: snapshot.generation,
      sessionToken: snapshot.sessionToken, sourceUrl: snapshot.sourceUrl, recoveryCount: snapshot.recoveryCount, paused: true,
      activeSource: source, overlaySource: source, windowSource: source, presentedTick: null, rendered: null }) },
  };
  const runner = createBenchmarkViewer(observer as unknown as ReplayBenchmarkObserver, host, {
    replayEnd: 240 * 48_000_000 as ReplayTick, opening: opening ? { cycle: 0, fixture_id: "S", gameTimestamp: "1" } : undefined,
    mountToken, isCurrentMount: token => mountLive && token === mountToken,
  });
  function event(kind: string, actionId?: string, payload: Record<string, unknown> = {}, generation = snapshot.generation) {
    const value: PlaybackEvent = { kind, actionId, payload, generation, atMs: performance.now() };
    runner.onPlaybackEvent(value);
  }
  const ready = async () => { runner.updateReadiness(); event("media_canplay"); event("first_presented_frame"); await settle(); };
  return { runner, host, observer, trace, ready, event, change, snapshot: () => snapshot,
    setDetails: (ready: boolean, error: unknown = null) => { details = { ready, error }; },
    stale: () => { mountLive = false; } };
};

it("keeps eligibility queued, generation-bound and idempotent with either details ordering", async () => {
  for (const first of ["media", "details"]) {
    const f = setup(); f.setDetails(false); f.runner.armReadinessDeadline();
    if (first === "media") await f.ready();
    f.setDetails(true); f.runner.detailsAdmitted(); f.runner.updateReadiness();
    expect(buildScenarioActions).not.toHaveBeenCalled();
    if (first === "details") await f.ready(); else await settle();
    await vi.advanceTimersByTimeAsync(100);
    f.event("media_canplay"); f.event("first_presented_frame"); f.runner.updateReadiness(); await settle();
    expect(buildScenarioActions).toHaveBeenCalledOnce(); expect(f.observer.complete).toHaveBeenCalledOnce();
    f.runner.dispose(); vi.mocked(buildScenarioActions).mockClear();
    expect(vi.getTimerCount()).toBe(0);
  }
});
it("arms the readiness deadline only at the explicit mount notification", async () => {
  const f = setup();
  await vi.advanceTimersByTimeAsync(30_001); expect(f.observer.emit).not.toHaveBeenCalled();
  f.runner.armReadinessDeadline(); await vi.advanceTimersByTimeAsync(30_100);
  expect(f.observer.emit).toHaveBeenCalledWith("scenario_failed", { phase: "media_readiness", reason: "media readiness timed out" }, expect.anything());
  expect(f.observer.complete).toHaveBeenCalledOnce(); f.runner.dispose(); expect(vi.getTimerCount()).toBe(0);
});
it("lets opening actions start before details but retains the details terminal gate", async () => {
  const f = setup([{ kind: "play" }], true, false); f.runner.armReadinessDeadline();
  await f.ready(); expect(f.host.play).toHaveBeenCalledOnce(); expect(f.host.release).not.toHaveBeenCalled();
  f.setDetails(true); f.runner.detailsAdmitted(); f.runner.updateReadiness(); await settle();
  await vi.advanceTimersByTimeAsync(100);
  expect(f.observer.complete).toHaveBeenCalledWith("complete", null, { action_count: 1 });
  f.runner.dispose(); expect(vi.getTimerCount()).toBe(0);
});
it.each(["ordinary", "opening"])("retains detail failure phase and exactly one terminal (%s)", async mode => {
  const f = setup([{ kind: "wait", durationMs: 1000 }], mode === "opening"); await f.ready();
  f.runner.detailsFailed("details unavailable"); f.runner.detailsFailed("again"); f.runner.dispose();
  await vi.advanceTimersByTimeAsync(40_000);
  expect(f.observer.complete).toHaveBeenCalledOnce();
  expect(f.observer.emit).toHaveBeenCalledWith("scenario_failed", expect.objectContaining({ phase: "league_replay_window", reason: "details unavailable" }), expect.anything());
  expect(f.trace).not.toContain("scenario_completed"); expect(vi.getTimerCount()).toBe(0);
});
it.each(["presented-first", "seeked-first", "deduped"])("registers synchronous seek evidence before request and supports %s", async order => {
  const f = setup([{ kind: "seek", targetMs: 10000, reason: "benchmark" }]);
  vi.mocked(f.host.requestSeek).mockImplementation((target, request) => {
    expect(request.actionId).toBe("seek-1");
    if (order === "deduped") f.event("seek_deduped", request.actionId);
    else {
      f.event("seek_dispatched", request.actionId);
      for (const kind of order === "seeked-first" ? ["seeked", "seek_presented"] : ["seek_presented", "seeked"])
        f.event(kind, request.actionId, { authoritative: true });
    }
    return Promise.resolve({ status: order === "deduped" ? "deduplicated" : "presented", target });
  });
  await f.ready(); await vi.advanceTimersByTimeAsync(200);
  expect(f.observer.complete).toHaveBeenCalledWith("complete", null, { action_count: 1 });
  expect(f.host.requestSeek).toHaveBeenCalledOnce(); f.runner.dispose(); expect(vi.getTimerCount()).toBe(0);
});
it.each(["recovery_started", "generation", "newer-intent", "not-authoritative", "failed", "unavailable", "superseded"])(
  "rejects incomplete/replaced seek evidence (%s)", async failure => {
    const f = setup([{ kind: "seek", targetMs: 10000, reason: "benchmark" }]);
    vi.mocked(f.host.requestSeek).mockImplementation(target => Promise.resolve({ status:
      ["failed", "unavailable", "superseded"].includes(failure) ? failure as SeekOutcome["status"] : "presented", target }));
    await f.ready();
    if (failure === "generation") f.event("seeked", "seek-1", {}, 8);
    else if (failure === "newer-intent") f.event("seek_requested", "user-2");
    else if (failure === "not-authoritative") {
      f.event("seek_dispatched", "seek-1"); f.event("seeked", "seek-1"); f.event("seek_presented", "seek-1", { authoritative: false });
    } else if (failure === "recovery_started") f.event(failure);
    await settle(); await vi.advanceTimersByTimeAsync(6200);
    expect(f.observer.complete).toHaveBeenCalledWith("failed", expect.any(String));
    expect(f.trace).not.toContain("scenario_completed"); f.runner.dispose(); expect(vi.getTimerCount()).toBe(0);
  });
it("enters fullscreen only after dispatch and waits for both layout frames and seek evidence", async () => {
  const f = setup([{ kind: "fullscreen", enabled: true, seekTargetMs: 5000 }]);
  const pending = deferred<SeekOutcome>(); vi.mocked(f.host.requestSeek).mockReturnValue(pending.promise);
  await f.ready(); expect(f.host.setFullscreen).not.toHaveBeenCalled();
  f.event("seek_dispatched", "seek-1"); await settle();
  expect(f.host.setFullscreen).toHaveBeenCalledWith(true, true); expect(frames.size).toBe(1);
  frame(); await settle(); expect(f.host.release).not.toHaveBeenCalled();
  frame(); f.event("seeked", "seek-1"); pending.resolve({ status: "presented", target: 240_000_000 as ReplayTick }); await settle();
  expect(f.host.release).not.toHaveBeenCalled(); f.event("seek_presented", "seek-1", { authoritative: true }); await settle();
  await vi.advanceTimersByTimeAsync(100); expect(f.observer.complete).toHaveBeenCalledOnce(); f.runner.dispose();
});
it("alternates endpoint intent and clears editing after successful evidence before the 100ms tail", async () => {
  const f = setup([10, 30].map(targetMs => ({ kind: "seek", targetMs, reason: "endpoint-edit" })));
  await f.ready();
  expect(f.host.editClipEndpoint).toHaveBeenNthCalledWith(1, "start", 480_000 as ReplayTick);
  expect(f.host.finishClipEndpointEdit).toHaveBeenCalledOnce();
  await vi.advanceTimersByTimeAsync(100);
  expect(f.host.editClipEndpoint).toHaveBeenNthCalledWith(2, "end", 1_440_000 as ReplayTick);
  await vi.advanceTimersByTimeAsync(200); expect(f.observer.complete).toHaveBeenCalledOnce(); f.runner.dispose();
});
it.each(["media", "details", "wait", "play", "rate", "stable-playback", "seek", "endpoint-tail", "scrub", "frame-one", "frame-two", "render", "resize", "release"])(
  "disposes finitely without late workload effects or duplicate terminals (%s)", async phase => {
    const actions: Record<string, ScenarioAction[]> = {
      wait: [{ kind: "wait", durationMs: 1000 }], play: [{ kind: "play" }], rate: [{ kind: "rate", rate: 2, durationMs: 1000 }],
      seek: [{ kind: "seek", targetMs: 1000, reason: "benchmark" }], scrub: [{ kind: "scrub", targetsMs: [1000, 2000], intervalMs: 1000 }],
      "endpoint-tail": [{ kind: "seek", targetMs: 1000, reason: "endpoint-edit" }], "stable-playback": [{ kind: "stable-playback", durationMs: 1000 }],
      "frame-one": [{ kind: "fullscreen", enabled: true }], "frame-two": [{ kind: "fullscreen", enabled: true }],
      render: [{ kind: "render-ready" }], resize: [{ kind: "desktop-resize" }],
    };
    const f = setup(actions[phase] ?? [], phase === "details", phase !== "details");
    const play = deferred<Awaited<ReturnType<BenchmarkViewerHost["play"]>>>(), seek = deferred<SeekOutcome>(), helper = deferred<void>();
    if (phase === "details") f.setDetails(false);
    if (phase === "play") vi.mocked(f.host.play).mockReturnValue(play.promise);
    if (phase === "rate") f.change({ media: { ...f.snapshot().media, paused: false } });
    if (phase === "seek") vi.mocked(f.host.requestSeek).mockReturnValue(seek.promise);
    if (phase === "render") vi.mocked(waitForBenchmarkRenderedReplay).mockReturnValue(helper.promise);
    if (phase === "resize") vi.mocked(runBenchmarkDesktopResize).mockReturnValue(helper.promise);
    f.runner.armReadinessDeadline();
    if (phase !== "media") await f.ready();
    if (phase === "frame-two") { frame(); await settle(); }
    f.runner.dispose(); f.runner.dispose();
    const trace = [...f.trace];
    play.resolve({ status: "playing" }); seek.resolve({ status: "presented", target: 0 as ReplayTick }); helper.resolve();
    f.event("seeked", "seek-1"); f.event("media_canplay"); f.runner.updateReadiness();
    await settle(); await vi.advanceTimersByTimeAsync(70_000);
    expect(f.trace).toEqual(trace); expect(f.observer.complete).toHaveBeenCalledOnce();
    expect(f.observer.complete).toHaveBeenCalledWith("failed", "viewer disposed before benchmark completion", ...(phase === "details" ? [expect.anything()] : []));
    expect(f.trace).not.toContain("scenario_completed"); expect(frames.size).toBe(0); expect(vi.getTimerCount()).toBe(0);
  });
it.each(["mount", "observer"])("cannot publish from an outgoing %s", async source => {
  const f = setup([{ kind: "wait", durationMs: 1000 }]); await f.ready();
  if (source === "mount") f.stale(); else vi.mocked(replayBenchmarkObserver).mockReturnValue({} as ReplayBenchmarkObserver);
  const trace = [...f.trace]; f.runner.dispose(); await vi.advanceTimersByTimeAsync(40_000);
  expect(f.trace).toEqual(trace); expect(f.observer.complete).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
});
it("allows native restoration after disposal without publishing late helper evidence", async () => {
  const f = setup([{ kind: "desktop-resize" }]), restored = deferred<void>();
  let publish!: () => void;
  vi.mocked(runBenchmarkDesktopResize).mockImplementation(async observer => {
    publish = () => { observer.emit("desktop_resize_restored", {}, { required: true }); };
    await restored.promise; publish();
  });
  await f.ready(); f.runner.dispose(); const trace = [...f.trace]; restored.resolve(); await settle();
  expect(f.trace).toEqual(trace); expect(f.observer.complete).toHaveBeenCalledOnce();
});
it("preserves success capture/release/delay/terminal order while media readiness is intentionally disposed", async () => {
  const f = setup(); await f.ready();
  expect(f.trace).toEqual(["pause", "refresh", "sample", "release"]);
  await vi.advanceTimersByTimeAsync(99); expect(f.observer.complete).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(f.trace).toEqual(["pause", "refresh", "sample", "release", "scenario_completed", "terminal"]);
  f.runner.dispose(); expect(f.observer.complete).toHaveBeenCalledOnce(); expect(vi.getTimerCount()).toBe(0);
});
it.each(["actions", "readiness"])("claims the original %s failure before the release tail can be disposed", async phase => {
  const f = setup(phase === "actions" ? [{ kind: "play" }] : []);
  vi.mocked(f.host.play).mockRejectedValue(new Error("native play failed"));
  if (phase === "actions") await f.ready();
  else { f.runner.armReadinessDeadline(); await vi.advanceTimersByTimeAsync(30_000); }
  expect(f.host.release).toHaveBeenCalledOnce(); expect(f.observer.complete).not.toHaveBeenCalled();
  expect(f.runner.isFinished()).toBe(true);
  f.runner.dispose(); await vi.advanceTimersByTimeAsync(40_000);
  expect(f.observer.complete).toHaveBeenCalledExactlyOnceWith("failed", phase === "actions" ? "native play failed" : "media readiness timed out");
  expect(f.observer.emit.mock.calls.filter(([kind]) => kind === "scenario_failed")).toHaveLength(1);
  expect(vi.getTimerCount()).toBe(0);
});
it("cannot sample or release after cancellation during the metrics refresh", async () => {
  const f = setup(), refresh = deferred<void>(); vi.mocked(f.host.refreshMetrics).mockReturnValue(refresh.promise);
  await f.ready(); f.runner.dispose(); const trace = [...f.trace]; refresh.resolve(); await settle();
  expect(f.trace).toEqual(trace); expect(f.host.sampleMetrics).not.toHaveBeenCalled(); expect(f.host.release).not.toHaveBeenCalled();
});
it("keeps the dependency direction and experiment resources outside Viewer", () => {
  expect(viewerSource).not.toMatch(/buildScenarioActions|ScenarioAction|benchmarkAbortController|benchmarkMediaReadyTimerId|benchmarkSeekObservers|dispatchObservers|runBenchmarkDesktopResize|runBenchmarkScenario/);
  expect(runnerSource).not.toMatch(/createPlaybackController|createHtmlVideoPlaybackAdapter|ReplayDataController|ReplayOverlayController|components\/ViewerScreen/);
  expect(viewerSource).toContain("const operation = controller.seek(target, options)");
  expect(viewerSource).toContain("requestSeek,");
});
