import { For, Show, createSignal, createUniqueId, onCleanup } from "solid-js";
import type { HistoryQueue } from "../leagueIndex";
import { MAX_TAG_FILTERS, type Tag } from "../managedClips";
import { queueLabels } from "../matchQueues";
import { FilterIcon, SearchIcon, StarIcon, TagIcon } from "../ui/icons";
import {
  ChampionAutocomplete, ClearFiltersButton, FilterChip, FilterDropdownTrigger,
  FilterQueueTabs, datePresets, filterControlStyles as styles,
} from "./FilterControls";
import { TagPicker } from "./TagPicker";

const resultLabels = { win: "Victory", loss: "Defeat" };
const dateOffsets = { today: [0, 0], yesterday: [-1, -1], week: [-6, 0], month: [-29, 0] };
const presets = Object.keys(datePresets) as (keyof typeof datePresets)[];
type DatePreset = keyof typeof datePresets;
export type DateFilter = { date: DatePreset | ""; from: string; to: string };
type Advanced = DateFilter & { result: keyof typeof resultLabels | "" };
export type SharedFilters = Advanced & { queue: HistoryQueue | null; champions: string[]; tags: string[]; search?: string };
export type HistoryOptions = { player: string; favorites: boolean };
export const emptyAdvanced: Advanced = { date: "", from: "", to: "", result: "" };

function calendarDay(now: Date, offset: number) {
  const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset);
  return [date.getFullYear(), String(date.getMonth() + 1).padStart(2, "0"), String(date.getDate()).padStart(2, "0")].join("-");
}

export function filterDays(filter: DateFilter, now = new Date()) {
  if (!filter.date) return { from: filter.from, to: filter.to };
  const [from, to] = dateOffsets[filter.date];
  return { from: calendarDay(now, from), to: calendarDay(now, to) };
}

export function dateFilterForDays(from = "", to = "", now = new Date()): DateFilter {
  const date = presets.find(preset => {
    const [start, end] = dateOffsets[preset];
    return calendarDay(now, start) === from && calendarDay(now, end) === to;
  });
  return date ? { date, from: "", to: "" } : { date: "", from, to };
}

const toggle = <T,>(values: T[], value: T) => values.includes(value)
  ? values.filter(item => item !== value) : [...values, value];
const validPlayer = (value: string) => {
  const parts = value.split("#");
  return parts.length === 2 && parts.every(part => part !== "" && part === part.trim())
    && !/\p{Cc}/u.test(value) && new TextEncoder().encode(value).length <= 256;
};
type Popup = "Tags" | "Filters";
type ChampionProps = Parameters<typeof ChampionAutocomplete>[0];

export default function LibraryFilters(props: {
  filters: SharedFilters;
  onChange: (patch: Partial<SharedFilters & HistoryOptions>) => void;
  onClear: () => void;
  champions: readonly string[];
  championPortrait: ChampionProps["championPortrait"];
  onChampionOpen?: ChampionProps["onOpen"];
  onChampionClose?: ChampionProps["onClose"];
  championPending?: boolean;
  championError?: string | null;
  onChampionRetry?: () => void;
  onBenchmarkInput?: ChampionProps["onBenchmarkInput"];
  tagToken?: string;
  renamedTag?: Tag;
  history?: HistoryOptions;
  clips?: { onOpenFolder: () => void };
}) {
  const id = createUniqueId();
  let root!: HTMLDivElement;
  let championInput!: HTMLInputElement;
  const [popup, setPopup] = createSignal<Popup>();
  const [queueOpen, setQueueOpen] = createSignal(false);
  const [draft, setDraft] = createSignal<Advanced & Pick<HistoryOptions, "player">>({ ...emptyAdvanced, player: "" });
  const [customDates, setCustomDates] = createSignal(false);
  const [playerError, setPlayerError] = createSignal(false);
  const [chosenTags, setChosenTags] = createSignal<Record<string, Tag>>({});
  const [championReset, resetChampion] = createSignal(0);

  const popupId = (name: Popup) => `${id}-${name.toLowerCase().replace(" ", "-")}`;
  const close = (restore = false) => {
    const name = popup();
    setPopup(undefined);
    if (restore && name) root.querySelector<HTMLButtonElement>(`[aria-controls="${popupId(name)}"]`)?.focus();
  };
  const togglePopup = (name: Popup) => {
    if (popup() === name) { close(); return; }
    if (name === "Filters") {
      const { date, from, to, result } = props.filters;
      setDraft({ date, from, to, result, player: props.history?.player ?? "" });
      setCustomDates(!!(from || to));
      setPlayerError(false);
    }
    setPopup(name);
  };
  const dismissOutside = (event: Event) => {
    if (event.target instanceof Node && !root.contains(event.target)) close();
  };
  document.addEventListener("pointerdown", dismissOutside);
  document.addEventListener("focusin", dismissOutside);
  onCleanup(() => {
    document.removeEventListener("pointerdown", dismissOutside);
    document.removeEventListener("focusin", dismissOutside);
  });

  const knownTag = (id: string) => props.renamedTag?.id === id ? props.renamedTag : chosenTags()[id];
  const toggleTag = (tag: Tag) => {
    setChosenTags(tags => ({ ...tags, [tag.id]: tag }));
    props.onChange({ tags: toggle(props.filters.tags, tag.id) });
  };
  const chooseQueue = (queue: HistoryQueue | null) => {
    if (props.filters.queue !== queue) props.onChange({ queue });
    close();
  };
  const activeDate = () => {
    const { date, from, to } = props.filters;
    if (date) return datePresets[date];
    if (from && to) return `${from} – ${to}`;
    return from ? `From ${from}` : to ? `Through ${to}` : undefined;
  };
  const advancedActive = () => !!(activeDate() || props.filters.result || props.history?.player);
  const hasCriteria = () => advancedActive() || props.filters.queue || props.filters.champions.length
    || props.filters.tags.length || props.filters.search?.trim() || props.history?.favorites;
  const invalidRange = () => {
    const { from, to } = draft();
    return !!(from && to && from > to);
  };

  return <div class={styles.filters} ref={root} onKeyDown={event => {
    if (event.key === "Escape" && popup() && !event.defaultPrevented) { event.preventDefault(); close(true); }
  }}>
    <FilterQueueTabs value={props.filters.queue} onOpen={() => close()} onExpandedChange={setQueueOpen} onChange={chooseQueue} />
    <div class={styles.toolbar}>
      <ChampionAutocomplete champions={props.champions} selected={props.filters.champions} championPortrait={props.championPortrait}
        dismiss={!!popup() || queueOpen()} reset={championReset()} inputRef={element => { championInput = element; }}
        leadingIcon={<SearchIcon size={16} />}
        label={props.clips ? "Search clips" : undefined}
        placeholder={props.clips ? "Search clips by title, tag or champion…" : undefined}
        maxLength={props.clips ? 80 : undefined}
        value={props.clips ? props.filters.search ?? "" : undefined}
        onValueChange={props.clips ? search => props.onChange({ search }) : undefined}
        onSelect={name => props.onChange({
          champions: [...props.filters.champions, name], ...(props.clips ? { search: "" } : {}),
        })}
        onOpen={() => { close(); props.onChampionOpen?.(); }} onClose={props.onChampionClose}
        pending={props.championPending} error={props.championError} onRetry={props.onChampionRetry}
        onBenchmarkInput={props.onBenchmarkInput} />
      <div class={styles.anchor}>
        <FilterDropdownTrigger chevron active={!!props.filters.tags.length} aria-controls={popupId("Tags")}
          aria-expanded={popup() === "Tags"} onClick={() => togglePopup("Tags")}><TagIcon size={16} />Tags</FilterDropdownTrigger>
        <div class={`${styles.panel} ${styles.tagsPanel}`} id={popupId("Tags")} hidden={popup() !== "Tags"}>
          <Show when={popup() === "Tags"}>
            <TagPicker token={props.tagToken} selected={props.filters.tags} onToggle={toggleTag}
              known={props.filters.tags.map(knownTag).filter((tag): tag is Tag => !!tag)}
              include={tag => !props.history || tag.builtin} canAdd={() => props.filters.tags.length < MAX_TAG_FILTERS} autoFocus />
          </Show>
        </div>
      </div>
      <Show when={props.history}>{history =>
        <button type="button" class={`${styles.tool} ${styles.star}`} title="Favorites only" aria-label="Favorites only"
          aria-pressed={history().favorites} data-active={history().favorites}
          onClick={() => { close(); props.onChange({ favorites: !history().favorites }); }}>
          <StarIcon size={16} filled={history().favorites} />
        </button>
      }</Show>
      <div class={styles.anchor}>
        <FilterDropdownTrigger active={advancedActive()} aria-controls={popupId("Filters")}
          aria-expanded={popup() === "Filters"} onClick={() => togglePopup("Filters")}><FilterIcon size={16} />Filters</FilterDropdownTrigger>
        <form class={`${styles.panel} ${styles.advanced}`} id={popupId("Filters")} hidden={popup() !== "Filters"} onSubmit={event => {
          event.preventDefault();
          if (invalidRange()) return;
          const { player: rawPlayer, ...advanced } = draft();
          const player = rawPlayer.trim();
          if (player && !validPlayer(player)) { setPlayerError(true); return; }
          props.onChange(props.history ? { ...advanced, player } : advanced);
          close(true);
        }}>
          <Show when={props.history}>
            <label>Player in match<input class={styles.field} type="search" value={draft().player} placeholder="Name#TAG"
              autocomplete="off" maxLength={256} aria-invalid={playerError()} aria-describedby={playerError() ? `${id}-riot-help` : undefined}
              onInput={event => { setDraft(value => ({ ...value, player: event.currentTarget.value })); setPlayerError(false); }} /></label>
            <Show when={playerError()}><p id={`${id}-riot-help`} role="alert">Enter the full Riot ID, including #TAG.</p></Show>
          </Show>
          <div class={styles.fieldRow}>
            <label>{props.history ? "Recorded" : "Match date"}
              <select class={styles.field} value={customDates() ? "custom" : draft().date} onChange={event => {
                const date = event.currentTarget.value as Advanced["date"] | "custom";
                setCustomDates(date === "custom");
                setDraft(value => date === "custom" ? { ...value, date: "" } : { ...value, date, from: "", to: "" });
              }}>
                <option value="">Any time</option>
                <For each={presets}>{date => <option value={date}>{datePresets[date]}</option>}</For>
                <option value="custom">Custom range</option>
              </select>
            </label>
            <label>Result<select class={styles.field} value={draft().result}
              onChange={event => setDraft(value => ({ ...value, result: event.currentTarget.value as Advanced["result"] }))}>
              <option value="">Any result</option><option value="win">Victory</option><option value="loss">Defeat</option>
            </select></label>
          </div>
          <Show when={customDates()}>
            <div class={styles.fieldRow}>
              <label>From<input class={styles.field} type="date" value={draft().from}
                aria-invalid={invalidRange()} aria-describedby={invalidRange() ? `${id}-date-error` : undefined}
                onInput={event => setDraft(value => ({ ...value, from: event.currentTarget.value }))} /></label>
              <label>Through<input class={styles.field} type="date" value={draft().to}
                aria-invalid={invalidRange()} aria-describedby={invalidRange() ? `${id}-date-error` : undefined}
                onInput={event => setDraft(value => ({ ...value, to: event.currentTarget.value }))} /></label>
            </div>
          </Show>
          <Show when={invalidRange()}><p id={`${id}-date-error`} role="alert">Choose an end date on or after the start date.</p></Show>
          <button type="submit" class={styles.apply} disabled={invalidRange()}>Apply filters</button>
          <Show when={props.clips}>{clips =>
            <button class={styles.smallAction} type="button" onClick={() => { close(true); clips().onOpenFolder(); }}>Open clips folder</button>
          }</Show>
        </form>
      </div>
    </div>
    <Show when={hasCriteria()}>
      <div class={styles.criteria} aria-label="Active filters">
        <Show when={props.filters.search?.trim()}>{term =>
          <FilterChip label={`Search: ${term()}`} onRemove={() => props.onChange({ search: "" })} />
        }</Show>
        <For each={props.filters.champions}>{name => <FilterChip label={name} champion={name} championPortrait={props.championPortrait}
          onRemove={() => props.onChange({ champions: toggle(props.filters.champions, name) })} />}</For>
        <For each={props.filters.tags}>{id => <FilterChip label={knownTag(id)?.label ?? id}
          onRemove={() => props.onChange({ tags: toggle(props.filters.tags, id) })} />}</For>
        <Show when={props.filters.queue}>{queue => <FilterChip label={queueLabels[queue()]} onRemove={() => chooseQueue(null)} />}</Show>
        <Show when={props.history?.favorites}><FilterChip label="Favorites only" onRemove={() => props.onChange({ favorites: false })} /></Show>
        <Show when={props.history?.player}>{player => <FilterChip label={player()} onRemove={() => props.onChange({ player: "" })} />}</Show>
        <Show when={activeDate()}>{label =>
          <FilterChip label={label()} onRemove={() => props.onChange({ date: "", from: "", to: "" })} />}</Show>
        <Show when={props.filters.result || undefined}>{result =>
          <FilterChip label={resultLabels[result()]} onRemove={() => props.onChange({ result: "" })} />}</Show>
        <ClearFiltersButton onClear={() => {
          close(); resetChampion(value => value + 1);
          props.onClear();
          championInput.focus();
        }} />
      </div>
    </Show>
  </div>;
}
