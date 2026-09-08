import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPlaybackController, type PlaybackClock, type PlaybackEvent } from "./playbackController";
import type { MediaObservation, PlaybackMediaAdapter, PlaybackMediaEvent } from "./htmlVideoPlaybackAdapter";
import { type FrameBoundary, type MediaTimelineV2, parseMediaId, type ReplayTick } from "./replayTime";

const tick = (seconds: number) => Math.round(seconds * 48_000_000) as ReplayTick;
const timeline: MediaTimelineV2 = {
  mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
  video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 60n,
    frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
    onePastLastPts: 14_460n, replayEnd: tick(240) },
  audio: { present: true },
};

class FakeVideo implements PlaybackMediaAdapter {
  hasVideoFrameCallback = true;
  state: MediaObservation = { source: "", readyState: 0, networkState: 0, error: null, tick: tick(0),
    paused: true, ended: false, seeking: false, rate: 1, muted: false, volume: 1,
    hidden: false, browserSeconds: 1, durationSeconds: 240, width: 1920, height: 1080 };
  handlers = new Map<PlaybackMediaEvent, Set<() => void>>();
  callbacks = new Map<number, (tick: ReplayTick | null, at: number) => void>();
  allCallbacks: Array<(tick: ReplayTick | null, at: number) => void> = [];
  nextHandle = 0;
  opens: string[] = [];
  seeks: ReplayTick[] = [];
  rates: number[] = [];
  released = false;
  playCalls = 0;
  playResult: (() => Promise<void>) | undefined;
  seekError = false;
  read = () => this.state;
  quality = () => ({ totalFrames: 100, droppedFrames: 2, heapBytes: null });
  update(change: Partial<MediaObservation>) { this.state = { ...this.state, ...change }; }
  open(url: string) {
    this.opens.push(url);
    this.update({ source: url, readyState: 0, seeking: false, error: null, tick: tick(0), paused: true });
    this.emit("loadstart");
  }
  metadata() { this.update({ readyState: 4 }); this.emit("loadedmetadata"); this.emit("canplay"); }
  play = () => {
    this.playCalls++;
    if (this.playResult) return this.playResult();
    this.update({ paused: false }); this.emit("play"); return Promise.resolve();
  };
  pause = () => { this.update({ paused: true }); this.emit("pause"); };
  seek(target: ReplayTick) {
    if (this.seekError) throw new Error("rejected seek");
    this.seeks.push(target); this.update({ tick: target, browserSeconds: 1 + target / 48_000_000, seeking: true });
    this.emit("seeking");
  }
  seeked(target = this.state.tick!) {
    this.update({ tick: target, browserSeconds: 1 + target / 48_000_000, seeking: false }); this.emit("seeked");
  }
  setRate(value: number) { this.rates.push(value); this.update({ rate: value }); this.emit("ratechange"); }
  setMuted(value: boolean) { this.update({ muted: value }); }
  setVolume(value: number) { this.update({ volume: value }); }
  listen(event: PlaybackMediaEvent, handler: () => void) {
    if (!this.handlers.has(event)) this.handlers.set(event, new Set());
    this.handlers.get(event)!.add(handler);
    return () => { this.handlers.get(event)!.delete(handler); };
  }
  emit(event: PlaybackMediaEvent) { [...this.handlers.get(event) ?? []].forEach((handler) => handler()); }
  requestFrame(callback: (tick: ReplayTick | null, at: number) => void) {
    const handle = ++this.nextHandle; this.callbacks.set(handle, callback); this.allCallbacks.push(callback); return handle;
  }
  cancelFrame(handle: number) { this.callbacks.delete(handle); }
  frame(target: ReplayTick) {
    this.update({ tick: target, browserSeconds: 1 + target / 48_000_000 });
    const callbacks = [...this.callbacks.values()]; this.callbacks.clear();
    callbacks.forEach((callback) => callback(target, performance.now()));
  }
  release() { this.released = true; this.update({ source: "", paused: true }); }
}

function setup(rvfc = true) {
  const media = new FakeVideo(); media.hasVideoFrameCallback = rvfc;
  let token = 0;
  const clock: PlaybackClock = { now: () => performance.now(), setTimeout: (callback, ms) => setTimeout(callback, ms),
    clearTimeout: (handle) => clearTimeout(handle), token: () => `session-${++token}` };
  const events: PlaybackEvent[] = [];
  const controller = createPlaybackController(media, { clock, eventSink: (event) => events.push(event) });
  const opening = controller.open({ url: "http://127.0.0.1:123/games/1/video.mp4", mediaId: timeline.mediaId, timeline });
  return { media, controller, opening, events };
}

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe("concrete playback controller", () => {
  it("opens after a current-source readiness barrier; metadata is not presentation", async () => {
    const { media, controller, opening } = setup();
    expect(controller.snapshot().readiness).toBe("loading");
    media.frame(tick(0));
    expect(controller.snapshot().seek.presented).toBeNull();
    media.metadata();
    expect(await opening).toEqual({ status: "ready" });
    expect(controller.snapshot().seek.presented).toBeNull();
    media.frame(tick(0));
    expect(controller.snapshot().seek.presented).toBe(0);
    expect(media.opens[0]).toContain("qb_playback_session=session-1");
  });

  it("keeps preview/end boundary, dispatched frame, seeked and RVFC as distinct facts", async () => {
    const { media, controller } = setup(); media.metadata();
    const result = controller.seek(tick(240));
    const target = tick(240 - 1 / 60);
    expect(controller.snapshot().seek).toMatchObject({ requestedPreview: tick(240), dispatched: { target }, seekedObservation: null, presented: null });
    media.seeked();
    expect(controller.snapshot().seek).toMatchObject({ seekedObservation: target, presented: null });
    media.frame(target);
    expect(await result).toMatchObject({ status: "presented", target, presented: target });
  });

  it("retains one native seek and latest pending intent with exactly-once supersession", async () => {
    const { media, controller } = setup(); media.metadata();
    const first = controller.seek(tick(10));
    const second = controller.seek(tick(20));
    const latest = controller.seek(tick(30));
    expect(await first).toMatchObject({ status: "superseded" });
    expect(await second).toMatchObject({ status: "superseded" });
    expect(media.seeks).toEqual([tick(10)]);
    expect(controller.snapshot().queue).toBe("1 ACTIVE + LATEST");
    media.seeked(tick(10));
    vi.advanceTimersByTime(99); expect(media.seeks).toHaveLength(1);
    vi.advanceTimersByTime(1); expect(media.seeks).toEqual([tick(10), tick(30)]);
    media.seeked(tick(30)); media.frame(tick(30));
    expect(await latest).toMatchObject({ status: "presented", target: tick(30) });
  });

  it("deduplicates within half a frame without fabricating presentation", async () => {
    const { media, controller } = setup(); media.metadata(); media.update({ tick: tick(10 + 1 / 240) });
    expect(await controller.seek(tick(10))).toMatchObject({ status: "deduplicated" });
    expect(media.seeks).toHaveLength(0); expect(controller.snapshot().seek.presented).toBeNull();
  });

  it("accepts presentation before seeked without releasing the native slot early", async () => {
    const { media, controller } = setup(); media.metadata();
    const result = controller.seek(tick(10)); media.frame(tick(10));
    expect(await result).toMatchObject({ status: "presented" });
    expect(controller.snapshot().owned.activeSeeks).toBe(1);
    media.seeked();
    expect(controller.snapshot().seek.seekedObservation).toBe(tick(10));
    expect(controller.snapshot().owned.activeSeeks).toBe(0);
    vi.advanceTimersByTime(1_500); expect(media.opens).toHaveLength(1);
  });

  it("rejects old callbacks from both superseded intent and old media generation", () => {
    const { media, controller } = setup(); media.metadata();
    const old = media.allCallbacks[media.allCallbacks.length - 1]!;
    void controller.seek(tick(20));
    old(tick(1), performance.now()); expect(controller.snapshot().seek.presented).toBeNull();
    const oldSeek = media.allCallbacks[media.allCallbacks.length - 1]!;
    controller.retry(); media.metadata();
    oldSeek(tick(20), performance.now()); expect(controller.snapshot().seek.presented).toBeNull();
  });

  it("has a native seek deadline and a separate deadline after seeked without RVFC", async () => {
    const { media, controller } = setup(); media.metadata();
    const result = controller.seek(tick(10));
    vi.advanceTimersByTime(1_400); media.seeked();
    vi.advanceTimersByTime(1_499); expect(media.opens).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(await result).toMatchObject({ status: "failed", reason: "First presentation timed out after 1500 ms" });
    expect(media.opens).toHaveLength(2);
    expect(controller.snapshot().readiness).toBe("recovering");
  });

  it("bounds slow metadata failures by both rolling and consecutive budgets", async () => {
    const { media, controller, opening } = setup();
    vi.advanceTimersByTime(15_000);
    expect(await opening).toMatchObject({ status: "failed" });
    expect(media.opens).toHaveLength(3);
    expect(controller.snapshot().readiness).toBe("degraded");
    vi.advanceTimersByTime(60_000); expect(media.opens).toHaveLength(3);
    expect(controller.snapshot().owned).toMatchObject({ timers: 0, listeners: 0, frames: 0 });
    controller.retry(); expect(media.opens).toHaveLength(4);
  });

  it("retains the explicit target when its native deadline starts recovery", async () => {
    const { media, controller } = setup(); media.metadata(); media.frame(tick(2));
    const result = controller.seek(tick(70));
    vi.advanceTimersByTime(1_500);
    expect(await result).toMatchObject({ status: "failed" });
    media.metadata(); expect(media.seeks[media.seeks.length - 1]).toBe(tick(70));
  });

  it("restores the newest selected rate when a paused nudge completes", async () => {
    const { media, controller } = setup(); media.metadata();
    const result = controller.seek(tick(10)); media.seeked();
    expect(media.state.rate).toBe(1 / 16);
    controller.setRate(4); controller.setVolume(0.3); controller.setMuted(true);
    media.frame(tick(10));
    expect(await result).toMatchObject({ status: "presented" });
    expect(media.state).toMatchObject({ rate: 4, paused: true, volume: 0.3, muted: true });
    expect(controller.snapshot().desiredPlaying).toBe(false);
  });

  it("limits nudge duration to 750ms even when native play never resolves", () => {
    const { media, controller } = setup(); media.metadata(); media.playResult = () => new Promise(() => {});
    void controller.seek(tick(10)); media.seeked();
    vi.advanceTimersByTime(750);
    expect(media.state).toMatchObject({ paused: true, rate: 1 });
  });

  it("never starts a nudge when the temporary rate assignment is rejected", () => {
    const { media, controller } = setup(); media.metadata();
    const setRate = media.setRate.bind(media);
    media.setRate = (rate) => { if (rate === 1 / 16) throw new Error("rate rejected"); setRate(rate); };
    void controller.seek(tick(10)); media.seeked(); vi.advanceTimersByTime(750);
    expect(media.playCalls).toBe(0); expect(media.state.paused).toBe(true);
  });

  it("reports native and synchronous open failures as failures rather than supersession", async () => {
    const { media, controller, opening } = setup();
    media.update({ error: { code: 3, message: "decode failed" } }); media.emit("error");
    expect(await opening).toMatchObject({ status: "failed", reason: "Media error (3): decode failed" });
    media.open = () => { throw new Error("source rejected"); };
    const reopened = controller.open({ url: "http://127.0.0.1/video", mediaId: timeline.mediaId, timeline });
    expect(await reopened).toMatchObject({ status: "failed", reason: "Media open failed: source rejected" });
  });

  it("calls native play synchronously and reports activation denial without reload", async () => {
    const { media, controller } = setup(); media.metadata();
    media.playResult = () => Promise.reject(Object.assign(new Error("gesture needed"), { name: "NotAllowedError" }));
    const result = controller.play();
    expect(media.playCalls).toBe(1);
    expect(await result).toMatchObject({ status: "activation-required" });
    expect(controller.snapshot().activationRequired).toBe(true);
    expect(media.opens).toHaveLength(1);
  });

  it("settles pending play on pause and ignores its late completion", async () => {
    const { media, controller } = setup(); media.metadata();
    let resolve!: () => void;
    media.playResult = () => new Promise((done) => { resolve = done; });
    const result = controller.play(); controller.pause(); resolve();
    expect(await result).toMatchObject({ status: "superseded" });
    expect(controller.snapshot().desiredPlaying).toBe(false);
    expect(media.opens).toHaveLength(1);
  });

  it("does not recover intentional AbortError", async () => {
    const { media, controller } = setup(); media.metadata();
    media.playResult = () => Promise.reject(Object.assign(new Error("pause"), { name: "AbortError" }));
    expect(await controller.play()).toMatchObject({ status: "superseded" });
    expect(media.opens).toHaveLength(1);
  });

  it("reports approximate fallback without settling authoritative frame success", async () => {
    const { media, controller } = setup(false); media.metadata();
    const frames: ReplayTick[] = []; controller.subscribeFrames((value) => frames.push(value));
    const result = controller.seek(tick(10)); media.seeked(); media.emit("timeupdate");
    expect(await result).toMatchObject({ status: "unavailable" });
    expect(frames).toContain(tick(10)); expect(controller.snapshot().seek.presented).toBeNull();
    expect(controller.snapshot().authority).toBe("media-clock-approximate");
  });

  it("coalesces loop and ended and gives explicit navigation precedence", async () => {
    const { media, controller } = setup(); media.metadata();
    controller.setLoopRange({ mediaId: timeline.mediaId, startFrame: 600 as FrameBoundary, endFrameExclusive: 720 as FrameBoundary });
    await controller.play(); media.frame(tick(12)); media.emit("ended");
    expect(media.seeks).toEqual([tick(10)]);
    const explicit = controller.seek(tick(11)); media.emit("ended");
    media.seeked(tick(10)); vi.advanceTimersByTime(100); media.seeked(tick(11)); media.frame(tick(11));
    expect(await explicit).toMatchObject({ status: "presented", target: tick(11) });
    expect(media.seeks).toEqual([tick(10), tick(11)]);
  });

  it("recovers on the same adapter, rotates identity and restores latest target/preferences", async () => {
    const { media, controller } = setup(); media.metadata();
    controller.setRate(2); controller.setVolume(0.4); controller.setMuted(true);
    await controller.play(); void controller.seek(tick(10)); void controller.seek(tick(20));
    controller.retry();
    expect(media.opens).toHaveLength(2); expect(media.opens[0]).not.toBe(media.opens[1]);
    media.metadata(); expect(media.seeks[media.seeks.length - 1]).toBe(tick(20));
    media.seeked(tick(20)); media.frame(tick(20));
    expect(media.state).toMatchObject({ rate: 2, muted: true, volume: 0.4 });
    expect(controller.snapshot().desiredPlaying).toBe(true);
  });

  it("settles pending operations and releases every callback/listener/timer on disposal", async () => {
    const { media, controller, opening } = setup();
    const seeking = controller.seek(tick(10));
    const old = media.allCallbacks[media.allCallbacks.length - 1]!;
    controller.dispose(); controller.dispose(); old(tick(10), performance.now());
    expect(await opening).toMatchObject({ status: "superseded" });
    expect(await seeking).toMatchObject({ status: "superseded" });
    expect(media.released).toBe(true); expect(media.callbacks.size).toBe(0);
    expect([...media.handlers.values()].every((set) => set.size === 0)).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
    expect(controller.snapshot().readiness).toBe("disposed");
  });

  it("retains a bounded diagnostics history", () => {
    const { media, controller } = setup(); media.metadata();
    for (let i = 0; i < 200; i++) media.emit("canplay");
    expect(controller.snapshot().diagnostics).toHaveLength(50);
  });
});
