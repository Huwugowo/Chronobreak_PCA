// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ViewerScreen from "./ViewerScreen";
import { loadPlaybackProbe, loadReplayDescriptor, loadServerMetrics } from "../api";
import type { PlaybackProbe } from "../types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";
import { buildScenarioActions, emitReplayBenchmarkEvent, replayBenchmarkObserver } from "../benchmark";

vi.mock("../api", async original => ({ ...await original<typeof import("../api")>(),
  loadPlaybackProbe: vi.fn(), loadReplayDescriptor: vi.fn(), loadServerMetrics: vi.fn() }));
vi.mock("../playbackDiagnostics", () => ({ createPlaybackDiagnostics: () => ({
  ready: Promise.resolve(false), bind: vi.fn(), dispose: vi.fn(),
}) }));
vi.mock("../benchmark", async original => ({ ...await original<typeof import("../benchmark")>(),
  emitReplayBenchmarkEvent: vi.fn(), replayBenchmarkObserver: vi.fn(() => null),
  buildScenarioActions: vi.fn() }));

afterEach(() => { vi.restoreAllMocks(); vi.mocked(emitReplayBenchmarkEvent).mockClear(); vi.mocked(replayBenchmarkObserver).mockReset().mockReturnValue(null); vi.mocked(buildScenarioActions).mockReset(); document.body.replaceChildren(); });

const replayProbe = (): PlaybackProbe => ({
    game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-08T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
    local_player_name: null, participants: [], player_timeline: [], kda_timeline: [], events: [],
});

it("marks only the persistent primary video across layout and recovery, then releases it", async () => {
  const probe = replayProbe();
  vi.mocked(loadPlaybackProbe).mockResolvedValue(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  const loads = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const probeVideo = document.createElement("video"); document.body.append(probeVideo);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelectorAll("video")).toHaveLength(1));
    const primary = host.querySelector("video")!;
    expect(primary.closest('[data-testid="video-frame"]')).not.toBeNull();
    expect([...document.querySelectorAll('[data-qb-primary-playback="true"]')]).toEqual([primary]);
    expect(probeVideo.hasAttribute("data-qb-primary-playback")).toBe(false);
    const source = primary.src;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 },
      error: { get: () => ({ code: 3, message: "fixture decode error" }) } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    const selectSpeed = (value: string) => {
      const select = host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!;
      expect([...select.options].map((option) => option.value)).toEqual(["0.25", "0.5", "1", "2", "4", "8"]);
      select.value = value; select.dispatchEvent(new Event("change", { bubbles: true }));
    };
    selectSpeed("4");
    host.querySelector<HTMLButtonElement>('[aria-label="Mute replay"]')!.click();
    const volume = host.querySelector<HTMLInputElement>('[aria-label="Replay volume"]')!;
    volume.value = "37"; volume.dispatchEvent(new Event("input", { bubbles: true }));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(host.querySelector('[data-testid="video-frame"]')?.getAttribute("data-fullscreen")).toBe("true");
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!.value).toBe("4");
    expect(host.querySelector<HTMLInputElement>('[aria-label="Replay volume"]')!.value).toBe("37");
    expect(host.querySelector('[aria-label="Mute replay"]')).not.toBeNull();
    primary.dispatchEvent(new Event("error"));
    expect(primary.src).not.toBe(source);
    expect(loads).toHaveBeenCalledTimes(2);
    expect(primary.playbackRate).toBe(4); expect(primary.muted).toBe(false); expect(primary.volume).toBe(0.37);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(host.querySelector("video")).toBe(primary);
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!.value).toBe("4");
    expect([...document.querySelectorAll('[data-qb-primary-playback="true"]')]).toEqual([primary]);
  } finally { dispose(); }
  expect(host.querySelector("video")).toBeNull();
  expect(document.querySelector('[data-qb-primary-playback="true"]')).toBeNull();
});

it.each([false, true])("keeps descriptor playback and clip state while details are held or fail (failure=%s)", async failure => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ width: 1000, height: 50, left: 0, right: 1000, top: 0, bottom: 50, x: 0, y: 0, toJSON: () => ({}) });
  const probe = replayProbe();
  probe.events = [{ event_type: "ChampionKill", game_tick: "1000000", replay_tick: 48_000_000 as ReplayTick,
    killer: "player", victim: "enemy", assisters: [], dragon_type: null, kill_streak: null,
    acer: null, acing_team: null, turret: null, inhibitor: null, result: null, relation: "ally" }];
  probe.participants = [{ summoner_name: "player", champion: "Ahri", relation: "ally" }];
  let resolveDetails!: (value: PlaybackProbe) => void;
  let rejectDetails!: (reason: Error) => void;
  const pending = new Promise<PlaybackProbe>((resolve, reject) => { resolveDetails = resolve; rejectDetails = reject; });
  vi.mocked(loadPlaybackProbe).mockReset().mockReturnValueOnce(pending).mockResolvedValue(probe);
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  let paused = true;
  vi.mocked(HTMLMediaElement.prototype.pause).mockImplementation(() => { paused = true; });
  const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(async () => { paused = false; });
  const loads = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const exported = vi.fn();
  const host = document.createElement("div"); document.body.append(host);
  const draft = { gameTimestamp: "1", mediaId: probe.media_timeline.mediaId,
    startFrame: 0 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} initialClipDraft={draft} onBack={() => {}} onExportClip={exported} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")).not.toBeNull());
    const primary = host.querySelector("video")!;
    const source = primary.src;
    const loadCount = loads.mock.calls.length;
    expect(host.querySelector('[data-testid="replay-details-state"]')?.textContent).toContain("Loading replay details");
    expect(host.querySelectorAll('[aria-label^="Seek to"]')).toHaveLength(0);
    expect(host.querySelector('[data-testid="fullscreen-hud"]')).not.toBeNull();
    expect(host.querySelector('[data-testid="champion-filter-option"]')).toBeNull();
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "viewer_mounted")).toBe(true);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "playback_payload_ready")).toBe(false);
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 }, paused: { get: () => paused } });
    primary.dispatchEvent(new Event("loadstart"));
    primary.dispatchEvent(new Event("loadedmetadata"));
    primary.dispatchEvent(new Event("canplay"));
    primary.dispatchEvent(new Event("seeked"));
    const speed = host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!;
    speed.value = "4"; speed.dispatchEvent(new Event("change", { bubbles: true }));
    const preview = host.querySelector<HTMLButtonElement>('[aria-label="Preview selected clip"]')!;
    expect(preview.disabled).toBe(false);
    preview.click();
    primary.dispatchEvent(new Event("seeked"));
    await vi.waitFor(() => expect(play).toHaveBeenCalled());
    const rail = () => host.querySelector<HTMLElement>('[data-testid="replay-timeline"]')!;
    rail().dispatchEvent(new KeyboardEvent("keydown", { key: "+", bubbles: true }));
    const viewport = () => [...host.querySelectorAll('[data-testid="timeline-viewport-readout"] > span')]
      .map(boundary => boundary.textContent).join(" - ");
    const zoomed = viewport();
    expect(zoomed).toContain("0:00 - 3:12");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(host.querySelector('[data-testid="video-frame"]')?.getAttribute("data-fullscreen")).toBe("true");
    expect(viewport()).toBe(zoomed);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    primary.currentTime = 3;
    primary.dispatchEvent(new Event("timeupdate"));
    const playCount = play.mock.calls.length;
    expect(paused).toBe(false);
    await vi.waitFor(() => expect(loadPlaybackProbe).toHaveBeenCalledTimes(1));
    if (failure) {
      rejectDetails(new Error("fixture details failed"));
      await vi.waitFor(() => expect(host.textContent).toContain("Replay details unavailable. Playback remains available."));
      expect(host.querySelector("video")).toBe(primary);
      [...host.querySelectorAll("button")].find(button => button.textContent === "Retry replay details")!.click();
    } else resolveDetails(probe);
    await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
    expect(host.querySelectorAll('[aria-label^="Seek to"]')).toHaveLength(1);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.filter(([kind]) => kind === "playback_payload_ready")).toHaveLength(1);
    expect(host.querySelector("video")).toBe(primary);
    expect(primary.src).toBe(source);
    expect(loads).toHaveBeenCalledTimes(loadCount);
    expect(primary.playbackRate).toBe(4);
    expect(primary.currentTime).toBe(3);
    expect(paused).toBe(false);
    expect(play).toHaveBeenCalledTimes(playCount);
    expect(viewport()).toBe(zoomed);
    expect(host.querySelector('[aria-label="Clip starts at 0:00"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Clip ends at 0:10"]')).not.toBeNull();
    [...host.querySelectorAll("button")].find(button => button.textContent?.includes("Export clip"))!.click();
    expect(exported).toHaveBeenCalledWith(draft);
    host.querySelector<HTMLButtonElement>('[data-testid="champion-filter-option"]')!.click();
    const handle = host.querySelector<HTMLButtonElement>('[aria-label="Clip ends at 0:10"]')!;
    const released = vi.fn();
    Object.assign(handle, { setPointerCapture: vi.fn(), hasPointerCapture: () => true, releasePointerCapture: released });
    const removed = vi.spyOn(handle, "removeEventListener");
    const pointer = (target: HTMLElement, kind: string, clientX: number) => {
      const event = new MouseEvent(kind, { bubbles: true, button: 0, clientX });
      Object.defineProperty(event, "pointerId", { value: 7 });
      target.dispatchEvent(event);
    };
    pointer(handle, "pointerdown", 52);
    pointer(handle, "pointermove", 100);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(handle.isConnected).toBe(true);
    expect(host.querySelector('[aria-label^="Clip ends at"]')).toBe(handle);
    expect(released).not.toHaveBeenCalled();
    pointer(handle, "pointercancel", 100);
    expect(released).toHaveBeenCalledWith(7);
    for (const kind of ["pointermove", "pointerup", "pointercancel", "lostpointercapture"]) {
      expect(removed.mock.calls.some(([event]) => event === kind)).toBe(true);
    }
    const endpoint = () => host.querySelector('[aria-label^="Clip ends at"]')?.getAttribute("aria-label");
    const editedEndpoint = endpoint();
    pointer(handle, "pointermove", 900);
    pointer(handle, "pointerup", 900);
    expect(endpoint()).toBe(editedEndpoint);
    expect(host.querySelector('[data-testid="champion-filter-option"]')?.getAttribute("aria-pressed")).toBe("true");
    expect(viewport()).toBe(zoomed);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(endpoint()).toBe(editedEndpoint);
    expect(host.querySelector('[data-testid="champion-filter-option"]')?.getAttribute("aria-pressed")).toBe("true");
    expect(host.querySelector("video")).toBe(primary);
    expect(primary.src).toBe(source);
    expect(loads).toHaveBeenCalledTimes(loadCount);
  } finally { dispose(); }
});

it("keeps descriptor errors local and retries without requesting details first", async () => {
  const probe = replayProbe();
  vi.mocked(loadPlaybackProbe).mockClear().mockResolvedValue(probe);
  vi.mocked(loadReplayDescriptor).mockRejectedValueOnce(new Error("fixture unavailable"))
    .mockResolvedValueOnce({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url,
      media_timeline: probe.media_timeline });
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.textContent).toContain("Recording could not be opened."));
    expect(host.querySelector("video")).toBeNull();
    expect(loadPlaybackProbe).not.toHaveBeenCalled();
    [...host.querySelectorAll("button")].find(button => button.textContent === "Retry recording")!.click();
    await vi.waitFor(() => expect(host.querySelector("video")).not.toBeNull());
  } finally { dispose(); }
});

it("does not mount a late descriptor or request details after the viewer is disposed", async () => {
  const probe = replayProbe();
  let resolveDescriptor!: (value: import("../types").ReplayDescriptor) => void;
  vi.mocked(loadReplayDescriptor).mockReturnValueOnce(new Promise(resolve => { resolveDescriptor = resolve; }));
  vi.mocked(loadPlaybackProbe).mockClear();
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  dispose();
  resolveDescriptor({ snapshot_token: "snapshot", game_timestamp: "1", video_url: probe.video_url,
    media_timeline: probe.media_timeline });
  await Promise.resolve(); await Promise.resolve();
  expect(host.querySelector("video")).toBeNull();
  expect(loadPlaybackProbe).not.toHaveBeenCalled();
});


it.each(["media-first", "details-first"] as const)("starts the benchmark once after both readiness gates (%s)", async order => {
  const probe = replayProbe();
  let resolveDetails!: (value: PlaybackProbe) => void;
  vi.mocked(loadPlaybackProbe).mockReturnValue(new Promise(resolve => { resolveDetails = resolve; }));
  vi.mocked(loadReplayDescriptor).mockResolvedValue({ snapshot_token: "snapshot", game_timestamp: "1",
    video_url: probe.video_url, media_timeline: probe.media_timeline });
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  const observer = { nextMediaGeneration: () => 1, scenario: () => ({ kind: "cold_open" }),
    emit: vi.fn(), complete: vi.fn() };
  vi.mocked(replayBenchmarkObserver).mockReturnValue(observer as unknown as NonNullable<ReturnType<typeof replayBenchmarkObserver>>);
  vi.mocked(buildScenarioActions).mockReturnValue([]);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const prototype = HTMLVideoElement.prototype;
  const originals = ["requestVideoFrameCallback", "cancelVideoFrameCallback"].map(name => Object.getOwnPropertyDescriptor(prototype, name));
  let frame!: VideoFrameRequestCallback;
  Object.defineProperty(prototype, "requestVideoFrameCallback", { configurable: true, value: (callback: VideoFrameRequestCallback) => { frame = callback; return 1; } });
  Object.defineProperty(prototype, "cancelVideoFrameCallback", { configurable: true, value: () => {} });
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" game={probe.game}
    readReplay={work => work("snapshot")} onBack={() => {}} onExportClip={() => {}} />, host);
  try {
    await vi.waitFor(() => expect(host.querySelector("video")?.src).toContain(probe.video_url));
    const primary = host.querySelector("video")!;
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
    const readyMedia = () => {
      primary.dispatchEvent(new Event("loadstart"));
      primary.dispatchEvent(new Event("loadedmetadata"));
      primary.dispatchEvent(new Event("canplay"));
      expect(frame).toBeTypeOf("function");
      frame(performance.now(), { mediaTime: 0 } as VideoFrameCallbackMetadata);
    };
    if (order === "media-first") {
      readyMedia();
      await Promise.resolve();
      expect(buildScenarioActions).not.toHaveBeenCalled();
      resolveDetails(probe);
    } else {
      resolveDetails(probe);
      await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
      expect(buildScenarioActions).not.toHaveBeenCalled();
      readyMedia();
    }
    await vi.waitFor(() => expect(buildScenarioActions).toHaveBeenCalledTimes(1));
    primary.dispatchEvent(new Event("canplay"));
    frame(performance.now(), { mediaTime: 1 } as VideoFrameCallbackMetadata);
    await Promise.resolve();
    expect(buildScenarioActions).toHaveBeenCalledTimes(1);
  } finally {
    dispose();
    ["requestVideoFrameCallback", "cancelVideoFrameCallback"].forEach((name, index) => {
      const descriptor = originals[index];
      if (descriptor) Object.defineProperty(prototype, name, descriptor);
      else Reflect.deleteProperty(prototype, name);
    });
  }
});
