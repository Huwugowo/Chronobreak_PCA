import { For, Show, createSignal } from "solid-js";
import { formatDuration } from "../format";
import { REPLAY_TICKS_PER_SECOND } from "../replayTime";
import { combatAssisters, combatParticipant, type TimelineParticipantIndex } from "../timelineCombat";
import type { TimelineDisplayEvent } from "../timelineEventDisplay";
import type { ReplayParticipant, ViewerEvent } from "../types";
import styles from "./ReplayTimeline.module.css";

function Portrait(props: { player?: ReplayParticipant; name: string; assetBase?: string | null; small?: boolean }) {
  const [failed, setFailed] = createSignal(false);
  const champion = () => props.player?.champion;
  const src = () => props.assetBase && champion() && champion() !== "Unknown champion"
    ? `${props.assetBase}/champion/${champion()}` : null;
  return <span classList={{ [styles.combatPortrait]: true, [styles.combatAssistPortrait]: props.small }}
    title={champion() ? `${champion()} · ${props.name}` : props.name}>
    <Show when={src() && !failed()} fallback={<span class={styles.combatPortraitFallback} aria-hidden="true">?</span>}>
      <img src={src()!} alt={champion()!} draggable={false} onError={() => setFailed(true)} />
    </Show>
  </span>;
}

export default function TimelineCombatCard(props: {
  events: readonly TimelineDisplayEvent[];
  participants: TimelineParticipantIndex;
  assetBase?: string | null;
  onHighlight: (event: ViewerEvent) => void;
  onSelect: (event: TimelineDisplayEvent) => void;
}) {
  return <div class={styles.combatCard} role="dialog" aria-label="Kill events">
    <For each={props.events}>{item => {
      const killer = () => combatParticipant(item.source, "killer", props.participants);
      const victim = () => combatParticipant(item.source, "victim", props.participants);
      const killerName = () => killer()?.summoner_name ?? item.source.killer ?? "Unknown killer";
      const victimName = () => victim()?.summoner_name ?? item.source.victim ?? "Unknown victim";
      const assists = () => combatAssisters(item.source, props.participants);
      const badges = () => [...new Set([...item.badges,
        ...(item.multiplier && item.multiplier > 1 ? [`${item.multiplier}× Multikill`] : [])])];
      const time = () => formatDuration(item.replay_tick * 1000 / REPLAY_TICKS_PER_SECOND);
      return <button type="button" class={styles.combatRow}
        aria-label={`Seek to ${killerName()} killed ${victimName()} at ${time()}${assists().length ? `; assists: ${assists().map(assist => assist.name).join(", ")}` : ""}${badges().length ? `; ${badges().join(", ")}` : ""}`}
        onPointerEnter={() => props.onHighlight(item.source)} onFocus={() => props.onHighlight(item.source)}
        onClick={() => props.onSelect(item)}>
        <time class={styles.combatTime}>{time()}</time>
        <span class={styles.combatDetails}>
          <span class={styles.combatKill}>
            <Portrait player={killer()} name={killerName()} assetBase={props.assetBase} />
            <span class={styles.combatVerb}>killed</span>
            <Portrait player={victim()} name={victimName()} assetBase={props.assetBase} />
          </span>
          <Show when={assists().length > 0}>
            <span class={styles.combatAssists}><span>Assists</span>
              <For each={assists()}>{assist => <Portrait small player={assist.player} name={assist.name} assetBase={props.assetBase} />}</For>
            </span>
          </Show>
          <Show when={badges().length > 0}><span class={styles.combatBadges}>
            <For each={badges()}>{badge => <span>{badge}</span>}</For>
          </span></Show>
        </span>
      </button>;
    }}</For>
  </div>;
}
