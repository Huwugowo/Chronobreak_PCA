// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import App from "./App";

const harness = vi.hoisted(() => ({
  events: [] as string[],
  scenarioId: "qb010-library-v2-n-cold",
  resolveBatch: null as null | (() => void),
}));
vi.mock("./benchmark", async (original) => ({
  ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: async () => ({
    emit: (kind: string) => harness.events.push(kind),
    complete: vi.fn(),
    scenario: () => ({ id: harness.scenarioId, kind: "app_idle", trial_id: "1", idle_seconds: 60 }),
  }),
}));
vi.mock("./api", async (original) => ({
  ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: "A", auto_delete_days: 0 }),
  loadDdragonStatus: async () => ({ state: "offline" }),
  ensureHevcCapability: async () => ({ tested: true, supported: false }),
  refreshLibrary: async () => ({ token: "snapshot-1", games: [],
    clips: [{ filename: "100_200.mp4" }],
    usage: { game_count: 0, clip_count: 1, game_bytes: 0, clip_bytes: 1, total_bytes: 1 } }),
  resolveClipDurations: () => new Promise(resolve => {
    harness.events.push("duration_batch_started");
    harness.resolveBatch = () => resolve({ snapshot_token: "snapshot-1",
      clips: [{ clip_id: "100_200.mp4", duration: { state: "available", duration_ms: 10000 } }] });
  }),
}));
vi.mock("./components/LibraryScreen", () => ({ default: () => <div>Games core</div> }));
vi.mock("./components/AppHeader", () => ({ default: () => null }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren();
  vi.restoreAllMocks();
  harness.events.length = 0;
  harness.resolveBatch = null;
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
