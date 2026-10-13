// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import App from "./App";

const harness = vi.hoisted(() => ({
  events: [] as string[],
  scenarioId: "qb010-library-v2-n-cold",
  resolveBatch: null as null | (() => void),
  opening: false,
  videoAvailable: true,
  complete: vi.fn(),
}));
vi.mock("./benchmark", async (original) => ({
  ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: async () => ({
    emit: (kind: string) => harness.events.push(kind),
    complete: harness.complete,
    scenario: () => ({ id: harness.scenarioId, kind: harness.opening ? "cold_open" : "app_idle", trial_id: "1", idle_seconds: 60 }),
    fixtureForScenario: () => ({ id: "S", alias: "S", game_timestamp: "1", codec: "h264" }),
    beginOpeningCycle: () => harness.events.push("opening_requested"),
  }),
}));
vi.mock("./api", async (original) => ({
  ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: "A", auto_delete_days: 0 }),
  loadDdragonStatus: () => harness.opening ? new Promise(() => {}) : Promise.resolve({ state: "offline" }),
  ensureHevcCapability: () => harness.opening ? new Promise(() => {}) : Promise.resolve({ tested: true, supported: false }),
  refreshLibrary: async () => ({ token: "snapshot-1", games: harness.opening ? [{ timestamp: "1", video_available: harness.videoAvailable }] : [],
    clips: [{ filename: "100_200.mp4" }],
    usage: { game_count: 0, clip_count: 1, game_bytes: 0, clip_bytes: 1, total_bytes: 1 } }),
  resolveClipDurations: () => new Promise(resolve => {
    harness.events.push("duration_batch_started");
    harness.resolveBatch = () => resolve({ snapshot_token: "snapshot-1",
      clips: [{ clip_id: "100_200.mp4", duration: { state: "available", duration_ms: 10000 } }] });
  }),
}));
vi.mock("./components/LibraryScreen", () => ({ default: () => <div>Games core</div> }));
vi.mock("./components/ViewerScreen", () => ({ default: () => <div>Opening viewer</div> }));
vi.mock("./components/AppHeader", () => ({ default: (props: { onTab: (tab: string) => void }) => <>
  <button onClick={() => props.onTab("clips")}>Clips</button>
  <button onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren();
  vi.restoreAllMocks();
  harness.events.length = 0;
  harness.resolveBatch = null;
  harness.opening = false;
  harness.videoAvailable = true;
  harness.complete.mockClear();
  localStorage.clear();
});

it.each(["qb010-library-v2-n-cold", "historical-cold"])(
  "keeps %s in the library until historical details finish after paint", async (scenarioId) => {
    harness.scenarioId = scenarioId;
    const frames: FrameRequestCallback[] = [];
    vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => {
      frames.push(callback); return frames.length;
    });
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
    const host = document.createElement("div"); document.body.append(host);
    dispose = render(() => <App />, host);
    await vi.waitFor(() => expect(frames).toHaveLength(1));
    expect(harness.events).not.toContain("library_useful");
    expect(harness.events).not.toContain("scenario_started");
    frames.shift()!(1);
    expect(harness.events).not.toContain("games_library_usable");
    frames.shift()!(2);
    await vi.waitFor(() => expect(harness.resolveBatch).not.toBeNull());
    expect(host.textContent).toContain("Games core");
    expect(harness.events).not.toContain("library_useful");
    expect(harness.events).not.toContain("scenario_started");
    if (scenarioId.startsWith("qb010-library-v2-")) {
      expect(harness.events.filter(kind => kind !== "hevc_capability")).toEqual([
        "library_view_admitted", "games_library_usable", "duration_batch_started",
      ]);
    } else expect(harness.events).not.toContain("games_library_usable");
    harness.resolveBatch!();
    await vi.waitFor(() => expect(harness.events).toContain("scenario_started"));
    expect(harness.events.filter(kind => kind === "library_useful")).toHaveLength(1);
    expect(harness.events.indexOf("library_useful")).toBeLessThan(harness.events.indexOf("scenario_started"));
  },
);

it.each(["stable", "away", "away-back", "unavailable"])("Games paint with optional work held: %s", async state => {
  harness.opening = true;
  harness.videoAvailable = state !== "unavailable";
  harness.scenarioId = "qb010-opening-v1-s-cold";
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(frames).toHaveLength(1));
  expect(harness.events).not.toContain("opening_requested");
  frames.shift()!(1);
  if (state === "away" || state === "away-back") host.querySelectorAll<HTMLButtonElement>("button")[0].click();
  if (state === "away-back") host.querySelectorAll<HTMLButtonElement>("button")[1].click();
  frames.shift()!(2);
  if (state !== "stable") {
    expect(harness.complete).toHaveBeenCalledWith("failed", "Opening Games view changed before paint");
    expect(harness.events).not.toContain("opening_library_usable");
    expect(harness.events).not.toContain("opening_requested");
  } else {
    await vi.waitFor(() => expect(host.textContent).toContain("Opening viewer"));
    expect(harness.events).toEqual(["opening_library_usable", "scenario_started", "opening_requested", "replay_requested"]);
    expect(harness.resolveBatch).toBeNull();
  }
  expect(harness.events).not.toContain("library_useful");
});
