import type { LibraryOrigin } from "./libraryController";
import { libraryError } from "./libraryController";
import type { CatalogPage, HistoryPage, HistoryQuery, ImportProgress } from "./leagueIndex";
import type { GameSummary } from "./types";

export type HistoryState = { page: HistoryPage | null; loading: boolean; indexing: boolean; error: string | null; indexError: string | null; laterPage: boolean };
type Context = { origin: LibraryOrigin; games: GameSummary[] };
type Dependencies = {
  current: (origin: LibraryOrigin) => boolean;
  catalog: (token: string, after: string | null) => Promise<CatalogPage>;
  recording: (token: string, game: string) => Promise<ImportProgress>;
  history: (token: string, query: HistoryQuery) => Promise<HistoryPage>;
};
const key = (origin: LibraryOrigin) => JSON.stringify(origin);

/** One idle import and one short query at a time, independent of media reads. */
export class HistoryController {
  private context: Context | null = null;
  private state: HistoryState = { page: null, loading: false, indexing: false, error: null, indexError: null, laterPage: false };
  private listeners = new Set<(state: HistoryState) => void>();
  private query: HistoryQuery = { limit: 50 };
  private queryGeneration = 0;
  private queryPending = false;
  private queryActive = false;
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
  setContext(origin: LibraryOrigin | null, games: GameSummary[] = []) {
    if (this.closed) return;
    if (origin && this.context && key(origin) === key(this.context.origin)) return;
    this.context = origin ? { origin, games } : null;
    clearTimeout(this.timer);
    this.pending.clear(); this.catalogAfter = null; this.catalogPending = origin !== null;
    this.query = { ...this.query, after: null }; this.queryGeneration++; this.queryPending = false;
    this.publish({ page: null, loading: origin !== null, indexing: origin !== null, error: null, indexError: null, laterPage: false });
    this.schedule();
  }
  setQuery(query: HistoryQuery) {
    this.query = { ...query, after: null, limit: 50 };
    this.publish({ page: null, laterPage: false });
    this.requestQuery();
  }
  nextPage() {
    if (!this.state.page?.next || this.state.loading) return;
    this.query = { ...this.query, after: this.state.page.next };
    this.publish({ laterPage: true }); this.requestQuery();
  }
  firstPage() { this.query = { ...this.query, after: null }; this.publish({ laterPage: false }); this.requestQuery(); }
  private requestQuery() {
    this.queryGeneration++; this.queryPending = true;
    this.publish({ loading: true }); this.pumpQuery();
  }
  private pumpQuery() {
    const context = this.context;
    if (!context || !this.current(context) || this.queryActive || !this.queryPending) return;
    this.queryPending = false; this.queryActive = true;
    const generation = this.queryGeneration, query = this.query;
    void this.api.history(context.origin.token, query).then(page => {
      if (!this.current(context) || generation !== this.queryGeneration) return;
      // SQL results cannot admit new media or overwrite the accepted scan.
      for (const row of page.rows) if (!context.games.some(game => game.timestamp === row.game.timestamp && game.media_id === row.media_id)) throw new Error("History recording changed since the scan");
      this.publish({ page, loading: false, error: null });
    }).catch(error => {
      if (this.current(context) && generation === this.queryGeneration) this.publish({ loading: false, error: libraryError(error) });
    }).finally(() => { this.queryActive = false; this.pumpQuery(); });
  }
  private schedule() {
    if (this.closed || this.importActive || !this.context || (!this.catalogPending && !this.pending.size)) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.pumpImport(), 100);
  }
  private pumpImport() {
    const context = this.context;
    if (!context || !this.current(context) || this.importActive) return;
    this.importActive = true;
    const catalog = this.catalogPending;
    const next = this.pending.entries().next().value as [string, string] | undefined;
    const work = catalog ? this.api.catalog(context.origin.token, this.catalogAfter) : next ? this.api.recording(context.origin.token, next[0]) : null;
    if (!work) { this.importActive = false; return; }
    void work.then(result => {
      if (!this.current(context)) return;
      if (catalog) {
        const page = result as CatalogPage;
        if (page.snapshot_token !== context.origin.token) throw new Error("Stale index page");
        this.catalogAfter = page.next; this.catalogPending = page.next !== null;
        for (const item of page.items) {
          if (item.error) this.publish({ indexError: "Some recordings have unavailable League data." });
          if (item.progress?.status === "pending" && item.media_id) this.pending.set(item.game_timestamp, item.media_id);
        }
      } else if (next) {
        this.pending.delete(next[0]);
        const progress = result as ImportProgress;
        if (progress.media_id !== next[1]) throw new Error("Index recording identity changed");
        if (progress.status === "pending") this.pending.set(next[0], next[1]);
      }
      this.publish({ indexing: this.catalogPending || this.pending.size > 0 });
      this.requestQuery();
    }).catch(error => {
      if (this.current(context)) {
        // Retry requires a new accepted refresh, avoiding an idle failure loop.
        this.catalogPending = false; this.pending.clear();
        this.publish({ indexing: false, loading: false, indexError: libraryError(error) });
      }
    }).finally(() => { this.importActive = false; this.schedule(); });
  }
  close() { this.closed = true; this.context = null; clearTimeout(this.timer); this.pending.clear(); this.listeners.clear(); }
}
