import { For, Show, createEffect, createMemo, createSignal, on, onCleanup } from "solid-js";
import { formatBytes, formatDate, formatDuration, formatTimestampDate } from "../format";
import { previewMatchOutcomeFor, type PreviewMatchOutcome } from "../dev/matchHistoryPreview";
import type { ClipSummary, DdragonStatus, GameSummary, LibraryTab } from "../types";
import type { HistoryQuery, HistoryStats } from "../leagueIndex";
import type { HistoryState } from "../historyController";
import { recordBenchmarkSearchInput, recordBenchmarkSearchSchedule } from "../benchmarkSearch";
import type { ActorRole } from "../leagueSource";
import type { DurationDisplay } from "../libraryController";
import type { LibraryOrigin } from "../libraryController";
import { MoreIcon, StarIcon } from "../ui/icons";
import matchStyles from "./MatchHistory.module.css";
import { ExternalLinkIcon, PlayIcon } from "../ui/icons";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

type Props = {
  tab: LibraryTab;
  games: GameSummary[];
  clips: ClipSummary[];
  ddragon: DdragonStatus;
  actionable: boolean;
  busyId: string | null;
  onOpenGame: (timestamp: string, origin: LibraryOrigin | null) => void;
  onToggleSaved: (game: GameSummary, origin: LibraryOrigin | null) => void;
  onDeleteGame: (game: GameSummary, origin: LibraryOrigin | null) => void;
  onOpenClip: (clip: ClipSummary) => void;
  onDeleteClip: (clip: ClipSummary, origin: LibraryOrigin | null) => void;
  onOpenClipsFolder: () => void;
  durationStates?: Readonly<Record<string, DurationDisplay>>;
  onRetryDuration?: (filename: string) => void;
  snapshotOrigin: LibraryOrigin | null;
  history?: HistoryState;
  onHistoryQuery?: (query: HistoryQuery, benchmarkActionId?: string) => void;
  onHistoryNext?: () => void;
  onHistoryFirst?: () => void;
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

const unavailableStats: HistoryStats = { source: "unavailable", kills: null, deaths: null, assists: null, items: null, keystone_id: null, secondary_style: null, summoner_spells: null, creep_score: null, duration_seconds: null, evidence: null, final_evidence: null, game_tick: null };

const hideFailedImage = (event: Event) => {
  if (event.currentTarget instanceof HTMLImageElement) event.currentTarget.hidden = true;
};

const formatKdaRatio = (stats: HistoryStats): string => {
  if (stats.kills === null || stats.deaths === null || stats.assists === null) return "—";
  if (stats.deaths === 0) return "PERFECT KDA";
  return `${((stats.kills + stats.assists) / stats.deaths).toFixed(2)}:1 KDA`;
};

const displayedOutcome = (game: GameSummary): PreviewMatchOutcome | "unknown" | null => {
  if (game.league_result) return game.league_result.outcome === "win" ? "victory" : game.league_result.outcome === "loss" ? "defeat" : "unknown";
  return previewMatchOutcomeFor(game.timestamp);
};

const outcomeLabel = (outcome: PreviewMatchOutcome | "unknown"): string => {
  switch (outcome) {
    case "unknown":
      return "Unknown";
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
  const [riotId, setRiotId] = createSignal("");
  const [eventKind, setEventKind] = createSignal("");
  const [actorRole, setActorRole] = createSignal("");
  const [outcomeFilter, setOutcomeFilter] = createSignal("");
  const [stolenOnly, setStolenOnly] = createSignal(false);
  const [textPending, setTextPending] = createSignal(false);
  const [expandedMatch, setExpandedMatch] = createSignal<string | null>(null);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchActionId: string | undefined;
  let scheduledSearchActionId: string | undefined;
  createEffect(on(
    [searchQuery, championFilter, modeFilter, savedOnly, riotId, eventKind,
      actorRole, outcomeFilter, stolenOnly, () => JSON.stringify(props.snapshotOrigin)],
    (values, previous) => {
      clearTimeout(searchTimer);
      recordBenchmarkSearchSchedule("coalesced", scheduledSearchActionId);
      scheduledSearchActionId = undefined;
      const query: HistoryQuery = { limit: 50, search: searchQuery().trim() || null,
        champion: championFilter() || null, mode: modeFilter() || null, saved_only: savedOnly(),
        participant_riot_id: riotId().trim() || null, event_kind: eventKind() || null,
        role: actorRole() as ActorRole || null, outcome: outcomeFilter() as HistoryQuery["outcome"] || null,
        stolen: stolenOnly() ? true : null };
      const dispatch = () => {
        scheduledSearchActionId = undefined;
        setTextPending(false);
        if (searchActionId) props.onHistoryQuery?.(query, searchActionId);
        else props.onHistoryQuery?.(query);
        searchActionId = undefined;
      };
      // Discrete filters and context changes include the latest text immediately.
      if (previous && query.search && values.slice(1).every((value, index) => value === previous[index + 1])) {
        setTextPending(true);
        scheduledSearchActionId = searchActionId;
        recordBenchmarkSearchSchedule("scheduled", searchActionId, 180);
        searchTimer = setTimeout(dispatch, 180);
      } else {
        recordBenchmarkSearchSchedule("scheduled", searchActionId);
        dispatch();
      }
    },
  ));
  onCleanup(() => { clearTimeout(searchTimer); recordBenchmarkSearchSchedule("coalesced", scheduledSearchActionId); });
  const updating = () => textPending() || !!props.history?.loading;
  const semanticQuery = () => !!(riotId() || eventKind() || actorRole() || outcomeFilter() || stolenOnly());
  const matchStatus = () => {
    const history = props.history;
    if (history?.initializing) return "Loading match data...";
    if (updating()) return history?.page ? "Updating matches. Showing previous results." : "Updating matches...";
    if (history?.error) return history.page ? "Could not update matches. Showing previous results." : "Match data unavailable. Recordings remain playable.";
    if (history?.indexIssue?.kind === "failed") return "Could not load match data. Recordings remain playable.";
    if (semanticQuery() && history?.indexIssue?.kind === "limited") return "Some match data is unavailable. Results may be incomplete.";
    if (semanticQuery() && history?.page?.unknown_recordings) return "Some recordings have limited match data. Results may be incomplete.";
    return "";
  };
  const paginationDisabled = () => updating() || !!props.history?.error;

  const groups = createMemo(() => {
    const byMatch = new Map<string, GameSummary[]>();
    for (const game of props.games) {
      if (!game.match_group || !game.video_available || game.incomplete) continue;
      const parts = byMatch.get(game.match_group);
      if (parts) parts.push(game);
      else byMatch.set(game.match_group, [game]);
    }
    for (const parts of byMatch.values()) {
      parts.sort((a, b) => Date.parse(a.recorded_at) - Date.parse(b.recorded_at) || a.timestamp.localeCompare(b.timestamp, undefined, { numeric: true }));
    }
    return byMatch;
  });
  const partsFor = (game: GameSummary) => game.match_group ? groups().get(game.match_group) ?? [] : [];
  const savedCount = () => [...groups().values()].filter(parts => parts.some(game => game.saved)).length;
  const fallbackCards = createMemo(() => [...groups().values()]
    .sort((a, b) => b[b.length - 1].recorded_at.localeCompare(a[a.length - 1].recorded_at))
    .map(parts => parts.reduce((best, part) => part.league_result || !best.league_result ? part : best)));
  const representativeByGroup = createMemo(() => {
    const cards = new Map(fallbackCards().map(game => [game.match_group, game]));
    for (const row of props.history?.page?.rows ?? []) cards.set(row.game.match_group, row.game);
    return [...cards.values()];
  });

  const champions = () =>
    [...new Set(representativeByGroup().map((game) => game.champion).filter((champion) => champion !== "Unknown"))]
      .sort((a, b) => a.localeCompare(b));

  const modes = () =>
    [...new Set(representativeByGroup().map((game) => game.game_mode).filter((mode) => mode !== "Unknown"))]
      .sort((a, b) => a.localeCompare(b));

  const filteredGames = () => {
    const query = searchQuery().trim().toLocaleLowerCase();
    const semantic = semanticQuery();
    const rows = props.history?.page?.rows;
    if (rows) {
      const matches = rows.map(row => partsFor(row.game).find(game => game.media_id === row.media_id && game.timestamp === row.game.timestamp)).filter((game): game is GameSummary => !!game);
      return matches;
    }
    if (semantic) return [];
    return fallbackCards().filter(basicMatch).slice(0, 50);
    function basicMatch(game: GameSummary) {
      const matchesSearch =
        !query
        || game.champion.toLocaleLowerCase().includes(query)
        || game.game_mode.toLocaleLowerCase().includes(query);

      const matchesChampion = !championFilter() || game.champion === championFilter();
      const matchesMode = !modeFilter() || game.game_mode === modeFilter();
      const matchesSaved = !savedOnly() || partsFor(game).some(part => part.saved);

      return matchesSearch && matchesChampion && matchesMode && matchesSaved;
    }
  };
  const rowFor = (game: GameSummary) => game.media_id && props.history?.historyReady?.[game.media_id] === false
    ? undefined : props.history?.page?.rows.find(row => row.media_id === game.media_id && row.game.timestamp === game.timestamp);
  const statsFor = (game: GameSummary) => rowFor(game)?.stats ?? unavailableStats;

  return (
    <section class={matchStyles.matchHistory} aria-label="Match History">
      <div class={matchStyles.toolbar}>
        <input
          class={matchStyles.searchField}
          type="search"
          value={searchQuery()}
          onInput={(event) => {
            searchActionId = recordBenchmarkSearchInput(event.currentTarget);
            setSearchQuery(event.currentTarget.value);
          }}
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
          <span><strong>{groups().size}</strong> {groups().size === 1 ? "match" : "matches"}</span>
          <i aria-hidden="true" />
          <span><strong>{savedCount()}</strong> saved</span>
        </div>
      </div>

      <details class={matchStyles.dataFilters}>
        <summary>League data filters</summary>
        <div class={matchStyles.toolbar}>
          <input class={matchStyles.searchField} type="text" placeholder="Player#TAG" aria-label="Exact Riot ID" value={riotId()} onChange={event => setRiotId(event.currentTarget.value)} />
          <select class={matchStyles.filterSelect} aria-label="Filter by event" value={eventKind()} onChange={event => setEventKind(event.currentTarget.value)}>
            <option value="">All events</option>
            <For each={["ChampionKill", "DragonKill", "BaronKill", "HeraldKill", "HordeKill", "TurretKilled", "InhibKilled", "Ace", "Multikill"]}>{kind => <option value={kind}>{kind}</option>}</For>
          </select>
          <select class={matchStyles.filterSelect} aria-label="Filter by actor role" value={actorRole()} onChange={event => setActorRole(event.currentTarget.value)}>
            <option value="">All roles</option><For each={["killer", "victim", "assister", "acer", "recipient"]}>{role => <option value={role}>{role}</option>}</For>
          </select>
          <select class={matchStyles.filterSelect} aria-label="Filter by outcome" value={outcomeFilter()} onChange={event => setOutcomeFilter(event.currentTarget.value)}>
            <option value="">All outcomes</option><option value="win">Victory</option><option value="loss">Defeat</option><option value="unknown">Unknown</option>
          </select>
          <label><input type="checkbox" checked={stolenOnly()} onChange={event => setStolenOnly(event.currentTarget.checked)} /> Stolen objective</label>
        </div>
      </details>
      <p class={matchStyles.coverage} role="status">
        {matchStatus()}
      </p>

      <Show
        when={groups().size > 0}
        fallback={<MatchHistoryEmptyState />}
      >
        <div class={matchStyles.matchScroller}>
          <section class={matchStyles.matchList} aria-label="Recorded matches" aria-busy={updating()}>
            <For each={filteredGames()}>
              {(game: GameSummary) => {
                const parts = () => partsFor(game);
                const multipart = () => parts().length > 1;
                const expanded = () => multipart() && expandedMatch() === game.match_group;
                const outcome = () => displayedOutcome(rowFor(game)?.game ?? game);
                const health = replayHealth(game);
                const stats = () => statsFor(game);
                const participants = createMemo(() => rowFor(game)?.game.participants ?? game.participants, undefined, {
                  equals: (before, after) => before.length === after.length && before.every((player, index) => {
                    const next = after[index];
                    return player.summoner_name === next.summoner_name && player.champion === next.champion && player.relation === next.relation;
                  }),
                });
                const finalCs = () => stats().source === "final_eog" ? stats().creep_score : null;
                const csPerMinute = () => {
                  const cs = finalCs(), duration = stats().duration_seconds;
                  return cs !== null && duration !== null && duration > 0 ? (cs * 60 / duration).toFixed(1) : "—";
                };
                const championAsset = () => ddragonAsset(
                  props.ddragon,
                  "champion",
                  game.champion === "Unknown" ? null : game.champion,
                );
                const recordedAt = () => parts()[parts().length - 1]?.recorded_at ?? game.recorded_at;
                const date = () => recordedAt() ? formatDate(recordedAt(), true) : "—";
                const time = () => recordedAt() ? formattedTime(recordedAt()) : "—";
                const duration = () => {
                  const seconds = stats().duration_seconds;
                  return seconds === null ? game.duration_ms : seconds * 1_000;
                };

                return (
                  <><article class={matchStyles.matchItem} data-outcome={outcome() ?? undefined}>
                    <button
                      class={matchStyles.matchOpen}
                      type="button"
                      onClick={() => multipart()
                        ? setExpandedMatch(expanded() ? null : game.match_group)
                        : props.onOpenGame(game.timestamp, props.snapshotOrigin)}
                      disabled={!game.video_available || !props.actionable}
                      aria-expanded={multipart() ? expanded() : undefined}
                      aria-controls={multipart() ? `recording-parts-${game.match_group}` : undefined}
                      aria-label={
                        game.video_available
                          ? multipart()
                            ? `Choose a recording for ${game.champion} match from ${formatDate(recordedAt())} (${parts().length} recordings)`
                            : `Open ${game.champion} recording from ${formatDate(recordedAt())}`
                          : `${game.champion} recording unavailable`
                      }
                    >
                      <span class={matchStyles.matchContext}>
                        <strong class={matchStyles.queueLabel}>
                          {game.game_mode === "Unknown" ? "—" : game.game_mode}
                        </strong>

                        <span
                          class={matchStyles.relativeAge}
                          title={recordedAt() ? `${date()} · ${time()}` : undefined}
                        >
                          {formatRelativeAge(recordedAt())}
                        </span>

                        <span class={matchStyles.contextDivider} aria-hidden="true" />

                        <Show
                          when={outcome()}
                          fallback={
                            <Show when={health}>
                              <strong class={matchStyles.health}>{health}</strong>
                            </Show>
                          }
                        >
                          <strong class={matchStyles.outcome} data-outcome={outcome()!}>
                            {outcomeLabel(outcome()!)}
                          </strong>
                        </Show>

                        <span class={matchStyles.contextDuration}>
                          {duration() ? formatMatchDuration(duration()) : "—"}
                        </span>

                        <Show when={outcome() && health}>
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
                              <For each={(stats().summoner_spells ?? (rowFor(game) ? [null, null] : game.summoner_spells)).slice(0, 2)}>
                                {(spell) => (
                                  <span class={matchStyles.assetIcon} title={spell?.replace(/^Summoner/, "") ?? "Summoner spell unavailable"}>
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

                            <span class={matchStyles.runes}>
                              <span class={`${matchStyles.assetIcon} ${matchStyles.runeIcon}`} title="Keystone rune">
                                <Show when={ddragonAsset(props.ddragon, "rune", stats().keystone_id ?? game.keystone_id)}>
                                  <img
                                    src={ddragonAsset(props.ddragon, "rune", stats().keystone_id ?? game.keystone_id)!}
                                    alt=""
                                    loading="lazy"
                                    decoding="async"
                                    onError={hideFailedImage}
                                  />
                                </Show>
                              </span>
                              <span class={`${matchStyles.assetIcon} ${matchStyles.runeIcon} ${matchStyles.secondaryRune}`} title="Secondary rune tree">
                                <Show when={ddragonAsset(props.ddragon, "rune", stats().secondary_style)}>
                                  <img
                                    src={ddragonAsset(props.ddragon, "rune", stats().secondary_style)!}
                                    alt=""
                                    loading="lazy"
                                    decoding="async"
                                    onError={hideFailedImage}
                                  />
                                </Show>
                              </span>
                            </span>
                          </span>
                        </span>

                        <span class={matchStyles.performance}>
                          <span class={matchStyles.kda}>
                            <strong>
                              {stats().kills ?? "—"} <i>/</i>{" "}
                              {stats().deaths ?? "—"} <i>/</i>{" "}
                              {stats().assists ?? "—"}
                            </strong>
                            <em>{formatKdaRatio(stats())}</em>
                          </span>

                          <span class={matchStyles.items} aria-hidden="true">
                            <For each={itemSlots}>
                              {(slot) => (
                                <span class={matchStyles.itemSlot}>
                                  <Show when={ddragonAsset(props.ddragon, "item", stats().items?.[slot])}>
                                    <img
                                      src={ddragonAsset(props.ddragon, "item", stats().items?.[slot])!}
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

                      <span class={matchStyles.farming} aria-label="Final creep score" title="Final lane and neutral minions killed; rate uses recorded final game duration">
                        <strong>CS {finalCs() ?? "—"}</strong>
                        <small>{csPerMinute()} / min</small>
                      </span>

                      <span class={matchStyles.teamRosters} aria-label="Team rosters">
                        <span class={matchStyles.teamRoster} data-team="ally">
                          <For each={participants().filter((participant) => participant.relation === "ally")}>
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
                          <For each={participants().filter((participant) => participant.relation === "enemy")}>
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
                        <strong>{formatBytes(parts().reduce((sum, part) => sum + part.video_size_bytes, 0))}</strong>
                        <Show when={multipart()} fallback={<small>Recording</small>}>
                          <small class={matchStyles.partsBadge}>{parts().length} recordings</small>
                        </Show>
                      </span>
                    </button>

                    <Show when={!multipart()}><div class={matchStyles.matchActions}>
                      <button
                        class={matchStyles.starButton}
                        type="button"
                        onClick={() => props.onToggleSaved(game, props.snapshotOrigin)}
                        disabled={props.busyId !== null || !props.actionable}
                        aria-label={game.saved ? `Unsave ${game.champion} recording` : `Save ${game.champion} recording`}
                        title={game.saved ? "Saved" : "Save"}
                        data-saved={game.saved}
                      >
                        <StarIcon size={17} filled={game.saved} />
                      </button>

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
                                props.onDeleteGame(game, props.snapshotOrigin);
                              }}
                              disabled={props.busyId !== null || !props.actionable}
                            >
                              Delete recording
                            </button>
                          </div>
                        </details>
                      </Show>
                    </div></Show>
                  </article>
                  <Show when={expanded()}>
                    <section id={`recording-parts-${game.match_group}`} class={matchStyles.recordingParts} aria-label="Match recordings">
                      <h3>Match recordings</h3>
                      <ol>
                        <For each={parts()}>{(part, index) => (
                          <li>
                            <strong>Part {index() + 1}</strong>
                            <time dateTime={part.recorded_at}>{formatDate(part.recorded_at, true)} · {formattedTime(part.recorded_at)}</time>
                            <span>{formatMatchDuration(part.duration_ms)}</span>
                            <div>
                              <button class={controlStyles.control} type="button" disabled={!props.actionable} onClick={() => props.onOpenGame(part.timestamp, props.snapshotOrigin)}>Play</button>
                              <button class={controlStyles.control} type="button" disabled={props.busyId !== null || !props.actionable} onClick={() => props.onToggleSaved(part, props.snapshotOrigin)}>{part.saved ? "Unsave" : "Save"}</button>
                              <Show when={!part.saved}>
                                <button class={controlStyles.control} type="button" disabled={props.busyId !== null || !props.actionable} onClick={() => props.onDeleteGame(part, props.snapshotOrigin)}>Delete</button>
                              </Show>
                            </div>
                          </li>
                        )}</For>
                      </ol>
                    </section>
                  </Show></>
                );
              }}
            </For>
          </section>
        </div>
      </Show>
      <div class={matchStyles.toolbar}>
        <Show when={props.history?.laterPage}><button type="button" class={matchStyles.savedFilter} onClick={props.onHistoryFirst} disabled={paginationDisabled()}>First page</button></Show>
        <Show when={props.history?.page?.next}><button type="button" class={matchStyles.savedFilter} onClick={props.onHistoryNext} disabled={paginationDisabled()}>Next 50 matches</button></Show>
        <Show when={groups().size > 0 && filteredGames().length === 0 && !updating()}><span>No observed matches. Coverage may be incomplete.</span></Show>
      </div>
    </section>
  );
}

function MatchHistoryEmptyState() {
  return (
    <section class={matchStyles.emptyState}>
      <h2>No matches yet</h2>
      <p>Recordings appear here when Chronobreak can identify the match.</p>
    </section>
  );
}

function ClipsLibrary(props: Props) {
  return (
    <>
      <section class={`${styles.libraryIntro} ${styles.clipsIntro}`}>
        <div class={styles.introActions}>
          <span>{props.clips.length} LOCAL FILES</span>
          <button
            class={`${controlStyles.control} ${controlStyles.iconLabel}`}
            type="button"
            onClick={props.onOpenClipsFolder}
          >
            <span>Open clips folder</span>
            <ExternalLinkIcon size={14} />
          </button>
        </div>
      </section>

      <Show
        when={props.clips.length > 0}
        fallback={
          <EmptyState
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
                  disabled={!props.actionable}
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
                    <em>{(() => {
                      const state = props.durationStates?.[clip.filename];
                      return state?.state === "available" ? formatDuration(state.duration_ms)
                        : state?.state === "unavailable" ? "Unavailable"
                        : clip.duration_ms === null ? "—" : formatDuration(clip.duration_ms);
                    })()}</em>
                    <i class={styles.playGlyph} aria-hidden="true">
                      <PlayIcon size={14} />
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
                <Show when={props.durationStates?.[clip.filename]?.state === "unavailable" && props.onRetryDuration}>
                  <button type="button" disabled={!props.actionable} onClick={() => props.onRetryDuration!(clip.filename)}>Retry duration</button>
                </Show>
                <button
                  class={`${controlStyles.control} ${styles.clipDelete}`}
                  type="button"
                  data-danger
                  onClick={() => props.onDeleteClip(clip, props.snapshotOrigin)}
                  disabled={props.busyId === clip.filename || !props.actionable}
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

function EmptyState(props: { title: string; detail: string }) {
  return (
    <section class={styles.emptyState}>
      <h2>{props.title}</h2>
      <p>{props.detail}</p>
    </section>
  );
}
export default LibraryScreen;
