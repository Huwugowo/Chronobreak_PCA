// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ViewerScreen from "./ViewerScreen";
import { loadPlaybackProbe, loadServerMetrics } from "../api";
import type { PlaybackProbe } from "../types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";

vi.mock("../api", () => ({ loadPlaybackProbe: vi.fn(), loadServerMetrics: vi.fn() }));
vi.mock("../playbackDiagnostics", () => ({ createPlaybackDiagnostics: () => ({
  ready: Promise.resolve(false), bind: vi.fn(), dispose: vi.fn(),
}) }));

afterEach(() => { vi.restoreAllMocks(); document.body.replaceChildren(); });

it("marks only the persistent primary video across layout and recovery, then releases it", async () => {
  vi.mocked(loadPlaybackProbe).mockResolvedValue({
    game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-08T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
    local_player_name: null, participants: [], player_timeline: [], kda_timeline: [], events: [],
  } satisfies PlaybackProbe);
  vi.mocked(loadServerMetrics).mockReturnValue(new Promise(() => {}));
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  const loads = vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const probeVideo = document.createElement("video"); document.body.append(probeVideo);
  const host = document.createElement("div"); document.body.append(host);
  const dispose = render(() => <ViewerScreen gameTimestamp="1" onBack={() => {}} onExportClip={() => {}} />, host);
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
