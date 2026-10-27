import { afterEach, expect, it, vi } from "vitest";
import { desktopResizeTargets, runBenchmarkDesktopResize, type ResizeGeometry, type ResizeWindow, type BenchmarkResizeSubject } from "./benchmarkResize";
import type { ReplayBenchmarkObserver } from "./benchmark";
import desktopConfig from "../src-tauri/tauri.conf.json";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
const setup = () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 0));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  const original: ResizeGeometry = { inner: { width: 1440, height: 900 }, outer: { width: 1456, height: 939 },
    position: { x: 50, y: 50 }, scale: 1, workArea: { position: { x: 0, y: 0 }, size: { width: 1920, height: 1080 } } };
  let current = structuredClone(original), generation = 1, paused = false;
  let native: ((size: { width: number; height: number }) => void) | undefined;
  const frames = new Set<(tick: number, authority: "rvfc" | "media-clock-approximate") => void>();
  const events: { kind: string; payload: Record<string, unknown>; meta?: Record<string, unknown> }[] = [];
  let nextId = 0;
  const observer = { nextActionId: () => `resize-${++nextId}`, now: () => Date.now(),
    emit: (kind: string, payload: Record<string, unknown>, meta?: Record<string, unknown>) => events.push({ kind, payload, meta }) } as unknown as ReplayBenchmarkObserver;
  const elementToken = {};
  const controller = { readMediaIdentity: () => ({ elementToken, connected: true, currentSrc: "fixture" }), snapshot: () => ({ generation, sessionToken: "load", sourceUrl: "fixture", mediaId: "media", readiness: "ready",
      recoveryCount: 0, media: { paused } }),
    subscribeFrames: (callback: (tick: number, authority: "rvfc" | "media-clock-approximate") => void) => {
      frames.add(callback); return () => frames.delete(callback);
    }, play: async () => { paused = false; return { status: "playing" }; }, pause: () => { paused = true; },
  } as unknown as BenchmarkResizeSubject;
  let deliverFrame = true, deliverNative = true;
  const window: ResizeWindow = { geometry: vi.fn(async () => current),
    listen: async callback => { native = callback; return () => { native = undefined; }; },
    resize: vi.fn(async size => {
      current = { ...current, inner: size, outer: { width: size.width + 16, height: size.height + 39 } };
      if (deliverNative) native?.(size);
      if (!paused && deliverFrame) setTimeout(() => { for (const callback of frames) callback(10, "rvfc"); }, 0);
    }) };
  const abort = new AbortController();
  const run = () => runBenchmarkDesktopResize(observer, controller, abort.signal, ms => new Promise(resolve => setTimeout(resolve, ms)), window);
  return { run, original, events, frames, window, abort, controller, current: () => current,
    listener: () => native, setGeneration: () => { generation++; },
    omitFrame: () => { deliverFrame = false; }, omitNative: () => { deliverNative = false; } };
};

it("uses physical sizes, eight RVFC and eight paused paint actions, then restores geometry", async () => {
  const f = setup();
  const run = f.run();
  await vi.advanceTimersByTimeAsync(5000); await run;
  const completed = f.events.filter(event => event.kind === "desktop_resize_completed");
  expect(completed).toHaveLength(16);
  expect(completed.filter(event => event.payload.authority === "rvfc")).toHaveLength(8);
  expect(completed.filter(event => event.payload.authority === "two-animation-frame-paint-opportunity")).toHaveLength(8);
  expect(f.current().inner).toEqual(f.original.inner);
  expect(f.events[f.events.length - 1].kind).toBe("desktop_resize_restored");
  expect(f.frames.size).toBe(0); expect(f.listener()).toBeUndefined(); expect(vi.getTimerCount()).toBe(0);
  const scaled = { ...f.original, scale: 1.25, position: { x: 0, y: 0 },
    workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1440 } } };
  expect(desktopResizeTargets(scaled)[0]).toEqual({ width: 1500, height: 850 });
  for (const geometry of [f.original, scaled]) {
    const [small] = desktopResizeTargets(geometry);
    expect(small.width).toBe(Math.ceil(desktopConfig.app.windows[0].minWidth * geometry.scale));
    expect(small.height).toBe(Math.ceil(desktopConfig.app.windows[0].minHeight * geometry.scale));
  }
});

it.each(["geometry", "listener"])("cancels during native %s admission before playback or resize", async phase => {
  const f = setup();
  let resume!: () => void;
  if (phase === "geometry") {
    vi.mocked(f.window.geometry).mockImplementationOnce(() => new Promise(resolve => { resume = () => resolve(f.original); }));
  } else {
    const listen = f.window.listen;
    f.window.listen = callback => new Promise(resolve => { resume = () => { void listen(callback).then(resolve); }; });
  }
  const play = vi.spyOn(f.controller, "play"), pause = vi.spyOn(f.controller, "pause");
  const outcome = f.run().catch(error => error as Error);
  await vi.advanceTimersByTimeAsync(0);
  f.abort.abort(); resume();
  await vi.advanceTimersByTimeAsync(0);
  expect(await outcome).toBeInstanceOf(Error);
  expect(play).not.toHaveBeenCalled(); expect(pause).not.toHaveBeenCalled();
  expect(f.window.resize).not.toHaveBeenCalled();
  if (phase === "geometry") expect(f.events).toEqual([]);
  expect(f.frames.size).toBe(0); expect(f.listener()).toBeUndefined(); expect(vi.getTimerCount()).toBe(0);
});

it.each(["frame", "native", "generation", "abort"])("rejects incomplete evidence and restores ownership (%s)", async failure => {
  const f = setup();
  if (failure === "frame") f.omitFrame();
  if (failure === "native") f.omitNative();
  const outcome = f.run().catch(error => error as Error);
  await vi.advanceTimersByTimeAsync(0);
  if (failure === "generation") f.setGeneration();
  if (failure === "abort") f.abort.abort();
  await vi.advanceTimersByTimeAsync(11000);
  expect(await outcome).toBeInstanceOf(Error);
  expect(f.current().inner).toEqual(f.original.inner);
  expect(f.frames.size).toBe(0); expect(f.listener()).toBeUndefined(); expect(vi.getTimerCount()).toBe(0);
});

it("drains an already issued native request before restoring after cancellation", async () => {
  const f = setup();
  const originalResize = f.window.resize;
  let finishNative!: () => void;
  let first = true;
  f.window.resize = vi.fn(size => {
    if (!first) return originalResize(size);
    first = false;
    return new Promise<void>(resolve => { finishNative = () => { void originalResize(size).then(resolve); }; });
  });
  const outcome = f.run().catch(error => error as Error);
  await vi.advanceTimersByTimeAsync(0);
  f.abort.abort();
  await vi.advanceTimersByTimeAsync(0);
  expect(f.events.some(event => event.kind === "desktop_resize_restored")).toBe(false);
  finishNative();
  await vi.advanceTimersByTimeAsync(0);
  expect(await outcome).toBeInstanceOf(Error);
  expect(f.current().inner).toEqual(f.original.inner);
  expect(f.events[f.events.length - 1].kind).toBe("desktop_resize_restored");
  expect(f.listener()).toBeUndefined(); expect(vi.getTimerCount()).toBe(0);
});
