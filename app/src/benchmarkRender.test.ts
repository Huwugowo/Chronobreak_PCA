// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { benchmarkRenderObservation, renderedReplayImages, waitForBenchmarkRenderedReplay, type BenchmarkRenderedState, type RenderSubjectState } from "./benchmarkRender";
import type { ReplayBenchmarkObserver } from "./benchmark";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); document.body.replaceChildren(); });
const fixture = () => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => setTimeout(() => callback(0), 16));
  vi.stubGlobal("cancelAnimationFrame", clearTimeout);
  const root = document.createElement("div");
  root.innerHTML = '<div data-testid="champion-filter-fullscreen"></div><span data-event-icon="kill"></span>';
  document.body.append(root);
  const state: BenchmarkRenderedState = { generation: 2, mediaTick: 52_433 * 48_000, sourceEpoch: 0, sourceToken: "fixture-source", eventCount: 107, samplePlayerCount: 10,
    players: Array.from({ length: 10 }, (_, index) => ({ name: `fixture-${index}`, level: index + 1,
      portraitUrl: `http://assets.test/champion/${index}`, spells: ["Flash", "Unleashed Teleport"] })) };
  const decode = vi.fn(async () => {});
  for (const player of state.players) {
    const button = document.createElement("button"); button.dataset.testid = "champion-filter-option"; button.dataset.playerName = player.name;
    button.innerHTML = `<span data-testid="champion-level">${player.level}</span>`;
    for (const src of [player.portraitUrl, "http://assets.test/spell/Flash", "http://assets.test/spell/Unleashed%20Teleport"]) {
      const image = document.createElement("img"); image.src = src;
      Object.defineProperties(image, { decode: { value: decode }, complete: { value: true }, naturalWidth: { value: 64 }, naturalHeight: { value: 64 } });
      button.append(image);
    }
    root.firstElementChild!.append(button);
  }
  const observer = { emit: vi.fn() };
  return { root, state, decode, observer, signal: new AbortController() };
};

it("requires actual matching roster/levels, timeline markers and decoded supported images", async () => {
  const f = fixture();
  const pending = waitForBenchmarkRenderedReplay(f.observer as unknown as ReplayBenchmarkObserver, f.signal.signal, () => f.state, f.root);
  await vi.advanceTimersByTimeAsync(40); await pending;
  expect(f.decode).toHaveBeenCalledTimes(20);
  expect(f.observer.emit).toHaveBeenCalledWith("replay_render_ready", expect.objectContaining({ participant_count: 10,
    decoded_image_count: 20, decoded_url_count: 11 }), { generation: 2, required: true });
  for (const player of f.state.players) expect(JSON.stringify(f.observer.emit.mock.calls)).not.toContain(player.name);
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["portrait", "level", "marker", "participant", "projected-participant"])("rejects missing rendered %s", what => {
  const f = fixture();
  if (what === "portrait") f.root.querySelector("img")!.remove();
  if (what === "level") f.root.querySelector('[data-testid="champion-level"]')!.textContent = "99";
  if (what === "marker") f.root.querySelector("[data-event-icon]")!.remove();
  if (what === "participant") f.root.querySelector("button")!.remove();
  if (what === "projected-participant") f.state = { ...f.state, samplePlayerCount: 10, players: f.state.players.slice(0, 9) };
  expect(renderedReplayImages(f.root, f.state)).toBeNull();
});

it.each(["decode", "generation", "abort", "timeout"])("fails closed and cleans up on %s", async outcome => {
  const f = fixture();
  if (outcome === "decode") f.decode.mockRejectedValue(new Error("image failed"));
  if (outcome === "timeout") f.root.querySelector("[data-event-icon]")!.remove();
  const pending = waitForBenchmarkRenderedReplay(f.observer as unknown as ReplayBenchmarkObserver, f.signal.signal, () => f.state, f.root);
  const rejected = expect(pending).rejects.toThrow();
  if (outcome === "generation") { await vi.advanceTimersByTimeAsync(1); f.state = { ...f.state, generation: 3 }; }
  if (outcome === "abort") f.signal.abort();
  await vi.advanceTimersByTimeAsync(outcome === "timeout" ? 10_001 : 40); await rejected;
  expect(f.observer.emit.mock.calls.some(([kind]) => kind === "replay_render_ready")).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});

it("does not allocate paint frames after cancellation while image decoding is pending", async () => {
  const f = fixture(); let decoded!: () => void;
  f.decode.mockReturnValue(new Promise<void>(resolve => { decoded = resolve; }));
  const pending = waitForBenchmarkRenderedReplay(f.observer, f.signal.signal, () => f.state, f.root);
  const rejected = expect(pending).rejects.toThrow("disposed");
  f.signal.abort(); await rejected; decoded();
  for (let i = 0; i < 10; i++) await Promise.resolve();
  expect(vi.getTimerCount()).toBe(0);
  expect(f.observer.emit.mock.calls.some(([kind]) => kind === "replay_render_ready")).toBe(false);
});

const observation = () => {
  const f = fixture();
  const source = { media_id: "media", run_id: "run", epoch: 0 } as unknown as NonNullable<RenderSubjectState["activeSource"]>;
  let state: RenderSubjectState = { elementToken: {}, connected: true, assetsAvailable: true, generation: 2,
    sessionToken: "load", sourceUrl: "fixture", recoveryCount: 0, paused: true,
    activeSource: source, overlaySource: source, windowSource: source, presentedTick: f.state.mediaTick as RenderSubjectState["presentedTick"], rendered: f.state };
  let root: Element | null = f.root;
  const subject = { root: () => root, read: () => state };
  return { ...f, subject, change: (patch: Partial<RenderSubjectState>) => { state = { ...state, ...patch }; },
    replaceRoot: () => { root = document.createElement("div"); } };
};
it.each(["assetsAvailable", "connected", "activeSource"] as const)("rejects missing %s at render action entry", key => {
  const f = observation(); f.change({ [key]: key === "activeSource" ? null : false });
  expect(() => benchmarkRenderObservation(f.subject)).toThrow(/unavailable/);
});
it.each(["elementToken", "connected", "generation", "sessionToken", "sourceUrl", "recoveryCount", "paused", "activeSource", "root"])(
  "rejects a changed render identity (%s) after admission and decode", async key => {
    const f = observation(); const { root, read } = benchmarkRenderObservation(f.subject);
    const pending = waitForBenchmarkRenderedReplay(f.observer, f.signal.signal, read, root);
    const rejected = expect(pending).rejects.toThrow(/changed/);
    if (key === "root") f.replaceRoot();
    else f.change({ [key]: ({ elementToken: {}, connected: false, generation: 3, sessionToken: "new", sourceUrl: "new",
      recoveryCount: 1, paused: false, activeSource: { ...f.subject.read().activeSource!, epoch: 1 } } as Record<string, unknown>)[key] });
    await vi.advanceTimersByTimeAsync(40); await rejected;
    expect(vi.getTimerCount()).toBe(0);
    expect(f.observer.emit.mock.calls.some(([kind]) => kind === "replay_render_ready")).toBe(false);
  });
it("waits for real source agreement, ten participants and the fixed presented tick", () => {
  const f = observation(); const { read } = benchmarkRenderObservation(f.subject);
  f.change({ overlaySource: null }); expect(read()).toBeNull();
  f.change({ overlaySource: f.subject.read().activeSource, presentedTick: 0 as NonNullable<RenderSubjectState["presentedTick"]> }); expect(read()).toBeNull();
  f.change({ presentedTick: f.state.mediaTick as RenderSubjectState["presentedTick"], rendered: { ...f.state, samplePlayerCount: 9 } }); expect(read()).toBeNull();
  f.change({ rendered: f.state }); expect(read()).toBe(f.state);
});
