import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup } from "solid-js";
import type { HistoryQueue, PersonalMoment } from "../leagueIndex";
import { MAX_TAG_FILTERS, tagChoices, type Tag } from "../managedClips";
import { queueLabels } from "../matchQueues";
import { FilterIcon, SearchIcon, StarIcon, TagIcon } from "../ui/icons";
import {
  ChampionAutocomplete, ClearFiltersButton, FilterCheckbox, FilterChip, FilterDropdownTrigger,
  FilterQueueTabs, datePresets, filterControlStyles as styles,
} from "./FilterControls";

const momentLabels: Record<PersonalMoment, string> = {
  pentakill: "My pentakill", quadra_kill: "My quadra kill",
  baron_steal: "My Baron steal", dragon_steal: "My dragon steal",
};
const resultLabels = { win: "Victory", loss: "Defeat" };
const dateOffsets = { today: [0, 0], yesterday: [-1, -1], week: [-6, 0], month: [-29, 0] };
const presets = Object.keys(datePresets) as (keyof typeof datePresets)[];
type DatePreset = keyof typeof datePresets;
export type DateFilter = { date: DatePreset | ""; from: string; to: string };
type Advanced = DateFilter & { result: keyof typeof resultLabels | "" };
export type SharedFilters = Advanced & { queue: HistoryQueue | null; champions: string[]; tags: string[] };
export type HistoryOptions = { player: string; moments: PersonalMoment[]; favorites: boolean };
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
type Popup = "Tags" | "My events" | "Filters";
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
  clips?: { search: string; onSearch: (value: string) => void; onOpenFolder: () => void };
}) {
  const id = createUniqueId();
  let root!: HTMLDivElement;
  let championInput!: HTMLInputElement;
  let searchInput: HTMLInputElement | undefined;
  const [popup, setPopup] = createSignal<Popup>();
  const [queueOpen, setQueueOpen] = createSignal(false);
  const [draft, setDraft] = createSignal<Advanced & { player: string }>({ ...emptyAdvanced, player: "" });
  const [customDates, setCustomDates] = createSignal(false);
  const [playerError, setPlayerError] = createSignal(false);
  const [prefix, setPrefix] = createSignal("");
  const [choices, setChoices] = createSignal<Tag[]>([]);
  const [tagLabels, setTagLabels] = createSignal<Record<string, string>>({});
  const [tagLoading, setTagLoading] = createSignal(false);
  const [tagFailed, setTagFailed] = createSignal(false);
  const [tagRetry, retryTags] = createSignal(0);
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

  createEffect(() => {
    const renamed = props.renamedTag;
    if (renamed) setTagLabels(labels => labels[renamed.id] === renamed.label
      ? labels : { ...labels, [renamed.id]: renamed.label });
    tagRetry();
    const token = props.tagToken;
    const term = prefix().trim();
    if (popup() !== "Tags") return;
    setTagFailed(false);
    setChoices([]);
    setTagLoading(!!token);
    if (!token) return;
    let live = true;
    const timer = setTimeout(() => void tagChoices(token, term).then(
      options => {
        if (!live) return;
        setChoices(options);
        setTagLoading(false);
      },
      () => { if (live) { setTagFailed(true); setTagLoading(false); } },
    ), term ? 120 : 0);
    onCleanup(() => { live = false; clearTimeout(timer); });
  });
  const displayedTags = createMemo(() => {
    const options = new Map<string, string>();
    for (const id of props.filters.tags) options.set(id, tagLabels()[id] ?? id);
    for (const tag of choices()) options.set(tag.id, tag.label);
    const renamed = props.renamedTag;
    if (renamed && options.has(renamed.id)) options.set(renamed.id, renamed.label);
    return options;
  });
  const toggleTag = (id: string, label: string) => {
    setTagLabels(labels => ({ ...labels, [id]: label }));
    props.onChange({ tags: toggle(props.filters.tags, id) });
  };
  const chooseQueue = (queue: HistoryQueue | null) => {
    if (props.filters.queue !== queue) props.onChange({ queue });
    close();
  };
  const moments = () => props.history?.moments ?? [];
  const changeMoment = (moment: PersonalMoment) => props.onChange({ moments: toggle(moments(), moment) });
  const activeDate = () => {
    const { date, from, to } = props.filters;
    if (date) return datePresets[date];
    if (from && to) return `${from} – ${to}`;
    return from ? `From ${from}` : to ? `Through ${to}` : undefined;
  };
  const advancedActive = () => !!(activeDate() || props.filters.result || props.history?.player);
  const hasCriteria = () => advancedActive() || props.filters.queue || props.filters.champions.length
    || props.filters.tags.length || moments().length || props.history?.favorites || props.clips?.search.trim();
  const invalidRange = () => {
    const { from, to } = draft();
    return !!(from && to && from > to);
  };

  return <div class={styles.filters} ref={root} onKeyDown={event => {
    if (event.key === "Escape" && popup() && !event.defaultPrevented) { event.preventDefault(); close(true); }
  }}>
    <FilterQueueTabs value={props.filters.queue} onOpen={() => close()} onExpandedChange={setQueueOpen} onChange={chooseQueue} />
    <div class={styles.toolbar}>
      <Show when={props.clips}>{clips =>
        <label class={`${styles.searchBox} ${styles.clipSearch}`}>
          <SearchIcon class={styles.searchIcon} size={16} />
          <input ref={searchInput} type="search" aria-label="Search clips" maxLength={80} value={clips().search}
            placeholder="Search clips by title, tag or champion…" onFocus={() => close()}
            onCompositionEnd={event => clips().onSearch(event.currentTarget.value)}
            onInput={event => { if (!event.isComposing) clips().onSearch(event.currentTarget.value); }} />
        </label>
      }</Show>
      <ChampionAutocomplete champions={props.champions} selected={props.filters.champions} championPortrait={props.championPortrait}
        dismiss={!!popup() || queueOpen()} reset={championReset()} inputRef={element => { championInput = element; }}
        leadingIcon={<SearchIcon size={16} />} onSelect={name => props.onChange({ champions: [...props.filters.champions, name] })}
        onOpen={() => { close(); props.onChampionOpen?.(); }} onClose={props.onChampionClose}
        pending={props.championPending} error={props.championError} onRetry={props.onChampionRetry}
        onBenchmarkInput={props.onBenchmarkInput} />
      <Show when={props.history}>
        <div class={styles.anchor}>
          <FilterDropdownTrigger chevron active={!!moments().length} aria-controls={popupId("My events")}
            aria-expanded={popup() === "My events"} onClick={() => togglePopup("My events")}>My events</FilterDropdownTrigger>
          <div class={`${styles.panel} ${styles.tagsPanel}`} id={popupId("My events")} hidden={popup() !== "My events"}>
            <fieldset><legend>My events</legend>
              <For each={Object.keys(momentLabels) as PersonalMoment[]}>{moment =>
                <FilterCheckbox label={momentLabels[moment]} checked={moments().includes(moment)} onChange={() => changeMoment(moment)} />
              }</For>
            </fieldset>
          </div>
        </div>
      </Show>
      <div class={styles.anchor}>
        <FilterDropdownTrigger chevron active={!!props.filters.tags.length} aria-controls={popupId("Tags")}
          aria-expanded={popup() === "Tags"} onClick={() => togglePopup("Tags")}><TagIcon size={16} />Tags</FilterDropdownTrigger>
        <div class={`${styles.panel} ${styles.tagsPanel}`} id={popupId("Tags")} hidden={popup() !== "Tags"}>
          <fieldset aria-label="Tags"><legend>Tags</legend>
            <Show when={props.tagToken}>
              <input class={styles.field} type="search" aria-label="Find tags" placeholder="Find tags…"
                value={prefix()} maxLength={40} onInput={event => setPrefix(event.currentTarget.value)} />
            </Show>
            <For each={[...displayedTags().keys()]}>{id => {
              const checked = () => props.filters.tags.includes(id);
              const label = () => displayedTags().get(id) ?? id;
              return <FilterCheckbox label={label()} checked={checked()} disabled={!checked() && props.filters.tags.length >= MAX_TAG_FILTERS}
                onChange={() => toggleTag(id, label())} />;
            }}</For>
            <Show when={tagFailed()}><p role="status">Tags are unavailable.</p>
              <button type="button" class={styles.smallAction} onClick={() => retryTags(n => n + 1)}>Try again</button>
            </Show>
            <Show when={tagLoading()}><p role="status">Loading tags…</p></Show>
            <Show when={!tagFailed() && !tagLoading() && !displayedTags().size}><p role="status">No matching tags</p></Show>
          </fieldset>
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
        </form>
      </div>
      <Show when={props.clips}>{clips =>
        <button class={`${styles.tool} ${styles.folder}`} type="button" onClick={clips().onOpenFolder}>Open clips folder</button>
      }</Show>
    </div>
    <Show when={hasCriteria()}>
      <div class={styles.criteria} aria-label="Active filters">
        <Show when={props.clips?.search.trim()}>{term =>
          <FilterChip label={`Search: ${term()}`} onRemove={() => props.clips?.onSearch("")} />
        }</Show>
        <For each={props.filters.champions}>{name => <FilterChip label={name} champion={name} championPortrait={props.championPortrait}
          onRemove={() => props.onChange({ champions: toggle(props.filters.champions, name) })} />}</For>
        <For each={moments()}>{moment => <FilterChip label={momentLabels[moment]} onRemove={() => changeMoment(moment)} />}</For>
        <For each={props.filters.tags}>{id => <FilterChip label={displayedTags().get(id) ?? id}
          onRemove={() => props.onChange({ tags: toggle(props.filters.tags, id) })} />}</For>
        <Show when={props.filters.queue}>{queue => <FilterChip label={queueLabels[queue()]} onRemove={() => chooseQueue(null)} />}</Show>
        <Show when={props.history?.player}>{player => <FilterChip label={player()} onRemove={() => props.onChange({ player: "" })} />}</Show>
        <Show when={activeDate()}>{label =>
          <FilterChip label={label()} onRemove={() => props.onChange({ date: "", from: "", to: "" })} />}</Show>
        <Show when={props.filters.result || undefined}>{result =>
          <FilterChip label={resultLabels[result()]} onRemove={() => props.onChange({ result: "" })} />}</Show>
        <ClearFiltersButton onClear={() => {
          close(); setPrefix(""); resetChampion(value => value + 1);
          props.onClear();
          (searchInput ?? championInput).focus();
        }} />
      </div>
    </Show>
  </div>;
}
