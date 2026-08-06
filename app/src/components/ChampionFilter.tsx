import { For, Show } from "solid-js";
import type { ReplayParticipant } from "../types";
import { samePlayer } from "../viewerUtils";
import styles from "./ChampionFilter.module.css";

type Props = {
  participants: readonly ReplayParticipant[];
  selectedPlayers: readonly string[];
  localPlayerName: string | null;
  mode: "panel" | "rail";
  onToggle: (summonerName: string) => void;
  onClear: () => void;
};

const monogram = (champion: string): string =>
  champion.replace(/[^a-z0-9]/gi, "").slice(0, 2).toUpperCase() || "??";

function ChampionFilter(props: Props) {
  const selected = (player: ReplayParticipant) =>
    props.selectedPlayers.includes(player.summoner_name);
  const local = (player: ReplayParticipant) =>
    props.localPlayerName !== null && samePlayer(player.summoner_name, props.localPlayerName);
  const groups = () => [
    { relation: "ally" as const, label: "ALLIES" },
    { relation: "enemy" as const, label: "ENEMIES" },
    { relation: "neutral" as const, label: "PLAYERS" },
  ].map((group) => ({
    ...group,
    players: props.participants.filter((player) => player.relation === group.relation),
  })).filter((group) => group.players.length > 0);

  return (
    <section
      classList={{
        [styles.root]: true,
        [styles.panel]: props.mode === "panel",
        [styles.rail]: props.mode === "rail",
      }}
      aria-label="Timeline champion filter"
      data-testid={`champion-filter-${props.mode}`}
    >
      <header class={styles.header}>
        <Show when={props.mode === "panel"}>
          <div>
            <span>TIMELINE FILTER</span>
            <strong>CHAMPIONS</strong>
          </div>
        </Show>
        <button
          classList={{ [styles.allButton]: true, [styles.allActive]: props.selectedPlayers.length === 0 }}
          type="button"
          onClick={props.onClear}
          aria-pressed={props.selectedPlayers.length === 0}
          aria-label="Show events for all champions"
        >
          ALL
          <Show when={props.mode === "panel"}>
            <span>{props.selectedPlayers.length === 0 ? "FULL TIMELINE" : `${props.selectedPlayers.length} SELECTED`}</span>
          </Show>
        </button>
      </header>

      <div class={styles.groups}>
        <For each={groups()}>
          {(group) => (
            <section class={`${styles.group} ${styles[`group${group.relation}`]}`}>
              <h3>{group.label}</h3>
              <div class={styles.players}>
                <For each={group.players}>
                  {(player) => (
                    <button
                      classList={{
                        [styles.player]: true,
                        [styles.selected]: selected(player),
                        [styles.local]: local(player),
                      }}
                      type="button"
                      onClick={() => props.onToggle(player.summoner_name)}
                      aria-pressed={selected(player)}
                      aria-label={`${selected(player) ? "Remove" : "Add"} ${player.champion}, ${player.summoner_name}, from timeline filter`}
                      title={`${player.champion} · ${player.summoner_name}`}
                      data-testid="champion-filter-option"
                      data-player-name={player.summoner_name}
                    >
                      <span class={styles.portrait} aria-hidden="true">{monogram(player.champion)}</span>
                      <Show when={props.mode === "panel"}>
                        <span class={styles.identity}>
                          <strong>{player.champion}</strong>
                          <small>{player.summoner_name}</small>
                        </span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            </section>
          )}
        </For>
      </div>
    </section>
  );
}

export default ChampionFilter;
