/** Optional TAB work gets idle time, never a frame/seek callback or a forced idle timeout. */
export function scheduleScoreboardWork(work: () => void, delay = 0): () => void {
  let cancelled = false;
  let idle: number | undefined;
  const admit = () => {
    if (cancelled) return;
    if (typeof globalThis.requestIdleCallback !== "function") { work(); return; }
    idle = globalThis.requestIdleCallback(deadline => {
      if (cancelled) return;
      if (deadline.timeRemaining() < 4) admit();
      else work();
    });
  };
  const timer = globalThis.setTimeout(admit, Math.max(delay, typeof globalThis.requestIdleCallback === "function" ? 0 : 50));
  return () => {
    cancelled = true; globalThis.clearTimeout(timer);
    if (idle !== undefined) globalThis.cancelIdleCallback(idle);
  };
}

/** Source-owned warm cache: one low-priority image fetch/decode at a time, no DOM
 * or selected snapshot changes. The browser shares these versioned resources
 * with visible icons. A seek pauses new work; an admitted image may finish. */
export class ScoreboardAssetPreloader {
  private known = new Set<string>();
  private queue: string[] = [];
  private images = new Map<string, HTMLImageElement>();
  private cancel: (() => void) | null = null;
  private active = false;
  private paused = false;
  private closed = false;
  private resumeAt = 0;
  add(urls: Iterable<string>) {
    if (this.closed) return;
    for (const url of urls) {
      if (this.known.size >= 512) break;
      if (!this.known.has(url)) { this.known.add(url); this.queue.push(url); }
    }
    this.pump();
  }
  setPaused(paused: boolean) {
    if (paused === this.paused) return;
    this.paused = paused;
    this.cancel?.(); this.cancel = null;
    if (!paused) { this.resumeAt = Date.now() + 150; this.pump(); }
  }
  private pump() {
    if (this.closed || this.paused || this.active || this.cancel || !this.queue.length) return;
    this.cancel = scheduleScoreboardWork(() => {
      this.cancel = null;
      const url = this.queue.shift()!;
      const image = new Image();
      image.decoding = "async"; image.fetchPriority = "low";
      this.images.set(url, image);
      this.active = true;
      const loaded = new Promise<void>((resolve, reject) => {
        image.onload = () => resolve(); image.onerror = () => reject(new Error("Icon unavailable"));
      });
      image.src = url;
      void loaded.then(() => typeof image.decode === "function" ? image.decode() : undefined)
        .catch(() => { this.images.delete(url); })
        .finally(() => {
          image.onload = null; image.onerror = null;
          this.active = false; this.pump();
        });
    }, Math.max(0, this.resumeAt - Date.now()));
  }
  dispose() {
    this.closed = true; this.cancel?.(); this.cancel = null;
    for (const image of this.images.values()) image.removeAttribute("src");
    this.images.clear(); this.queue = []; this.known.clear();
  }
}
