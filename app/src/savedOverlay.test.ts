import { expect, it, vi } from "vitest";
import { LibraryController } from "./libraryController";
import type { LibrarySnapshot } from "./types";
import { reconcileSavedOverlay, isSaveCompletionCurrent, type SavedOverlay } from "./savedOverlay";

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const snapshot = (token: string, saved = false): LibrarySnapshot => ({ token,
  games: [{ timestamp: "1", champion: "Ahri", game_mode: "CLASSIC", duration_ms: 10_000,
    recorded_at: "", kills: 0, deaths: 0, assists: 0, summoner_spells: [], keystone_id: null,
    items: [], participants: [], saved, incomplete: false, video_size_bytes: 1, video_available: true }],
  clips: [], usage: { games_bytes: 1, clips_bytes: 0, game_count: 1, clip_count: 0 },
});
const setup = async () => {
  const refresh = vi.fn<() => Promise<LibrarySnapshot>>().mockResolvedValueOnce(snapshot("a"));
  const controller = new LibraryController({ refresh, durations: vi.fn() });
  controller.setRoot("A"); await Promise.resolve();
  let overlay: SavedOverlay | null = null;
  controller.subscribe(state => { overlay = reconcileSavedOverlay(overlay, controller.origin(), state); });
  const start = () => {
    const intent: SavedOverlay = { origin: controller.origin()!, snapshot: controller.value.snapshot!,
      game: "1", saved: true, phase: "pending" };
    overlay = intent;
    return intent;
  };
  return { controller, refresh, start, overlay: () => overlay };
};

it("bridges only its successful mutation refresh, then accepts canonical saved state even if different", async () => {
  const h = await setup();
  const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  h.refresh.mockReturnValueOnce(scan.promise);
  const intent = h.start();
  const work = h.controller.mutate(intent.origin, "1", () => save.promise);
  expect(h.overlay()?.phase).toBe("pending");
  expect(h.controller.value.snapshot!.games[0].saved).toBe(false);
  save.resolve(); await work;
  expect(h.overlay()).toMatchObject({ saved: true, phase: "refresh" });
  expect(isSaveCompletionCurrent(intent, h.controller.origin(), h.controller.value)).toBe(true);
  expect(h.controller.value.actionable).toBe(false);
  scan.resolve(snapshot("b", false)); await Promise.resolve();
  expect(h.overlay()).toBeNull();
  expect(h.controller.value.snapshot!.games[0].saved).toBe(false);
  h.controller.dispose();
});

it.each(["save", "refresh"])("rolls back after %s failure without losing mandatory reconciliation", async phase => {
  const h = await setup(); const scan = deferred<LibrarySnapshot>();
  h.refresh.mockReturnValueOnce(scan.promise);
  const intent = h.start();
  const result = await h.controller.mutate(intent.origin, "1", async () => {
    if (phase === "save") throw new Error("save failed");
  });
  expect(h.refresh).toHaveBeenCalledTimes(2);
  expect(h.overlay() !== null).toBe(phase === "refresh");
  if (phase === "save") expect(result).toEqual({ admitted: false, error: "save failed" });
  scan.reject(new Error("refresh failed")); await Promise.resolve();
  expect(h.overlay()).toBeNull(); expect(h.controller.value.error).toBe("refresh failed");
  h.controller.dispose();
});

it.each(["refresh", "navigation", "root", "a-b-a"])("rejects old save completion after %s invalidation", async change => {
  const h = await setup(); const save = deferred<void>(); const scan = deferred<LibrarySnapshot>();
  h.refresh.mockReturnValue(scan.promise);
  const intent = h.start(); const result = h.controller.mutate(intent.origin, "1", () => save.promise);
  if (change === "refresh") h.controller.refresh();
  else if (change === "navigation") h.controller.navigate(false);
  else { h.controller.setRoot("B"); if (change === "a-b-a") h.controller.setRoot("A"); }
  expect(h.overlay()).toBeNull();
  save.resolve(); expect((await result).admitted).toBe(false);
  expect(isSaveCompletionCurrent(intent, h.controller.origin(), h.controller.value)).toBe(false);
  expect(h.overlay()).toBeNull();
  scan.resolve(snapshot("new")); await Promise.resolve();
  h.controller.dispose();
});

it("clears the successful bridge on unrelated refresh and never transfers it to a new operation", async () => {
  const h = await setup(); const scan = deferred<LibrarySnapshot>();
  h.refresh.mockReturnValueOnce(scan.promise).mockResolvedValue(snapshot("next"));
  const first = h.start(); await h.controller.mutate(first.origin, "1", async () => {});
  expect(h.overlay()?.phase).toBe("refresh");
  h.controller.refresh(); expect(h.overlay()).toBeNull();
  scan.resolve(snapshot("ignored")); await Promise.resolve(); await Promise.resolve();
  const second = h.start();
  expect(isSaveCompletionCurrent(first, h.controller.origin(), h.controller.value)).toBe(false);
  expect(h.overlay()).toBe(second);
  h.controller.dispose();
});
