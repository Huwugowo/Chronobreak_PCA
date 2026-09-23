// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { ComponentProps } from "solid-js";
import App from "./App";
import type ViewerScreen from "./components/ViewerScreen";
import type LibraryScreen from "./components/LibraryScreen";
import type ClipExporterScreen from "./components/ClipExporterScreen";
import { ensureHevcCapability, resolveClipDurations } from "./api";
import { initializeReplayBenchmark } from "./benchmark";
import { parseMediaId, type FrameBoundary } from "./replayTime";

const harness = vi.hoisted(() => ({ root: "A", revision: 0,
  viewer: null as ComponentProps<typeof ViewerScreen> | null,
  exporter: null as ComponentProps<typeof ClipExporterScreen> | null,
}));
vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(),
  initializeReplayBenchmark: vi.fn(async () => null) }));
vi.mock("./api", async original => ({ ...await original<typeof import("./api")>(),
  loadSettings: async () => ({ output_path: harness.root, auto_delete_days: 0 }),
  loadDdragonStatus: () => new Promise(() => {}),
  ensureHevcCapability: vi.fn(() => new Promise(() => {})),
  chooseOutputFolder: async () => "B",
  persistSettings: async (value: { output_path: string }) => { harness.root = value.output_path; return value; },
  refreshLibrary: async () => ({ token: `${harness.root}-${++harness.revision}`, games: [{
    timestamp: "1", champion: harness.root, game_mode: "CLASSIC", recorded_at: "2026-09-21T00:00:00Z",
    kills: 0, deaths: 0, assists: 0, duration_ms: 10000, summoner_spells: [], keystone_id: null,
    items: [], participants: [], saved: false, incomplete: false, video_size_bytes: 1, video_available: true,
  }], clips: [{ filename: "1_2.mp4" }],
    usage: { games_bytes: 1, clips_bytes: 1, game_count: 1, clip_count: 1 } }),
  resolveClipDurations: vi.fn(() => new Promise(() => {})),
}));
vi.mock("./components/LibraryScreen", () => ({ default: (props: ComponentProps<typeof LibraryScreen>) =>
  <button data-testid="open" onClick={() => props.onOpenGame("1", props.snapshotOrigin)}>Open recording</button> }));
vi.mock("./components/AppHeader", () => ({ default: (props: { onSettings: () => void; onTab: (tab: "games" | "clips") => void }) => <>
  <button data-testid="settings" onClick={props.onSettings}>Settings</button>
  <button data-testid="clips" onClick={() => props.onTab("clips")}>Clips</button>
  <button data-testid="games" onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/SettingsScreen", () => ({ default: (props: { onBack: () => void; onChooseFolder: () => void }) => <>
  <button data-testid="settings-back" onClick={props.onBack}>Back</button>
  <button data-testid="root" onClick={props.onChooseFolder}>Change root</button>
</> }));
vi.mock("./components/StorageIndicator", () => ({ default: () => null }));
vi.mock("./components/ViewerScreen", () => ({ default: (props: ComponentProps<typeof ViewerScreen>) => {
  harness.viewer = props;
  return <div data-testid="viewer">{props.game.champion}</div>;
} }));
vi.mock("./components/ClipExporterScreen", () => ({ default: (props: ComponentProps<typeof ClipExporterScreen>) => {
  harness.exporter = props;
  return <div data-testid="exporter" />;
} }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.(); dispose = undefined;
  document.body.replaceChildren(); localStorage.clear(); vi.clearAllMocks(); vi.restoreAllMocks();
  harness.root = "A"; harness.revision = 0; harness.viewer = null; harness.exporter = null;
});
const mount = async () => {
  const host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.querySelector('[data-testid="open"]')).not.toBeNull());
  return { host, click: (id: string) => host.querySelector<HTMLButtonElement>(`[data-testid="${id}"]`)!.click() };
};

it("opens independently of held assets/durations and reacquires origins on Settings/exporter return", async () => {
  const { host, click } = await mount();
  click("clips");
  await vi.waitFor(() => expect(resolveClipDurations).toHaveBeenCalledTimes(1));
  click("games"); click("open");
  expect(host.querySelector('[data-testid="viewer"]')?.textContent).toBe("A");
  const first = harness.viewer!;
  let release!: (value: string) => void;
  const read = first.readReplay(() => new Promise<string>(resolve => { release = resolve; }));
  const stale = expect(read).rejects.toThrow("stale");
  click("settings"); click("settings-back");
  const second = harness.viewer!;
  expect(second).not.toBe(first);
  let entered = false;
  const next = second.readReplay(async token => { entered = true; return token; });
  expect(entered).toBe(false);
  release("old details"); await stale;
  await expect(next).resolves.toMatch(/^A-/);
  await expect(first.readReplay(async () => "late retry")).rejects.toThrow("stale");
  const draft = { gameTimestamp: "1", mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
    startFrame: 0 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  second.onExportClip(draft);
  expect(host.querySelector('[data-testid="exporter"]')).not.toBeNull();
  await expect(harness.exporter!.readReplay(async token => token)).resolves.toMatch(/^A-/);
  harness.exporter!.onBack(draft);
  expect(harness.viewer!.initialClipDraft).toEqual(draft);
  await expect(harness.viewer!.readReplay(async token => token)).resolves.toMatch(/^A-/);
  await expect(second.readReplay(async () => "late retry")).rejects.toThrow("stale");
});

it("does not reopen a retained viewer selection after Settings changes to an overlapping-ID root", async () => {
  const { host, click } = await mount(); click("open");
  const old = harness.viewer!;
  let release!: (value: string) => void;
  const stale = expect(old.readReplay(() => new Promise<string>(resolve => { release = resolve; }))).rejects.toThrow("stale");
  click("settings"); click("root");
  await vi.waitFor(() => expect(host.textContent).toContain("Settings saved."));
  click("settings-back");
  await vi.waitFor(() => expect(host.querySelector('[data-testid="open"]')).not.toBeNull());
  expect(host.querySelector('[data-testid="viewer"]')).toBeNull();
  click("open");
  expect(host.querySelector('[data-testid="viewer"]')?.textContent).toBe("B");
  const next = harness.viewer!.readReplay(async token => token);
  release("root A descriptor"); await stale;
  await expect(next).resolves.toMatch(/^B-/);
});


it.each(["stable", "away", "away-back"])("admits benchmark library milestones only for the painted origin (%s)", async navigation => {
  const emit = vi.fn();
  const complete = vi.fn().mockResolvedValue(undefined);
  const observer = { emit, complete, scenario: () => ({ id: "qb010-library-v2-fixture", kind: "app_idle" }) };
  vi.mocked(initializeReplayBenchmark).mockResolvedValueOnce(observer as unknown as NonNullable<Awaited<ReturnType<typeof initializeReplayBenchmark>>>);
  vi.mocked(ensureHevcCapability).mockResolvedValueOnce({ tested: true, supported: true, probe_url: "fixture" });
  vi.mocked(resolveClipDurations).mockImplementation(async token => ({ snapshot_token: token,
    clips: [{ clip_id: "1_2.mp4", duration: { state: "available", duration_ms: 1000 } }] }));
  const frames: FrameRequestCallback[] = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frames.push(callback); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
  const { click } = await mount();
  await vi.waitFor(() => expect(emit.mock.calls.some(([kind]) => kind === "library_view_admitted")).toBe(true));
  frames.shift()!(performance.now());
  if (navigation !== "stable") click("clips");
  if (navigation === "away-back") click("games");
  frames.shift()!(performance.now());
  if (navigation === "stable") {
    await vi.waitFor(() => expect(emit.mock.calls.some(([kind]) => kind === "library_useful")).toBe(true));
    const events = emit.mock.calls.map(([kind]) => kind);
    expect(events.indexOf("games_library_usable")).toBeLessThan(events.indexOf("library_useful"));
    expect(events).toContain("scenario_started");
    expect(complete).not.toHaveBeenCalled();
  } else {
    await vi.waitFor(() => expect(complete).toHaveBeenCalledTimes(1));
    expect(complete).toHaveBeenCalledWith("failed", expect.any(String), { phase: "library_paint" });
    const events = emit.mock.calls.map(([kind]) => kind);
    for (const kind of ["games_library_usable", "library_useful", "scenario_started"]) expect(events).not.toContain(kind);
  }
});


it("resumes only the returned viewer after held details outlive an exporter round trip", async () => {
  const { click } = await mount(); click("open");
  const first = harness.viewer!;
  let release!: (value: string) => void;
  const stale = expect(first.readReplay(() => new Promise<string>(resolve => { release = resolve; }))).rejects.toThrow("stale");
  const draft = { gameTimestamp: "1", mediaId: parseMediaId("11111111-2222-4333-8444-555555555555"),
    startFrame: 60 as FrameBoundary, endFrameExclusive: 600 as FrameBoundary };
  first.onExportClip(draft);
  const exporterWork = vi.fn(async () => "exporter details");
  const cancelledExporter = expect(harness.exporter!.readReplay(exporterWork)).rejects.toThrow("stale");
  harness.exporter!.onBack(draft);
  const returned = harness.viewer!;
  expect(returned.initialClipDraft).toEqual(draft);
  const returnedWork = vi.fn(async token => token);
  const latest = returned.readReplay(returnedWork);
  await cancelledExporter;
  expect(exporterWork).not.toHaveBeenCalled();
  expect(returnedWork).not.toHaveBeenCalled();
  release("old optional details");
  await stale;
  await expect(latest).resolves.toMatch(/^A-/);
  expect(returnedWork).toHaveBeenCalledTimes(1);
  expect(exporterWork).not.toHaveBeenCalled();
});
