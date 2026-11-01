import { For, Show, createEffect, createMemo, createSignal, onCleanup, untrack } from "solid-js";
import type { OverlaySample, ReplayIdentity, ScoreboardItem } from "../leagueIndex";
import type { ScoreboardSnapshot } from "../replayOverlayController";
import { recordedGameClock } from "../replayProjection";
import type { ParticipantSpellAssets } from "../replayProjection";
import { ScoreboardAssetPreloader } from "../scoreboardPreload";
import styles from "./ReplayScoreboard.module.css";

type Props = {
  open: boolean;
  assetBaseUrl?: string | null;
  identities: readonly ReplayIdentity[];
  localParticipant: number | null;
  sample: OverlaySample | null;
  snapshot: ScoreboardSnapshot | null;
  status: "idle" | "loading" | "ready" | "error";
  preloadItemIds?: readonly number[];
  spellAssets?: ParticipantSpellAssets;
  backgroundBusy?: boolean;
};

/** Null inventory is unavailable; an observed empty collection has empty slots.
 * Unplaceable entries cannot be assigned an invented slot. */
export function scoreboardSlots(items: readonly ScoreboardItem[] | null): (ScoreboardItem | "empty" | null)[] {
  if (items === null || items.some(item => item.slot === null || item.slot > 6)) return Array(7).fill(null);
  const slots: (ScoreboardItem | "empty" | null)[] = Array(7).fill("empty");
  const seen = new Set<number>();
  for (const item of items) {
    slots[item.slot!] = seen.has(item.slot!) ? null : item.item_id === 0 ? "empty" : item;
    seen.add(item.slot!);
  }
  return slots;
}

/** Only assets actually visited while open are mounted. Retained decoded nodes
 * make backward seeks/toggles reuse images; all are released with this source. */
function CachedIcon(props: { url: string | null; fallback?: string; busy?: boolean }) {
  const [visited, setVisited] = createSignal<string[]>([]);
  createEffect(() => {
    if (props.busy) return;
    const url = props.url;
    if (url && !untrack(visited).includes(url)) setVisited(previous => [...previous, url]);
  });
  return <>
    <span class={styles.fallback} aria-hidden="true">{props.fallback}</span>
    <For each={visited()}>{url => <img src={url} alt="" hidden={props.url !== url} decoding="async" fetchpriority="low" draggable={false}
      onError={event => { event.currentTarget.style.visibility = "hidden"; }} />}</For>
  </>;
}

export default function ReplayScoreboard(props: Props) {
  const [activated, setActivated] = createSignal(false);
  createEffect(() => { if (props.open) setActivated(true); });
  createEffect(() => {
    const base = props.assetBaseUrl;
    if (!activated() || !base) return;
    const preloader = new ScoreboardAssetPreloader();
    onCleanup(() => preloader.dispose());
    createEffect(() => preloader.setPaused(props.backgroundBusy ?? false));
    createEffect(() => {
      // Source context and page arrivals only: never scan snapshots on the frame clock.
      const champions = props.identities, spells = props.spellAssets, items = props.preloadItemIds;
      preloader.add((function* () {
        for (const identity of champions) if (identity.champion_name) yield `${base}/champion/${encodeURIComponent(identity.champion_name)}`;
        for (const slots of spells?.values() ?? []) for (const labels of slots) for (const label of labels) yield `${base}/spell/${encodeURIComponent(label)}`;
        for (const item of items ?? []) yield `${base}/item/${item}`;
      })());
    });
  });
  // A closed panel has no frame/snapshot dependency. Keep its last view and
  // decoded assets until reopened rather than building rows on the frame clock.
  const current = createMemo<{ sample: OverlaySample | null; facts: ScoreboardSnapshot | null }>(previous => props.open ? { sample: props.sample, facts: props.snapshot }
    : previous ?? { sample: null, facts: null });
  const identities = createMemo<readonly ReplayIdentity[]>(previous => props.open ? props.identities : previous ?? []);
  const assetBaseUrl = createMemo<string | null | undefined>(previous => props.open ? props.assetBaseUrl : previous ?? null);
  const groups = createMemo(() => {
    const players = [...identities()].sort((a, b) => a.participant - b.participant);
    const teams = ["ORDER", "CHAOS", ...new Set(players.map(p => p.team).filter(team => team !== "ORDER" && team !== "CHAOS"))];
    return teams.map(team => ({ team, label: team === "ORDER" ? "Blue team" : team === "CHAOS" ? "Red team" : team ?? "Team unavailable",
      players: players.filter(p => p.team === team) })).filter(group => group.players.length || group.team === "ORDER" || group.team === "CHAOS");
  });
  const asset = (kind: string, value: string | number | null | undefined) =>
    assetBaseUrl() && value !== null && value !== undefined ? `${assetBaseUrl()}/${kind}/${encodeURIComponent(value)}` : null;
  const value = (n: number | null | undefined) => n ?? "—";
  return <section class={styles.panel} hidden={!props.open} aria-label="Recorded scoreboard" data-testid="replay-scoreboard">
    <header class={styles.header}>
      <strong>SCOREBOARD <span>RECORDED</span></strong>
      <span>{current().sample ? `Game ${recordedGameClock(current().sample!.game_tick)}` : "No recorded state at this position"}</span>
      <kbd>Tab</kbd>
    </header>
    <div class={styles.teams}>
      <For each={groups()}>{group => <section class={styles.team} data-team={group.team} aria-label={group.label}>
        <h2>{group.label}</h2>
        <div class={styles.columns} aria-hidden="true"><span>PLAYER</span><span>K / D / A</span><span>CS</span><span title="Vision score">VS</span><span>ITEMS</span></div>
        <For each={group.players}>{identity => {
          const player = createMemo(() => current().sample?.players.find(p => p.participant === identity.participant));
          const facts = createMemo(() => current().facts?.get(identity.participant));
          const visionScore = () => {
            const score = facts()?.ward_score;
            return score == null ? "—" : Math.floor(score);
          };
          const slots = createMemo(() => scoreboardSlots(facts()?.items ?? null));
          const name = () => identity.riot_id?.split("#", 1)[0] ?? "Player unavailable";
          return <div class={styles.row} data-testid="scoreboard-player" data-participant={identity.participant}
            data-local={identity.participant === props.localParticipant || undefined}>
            <div class={styles.identity}>
              <span class={styles.portrait} title={identity.champion_name ?? "Champion unavailable"}>
                <CachedIcon url={asset("champion", identity.champion_name)} busy={props.backgroundBusy} fallback={identity.champion_name?.slice(0, 2).toUpperCase() ?? "?"} />
                <span class={styles.level} aria-label="Level">{value(player()?.level)}</span>
              </span>
              <span class={styles.spells}><For each={[0, 1] as const}>{slot => <span class={styles.spell}
                title={player()?.spells[slot] ?? "Summoner spell unavailable"}>
                <CachedIcon url={asset("spell", player()?.spells[slot])} busy={props.backgroundBusy} fallback="?" />
              </span>}</For></span>
              <span class={styles.name} title={identity.riot_id ?? name()}>{name()}</span>
            </div>
            <span class={styles.kda} aria-label="Kills / Deaths / Assists">{value(facts()?.scores.kills)} / {value(facts()?.scores.deaths)} / {value(facts()?.scores.assists)}</span>
            <span class={styles.stat} aria-label="Creep score">{value(facts()?.scores.creep_score)}</span>
            <span class={styles.stat} aria-label="Vision score">{visionScore()}</span>
            <span class={styles.items}><For each={[0, 1, 2, 3, 4, 5, 6]}>{slot => {
              const item = () => slots()[slot];
              const fact = () => { const current = item(); return current && current !== "empty" ? current : null; };
              const label = () => item() === "empty" ? `Slot ${slot + 1}: empty` : fact()?.item_id
                ? `Item ${fact()!.item_id}${fact()?.count === null ? ", quantity unavailable" : ` × ${fact()!.count}`}` : `Slot ${slot + 1}: unavailable`;
              return <span class={styles.item} data-slot={slot} data-unavailable={item() === null || fact()?.item_id === null || undefined}
                title={label()} aria-label={label()}>
                <CachedIcon url={asset("item", fact()?.item_id)} busy={props.backgroundBusy} fallback={item() === "empty" ? "" : "?"} />
                <Show when={fact()?.count !== null && fact()?.count !== undefined && fact()?.count !== 1}>
                  <span class={styles.quantity}>{fact()?.count}</span>
                </Show>
              </span>;
            }}</For></span>
          </div>;
        }}</For>
        <Show when={!group.players.length}><p class={styles.empty}>No recorded players</p></Show>
      </section>}</For>
    </div>
    <footer class={styles.footer}>
      {props.status === "error" ? "Recorded scores and inventory unavailable."
        : props.status === "loading" || props.status === "idle" ? "Loading recorded scores and inventory…"
        : "Latest recorded observation at or before the video · — / ? unavailable"}
    </footer>
  </section>;
}
