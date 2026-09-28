import { afterEach, expect, it, vi } from "vitest";
import { emitReplayBenchmarkEvent, replayBenchmarkObserver } from "./benchmark";
import { createViewerBenchmarkActions, type ViewerBenchmarkActions } from "./viewerBenchmarkActions";
import type { PlaybackSnapshot } from "./playbackController";
import { createClipRange, parseMediaId, type FrameBoundary, type MediaTimelineV2, type ReplayTick } from "./replayTime";
import type { ClipRange, ViewerEvent } from "./types";
import { defaultClipRange, replayTickAtFrameBoundary } from "./viewerUtils";

vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  emitReplayBenchmarkEvent: vi.fn(), replayBenchmarkObserver: vi.fn() }));
afterEach(() => { vi.restoreAllMocks(); vi.clearAllMocks(); });

const tick = (seconds: number) => seconds * 48_000_000 as ReplayTick;
const timeline: MediaTimelineV2 = {
  mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
  video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
    frameRate: { numerator: 60n, denominator: 1n }, frameCount: 3600 as FrameBoundary,
    onePastLastPts: 3600n, replayEnd: tick(60) }, audio: { present: true },
};

function setup() {
  let generation = 1;
  let aborted = false;
  let range: ClipRange | null = null;
  const state = { media: { browserSeconds: 1, tick: tick(1) as ReplayTick | null, paused: true, ended: false },
    rate: { selected: 1, applied: 1 }, muted: false, volume: 1 };
  const quality = { totalFrames: 10, droppedFrames: 0, heapBytes: null };
  const controller = {
    // The fake exposes only observations consumed by the action dispatcher.
    snapshot: () => state as PlaybackSnapshot,
    sampleMetrics: () => ({ quality: { ...quality } }) as PlaybackSnapshot,
    pause: vi.fn(() => { state.media.paused = true; return { status: "paused" as const }; }),
    setRate: vi.fn((rate: number) => { state.rate.applied = rate; }),
  };
  const observer = { nextActionId: (kind: string) => `${kind}-1` };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  const context: ViewerBenchmarkActions = {
    getController: () => controller, getGeneration: () => generation, isAborted: () => aborted,
    mediaTimeline: timeline, replayEnd: timeline.video.replayEnd,
    events: () => [], clipRange: () => range,
    setClipRange: next => { range = next; }, setEditingEndpoint: vi.fn(),
    clipRangeForAnchor: (anchor, event) => defaultClipRange(anchor, timeline, [], event),
    clipStartTick: clip => replayTickAtFrameBoundary(clip.startFrame, timeline),
    clipEndTick: clip => replayTickAtFrameBoundary(clip.endFrameExclusive, timeline),
    replayPosition: () => tick(5), clockSource: () => "VIDEO FRAME",
    playNativeVideo: vi.fn(async () => { state.media.paused = false; }),
    seekTo: vi.fn(async () => {}), waitForBenchmark: vi.fn(async () => {}),
    waitForBenchmarkFrames: vi.fn(async () => {}), withBenchmarkTimeout: promise => promise,
    enterFullscreen: vi.fn(), exitFullscreen: vi.fn(), cancelClipMode: () => { range = null; },
  };
  return { context, controller, state, quality,
    generation: (value: number) => { generation = value; }, abort: () => { aborted = true; } };
}

it("routes play/pause through the owner and emits action metadata with the live generation", async () => {
  const f = setup();
  const run = createViewerBenchmarkActions(f.context);
  await run({ kind: "play" });
  expect(f.context.playNativeVideo).toHaveBeenCalledWith(true);
  f.generation(2);
  await run({ kind: "pause" });
  expect(f.controller.pause).toHaveBeenCalledOnce();
  expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.map(([kind]) => kind))
    .toEqual(["play_requested", "play_complete", "pause_requested", "pause_complete"]);
  expect(emitReplayBenchmarkEvent).toHaveBeenLastCalledWith("pause_complete", { media_time_ms: 1000 },
    { generation: 2, actionId: "pause-1", required: true });
});

it.each([false, true])("propagates action failure and suppresses failure telemetry only after abort (%s)", async aborted => {
  const f = setup();
  f.context.playNativeVideo = async () => { if (aborted) f.abort(); throw new Error("native failure"); };
  await expect(createViewerBenchmarkActions(f.context)({ kind: "play" })).rejects.toThrow("native failure");
  expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.filter(([kind]) => kind === "action_failed"))
    .toHaveLength(aborted ? 0 : 1);
});

it("waits and measures steady/rate advancement, rejecting a changed media generation", async () => {
  const f = setup(); let now = 0;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  f.state.media.paused = false;
  f.context.waitForBenchmark = vi.fn(async ms => {
    now += ms; f.state.media.browserSeconds += ms / 1000 * f.state.rate.applied;
    f.quality.totalFrames += 60;
  });
  const run = createViewerBenchmarkActions(f.context);
  await run({ kind: "wait", durationMs: 250 });
  await run({ kind: "stable-playback", durationMs: 1000 });
  expect(emitReplayBenchmarkEvent).toHaveBeenCalledWith("steady_playback",
    expect.objectContaining({ duration_ms: 1000, media_advance_ms: 1000, total_frame_delta: 60 }),
    { generation: 1, required: true });
  await run({ kind: "rate", rate: 2, durationMs: 1000 });
  expect(f.controller.setRate).toHaveBeenCalledWith(2);
  expect(emitReplayBenchmarkEvent).toHaveBeenLastCalledWith("rate_observed",
    expect.objectContaining({ requested_rate: 2, actual_rate: 2, effective_rate: 2 }),
    { generation: 1, actionId: "rate-1", required: true });
  vi.mocked(f.context.waitForBenchmark).mockImplementationOnce(async () => { f.generation(2); });
  await expect(run({ kind: "stable-playback", durationMs: 1000 })).rejects.toThrow("did not advance");
});

it("uses canonical event positions and alternates exact clip endpoints", async () => {
  const f = setup();
  const event: ViewerEvent & { replay_tick: ReplayTick } = { event_type: "ChampionKill", game_tick: "20000000",
    replay_tick: tick(20), killer: null, victim: null, assisters: [], dragon_type: null, kill_streak: null,
    acer: null, acing_team: null, turret: null, inhibitor: null, result: null, relation: "ally" };
  f.context.events = () => [event];
  f.context.setClipRange(createClipRange(timeline, 600 as FrameBoundary, 1800 as FrameBoundary));
  const run = createViewerBenchmarkActions(f.context);
  await run({ kind: "seek", targetMs: 19000, reason: "event-jump" });
  expect(f.context.seekTo).toHaveBeenLastCalledWith(tick(20), { reason: "event-jump" });
  expect(f.context.waitForBenchmark).toHaveBeenCalledWith(100);
  f.context.setClipRange(createClipRange(timeline, 600 as FrameBoundary, 1800 as FrameBoundary));
  await run({ kind: "seek", targetMs: 12000, reason: "endpoint-edit" });
  await run({ kind: "seek", targetMs: 25000, reason: "endpoint-edit" });
  expect(f.context.clipRange()).toMatchObject({ startFrame: 720, endFrameExclusive: 1500 });
  expect(vi.mocked(f.context.setEditingEndpoint).mock.calls.map(([endpoint]) => endpoint))
    .toEqual(["start", null, "end", null]);
});

it("scrubs every target but only awaits the last seek, with bounds and unchanged interval", async () => {
  const f = setup();
  vi.mocked(f.context.seekTo).mockImplementationOnce(() => new Promise(() => {}));
  await createViewerBenchmarkActions(f.context)({ kind: "scrub", targetsMs: [-100, 90000], intervalMs: 25 });
  expect(vi.mocked(f.context.seekTo).mock.calls.map(([target]) => target)).toEqual([tick(0), tick(60)]);
  expect(vi.mocked(f.context.waitForBenchmark).mock.calls).toEqual([[25], [25]]);
  expect(emitReplayBenchmarkEvent).toHaveBeenLastCalledWith("scrub_settled",
    { request_count: 2, media_time_ms: 1000 }, { generation: 1, required: true });
});

it("waits for seek dispatch before fullscreen, then frames before seek settlement", async () => {
  const f = setup(); const order: string[] = [];
  let dispatched!: () => void; let settle!: () => void;
  f.context.seekTo = async (_target, options) => {
    order.push("seek"); dispatched = options.onBenchmarkDispatch!;
    await new Promise<void>(resolve => { settle = resolve; }); order.push("settled");
  };
  f.context.enterFullscreen = required => { expect(required).toBe(true); order.push("layout"); };
  f.context.waitForBenchmarkFrames = async () => { order.push("frames"); };
  const run = createViewerBenchmarkActions(f.context);
  const pending = run({ kind: "fullscreen", enabled: true, seekTargetMs: 10000 });
  expect(order).toEqual(["seek"]);
  dispatched(); await Promise.resolve(); await Promise.resolve();
  expect(order).toEqual(["seek", "layout", "frames"]);
  settle(); await pending;
  expect(order).toEqual(["seek", "layout", "frames", "settled"]);
  await run({ kind: "fullscreen", enabled: false });
  expect(f.context.exitFullscreen).toHaveBeenCalledWith(true);
});

it("uses the viewer's clip policy and fallback position without owning clip lifecycle", async () => {
  const f = setup(); f.state.media.tick = null;
  f.context.clipRangeForAnchor = vi.fn(f.context.clipRangeForAnchor);
  const run = createViewerBenchmarkActions(f.context);
  await run({ kind: "clip-mode", enabled: true });
  expect(f.context.clipRangeForAnchor).toHaveBeenCalledWith(tick(5));
  expect(f.context.clipRange()).not.toBeNull();
  await run({ kind: "clip-mode", enabled: false });
  expect(f.context.clipRange()).toBeNull();
  expect(emitReplayBenchmarkEvent).toHaveBeenLastCalledWith("clip_mode_applied",
    { enabled: false, anchor_ms: null }, { generation: 1, required: true });
});
