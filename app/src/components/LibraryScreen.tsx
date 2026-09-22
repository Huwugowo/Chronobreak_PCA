import { For, Show, createSignal } from "solid-js";
import { formatBytes, formatDate, formatDuration, formatTimestampDate } from "../format";
import { previewMatchOutcomeFor, type PreviewMatchOutcome } from "../dev/matchHistoryPreview";
import type { ClipSummary, DdragonStatus, GameItemSummary, GameSummary, LibraryTab } from "../types";
import { MoreIcon, StarIcon } from "../ui/icons";
import matchStyles from "./MatchHistory.module.css";
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

const clipAccentColors = ["#2457ff", "#8b5cff", "#00a889", "#d24b68", "#d68c2f"];
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

const displayedOutcome = (game: GameSummary): PreviewMatchOutcome | null =>
  game.incomplete ? null : previewMatchOutcomeFor(game.timestamp);

const outcomeLabel = (outcome: PreviewMatchOutcome): string => {
  switch (outcome) {
    case "victory":
      return "Victory";
    case "defeat":
      return "Defeat";
    case "remake":
      return "Remake";
    case "terminated":
      return "Terminated";
  }
};

const replayHealth = (game: GameSummary): string | null => {
  if (game.incomplete) return game.video_available ? "INCOMPLETE" : "INCOMPLETE · UNAVAILABLE";
  if (!game.video_available) return "UNAVAILABLE";
  return null;
};

const formattedTime = (value: string): string => {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" }).format(date);
};

const formatRelativeAge = (value: string): string => {
  if (!value) return "—";

  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return "—";

  const delta = timestamp - Date.now();
  const absolute = Math.abs(delta);

  const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ["year", 365 * 24 * 60 * 60 * 1000],
    ["month", 30 * 24 * 60 * 60 * 1000],
    ["week", 7 * 24 * 60 * 60 * 1000],
    ["day", 24 * 60 * 60 * 1000],
    ["hour", 60 * 60 * 1000],
    ["minute", 60 * 1000],
    ["second", 1000],
  ];

  const [unit, size] = units.find(([, size]) => absolute >= size) ?? units[units.length - 1];
  return new Intl.RelativeTimeFormat("en", { numeric: "always" })
    .format(Math.round(delta / size), unit);
};

const formatMatchDuration = (durationMs: number): string => {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${seconds}s`;
};

const closeActionMenu = (event: MouseEvent) => {
  const current = event.currentTarget;
  if (!(current instanceof HTMLElement)) return;
  current.closest("details")?.removeAttribute("open");
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
  const [searchQuery, setSearchQuery] = createSignal("");
  const [championFilter, setChampionFilter] = createSignal("");
  const [modeFilter, setModeFilter] = createSignal("");
  const [savedOnly, setSavedOnly] = createSignal(false);

  const savedCount = () => props.games.filter((game) => game.saved).length;

  const champions = () =>
    [...new Set(props.games.map((game) => game.champion).filter((champion) => champion !== "Unknown"))]
      .sort((a, b) => a.localeCompare(b));

  const modes = () =>
    [...new Set(props.games.map((game) => game.game_mode).filter((mode) => mode !== "Unknown"))]
      .sort((a, b) => a.localeCompare(b));

  const filteredGames = () => {
    const query = searchQuery().trim().toLocaleLowerCase();

    return props.games.filter((game) => {
      const matchesSearch =
        !query
        || game.champion.toLocaleLowerCase().includes(query)
        || game.game_mode.toLocaleLowerCase().includes(query);

      const matchesChampion = !championFilter() || game.champion === championFilter();
      const matchesMode = !modeFilter() || game.game_mode === modeFilter();
      const matchesSaved = !savedOnly() || game.saved;

      return matchesSearch && matchesChampion && matchesMode && matchesSaved;
    });
  };

  return (
    <section class={matchStyles.matchHistory} aria-label="Match History">
      <div class={matchStyles.toolbar}>
        <input
          class={matchStyles.searchField}
          type="search"
          value={searchQuery()}
          onInput={(event) => setSearchQuery(event.currentTarget.value)}
          placeholder="Search matches..."
          aria-label="Search matches"
        />

        <select
          class={matchStyles.filterSelect}
          value={championFilter()}
          onChange={(event) => setChampionFilter(event.currentTarget.value)}
          aria-label="Filter by champion"
        >
          <option value="">All champions</option>
          <For each={champions()}>
            {(champion) => <option value={champion}>{champion}</option>}
          </For>
        </select>

        <select
          class={matchStyles.filterSelect}
          value={modeFilter()}
          onChange={(event) => setModeFilter(event.currentTarget.value)}
          aria-label="Filter by mode"
        >
          <option value="">All modes</option>
          <For each={modes()}>
            {(mode) => <option value={mode}>{mode}</option>}
          </For>
        </select>

        <button
          class={matchStyles.savedFilter}
          type="button"
          data-active={savedOnly()}
          aria-pressed={savedOnly()}
          onClick={() => setSavedOnly((value) => !value)}
        >
          <StarIcon size={15} filled={savedOnly()} />
          Favorites
        </button>
        <div class={matchStyles.summaryBar} aria-label="Match History summary">
          <span><strong>{props.games.length}</strong> recordings</span>
          <i aria-hidden="true" />
          <span><strong>{savedCount()}</strong> saved</span>
        </div>
      </div>

      <Show
        when={props.games.length > 0}
        fallback={<MatchHistoryEmptyState />}
      >
        <div class={matchStyles.matchScroller}>
          <section class={matchStyles.matchList} aria-label="Recorded matches">
            <For each={filteredGames()}>
              {(game: GameSummary) => {
                const outcome = displayedOutcome(game);
                const health = replayHealth(game);
                const championAsset = () => ddragonAsset(
                  props.ddragon,
                  "champion",
                  game.champion === "Unknown" ? null : game.champion,
                );
                const date = game.recorded_at ? formatDate(game.recorded_at, true) : "—";
                const time = game.recorded_at ? formattedTime(game.recorded_at) : "—";

                return (
                  <article class={matchStyles.matchItem} data-incomplete={game.incomplete} data-outcome={outcome ?? undefined}>
                    <button
                      class={matchStyles.matchOpen}
                      type="button"
                      onClick={() => props.onOpenGame(game.timestamp)}
                      disabled={!game.video_available}
                      aria-label={
                        game.video_available
                          ? `Open ${game.champion} recording from ${formatDate(game.recorded_at)}`
                          : `${game.champion} recording unavailable`
                      }
                    >
                      <span class={matchStyles.matchContext}>
                        <strong class={matchStyles.queueLabel}>
                          {game.game_mode === "Unknown" ? "—" : game.game_mode}
                        </strong>

                        <span
                          class={matchStyles.relativeAge}
                          title={game.recorded_at ? `${date} · ${time}` : undefined}
                        >
                          {formatRelativeAge(game.recorded_at)}
                        </span>

                        <span class={matchStyles.contextDivider} aria-hidden="true" />

                        <Show
                          when={outcome}
                          fallback={
                            <Show when={health}>
                              <strong class={matchStyles.health}>{health}</strong>
                            </Show>
                          }
                        >
                          <strong class={matchStyles.outcome} data-outcome={outcome!}>
                            {outcomeLabel(outcome!)}
                          </strong>
                        </Show>

                        <span class={matchStyles.contextDuration}>
                          {game.duration_ms ? formatMatchDuration(game.duration_ms) : "—"}
                        </span>

                        <Show when={outcome && health}>
                          <em class={matchStyles.health}>{health}</em>
                        </Show>
                      </span>
                      <span class={matchStyles.playerSummary}>
                        <span class={matchStyles.championLoadout} aria-hidden="true">
                          <span class={matchStyles.championSlot}>
                            <span>?</span>
                            <Show when={championAsset()}>
                              <img
                                src={championAsset()!}
                                alt=""
                                loading="lazy"
                                decoding="async"
                                onError={hideFailedImage}
                              />
                            </Show>
                          </span>

                          <span class={matchStyles.loadout}>
                            <span class={matchStyles.spells}>
                              <For each={game.summoner_spells.slice(0, 2)}>
                                {(spell: string) => (
                                  <span class={matchStyles.assetIcon} title={spell.replace(/^Summoner/, "")}>
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

                            <span class={`${matchStyles.assetIcon} ${matchStyles.runeIcon}`} title="Keystone rune">
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

                        <span class={matchStyles.performance}>
                          <span class={matchStyles.kda}>
                            <strong>
                              {game.incomplete ? "—" : game.kills} <i>/</i>{" "}
                              {game.incomplete ? "—" : game.deaths} <i>/</i>{" "}
                              {game.incomplete ? "—" : game.assists}
                            </strong>
                            <em>{formatKdaRatio(game)}</em>
                          </span>

                          <span class={matchStyles.items} aria-hidden="true">
                            <For each={finalBuild(game.items)}>
                              {(item: GameItemSummary | null) => (
                                <span class={matchStyles.itemSlot}>
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
                      </span>

                      <span class={matchStyles.teamRosters} aria-label="Team rosters">
                        <span class={matchStyles.teamRoster} data-team="ally">
                          <For each={game.participants.filter((participant) => participant.relation === "ally").slice(0, 5)}>
                            {(participant) => (
                              <span class={matchStyles.rosterPlayer}>
                                <span class={matchStyles.rosterChampion} aria-hidden="true">
                                  <Show when={ddragonAsset(
                                    props.ddragon,
                                    "champion",
                                    participant.champion === "Unknown" ? null : participant.champion,
                                  )}>
                                    <img
                                      src={ddragonAsset(
                                        props.ddragon,
                                        "champion",
                                        participant.champion === "Unknown" ? null : participant.champion,
                                      )!}
                                      alt=""
                                      loading="lazy"
                                      decoding="async"
                                      onError={hideFailedImage}
                                    />
                                  </Show>
                                </span>
                                <span>{participant.summoner_name}</span>
                              </span>
                            )}
                          </For>
                        </span>

                        <span class={matchStyles.teamRoster} data-team="enemy">
                          <For each={game.participants.filter((participant) => participant.relation === "enemy").slice(0, 5)}>
                            {(participant) => (
                              <span class={matchStyles.rosterPlayer}>
                                <span class={matchStyles.rosterChampion} aria-hidden="true">
                                  <Show when={ddragonAsset(
                                    props.ddragon,
                                    "champion",
                                    participant.champion === "Unknown" ? null : participant.champion,
                                  )}>
                                    <img
                                      src={ddragonAsset(
                                        props.ddragon,
                                        "champion",
                                        participant.champion === "Unknown" ? null : participant.champion,
                                      )!}
                                      alt=""
                                      loading="lazy"
                                      decoding="async"
                                      onError={hideFailedImage}
                                    />
                                  </Show>
                                </span>
                                <span>{participant.summoner_name}</span>
                              </span>
                            )}
                          </For>
                        </span>
                      </span>
                      <span class={matchStyles.recordingMeta}>
                        <strong>{formatBytes(game.video_size_bytes)}</strong>
                        <small>Recording</small>
                      </span>
                    </button>

                    <div class={matchStyles.matchActions}>
                      <Show when={!game.incomplete}>
                        <button
                          class={matchStyles.starButton}
                          type="button"
                          onClick={() => props.onToggleSaved(game)}
                          disabled={props.busyId === game.timestamp}
                          aria-label={game.saved ? `Unsave ${game.champion} recording` : `Save ${game.champion} recording`}
                          title={game.saved ? "Saved" : "Save"}
                          data-saved={game.saved}
                        >
                          <StarIcon size={17} filled={game.saved} />
                        </button>
                      </Show>

                      <Show when={!game.saved}>
                        <details class={matchStyles.moreMenu} data-busy={props.busyId === game.timestamp}>
                          <summary aria-label={`More actions for ${game.champion} recording`} title="More actions">
                            <MoreIcon size={17} />
                          </summary>
                          <div class={matchStyles.actionMenu}>
                            <button
                              type="button"
                              data-danger
                              onClick={(event: MouseEvent) => {
                                closeActionMenu(event);
                                props.onDeleteGame(game);
                              }}
                              disabled={props.busyId === game.timestamp}
                            >
                              Delete recording
                            </button>
                          </div>
                        </details>
                      </Show>
                    </div>
                  </article>
                );
              }}
            </For>
          </section>
        </div>
      </Show>
    </section>
  );
}

function MatchHistoryEmptyState() {
  return (
    <section class={matchStyles.emptyState}>
      <h2>No recordings yet</h2>
      <p>Your next automatically recorded League game will appear here.</p>
    </section>
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
            {(clip: ClipSummary, index: () => number) => (
              <article class={styles.clipCard}>
                <button
                  class={styles.clipOpen}
                  type="button"
                  onClick={() => props.onOpenClip(clip)}
                  aria-label={`Play ${clip.source_champion ?? "unknown"} clip`}
                >
                  <span
                    class={styles.clipArtwork}
                    style={{ "--clip-accent": clipAccentColors[index() % clipAccentColors.length] }}
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
