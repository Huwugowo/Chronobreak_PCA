import { For, Show, createEffect, createMemo, createSignal, createUniqueId, onCleanup, onMount } from "solid-js";
import { tagChoices, type Tag } from "../managedClips";
import controls from "../ui/controls.module.css";
import styles from "./ManagedClips.module.css";

type Choice = { tag: Tag; creatable?: never } | { creatable: string; tag?: never };
const key = (label: string) => label.trim().toLowerCase();

export function TagPicker(props: {
  token?: string;
  selected: readonly string[];
  onToggle: (tag: Tag) => void;
  include?: (tag: Tag) => boolean;
  known?: readonly Tag[];
  canAdd?: (tag?: Tag) => boolean;
  onCreate?: (label: string) => Promise<void>;
  autoFocus?: boolean;
  disabled?: boolean;
}) {
  const id = createUniqueId();
  const list = `${id}-tag-options`;
  let input!: HTMLInputElement;
  let picker!: HTMLDivElement;
  const [text, setText] = createSignal("");
  const [open, setOpen] = createSignal(false);
  const [index, setIndex] = createSignal(-1);
  const [found, setFound] = createSignal<Tag[]>([]);
  const [status, setStatus] = createSignal<"loading" | "ready" | "failed">("ready");
  const [retry, setRetry] = createSignal(0);
  const [creating, setCreating] = createSignal(false);
  const [createFailed, setCreateFailed] = createSignal(false);
  let outsidePress = false;
  const contains = (target: EventTarget | null) => target instanceof Node && picker.contains(target);
  const close = () => { setOpen(false); setIndex(-1); };
  onMount(() => {
    const trackPress = (event: PointerEvent) => { outsidePress = !contains(event.target); };
    const endPress = () => { outsidePress = false; };
    const closeAfterClick = (event: MouseEvent) => { if (!contains(event.target)) close(); };
    // Keep the in-flow suggestions in place until an outside click lands.
    // Closing on pointer-induced blur can move the Save or Export button mid-click.
    document.addEventListener("pointerdown", trackPress, true);
    document.addEventListener("pointerup", endPress, true);
    document.addEventListener("pointercancel", endPress, true);
    document.addEventListener("click", closeAfterClick, true);
    onCleanup(() => {
      document.removeEventListener("pointerdown", trackPress, true);
      document.removeEventListener("pointerup", endPress, true);
      document.removeEventListener("pointercancel", endPress, true);
      document.removeEventListener("click", closeAfterClick, true);
    });
    if (props.autoFocus) input.focus({ preventScroll: true });
  });

  createEffect(() => {
    retry();
    const token = props.token;
    const prefix = text().trim();
    if (!open()) return;
    setFound([]);
    setStatus(token ? "loading" : "ready");
    if (!token) return;
    let current = true;
    const timer = setTimeout(() => void tagChoices(token, prefix).then(
      tags => { if (current) { setFound(tags); setStatus("ready"); } },
      () => { if (current) setStatus("failed"); },
    ), prefix ? 120 : 0);
    onCleanup(() => { current = false; clearTimeout(timer); });
  });

  const choices = createMemo<Choice[]>(() => {
    const options = new Map(found().map(tag => [tag.id, tag]));
    for (const tag of props.known ?? []) options.set(tag.id, tag);
    const prefix = key(text());
    const visible = [...options.values()].filter(tag => key(tag.label).startsWith(prefix));
    const result: Choice[] = visible.filter(tag => props.include?.(tag) ?? true).map(tag => ({ tag }));
    if (props.onCreate && props.token && prefix && !/\p{Cc}/u.test(text()) && status() === "ready"
      && !visible.some(tag => key(tag.label) === prefix)) {
      result.push({ creatable: text().trim() });
    }
    return result;
  });
  const selected = (choice: Choice) => !!choice.tag && props.selected.includes(choice.tag.id);
  const enabled = (choice: Choice) => selected(choice) || (props.canAdd?.(choice.tag) ?? true);
  const optionId = (i: number) => `${list}-${i}`;
  const type = (value: string) => {
    setText(value);
    setOpen(true);
    setIndex(-1);
    setCreateFailed(false);
  };
  const choose = async (choice: Choice) => {
    if (props.disabled || creating() || !enabled(choice)) return;
    if (choice.tag) {
      props.onToggle(choice.tag);
      input.focus();
      return;
    }
    if (!props.onCreate) return;
    setCreating(true);
    setCreateFailed(false);
    try {
      await props.onCreate(choice.creatable);
      type("");
    } catch { setCreateFailed(true); }
    finally { setCreating(false); }
  };
  const move = (delta: number) => {
    setOpen(true);
    const available = choices().flatMap((choice, position) => enabled(choice) ? [position] : []);
    if (!available.length) return;
    const current = available.indexOf(index());
    const next = current < 0 ? available[delta > 0 ? 0 : available.length - 1]
      : available[(current + delta + available.length) % available.length];
    setIndex(next);
    document.getElementById(optionId(next))?.scrollIntoView?.({ block: "nearest" });
  };

  return <div ref={picker} class={styles.tagPicker} onFocusOut={event => {
    if (!outsidePress && !contains(event.relatedTarget)) close();
  }}>
    <input class={styles.tagSearch} ref={input} type="search" role="combobox" aria-label="Find tags"
      aria-autocomplete="list" aria-expanded={open()} aria-controls={list}
      aria-activedescendant={open() && choices()[index()] ? optionId(index()) : undefined}
      placeholder="Find tags…" maxLength={40} autocomplete="off" spellcheck={false}
      disabled={props.disabled} readOnly={creating()} value={text()}
      onFocus={() => setOpen(true)} onClick={() => setOpen(true)}
      onCompositionEnd={event => type(event.currentTarget.value)}
      onInput={event => { if (!event.isComposing) type(event.currentTarget.value); }}
      onKeyDown={event => {
        if (event.isComposing) return;
        if (event.key === "Escape" && open()) {
          event.preventDefault();
          close();
        } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          move(event.key === "ArrowDown" ? 1 : -1);
        } else if (event.key === "Enter") {
          event.preventDefault();
          const choice = choices()[index()] ?? choices().find(enabled);
          if (open() && choice) void choose(choice);
        }
      }} />
    <div hidden={!open()}>
      <div class={styles.tagOptions} id={list} role="listbox" aria-label="Tag options" aria-multiselectable="true"
        aria-busy={status() === "loading" || creating()}>
        <For each={choices()}>{(choice, position) =>
          <button type="button" role="option" tabIndex={-1} id={optionId(position())}
            class={styles.tagPill} aria-selected={selected(choice)} aria-disabled={!enabled(choice) || creating()}
            data-active={position() === index()} data-create={!!choice.creatable}
            onMouseDown={event => event.preventDefault()} onClick={() => void choose(choice)}>
            {choice.tag?.label ?? `Create “${choice.creatable}”`}
          </button>}
        </For>
      </div>
      <Show when={status() === "loading"}><small role="status">Loading tags…</small></Show>
      <Show when={creating()}><small role="status">Creating tag…</small></Show>
      <Show when={status() === "failed"}><small role="status">Tags are unavailable.</small>
        <button class={controls.control} type="button" onClick={() => { input.focus(); setRetry(n => n + 1); }}>Try again</button>
      </Show>
      <Show when={status() === "ready" && choices().length === 0}><small role="status">No matching tags</small></Show>
      <Show when={createFailed()}><small role="alert">Couldn’t create the tag. Try again.</small></Show>
    </div>
  </div>;
}
