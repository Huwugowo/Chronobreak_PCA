// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import App from "./App";
import { refreshLibrary, setGameSaved } from "./api";
import type { GameSummary, LibrarySnapshot } from "./types";

const harness = vi.hoisted(() => ({ root: "A", destination: "B" }));
vi.mock("./benchmark", async original => ({ ...await original<typeof import("./benchmark")>(), initializeReplayBenchmark: async () => null }));
vi.mock("./api", async original => ({ ...await original<typeof import("./api")>(),
  refreshLibrary: vi.fn(), setGameSaved: vi.fn(),
  loadSettings: async () => ({ output_path: harness.root, auto_delete_days: 0 }),
  loadDdragonStatus: () => new Promise(() => {}), ensureHevcCapability: () => new Promise(() => {}),
  chooseOutputFolder: async () => harness.destination,
  persistSettings: async (value: { output_path: string }) => { harness.root = value.output_path; return value; },
}));
vi.mock("./components/AppHeader", () => ({ default: (props: { onSettings: () => void; onTab: (tab: "games") => void }) => <>
  <button data-testid="settings" onClick={props.onSettings}>Settings</button>
  <button data-testid="games" onClick={() => props.onTab("games")}>Games</button>
</> }));
vi.mock("./components/SettingsScreen", () => ({ default: (props: { onBack: () => void; onChooseFolder: () => void }) => <>
  <button data-testid="back" onClick={props.onBack}>Back</button>
  <button data-testid="root" onClick={props.onChooseFolder}>Root</button>
</> }));
const game: GameSummary = { timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", recorded_at: "2026-09-23T00:00:00Z",
  duration_ms: 10000, kills: 1, deaths: 2, assists: 3, items: [], participants: [], summoner_spells: [],
  keystone_id: null, saved: false, incomplete: false, video_size_bytes: 1, video_available: true };
const snapshot = (token: string, saved = false): LibrarySnapshot => ({ token,
  games: [{ ...game, saved }], clips: [], usage: { games_bytes: 1, clips_bytes: 0, game_count: 1, clip_count: 0 } });
const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
let dispose: (() => void) | undefined;
beforeEach(() => { vi.mocked(refreshLibrary).mockReset().mockResolvedValueOnce(snapshot("a")); vi.mocked(setGameSaved).mockReset(); });
afterEach(() => { dispose?.(); document.body.replaceChildren(); localStorage.clear(); vi.clearAllMocks(); harness.root = "A"; });
const mount = async () => {
  const host = document.createElement("div"); document.body.append(host); dispose = render(() => <App />, host);
  await vi.waitFor(() => expect(host.querySelector('[aria-label="Save Ahri recording"]')).not.toBeNull());
  return { host, star: () => host.querySelector<HTMLButtonElement>('[aria-label$="save Ahri recording" i]')!,
    click: (id: string) => host.querySelector<HTMLButtonElement>('[data-testid="'+id+'"]')!.click() };
};
it.each([false, true])("keeps an immediate Star through its save refresh and then uses canonical saved=%s", async canonical => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.star().click();
  expect(h.star().getAttribute("data-saved")).toBe("true"); expect(h.star().disabled).toBe(true);
  h.star().click(); expect(setGameSaved).toHaveBeenCalledTimes(1);
  expect(setGameSaved).toHaveBeenCalledWith("1", true, "a");
  save.resolve(); await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  expect(h.star().getAttribute("data-saved")).toBe("true"); expect(h.star().disabled).toBe(true);
  scan.resolve(snapshot("b", canonical)); await vi.waitFor(() => expect(h.star().disabled).toBe(false));
  expect(h.star().getAttribute("data-saved")).toBe(String(canonical));
});
it("rolls back a failed save while retaining cards through the compulsory refresh", async () => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValueOnce(scan.promise);
  const h = await mount(); h.star().click(); save.reject(new Error("fixture save failure"));
  await vi.waitFor(() => expect(h.host.textContent).toContain("fixture save failure"));
  expect(h.star().getAttribute("data-saved")).toBe("false"); expect(refreshLibrary).toHaveBeenCalledTimes(2);
  scan.resolve(snapshot("b")); await vi.waitFor(() => expect(h.star().disabled).toBe(false));
});
it("suppresses stale save notices and stars after navigation while still reconciling the filesystem", async () => {
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  vi.mocked(setGameSaved).mockReturnValueOnce(save.promise); vi.mocked(refreshLibrary).mockReturnValue(scan.promise);
  const h = await mount(); h.star().click(); h.click("settings");
  save.resolve(); await vi.waitFor(() => expect(refreshLibrary).toHaveBeenCalledTimes(2));
  expect(h.host.textContent).not.toContain("Recording saved.");
  scan.resolve(snapshot("b", false)); await Promise.resolve(); h.click("back");
  await vi.waitFor(() => expect(h.star()?.disabled).toBe(false));
  expect(h.star().getAttribute("data-saved")).toBe("false"); expect(h.host.textContent).not.toContain("Recording saved.");
});
