import { compareOverlayCursor, type OverlayPage, type OverlayQuery, type OverlaySample, type ReplayIdentity, type ScoreboardFacts, type SourceScope } from "./leagueIndex";
import { participantKey, sameSource } from "./leagueSource";
import type { InitialSpellLoadout, ParticipantSpellAssets } from "./replayProjection";
import type { ReplayRead } from "./libraryController";
import type { ReplayTick } from "./replayTime";

type Dependencies = { gameTimestamp: string; replayEnd: ReplayTick; readReplay: ReplayRead;
  queryOverlayPage: (token: string, game: string, query: OverlayQuery) => Promise<OverlayPage> };
export type OverlayState = { scope: SourceScope | null; identities: ReplayIdentity[]; spellAssets: ParticipantSpellAssets; initialLoadout: InitialSpellLoadout | null; sampleCount: number; bytes: number;
  status: "idle" | "loading" | "ready" | "error"; error: string | null;
  scoreboardStatus: "idle" | "loading" | "ready" | "error"; scoreboardItemIds: readonly number[] };
export type ScoreboardSnapshot = ReadonlyMap<number, ScoreboardFacts>;
const MAX_SAMPLES = 131_072;
const MAX_BYTES = 64 * 1024 * 1024;
const PAGE_SIZE = 128;
const encodedSize = (samples: OverlaySample[]) => new TextEncoder().encode(JSON.stringify(samples)).length;

/** One recorded source, one bounded page in flight, no seek-triggered reads. */
export class ReplayOverlayController {
  private state: OverlayState = { scope: null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: "idle", error: null, scoreboardStatus: "idle", scoreboardItemIds: [] };
  private assets = new Map<string, [Set<string>, Set<string>]>();
  private samples: OverlaySample[] = [];
  private listeners = new Set<(state: OverlayState) => void>();
  private generation = 0;
  private active = false;
  private pending: { scope: SourceScope; generation: number } | null = null;
  private closed = false;
  private scoreboardItems = new Set<number>();
  private scoreboards = new Map<number, ScoreboardSnapshot>();
  private scoreboardPayloads = new Map<number, ScoreboardFacts>();
  private scoreboardVariants = new Map<string, ScoreboardSnapshot>();
  constructor(private readonly api: Dependencies) {}
  get value() { return this.state; }
  subscribe(listener: (state: OverlayState) => void) { this.listeners.add(listener); return () => this.listeners.delete(listener); }
  private publish(patch: Partial<OverlayState>) {
    if (this.closed) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener(this.state);
  }
  select(scope: SourceScope | undefined, restart = false) {
    if (this.closed || (!restart && scope && this.state.scope && sameSource(scope.source, this.state.scope.source)
      && scope.local_participant === this.state.scope.local_participant)) return;
    const generation = ++this.generation;
    this.samples = [];
    this.assets.clear();
    this.clearScoreboards();
    this.pending = scope ? { scope, generation } : null;
    this.publish({ scope: scope ?? null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: scope ? "loading" : "idle", error: null, scoreboardStatus: scope ? "loading" : "idle", scoreboardItemIds: [] });
    this.pump();
  }
  retry() { if (this.state.scope) this.select(this.state.scope, true); }
  scoreboardFor(sample: OverlaySample | null): ScoreboardSnapshot | null {
    return sample && this.state.scoreboardStatus !== "error" ? this.scoreboards.get(sample.observation_id) ?? null : null;
  }
  /** Pending pages do not prove the final sample at their last tick (ties can split). */
  sampleAt(position: ReplayTick | null): OverlaySample | null {
    if (position === null || this.state.error || !this.samples.length) return null;
    if (this.state.status !== "ready" && position >= this.samples[this.samples.length - 1].replay_tick) return null;
    let low = 0, high = this.samples.length;
    while (low < high) {
      const mid = low + Math.floor((high - low) / 2);
      if (this.samples[mid].replay_tick <= position) low = mid + 1;
      else high = mid;
    }
    return this.samples[low - 1] ?? null;
  }
  private pump() {
    if (this.closed || this.active || !this.pending) return;
    const { scope, generation } = this.pending;
    this.pending = null;
    this.active = true;
    const current = () => !this.closed && generation === this.generation;
    void (async () => {
      let after: OverlayQuery["after"] = null, revision: OverlayQuery["revision"] = null;
      let includeScoreboard = true, scoreboardBytes = 0;
      try {
        do {
          const readPage = () => this.api.readReplay(token => this.api.queryOverlayPage(token, this.api.gameTimestamp,
            { scope: scope.source, after, revision, limit: PAGE_SIZE, include_scoreboard: includeScoreboard }), "overlay");
          let page: OverlayPage;
          try {
            page = await readPage();
          } catch (error) {
            if (!current()) return;
            if (!includeScoreboard) throw error;
            // Preserve the compact overlay when optional scoreboard facts cannot be read.
            includeScoreboard = false;
            this.clearScoreboards();
            page = await readPage();
          }
          if (!current()) return;
          if (!sameSource(page.source, scope.source) || page.local_participant !== scope.local_participant)
            throw new Error("Overlay source identity changed");
          if (revision && JSON.stringify(revision) !== JSON.stringify(page.revision)) throw new Error("Overlay revision changed");
          if (page.samples.length > PAGE_SIZE) throw new Error("Overlay page capacity");
          let previous = after;
          for (const sample of page.samples) {
            if (sample.replay_tick > this.api.replayEnd || (previous && compareOverlayCursor(sample, previous) <= 0))
              throw new Error("Overlay sample order or media coverage");
            previous = sample;
          }
          if (page.next && (!previous || !page.samples.length || compareOverlayCursor(page.next, previous) !== 0))
            throw new Error("Overlay continuation did not advance");
          if (includeScoreboard) {
            scoreboardBytes += encodedSize(page.samples);
            includeScoreboard = scoreboardBytes <= MAX_BYTES && this.storeScoreboards(page.samples);
            if (!includeScoreboard) this.clearScoreboards();
            // Keep only the compact observations and interned scoreboard facts.
            for (const sample of page.samples) for (const player of sample.players) delete player.scoreboard;
          }
          const bytes = this.state.bytes + encodedSize(page.samples);
          if (bytes > MAX_BYTES || this.samples.length + page.samples.length > MAX_SAMPLES) throw new Error("Recorded overlay exceeds its memory capacity");
          this.samples.push(...page.samples);
          let assetsChanged = false;
          for (const sample of page.samples) for (const player of sample.players) {
            const key = participantKey({ source: scope.source, participant: player.participant });
            let slots = this.assets.get(key);
            if (!slots) { slots = [new Set(), new Set()]; this.assets.set(key, slots); }
            for (const slot of [0, 1] as const) {
              const label = player.spells[slot];
              if (label && !slots[slot].has(label)) { slots[slot].add(label); assetsChanged = true; }
            }
          }
          revision = page.revision;
          after = page.next;
          const first = this.samples[0];
          this.publish({ identities: page.identities ?? this.state.identities,
            spellAssets: assetsChanged ? new Map([...this.assets].map(([key, slots]) => [key, [[...slots[0]], [...slots[1]]] as const])) : this.state.spellAssets,
            initialLoadout: !after && first ? { replay_tick: first.replay_tick,
              players: first.players.map(({ participant, spells }) => ({ participant, spells })) } : null,
            sampleCount: this.samples.length, bytes,
            status: after ? "loading" : "ready",
            scoreboardStatus: includeScoreboard ? (after ? "loading" : "ready") : "error",
            scoreboardItemIds: this.scoreboardItems.size === this.state.scoreboardItemIds.length
              ? this.state.scoreboardItemIds : [...this.scoreboardItems] });
        } while (after && current());
      } catch (error) {
        if (current()) {
          this.samples = [];
          this.clearScoreboards();
          this.publish({ initialLoadout: null, sampleCount: 0, bytes: 0, status: "error", scoreboardStatus: "error",
            scoreboardItemIds: [], error: error instanceof Error ? error.message : String(error) });
        }
      } finally {
        this.active = false;
        this.pump();
      }
    })();
  }
  private storeScoreboards(samples: OverlaySample[]): boolean {
    for (const sample of samples) {
      const facts = new Map<number, ScoreboardFacts>();
      for (const player of sample.players) {
        if (!player.scoreboard) return false;
        const id = player.scoreboard.payload_id;
        if (!this.scoreboardPayloads.has(id)) {
          this.scoreboardPayloads.set(id, player.scoreboard);
          for (const item of player.scoreboard.items ?? []) {
            if (item.item_id && this.scoreboardItems.size < 512) this.scoreboardItems.add(item.item_id);
          }
        }
        facts.set(player.participant, this.scoreboardPayloads.get(id)!);
      }
      const key = [...facts].map(([participant, f]) => `${participant}:${f.payload_id}`).join(",");
      if (!this.scoreboardVariants.has(key)) this.scoreboardVariants.set(key, facts);
      this.scoreboards.set(sample.observation_id, this.scoreboardVariants.get(key)!);
    }
    return true;
  }
  private clearScoreboards() {
    this.scoreboards.clear(); this.scoreboardPayloads.clear(); this.scoreboardVariants.clear();
    this.scoreboardItems.clear();
  }
  dispose() {
    this.closed = true; this.generation++; this.pending = null; this.samples = []; this.assets.clear(); this.listeners.clear();
    this.clearScoreboards();
    this.state = { scope: null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: "idle", error: null, scoreboardStatus: "idle", scoreboardItemIds: [] };
  }
}
