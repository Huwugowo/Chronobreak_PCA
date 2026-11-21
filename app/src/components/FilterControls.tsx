import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup, splitProps, type JSX } from "solid-js";
import styles from "./FilterControls.module.css";
import type { HistoryQueue } from "../leagueIndex";
import { queueLabels } from "../matchQueues";
import { CheckIcon } from "../ui/icons";

export { styles as filterControlStyles };

export const datePresets = {
  today: "Today",
  yesterday: "Yesterday",
  week: "Last 7 days",
  month: "Last 30 days",
} as const;

const searchKey = (value: string) => value.normalize("NFKD").toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");

function FilterTabButton(props: { active: boolean; children: JSX.Element } & JSX.ButtonHTMLAttributes<HTMLButtonElement>) {
  const [local, buttonProps] = splitProps(props, ["active", "class", "children"]);
  return <button type="button" {...buttonProps} class={`${styles.tab} ${local.class ?? ""}`} data-active={local.active}>{local.children}</button>;
}

type FilterQueueOption = {
  label: string;
  value?: HistoryQueue;
  children?: readonly HistoryQueue[];
};

const queueOptions: readonly FilterQueueOption[] = [
  { label: "All" },
  { label: "Ranked", children: ["ranked_solo_duo", "ranked_flex", "ranked_5s"] },
  { label: "ARAM", children: ["aram_mayhem", "aram", "aram_mayhem_classic"] },
  { label: "Arena", value: "arena" },
  { label: "Other", children: ["other", "normal_draft", "swiftplay", "urf"] },
];

export function FilterQueueTabs(props: {
  value: HistoryQueue | null;
  onChange: (value: HistoryQueue | null) => void;
  onOpen?: () => void;
  onExpandedChange?: (expanded: boolean) => void;
}) {
  const id = createUniqueId();
  const [open, setOpen] = createSignal<string>();
  const [pinned, setPinned] = createSignal(false);
  let closeTimer: ReturnType<typeof setTimeout> | undefined;
  let root!: HTMLElement;
  const close = () => { clearTimeout(closeTimer); setOpen(undefined); setPinned(false); props.onExpandedChange?.(false); };
  const dismissOutside = (event: Event) => {
    if (open() && event.target instanceof Node && !root.contains(event.target)) close();
  };
  document.addEventListener("pointerdown", dismissOutside);
  document.addEventListener("focusin", dismissOutside);
  onCleanup(() => {
    clearTimeout(closeTimer);
    document.removeEventListener("pointerdown", dismissOutside);
    document.removeEventListener("focusin", dismissOutside);
  });
  const select = (value: HistoryQueue | null, restore?: HTMLButtonElement) => {
    props.onChange(value);
    close();
    restore?.focus();
  };
  const active = (option: FilterQueueOption) => option.children
    ? props.value !== null && option.children.includes(props.value)
    : (option.value ?? null) === props.value;
  const expand = (name: string, pin: boolean) => {
    clearTimeout(closeTimer);
    props.onOpen?.();
    setOpen(name);
    setPinned(pin);
    props.onExpandedChange?.(true);
  };
  return <nav ref={root} class={styles.queues} aria-label="Match queue" onKeyDown={event => {
    if (event.key === "Escape" && open()) {
      event.preventDefault();
      const trigger = root.querySelector<HTMLButtonElement>('button[aria-expanded="true"]');
      close();
      trigger?.focus();
    }
  }}>
    <For each={queueOptions}>{option => {
      const popupId = `${id}-${option.label.toLowerCase()}`;
      const expanded = () => open() === option.label;
      let trigger!: HTMLButtonElement;
      if (!option.children) return <FilterTabButton class={styles.queue} active={active(option)} aria-pressed={active(option)}
        onClick={() => select(option.value ?? null)}>{option.label}</FilterTabButton>;
      return <div class={styles.anchor} onPointerEnter={event => {
        if (event.pointerType === "mouse" && !(pinned() && root.querySelector(`.${styles.queuePanel}:focus-within`))) {
          expand(option.label, false);
        }
      }} onPointerLeave={event => {
        if (event.pointerType !== "mouse" || pinned()) return;
        closeTimer = setTimeout(() => { if (expanded() && !pinned()) close(); }, 150);
      }}>
        <FilterTabButton ref={trigger!} class={styles.queue} active={active(option)} aria-expanded={expanded()} aria-controls={popupId}
          onClick={() => { if (expanded() && pinned()) close(); else expand(option.label, true); }}
          onKeyDown={event => {
            if (event.key !== "ArrowDown") return;
            event.preventDefault();
            expand(option.label, true);
            document.getElementById(popupId)?.querySelector("button")?.focus();
          }}>
          {option.label}<i class={styles.chevron} aria-hidden="true" />
        </FilterTabButton>
        <div class={`${styles.panel} ${styles.queuePanel}`} id={popupId} hidden={!expanded()}>
          <For each={option.children}>{choice => <button type="button" aria-pressed={props.value === choice} onClick={() => select(choice, trigger)}>
            {queueLabels[choice]}<Show when={props.value === choice}><CheckIcon size={15} /></Show>
          </button>}</For>
        </div>
      </div>;
    }}</For>
  </nav>;
}

export function FilterChip(props: {
  label: string;
  champion?: string;
  championPortrait?: (name: string) => string | null;
  onRemove: () => void;
}) {
  const portrait = () => props.champion && props.championPortrait?.(props.champion);
  return <button type="button" class={styles.chip} aria-label={`Remove ${props.label}`} onClick={props.onRemove}>
    <Show when={portrait()} keyed>{src => <span class={styles.portrait} aria-hidden="true"><img src={src} alt="" decoding="async" onError={event => { event.currentTarget.hidden = true; }} /></span>}</Show>
    {props.label}<span aria-hidden="true">×</span>
  </button>;
}

export function ClearFiltersButton(props: { onClear: () => void }) {
  return <button type="button" class={styles.clear} onClick={props.onClear}>Clear all</button>;
}

export function FilterDropdownTrigger(props: { active?: boolean; chevron?: boolean; children: JSX.Element } & JSX.ButtonHTMLAttributes<HTMLButtonElement>) {
  const [local, buttonProps] = splitProps(props, ["active", "chevron", "children", "class"]);
  return <button type="button" {...buttonProps} class={`${styles.tool} ${local.class ?? ""}`} data-active={local.active ?? false}>
    {local.children}<Show when={local.chevron}><i class={styles.chevron} aria-hidden="true" /></Show>
  </button>;
}

export function ChampionAutocomplete(props: {
  champions: readonly string[];
  selected: readonly string[];
  championPortrait: (name: string) => string | null;
  onSelect: (name: string) => void;
  onOpen?: () => void;
  onClose?: () => void;
  inputRef?: (element: HTMLInputElement) => void;
  leadingIcon?: JSX.Element;
  onBenchmarkInput?: (event: InputEvent & { currentTarget: HTMLInputElement }) => boolean;
  pending?: boolean;
  dismiss?: boolean;
  reset?: number;
  error?: string | null;
  onRetry?: () => void;
  label?: string;
  placeholder?: string;
  maxLength?: number;
  value?: string;
  onValueChange?: (value: string) => void;
}) {
  const id = createUniqueId();
  const optionsId = `${id}-champions`;
  const [draftText, setDraftText] = createSignal("");
  const text = () => props.value ?? draftText();
  const updateText = (value: string) => {
    if (text() === value) return;
    if (props.onValueChange) props.onValueChange(value);
    else setDraftText(value);
  };
  const [open, setOpen] = createSignal(false);
  const [activeOption, setActiveOption] = createSignal(-1);
  let input!: HTMLInputElement;
  let picker!: HTMLDivElement;
  const catalog = createMemo(() => props.champions
    .map(name => ({ name, key: searchKey(name) })).sort((a, b) => a.name.localeCompare(b.name)));
  const suggestions = createMemo(() => {
    const key = searchKey(text());
    const selected = new Set(props.selected);
    const exact: string[] = [], prefixes: string[] = [], contains: string[] = [];
    for (const champion of catalog()) {
      if (selected.has(champion.name) || !champion.key.includes(key)) continue;
      if (champion.key === key) exact.push(champion.name);
      else if (champion.key.startsWith(key)) prefixes.push(champion.name);
      else contains.push(champion.name);
    }
    return [...exact, ...prefixes, ...contains].slice(0, 6);
  });
  const show = () => { props.onOpen?.(); setOpen(true); };
  const close = () => { props.onClose?.(); setOpen(false); setActiveOption(-1); };
  createEffect(() => { if (props.dismiss) close(); });
  createEffect(() => { props.reset; setDraftText(""); setActiveOption(-1); });
  const choose = (name: string) => { props.onSelect(name); setDraftText(""); input.focus(); close(); };
  const expanded = () => open() && (!props.onValueChange || suggestions().length > 0 || !!props.pending || !!props.error);
  const onSearchInput = (value: string) => {
    updateText(value);
    show();
    setActiveOption(-1);
  };
  const optionId = (index: number) => `${optionsId}-${index}`;
  return <div ref={picker} class={`${styles.anchor} ${styles.championPicker}`} onFocusOut={event => {
    if (!(event.relatedTarget instanceof Node) || !picker.contains(event.relatedTarget)) close();
  }} onKeyDown={event => {
    if (event.key === "Escape" && open()) {
      event.preventDefault();
      input.focus();
      close();
    }
  }}>
    <label class={styles.searchBox}>
      <span class={styles.searchIcon} aria-hidden="true">{props.leadingIcon ?? "⌕"}</span>
      <input ref={element => { input = element; props.inputRef?.(element); }} type="search" role="combobox" aria-label={props.label ?? "Search played champions"} aria-autocomplete="list"
        aria-expanded={expanded()} aria-controls={optionsId} aria-activedescendant={expanded() && suggestions()[activeOption()] ? optionId(activeOption()) : undefined}
        placeholder={props.placeholder ?? "Search played champions…"} maxLength={props.maxLength}
        autocomplete="off" spellcheck={false} value={text()}
        onFocus={show} onClick={show}
        onCompositionEnd={event => onSearchInput(event.currentTarget.value)}
        onInput={event => {
          if (props.onBenchmarkInput?.(event)) return;
          if (!event.isComposing) onSearchInput(event.currentTarget.value);
        }}
        onKeyDown={event => {
          if (event.isComposing) return;
          const choices = suggestions();
          if (!choices.length) return;
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            show();
            setActiveOption(index => event.key === "ArrowDown" ? (index + 1) % choices.length
              : index <= 0 ? choices.length - 1 : index - 1);
          } else if (event.key === "Enter" && expanded() && (!props.onValueChange || activeOption() >= 0)) {
            event.preventDefault();
            choose(choices[activeOption()] ?? choices[0]);
          }
        }} />
    </label>
    <div class={`${styles.panel} ${styles.suggestions}`} hidden={!expanded()}>
      <div role="listbox" aria-label={props.label ?? "Played champions"} id={optionsId}>
        <For each={suggestions()}>{(name, index) =>
          <button type="button" id={optionId(index())} role="option" tabIndex={-1} aria-selected={index() === activeOption()}
            onMouseDown={event => event.preventDefault()} onClick={() => choose(name)}>
            <ChampionPortrait name={name} championPortrait={props.championPortrait} />{name}
          </button>
        }</For>
      </div>
      <Show when={props.error}>
        <p role="status">{props.error}</p>
        <Show when={props.onRetry}><button type="button" class={styles.smallAction} onClick={() => props.onRetry?.()}>Try again</button></Show>
      </Show>
      <Show when={!props.error && props.pending}><p role="status">Loading champions…</p></Show>
      <Show when={!props.error && !props.pending && open() && !suggestions().length}><p role="status">No played champions found</p></Show>
    </div>
  </div>;
}

function ChampionPortrait(props: { name: string; championPortrait: (name: string) => string | null }) {
  return <span class={styles.portrait} aria-hidden="true"><Show when={props.championPortrait(props.name)} keyed>{src => <img src={src} alt="" decoding="async" onError={event => { event.currentTarget.hidden = true; }} />}</Show></span>;
}
