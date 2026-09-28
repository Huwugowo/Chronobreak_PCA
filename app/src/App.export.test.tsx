// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { ComponentProps } from "solid-js";
import App from "./App";
import type LibraryScreen from "./components/LibraryScreen";
import type ViewerScreen from "./components/ViewerScreen";
import type { PlaybackProbe } from "./types";
import { exportClip, loadPlaybackProbe } from "./api";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "./replayTime";

const harness = vi.hoisted(() => ({ revision: 0, fail: false, probe: null as PlaybackProbe | null,
  refreshFailure: false, missingSource: false,
  holdProbe: false, releaseProbe: null as ((probe: PlaybackProbe) => void) | null }));
vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: vi.fn(async () => null) }));
vi.mock("./api", async original => ({ ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: "A", auto_delete_days: 0 }),
  loadDdragonStatus: () => new Promise(() => {}),
  ensureHevcCapability: () => new Promise(() => {}),
  loadBuiltInMusic: async () => [],
  loadPlaybackProbe: vi.fn(async () => harness.holdProbe
    ? new Promise<PlaybackProbe>(resolve => { harness.releaseProbe = resolve; }) : harness.probe!),
  refreshLibrary: async () => {
    if (harness.refreshFailure) throw new Error("scan fixture failed");
    return { token: `A-${++harness.revision}`, games: harness.missingSource ? [] : [harness.probe!.game], clips: [],
      usage: { games_bytes: 1, clips_bytes: 0, game_count: 1, clip_count: 0 } };
  },
  exportClip: vi.fn(async () => {
    if (harness.fail) { harness.fail = false; throw new Error("encoder fixture failed"); }
    return { outputs: [], elapsed_ms: 10, total_file_size_bytes: 0 };
  }),
}));
vi.mock("./components/LibraryScreen", () => ({ default: (props: ComponentProps<typeof LibraryScreen>) =>
  <button onClick={() => props.onOpenGame("1", props.snapshotOrigin)}>Open recording</button> }));
vi.mock("./components/ViewerScreen", () => ({ default: (props: ComponentProps<typeof ViewerScreen>) =>
  <button onClick={() => props.onExportClip({ gameTimestamp: "1", mediaId: harness.probe!.media_timeline!.mediaId,
    startFrame: 0 as FrameBoundary, endFrameExclusive: 300 as FrameBoundary })}>Edit clip</button> }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren(); localStorage.clear(); vi.clearAllMocks(); vi.restoreAllMocks();
  harness.revision = 0; harness.fail = false; harness.holdProbe = false; harness.releaseProbe = null;
  harness.refreshFailure = false; harness.missingSource = false;
});

const mount = async () => {
  harness.probe = {
    game: { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-27T00:00:00Z",
      kills: 1, deaths: 2, assists: 3, duration_ms: 240_000, summoner_spells: [], keystone_id: null,
      items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true },
    video_url: "http://127.0.0.1:123/games/1/video.mp4",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14_400 as FrameBoundary,
        onePastLastPts: 14_400n, replayEnd: 11_520_000_000 as ReplayTick }, audio: { present: true } },
    local_player_name: null, participants: [], player_timeline: [], kda_timeline: [], events: [],
  };
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  const button = (text: string) => [...host.querySelectorAll("button")].find(item => item.textContent?.startsWith(text));
  await vi.waitFor(() => expect(button("Open recording")).toBeDefined());
  button("Open recording")!.click(); button("Edit clip")!.click();
  await vi.waitFor(() => expect(button("EXPORT ")).toBeDefined());
  return { host, button };
};

it.each([false, true])("exports again with refreshed admission and unchanged draft/options (first failure=%s)", async failure => {
  const { host, button } = await mount();
  harness.fail = failure;
  host.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  button("EXPORT ")!.click();
  await vi.waitFor(() => expect(loadPlaybackProbe).toHaveBeenCalledTimes(2));
  await vi.waitFor(() => expect(button(failure ? "TRY AGAIN" : "EXPORT ")?.disabled).toBe(false));
  button(failure ? "TRY AGAIN" : "EXPORT ")!.click();
  await vi.waitFor(() => expect(exportClip).toHaveBeenCalledTimes(2));
  const calls = vi.mocked(exportClip).mock.calls;
  expect(calls[0][2]).toBe("A-1"); expect(calls[1][2]).toBe("A-2");
  expect(calls[1][0]).toEqual(calls[0][0]);
  expect(calls[1][0].presets).toEqual(["discord", "horizontal"]);
  expect(calls[1][0].start_frame).toBe("0"); expect(calls[1][0].end_frame_exclusive).toBe("300");
});

it("blocks retry during revalidation and rejects replacement media without rewriting the draft", async () => {
  const { host, button } = await mount();
  harness.holdProbe = true;
  const first = button("EXPORT ")!;
  first.click();
  await vi.waitFor(() => expect(harness.releaseProbe).not.toBeNull());
  expect(button("EXPORT ")).toBeUndefined();
  first.click();
  expect(exportClip).toHaveBeenCalledTimes(1);
  const original = harness.probe!;
  harness.releaseProbe!({ ...original, media_timeline: { ...original.media_timeline!,
    mediaId: parseMediaId("aaaaaaaa-2222-4333-8444-555555555555") } });
  await vi.waitFor(() => expect(host.textContent).toContain("The recording changed."));
  expect(button("EXPORT ")).toBeUndefined();
  expect(exportClip).toHaveBeenCalledTimes(1);
});

it.each(["refresh failure", "missing source"])("disables stale export retry after %s with a recovery message", async outcome => {
  const { host, button } = await mount();
  harness.fail = true;
  harness.refreshFailure = outcome === "refresh failure";
  harness.missingSource = outcome === "missing source";
  button("EXPORT ")!.click();
  await vi.waitFor(() => expect(button("TRY AGAIN")?.disabled).toBe(true));
  expect(host.textContent).toContain("Return to games");
  expect(host.textContent).toContain(outcome === "refresh failure" ? "scan fixture failed" : "source recording changed");
  button("TRY AGAIN")!.click();
  expect(exportClip).toHaveBeenCalledTimes(1);
  expect(loadPlaybackProbe).toHaveBeenCalledTimes(1);
});
