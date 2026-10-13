// Install at app/src/components/ViewerScreen.opening.test.tsx in the isolated reference.
// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ViewerScreen from "./ViewerScreen";
import { loadPlaybackProbe } from "../api";
import type { PlaybackProbe } from "../types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";

const harness = vi.hoisted(() => ({ events: [] as { kind: string; generation?: number }[], complete: vi.fn() }));
vi.mock("../api", async original => ({ ...await original<typeof import("../api")>(),
  loadPlaybackProbe: vi.fn(), loadServerMetrics: () => new Promise(() => {}),
}));
vi.mock("../playbackDiagnostics", () => ({ createPlaybackDiagnostics: () => ({
  ready: new Promise(() => {}), bind: vi.fn(), dispose: vi.fn(),
}) }));
vi.mock("../benchmark", async original => ({ ...await original<typeof import("../benchmark")>(),
  emitReplayBenchmarkEvent: (kind: string, _payload: unknown, meta: { generation?: number } = {}) => harness.events.push({ kind, generation: meta.generation }),
  buildScenarioActions: () => [],
  replayBenchmarkObserver: () => ({
    scenario: () => ({ id: "qb010-opening-v1-s-test", kind: "cold_open" }),
    openingCycle: () => ({ cycle: 0, fixture_id: "S", gameTimestamp: "1" }),
    nextMediaGeneration: () => 0,
    emitOpening: (kind: string, _cycle: unknown, meta: { generation?: number } = {}) => harness.events.push({ kind, generation: meta.generation }),
    emit: (kind: string) => harness.events.push({ kind }), complete: harness.complete,
  }),
}));

const probe: PlaybackProbe = {
  game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-08T00:00:00Z",
    kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
    items: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
  video_url: "http://127.0.0.1:123/games/1/video.mp4",
  media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
    video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
      frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
      onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
  local_player_name: null, participants: [], player_timeline: [], kda_timeline: [], events: [],
};

afterEach(() => { vi.restoreAllMocks(); harness.events.length = 0; harness.complete.mockClear(); document.body.replaceChildren(); });

it.each([false, true])("keeps the real full-probe gate and drops disposed results, disposed=%s", async disposed => {
  let resolve!: (value: PlaybackProbe) => void;
  vi.mocked(loadPlaybackProbe).mockReturnValue(new Promise(done => { resolve = done; }));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const prototype = HTMLVideoElement.prototype;
  const names = ["requestVideoFrameCallback", "cancelVideoFrameCallback"];
  const originals = names.map(name => Object.getOwnPropertyDescriptor(prototype, name));
  let frame!: VideoFrameRequestCallback;
  Object.defineProperty(prototype, names[0], { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
  Object.defineProperty(prototype, names[1], { configurable: true, value: () => {} });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    expect(host.querySelector("video")).toBeNull();
    expect(harness.events).toHaveLength(0);
    if (disposed) dispose();
    resolve(probe);
    if (disposed) {
      await Promise.resolve(); await Promise.resolve();
      expect(host.querySelector("video")).toBeNull();
      expect(harness.events).toHaveLength(0);
      return;
    }
    await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
    const primary = host.querySelector("video")!;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    primary.dispatchEvent(new Event("canplay"));
    frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
    const kinds = harness.events.map(event => event.kind);
    expect(kinds.filter(kind => kind.startsWith("opening_"))).toEqual([
      "opening_details_ready", "opening_media_open_requested", "opening_media_open_bound",
    ]);
    expect(kinds.indexOf("opening_details_ready")).toBeLessThan(kinds.indexOf("opening_media_open_requested"));
    expect(kinds.indexOf("opening_media_open_requested")).toBeLessThan(kinds.indexOf("first_presented_frame"));
    const bound = harness.events.find(event => event.kind === "opening_media_open_bound")!;
    expect(bound.generation).toBe(1);
    for (const kind of ["media_loadstart", "metadata_ready", "media_canplay", "first_presented_frame"]) {
      expect(harness.events.find(event => event.kind === kind)?.generation).toBe(bound.generation);
    }
  } finally {
    dispose();
    names.forEach((name, index) => {
      const original = originals[index];
      if (original) Object.defineProperty(prototype, name, original);
      else Reflect.deleteProperty(prototype, name);
    });
  }
});
