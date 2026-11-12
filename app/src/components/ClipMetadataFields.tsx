import { For, Show, createEffect, createSignal, createUniqueId, onCleanup } from "solid-js";
import { highlightLabel, readClipTagSuggestions, usesManagedClips, type Availability, type Highlight } from "../managedClips";
import controls from "../ui/controls.module.css";
import styles from "./ManagedClips.module.css";

export function ClipTagInput(props: { token: string; tags: string[]; onChange: (tags: string[]) => void; disabled?: boolean }) {
  const list = createUniqueId();
  const [text, setText] = createSignal("");
  const [focused, setFocused] = createSignal(false);
  const [suggestions, setSuggestions] = createSignal<string[]>([]);
  createEffect(() => {
    if (!focused() || !usesManagedClips()) return;
    const prefix = text().trim();
    const token = props.token;
    let active = true;
    const timer = window.setTimeout(() => {
      void readClipTagSuggestions(token, prefix).then(
        tags => { if (active) setSuggestions(tags); },
        () => { if (active) setSuggestions([]); },
      );
    }, 180);
    onCleanup(() => { active = false; window.clearTimeout(timer); });
  });
  const add = () => {
    const tag = text().trim();
    if (!tag || props.tags.length >= 32) return;
    if (!props.tags.some(value => value.toLowerCase() === tag.toLowerCase())) props.onChange([...props.tags, tag]);
    setText("");
  };
  return <div class={styles.tagEditor}>
    <div class={styles.tagInput}>
      <label>Custom tags
        <input list={list} value={text()} maxLength={40} onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)} disabled={props.disabled || props.tags.length >= 32}
          onInput={event => setText(event.currentTarget.value)}
          onKeyDown={event => { if (event.key === "Enter") { event.preventDefault(); add(); } }} />
      </label>
      <datalist id={list}><For each={suggestions()}>{tag => <option value={tag} />}</For></datalist>
      <button class={controls.control} type="button" onClick={add} disabled={props.disabled || !text().trim() || props.tags.length >= 32}>Add tag</button>
    </div>
    <div class={styles.tags}>
      <For each={props.tags}>{tag =>
        <button class={controls.control} type="button" disabled={props.disabled} aria-label={`Remove tag ${tag}`}
          onClick={() => props.onChange(props.tags.filter(value => value !== tag))}>{tag} ×</button>
      }</For>
    </div>
  </div>;
}

export function ClipHighlights(props: {
  candidates: Highlight[]; availability: Availability; selected: number[];
  onChange: (selected: number[]) => void; disabled?: boolean;
}) {
  return <fieldset class={styles.highlights} disabled={props.disabled}>
    <legend>Highlights</legend>
    <Show when={props.availability === "unavailable"}><small>Event data is unavailable for this clip.</small></Show>
    <Show when={props.availability === "partial"}><small>Some event data is unavailable.</small></Show>
    <Show when={props.availability === "available" && props.candidates.length === 0}>
      <small>No highlight tags found in this range.</small>
    </Show>
    <For each={props.candidates}>{tag =>
      <label>
        <input type="checkbox" checked={props.selected.includes(tag.event_index)}
          onChange={event => props.onChange(event.currentTarget.checked
            ? [...props.selected, tag.event_index] : props.selected.filter(id => id !== tag.event_index))} />
        {highlightLabel(tag)}
      </label>
    }</For>
  </fieldset>;
}
