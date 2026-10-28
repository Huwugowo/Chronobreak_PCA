import type { LibraryOrigin } from "./libraryController";
import { libraryError } from "./libraryController";
import type { CatalogPage, HistoryPage, HistoryQuery, ImportProgress } from "./leagueIndex";
import type { GameSummary } from "./types";

export type HistoryState = { page: HistoryPage | null; loading: boolean; indexing: boolean; initializing: boolean; error: string | null; indexError: string | null; laterPage: boolean };
export type HistoryQueryObservation = {
  kind: "history_query_scheduled" | "history_query_replaced" | "history_query_dispatched" |
    "history_query_completed" | "history_query_superseded" | "history_query_admitted" | "history_query_error";
  generation: number;
  actionId?: string;
  rowCount?: number;
};
type Context = { origin: LibraryOrigin; games: GameSummary[]; revision: number; queriedRevision: number };
type Dependencies = {
  current: (origin: LibraryOrigin) => boolean;
  catalog: (token: string, after: string | null, origin: LibraryOrigin) => Promise<CatalogPage>;
  recording: (token: string, game: string, origin: LibraryOrigin) => Promise<ImportProgress>;
  history: (token: string, query: HistoryQuery) => Promise<HistoryPage>;
  observeQuery?: (event: HistoryQueryObservation) => void;
};
const key = (origin: LibraryOrigin) => JSON.stringify(origin);

/** One idle import and one short query at a time, independent of media reads. */
export class HistoryController {
  private context: Context | null = null;
  private state: HistoryState = { page: null, loading: false, indexing: false, initializing: false, error: null, indexError: null, laterPage: false };
  private listeners = new Set<(state: HistoryState) => void>();
  private query: HistoryQuery = { limit: 50 };
  private queryGeneration = 0;
  private queryActionId: string | undefined;
  private queryPending = false;
  private queryActive = false;
  private queryDirty = false;
  private importActive = false;
  private catalogPending = false;
  private catalogAfter: string | null = null;
  private pending = new Map<string, string>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private closed = false;
  constructor(private readonly api: Dependencies) {}
  get value() { return this.state; }
  subscribe(listener: (state: HistoryState) => void) { this.listeners.add(listener); return () => this.listeners.delete(listener); }
  private publish(patch: Partial<HistoryState>) {
    if (this.closed) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener(this.state);
  }
  private current(context: Context) { return !this.closed && this.context === context && this.api.current(context.origin); }
  isQueryCurrent(generation: number) {
    return !!this.context && this.current(this.context) && generation === this.queryGeneration;
  }
  private observeQuery(kind: HistoryQueryObservation["kind"], generation: number,
    actionId: string | undefined, rowCount?: number) {
    this.api.observeQuery?.({ kind, generation, actionId, rowCount });
  }
  setContext(origin: LibraryOrigin | null, games: GameSummary[] = []) {
    if (this.closed) return;
    if (origin && this.context && key(origin) === key(this.context.origin)) return;
    if (this.queryPending) this.observeQuery("history_query_replaced", this.queryGeneration, this.queryActionId);
    this.context = origin ? { origin, games, revision: 0, queriedRevision: -1 } : null;
    clearTimeout(this.timer);
    this.pending.clear(); this.catalogAfter = null; this.catalogPending = origin !== null;
    this.query = { ...this.query, after: null }; this.queryGeneration++; this.queryPending = false; this.queryActionId = undefined; this.queryDirty = false;
    this.publish({ page: null, loading: origin !== null, indexing: origin !== null, initializing: origin !== null, error: null, indexError: null, laterPage: false });
    this.schedule();
  }
  setQuery(query: HistoryQuery, actionId?: string) {
    this.query = { ...query, after: null, limit: 50 };
    this.requestQuery(actionId);
  }
  nextPage() {
    if (!this.state.page?.next || this.state.loading || this.state.error) return;
    this.query = { ...this.query, after: this.state.page.next };
    this.requestQuery();
  }
  firstPage() {
    if (this.state.loading || this.state.error) return;
    this.query = { ...this.query, after: null }; this.requestQuery();
  }
  private requestQuery(actionId?: string) {
    if (this.queryPending) this.observeQuery("history_query_replaced", this.queryGeneration, this.queryActionId);
    this.queryActionId = actionId;
    this.queryGeneration++; this.queryPending = true;
    this.observeQuery("history_query_scheduled", this.queryGeneration, this.queryActionId);
    this.publish({ loading: true, error: null }); this.pumpQuery();
  }
  private refreshQuery() {
    if (!this.context || !this.current(this.context)) return;
    // Imports invalidate the data, not an outstanding foreground intent. One
    // coalesced refresh follows settlement; pending user input takes priority.
    if (this.queryActive) { this.queryDirty = true; return; }
    if (this.queryPending) this.pumpQuery();
    else this.requestQuery();
  }
  private pumpQuery() {
    const context = this.context;
    if (!context || !this.current(context) || this.catalogPending || this.queryActive || !this.queryPending) return;
    this.queryPending = false; this.queryActive = true; this.queryDirty = false;
    const generation = this.queryGeneration, query = this.query, actionId = this.queryActionId, revision = context.revision;
    this.observeQuery("history_query_dispatched", generation, actionId);
    void (async () => this.api.history(context.origin.token, query))().then(page => {
      this.observeQuery("history_query_completed", generation, actionId, page.rows.length);
      if (!this.current(context) || generation !== this.queryGeneration) {
        this.observeQuery("history_query_superseded", generation, actionId);
        return;
      }
      // SQL results cannot admit new media or overwrite the accepted scan.
      for (const row of page.rows) if (!context.games.some(game => game.timestamp === row.game.timestamp && game.media_id === row.media_id)) throw new Error("History recording changed since the scan");
      context.queriedRevision = revision;
      this.publish({ page, laterPage: !!query.after, loading: false, error: null });
      this.observeQuery("history_query_admitted", generation, actionId, page.rows.length);
    }).catch(error => {
      this.observeQuery("history_query_error", generation, actionId);
      if (this.current(context) && generation === this.queryGeneration) {
        context.queriedRevision = revision;
        this.publish({ loading: false, error: libraryError(error) });
      }
    }).finally(() => {
      this.queryActive = false;
      if (this.queryPending) this.pumpQuery();
      else if (this.queryDirty) this.refreshQuery();
      this.finishInitialization(context);
    });
  }
  private finishInitialization(context: Context) {
    if (this.current(context) && this.state.initializing && !this.importActive && !this.catalogPending && !this.pending.size
      && !this.queryActive && !this.queryPending && context.queriedRevision === context.revision)
      this.publish({ initializing: false });
  }
  private schedule() {
    if (this.closed || this.importActive || !this.context || (!this.catalogPending && !this.pending.size)) return;
    clearTimeout(this.timer);
    // Yield to input/rendering between bounded worker calls without adding
    // 100 ms for every four journal commits in a long recording.
    this.timer = setTimeout(() => this.pumpImport(), 4);
  }
  private pumpImport() {
    const context = this.context;
    if (!context || !this.current(context) || this.importActive) return;
    this.importActive = true;
    const catalog = this.catalogPending;
    const next = this.pending.entries().next().value as [string, string] | undefined;
    if (!catalog && !next) { this.importActive = false; return; }
    // Include synchronous adapter failure in the same settlement path.
    void (async (): Promise<CatalogPage | ImportProgress> => catalog ? this.api.catalog(context.origin.token, this.catalogAfter, context.origin)
      : this.api.recording(context.origin.token, next![0], context.origin))().then(result => {
      if (!this.current(context)) return;
      context.revision++;
      if (catalog) {
        const page = result as CatalogPage;
        if (page.snapshot_token !== context.origin.token) throw new Error("Stale index page");
        this.catalogAfter = page.next; this.catalogPending = page.next !== null;
        for (const item of page.items) {
          if (item.error || item.progress?.status === "unavailable") this.publish({ indexError: "Some recordings have unavailable League data." });
          if (item.progress?.status === "pending" && item.media_id) this.pending.set(item.game_timestamp, item.media_id);
        }
      } else if (next) {
        this.pending.delete(next[0]);
        const progress = result as ImportProgress;
        if (progress.media_id !== next[1]) throw new Error("Index recording identity changed");
        if (progress.status === "pending") this.pending.set(next[0], next[1]);
        if (progress.status === "unavailable") this.publish({ indexError: "Some recordings have unavailable League data." });
      }
      this.publish({ indexing: this.catalogPending || this.pending.size > 0 });
      // Preserve input queued before the catalog token existed. Recording
      // imports can continue independently once catalog admission completes.
      this.refreshQuery();
    }).catch(error => {
      if (this.current(context)) {
        // Retry requires a new accepted refresh, avoiding an idle failure loop.
        this.catalogPending = false; this.pending.clear();
        context.revision++;
        this.publish({ indexing: false, indexError: libraryError(error) });
        this.refreshQuery();
      }
    }).finally(() => { this.importActive = false; this.finishInitialization(context); this.schedule(); });
  }
  close() {
    if (this.queryPending) this.observeQuery("history_query_replaced", this.queryGeneration, this.queryActionId);
    this.closed = true; this.context = null; this.queryDirty = false; clearTimeout(this.timer); this.pending.clear(); this.listeners.clear();
  }
}
