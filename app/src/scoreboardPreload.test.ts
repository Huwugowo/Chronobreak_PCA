import { afterEach, expect, it, vi } from "vitest";
import { ScoreboardAssetPreloader } from "./scoreboardPreload";

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

function harness() {
  vi.useFakeTimers();
  let next = 0;
  const idle = new Map<number, IdleRequestCallback>();
  vi.stubGlobal("requestIdleCallback", (callback: IdleRequestCallback) => { idle.set(++next, callback); return next; });
  vi.stubGlobal("cancelIdleCallback", (id: number) => idle.delete(id));
  class TestImage {
    static instances: TestImage[] = [];
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    src = "";
    decoding = "";
    fetchPriority = "";
    finishDecode!: () => void;
    decode = vi.fn(() => new Promise<void>(resolve => { this.finishDecode = resolve; }));
    constructor() { TestImage.instances.push(this); }
    removeAttribute(name: string) { if (name === "src") this.src = ""; }
  }
  vi.stubGlobal("Image", TestImage);
  const runIdle = (budget = 8) => {
    const callbacks = [...idle.values()]; idle.clear();
    for (const callback of callbacks) callback({ didTimeout: false, timeRemaining: () => budget });
  };
  return { idle, images: TestImage.instances, runIdle };
}

it("preloads unvisited icons once, serializes decode and yields to scrubbing and idle budgets", async () => {
  const { images, idle, runIdle } = harness();
  const cache = new ScoreboardAssetPreloader();
  cache.setPaused(true); cache.add(["early", "late", "early"]);
  await vi.advanceTimersByTimeAsync(1000); runIdle();
  expect(images).toHaveLength(0);
  cache.setPaused(false);
  await vi.advanceTimersByTimeAsync(100);
  expect(idle.size).toBe(0);
  cache.setPaused(true); // A new seek cancels admission before the cooldown.
  await vi.advanceTimersByTimeAsync(1000); runIdle();
  expect(images).toHaveLength(0);
  cache.setPaused(false); await vi.advanceTimersByTimeAsync(150);
  runIdle(0); expect(images).toHaveLength(0); // Busy frames cannot force progress.
  runIdle(); expect(images).toHaveLength(1);
  expect(images[0]).toMatchObject({ src: "early", decoding: "async", fetchPriority: "low" });
  await vi.advanceTimersByTimeAsync(1000); runIdle();
  expect(images).toHaveLength(1);
  cache.setPaused(true); images[0].onload!(); await Promise.resolve();
  expect(images[0].decode).toHaveBeenCalledOnce();
  images[0].finishDecode(); await vi.advanceTimersByTimeAsync(1000); runIdle();
  expect(images).toHaveLength(1); // Finishing admitted work cannot start another during a seek.
  cache.setPaused(false); await vi.advanceTimersByTimeAsync(150); runIdle();
  expect(images).toHaveLength(2); expect(images[1].src).toBe("late");
  cache.dispose(); expect(images.every(image => image.src === "")).toBe(true);
  images[1].onerror!(); await vi.runAllTimersAsync(); runIdle();
  expect(images).toHaveLength(2);
});

it("bounds the source cache and releases queued and decoded work on replacement", async () => {
  const { images, idle, runIdle } = harness();
  const cache = new ScoreboardAssetPreloader();
  cache.add(Array.from({ length: 700 }, (_, i) => `icon-${i}`));
  for (let i = 0; i < 512; i++) {
    await vi.runAllTimersAsync(); runIdle();
    expect(images).toHaveLength(i + 1);
    images[i].onload!(); await Promise.resolve(); images[i].finishDecode();
  }
  await vi.runAllTimersAsync(); runIdle();
  expect(images).toHaveLength(512); expect(idle.size).toBe(0);
  cache.dispose(); expect(images.every(image => image.src === "")).toBe(true);
  const replacement = new ScoreboardAssetPreloader(); replacement.add(["new-source"]);
  await vi.runAllTimersAsync();
  expect(idle.size).toBe(1);
  replacement.dispose(); expect(idle.size).toBe(0); runIdle();
  expect(images).toHaveLength(512);
});
