import { localDayBounds } from "../format";
import type { HistoryQuery } from "../leagueIndex";
import LibraryFilters, { emptyAdvanced, filterDays, type HistoryOptions, type SharedFilters } from "./LibraryFilters";

export type HistoryFilters = SharedFilters & HistoryOptions;

export const emptyHistoryFilters = (): HistoryFilters => ({
  ...emptyAdvanced, queue: null, champions: [], tags: [], favorites: false, player: "",
});

export function historyQueryForFilters(filters: HistoryFilters, now = new Date()): HistoryQuery {
  const query: HistoryQuery = { limit: 50 };
  if (filters.queue) query.queue = filters.queue;
  if (filters.champions.length) query.played_champions = filters.champions;
  if (filters.tags.length) query.tags = filters.tags.map(tag => tag.id);
  if (filters.favorites) query.saved_only = true;
  if (filters.player) query.player_riot_id = filters.player;
  if (filters.result) query.outcome = filters.result;

  const days = filterDays(filters, now);
  const { from, before } = localDayBounds(days.from, days.to);
  if (from !== undefined) query.recorded_from = from;
  if (before !== undefined) query.recorded_before = before;
  return query;
}

export default function MatchHistoryFilters(props: {
  filters: HistoryFilters;
  onChange: (filters: HistoryFilters) => void;
  champions: readonly string[];
  championPortrait: (name: string) => string | null;
  tagToken?: string;
  onBenchmarkInput: (event: InputEvent & { currentTarget: HTMLInputElement }) => boolean;
  onClear: () => void;
}) {
  return <LibraryFilters filters={props.filters} history={props.filters}
    onChange={patch => props.onChange({ ...props.filters, ...patch })} onClear={props.onClear}
    champions={props.champions} championPortrait={props.championPortrait}
    tagToken={props.tagToken} onBenchmarkInput={props.onBenchmarkInput} />;
}
