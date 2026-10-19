import type { ReplayRead } from "./libraryController";
import type { SourceScope, RecordingFacts, ReplayWindow, ReplayWindowQuery } from "./leagueIndex";
import type { MediaId } from "./replayTime";
import { sameSource, sourceKey } from "./leagueSource";

/** The bounded replay projection returned by the M3 IPC contract. */
export type { RecordingFacts, ReplayCursor, ReplayRoster, ReplayWindow } from "./leagueIndex";
export type ReplayWindowRequest = ReplayWindowQuery;
export type ImportProgress = { status: "pending" | "ready" | "unavailable" | string };

type Dependencies = {
  gameTimestamp: string;
  mediaId: MediaId;
  readReplay: ReplayRead;
  loadRecordingFacts: (token: string, gameTimestamp: string, mediaId: MediaId) => Promise<RecordingFacts>;
  indexRecording: (token: string, gameTimestamp: string) => Promise<ImportProgress>;
  sourceScopes: (token: string, gameTimestamp: string) => Promise<SourceScope[]>;
  queryReplayWindow: (token: string, gameTimestamp: string, query: ReplayWindowRequest) => Promise<ReplayWindow>;
  setTimeout?: (callback: () => void, delay: number) => ReturnType<typeof setTimeout>;
  clearTimeout?: (handle: ReturnType<typeof setTimeout>) => void;
};

export type ReplayDataState = {
  facts: RecordingFacts | null;
  scopes: SourceScope[];
  windows: ReplayWindow[];
  activeWindow: ReplayWindow | null;
  indexing: "idle" | "pending" | "ready" | "unavailable";
  loading: { facts: boolean; index: boolean; scopes: boolean; window: boolean };
  error: string | null;
  factsError: string | null;
  windowError: string | null;
};

type WindowIntent = { id: number; query: ReplayWindowRequest };
const PAGE_SIZE = 512;
const CACHE_WINDOWS = 4;
const IMPORT_PAUSE_MS = 100;
const VIEWPORT_DEBOUNCE_MS = 120;

const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);
const cursorKey = (cursor: ReplayWindowQuery["after"]) => cursor ? `${cursor.replay_tick}:${cursor.source_id}` : "first";
const windowKey = (query: ReplayWindowRequest) =>
  `${sourceKey(query.scope)}:${query.start}:${query.end}:${query.at}:${cursorKey(query.after)}`;

/**
 * A selected-recording reader. It deliberately retains pages, rather than a
 * synthetic all-sources timeline: callers choose a source epoch and ask for
 * another page when `activeWindow.next` is non-null.
 */
export class ReplayDataController {
  private state: ReplayDataState = {
    facts: null, scopes: [], windows: [], activeWindow: null, indexing: "idle",
    loading: { facts: false, index: false, scopes: false, window: false },
    error: null, factsError: null, windowError: null,
  };
  private listeners = new Set<(state: ReplayDataState) => void>();
  private closed = false;
  private initPromise: Promise<void> | null = null;
  private initGeneration = 0;
  private windowGeneration = 0;
  private windowActive = false;
  private windowPending: WindowIntent | null = null;
  private deferredWindow: WindowIntent | null = null;
  private latestWindow: Omit<ReplayWindowRequest, "after" | "limit"> | null = null;
  private debounce: ReturnType<typeof setTimeout> | null = null;
  private importPause: { timer: ReturnType<typeof setTimeout>; resolve: () => void } | null = null;
  private readonly cachedWindowKeys = new WeakMap<ReplayWindow, string>();
  private readonly cachedWindowQueries = new WeakMap<ReplayWindow, ReplayWindowRequest>();
  private readonly schedule: NonNullable<Dependencies["setTimeout"]>;
  private readonly cancelSchedule: NonNullable<Dependencies["clearTimeout"]>;

  constructor(private readonly api: Dependencies) {
    this.schedule = api.setTimeout ?? setTimeout;
    this.cancelSchedule = api.clearTimeout ?? clearTimeout;
  }

  get value(): ReplayDataState { return this.state; }
  subscribe(listener: (state: ReplayDataState) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  private publish(patch: Partial<ReplayDataState>) {
    if (this.closed) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener(this.state);
  }
  private setLoading(key: keyof ReplayDataState["loading"], value: boolean) {
    this.publish({ loading: { ...this.state.loading, [key]: value } });
  }
  private currentInit(generation: number) { return !this.closed && generation === this.initGeneration; }
  private currentWindow(intent: WindowIntent) {
    return !this.closed && intent.id === this.windowGeneration;
  }
  private async read<T>(work: (token: string) => Promise<T>): Promise<T> { return this.api.readReplay(work); }

  initialize(): Promise<void> {
    if (this.closed) return Promise.resolve();
    if (this.initPromise) return this.initPromise;
    const generation = ++this.initGeneration;
    this.initPromise = this.runInitialization(generation).finally(() => {
      if (this.currentInit(generation)) this.initPromise = null;
    });
    return this.initPromise;
  }

  /** Explicit retry only; background failures never restart their own IPC work. */
  retry(): Promise<void> {
    if (this.initPromise) return this.initPromise;
    this.state = { ...this.state, indexing: "idle", error: null, factsError: null, windowError: null };
    return this.initialize().then(() => {
      if (!this.closed && this.latestWindow) this.requestWindow(this.latestWindow);
    });
  }

  private async runInitialization(generation: number) {
    this.publish({ error: null });
    this.setLoading("facts", true);
    try {
      const facts = await this.read(token => this.api.loadRecordingFacts(token, this.api.gameTimestamp, this.api.mediaId));
      if (this.currentInit(generation)) this.publish({ facts, factsError: null });
    } catch (error) {
      if (this.currentInit(generation)) {
        const message = errorText(error);
        this.publish({ factsError: message });
      }
    } finally {
      if (this.currentInit(generation)) this.setLoading("facts", false);
    }
    if (!this.currentInit(generation)) return;

    this.setLoading("index", true);
    this.publish({ indexing: "pending" });
    try {
      // A pending import is deliberately resumed at a human-scale cadence.
      for (;;) {
        const progress = await this.read(token => this.api.indexRecording(token, this.api.gameTimestamp));
        if (!this.currentInit(generation)) return;
        if (progress.status === "ready") break;
        if (progress.status === "unavailable") { this.publish({ indexing: "unavailable" }); return; }
        await this.pause(IMPORT_PAUSE_MS);
        if (!this.currentInit(generation)) return;
      }
    } catch (error) {
      if (this.currentInit(generation)) this.publish({ indexing: "unavailable", error: errorText(error) });
      return;
    } finally {
      if (this.currentInit(generation)) this.setLoading("index", false);
    }
    if (!this.currentInit(generation)) return;

    this.setLoading("scopes", true);
    try {
      const scopes = await this.read(token => this.api.sourceScopes(token, this.api.gameTimestamp));
      if (!this.currentInit(generation)) return;
      if (scopes.some(scope => scope.source.media_id !== this.api.mediaId)) throw new Error("Replay source media changed");
      this.publish({ scopes, indexing: "ready", error: null });
      this.armDeferredWindow();
    } catch (error) {
      if (this.currentInit(generation)) this.publish({ error: errorText(error) });
    } finally {
      if (this.currentInit(generation)) this.setLoading("scopes", false);
    }
  }

  private pause(delay: number): Promise<void> {
    return new Promise(resolve => {
      const timer = this.schedule(() => { this.importPause = null; resolve(); }, delay);
      this.importPause = { timer, resolve };
    });
  }

  requestWindow(query: Omit<ReplayWindowRequest, "after" | "limit">): void {
    if (this.closed) return;
    this.latestWindow = query;
    const intent = { id: ++this.windowGeneration, query: { ...query, after: null, limit: PAGE_SIZE } };
    this.windowPending = null;
    if (this.debounce !== null) this.cancelSchedule(this.debounce);
    this.debounce = null;
    this.deferredWindow = intent;
    if (!this.state.scopes.some(scope => sourceKey(scope.source) === sourceKey(query.scope))) return;
    this.armDeferredWindow();
  }

  /** Move to the next bounded page. The preceding page stays in the four-page cache. */
  loadMore(): void {
    const current = this.state.activeWindow;
    if (this.closed || !current?.next) return;
    const query = this.cachedWindowQueries.get(current) ?? this.windowPending?.query ?? this.deferredWindow?.query;
    if (!query) return;
    const intent = { id: ++this.windowGeneration, query: { ...query, after: current.next, limit: PAGE_SIZE } };
    this.windowPending = null;
    this.deferredWindow = intent;
    this.armDeferredWindow();
  }

  private armDeferredWindow() {
    if (this.closed || !this.deferredWindow || !this.state.scopes.some(scope =>
      sourceKey(scope.source) === sourceKey(this.deferredWindow!.query.scope))) return;
    if (this.debounce !== null) this.cancelSchedule(this.debounce);
    this.debounce = this.schedule(() => {
      this.debounce = null;
      const intent = this.deferredWindow;
      this.deferredWindow = null;
      if (!intent || !this.currentWindow(intent) || !this.state.scopes.some(scope =>
        sameSource(scope.source, intent.query.scope))) return;
      this.windowPending = intent;
      this.pumpWindow();
    }, VIEWPORT_DEBOUNCE_MS);
  }

  private pumpWindow() {
    if (this.closed || this.windowActive || !this.windowPending) return;
    const intent = this.windowPending;
    this.windowPending = null;
    if (!this.currentWindow(intent)) return;
    this.windowActive = true;
    this.setLoading("window", true);
    void (async () => {
      try {
        const window = await this.read(token => this.api.queryReplayWindow(token, this.api.gameTimestamp, intent.query));
        if (!this.currentWindow(intent)) return;
        if (!sameSource(window.source, intent.query.scope)) throw new Error("Replay window source changed");
        if (window.events.length > PAGE_SIZE) throw new Error("Replay window exceeded its 512-event contract");
        this.storeWindow(intent.query, window);
        this.publish({ activeWindow: window, windowError: null });
      } catch (error) {
        if (this.currentWindow(intent)) {
          const message = errorText(error);
          this.publish({ windowError: message });
        }
      } finally {
        this.windowActive = false;
        if (!this.closed && !this.windowPending) this.setLoading("window", false);
        this.pumpWindow();
      }
    })();
  }

  private storeWindow(query: ReplayWindowRequest, window: ReplayWindow) {
    const key = windowKey(query);
    this.cachedWindowKeys.set(window, key);
    this.cachedWindowQueries.set(window, query);
    const windows = [window, ...this.state.windows.filter(existing => this.cachedWindowKeys.get(existing) !== key)].slice(0, CACHE_WINDOWS);
    // Every entry is one 512-row page, so this is also a hard 2,048-event cap.
    if (windows.reduce((count, item) => count + item.events.length, 0) > CACHE_WINDOWS * PAGE_SIZE)
      throw new Error("Replay page cache capacity exceeded");
    this.publish({ windows });
  }

  dispose() {
    this.closed = true;
    this.initGeneration++;
    this.windowGeneration++;
    this.windowPending = null;
    this.deferredWindow = null;
    if (this.debounce !== null) this.cancelSchedule(this.debounce);
    this.debounce = null;
    if (this.importPause !== null) {
      this.cancelSchedule(this.importPause.timer);
      this.importPause.resolve();
      this.importPause = null;
    }
    this.listeners.clear();
  }
}
