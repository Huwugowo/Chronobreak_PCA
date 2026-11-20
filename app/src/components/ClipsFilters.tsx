import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup } from "solid-js";
import { clipLabels, readClipGridChampions, readClipTagSuggestions, type ClipFilters, type ClipQueue } from "../managedClips";
import { FilterIcon, SearchIcon, TagIcon } from "../ui/icons";
import {
  ChampionAutocomplete, ClearFiltersButton, FilterCheckbox, FilterChip, FilterDropdownTrigger, FilterTabButton,
  datePresets, filterControlStyles as shared,
} from "./FilterControls";
import styles from "./ClipsFilters.module.css";

const queues: { label: string; value?: ClipQueue }[] = [
  { label: "All" }, { label: "Ranked", value: "ranked" }, { label: "ARAM", value: "aram" },
  { label: "Arena", value: "arena" }, { label: "Other", value: "other" },
];
const presets = [
  { label: datePresets.today, from: 0, to: 0 }, { label: datePresets.yesterday, from: -1, to: -1 },
  { label: datePresets.week, from: -6, to: 0 }, { label: datePresets.month, from: -29, to: 0 },
];
const resultLabels = { win: "Victory", loss: "Defeat" };
const tagKey = (tag: string) => tag.trim().toLowerCase();
const predefined = Object.values(clipLabels);
type Popup = "Tags" | "Filters";
type Advanced = Pick<ClipFilters, "from" | "to" | "outcome">;

function calendarDay(now: Date, offset: number) {
  const day = new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset);
  return [day.getFullYear(), String(day.getMonth() + 1).padStart(2, "0"), String(day.getDate()).padStart(2, "0")].join("-");
}

function dateChoices() {
  const now = new Date();
  return presets.map(({ label, from, to }) => ({ label, from: calendarDay(now, from), to: calendarDay(now, to) }));
}

function dateCaption(filters: Advanced, dates: ReturnType<typeof dateChoices>) {
  const preset = dates.find(date => date.from === filters.from && date.to === filters.to);
  if (preset) return preset.label;
  if (filters.from && filters.to) return `${filters.from} – ${filters.to}`;
  return filters.from ? `From ${filters.from}` : filters.to ? `Through ${filters.to}` : undefined;
}

export default function ClipsFilters(props: {
  filters: ClipFilters; onChange: (filters: ClipFilters) => void; token?: string; library: string | null;
  championPortrait: (name: string) => string | null; onOpenFolder: () => void;
}) {
  const id = createUniqueId();
  let root!: HTMLDivElement;
  let search!: HTMLInputElement;
  const triggers: Partial<Record<Popup, HTMLButtonElement>> = {};
  const [popup, setPopup] = createSignal<Popup>();
  const [prefix, setPrefix] = createSignal("");
  const [draft, setDraft] = createSignal<Advanced>({});
  const [dateChoice, setDateChoice] = createSignal("");
  const [dates, setDates] = createSignal(dateChoices());
  const [champions, setChampions] = createSignal<string[]>([]);
  const [championOpen, setChampionOpen] = createSignal(false);
  const [championsLoading, setChampionsLoading] = createSignal(false);
  const [championsError, setChampionsError] = createSignal(false);
  const [championsRetry, retryChampions] = createSignal(0);
  const [suggestions, setSuggestions] = createSignal<string[]>([]);
  const [tagsLoading, setTagsLoading] = createSignal(false);
  const [tagsError, setTagsError] = createSignal(false);
  const [tagsRetry, retryTags] = createSignal(0);
  let championToken: string | undefined;
  let lastTags: { token: string; prefix: string; choices: string[] } | undefined;

  const change = (patch: Partial<ClipFilters>) => props.onChange({ ...props.filters, ...patch });
  const close = (restoreFocus = false) => {
    const name = popup();
    setPopup(undefined);
    if (restoreFocus && name) triggers[name]?.focus();
  };
  createEffect(() => {
    props.library;
    championToken = undefined;
    lastTags = undefined;
    setChampions([]);
    setSuggestions([]);
  });
  const toggle = (name: Popup) => {
    if (popup() === name) { close(); return; }
    if (name === "Filters") {
      const { from, to, outcome } = props.filters;
      const choices = dateChoices();
      setDates(choices);
      setDraft({ from, to, outcome });
      setDateChoice(choices.find(date => date.from === from && date.to === to)?.label ?? (from || to ? "custom" : ""));
    }
    setPopup(name);
  };
  createEffect(() => {
    if (!popup()) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.contains(event.target)) close();
    };
    document.addEventListener("pointerdown", outside);
    onCleanup(() => document.removeEventListener("pointerdown", outside));
  });
  createEffect(() => {
    const token = props.token;
    championsRetry();
    if (!championOpen() || !token) return;
    if (championToken === token) {
      setChampionsLoading(false);
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
  createEffect(() => {
    const token = props.token;
    const term = prefix().trim();
    tagsRetry();
    if (popup() !== "Tags") return;
    setTagsError(false);
    const cached = lastTags?.token === token && lastTags?.prefix === term ? lastTags.choices : undefined;
    if (!token || cached) {
      setSuggestions(cached ?? []);
      setTagsLoading(false);
      return;
    }
    let current = true;
    setTagsLoading(true);
    setSuggestions([]);
    const timer = window.setTimeout(() => void readClipTagSuggestions(token, term).then(
      choices => {
        if (!current) return;
        lastTags = { token, prefix: term, choices };
        setSuggestions(choices);
        setTagsLoading(false);
      },
      () => { if (current) { setTagsError(true); setTagsLoading(false); } },
    ), term ? 120 : 0);
    onCleanup(() => { current = false; window.clearTimeout(timer); });
  });

  const selected = () => props.filters.tags ?? [];
  const selectedKeys = createMemo(() => new Set(selected().map(tagKey)));
  const tags = createMemo(() => {
    const term = tagKey(prefix());
    const choices = new Map<string, string>();
    for (const label of [...predefined.filter(tag => tagKey(tag).includes(term)), ...suggestions(), ...selected()]) {
      const key = tagKey(label);
      if (!choices.has(key)) choices.set(key, label);
    }
    return [...choices.values()];
  });
  const toggleTag = (tag: string) => {
    const key = tagKey(tag);
    const next = selectedKeys().has(key) ? selected().filter(value => tagKey(value) !== key) : [...selected(), tag];
    change({ tags: next.length ? next : undefined });
  };
  const championNames = () => props.filters.champions ?? [];
  const advancedActive = () => !!(props.filters.from || props.filters.to || props.filters.outcome);
  const hasFilters = () => advancedActive() || props.filters.search?.trim() || props.filters.queue ||
    championNames().length || selected().length;
  const invalidRange = () => {
    const { from, to } = draft();
    return !!(from && to && from > to);
  };

  return <div ref={root} class={`${shared.filters} ${styles.filters}`} onKeyDown={event => {
    if (event.key === "Escape" && popup() && !event.defaultPrevented) { event.preventDefault(); close(true); }
  }}>
    <nav class={`${shared.queues} ${styles.queues}`} aria-label="Clip queue">
      <For each={queues}>{queue => <FilterTabButton class={shared.queue}
        active={props.filters.queue === queue.value} aria-pressed={props.filters.queue === queue.value}
        onClick={() => { close(); if (props.filters.queue !== queue.value) change({ queue: queue.value }); }}>
        {queue.label}
      </FilterTabButton>}</For>
    </nav>
    <div class={`${shared.toolbar} ${styles.toolbar}`}>
      <label class={`${shared.searchBox} ${styles.search}`}>
        <SearchIcon class={shared.searchIcon} size={16} />
        <input ref={search} type="search" aria-label="Search clips" maxLength={80} value={props.filters.search ?? ""}
          placeholder="Search clips by title, tag or champion…" onFocus={() => close()}
          onCompositionEnd={event => change({ search: event.currentTarget.value || undefined })}
          onInput={event => { if (!event.isComposing) change({ search: event.currentTarget.value || undefined }); }} />
      </label>
      <ChampionAutocomplete champions={champions()} selected={championNames()} championPortrait={props.championPortrait}
        label="Champion" placeholder="Search champions…"
        dismiss={!!popup()}
        leadingIcon={<SearchIcon size={16} />}
        onOpen={() => { close(); setChampionOpen(true); }} onClose={() => setChampionOpen(false)}
        pending={championsLoading() || !props.token} error={championsError() ? "Couldn’t load champions." : null}
        onRetry={() => retryChampions(n => n + 1)}
        onSelect={name => change({ champions: [...championNames(), name] })} />
      <div class={`${shared.anchor} ${styles.anchor}`}>
        <FilterDropdownTrigger ref={button => { triggers.Tags = button; }} active={!!selected().length} chevron
          aria-controls={`${id}-tags`} aria-expanded={popup() === "Tags"} onClick={() => toggle("Tags")}>
          <TagIcon size={16} />Tags
        </FilterDropdownTrigger>
        <div class={`${shared.panel} ${shared.tagsPanel} ${styles.popup}`} id={`${id}-tags`} hidden={popup() !== "Tags"}>
          <fieldset aria-label="Tags">
            <input class={shared.field} type="search" aria-label="Find tags" placeholder="Find tags…"
              value={prefix()} maxLength={40} onInput={event => setPrefix(event.currentTarget.value)} />
            <For each={tags()}>{tag => {
              const checked = () => selectedKeys().has(tagKey(tag));
              return <FilterCheckbox label={tag} checked={checked()} disabled={!checked() && selected().length >= 32}
                onChange={() => toggleTag(tag)} />;
            }}</For>
            <Show when={tagsLoading()}><p role="status">Loading tags…</p></Show>
            <Show when={tagsError()}><p role="status">Couldn’t load saved tags.</p>
              <button type="button" class={shared.smallAction} onClick={() => retryTags(n => n + 1)}>Try again</button>
            </Show>
            <Show when={!tagsLoading() && !tagsError() && !tags().length}><p role="status">No matching tags</p></Show>
          </fieldset>
        </div>
      </div>
      <div class={`${shared.anchor} ${styles.anchor}`}>
        <FilterDropdownTrigger ref={button => { triggers.Filters = button; }} active={advancedActive()}
          aria-controls={`${id}-advanced`} aria-expanded={popup() === "Filters"}
          onClick={() => toggle("Filters")}><FilterIcon size={16} />Filters</FilterDropdownTrigger>
        <form class={`${shared.panel} ${shared.advanced} ${styles.popup}`} id={`${id}-advanced`} hidden={popup() !== "Filters"}
          onSubmit={event => {
            event.preventDefault();
            if (!invalidRange()) { change(draft()); close(true); }
          }}>
          <div class={shared.fieldRow}>
            <label>Match date<select class={shared.field} value={dateChoice()} onChange={event => {
              const value = event.currentTarget.value;
              const date = dates().find(option => option.label === value);
              setDateChoice(value);
              if (value !== "custom") setDraft(current => ({ ...current, from: date?.from, to: date?.to }));
            }}>
              <option value="">Any time</option>
              <For each={dates()}>{date => <option value={date.label}>{date.label}</option>}</For>
              <option value="custom">Custom range</option>
            </select></label>
            <label>Result<select class={shared.field} value={draft().outcome ?? ""} onChange={event => {
              const value = event.currentTarget.value as Advanced["outcome"] | "";
              setDraft(current => ({ ...current, outcome: value || undefined }));
            }}>
              <option value="">Any result</option><option value="win">Victory</option><option value="loss">Defeat</option>
            </select></label>
          </div>
          <Show when={dateChoice() === "custom"}>
            <div class={shared.fieldRow}>
              <label>From<input class={shared.field} type="date" value={draft().from ?? ""}
                aria-invalid={invalidRange()} aria-describedby={invalidRange() ? `${id}-date-error` : undefined}
                onInput={event => { const from = event.currentTarget.value; setDraft(current => ({ ...current, from: from || undefined })); }} /></label>
              <label>Through<input class={shared.field} type="date" value={draft().to ?? ""}
                aria-invalid={invalidRange()} aria-describedby={invalidRange() ? `${id}-date-error` : undefined}
                onInput={event => { const to = event.currentTarget.value; setDraft(current => ({ ...current, to: to || undefined })); }} /></label>
            </div>
          </Show>
          <Show when={invalidRange()}><p id={`${id}-date-error`} role="alert">Choose an end date on or after the start date.</p></Show>
          <button type="submit" class={shared.apply} disabled={invalidRange()}>Apply filters</button>
        </form>
      </div>
      <button class={`${shared.tool} ${styles.folder}`} type="button" onClick={props.onOpenFolder}>Open clips folder</button>
    </div>
    <Show when={hasFilters()}>
      <div class={shared.criteria} aria-label="Active filters">
        <Show when={props.filters.search?.trim()}>{term =>
          <FilterChip label={`Search: ${term()}`} onRemove={() => change({ search: undefined })} />}</Show>
        <Show when={queues.find(queue => queue.value && queue.value === props.filters.queue)}>{queue =>
          <FilterChip label={queue().label} onRemove={() => change({ queue: undefined })} />}</Show>
        <For each={championNames()}>{name => <FilterChip label={name} champion={name} championPortrait={props.championPortrait}
          onRemove={() => {
            const next = championNames().filter(value => value !== name);
            change({ champions: next.length ? next : undefined });
          }} />}</For>
        <For each={selected()}>{tag => <FilterChip label={tag} onRemove={() => toggleTag(tag)} />}</For>
        <Show when={dateCaption(props.filters, dates())}>{label =>
          <FilterChip label={label()} onRemove={() => change({ from: undefined, to: undefined })} />}</Show>
        <Show when={props.filters.outcome}>{result =>
          <FilterChip label={resultLabels[result()]} onRemove={() => change({ outcome: undefined })} />}</Show>
        <ClearFiltersButton onClear={() => { close(); setPrefix(""); props.onChange({}); search.focus(); }} />
      </div>
    </Show>
  </div>;
}
