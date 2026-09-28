import { describe, expect, it } from "vitest";
import { LibraryController } from "./libraryController";
import type { LibrarySnapshot } from "./types";

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
};

const snapshot = (token: string, game = token): LibrarySnapshot => ({
  token,
  games: [{ timestamp: game, champion: "Ahri", game_mode: "CLASSIC", duration_ms: 1,
    recorded_at: "2026-01-01T00:00:00Z", kills: 0, deaths: 0, assists: 0,
    summoner_spells: [], keystone_id: null, items: [], participants: [], saved: false, incomplete: false,
    video_size_bytes: 1, video_available: true }],
  clips: [{ filename: `${game}_clip`, game_timestamp: game, clip_timestamp: "1", duration_ms: null,
    file_size_bytes: 1, thumbnail_path: null, thumbnail_url: null, video_url: "",
    source_champion: "Ahri", source_date: null }],
  usage: { games_bytes: 1, clips_bytes: 1, game_count: 1, clip_count: 1 },
});
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };

describe("LibraryController", () => {
  it("settles only the requested refresh and releases superseded/disposed waiters", async () => {
    let next = deferred<LibrarySnapshot>();
    const controller = new LibraryController({ refresh: () => next.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A");
    next.resolve(snapshot("initial", "1")); await flush();
    next = deferred<LibrarySnapshot>();
    const old = controller.invalidateRoot("A");
    const pending = controller.refresh();
    await expect(old).resolves.toBeNull();
    const held = next;
    next = deferred<LibrarySnapshot>();
    held.resolve(snapshot("superseded", "1")); await flush();
    next.resolve(snapshot("fresh", "1"));
    await expect(pending).resolves.toMatchObject({ token: "fresh", root: "A" });
    next = deferred<LibrarySnapshot>();
    const failure = controller.refresh(); next.reject(new Error("scan failed"));
    await expect(failure).resolves.toBeNull();
    next = deferred<LibrarySnapshot>();
    const disposed = controller.refresh(); controller.dispose();
    await expect(disposed).resolves.toBeNull();
    next.resolve(snapshot("too late")); await flush();
  });

  it("bounds replay reads across remounts to one active and one latest intent", async () => {
    const active = deferred<string>();
    const calls: string[] = [];
    const controller = new LibraryController({ refresh: async () => snapshot("token", "1"),
      durations: async () => ({ snapshot_token: "token", clips: [] }) });
    controller.setRoot("A"); await flush();
    const oldOrigin = controller.origin()!;
    const first = controller.readReplay(oldOrigin, "1", token => {
      calls.push(token); return active.promise;
    });
    const firstResult = expect(first).rejects.toThrow("stale");
    controller.navigate(false);
    const origin = controller.origin()!;
    const pending = controller.readReplay(origin, "1", async () => { calls.push("superseded"); return "old"; });
    const pendingResult = expect(pending).rejects.toThrow("stale");
    const latest = controller.readReplay(origin, "1", async token => { calls.push(token); return "latest"; });
    await pendingResult;
    expect(calls).toEqual(["token"]);
    active.resolve("late descriptor");
    await firstResult;
    await expect(latest).resolves.toBe("latest");
    expect(calls).toEqual(["token", "token"]);
    await expect(controller.readReplay(oldOrigin, "1", async () => "not entered")).rejects.toThrow("stale");
    await expect(controller.readReplay(origin, "missing", async () => "not entered")).rejects.toThrow("stale");
    controller.dispose();
  });

  it.each(["refresh", "root ABA", "navigation", "dispose"])(
    "rejects both success and failure after replay invalidation by %s", async invalidation => {
      for (const failure of [false, true]) {
        const active = deferred<string>();
        let token = 0;
        const controller = new LibraryController({ refresh: async () => snapshot(`token-${++token}`, "1"),
          durations: async () => ({ snapshot_token: "", clips: [] }) });
        controller.setRoot("A"); await flush();
        const read = controller.readReplay(controller.origin()!, "1", () => active.promise);
        const result = expect(read).rejects.toThrow("stale");
        if (invalidation === "refresh") controller.refresh();
        else if (invalidation === "root ABA") { controller.setRoot("B"); controller.setRoot("A"); }
        else if (invalidation === "navigation") controller.navigate(false);
        else controller.dispose();
        if (failure) active.reject(new Error("late failure"));
        else active.resolve("late details");
        await result;
        controller.dispose();
      }
    });

  it("releases the replay owner after failure and permits an explicit retry", async () => {
    const controller = new LibraryController({ refresh: async () => snapshot("token", "1"),
      durations: async () => ({ snapshot_token: "token", clips: [] }) });
    controller.setRoot("A"); await flush();
    await expect(controller.readReplay(controller.origin()!, "1", async () => {
      throw new Error("read failed");
    })).rejects.toThrow("read failed");
    await expect(controller.readReplay(controller.origin()!, "1", async () => "retry")).resolves.toBe("retry");
    controller.dispose();
  });

  it("retains a complete same-root view while admitting only the newest refresh", async () => {
    const first = deferred<LibrarySnapshot>();
    const second = deferred<LibrarySnapshot>();
    const requests = [first, second];
    const controller = new LibraryController({ refresh: () => requests.shift()!.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A");
    first.resolve(snapshot("a1", "A1"));
    await flush();
    expect(controller.value.snapshot?.token).toBe("a1");
    const oldOrigin = controller.origin()!;
    controller.refresh();
    expect(controller.value.snapshot?.token).toBe("a1");
    expect(controller.value.actionable).toBe(false);
    second.resolve(snapshot("a2", "A2"));
    await flush();
    expect(controller.value.snapshot?.token).toBe("a2");
    expect(controller.mayPublish(oldOrigin)).toBe(false);
  });

  it("preserves selections for same-root refresh and clears them for a root change", async () => {
    const first = deferred<LibrarySnapshot>();
    const second = deferred<LibrarySnapshot>();
    const scans = [first, second];
    const controller = new LibraryController({ refresh: () => scans.shift()!.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A"); first.resolve(snapshot("a1", "A")); await flush();
    controller.selectGame("A"); controller.selectClip("A_clip");
    controller.refresh();
    expect(controller.value.selectedGame?.id).toBe("A");
    expect(controller.value.activeClip).toBe("A_clip");
    second.resolve(snapshot("a2", "A")); await flush();
    controller.setRoot("B");
    expect(controller.value.selectedGame).toBeNull();
    expect(controller.value.activeClip).toBeNull();
  });

  it("clears A immediately, rejects A/B/A late results, and coalesces refreshes", async () => {
    const a1 = deferred<LibrarySnapshot>();
    const b = deferred<LibrarySnapshot>();
    const a2 = deferred<LibrarySnapshot>();
    const c1 = deferred<LibrarySnapshot>();
    const c2 = deferred<LibrarySnapshot>();
    const pending = [a1, b, a2, c1, c2];
    let calls = 0;
    const controller = new LibraryController({ refresh: () => { calls++; return pending.shift()!.promise; },
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A");
    a1.resolve(snapshot("a1"));
    await flush();
    controller.setRoot("B");
    expect(controller.value.snapshot).toBeNull();
    controller.setRoot("A");
    b.resolve(snapshot("b", "B"));
    await flush();
    expect(controller.value.snapshot).toBeNull();
    a2.resolve(snapshot("a2", "A2"));
    await flush();
    expect(controller.value.snapshot?.token).toBe("a2");
    controller.refresh(); controller.refresh(); controller.refresh();
    expect(calls).toBe(4); // one active scan plus one latest pending intent
    c1.resolve(snapshot("c1"));
    await Promise.resolve();
    expect(calls).toBe(5);
    expect(controller.value.snapshot?.token).toBe("a2");
    c2.resolve(snapshot("c2"));
  });

  it("uses the originating token and keeps side effects separate from response admission", async () => {
    const scan = deferred<LibrarySnapshot>();
    const mutation = deferred<void>();
    let received = "";
    let controller: LibraryController;
    let refreshCount = 0;
    const refreshed = deferred<LibrarySnapshot>();
    controller = new LibraryController({ refresh: () => (++refreshCount === 1 ? scan.promise : refreshed.promise),
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A"); scan.resolve(snapshot("token-a")); await flush();
    const origin = controller.origin()!;
    const operation = controller.mutate(origin, "A", async token => { received = token; return mutation.promise; });
    controller.navigate(false); // completion response is stale
    mutation.resolve();
    const result = await operation;
    expect(received).toBe("token-a");
    expect(result.admitted).toBe(false);
    expect(refreshCount).toBe(2); // stale response still reconciles the captured root
  });

  it("rejects a token from a superseded refresh without entering the worker", async () => {
    const first = deferred<LibrarySnapshot>();
    const second = deferred<LibrarySnapshot>();
    const calls = [first, second];
    const controller = new LibraryController({ refresh: () => calls.shift()!.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A"); first.resolve(snapshot("a1", "A")); await flush();
    const oldOrigin = controller.origin()!;
    controller.refresh();
    let entered = false;
    const result = await controller.mutate(oldOrigin, "A", async () => {
      entered = true;
      return undefined;
    });
    expect(result.admitted).toBe(false);
    expect(entered).toBe(false);
    second.resolve(snapshot("a2", "A"));
    await flush();
    expect(controller.value.actionable).toBe(true);
  });

  it("publishes a settings root side effect even when its response is stale", async () => {
    const scanA = deferred<LibrarySnapshot>();
    const scanB = deferred<LibrarySnapshot>();
    const setting = deferred<{ output_path: string; auto_delete_days: number; hevc_playback_supported: boolean }>();
    const scans = [scanA, scanB];
    const controller = new LibraryController({ refresh: () => scans.shift()!.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A"); scanA.resolve(snapshot("a", "A")); await flush();
    const operation = controller.saveSettings(controller.origin()!, async () => setting.promise);
    controller.navigate(false);
    setting.resolve({ output_path: "B", auto_delete_days: 30, hevc_playback_supported: true });
    const result = await operation;
    expect(result.admitted).toBe(false);
    expect(controller.value.root).toBe("B");
    scanB.resolve(snapshot("b", "B"));
    await flush();
    expect(controller.value.snapshot?.token).toBe("b");
  });

  it("bounds visible duration work, ignores late navigation, and retries unavailable clips", async () => {
    const scan = deferred<LibrarySnapshot>();
    const d1 = deferred<import("./types").ClipDurations>();
    const d2 = deferred<import("./types").ClipDurations>();
    const durationCalls: string[][] = [];
    const controller = new LibraryController({ refresh: () => scan.promise,
      durations: async (_token, ids) => { durationCalls.push(ids); return (durationCalls.length === 1 ? d1 : d2).promise; } });
    controller.setRoot("A"); scan.resolve(snapshot("token-a")); await flush();
    controller.navigate(true);
    const clipId = "token-a_clip";
    controller.requestDurations([clipId, clipId, "missing"]);
    expect(durationCalls).toEqual([[clipId]]);
    controller.navigate(false);
    d1.resolve({ snapshot_token: "token-a", clips: [{ clip_id: clipId, duration: { state: "unavailable" } }] });
    await flush();
    expect(controller.value.durations[clipId]).toBeUndefined();
    controller.navigate(true); controller.requestDurations([clipId], true);
    d2.resolve({ snapshot_token: "token-a", clips: [{ clip_id: clipId, duration: { state: "available", duration_ms: 1234 } }] });
    await flush();
    expect(controller.value.durations[clipId]).toEqual({ state: "available", duration_ms: 1234 });
  });

  it("keeps a failed duration batch local and retries deterministically", async () => {
    const scan = deferred<LibrarySnapshot>();
    const failed = deferred<import("./types").ClipDurations>();
    const retried = deferred<import("./types").ClipDurations>();
    let calls = 0;
    const controller = new LibraryController({ refresh: () => scan.promise, durations: async (_token, ids) => {
      calls++;
      if (calls === 1) {
        await failed.promise;
        return { snapshot_token: "a", clips: ids.map(clip_id => ({ clip_id, duration: { state: "unavailable" as const } })) };
      }
      return retried.promise;
    } });
    controller.setRoot("A"); scan.resolve(snapshot("a")); await flush(); controller.navigate(true);
    const id = "a_clip";
    controller.requestDurations([id]);
    failed.reject(new Error("ffprobe unavailable"));
    await flush();
    expect(controller.value.durations[id]).toEqual({ state: "unavailable" });
    controller.requestDurations([id], true);
    retried.resolve({ snapshot_token: "a", clips: [{ clip_id: id, duration: { state: "available", duration_ms: 99 } }] });
    await flush();
    expect(controller.value.durations[id]).toEqual({ state: "available", duration_ms: 99 });
  });

  it("coalesces pending duration intents without a FIFO backlog", async () => {
    const scan = deferred<LibrarySnapshot>();
    const first = deferred<import("./types").ClipDurations>();
    const second = deferred<import("./types").ClipDurations>();
    const calls: string[][] = [];
    const controller = new LibraryController({ refresh: () => scan.promise, durations: async (_token, ids) => {
      calls.push(ids); return (calls.length === 1 ? first : second).promise;
    } });
    const value = snapshot("a");
    value.clips.push({ ...value.clips[0]!, filename: "a_clip_2" });
    controller.setRoot("A"); scan.resolve(value); await flush(); controller.navigate(true);
    controller.requestDurations(["a_clip"]); controller.requestDurations(["a_clip_2"]);
    expect(calls).toEqual([["a_clip"]]);
    first.resolve({ snapshot_token: "a", clips: [{ clip_id: "a_clip", duration: { state: "available", duration_ms: 10 } }] });
    await flush();
    expect(calls).toEqual([["a_clip"], ["a_clip_2"]]);
    second.resolve({ snapshot_token: "a", clips: [{ clip_id: "a_clip_2", duration: { state: "available", duration_ms: 20 } }] });
    await flush();
    expect(controller.value.durations["a_clip_2"]).toEqual({ state: "available", duration_ms: 20 });
  });

  it("drains benchmark duration compatibility work in sequential batches of eight", async () => {
    const scan = deferred<LibrarySnapshot>();
    const first = deferred<import("./types").ClipDurations>();
    const second = deferred<import("./types").ClipDurations>();
    const calls: string[][] = [];
    const controller = new LibraryController({ refresh: () => scan.promise, durations: async (_token, ids) => {
      calls.push(ids);
      return (calls.length === 1 ? first : second).promise;
    } });
    const value = snapshot("a");
    value.clips = Array.from({ length: 16 }, (_, index) => ({
      ...value.clips[0]!, filename: `a_clip_${index}`,
    }));
    controller.setRoot("A"); scan.resolve(value); await flush();
    const drain = controller.drainDurations(value.clips.map(clip => clip.filename));
    expect(calls[0]).toHaveLength(8);
    first.resolve({ snapshot_token: "a", clips: calls[0]!.map(clip_id => ({ clip_id, duration: { state: "available" as const, duration_ms: 1 } })) });
    await flush();
    expect(calls[1]).toHaveLength(8);
    second.resolve({ snapshot_token: "a", clips: calls[1]!.map(clip_id => ({ clip_id, duration: { state: "available" as const, duration_ms: 2 } })) });
    await expect(drain).resolves.toBe(true);
  });

  it("does not publish after disposal", async () => {
    const scan = deferred<LibrarySnapshot>();
    const controller = new LibraryController({ refresh: () => scan.promise, durations: async () => ({ snapshot_token: "", clips: [] }) });
    let publications = 0;
    controller.subscribe(() => publications++);
    controller.setRoot("A"); controller.dispose(); const before = publications; scan.resolve(snapshot("late")); await flush();
    expect(controller.value.snapshot).toBeNull();
    expect(publications).toBe(before);
  });

  it("invalidates the returned A view after an export completes across A/B/A", async () => {
    const scans = [deferred<LibrarySnapshot>(), deferred<LibrarySnapshot>(), deferred<LibrarySnapshot>(), deferred<LibrarySnapshot>()];
    let scanIndex = 0;
    const exportWork = deferred<void>();
    const controller = new LibraryController({ refresh: () => scans[scanIndex++]!.promise,
      durations: async () => ({ snapshot_token: "", clips: [] }) });
    controller.setRoot("A"); scans[0]!.resolve(snapshot("a1")); await flush();
    const origin = controller.origin()!;
    const operation = controller.mutate(origin, "export", () => exportWork.promise, true);
    controller.setRoot("B"); controller.setRoot("A");
    scans[1]!.resolve(snapshot("b")); scans[2]!.resolve(snapshot("a2")); await flush();
    exportWork.resolve();
    const result = await operation;
    expect(result.admitted).toBe(false);
    expect(scanIndex).toBe(4); // completion invalidates the currently viewed A root
  });
});
