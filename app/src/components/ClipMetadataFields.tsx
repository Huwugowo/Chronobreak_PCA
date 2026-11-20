import { For, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { clipLabels, readClipTagSuggestions, usesManagedClips, type HighlightKind } from "../managedClips";
import controls from "../ui/controls.module.css";
import { FilterCheckbox } from "./FilterControls";
import styles from "./ManagedClips.module.css";

const predefinedOptions = Object.keys(clipLabels) as HighlightKind[];

export function ClipTagEditor(props: {
  token: string; customTags: string[]; predefinedTags: HighlightKind[];
  onCustomTagsChange: (tags: string[]) => void; onPredefinedTagsChange: (tags: HighlightKind[]) => void;
  disabled?: boolean;
}) {
  const [text, setText] = createSignal("");
  const [focused, setFocused] = createSignal(false);
  const [suggestions, setSuggestions] = createSignal<{ token: string; tags: string[] }>({ token: "", tags: [] });
  const kindForLabel = (label: string) => predefinedOptions.find(kind => clipLabels[kind].toLowerCase() === label.toLowerCase());
  const selected = createMemo(() => props.customTags.filter(tag => !kindForLabel(tag)));
  const isKnownCustom = (tag: string) => selected().some(value => value.toLowerCase() === tag.toLowerCase());
  createEffect(() => {
    if (!focused() || !usesManagedClips()) return;
    const prefix = text().trim();
    const token = props.token;
    let active = true;
    const timer = window.setTimeout(() => {
      void readClipTagSuggestions(token, prefix).then(
        tags => { if (active) setSuggestions({ token, tags }); },
        () => { if (active) setSuggestions({ token, tags: [] }); },
      );
    }, 180);
    onCleanup(() => { active = false; window.clearTimeout(timer); });
  });
  const add = (candidate = text().trim()) => {
    const tag = candidate.trim();
    const kind = kindForLabel(tag);
    if (kind) {
      if (!props.predefinedTags.includes(kind)) props.onPredefinedTagsChange([...props.predefinedTags, kind]);
      setText("");
      return;
    }
    if (!tag || props.customTags.length >= 32 || isKnownCustom(tag)) return;
    props.onCustomTagsChange([...props.customTags, tag]);
    setText("");
  };
  const checked = (kind: HighlightKind) => props.predefinedTags.includes(kind) ||
    props.customTags.some(tag => kindForLabel(tag) === kind);
  const removeBuiltIn = (kind: HighlightKind) => {
    props.onPredefinedTagsChange(props.predefinedTags.filter(tag => tag !== kind));
    props.onCustomTagsChange(props.customTags.filter(tag => kindForLabel(tag) !== kind));
  };
  const optionLabels = createMemo(() => {
    const term = text().trim().toLowerCase();
    const labels = new Map<string, string>();
    const add = (label: string) => {
      const key = label.toLowerCase();
      if (key.includes(term) && !labels.has(key)) labels.set(key, label);
    };
    for (const kind of predefinedOptions) add(clipLabels[kind]);
    if (suggestions().token === props.token) for (const tag of suggestions().tags) add(tag);
    for (const tag of selected()) add(tag);
    return [...labels.values()];
  });
  const isChecked = (label: string) => {
    const kind = kindForLabel(label);
    return kind ? checked(kind) : isKnownCustom(label);
  };
  const optionDisabled = (label: string) => props.disabled ||
    (!isChecked(label) && !kindForLabel(label) && props.customTags.length >= 32);
  const changeOption = (label: string, value: boolean) => {
    const kind = kindForLabel(label);
    if (kind) {
      if (value) props.onPredefinedTagsChange([...props.predefinedTags, kind]);
      else removeBuiltIn(kind);
    } else if (value) add(label); else props.onCustomTagsChange(props.customTags.filter(current => current.toLowerCase() !== label.toLowerCase()));
  };
  const canAdd = () => {
    const tag = text().trim();
    return !!tag && (!!kindForLabel(tag) || (!isKnownCustom(tag) && props.customTags.length < 32));
  };
  return <fieldset class={styles.tagFieldset} disabled={props.disabled}>
    <legend>Tags</legend>
    <div class={styles.tagEditor}>
      <div class={styles.tagInput}>
        <label>Find or create a tag
          <input value={text()} maxLength={40} onFocus={() => setFocused(true)} onBlur={() => setFocused(false)}
            onInput={event => setText(event.currentTarget.value)}
            onKeyDown={event => { if (event.key === "Enter" && !event.isComposing) { event.preventDefault(); add(); } }} />
        </label>
        <button class={controls.control} type="button" onClick={() => add()} disabled={!canAdd()}>Add tag</button>
      </div>
      <div class={styles.tagOptions} role="group" aria-label="Tag options">
        <For each={optionLabels()}>{label => <FilterCheckbox label={label} labelClass={styles.tagOption}
          checked={isChecked(label)} disabled={optionDisabled(label)}
          onChange={event => changeOption(label, event.currentTarget.checked)} />}</For>
      </div>
    </div>
  </fieldset>;
}
