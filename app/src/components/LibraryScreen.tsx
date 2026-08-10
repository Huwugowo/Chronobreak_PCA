import { For, Show } from "solid-js";
import { formatBytes, formatDate, formatDuration, formatTimestampDate } from "../format";
import type { ClipSummary, DdragonStatus, GameItemSummary, GameSummary, LibraryTab } from "../types";
import styles from "./Library.module.css";

type Props = {
  tab: LibraryTab;
  games: GameSummary[];
  clips: ClipSummary[];
  ddragon: DdragonStatus;
  busyId: string | null;
  onOpenGame: (timestamp: string) => void;
  onToggleSaved: (game: GameSummary) => void;
  onDeleteGame: (game: GameSummary) => void;
  onOpenClip: (clip: ClipSummary) => void;
  onDeleteClip: (clip: ClipSummary) => void;
  onOpenClipsFolder: () => void;
};

const accentColors = ["#2457ff", "#8b5cff", "#00a889", "#d24b68", "#d68c2f"];
const itemSlots = Array.from({ length: 7 }, (_, slot) => slot);

const ddragonAsset = (
  status: DdragonStatus,
  kind: "champion" | "spell" | "rune" | "item",
  asset: string | number | null | undefined,
): string | null => {
  if (!status.asset_base_url || asset === null || asset === undefined || asset === "") return null;
  return `${status.asset_base_url}/${kind}/${encodeURIComponent(String(asset))}`;
};

const finalBuild = (items: GameItemSummary[]): Array<GameItemSummary | null> =>
  itemSlots.map((slot) => items.find((item) => item.slot === slot) ?? null);

const hideFailedImage = (event: Event) => {
  if (event.currentTarget instanceof HTMLImageElement) event.currentTarget.hidden = true;
};

const formatKdaRatio = (game: GameSummary): string => {
  if (game.incomplete) return "—";
  if (game.deaths === 0) return "PERFECT KDA";
  return `${((game.kills + game.assists) / game.deaths).toFixed(2)}:1 KDA`;
};

function LibraryScreen(props: Props) {
  return (
    <div class={styles.library}>
      <Show when={props.tab === "games"} fallback={<ClipsLibrary {...props} />}>
        <GamesLibrary {...props} />
      </Show>
    </div>
  );
}

function GamesLibrary(props: Props) {
  return (
    <>
      <section class={styles.libraryIntro}>
        <div>
          <p>MATCH ARCHIVE</p>
          <h1>Your games, locally recorded.</h1>
        </div>
        <dl>
          <div>
            <dt>RECORDINGS</dt>
            <dd>{props.games.length.toString().padStart(2, "0")}</dd>
          </div>
          <div>
            <dt>SAVED</dt>
            <dd>{props.games.filter((game) => game.saved).length.toString().padStart(2, "0")}</dd>
          </div>
        </dl>
      </section>

      <Show
        when={props.games.length > 0}
        fallback={
          <EmptyState
            label="NO RECORDINGS"
            title="Your next game will appear here."
            detail="Start League of Legends with the recorder running. Capture begins automatically."
          />
        }
      >
        <section class={styles.matchList} aria-label="Recorded games">
          <For each={props.games}>
            {(game, index) => (
              <article
                class={styles.matchRow}
                style={{ "--match-accent": accentColors[index() % accentColors.length] }}
                data-incomplete={game.incomplete}
              >
                <button
                  class={styles.matchOpen}
                  type="button"
                  onClick={() => props.onOpenGame(game.timestamp)}
                  disabled={!game.video_available}
                  aria-label={`Open ${game.champion} recording from ${formatDate(game.recorded_at)}`}
                >
                  <span class={styles.matchLoadout} aria-hidden="true">
                    <span class={styles.matchChampion}>
                      <i />
                      <strong>{game.champion === "Unknown" ? "?" : game.champion.slice(0, 2)}</strong>
                      <Show
                        when={ddragonAsset(
                          props.ddragon,
                          "champion",
                          game.champion === "Unknown" ? null : game.champion,
                        )}
                      >
                        <img
                          src={ddragonAsset(
                            props.ddragon,
                            "champion",
                            game.champion === "Unknown" ? null : game.champion,
                          )!}
                          alt=""
                          loading="lazy"
                          decoding="async"
                          onError={hideFailedImage}
                        />
                      </Show>
                    </span>
                    <span class={styles.matchSpells}>
                      <For each={game.summoner_spells.slice(0, 2)}>
                        {(spell) => (
                          <span class={styles.matchAssetIcon} title={spell.replace(/^Summoner/, "")}>
                            <Show when={ddragonAsset(props.ddragon, "spell", spell)}>
                              <img
                                src={ddragonAsset(props.ddragon, "spell", spell)!}
                                alt=""
                                loading="lazy"
                                decoding="async"
                                onError={hideFailedImage}
                              />
                            </Show>
                          </span>
                        )}
                      </For>
                    </span>
                    <span class={styles.matchRunes}>
                      <span class={styles.matchAssetIcon} title="Keystone rune">
                        <Show when={ddragonAsset(props.ddragon, "rune", game.keystone_id)}>
                          <img
                            src={ddragonAsset(props.ddragon, "rune", game.keystone_id)!}
                            alt=""
                            loading="lazy"
                            decoding="async"
                            onError={hideFailedImage}
                          />
                        </Show>
                      </span>
                    </span>
                  </span>
                  <span class={styles.matchIdentity}>
                    <span class={styles.matchTitle}>
                      <strong>{game.champion}</strong>
                      <small>{game.game_mode}</small>
                    </span>
                    <span class={styles.matchDate}>
                      {game.incomplete ? `Recovered bundle · ${game.timestamp}` : formatDate(game.recorded_at)}
                    </span>
                    <span class={styles.matchBadges}>
                      <Show when={game.saved}>
                        <em data-saved>SAVED</em>
                      </Show>
                      <Show when={game.incomplete}>
                        <em data-incomplete>INCOMPLETE</em>
                      </Show>
                    </span>
                  </span>
                  <span class={styles.matchKda}>
                    <small>K / D / A</small>
                    <strong>
                      {game.kills} <i>/</i> {game.deaths} <i>/</i> {game.assists}
                    </strong>
                    <em>{formatKdaRatio(game)}</em>
                  </span>
                  <span class={styles.matchItems} aria-hidden="true">
                    <small>FINAL BUILD</small>
                    <span>
                      <For each={finalBuild(game.items)}>
                        {(item) => (
                          <span class={styles.matchItem} title={item ? "Final item" : undefined}>
                            <Show when={ddragonAsset(props.ddragon, "item", item?.item_id)}>
                              <img
                                src={ddragonAsset(props.ddragon, "item", item?.item_id)!}
                                alt=""
                                loading="lazy"
                                decoding="async"
                                onError={hideFailedImage}
                              />
                            </Show>
                          </span>
                        )}
                      </For>
                    </span>
                  </span>
                  <span class={styles.matchMetric}>
                    <small>DURATION</small>
                    <strong>{game.duration_ms ? formatDuration(game.duration_ms) : "—"}</strong>
                  </span>
                  <span class={`${styles.matchMetric} ${styles.matchFile}`}>
                    <small>RECORDING</small>
                    <strong>{formatBytes(game.video_size_bytes)}</strong>
                  </span>
                  <span class={styles.matchChevron} aria-hidden="true">›</span>
                </button>
                <div class={styles.matchActions}>
                  <Show when={!game.incomplete}>
                    <button
                      type="button"
                      onClick={() => props.onToggleSaved(game)}
                      disabled={props.busyId === game.timestamp}
                      aria-label={game.saved ? `Unsave ${game.champion} recording` : `Save ${game.champion} recording`}
                      data-active={game.saved}
                    >
                      <span aria-hidden="true">{game.saved ? "★" : "☆"}</span>
                      {game.saved ? "SAVED" : "SAVE"}
                    </button>
                  </Show>
                  <Show when={!game.saved}>
                    <button
                      type="button"
                      data-danger
                      onClick={() => props.onDeleteGame(game)}
                      disabled={props.busyId === game.timestamp}
                      aria-label={`Delete ${game.champion} recording`}
                    >
                      DELETE
                    </button>
                  </Show>
                </div>
              </article>
            )}
          </For>
        </section>
      </Show>
    </>
  );
}

function ClipsLibrary(props: Props) {
  return (
    <>
      <section class={styles.libraryIntro}>
        <div>
          <p>EXPORTED MOMENTS</p>
          <h1>Clips, ready to revisit.</h1>
        </div>
        <div class={styles.introActions}>
          <span>{props.clips.length} LOCAL FILES</span>
          <button type="button" onClick={props.onOpenClipsFolder}>
            Open clips folder ↗
          </button>
        </div>
      </section>

      <Show
        when={props.clips.length > 0}
        fallback={
          <EmptyState
            label="NO CLIPS"
            title="Your exported moments will live here."
            detail="Open a recording, select a moment on its timeline, and export it for Discord or social video."
          />
        }
      >
        <section class={styles.clipGrid} aria-label="Exported clips">
          <For each={props.clips}>
            {(clip, index) => (
              <article class={styles.clipCard}>
                <button
                  class={styles.clipOpen}
                  type="button"
                  onClick={() => props.onOpenClip(clip)}
                  aria-label={`Play ${clip.source_champion ?? "unknown"} clip`}
                >
                  <span
                    class={styles.clipArtwork}
                    style={{ "--clip-accent": accentColors[index() % accentColors.length] }}
                  >
                    <Show
                      when={clip.thumbnail_url}
                      fallback={
                        <span class={styles.clipPlaceholder} aria-hidden="true">
                          <i />
                          <strong>{clip.source_champion?.slice(0, 2) ?? "LR"}</strong>
                        </span>
                      }
                    >
                      <img src={clip.thumbnail_url!} alt="" loading="lazy" />
                    </Show>
                    <em>{formatDuration(clip.duration_ms)}</em>
                    <i class={styles.playGlyph} aria-hidden="true">
                      ▶
                    </i>
                  </span>
                  <span class={styles.clipContent}>
                    <strong>{clip.source_champion ?? "Source game unavailable"}</strong>
                    <small>
                      {clip.source_date
                        ? formatDate(clip.source_date)
                        : formatTimestampDate(clip.clip_timestamp)}
                    </small>
                    <span>{formatBytes(clip.file_size_bytes)}</span>
                  </span>
                </button>
                <button
                  class={styles.clipDelete}
                  type="button"
                  data-danger
                  onClick={() => props.onDeleteClip(clip)}
                  disabled={props.busyId === clip.filename}
                >
                  Delete
                </button>
              </article>
            )}
          </For>
        </section>
      </Show>
    </>
  );
}

function EmptyState(props: { label: string; title: string; detail: string }) {
  return (
    <section class={styles.emptyState}>
      <span aria-hidden="true">LR</span>
      <p>{props.label}</p>
      <h2>{props.title}</h2>
      <small>{props.detail}</small>
    </section>
  );
}

export default LibraryScreen;
