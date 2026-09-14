// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import ClipExporterScreen from "./ClipExporterScreen";
import {
  chooseMusicFile,
  loadBuiltInMusic,
  loadPlaybackProbe,
  prepareImportedMusicPreview,
  releaseImportedMusicPreview,
} from "../api";
import type { PlaybackProbe } from "../types";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "../replayTime";

vi.mock("../api", () => ({
  chooseMusicFile: vi.fn(),
  exportClip: vi.fn(),
  loadBuiltInMusic: vi.fn(),
  loadPlaybackProbe: vi.fn(),
  prepareImportedMusicPreview: vi.fn(),
  releaseImportedMusicPreview: vi.fn(),
}));

afterEach(() => { vi.restoreAllMocks(); document.body.replaceChildren(); });

it("retries the same imported file after preparation fails and releases the successful preview", async () => {
  const probe: PlaybackProbe = {
    game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-14T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
    local_player_name: null, participants: [], player_timeline: [], kda_timeline: [], events: [],
  };
  const path = "C:\\Music\\same.wav";
  const preview = { url: "http://127.0.0.1:123/music-preview/retry", token: "retry" };
  vi.mocked(loadPlaybackProbe).mockResolvedValue(probe);
  vi.mocked(loadBuiltInMusic).mockResolvedValue([]);
  vi.mocked(chooseMusicFile).mockResolvedValue(path);
  vi.mocked(prepareImportedMusicPreview)
    .mockRejectedValueOnce(new Error("Temporary preview failure"))
    .mockResolvedValueOnce(preview);
  vi.mocked(releaseImportedMusicPreview).mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div");
  document.body.append(host);
  const dispose = render(() => <ClipExporterScreen
    draft={{ gameTimestamp: "1", mediaId: probe.media_timeline!.mediaId,
      startFrame: 0 as FrameBoundary, endFrameExclusive: 60 as FrameBoundary }}
    outputPath="C:\\output" onBack={() => {}} onExported={() => {}}
    onOpenClips={() => {}} onOpenFolder={() => {}}
  />, host);
  let audio: HTMLAudioElement | null = null;
  try {
    const browse = () => [...host.querySelectorAll("button")].find((button) => button.textContent === "BROWSE");
    await vi.waitFor(() => expect(browse()).toBeDefined());
    audio = host.querySelector("audio");
    expect(audio).not.toBeNull();
    browse()!.click();
    await vi.waitFor(() => expect(host.textContent).toContain("Temporary preview failure"));
    expect(prepareImportedMusicPreview).toHaveBeenCalledExactlyOnceWith(path);
    expect(audio!.hasAttribute("src")).toBe(false);

    // Cancelling the picker preserves the failed selection without retrying it.
    vi.mocked(chooseMusicFile).mockResolvedValueOnce(null);
    browse()!.click();
    await vi.waitFor(() => expect(chooseMusicFile).toHaveBeenCalledTimes(2));
    expect(prepareImportedMusicPreview).toHaveBeenCalledTimes(1);
    expect(host.textContent).toContain("Temporary preview failure");

    browse()!.click();
    await vi.waitFor(() => expect(prepareImportedMusicPreview).toHaveBeenCalledTimes(2));
    expect(prepareImportedMusicPreview).toHaveBeenNthCalledWith(2, path);
    await vi.waitFor(() => expect(audio!.getAttribute("src")).toBe(preview.url));
    expect(host.textContent).not.toContain("Temporary preview failure");
    expect(releaseImportedMusicPreview).not.toHaveBeenCalled();
  } finally { dispose(); }
  await vi.waitFor(() => expect(releaseImportedMusicPreview).toHaveBeenCalledExactlyOnceWith(preview.token));
  expect(audio!.hasAttribute("src")).toBe(false);
});
