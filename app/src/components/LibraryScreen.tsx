import { For, Show, createEffect, createMemo, createSignal, on, onCleanup, type JSX } from "solid-js";
import { formatBytes, formatDate, formatDuration, formatMatchDuration, formatTimestampDate } from "../format";
import { ddragonAsset, hideFailedImage } from "../ddragonAssets";
import { previewMatchOutcomeFor, type PreviewMatchOutcome } from "../dev/matchHistoryPreview";
import type { ClipSummary, DdragonStatus, GameSummary, LibraryTab } from "../types";
import type { HistoryQuery, HistoryStats } from "../leagueIndex";
import type { HistoryState } from "../historyController";
import type { ClipCard, ClipTagUpdate, Tag } from "../managedClips";
import { TagBadges, TagEditor } from "./ClipMetadataFields";
import { ClipGroups } from "./ManagedClips";
import { recordBenchmarkSearchInput, recordBenchmarkSearchSchedule } from "../benchmarkSearch";
import type { DurationDisplay } from "../libraryController";
import type { LibraryOrigin } from "../libraryController";
import { MoreIcon, StarIcon } from "../ui/icons";
import matchStyles from "./MatchHistory.module.css";
import { ExternalLinkIcon, PlayIcon } from "../ui/icons";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

import ClipThumbnail from "./ClipThumbnail";
import MatchHistoryFilters, { emptyHistoryFilters, historyQueryForFilters } from "./MatchHistoryFilters";

type Props = {
  tab: LibraryTab;
  games: GameSummary[];
  clips: ClipSummary[];
  managedClips?: JSX.Element;
  onOpenManagedClip?: (clip: ClipCard, button: HTMLElement) => void;
  onPlaySingleClip?: (group: string, origin: LibraryOrigin, button: HTMLButtonElement) => void;
  pendingSingleClipGroup?: string;
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
  tagToken?: string;
  onEditMatchTags?: (group: string, previous: Tag[], selected: Tag[]) => Promise<boolean>;
  onTagRenamed?: (tag: Tag) => void;
  renamedTag?: Tag;
  clipTagUpdate?: ClipTagUpdate;
  onHistoryQuery?: (query: HistoryQuery, benchmarkActionId?: string) => void;
  onHistoryNext?: () => void;
  onHistoryFirst?: () => void;
};

const itemSlots = Array.from({ length: 7 }, (_, slot) => slot);

const unavailableStats: HistoryStats = { source: "unavailable", kills: null, deaths: null, assists: null, items: null, keystone_id: null, secondary_style: null, summoner_spells: null, creep_score: null, duration_seconds: null, evidence: null, final_evidence: null, game_tick: null };

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

const closeActionMenu = (event: MouseEvent) => {
  const current = event.currentTarget;
  if (!(current instanceof HTMLElement)) return;
  current.closest("details")?.removeAttribute("open");
};

function LibraryScreen(props: Props) {
  return (
    <div class={styles.library}>
      <Show when={props.tab === "games"} fallback={props.managedClips ?? <ClipsLibrary {...props} />}>
        <GamesLibrary {...props} />
      </Show>
    </div>
  );
}

function GamesLibrary(props: Props) {
  const [filters, setFilters] = createSignal(emptyHistoryFilters());
  const [benchmarkText, setBenchmarkText] = createSignal("");
  const [textPending, setTextPending] = createSignal(false);
  const [expandedMatch, setExpandedMatch] = createSignal<string | null>(null);
  const [expandedClips, setExpandedClips] = createSignal<string | null>(null);
  const [tagEdit, setTagEdit] = createSignal<{ group: string; previous: Tag[] }>();
  const [draftTags, setDraftTags] = createSignal<Tag[]>([]);
  const [savingTags, setSavingTags] = createSignal(false);
  const saveTags = async () => {
    const edit = tagEdit();
    if (!edit || savingTags() || !props.onEditMatchTags) return;
    setSavingTags(true);
    try {
      if (await props.onEditMatchTags(edit.group, edit.previous, draftTags())) setTagEdit(undefined);
    } finally { setSavingTags(false); }
  };
  createEffect(on(() => props.snapshotOrigin?.root, () => setTagEdit(undefined), { defer: true }));
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchActionId: string | undefined;
  let scheduledSearchActionId: string | undefined;
  createEffect(on(
    [benchmarkText, filters, () => JSON.stringify(props.snapshotOrigin)],
    (values, previous) => {
      clearTimeout(searchTimer);
      recordBenchmarkSearchSchedule("coalesced", scheduledSearchActionId);
      scheduledSearchActionId = undefined;
      const query = historyQueryForFilters(values[1]);
      if (values[0].trim()) query.search = values[0].trim();
      else if (searchActionId) query.search = null;
      const dispatch = () => {
        scheduledSearchActionId = undefined;
        setTextPending(false);
        if (searchActionId) props.onHistoryQuery?.(query, searchActionId);
        else props.onHistoryQuery?.(query);
        searchActionId = undefined;
      };
      // The synthetic benchmark keeps its original free-text query workload.
      if (previous && query.search && values[1] === previous[1] && values[2] === previous[2]) {
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
  const semanticQuery = () => !!(filters().queue || filters().player || filters().result || filters().moments.length);
  const filterActive = createMemo(() => Object.keys(historyQueryForFilters(filters())).length > 1);
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
  const paginationDisabled = () => !props.actionable || props.busyId !== null || updating() || !!props.history?.error;
  const mutationsBlocked = () => !props.actionable || props.busyId !== null;

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
  const fallbackCards = createMemo(() => [...groups().values()]
    .sort((a, b) => b[b.length - 1].recorded_at.localeCompare(a[a.length - 1].recorded_at))
    .map(parts => parts.reduce((best, part) => part.league_result || !best.league_result ? part : best)));
  const champions = createMemo(() => {
    const names = new Set<string>();
    for (const parts of groups().values()) {
      for (const part of parts) if (part.champion && part.champion !== "Unknown") names.add(part.champion);
    }
    return [...names];
  });

  const filteredGames = () => {
    const query = benchmarkText().trim().toLowerCase();
    const rows = props.history?.page?.rows;
    if (rows) {
      const matches = rows.map(row => partsFor(row.game).find(game => game.media_id === row.media_id && game.timestamp === row.game.timestamp)).filter((game): game is GameSummary => !!game);
      return matches;
    }
    if (semanticQuery() || filters().tags.length || filters().date) return [];
    return fallbackCards().filter(basicMatch).slice(0, 50);
    function basicMatch(game: GameSummary) {
      const matchesSearch =
        !query
        || game.champion.toLocaleLowerCase().includes(query)
        || game.game_mode.toLocaleLowerCase().includes(query);

      const parts = partsFor(game);
      const matchesChampion = !filters().champions.length || parts.some(part => filters().champions.includes(part.champion));
      const matchesSaved = !filters().favorites || parts.some(part => part.saved);

      return matchesSearch && matchesChampion && matchesSaved;
    }
  };
  const visibleGames = createMemo(() => new Map(filteredGames().map(game => [game.match_group!, game])));
  const rowsByMedia = createMemo(() => new Map(props.history?.page?.rows.map(row => [row.media_id, row])));
  const rowFor = (game: GameSummary) => {
    if (!game.media_id) return undefined;
    const row = rowsByMedia().get(game.media_id);
    return row?.game.timestamp === game.timestamp ? row : undefined;
  };
  const statsFor = (game: GameSummary) => rowFor(game)?.stats ?? unavailableStats;

  return (
    <section class={matchStyles.matchHistory} aria-label="Match History">
      <MatchHistoryFilters filters={filters()} onChange={setFilters} champions={champions()}
        championPortrait={name => ddragonAsset(props.ddragon, "champion", name)}
        tagToken={props.tagToken} renamedTag={props.renamedTag}
        onBenchmarkInput={event => {
          if (event.isTrusted) return false;
          searchActionId = recordBenchmarkSearchInput(event.currentTarget);
          if (!searchActionId) return false;
          setBenchmarkText(event.currentTarget.value);
          return true;
        }}
        onClear={() => { setBenchmarkText(""); setFilters(emptyHistoryFilters()); }} />
      <div class={matchStyles.summaryBar} aria-label="Match History summary">
        <Show when={visibleGames().size > 0}>
          <span>{visibleGames().size} {visibleGames().size === 1 ? "match" : "matches"}
            {props.history?.page?.next || props.history?.laterPage ? " shown" : ""}</span>
        </Show>
        <p class={matchStyles.coverage} role="status">{matchStatus()}</p>
        <Show when={visibleGames().size > 0}><span>Newest first</span></Show>
      </div>

      <Show
        when={groups().size > 0}
        fallback={<MatchHistoryEmptyState />}
      >
        <div class={matchStyles.matchScroller}>
          <section class={matchStyles.matchList} aria-label="Recorded matches" aria-busy={updating()}>
            <For each={[...visibleGames().keys()]}>
              {group => {
                const game = createMemo<GameSummary>(previous => visibleGames().get(group) ?? previous, visibleGames().get(group)!);
                const parts = createMemo(() => groups().get(group) ?? []);
                const multipart = () => parts().length > 1;
                const expanded = () => multipart() && expandedMatch() === group;
                const clipCount = () => {
                  const mediaId = game().media_id;
                  return mediaId ? rowsByMedia().get(mediaId)?.clip_count : undefined;
                };
                const clipsExpanded = () => expandedClips() === group;
                const matchClipQuery = { match_group: group, include_game_clock: true };
                const clipRegionId = `match-clips-${group}`;
                const outcome = () => displayedOutcome(rowFor(game())?.game ?? game());
                const health = () => replayHealth(game());
                const stats = () => statsFor(game());
                const matchTags = () => rowFor(game())?.tags ?? [];
                const participants = createMemo(() => rowFor(game())?.game.participants ?? game().participants, undefined, {
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
                  game().champion === "Unknown" ? null : game().champion,
                );
                const recordedAt = () => parts()[parts().length - 1]?.recorded_at ?? game().recorded_at;
                const date = () => recordedAt() ? formatDate(recordedAt(), true) : "—";
                const time = () => recordedAt() ? formattedTime(recordedAt()) : "—";
                const duration = () => {
                  const seconds = stats().duration_seconds;
                  return seconds === null ? game().duration_ms : seconds * 1_000;
                };

                return (
                  <><article class={matchStyles.matchItem} data-outcome={outcome() ?? undefined}>
                    <div class={matchStyles.matchSummary}>
                    <button
                      class={matchStyles.matchOpen}
                      type="button"
                      onClick={() => {
                        if (!props.actionable) return;
                        if (multipart()) setExpandedMatch(expanded() ? null : group);
                        else props.onOpenGame(game().timestamp, props.snapshotOrigin);
                      }}
                      disabled={!game().video_available}
                      aria-disabled={!props.actionable}
                      aria-expanded={multipart() ? expanded() : undefined}
                      aria-controls={multipart() ? `recording-parts-${group}` : undefined}
                      aria-label={
                        game().video_available
                          ? multipart()
                            ? `Choose a recording for ${game().champion} match from ${formatDate(recordedAt())} (${parts().length} recordings)`
                            : `Open ${game().champion} recording from ${formatDate(recordedAt())}`
                          : `${game().champion} recording unavailable`
                      }
                    >
                      <span class={matchStyles.matchContext}>
                        <strong class={matchStyles.queueLabel}>
                          {game().game_mode === "Unknown" ? "—" : game().game_mode}
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
                            <Show when={health()}>
                              <strong class={matchStyles.health}>{health()}</strong>
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

                        <Show when={outcome() && health()}>
                          <em class={matchStyles.health}>{health()}</em>
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
                              <For each={(stats().summoner_spells ?? (rowFor(game()) ? [null, null] : game().summoner_spells)).slice(0, 2)}>
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
                                <Show when={ddragonAsset(props.ddragon, "rune", stats().keystone_id ?? game().keystone_id)}>
                                  <img
                                    src={ddragonAsset(props.ddragon, "rune", stats().keystone_id ?? game().keystone_id)!}
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

                     <div class={matchStyles.matchActions}>
                       <Show when={clipCount() === 1 && props.onPlaySingleClip && props.snapshotOrigin}>
                         <button class={`${controlStyles.control} ${matchStyles.clipAction}`} type="button"
                           aria-disabled={!props.actionable}
                           aria-busy={props.pendingSingleClipGroup === group}
                           onClick={event => {
                             if (props.actionable) props.onPlaySingleClip!(group, props.snapshotOrigin!, event.currentTarget);
                           }}>
                           <PlayIcon size={14} /> Play clip
                         </button>
                       </Show>
                       <Show when={props.onOpenManagedClip && group && (clipCount() ?? 0) > 1}>
                         <button class={`${controlStyles.control} ${matchStyles.clipAction}`} type="button" aria-disabled={!props.actionable}
                           aria-expanded={clipsExpanded()} aria-controls={clipRegionId}
                           onClick={() => {
                             if (props.actionable) setExpandedClips(clipsExpanded() ? null : group);
                           }}>
                           {clipCount()} clips <span class={matchStyles.clipChevron} aria-hidden="true" />
                         </button>
                       </Show>
                       <Show when={!multipart()}>
                      <button
                        class={matchStyles.starButton}
                        type="button"
                        onClick={() => {
                          if (!mutationsBlocked()) props.onToggleSaved(game(), props.snapshotOrigin);
                        }}
                        aria-disabled={mutationsBlocked()}
                        aria-label={game().saved ? `Unsave ${game().champion} recording` : `Save ${game().champion} recording`}
                        title={game().saved ? "Saved" : "Save"}
                        data-saved={game().saved}
                      >
                        <StarIcon size={17} filled={game().saved} />
                      </button>

                      <Show when={!game().saved}>
                        <details class={matchStyles.moreMenu}>
                          <summary aria-label={`More actions for ${game().champion} recording`} title="More actions"
                            aria-disabled={props.busyId === game().timestamp}
                            onClick={event => { if (props.busyId === game().timestamp) event.preventDefault(); }}>
                            <MoreIcon size={17} />
                          </summary>
                          <div class={matchStyles.actionMenu}>
                            <button
                              type="button"
                              data-danger
                              onClick={(event: MouseEvent) => {
                                closeActionMenu(event);
                                props.onDeleteGame(game(), props.snapshotOrigin);
                              }}
                              disabled={props.busyId !== null || !props.actionable}
                            >
                              Delete recording
                            </button>
                          </div>
                        </details>
                      </Show>
                       </Show>
                     </div>
                    </div>
                  <Show when={matchTags().length || (rowFor(game()) && props.onEditMatchTags && props.tagToken)}>
                    <div class={matchStyles.matchTags}>
                      <TagBadges tags={matchTags()} />
                      <Show when={rowFor(game()) && props.onEditMatchTags && props.tagToken}>
                        <button class={controlStyles.control} type="button" disabled={mutationsBlocked() || savingTags()}
                          aria-expanded={tagEdit()?.group === group} aria-controls={`match-tags-${group}`}
                          onClick={() => {
                            if (tagEdit()?.group === group) { setTagEdit(undefined); return; }
                            const previous = matchTags();
                            setDraftTags(previous);
                            setTagEdit({ group, previous });
                          }}>Edit tags</button>
                      </Show>
                    </div>
                  </Show>
                  <Show when={tagEdit()?.group === group && props.tagToken}>
                    <div class={matchStyles.matchTagEditor} id={`match-tags-${group}`}>
                      <TagEditor token={props.tagToken!} tags={draftTags()} onTagsChange={setDraftTags}
                        disabled={savingTags()} onTagRenamed={props.onTagRenamed} />
                      <button type="button" class={controlStyles.control} disabled={savingTags()} onClick={() => void saveTags()}>Save tags</button>
                      <button type="button" class={controlStyles.control} disabled={savingTags()} onClick={() => setTagEdit(undefined)}>Cancel</button>
                    </div>
                  </Show>
                  <Show when={props.onOpenManagedClip && group && (clipCount() ?? 0) > 1}>
                    <section class={matchStyles.matchClipStrip} id={clipRegionId} aria-label="Clips from this match" hidden={!clipsExpanded()}>
                      <Show when={props.snapshotOrigin && clipsExpanded()}>
                        <ClipGroups token={props.snapshotOrigin!.token} ready={props.actionable}
                          query={matchClipQuery} onOpen={props.onOpenManagedClip!} tagUpdate={props.clipTagUpdate}
                          renamedTag={props.renamedTag} inline />
                      </Show>
                    </section>
                  </Show>
                  </article>
                  <Show when={expanded()}>
                    <section id={`recording-parts-${group}`} class={matchStyles.recordingParts} aria-label="Match recordings">
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
      </div>
      <Show when={groups().size > 0 && filterActive() && visibleGames().size === 0 && !updating()
        && !props.history?.initializing && !props.history?.error && props.history?.indexIssue?.kind !== "failed"
        }>
        <p class={matchStyles.filteredEmpty}>No matches for these filters.</p>
      </Show>
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
      <section class={styles.clipsIntro}>
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
            {(clip: ClipSummary) => (
              <article class={styles.clipCard}>
                <button
                  class={styles.clipOpen}
                  type="button"
                  onClick={() => props.onOpenClip(clip)}
                  disabled={!props.actionable}
                  aria-label={`Play ${clip.source_champion ?? "unknown"} clip`}
                >
                  <span class={styles.clipArtwork}>
                    <ClipThumbnail src={clip.thumbnail_url} />
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
