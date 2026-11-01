import { compareOverlayCursor, type OverlayPage, type OverlayQuery, type OverlaySample, type ReplayIdentity, type ScoreboardFacts, type SourceScope } from "./leagueIndex";
import { participantKey, sameSource } from "./leagueSource";
import type { InitialSpellLoadout, ParticipantSpellAssets } from "./replayProjection";
import type { ReplayRead } from "./libraryController";
import type { ReplayTick } from "./replayTime";
import { scheduleScoreboardWork } from "./scoreboardPreload";

type Dependencies = { gameTimestamp: string; replayEnd: ReplayTick; readReplay: ReplayRead;
  queryOverlayPage: (token: string, game: string, query: OverlayQuery) => Promise<OverlayPage> };
export type OverlayState = { scope: SourceScope | null; identities: ReplayIdentity[]; spellAssets: ParticipantSpellAssets; initialLoadout: InitialSpellLoadout | null; sampleCount: number; bytes: number;
  status: "idle" | "loading" | "ready" | "error"; error: string | null;
  scoreboardStatus: "idle" | "loading" | "ready" | "error"; scoreboardVersion: number; scoreboardItemIds: readonly number[] };
export type ScoreboardSnapshot = ReadonlyMap<number, ScoreboardFacts>;
const MAX_SAMPLES = 131_072;
const MAX_BYTES = 64 * 1024 * 1024;

/** One recorded source, one bounded page in flight, no seek-triggered reads. */
export class ReplayOverlayController {
  private state: OverlayState = { scope: null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: "idle", error: null, scoreboardStatus: "idle", scoreboardVersion: 0, scoreboardItemIds: [] };
  private assets = new Map<string, [Set<string>, Set<string>]>();
  private samples: OverlaySample[] = [];
  private listeners = new Set<(state: OverlayState) => void>();
  private generation = 0;
  private active = false;
  private pending: { scope: SourceScope; generation: number } | null = null;
  private closed = false;
  private scoreboardOpen = false;
  private scoreboardRequested = false;
  private scoreboardBusy = false;
  private scoreboardResumeAt = 0;
  private cancelScoreboardWork: (() => void) | null = null;
  private scoreboardItems = new Set<number>();
  private revision: OverlayPage["revision"] | null = null;
  private scoreboardOffset = 0;
  private scoreboardBytes = 0;
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
    this.cancelScoreboardWork?.(); this.cancelScoreboardWork = null;
    this.scoreboardRequested = this.scoreboardOpen;
    this.scoreboardItems.clear();
    this.samples = [];
    this.assets.clear();
    this.revision = null; this.scoreboardOffset = 0; this.scoreboardBytes = 0;
    this.scoreboards.clear(); this.scoreboardPayloads.clear(); this.scoreboardVariants.clear();
    this.pending = scope ? { scope, generation } : null;
    this.publish({ scope: scope ?? null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: scope ? "loading" : "idle", error: null, scoreboardStatus: "idle", scoreboardVersion: 0, scoreboardItemIds: [] });
    this.pump();
  }
  retry() { if (this.state.scope) this.select(this.state.scope, true); }
  /** First open opts this source into background enrichment, also while closed. */
  setScoreboardOpen(open: boolean) { this.scoreboardOpen = open; this.scoreboardRequested ||= open; this.pump(); }
  setScoreboardBusy(busy: boolean) {
    if (busy === this.scoreboardBusy) return;
    this.scoreboardBusy = busy;
    this.cancelScoreboardWork?.(); this.cancelScoreboardWork = null;
    if (!busy) { this.scoreboardResumeAt = Date.now() + 150; this.pump(); }
  }
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
    if (this.closed || this.active) return;
    if (!this.pending) {
      if (this.scoreboardRequested && !this.scoreboardBusy && !this.cancelScoreboardWork && this.state.scope && this.state.status === "ready"
        && (this.state.scoreboardStatus === "idle" || this.state.scoreboardStatus === "loading")) {
        this.cancelScoreboardWork = scheduleScoreboardWork(() => {
          this.cancelScoreboardWork = null;
          void this.loadScoreboard(this.state.scope!, this.generation);
        }, Math.max(0, this.scoreboardResumeAt - Date.now()));
      }
      return;
    }
    const { scope, generation } = this.pending;
    this.pending = null;
    this.active = true;
    const current = () => !this.closed && generation === this.generation;
    void (async () => {
      let after: OverlayQuery["after"] = null, revision: OverlayQuery["revision"] = null;
      try {
        do {
          const page = await this.api.readReplay(token => this.api.queryOverlayPage(token, this.api.gameTimestamp,
            { scope: scope.source, after, revision, limit: 128 }), "overlay");
          if (!current()) return;
          if (!sameSource(page.source, scope.source) || page.local_participant !== scope.local_participant)
            throw new Error("Overlay source identity changed");
          if (revision && JSON.stringify(revision) !== JSON.stringify(page.revision)) throw new Error("Overlay revision changed");
          if (page.samples.length > 128) throw new Error("Overlay page capacity");
          let previous = after;
          for (const sample of page.samples) {
            if (sample.replay_tick > this.api.replayEnd || (previous && compareOverlayCursor(sample, previous) <= 0))
              throw new Error("Overlay sample order or media coverage");
            previous = sample;
          }
          if (page.next && (!previous || !page.samples.length || compareOverlayCursor(page.next, previous) !== 0))
            throw new Error("Overlay continuation did not advance");
          const bytes = this.state.bytes + new TextEncoder().encode(JSON.stringify(page.samples)).length;
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
          this.revision = revision;
          after = page.next;
          const first = this.samples[0];
          this.publish({ identities: page.identities ?? this.state.identities,
            spellAssets: assetsChanged ? new Map([...this.assets].map(([key, slots]) => [key, [[...slots[0]], [...slots[1]]] as const])) : this.state.spellAssets,
            initialLoadout: !after && first ? { replay_tick: first.replay_tick,
              players: first.players.map(({ participant, spells }) => ({ participant, spells })) } : null,
            sampleCount: this.samples.length, bytes,
            status: after ? "loading" : "ready" });
        } while (after && current());
      } catch (error) {
        if (current()) {
          this.samples = [];
          this.publish({ initialLoadout: null, sampleCount: 0, bytes: 0, status: "error", error: error instanceof Error ? error.message : String(error) });
        }
      } finally {
        this.active = false;
        this.pump();
      }
    })();
  }
  private async loadScoreboard(scope: SourceScope, generation: number) {
    this.active = true;
    const current = () => !this.closed && generation === this.generation;
    this.publish({ scoreboardStatus: "loading" });
    try {
      const previous = this.samples[this.scoreboardOffset - 1];
      const page = await this.api.readReplay(token => this.api.queryOverlayPage(token, this.api.gameTimestamp,
        { scope: scope.source, after: previous ? { replay_tick: previous.replay_tick, observation_id: previous.observation_id } : null,
          revision: previous ? this.revision : null, limit: 32, include_scoreboard: true }), "overlay");
      if (!current()) return;
      if (!sameSource(page.source, scope.source) || page.local_participant !== scope.local_participant
        || JSON.stringify(page.revision) !== JSON.stringify(this.revision)) throw new Error("Scoreboard source revision changed");
      if (page.samples.length > 32) throw new Error("Scoreboard page capacity");
      const bytes = this.scoreboardBytes + new TextEncoder().encode(JSON.stringify(page.samples)).length;
      if (bytes > MAX_BYTES) throw new Error("Recorded scoreboard exceeds its memory capacity");
      for (const sample of page.samples) {
        const base = this.samples[this.scoreboardOffset];
        if (!base || compareOverlayCursor(sample, base) !== 0 || sample.players.length !== base.players.length)
          throw new Error("Scoreboard observation changed");
        const facts = new Map<number, ScoreboardFacts>();
        for (const player of sample.players) {
          if (!player.scoreboard || !base.players.some(p => p.participant === player.participant)) throw new Error("Scoreboard participant changed");
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
        this.scoreboardOffset++;
      }
      const last = page.samples[page.samples.length - 1];
      if (page.next ? !last || compareOverlayCursor(page.next, last) !== 0 || this.scoreboardOffset >= this.samples.length
        : this.scoreboardOffset !== this.samples.length) throw new Error("Scoreboard continuation changed");
      this.scoreboardBytes = bytes;
      this.publish({ scoreboardStatus: page.next ? "loading" : "ready", scoreboardVersion: this.state.scoreboardVersion + 1,
        scoreboardItemIds: this.scoreboardItems.size === this.state.scoreboardItemIds.length ? this.state.scoreboardItemIds : [...this.scoreboardItems] });
    } catch {
      if (current()) {
        this.scoreboards.clear(); this.scoreboardPayloads.clear(); this.scoreboardVariants.clear();
        this.scoreboardItems.clear();
        this.publish({ scoreboardStatus: "error", scoreboardVersion: this.state.scoreboardVersion + 1, scoreboardItemIds: [] });
      }
    } finally { this.active = false; this.pump(); }
  }
  dispose() {
    this.cancelScoreboardWork?.(); this.cancelScoreboardWork = null; this.scoreboardItems.clear();
    this.closed = true; this.generation++; this.pending = null; this.samples = []; this.assets.clear(); this.listeners.clear();
    this.scoreboards.clear(); this.scoreboardPayloads.clear(); this.scoreboardVariants.clear();
    this.state = { scope: null, identities: [], spellAssets: new Map(), initialLoadout: null, sampleCount: 0, bytes: 0, status: "idle", error: null, scoreboardStatus: "idle", scoreboardVersion: 0, scoreboardItemIds: [] };
  }
}
