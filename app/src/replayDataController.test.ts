import { describe, expect, it, vi } from "vitest";
import { ReplayDataController, type ReplayWindow, type ReplayWindowRequest } from "./replayDataController";
import type { MediaId, ReplayTick } from "./replayTime";
import type { ImportProgress } from "./leagueIndex";

const tick = (value: number) => value as ReplayTick;
const media = "11111111-2222-4333-8444-555555555555" as MediaId;
const scope = { source: { media_id: media, run_id: media, epoch: 1 }, local_participant: 1 };
const query = (): Omit<ReplayWindowRequest, "after" | "limit"> => ({ scope: scope.source, start: tick(0), end: tick(1000), at: tick(500) });
const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; }); return { promise, resolve, reject }; };
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
const page = (id: number, next: ReplayWindow["next"] = null): ReplayWindow => ({ ...scope, events: Array.from({ length: 1 }, () => ({ sequence: id } as ReplayWindow["events"][number])), next, roster: null,
  identities: [], coverage: {} as ReplayWindow["coverage"] });

function setup(overrides: Partial<ConstructorParameters<typeof ReplayDataController>[0]> = {}) {
  const api = {
    gameTimestamp: "game", mediaId: media,
    readReplay: <T,>(work: (token: string) => Promise<T>) => work("snapshot"),
    loadRecordingFacts: vi.fn(async () => ({ media_id: media, local_player_name: "Me", league_match: null, league_result: null })),
    indexRecording: vi.fn(async () => ({ status: "ready" })),
    sourceScopes: vi.fn(async () => [scope]),
    queryReplayWindow: vi.fn(async () => page(1)),
    ...overrides,
  };
  return { api, controller: new ReplayDataController(api) };
}

describe("ReplayDataController", () => {
  it("finishes positive tail coverage without polling and explicit retry observes a new commit", async () => {
    vi.useFakeTimers();
    const tail: ImportProgress = { media_id: media, status: "ready", batches: 0,
      position: { end_offset: 100, last_sequence: 7, checksum: Array(32).fill(1) }, tail_bytes: 512 };
    const indexRecording = vi.fn().mockResolvedValueOnce({ ...tail, status: "pending", batches: 4, tail_bytes: 0 })
      .mockResolvedValueOnce(tail).mockResolvedValue({ ...tail, batches: 1, tail_bytes: 0,
        position: { end_offset: 900, last_sequence: 8, checksum: Array(32).fill(2) } });
    const { api, controller } = setup({ indexRecording });
    try {
      const initialized = controller.initialize();
      await vi.advanceTimersByTimeAsync(100);
      await initialized;
      expect(indexRecording).toHaveBeenCalledTimes(2);
      expect(api.sourceScopes).toHaveBeenCalledTimes(1);
      expect(controller.value).toMatchObject({ indexing: "ready", loading: { index: false }, error: null });
      await vi.advanceTimersByTimeAsync(30_000);
      expect(indexRecording).toHaveBeenCalledTimes(2);
      expect(vi.getTimerCount()).toBe(0);
      await controller.retry();
      expect(indexRecording).toHaveBeenCalledTimes(3);
      expect(api.sourceScopes).toHaveBeenCalledTimes(2);
    } finally { controller.dispose(); vi.useRealTimers(); }
  });

  it("calls browser timers with their global receiver while loading and disposing replay details", async () => {
    vi.useFakeTimers();
    const schedule = globalThis.setTimeout, cancel = globalThis.clearTimeout;
    const checkedSchedule = vi.spyOn(globalThis, "setTimeout").mockImplementation(function (this: unknown, ...args: Parameters<typeof setTimeout>) {
      if (this !== globalThis) throw new TypeError("Illegal invocation");
      return schedule(...args);
    });
    const checkedCancel = vi.spyOn(globalThis, "clearTimeout").mockImplementation(function (this: unknown, ...args: Parameters<typeof clearTimeout>) {
      if (this !== globalThis) throw new TypeError("Illegal invocation");
      return cancel(...args);
    });
    const { api, controller } = setup();
    try {
      controller.requestWindow(query()); await controller.initialize();
      expect(controller.value.error).toBeNull();
      await vi.advanceTimersByTimeAsync(120);
      expect(api.queryReplayWindow).toHaveBeenCalledOnce();
      controller.requestWindow({ ...query(), at: tick(700) });
      controller.dispose();
      expect(checkedCancel).toHaveBeenCalled();
    } finally {
      checkedSchedule.mockRestore(); checkedCancel.mockRestore(); vi.useRealTimers();
    }
  });
  it("loads facts independently, then scopes and a debounced selected source page", async () => {
    vi.useFakeTimers(); const { api, controller } = setup();
    controller.requestWindow(query()); const started = controller.initialize();
    await started; expect(controller.value.facts?.local_player_name).toBe("Me");
    expect(api.queryReplayWindow).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledWith("snapshot", "game", expect.objectContaining({ scope: scope.source, limit: 512 }));
    expect(controller.value.activeWindow?.events).toHaveLength(1);
    vi.useRealTimers();
  });

  it("continues optional indexing after facts fail and clears that error on explicit retry", async () => {
    const facts = vi.fn().mockRejectedValueOnce(new Error("fixture facts failure"))
      .mockResolvedValue({ media_id: media, local_player_name: null, league_match: null, league_result: null });
    const { api, controller } = setup({ loadRecordingFacts: facts });
    await controller.initialize();
    expect(api.indexRecording).toHaveBeenCalledTimes(1);
    expect(controller.value.scopes).toEqual([scope]);
    expect(controller.value.factsError).toBe("fixture facts failure");
    await controller.retry();
    expect(controller.value.factsError).toBeNull();
    expect(controller.value.error).toBeNull();
    controller.dispose();
  });

  it("keeps one active window IPC sequence and replaces it with only the latest intent", async () => {
    vi.useFakeTimers(); const first = deferred<ReplayWindow>(); const { api, controller } = setup({ queryReplayWindow: vi.fn(() => first.promise) });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    const later = { ...query(), at: tick(900) }; controller.requestWindow(later); await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(1);
    first.resolve(page(1)); await flush();
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(2);
    expect(api.queryReplayWindow).toHaveBeenLastCalledWith("snapshot", "game", expect.objectContaining({ at: 900 }));
    vi.useRealTimers();
  });

  it("never publishes a stale page or follows an await after disposal", async () => {
    vi.useFakeTimers(); const pending = deferred<ReplayWindow>(); const { api, controller } = setup({ queryReplayWindow: vi.fn(() => pending.promise) });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    controller.dispose(); pending.resolve(page(1)); await flush();
    expect(controller.value.activeWindow).toBeNull();
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(1); vi.useRealTimers();
  });

  it("uses explicit 512-row pagination and retains no more than four pages", async () => {
    vi.useFakeTimers(); let call = 0;
    const { api, controller } = setup({ queryReplayWindow: vi.fn(async () =>
      page(++call, call < 6 ? { replay_tick: tick(call), source_id: call } : null)) });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    expect(controller.isFollowingPage(controller.value.activeWindow)).toBe(false);
    for (let index = 0; index < 5; index++) { controller.loadMore(); await vi.advanceTimersByTimeAsync(120); }
    expect(controller.isFollowingPage(controller.value.activeWindow)).toBe(true);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(6);
    expect(controller.value.windows).toHaveLength(4);
    expect(controller.value.windows.flatMap(item => item.events)).toHaveLength(4);
    expect(controller.value.activeWindow?.next).toBeNull(); vi.useRealTimers();
  });

  it("restores an exact cached viewport immediately without refetching or retaining a superseded intent", async () => {
    vi.useFakeTimers(); let call = 0;
    const { api, controller } = setup({ queryReplayWindow: vi.fn(async () => page(++call)) });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    const whole = controller.value.activeWindow;
    controller.requestWindow({ ...query(), end: tick(1500) }); await vi.advanceTimersByTimeAsync(120);
    controller.requestWindow({ ...query(), at: tick(600) });
    controller.requestWindow(query());
    expect(controller.value.activeWindow).toBe(whole);
    expect(controller.value.loading.window).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(2);
    controller.requestWindow({ ...query(), at: tick(600) }); await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(3);
    controller.dispose(); vi.useRealTimers();
  });

  it("does not let an in-flight response replace a restored cached window", async () => {
    vi.useFakeTimers(); const held = deferred<ReplayWindow>();
    const read = vi.fn().mockResolvedValueOnce(page(1)).mockReturnValueOnce(held.promise);
    const { controller } = setup({ queryReplayWindow: read });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    const whole = controller.value.activeWindow;
    controller.requestWindow({ ...query(), end: tick(1500) }); await vi.advanceTimersByTimeAsync(120);
    controller.requestWindow(query());
    held.resolve(page(2)); await flush();
    expect(controller.value.activeWindow).toBe(whole);
    expect(read).toHaveBeenCalledTimes(2);
    controller.dispose(); vi.useRealTimers();
  });

  it("keeps a complete containing page during repeated zooms without replacing its events or roster time", async () => {
    vi.useFakeTimers(); const { api, controller } = setup();
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    const whole = controller.value.activeWindow;
    for (let zoom = 1; zoom <= 10; zoom++) {
      controller.requestWindow({ ...query(), start: tick(zoom * 10), end: tick(1000 - zoom * 10) });
      expect(controller.value.activeWindow).toBe(whole);
    }
    controller.requestWindow(query());
    expect(controller.value.activeWindow).toBe(whole);
    expect(api.queryReplayWindow).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
    controller.requestWindow({ ...query(), at: tick(600) }); await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(2);
    controller.dispose(); vi.useRealTimers();
  });

  it("refetches a narrower viewport when cached pages are truncated or start after a cursor", async () => {
    vi.useFakeTimers();
    const read = vi.fn().mockResolvedValueOnce(page(1, { replay_tick: tick(100), source_id: 1 }))
      .mockResolvedValueOnce(page(2)).mockResolvedValue(page(3));
    const { controller } = setup({ queryReplayWindow: read });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    controller.loadMore(); await vi.advanceTimersByTimeAsync(120);
    controller.requestWindow({ ...query(), start: tick(200), end: tick(700) }); await vi.advanceTimersByTimeAsync(120);
    expect(read).toHaveBeenCalledTimes(3);
    expect(read).toHaveBeenLastCalledWith("snapshot", "game", expect.objectContaining({ after: null, start: 200, end: 700 }));
    controller.dispose(); vi.useRealTimers();
  });

  it("rechecks scope membership after replacing a debounced intent and accepts reordered scope keys", async () => {
    vi.useFakeTimers(); const { api, controller } = setup();
    await controller.initialize();
    controller.requestWindow(query());
    controller.requestWindow({ ...query(), scope: { ...scope.source, epoch: 99 } });
    await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).not.toHaveBeenCalled();
    controller.requestWindow({ ...query(), scope: { epoch: scope.source.epoch, run_id: media, media_id: media } });
    await vi.advanceTimersByTimeAsync(120);
    expect(api.queryReplayWindow).toHaveBeenCalledTimes(1);
    controller.dispose(); vi.useRealTimers();
  });

  it("clears a recovered window failure and rejects a response from another epoch", async () => {
    vi.useFakeTimers();
    const read = vi.fn().mockRejectedValueOnce(new Error("fixture window failure"))
      .mockResolvedValueOnce({ ...page(1), source: { ...scope.source, epoch: 2 } })
      .mockResolvedValue(page(3));
    const { controller } = setup({ queryReplayWindow: read });
    await controller.initialize();
    for (let attempt = 0; attempt < 3; attempt++) {
      controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
      if (attempt < 2) expect(controller.value.windowError).not.toBeNull();
    }
    expect(controller.value.windowError).toBeNull();
    expect(controller.value.error).toBeNull();
    expect(controller.value.activeWindow?.events[0].sequence).toBe(3);
    controller.dispose(); vi.useRealTimers();
  });

  it("cancels a pending import pause without continuing work after disposal", async () => {
    vi.useFakeTimers(); const { api, controller } = setup({ indexRecording: vi.fn(async () => ({ status: "pending" })) });
    const initialized = controller.initialize(); await flush(); await flush();
    expect(api.indexRecording).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(1);
    controller.dispose(); await initialized;
    expect(vi.getTimerCount()).toBe(0);
    expect(api.sourceScopes).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("discards a superseded pending viewport when pagination becomes the latest intent", async () => {
    vi.useFakeTimers(); const held = deferred<ReplayWindow>();
    const read = vi.fn().mockResolvedValueOnce(page(1, { replay_tick: tick(10), source_id: 1 }))
      .mockReturnValueOnce(held.promise).mockResolvedValue(page(3));
    const { controller } = setup({ queryReplayWindow: read });
    await controller.initialize(); controller.requestWindow(query()); await vi.advanceTimersByTimeAsync(120);
    controller.requestWindow({ ...query(), at: tick(600) }); await vi.advanceTimersByTimeAsync(120);
    controller.requestWindow({ ...query(), at: tick(700) }); await vi.advanceTimersByTimeAsync(120);
    controller.loadMore(); await vi.advanceTimersByTimeAsync(120);
    held.resolve(page(2)); await flush(); await flush();
    expect(read).toHaveBeenCalledTimes(3);
    expect(read).toHaveBeenLastCalledWith("snapshot", "game", expect.objectContaining({
      at: 500, after: { replay_tick: 10, source_id: 1 },
    }));
    controller.dispose(); vi.useRealTimers();
  });
});
