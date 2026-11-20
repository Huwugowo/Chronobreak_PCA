import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup } from "solid-js";
import type { HistoryQuery, HistoryQueue, PersonalMoment } from "../leagueIndex";
import { readClipTagSuggestions } from "../managedClips";
import { queueLabels } from "../matchQueues";
import { CheckIcon, FilterIcon, SearchIcon, StarIcon, TagIcon } from "../ui/icons";
import {
  ChampionAutocomplete, ClearFiltersButton, FilterCheckbox, FilterChip, FilterDropdownTrigger, FilterTabButton,
  datePresets as dateLabels, filterControlStyles as styles,
} from "./FilterControls";
import historyStyles from "./MatchHistoryFilters.module.css";

const queueGroups = {
  Ranked: ["ranked_solo_duo", "ranked_flex", "ranked_5s"],
  ARAM: ["aram_mayhem", "aram", "aram_mayhem_classic"],
  Other: ["other", "normal_draft", "swiftplay", "urf"],
} satisfies Record<string, HistoryQueue[]>;
const momentLabels: Record<PersonalMoment, string> = {
  pentakill: "My pentakill", quadra_kill: "My quadra kill",
  baron_steal: "My Baron steal", dragon_steal: "My dragon steal",
};
const resultLabels = { win: "Victory", loss: "Defeat" };
const dateOffsets = { today: [0, 1], yesterday: [-1, 0], week: [-6, 1], month: [-29, 1] };
const maxClipTags = 32;

type AdvancedFilters = { player: string; date: keyof typeof dateLabels | ""; result: keyof typeof resultLabels | "" };
export type HistoryFilters = AdvancedFilters & {
  queue: HistoryQueue | null; champions: string[]; moments: PersonalMoment[]; clipTags: string[]; favorites: boolean;
};
const emptyAdvanced: AdvancedFilters = { player: "", date: "", result: "" };
export const emptyHistoryFilters = (): HistoryFilters => ({
  ...emptyAdvanced, queue: null, champions: [], moments: [], clipTags: [], favorites: false,
});

export function historyQueryForFilters(filters: HistoryFilters, now = new Date()): HistoryQuery {
  const query: HistoryQuery = { limit: 50 };
  if (filters.queue) query.queue = filters.queue;
  if (filters.champions.length) query.played_champions = filters.champions;
  if (filters.moments.length) query.personal_moments = filters.moments;
  if (filters.clipTags.length) query.clip_tags = filters.clipTags;
  if (filters.favorites) query.saved_only = true;
  if (filters.player) query.player_riot_id = filters.player;
  if (filters.result) query.outcome = filters.result;
  if (filters.date) {
    const [from, before] = dateOffsets[filters.date];
    const midnight = (offset: number) => new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset).getTime() / 1000;
    query.recorded_from = midnight(from);
    query.recorded_before = midnight(before);
  }
  return query;
}

const tagKey = (name: string) => name.trim().toLowerCase();
const toggle = <T,>(values: T[], value: T) => values.includes(value) ? values.filter(item => item !== value) : [...values, value];
const validPlayer = (value: string) => {
  const parts = value.split("#");
  return parts.length === 2 && parts.every(part => part !== "" && part === part.trim())
    && !/\p{Cc}/u.test(value) && new TextEncoder().encode(value).length <= 256;
};
type Popup = keyof typeof queueGroups | "Tags" | "Filters";

type Props = {
  filters: HistoryFilters;
  onChange: (filters: HistoryFilters) => void;
  champions: readonly string[];
  championPortrait: (name: string) => string | null;
  clipTagToken?: string;
  clipTagsPending?: boolean;
  clipTagError?: string | null;
  onClipTagRetry?: () => void;
  onBenchmarkInput: (event: InputEvent & { currentTarget: HTMLInputElement }) => boolean;
  onClear: () => void;
};

export default function MatchHistoryFilters(props: Props) {
  const id = createUniqueId();
  const [popup, setPopup] = createSignal<Popup | null>(null);
  const [pinned, setPinned] = createSignal(false);
  const [draft, setDraft] = createSignal<AdvancedFilters>(emptyAdvanced);
  const [playerError, setPlayerError] = createSignal(false);
  const [clipPrefix, setClipPrefix] = createSignal("");
  const [clipChoices, setClipChoices] = createSignal<string[]>([]);
  const [clipLoading, setClipLoading] = createSignal(false);
  const [clipFailed, setClipFailed] = createSignal(false);
  const [clipRetry, setClipRetry] = createSignal(0);
  const [championReset, resetChampion] = createSignal(0);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let root!: HTMLDivElement;
  let championInput!: HTMLInputElement;

  const commit = (patch: Partial<HistoryFilters>) => props.onChange({ ...props.filters, ...patch });
  const popupId = (name: Popup) => `${id}-${name.toLowerCase()}`;
  const trigger = (name: Popup) => root.querySelector<HTMLButtonElement>(`[aria-controls="${popupId(name)}"]`);
  const close = (restore = false) => {
    clearTimeout(closeTimer);
    const name = popup();
    setPopup(null);
    setPinned(false);
    if (restore && name) trigger(name)?.focus();
  };
  const open = (name: Popup, pin: boolean) => {
    clearTimeout(closeTimer);
    setPopup(name);
    setPinned(pin);
  };
  const openByClick = (name: Popup) => {
    if (popup() === name && pinned()) { close(); return; }
    if (name === "Filters") {
      const { player, date, result } = props.filters;
      setDraft({ player, date, result });
      setPlayerError(false);
    }
    open(name, true);
  };
  const dismissOutside = (event: Event) => {
    if (event.target instanceof Node && !root.contains(event.target)) {
      close();
    }
  };
  document.addEventListener("pointerdown", dismissOutside);
  document.addEventListener("focusin", dismissOutside);
  onCleanup(() => {
    document.removeEventListener("pointerdown", dismissOutside);
    document.removeEventListener("focusin", dismissOutside);
    clearTimeout(closeTimer);
  });


  createEffect(() => {
    const token = props.clipTagToken;
    const prefix = clipPrefix().trim();
    clipRetry();
    setClipChoices([]);
    setClipFailed(false);
    setClipLoading(false);
    if (popup() !== "Tags" || !token) return;
    let current = true;
    setClipLoading(true);
    const timer = setTimeout(() => void readClipTagSuggestions(token, prefix).then(
      choices => { if (current) { setClipChoices(choices); setClipLoading(false); } },
      () => { if (current) { setClipFailed(true); setClipLoading(false); } },
    ), prefix ? 120 : 0);
    onCleanup(() => { current = false; clearTimeout(timer); });
  });
  const displayedTags = createMemo(() => {
    const byKey = new Map(props.filters.clipTags.map(tag => [tagKey(tag), tag]));
    for (const tag of clipChoices()) if (!byKey.has(tagKey(tag))) byKey.set(tagKey(tag), tag);
    return [...byKey.values()];
  });
  const toggleTag = (tag: string) => {
    const previous = props.filters.clipTags;
    const key = tagKey(tag);
    commit({ clipTags: previous.some(value => tagKey(value) === key)
      ? previous.filter(value => tagKey(value) !== key) : [...previous, tag] });
  };
  const chooseQueue = (queue: HistoryQueue | null, restore = false) => {
    if (props.filters.queue !== queue) commit({ queue });
    close(restore);
  };
  const QueueGroup = ({ name }: { name: keyof typeof queueGroups }) => {
    const choices: HistoryQueue[] = queueGroups[name];
    const expanded = () => popup() === name;
    const selected = () => choices.some(queue => queue === props.filters.queue);
    return (
      <div class={styles.anchor}
        onPointerEnter={event => {
          if (event.pointerType !== "mouse" || (pinned() && root.querySelector("[data-history-popup]:focus-within"))) return;
          open(name, false);
        }}
        onPointerLeave={event => {
          if (event.pointerType !== "mouse" || pinned()) return;
          closeTimer = setTimeout(() => { if (expanded() && !pinned()) close(); }, 150);
        }}>
        <FilterTabButton class={styles.queue} active={selected()} aria-expanded={expanded()}
          aria-controls={popupId(name)} onClick={() => openByClick(name)}
          onKeyDown={event => {
            if (event.key !== "ArrowDown") return;
            event.preventDefault();
            open(name, true);
            root.querySelector<HTMLButtonElement>(`#${popupId(name)} button`)?.focus();
          }}>{name}<i class={styles.chevron} aria-hidden="true" /></FilterTabButton>
        <div class={`${styles.panel} ${historyStyles.queuePanel}`} id={popupId(name)} hidden={!expanded()} data-history-popup>
          <For each={choices}>{queue =>
            <button type="button" aria-pressed={props.filters.queue === queue} onClick={() => chooseQueue(queue, true)}>
              {queueLabels[queue]}<Show when={props.filters.queue === queue}><CheckIcon size={15} /></Show>
            </button>
          }</For>
        </div>
      </div>
    );
  };
  const hasAdvanced = () => !!(props.filters.player || props.filters.date || props.filters.result);
  const hasCriteria = () => hasAdvanced() || props.filters.queue || props.filters.favorites
    || props.filters.champions.length || props.filters.moments.length || props.filters.clipTags.length;

  return (
    <div class={styles.filters} ref={root!} onKeyDown={event => {
      if (event.key === "Escape" && popup()) { event.preventDefault(); close(true); }
    }}>
      <nav class={styles.queues} aria-label="Match queue">
        <FilterTabButton class={styles.queue} active={props.filters.queue === null} aria-pressed={props.filters.queue === null}
          onClick={() => chooseQueue(null)}>All</FilterTabButton>
        <QueueGroup name="Ranked" />
        <QueueGroup name="ARAM" />
        <FilterTabButton class={styles.queue} active={props.filters.queue === "arena"} aria-pressed={props.filters.queue === "arena"}
          onClick={() => chooseQueue("arena")}>Arena</FilterTabButton>
        <QueueGroup name="Other" />
      </nav>
      <div class={styles.toolbar}>
        <ChampionAutocomplete champions={props.champions} selected={props.filters.champions} championPortrait={props.championPortrait}
          dismiss={!!popup()} reset={championReset()} inputRef={element => { championInput = element; }}
          leadingIcon={<SearchIcon size={16} />} onSelect={name => commit({ champions: [...props.filters.champions, name] })}
          onOpen={() => close()} onBenchmarkInput={props.onBenchmarkInput} />
        <div class={styles.anchor}>
          <FilterDropdownTrigger chevron active={!!(props.filters.moments.length || props.filters.clipTags.length)}
            aria-controls={popupId("Tags")} aria-expanded={popup() === "Tags"} onClick={() => openByClick("Tags")}>
            <TagIcon size={16} />Tags
          </FilterDropdownTrigger>
          <div class={`${styles.panel} ${styles.tagsPanel}`} id={popupId("Tags")} hidden={popup() !== "Tags"} data-history-popup>
            <fieldset><legend>Personal moments</legend>
              <For each={Object.keys(momentLabels) as PersonalMoment[]}>{moment =>
                <FilterCheckbox label={momentLabels[moment]} checked={props.filters.moments.includes(moment)}
                  onChange={() => commit({ moments: toggle(props.filters.moments, moment) })} />
              }</For>
            </fieldset>
            <fieldset><legend>Custom tags</legend>
              <Show when={props.clipTagToken}>
                <input class={styles.field} type="search" aria-label="Find custom tags" placeholder="Find custom tags…"
                  value={clipPrefix()} maxLength={40} onInput={event => setClipPrefix(event.currentTarget.value)} />
              </Show>
              <For each={displayedTags()}>{tag => {
                const checked = () => props.filters.clipTags.some(value => tagKey(value) === tagKey(tag));
                return <FilterCheckbox label={tag} checked={checked()} disabled={!checked() && props.filters.clipTags.length >= maxClipTags}
                  onChange={() => toggleTag(tag)} />;
              }}</For>
              <Show when={props.clipTagError || clipFailed()}><p role="status">Custom tags are unavailable.</p>
                <button type="button" class={styles.smallAction} onClick={() => props.clipTagError ? props.onClipTagRetry?.() : setClipRetry(n => n + 1)}>Try again</button>
              </Show>
              <Show when={!props.clipTagError && !clipFailed()}>
                <Show when={props.clipTagsPending || clipLoading()} fallback={
                  <Show when={!displayedTags().length}><p role="status">{clipPrefix().trim() ? "No matching tags" : "No custom tags yet"}</p></Show>
                }><p role="status">Loading custom tags…</p></Show>
              </Show>
            </fieldset>
          </div>
        </div>
        <button type="button" class={`${styles.tool} ${historyStyles.star}`} title="Favorites only" aria-label="Favorites only"
          aria-pressed={props.filters.favorites} data-active={props.filters.favorites} onClick={() => {
            close();
            commit({ favorites: !props.filters.favorites });
          }}><StarIcon size={16} filled={props.filters.favorites} /></button>
        <div class={styles.anchor}>
          <FilterDropdownTrigger active={hasAdvanced()} aria-controls={popupId("Filters")}
            aria-expanded={popup() === "Filters"} onClick={() => openByClick("Filters")}>
            <FilterIcon size={16} />Filters
          </FilterDropdownTrigger>
          <form class={`${styles.panel} ${styles.advanced}`} id={popupId("Filters")} hidden={popup() !== "Filters"} data-history-popup
            onSubmit={event => {
              event.preventDefault();
              const player = draft().player.trim();
              if (player && !validPlayer(player)) { setPlayerError(true); return; }
              commit({ ...draft(), player });
              close(true);
            }}>
            <label>Player in match<input class={styles.field} type="search" value={draft().player} placeholder="Name#TAG"
              autocomplete="off" maxLength={256} aria-invalid={playerError()} aria-describedby={playerError() ? `${id}-riot-help` : undefined}
              onInput={event => { setDraft(value => ({ ...value, player: event.currentTarget.value })); setPlayerError(false); }} /></label>
            <Show when={playerError()}><p id={`${id}-riot-help`} role="alert">Enter the full Riot ID, including #TAG.</p></Show>
            <div class={styles.fieldRow}>
              <label>Recorded<select class={styles.field} value={draft().date}
                onChange={event => setDraft(value => ({ ...value, date: event.currentTarget.value as AdvancedFilters["date"] }))}>
                <option value="">Any time</option>
                <For each={Object.keys(dateLabels) as Array<keyof typeof dateLabels>}>{date => <option value={date}>{dateLabels[date]}</option>}</For>
              </select></label>
              <label>Result<select class={styles.field} value={draft().result}
                onChange={event => setDraft(value => ({ ...value, result: event.currentTarget.value as AdvancedFilters["result"] }))}>
                <option value="">Any result</option><option value="win">Victory</option><option value="loss">Defeat</option>
              </select></label>
            </div>
            <button type="submit" class={styles.apply}>Apply filters</button>
          </form>
        </div>
      </div>
      <Show when={hasCriteria()}>
        <div class={styles.criteria} aria-label="Active filters">
          <For each={props.filters.champions}>{name => <FilterChip label={name} champion={name} championPortrait={props.championPortrait} onRemove={() => commit({ champions: toggle(props.filters.champions, name) })} />}</For>
          <For each={props.filters.moments}>{moment => <FilterChip label={momentLabels[moment]} onRemove={() => commit({ moments: toggle(props.filters.moments, moment) })} />}</For>
          <For each={props.filters.clipTags}>{tag => <FilterChip label={tag} onRemove={() => toggleTag(tag)} />}</For>
          <Show when={props.filters.queue}>{queue => <FilterChip label={queueLabels[queue()]} onRemove={() => chooseQueue(null)} />}</Show>
          <Show when={props.filters.player}><FilterChip label={props.filters.player} onRemove={() => commit({ player: "" })} /></Show>
          <Show when={props.filters.date ? dateLabels[props.filters.date] : null}>{label =>
            <FilterChip label={label()} onRemove={() => commit({ date: "" })} />}</Show>
          <Show when={props.filters.result ? resultLabels[props.filters.result] : null}>{label =>
            <FilterChip label={label()} onRemove={() => commit({ result: "" })} />}</Show>
          <ClearFiltersButton onClear={() => {
            close();
            setClipPrefix("");
            resetChampion(value => value + 1);
            props.onClear();
            championInput.focus();
          }} />
        </div>
      </Show>
    </div>
  );
}
