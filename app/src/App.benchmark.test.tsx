// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { ComponentProps } from "solid-js";
import type ViewerScreen from "./components/ViewerScreen";
import type { BenchmarkScenario, BenchmarkEventMeta } from "./benchmark";
import { exportClip, refreshLibrary } from "./api";
import { parseMediaId, type FrameBoundary, type ReplayTick } from "./replayTime";
import App from "./App";

const harness = vi.hoisted(() => ({
  events: [] as string[],
  records: [] as { kind: string; payload: Record<string, unknown>; meta?: BenchmarkEventMeta }[],
  scenario: null as BenchmarkScenario | null,
  enabled: true,
  withClips: true,
  codec: "h264",
  exportFailure: false,
  viewer: null as ComponentProps<typeof ViewerScreen> | null,
  scenarioId: "qb010-library-v2-n-cold",
  resolveBatch: null as null | (() => void),
  opening: false,
  videoAvailable: true,
  complete: vi.fn(),
}));
vi.mock("./benchmark", async (original) => ({
  ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: async () => harness.enabled ? ({
    emit: (kind: string, payload: Record<string, unknown> = {}, meta?: BenchmarkEventMeta) => {
      harness.events.push(kind); harness.records.push({ kind, payload, meta });
    },
    complete: harness.complete,
    scenario: () => harness.scenario ?? ({ id: harness.scenarioId, kind: harness.opening ? "cold_open" : "app_idle", trial_id: "1", idle_seconds: 60 }),
    fixtureForScenario: () => ({ id: "S", alias: "S", game_timestamp: "1", codec: harness.codec }),
    beginOpeningCycle: () => harness.events.push("opening_requested"),
  }) : null,
}));
vi.mock("./api", async (original) => ({
  ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: "A", auto_delete_days: 0 }),
  loadDdragonStatus: () => harness.opening ? new Promise(() => {}) : Promise.resolve({ state: "offline" }),
  ensureHevcCapability: () => harness.opening ? new Promise(() => {}) : Promise.resolve({ tested: true, supported: false }),
  refreshLibrary: vi.fn(async () => ({ token: "snapshot-1", games: harness.opening || harness.scenario ? [{ timestamp: "1", video_available: harness.videoAvailable, duration_ms: 240000 }] : [],
    clips: harness.withClips ? [{ filename: "100_200.mp4" }] : [],
    usage: { game_count: 0, clip_count: 1, game_bytes: 0, clip_bytes: 1, total_bytes: 1 } })),
  loadReplayDescriptor: async () => ({ game_timestamp: "1", snapshot_token: "snapshot-1",
    media_timeline: { mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
      video: { codec: "h264", profile: "High", timeBase: { numerator: 1n, denominator: 60n }, firstPts: 0n,
        frameRate: { numerator: 60n, denominator: 1n }, frameCount: 14400 as FrameBoundary,
        onePastLastPts: 14400n, replayEnd: 11520000000 as ReplayTick }, audio: { present: true } } }),
  exportClip: vi.fn(async (_request, progress) => {
    progress({ stage: "encode", percent: 50, preset: "horizontal", completed_outputs: 0, total_outputs: 1 });
    if (harness.exportFailure) throw new Error("encoder fixture failed");
    return { elapsed_ms: 10, outputs: [{ filename: "fixture.mp4" }], total_file_size_bytes: 100 };
  }),
  resolveClipDurations: () => new Promise(resolve => {
    harness.events.push("duration_batch_started");
    harness.resolveBatch = () => resolve({ snapshot_token: "snapshot-1",
      clips: [{ clip_id: "100_200.mp4", duration: { state: "available", duration_ms: 10000 } }] });
  }),
}));
vi.mock("./components/LibraryScreen", () => ({ default: () => <div>Games core</div> }));
vi.mock("./components/ViewerScreen", () => ({ default: (props: ComponentProps<typeof ViewerScreen>) => {
  harness.viewer = props; return <div>Opening viewer</div>;
} }));
vi.mock("./components/AppHeader", () => ({ default: (props: { onTab: (tab: string) => void }) => <>
  <button onClick={() => props.onTab("clips")}>Clips</button>
  <button onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren();
  vi.useRealTimers();
  vi.restoreAllMocks();
  harness.events.length = 0;
  harness.records.length = 0;
  harness.scenario = null; harness.enabled = true; harness.withClips = true;
  harness.codec = "h264"; harness.exportFailure = false; harness.viewer = null;
  harness.resolveBatch = null;
  harness.opening = false;
  harness.videoAvailable = true;
  harness.complete.mockClear();
  vi.mocked(refreshLibrary).mockClear(); vi.mocked(exportClip).mockClear();
  localStorage.clear();
});

const mountReady = async () => {
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  if (harness.enabled) {
    await vi.waitFor(() => expect(frames).toHaveLength(1));
    frames.shift()!(1); frames.shift()!(2);
    await vi.waitFor(() => expect(harness.events).toContain("scenario_started"));
  } else await vi.waitFor(() => expect(host.textContent).toContain("Games core"));
  return { host, frames };
};

// Exercise the callback captured by the actual App viewer mount.
const finishViewerCycle = () => {
  harness.events.push("viewer_cycle_completed");
  harness.viewer!.onBenchmarkViewerCycleCompleted!();
};

it("uses the actual App replay path and historical no-refresh return between warm cycles", async () => {
  harness.withClips = false;
  harness.scenario = { id: "historical-warm", kind: "warm_open", trial_id: "1", iterations: 2 };
  const { host } = await mountReady();
  await vi.waitFor(() => expect(harness.viewer?.gameTimestamp).toBe("1"));
  expect(refreshLibrary).toHaveBeenCalledTimes(1);
  vi.useFakeTimers();
  finishViewerCycle();
  expect(host.textContent).toContain("Games core");
  expect(harness.events.slice(-2)).toEqual(["viewer_cycle_completed", "viewer_reopen_requested"]);
  expect(harness.records[harness.records.length - 1]).toEqual({ kind: "viewer_reopen_requested",
    payload: { completed_cycles: 1, expected_cycles: 2 }, meta: { required: true } });
  await vi.advanceTimersByTimeAsync(99);
  expect(host.textContent).not.toContain("Opening viewer");
  await vi.advanceTimersByTimeAsync(1);
  expect(host.textContent).toContain("Opening viewer");
  expect(refreshLibrary).toHaveBeenCalledTimes(1);
  finishViewerCycle();
  expect(harness.events.slice(-2)).toEqual(["viewer_cycle_completed", "scenario_completed"]);
  expect(harness.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { viewer_cycles: 2 });
});

it.each(["idle", "reopen"])("disposal cancels scheduled %s work", async mode => {
  harness.withClips = false;
  harness.scenario = { id: "historical", kind: mode === "idle" ? "app_idle" : "warm_open", trial_id: "1", iterations: 2, idle_seconds: 1 };
  const { host } = await mountReady();
  vi.useFakeTimers();
  if (mode === "reopen") finishViewerCycle();
  dispose!(); dispose = undefined;
  const observed = [...harness.events];
  await vi.advanceTimersByTimeAsync(61000);
  expect(harness.events).toEqual(observed);
  expect(harness.complete).not.toHaveBeenCalled();
  expect(host.textContent).toBe("");
});

it("creates no benchmark paint work without a session", async () => {
  harness.enabled = false;
  const { frames } = await mountReady();
  expect(frames).toHaveLength(0); expect(harness.events).toEqual([]);
});

it("cannot credit an outgoing viewer callback to a replacement App session", async () => {
  harness.withClips = false;
  harness.scenario = { id: "historical-warm", kind: "warm_open", trial_id: "1", iterations: 2 };
  await mountReady(); const outgoing = harness.viewer!.onBenchmarkViewerCycleCompleted!;
  dispose!(); dispose = undefined;
  harness.events.length = 0; harness.records.length = 0;
  await mountReady();
  const before = [...harness.events]; outgoing();
  expect(harness.events).toEqual(before); expect(harness.complete).not.toHaveBeenCalled();
  vi.useFakeTimers(); finishViewerCycle(); await vi.advanceTimersByTimeAsync(100); finishViewerCycle();
  expect(harness.complete).toHaveBeenCalledExactlyOnceWith("complete", null, { viewer_cycles: 2 });
});

it("preserves an explicit unsupported-media failure before opening", async () => {
  harness.withClips = false; harness.codec = "hevc";
  harness.scenario = { id: "historical", kind: "cold_open", trial_id: "1" };
  await mountReady();
  expect(harness.records[harness.records.length - 1]).toMatchObject({ kind: "scenario_failed", payload: { phase: "media_capability" }, meta: { required: true } });
  expect(harness.viewer).toBeNull(); expect(harness.complete).toHaveBeenCalledOnce();
});

it.each([false, true])("preserves export ordering, action identity and source reconciliation (failure=%s)", async failure => {
  harness.withClips = false; harness.exportFailure = failure;
  harness.scenario = { id: "export", kind: "export", trial_id: "1", clip_start_ms: 10000, clip_end_ms: 15000 };
  await mountReady();
  await vi.waitFor(() => expect(harness.complete).toHaveBeenCalledOnce());
  expect(harness.events.filter(kind => !["hevc_capability", "library_useful"].includes(kind))).toEqual([
    "scenario_started", "export_requested", "export_progress", ...(failure ? ["action_failed", "scenario_failed"] : ["export_completed", "scenario_completed"]),
  ]);
  expect(harness.records.find(event => event.kind === "export_requested")).toMatchObject({
    payload: { start_frame: 600, end_frame_exclusive: 900, benchmark_scope: "backend_command", ui_workflow_included: false },
    meta: { actionId: "export-1", required: true },
  });
  expect(harness.records.find(event => event.kind === "export_progress")?.meta).toEqual({ actionId: "export-1" });
  expect(vi.mocked(exportClip).mock.calls[0][2]).toBe("snapshot-1");
  await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
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
