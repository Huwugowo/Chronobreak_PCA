import type { AppSettings, ClipDuration, ClipDurations, LibrarySnapshot } from "./types";

/** An action keeps this identity from selection through dispatch and completion. */
export type LibraryOrigin = Readonly<{
  root: string; rootEpoch: number; request: number; token: string; navigation: number;
}>;
export type LibrarySelection = { kind: "game" | "clip"; id: string; origin: LibraryOrigin };
export type DurationDisplay = ClipDuration | { state: "pending" };
export type LibraryState = {
  root: string | null;
  snapshot: LibrarySnapshot | null;
  request: number;
  refreshing: boolean;
  actionable: boolean;
  error: string | null;
  selectedGame: LibrarySelection | null;
  activeClip: string | null;
  deletion: LibrarySelection | null;
  durations: Readonly<Record<string, DurationDisplay>>;
  busy: string | null;
};
type Dependencies = {
  refresh: () => Promise<LibrarySnapshot>;
  durations: (token: string, ids: string[], retry: boolean) => Promise<ClipDurations>;
};
type DurationIntent = { origin: LibraryOrigin; epoch: number; ids: string[]; retry: boolean };
export type ReplayRead = <T>(work: (token: string) => Promise<T>) => Promise<T>;
type ReplayIntent = { run: () => Promise<void>; cancel: () => void };
export type OperationResult<T> =
  | { admitted: true; value: T }
  | { admitted: false; error?: string };

export const libraryError = (error: unknown): string => {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null) {
    const wire = error as { message?: unknown; code?: unknown };
    return String(wire.message ?? wire.code ?? "Library operation failed");
  }
  return String(error);
};

/**
 * Admission: root epoch + request generation + snapshot token (and live owner).
 * UI response: admission identity + navigation epoch.
 * Side effects: logical destination root only, even after A/B/A or refresh.
 * Never let the response gate suppress filesystem reconciliation.
 */
export class LibraryController {
  private state: LibraryState = {
    root: null, snapshot: null, request: 0, refreshing: false, actionable: false,
    error: null, selectedGame: null, activeClip: null, deletion: null, durations: {}, busy: null,
  };
  private listeners = new Set<(state: LibraryState) => void>();
  private closed = false;
  private rootEpoch = 0;
  private navigation = 0;
  private scanActive = false;
  private scanPending = false;
  private mutationActive = false;
  private durationActive: DurationIntent | null = null;
  private durationPending: DurationIntent | null = null;
  private durationEpoch = 0;
  private clipsVisible = false;
  private replayActive = false;
  private replayPending: ReplayIntent | null = null;
  private replayRequest = 0;

  constructor(private readonly api: Dependencies) {}
  get value(): LibraryState { return this.state; }
  subscribe(listener: (state: LibraryState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  private publish(patch: Partial<LibraryState>) {
    if (this.closed) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener(this.state);
  }
  origin(): LibraryOrigin | null {
    const { root, snapshot, request } = this.state;
    return root !== null && snapshot ? {
      root, rootEpoch: this.rootEpoch, request, token: snapshot.token, navigation: this.navigation,
    } : null;
  }
  mayStart(origin: LibraryOrigin): boolean {
    return !this.closed && this.state.actionable && origin.root === this.state.root &&
      origin.rootEpoch === this.rootEpoch && origin.request === this.state.request &&
      origin.token === this.state.snapshot?.token;
  }
  mayPublish(origin: LibraryOrigin): boolean {
    return this.mayStart(origin) && origin.navigation === this.navigation;
  }
  setRoot(root: string) {
    if (this.closed || root === this.state.root) return;
    this.rootEpoch++;
    this.supersedeReplay();
    this.navigation++;
    this.supersedeDurations();
    this.publish({ root, snapshot: null, selectedGame: null, activeClip: null, deletion: null,
      actionable: false, error: null });
    this.refresh();
  }
  refresh() {
    if (this.closed || this.state.root === null) return;
    this.scanPending = true;
    this.supersedeReplay();
    this.supersedeDurations();
    this.publish({ request: this.state.request + 1, refreshing: true,
      // Keep the complete cards visible, but stop admitting tokenized actions
      // while the backend has invalidated the old selected token.
      actionable: false, error: null });
    this.pumpRefresh();
  }
  /** Reconcile a completed export even when its UI response was superseded. */
  invalidateRoot(root: string) {
    if (!this.closed && this.state.root === root) this.refresh();
  }
  private pumpRefresh() {
    if (this.closed || this.scanActive || this.mutationActive || !this.scanPending) return;
    this.scanPending = false;
    this.scanActive = true;
    const request = this.state.request;
    const rootEpoch = this.rootEpoch;
    const current = () => !this.closed && request === this.state.request && rootEpoch === this.rootEpoch;
    void (async () => {
      try {
        const snapshot = await this.api.refresh();
        if (current()) {
          const activeClip = snapshot.clips.some(clip => clip.filename === this.state.activeClip)
            ? this.state.activeClip : null;
          const selectedGame = snapshot.games.some(game => game.timestamp === this.state.selectedGame?.id)
            ? this.state.selectedGame : null;
          this.publish({ snapshot, activeClip, selectedGame, deletion: null, durations: {},
            refreshing: false, actionable: true, error: null });
        }
      } catch (error) {
        if (current()) this.publish({ refreshing: false, actionable: false, error: libraryError(error) });
      } finally {
        this.scanActive = false;
        this.pumpRefresh();
      }
    })();
  }
  navigate(clipsVisible: boolean) {
    this.navigation++;
    this.supersedeReplay();
    this.clipsVisible = clipsVisible;
    this.supersedeDurations();
    this.publish({ activeClip: null, deletion: null });
  }
  selectGame(id: string) {
    const origin = this.origin();
    if (origin && this.mayStart(origin) && this.state.snapshot?.games.some(game => game.timestamp === id))
      this.publish({ selectedGame: { kind: "game", id, origin } });
  }
  selectClip(id: string | null) {
    if (id === null || this.state.snapshot?.clips.some(clip => clip.filename === id))
      this.publish({ activeClip: id });
  }
  selectDeletion(kind: "game" | "clip", id: string) {
    const origin = this.origin();
    const exists = kind === "game" ? this.state.snapshot?.games.some(game => game.timestamp === id)
      : this.state.snapshot?.clips.some(clip => clip.filename === id);
    if (origin && this.mayStart(origin) && exists) this.publish({ deletion: { kind, id, origin } });
  }
  clearDeletion() { this.publish({ deletion: null }); }

  /** Shared across viewer/exporter lifetimes: one active read and one latest intent. */
  readReplay<T>(origin: LibraryOrigin, game: string, work: (token: string) => Promise<T>): Promise<T> {
    const currentSelection = () => this.mayPublish(origin) &&
      this.state.snapshot?.games.some(item => item.timestamp === game);
    if (!currentSelection()) return Promise.reject(new Error("Replay selection became stale. Return to games."));
    const request = ++this.replayRequest;
    this.replayPending?.cancel();
    return new Promise<T>((resolve, reject) => {
      const stale = () => reject(new Error("Replay selection became stale. Return to games."));
      this.replayPending = {
        cancel: stale,
        run: async () => {
          if (!currentSelection() || request !== this.replayRequest) { stale(); return; }
          try {
            const result = await work(origin.token);
            if (currentSelection() && request === this.replayRequest) resolve(result);
            else stale();
          } catch (error) {
            if (currentSelection() && request === this.replayRequest) reject(error);
            else stale();
          }
        },
      };
      this.pumpReplay();
    });
  }
  private pumpReplay() {
    if (this.closed || this.replayActive || !this.replayPending) return;
    const intent = this.replayPending;
    this.replayPending = null;
    this.replayActive = true;
    void intent.run().finally(() => { this.replayActive = false; this.pumpReplay(); });
  }
  private supersedeReplay() {
    this.replayRequest++;
    this.replayPending?.cancel();
    this.replayPending = null;
  }

  async mutate<T>(origin: LibraryOrigin, id: string, work: (token: string) => Promise<T>,
    independentExport = false): Promise<OperationResult<T>> {
    if (!this.mayStart(origin) || (!independentExport && this.mutationActive))
      return { admitted: false, error: "The library changed. Refresh and select the item again." };
    if (!independentExport) {
      this.mutationActive = true;
      this.publish({ busy: id });
    }
    let value: T | undefined;
    let error: string | undefined;
    try { value = await work(origin.token); } catch (failure) { error = libraryError(failure); }
    const admitted = this.mayPublish(origin);
    if (!independentExport) {
      this.mutationActive = false;
      this.publish({ busy: null });
    }
    // Includes stale-token rejection and partial-error completion. M2 can reject
    // an old export response AFTER publishing its files into the captured root.
    if (!this.closed && origin.root === this.state.root) this.refresh();
    this.pumpRefresh();
    return admitted && error === undefined ? { admitted: true, value: value as T }
      : { admitted: false, ...(admitted && error !== undefined ? { error } : {}) };
  }

  async saveSettings(origin: LibraryOrigin, work: (token: string) => Promise<AppSettings>):
    Promise<OperationResult<AppSettings>> {
    if (!this.mayStart(origin) || this.mutationActive) return { admitted: false, error: "Refresh the library before changing settings." };
    this.mutationActive = true;
    this.publish({ busy: "settings" });
    try {
      const settings = await work(origin.token);
      const admitted = this.mayPublish(origin);
      // Persisted root publication is a real effect, independent of navigation.
      if (!this.closed) {
        if (settings.output_path === this.state.root) this.refresh();
        else this.setRoot(settings.output_path);
      }
      return admitted ? { admitted: true, value: settings } : { admitted: false, error: "Settings response became stale." };
    } catch (error) {
      const admitted = this.mayPublish(origin);
      if (!this.closed && origin.root === this.state.root) this.refresh();
      return { admitted: false, ...(admitted ? { error: libraryError(error) } : {}) };
    } finally {
      this.mutationActive = false;
      this.publish({ busy: null });
      this.pumpRefresh();
    }
  }

  private supersedeDurations() {
    this.durationEpoch++;
    this.durationPending = null;
    const durations = Object.fromEntries(Object.entries(this.state.durations).filter(([, item]) => item.state !== "pending"));
    this.publish({ durations });
  }
  requestDurations(ids: string[], retry = false, benchmark = false) {
    const origin = this.origin();
    if (!origin || !this.mayStart(origin) || (!this.clipsVisible && !benchmark)) return;
    const unique = [...new Set(ids)].slice(0, 8).filter(id => this.state.snapshot!.clips.some(clip => clip.filename === id));
    const eligible = unique.filter(id => {
      const cached = this.state.durations[id];
      return !cached || (retry && cached.state === "unavailable") || cached.state === "pending";
    });
    // Repeated render intents must not replace or duplicate the active batch.
    const active = this.durationActive;
    const needed = eligible.filter(id => !(active && active.epoch === this.durationEpoch &&
      active.origin.token === origin.token && active.ids.includes(id)));
    if (needed.length) {
      const existing = this.durationPending && this.durationPending.epoch === this.durationEpoch &&
        this.durationPending.origin.token === origin.token ? this.durationPending : null;
      this.durationPending = {
        origin, epoch: this.durationEpoch,
        ids: [...new Set([...(existing?.ids ?? []), ...needed])].slice(0, 8),
        retry: Boolean(retry || existing?.retry),
      };
    }
    this.pumpDurations();
  }

  /**
   * Benchmark-only compatibility path for the historical clips-ready gate.
   * Batches are issued sequentially through the same eight-ID API, so this
   * never creates a duration queue or competes with production view work.
   */
  async drainDurations(ids: string[]): Promise<boolean> {
    const origin = this.origin();
    if (!origin || !this.mayStart(origin) || this.durationActive || this.durationPending) return false;
    const unique = [...new Set(ids)].filter(id => this.state.snapshot?.clips.some(clip => clip.filename === id));
    for (let offset = 0; offset < unique.length; offset += 8) {
      if (!this.mayStart(origin)) return false;
      const batch = unique.slice(offset, offset + 8);
      this.publish({ durations: { ...this.state.durations,
        ...Object.fromEntries(batch.map(id => [id, { state: "pending" as const }])) } });
      let results: ClipDurations["clips"];
      try {
        const response = await this.api.durations(origin.token, batch, false);
        if (response.snapshot_token !== origin.token) throw new Error("Duration token mismatch");
        results = response.clips;
      } catch {
        results = batch.map(clip_id => ({ clip_id, duration: { state: "unavailable" } }));
      }
      if (!this.mayStart(origin)) return false;
      const durations = { ...this.state.durations };
      for (const id of batch) durations[id] = results.find(item => item.clip_id === id)?.duration ?? { state: "unavailable" };
      this.publish({ durations });
    }
    return true;
  }
  private pumpDurations() {
    if (this.closed || this.durationActive || !this.durationPending) return;
    const intent = this.durationPending;
    this.durationPending = null;
    if (!this.mayStart(intent.origin) || intent.epoch !== this.durationEpoch) return;
    this.durationActive = intent;
    this.publish({ durations: { ...this.state.durations, ...Object.fromEntries(intent.ids.map(id => [id, { state: "pending" }])) } });
    void (async () => {
      let results: ClipDurations["clips"];
      try {
        const response = await this.api.durations(intent.origin.token, intent.ids, intent.retry);
        if (response.snapshot_token !== intent.origin.token) throw new Error("Duration token mismatch");
        results = response.clips;
      } catch {
        results = intent.ids.map(clip_id => ({ clip_id, duration: { state: "unavailable" } }));
      }
      if (this.mayStart(intent.origin) && intent.epoch === this.durationEpoch) {
        const durations = { ...this.state.durations };
        for (const id of intent.ids) durations[id] = results.find(item => item.clip_id === id)?.duration ?? { state: "unavailable" };
        this.publish({ durations });
      }
      this.durationActive = null;
      this.pumpDurations();
    })();
  }
  dispose() {
    this.closed = true;
    this.supersedeReplay();
    this.scanPending = false;
    this.durationPending = null;
    this.listeners.clear();
    // In-flight invokes still settle; backend workers/children retain ownership.
  }
}
