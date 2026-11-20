import type { HistoryQuery } from "../leagueIndex";
import type { Tag } from "../managedClips";
import LibraryFilters, { emptyAdvanced, filterDays, type HistoryOptions, type SharedFilters } from "./LibraryFilters";

export type HistoryFilters = SharedFilters & HistoryOptions;

export const emptyHistoryFilters = (): HistoryFilters => ({
  ...emptyAdvanced, queue: null, champions: [], moments: [], tags: [], favorites: false, player: "",
});

export function historyQueryForFilters(filters: HistoryFilters, now = new Date()): HistoryQuery {
  const query: HistoryQuery = { limit: 50 };
  if (filters.queue) query.queue = filters.queue;
  if (filters.champions.length) query.played_champions = filters.champions;
  if (filters.moments.length) query.personal_moments = filters.moments;
  if (filters.tags.length) query.tags = filters.tags;
  if (filters.favorites) query.saved_only = true;
  if (filters.player) query.player_riot_id = filters.player;
  if (filters.result) query.outcome = filters.result;

  const localMidnight = (date: string, dayOffset = 0) => {
    const [year, month, day] = date.split("-").map(Number);
    return new Date(year, month - 1, day + dayOffset).getTime() / 1000;
  };
  const { from, to } = filterDays(filters, now);
  if (from) query.recorded_from = localMidnight(from);
  if (to) query.recorded_before = localMidnight(to, 1);
  return query;
}

export default function MatchHistoryFilters(props: {
  filters: HistoryFilters;
  onChange: (filters: HistoryFilters) => void;
  champions: readonly string[];
  championPortrait: (name: string) => string | null;
  tagToken?: string;
  renamedTag?: Tag;
  onBenchmarkInput: (event: InputEvent & { currentTarget: HTMLInputElement }) => boolean;
  onClear: () => void;
}) {
  return <LibraryFilters filters={props.filters} history={props.filters}
    onChange={patch => props.onChange({ ...props.filters, ...patch })} onClear={props.onClear}
    champions={props.champions} championPortrait={props.championPortrait}
    tagToken={props.tagToken} renamedTag={props.renamedTag} onBenchmarkInput={props.onBenchmarkInput} />;
}
