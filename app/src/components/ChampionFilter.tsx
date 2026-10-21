import { For, Show, createMemo } from "solid-js";
import type { ReplayParticipant } from "../types";
import { participantKey } from "../leagueSource";
import type { ParticipantSpellAssets, RecordedParticipantState } from "../replayProjection";
import styles from "./ChampionFilter.module.css";

type Props = {
  ddragonAssetBaseUrl?: string | null;
  participants: readonly ReplayParticipant[];
  participantStates?: ReadonlyMap<string, RecordedParticipantState>;
  spellAssets?: ParticipantSpellAssets;
  selectedPlayers: readonly string[];
  localPlayerName: string | null;
  mode: "panel" | "rail" | "fullscreen";
  onToggle: (summonerName: string) => void;
  onClear: () => void;
};

const monogram = (champion: string): string =>
  champion.replace(/[^a-z0-9]/gi, "").slice(0, 2).toUpperCase() || "??";

const championPortraitUrl = (
  assetBaseUrl: string | null | undefined,
  champion: string,
): string | null =>
  assetBaseUrl
    ? `${assetBaseUrl}/champion/${encodeURIComponent(champion)}`
    : null;

function ChampionFilter(props: Props) {
  const state = (player: ReplayParticipant) => player.identity
    ? props.participantStates?.get(participantKey(player.identity)) : undefined;
  const statusDescription = (player: ReplayParticipant) => {
    const current = state(player);
    return [current?.level !== undefined ? `Level ${current.level}` : null,
      current?.dead ? current.respawnSeconds !== undefined ? `Dead, respawn in ${current.respawnSeconds} seconds` : "Dead" : null,
    ].filter(Boolean).join(" · ");
  };
  const selected = (player: ReplayParticipant) =>
    !!player.identity && props.selectedPlayers.includes(participantKey(player.identity));
  const local = (player: ReplayParticipant) =>
    player.is_local === true;
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
        [styles.fullscreen]: props.mode === "fullscreen",
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
            <section class={`${styles.group} ${styles[`group${group.relation}`]}`} aria-label={group.label}>
              <Show when={props.mode !== "fullscreen"}><h3>{group.label}</h3></Show>
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
                      onClick={() => player.identity && props.onToggle(participantKey(player.identity))}
                      disabled={!player.identity}
                      aria-pressed={selected(player)}
                      aria-label={`${selected(player) ? "Remove" : "Add"} ${player.champion}, ${player.summoner_name}, from timeline filter`}
                      aria-description={statusDescription(player) || undefined}
                      title={`${player.champion} · ${player.summoner_name}${statusDescription(player) ? ` · ${statusDescription(player)}` : ""}`}
                      data-testid="champion-filter-option"
                      data-player-name={player.summoner_name}
                      data-dead={state(player)?.dead === true ? "true" : undefined}
                    >
                      <Show when={props.mode === "fullscreen"}>
                        <span class={styles.playerName} title={player.summoner_name.split("#", 1)[0]} aria-hidden="true">
                          {player.summoner_name.split("#", 1)[0]}
                        </span>
                      </Show>
                      <span classList={{ [styles.portrait]: true, [styles.dead]: state(player)?.dead === true }} aria-hidden="true">
                        <span class={styles.portraitFallback}>
                          {monogram(player.champion)}
                        </span>
                        <Show
                          when={championPortraitUrl(
                            props.ddragonAssetBaseUrl,
                            player.champion,
                          )}
                        >
                          {(src) => (
                            <img
                              class={styles.portraitImage}
                              src={src()}
                              alt=""
                              decoding="async"
                              draggable={false}
                              onError={(event) =>
                                event.currentTarget.remove()
                              }
                            />
                          )}
                        </Show>
                        <Show when={state(player)?.level !== undefined}>
                          <span class={styles.level} data-testid="champion-level">{state(player)?.level}</span>
                        </Show>
                        <Show when={state(player)?.dead && state(player)?.respawnSeconds !== undefined}>
                          <span class={styles.respawn} data-testid="champion-respawn">{state(player)?.respawnSeconds}</span>
                        </Show>
                      </span>
                      <Show when={props.mode === "fullscreen"}>
                        <span class={styles.spells} aria-hidden="true">
                          <For each={[0, 1] as const}>
                            {(slot) => {
                              const spell = () => state(player)?.spells[slot];
                              // Mount/decode each recorded asset once per source.
                              // Seeking only changes visibility, never its URL.
                              const assets = createMemo(() => [...new Set([
                                ...(player.identity ? props.spellAssets?.get(participantKey(player.identity))?.[slot] ?? [] : []),
                                ...(spell() ? [spell()!] : []),
                              ])], undefined, { equals: (a, b) => a.length === b.length && a.every((label, i) => label === b[i]) });
                              return <span class={styles.spell} title={spell()
                                ? `${state(player)?.spellsAreInitial ? "First recorded: " : ""}${spell()}` : "Summoner spell unavailable"}>
                                <For each={assets()}>{label =>
                                  <img src={props.ddragonAssetBaseUrl ? `${props.ddragonAssetBaseUrl}/spell/${encodeURIComponent(label)}` : undefined}
                                    hidden={label !== spell() || !props.ddragonAssetBaseUrl} alt="" decoding="async" draggable={false}
                                    onError={event => { event.currentTarget.style.visibility = "hidden"; }}
                                    onLoad={event => { const image = event.currentTarget; void image.decode().then(() => { image.style.visibility = ""; }, () => {}); }} />
                                }</For>
                              </span>;
                            }}
                          </For>
                        </span>
                      </Show>
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
