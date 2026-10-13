// Install at app/src/App.opening.test.tsx in the isolated 3a750e2 reference.
// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import App from "./App";
import { REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT } from "./benchmark";

const harness = vi.hoisted(() => ({
  held: "clips", kind: "cold_open", calls: [] as string[],
  events: [] as { kind: string; payload: Record<string, unknown> }[],
  cycles: [] as number[], complete: vi.fn(), release: undefined as (() => void) | undefined,
}));
vi.mock("./benchmark", async original => ({
  ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: async () => ({
    emit: (kind: string, payload: Record<string, unknown>) => harness.events.push({ kind, payload }),
    complete: harness.complete,
    scenario: () => ({ id: "qb010-opening-v1-s-test", kind: harness.kind, trial_id: "1", iterations: 6 }),
    fixtureForScenario: () => ({ id: "S", alias: "S", game_timestamp: "1", codec: "h264" }),
    beginOpeningCycle: (_fixture: unknown, cycle: number) => harness.cycles.push(cycle),
  }),
}));
vi.mock("./api", async original => {
  const eager = async (name: string, result: unknown) => {
    harness.calls.push(name);
    if (harness.held === name) await new Promise<void>(resolve => { harness.release = resolve; });
    return result;
  };
  return { ...await original<typeof import("./api")>(),
    loadGames: () => eager("games", [{ timestamp: "1", video_available: true }]),
    loadClips: () => eager("clips", [{ filename: "100_200.mp4" }]),
    loadStorageUsage: () => eager("usage", { game_count: 1, clip_count: 1, game_bytes: 1, clip_bytes: 1, total_bytes: 2 }),
    loadSettings: () => eager("settings", { output_path: "A", auto_delete_days: 0 }),
    loadDdragonStatus: () => new Promise(() => {}),
    ensureHevcCapability: () => new Promise(() => {}),
  };
});
vi.mock("./components/LibraryScreen", () => ({ default: () => <div>Games core</div> }));
vi.mock("./components/ViewerScreen", () => ({ default: () => <div>Opening viewer</div> }));
vi.mock("./components/AppHeader", () => ({ default: (props: { onTab: (tab: string) => void }) => <>
  <button onClick={() => props.onTab("clips")}>Clips</button><button onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  vi.restoreAllMocks(); harness.complete.mockClear();
  harness.calls.length = harness.events.length = harness.cycles.length = 0;
  harness.held = "clips"; harness.kind = "cold_open"; harness.release = undefined;
  document.body.replaceChildren(); localStorage.clear();
});

it.each(["clips", "usage", "games", "settings"])("preserves eager %s readiness while HEVC and assets are pending", async held => {
  harness.held = held;
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(harness.release).toBeTypeOf("function"));
  expect(new Set(harness.calls)).toEqual(new Set(["games", "clips", "usage", "settings"]));
  expect(frames).toHaveLength(0);
  expect(harness.cycles).toHaveLength(0);
  harness.release!();
  await vi.waitFor(() => expect(frames).toHaveLength(1));
  frames.shift()!(1);
  expect(harness.events).toHaveLength(0);
  frames.shift()!(2);
  await vi.waitFor(() => expect(host.textContent).toContain("Opening viewer"));
  expect(harness.cycles).toEqual([0]);
  expect(harness.events.map(event => event.kind)).toEqual(["opening_library_usable", "scenario_started", "replay_requested"]);
  expect(harness.events[0].payload).toEqual({ measurement_contract: "replay-opening-v1", gate: "legacy-resources",
    fixture_id: "S", after_library_paint: true, game_count: 1, clip_count: 1 });
});

it.each([false, true])("rejects changed Games navigation before paint, returning=%s", async back => {
  harness.held = "none";
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(frames).toHaveLength(1));
  frames.shift()!(1);
  host.querySelectorAll<HTMLButtonElement>("button")[0].click();
  if (back) host.querySelectorAll<HTMLButtonElement>("button")[1].click();
  frames.shift()!(2);
  expect(harness.complete).toHaveBeenCalledWith("failed", "Opening Games view changed before paint");
  expect(harness.events.map(event => event.kind)).not.toContain("opening_library_usable");
  expect(harness.cycles).toHaveLength(0);
});

it("retains six warm cycles and the existing remount timer", async () => {
  harness.held = "none"; harness.kind = "warm_open";
  const timers = vi.spyOn(window, "setTimeout");
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.textContent).toContain("Opening viewer"));
  for (let cycle = 1; cycle < 6; cycle++) {
    window.dispatchEvent(new CustomEvent(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT));
    expect(harness.cycles).toEqual(Array.from({ length: cycle + 1 }, (_, index) => index));
    expect(host.textContent).not.toContain("Opening viewer");
    await vi.waitFor(() => expect(host.textContent).toContain("Opening viewer"));
  }
  window.dispatchEvent(new CustomEvent(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT));
  await vi.waitFor(() => expect(harness.complete).toHaveBeenCalledWith("complete", null, { viewer_cycles: 6 }));
  expect(timers.mock.calls.filter(([, delay]) => delay === 100)).toHaveLength(5);
  expect(harness.events.filter(event => event.kind === "opening_library_usable")).toHaveLength(1);
});
