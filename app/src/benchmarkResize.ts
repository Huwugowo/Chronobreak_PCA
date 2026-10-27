import { invoke } from "@tauri-apps/api/core";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import type { PlaybackController } from "./playbackController";
import type { ReplayBenchmarkObserver } from "./benchmark";

type Size = { width: number; height: number };
type Position = { x: number; y: number };
export type ResizeGeometry = {
  inner: Size; outer: Size; position: Position; scale: number;
  workArea: { position: Position; size: Size };
};
export type ResizeWindow = {
  geometry(): Promise<ResizeGeometry>;
  listen(callback: (size: Size) => void): Promise<() => void>;
  resize(size: Size): Promise<void>;
};
const nativeWindow: ResizeWindow = {
  async geometry() {
    const window = getCurrentWindow();
    const [inner, outer, position, scale, monitor, minimized, maximized] = await Promise.all([
      window.innerSize(), window.outerSize(), window.outerPosition(), window.scaleFactor(), currentMonitor(),
      window.isMinimized(), window.isMaximized(),
    ]);
    if (!monitor || minimized || maximized) {
      throw new Error(`resize requires a visible normal window and monitor (monitor=${!!monitor}, minimized=${minimized}, maximized=${maximized})`);
    }
    return { inner, outer, position, scale, workArea: monitor.workArea };
  },
  async listen(callback) { return getCurrentWindow().onResized(event => callback(event.payload)); },
  async resize(size) { await invoke("benchmark_resize_window", size); },
};
const equalSize = (a: Size, b: Size) => a.width === b.width && a.height === b.height;
const sameDisplay = (a: ResizeGeometry, b: ResizeGeometry) => a.scale === b.scale &&
  a.position.x === b.position.x && a.position.y === b.position.y &&
  a.workArea.position.x === b.workArea.position.x && a.workArea.position.y === b.workArea.position.y &&
  equalSize(a.workArea.size, b.workArea.size);
const bounded = async <T,>(pending: Promise<T>, label: string): Promise<T> => {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([pending, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out`)), 10_000);
    })]);
  } finally { clearTimeout(timer); }
};

export const desktopResizeTargets = (geometry: ResizeGeometry): [Size, Size] => {
  const { scale, workArea, position, outer, inner } = geometry;
  if (!Number.isFinite(scale) || scale <= 0 || position.x < workArea.position.x || position.y < workArea.position.y) {
    throw new Error("resize window is outside its monitor work area");
  }
  const width = Math.floor(workArea.position.x + workArea.size.width - position.x - (outer.width - inner.width));
  const height = Math.floor(workArea.position.y + workArea.size.height - position.y - (outer.height - inner.height));
  const small = { width: Math.ceil(1080 * scale), height: Math.ceil(680 * scale) };
  const large = { width: Math.min(Math.floor(1440 * scale), width), height: Math.min(Math.floor(900 * scale), height) };
  if (large.width < small.width + 100 * scale || large.height < small.height + 50 * scale) {
    throw new Error("monitor work area cannot fit two supported resize sizes");
  }
  return [small, large];
};

export type BenchmarkResizeSubject = Pick<PlaybackController, "snapshot" | "play" | "pause" | "subscribeFrames"> & {
  readMediaIdentity(): Readonly<{ elementToken: object; connected: boolean; currentSrc: string }>;
};
type ResizeObserver = Pick<ReplayBenchmarkObserver, "emit" | "now" | "nextActionId">;

export const runBenchmarkDesktopResize = async (observer: ResizeObserver, controller: BenchmarkResizeSubject,
  signal: AbortSignal, wait: (ms: number) => Promise<void>, window: ResizeWindow = nativeWindow) => {
  const readGeometry = () => bounded(window.geometry(), "native geometry read");
  const original = await readGeometry();
  const targets = desktopResizeTargets(original);
  const identity = controller.snapshot();
  const media = controller.readMediaIdentity();
  const source = media.currentSrc;
  const generation = identity.generation;
  observer.emit("desktop_resize_started", { original_width: original.inner.width, original_height: original.inner.height,
    scale_factor: original.scale, target_sizes: targets }, { generation, required: true });
  const assertIdentity = () => {
    const state = controller.snapshot();
    const currentMedia = controller.readMediaIdentity();
    if (signal.aborted || !currentMedia.connected || currentMedia.elementToken !== media.elementToken || state.generation !== generation ||
        state.sessionToken !== identity.sessionToken || state.sourceUrl !== identity.sourceUrl ||
        state.mediaId !== identity.mediaId || currentMedia.currentSrc !== source || state.readiness !== "ready" ||
        state.recoveryCount !== identity.recoveryCount) throw new Error("resize changed or disposed the media load");
    return state;
  };
  let active: ((size: Size) => void) | undefined;
  let pendingRequest: Promise<unknown> = Promise.resolve();
  let abandonedListener = false;
  const listener = window.listen(size => active?.(size)).then(unlisten => {
    if (abandonedListener) unlisten();
    return unlisten;
  });
  let unlisten: () => void;
  try { unlisten = await bounded(listener, "native resize listener"); }
  catch (error) { abandonedListener = true; throw error; }
  const resize = async (target: Size, mode: "playing" | "paused") => {
    const actionId = observer.nextActionId("resize");
    let cleanup = () => {};
    const evidence = new Promise<void>((resolve, reject) => {
      let geometryReady = false, finishing = false, ended = false;
      let frame: number | undefined;
      let unsubscribeFrames = () => {};
      const finish = (error?: unknown) => {
        if (ended) return;
        ended = true; cleanup();
        if (error) reject(error); else resolve();
      };
      const abort = () => finish(new Error("desktop resize disposed"));
      const timeout = setTimeout(() => finish(new Error("desktop resize timed out")), 10_000);
      cleanup = () => {
        clearTimeout(timeout); active = undefined; unsubscribeFrames();
        if (frame !== undefined) cancelAnimationFrame(frame);
        signal.removeEventListener("abort", abort);
      };
      const complete = async (authority: string, tick?: number) => {
        if (ended || finishing) return;
        finishing = true;
        const observedAt = observer.now();
        try {
          const current = await readGeometry();
          if (ended) return;
          const state = assertIdentity();
          if (!sameDisplay(original, current) || !equalSize(current.inner, target) ||
              (mode === "playing" && state.media.paused) || (mode === "paused" && !state.media.paused)) {
            throw new Error("resize presentation no longer matches its geometry/mode");
          }
          observer.emit("desktop_resize_completed", { mode, authority, ...target, scale_factor: current.scale,
            media_tick: tick ?? null, same_element: true, same_load: true },
            { generation, actionId, required: true, monotonicMs: observedAt });
          finish();
        } catch (error) { finish(error); }
      };
      const geometry = async (size: Size) => {
        if (ended || geometryReady || !equalSize(size, target)) return;
        try {
          const current = await readGeometry();
          if (ended || geometryReady) return;
          assertIdentity();
          if (!sameDisplay(original, current) || !equalSize(current.inner, target)) throw new Error("native resize geometry mismatch");
          geometryReady = true;
          observer.emit("desktop_resize_geometry", { mode, ...target, scale_factor: current.scale }, { generation, actionId, required: true });
          if (mode === "paused") frame = requestAnimationFrame(() => {
            frame = requestAnimationFrame(() => { frame = undefined; void complete("two-animation-frame-paint-opportunity"); });
          });
        } catch (error) { finish(error); }
      };
      active = size => { void geometry(size); };
      if (mode === "playing") unsubscribeFrames = controller.subscribeFrames((tick, authority) => {
        if (geometryReady && authority === "rvfc") void complete("rvfc", tick);
      });
      signal.addEventListener("abort", abort, { once: true });
      observer.emit("desktop_resize_requested", { mode, ...target, scale_factor: original.scale }, { generation, actionId, required: true });
      pendingRequest = readGeometry().then(current => {
        if (ended) return;
        if (equalSize(current.inner, target)) throw new Error("resize target equals the current size");
        return window.resize(target);
      });
      void pendingRequest.catch(finish);
    });
    try { await Promise.all([evidence, pendingRequest]); } finally { cleanup(); }
  };
  try {
    for (const mode of ["playing", "paused"] as const) {
      if (mode === "playing") {
        const outcome = await controller.play();
        if (outcome.status !== "playing") throw new Error("resize playback could not start");
      } else controller.pause();
      for (let index = 0; index < 8; index++) {
        assertIdentity();
        const current = await readGeometry();
        const target = equalSize(current.inner, targets[0]) ? targets[1] : targets[0];
        await resize(target, mode);
        await wait(250);
      }
    }
  } finally {
    active = undefined;
    // A cancelled viewer cannot supply presentation evidence, but native restoration
    // still runs with its own finite deadline before listener ownership is released.
    try {
      let drainError: unknown;
      try { await bounded(pendingRequest, "pending native resize"); }
      catch (error) { drainError = error; }
      const current = await readGeometry();
      if (!equalSize(current.inner, original.inner)) {
        let timer: ReturnType<typeof setTimeout> | undefined;
        let stopped = false;
        const restore = async () => {
          await window.resize(original.inner);
          while (!stopped) {
            const restored = await readGeometry();
            if (equalSize(restored.inner, original.inner)) return;
            await new Promise(resolve => setTimeout(resolve, 50));
          }
        };
        try {
          await Promise.race([restore(), new Promise<never>((_, reject) => {
            timer = setTimeout(() => reject(new Error("resize restoration timed out")), 10_000);
          })]);
        } finally { stopped = true; clearTimeout(timer); }
      }
      if (drainError) throw drainError;
      observer.emit("desktop_resize_restored", { ...original.inner, scale_factor: original.scale }, { generation, required: true });
    } finally { unlisten(); }
  }
};
