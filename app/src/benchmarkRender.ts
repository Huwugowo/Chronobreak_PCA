import type { ReplayBenchmarkObserver } from "./benchmark";

export type BenchmarkRenderedState = {
  generation: number;
  mediaTick: number;
  sourceEpoch: number;
  sourceToken: string;
  eventCount: number;
  players: { name: string; level: number; portraitUrl: string; spells: readonly (string | null)[] }[];
};
const unsupportedSpells = new Set(["Primal Smite", "Unleashed Smite", "Unleashed Teleport"]);
const unsupported = (image: HTMLImageElement) => {
  const url = new URL(image.src);
  return url.pathname.includes("/spell/") && unsupportedSpells.has(decodeURIComponent(url.pathname.split("/").pop()!));
};

// Names are used only to reconcile the actual DOM with the admitted sample.
// Receipts contain counts, source epoch and generation, never player identities.
export const renderedReplayImages = (root: Element, state: BenchmarkRenderedState): HTMLImageElement[] | null => {
  const roster = root.querySelector('[data-testid="champion-filter-fullscreen"]');
  const buttons = [...(roster?.querySelectorAll<HTMLElement>('[data-testid="champion-filter-option"]') ?? [])];
  if (state.players.length !== 10 || state.eventCount < 1 || buttons.length !== 10 ||
      root.querySelectorAll("[data-event-icon], [data-cluster-kind]").length < 1) return null;
  for (const player of state.players) {
    const matches = buttons.filter(button => button.dataset.playerName === player.name);
    if (matches.length !== 1) return null;
    const button = matches[0];
    if (button.querySelector('[data-testid="champion-level"]')?.textContent !== String(player.level) ||
        ![...button.querySelectorAll<HTMLImageElement>("img")].some(image => image.src === player.portraitUrl)) return null;
    for (const spell of player.spells) {
      if (!spell || unsupportedSpells.has(spell)) continue;
      if (![...button.querySelectorAll<HTMLImageElement>("img")].some(image => !image.hidden &&
          new URL(image.src).pathname.endsWith(`/spell/${encodeURIComponent(spell)}`))) return null;
    }
  }
  return [...roster!.querySelectorAll<HTMLImageElement>("img")].filter(image => !unsupported(image));
};

export const waitForBenchmarkRenderedReplay = async (observer: ReplayBenchmarkObserver, signal: AbortSignal,
  read: () => BenchmarkRenderedState | null, root: Element) => {
  let frame: number | undefined, timer: ReturnType<typeof setTimeout> | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;
  let rejectStopped!: (error: Error) => void;
  let ended = false;
  const stopped = new Promise<never>((_, reject) => { rejectStopped = reject; });
  const abort = () => { ended = true; rejectStopped(new Error("render readiness disposed")); };
  const pause = (ms: number) => new Promise<void>(resolve => { timer = setTimeout(resolve, ms); });
  const paint = () => new Promise<void>(resolve => {
    frame = requestAnimationFrame(() => { frame = requestAnimationFrame(() => resolve()); });
  });
  const work = async () => {
    while (!ended) {
      const state = read();
      const images = state && renderedReplayImages(root, state);
      if (!state || !images) { await pause(50); continue; }
      if (!images.length) throw new Error("render readiness has no supported images");
      observer.emit("replay_render_admitted", { participant_count: state.players.length, event_count: state.eventCount,
        source_epoch: state.sourceEpoch, source_token: state.sourceToken, media_tick: state.mediaTick },
        { generation: state.generation, required: true });
      await Promise.all(images.map(async image => {
        try { await image.decode(); }
        catch { throw new Error(`render readiness image could not decode: ${new URL(image.src).pathname.split("/").slice(-2).join("/")}`); }
        if (!image.isConnected || !image.complete || image.naturalWidth <= 0 || image.naturalHeight <= 0) {
          throw new Error("render readiness image did not decode");
        }
      }));
      await paint();
      if (ended) return;
      const current = read();
      const currentImages = current && renderedReplayImages(root, current);
      if (!current || current.generation !== state.generation || current.sourceEpoch !== state.sourceEpoch ||
          current.sourceToken !== state.sourceToken ||
          current.mediaTick !== state.mediaTick || !currentImages || currentImages.length !== images.length ||
          images.some((image, index) => image !== currentImages[index] || !image.isConnected)) {
        throw new Error("render readiness changed during decode/paint");
      }
      observer.emit("replay_render_ready", { participant_count: current.players.length, event_count: current.eventCount,
        rendered_marker_count: root.querySelectorAll("[data-event-icon], [data-cluster-kind]").length,
        decoded_image_count: images.length, decoded_url_count: new Set(images.map(image => image.src)).size,
        source_epoch: current.sourceEpoch, source_token: current.sourceToken, media_tick: current.mediaTick,
        authority: "dom-and-image-decode-after-two-animation-frames",
        unsupported_spell_labels: [...unsupportedSpells] }, { generation: current.generation, required: true });
      return;
    }
  };
  signal.addEventListener("abort", abort, { once: true });
  deadline = setTimeout(() => { ended = true; rejectStopped(new Error("render readiness timed out")); }, 10_000);
  try {
    if (signal.aborted) abort();
    await Promise.race([stopped, work()]);
  } finally {
    ended = true; clearTimeout(timer); clearTimeout(deadline);
    if (frame !== undefined) cancelAnimationFrame(frame);
    signal.removeEventListener("abort", abort);
  }
};
