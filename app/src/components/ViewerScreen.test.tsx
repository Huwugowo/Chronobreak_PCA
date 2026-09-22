// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ViewerScreen from "./ViewerScreen";
import { loadPlaybackProbe, loadReplayDescriptor, loadServerMetrics } from "../api";
import type { PlaybackProbe } from "../types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";
import { emitReplayBenchmarkEvent } from "../benchmark";

vi.mock("../api", async original => ({ ...await original<typeof import("../api")>(),
  loadPlaybackProbe: vi.fn(), loadReplayDescriptor: vi.fn(), loadServerMetrics: vi.fn() }));
vi.mock("../playbackDiagnostics", () => ({ createPlaybackDiagnostics: () => ({
  ready: Promise.resolve(false), bind: vi.fn(), dispose: vi.fn(),
}) }));
vi.mock("../benchmark", async original => ({ ...await original<typeof import("../benchmark")>(),
  emitReplayBenchmarkEvent: vi.fn() }));

afterEach(() => { vi.restoreAllMocks(); vi.mocked(emitReplayBenchmarkEvent).mockClear(); document.body.replaceChildren(); });

const replayProbe = (): PlaybackProbe => ({
    game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-08T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
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
    expect(host.querySelector('[aria-label="Unmute replay"]')).not.toBeNull();
    primary.dispatchEvent(new Event("error"));
    expect(primary.src).not.toBe(source);
    expect(loads).toHaveBeenCalledTimes(2);
    expect(primary.playbackRate).toBe(4); expect(primary.muted).toBe(true); expect(primary.volume).toBe(0.37);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(host.querySelector("video")).toBe(primary);
    expect(host.querySelector<HTMLSelectElement>('[aria-label="Replay speed"]')!.value).toBe("4");
    expect([...document.querySelectorAll('[data-qb-primary-playback="true"]')]).toEqual([primary]);
  } finally { dispose(); }
  expect(host.querySelector("video")).toBeNull();
  expect(document.querySelector('[data-qb-primary-playback="true"]')).toBeNull();
});

it.each([false, true])("keeps descriptor playback and clip state while details are held or fail (failure=%s)", async failure => {
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
  const play = vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
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
    expect(host.querySelectorAll('[data-event-index]')).toHaveLength(0);
    expect(host.querySelector('[data-testid="current-cs"]')?.textContent).toBe("—");
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "viewer_mounted")).toBe(true);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.some(([kind]) => kind === "playback_payload_ready")).toBe(false);
    Object.defineProperties(primary, { currentSrc: { get: () => primary.src }, readyState: { get: () => 4 } });
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
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "f" }));
    expect(host.querySelector('[data-testid="video-frame"]')?.getAttribute("data-fullscreen")).toBe("true");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await vi.waitFor(() => expect(loadPlaybackProbe).toHaveBeenCalledTimes(1));
    if (failure) {
      rejectDetails(new Error("fixture details failed"));
      await vi.waitFor(() => expect(host.textContent).toContain("Replay details unavailable. Playback remains available."));
      expect(host.querySelector("video")).toBe(primary);
      [...host.querySelectorAll("button")].find(button => button.textContent === "Retry replay details")!.click();
    } else resolveDetails(probe);
    await vi.waitFor(() => expect(host.querySelector('[data-testid="replay-details-state"]')).toBeNull());
    expect(host.querySelectorAll('[data-event-index]')).toHaveLength(1);
    expect(vi.mocked(emitReplayBenchmarkEvent).mock.calls.filter(([kind]) => kind === "playback_payload_ready")).toHaveLength(1);
    expect(host.querySelector("video")).toBe(primary);
    expect(primary.src).toBe(source);
    expect(loads).toHaveBeenCalledTimes(loadCount);
    expect(primary.playbackRate).toBe(4);
    expect(host.querySelector('[aria-label="Clip starts at 0:00"]')).not.toBeNull();
    expect(host.querySelector('[aria-label="Clip ends at 0:10"]')).not.toBeNull();
    [...host.querySelectorAll("button")].find(button => button.textContent?.includes("EXPORT CLIP"))!.click();
    expect(exported).toHaveBeenCalledWith(draft);
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
