import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { readClipGridChampions, type ClipFilters, type Tag } from "../managedClips";
import LibraryFilters, { dateFilterForDays, filterDays, type SharedFilters } from "./LibraryFilters";

export default function ClipsFilters(props: {
  filters: ClipFilters;
  onChange: (filters: ClipFilters) => void;
  token?: string;
  library: string | null;
  renamedTag?: Tag;
  championPortrait: (name: string) => string | null;
  onOpenFolder: () => void;
}) {
  const [champions, setChampions] = createSignal<string[]>([]);
  const [championOpen, setChampionOpen] = createSignal(false);
  const [championsLoading, setChampionsLoading] = createSignal(false);
  const [championsError, setChampionsError] = createSignal(false);
  const [championsRetry, retryChampions] = createSignal(0);
  let championToken: string | undefined;

  createEffect(() => {
    props.library;
    championToken = undefined;
    setChampions([]);
  });
  createEffect(() => {
    const token = props.token;
    championsRetry();
    if (!championOpen() || !token) return;
    if (championToken === token) {
      setChampionsLoading(false);
      setChampionsError(false);
      return;
    }

    let current = true;
    setChampionsLoading(true);
    setChampionsError(false);
    void readClipGridChampions(token).then(
      choices => {
        if (current) { championToken = token; setChampions(choices); }
      },
      () => { if (current) setChampionsError(true); },
    ).finally(() => { if (current) setChampionsLoading(false); });
    onCleanup(() => { current = false; });
  });

  const shared = createMemo((): SharedFilters => ({
    ...dateFilterForDays(props.filters.from, props.filters.to),
    queue: props.filters.queue ?? null,
    champions: props.filters.champions ?? [],
    tags: props.filters.tags ?? [],
    result: props.filters.outcome ?? "",
  }));
  const update = (patch: Partial<SharedFilters>) => {
    const next = { ...shared(), ...patch };
    const { from, to } = "date" in patch ? filterDays(next) : props.filters;
    props.onChange({
      search: props.filters.search,
      queue: next.queue ?? undefined,
      champions: next.champions.length ? next.champions : undefined,
      tags: next.tags.length ? next.tags : undefined,
      from: from || undefined,
      to: to || undefined,
      outcome: next.result || undefined,
    });
  };

  return <LibraryFilters filters={shared()} onChange={update} onClear={() => props.onChange({})}
    champions={champions()} championPortrait={props.championPortrait}
    onChampionOpen={() => setChampionOpen(true)} onChampionClose={() => setChampionOpen(false)}
    championPending={championsLoading() || !props.token}
    championError={championsError() ? "Couldn’t load champions." : null}
    onChampionRetry={() => retryChampions(value => value + 1)}
    tagToken={props.token} renamedTag={props.renamedTag} clips={{
      search: props.filters.search ?? "",
      onSearch: search => props.onChange({ ...props.filters, search: search || undefined }),
      onOpenFolder: props.onOpenFolder,
    }} />;
}
