import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { createTag, renameTag, tagChoices, visibleTagAssignments, type Tag, type TagAssignment } from "../managedClips";
import controls from "../ui/controls.module.css";
import { FilterCheckbox } from "./FilterControls";
import styles from "./ManagedClips.module.css";

export function TagBadges(props: { tags: TagAssignment[]; class?: string; limit?: number; id?: string }) {
  const tags = createMemo(() => visibleTagAssignments(props.tags));
  const limit = () => props.limit ?? tags().length;
  const remaining = () => tags().length - limit();
  return <Show when={tags().length}>
    <span id={props.id} classList={{ [styles.clipTags]: true, [props.class ?? ""]: !!props.class }}>
      <For each={tags().slice(0, limit())}>
        {tag => <span class={tag.source === "detected" ? styles.detectedBadge : styles.manualBadge}>{tag.label}</span>}
      </For>
      <Show when={remaining() > 0}>
        <span class={styles.manualBadge} title={`${remaining()} more tags`}>+{remaining()}</span>
      </Show>
    </span>
  </Show>;
}

export function TagEditor(props: {
  token: string; tags: Tag[]; onTagsChange: (tags: Tag[]) => void;
  detected: TagAssignment[]; disabled?: boolean; onTagRenamed?: (tag: Tag) => void;
}) {
  const [text, setText] = createSignal("");
  const [choices, setChoices] = createSignal<Tag[]>([]);
  const [failed, setFailed] = createSignal(false);
  const [writing, setWriting] = createSignal(false);
  const [renaming, setRenaming] = createSignal<Tag>();
  const [newName, setNewName] = createSignal("");
  const detected = createMemo(() => new Set(props.detected.map(tag => tag.id)));
  const manualTags = createMemo(() => props.tags.filter(tag => !detected().has(tag.id)));
  const selected = createMemo(() => new Set(manualTags().map(tag => tag.id)));
  createEffect(() => {
    if (manualTags().length !== props.tags.length) props.onTagsChange(manualTags());
  });
  createEffect(() => {
    if (!props.token) return;
    const prefix = text().trim();
    let live = true;
    const timer = window.setTimeout(() => void tagChoices(props.token, prefix).then(
      tags => { if (live) { setChoices(tags); setFailed(false); } },
      () => { if (live) { setChoices([]); setFailed(true); } },
    ), prefix ? 120 : 0);
    onCleanup(() => { live = false; window.clearTimeout(timer); });
  });
  const options = createMemo(() => {
    const all = new Map<string, Tag>();
    for (const tag of choices()) if (!detected().has(tag.id)) all.set(tag.id, tag);
    for (const tag of manualTags()) all.set(tag.id, tag);
    return [...all.values()];
  });
  const toggle = (tag: Tag, checked: boolean) => {
    if (detected().has(tag.id)) return;
    props.onTagsChange(checked
      ? [...manualTags().filter(current => tag.tier === null || current.tier === null), tag]
      : manualTags().filter(current => current.id !== tag.id));
  };
  const create = async () => {
    const label = text().trim();
    if (!label || props.disabled || writing()) return;
    if (props.detected.some(tag => tag.label.localeCompare(label, undefined, { sensitivity: "accent" }) === 0)) {
      setText("");
      return;
    }
    const existing = options().find(tag => tag.label.localeCompare(label, undefined, { sensitivity: "accent" }) === 0);
    setWriting(true);
    try {
      const tag = existing ?? await createTag(props.token, label);
      if (!selected().has(tag.id)) toggle(tag, true);
      setText("");
      setFailed(false);
    } catch { setFailed(true); } finally { setWriting(false); }
  };
  const rename = async () => {
    const tag = renaming();
    const label = newName().trim();
    if (!tag || !label || writing()) return;
    setWriting(true);
    try {
      const updated = await renameTag(props.token, tag.id, label);
      props.onTagsChange(manualTags().map(current => current.id === tag.id ? updated : current));
      setChoices(tags => tags.map(current => current.id === tag.id ? updated : current));
      props.onTagRenamed?.(updated);
      setRenaming(undefined);
      setFailed(false);
    } catch { setFailed(true); } finally { setWriting(false); }
  };
  return <fieldset class={styles.tagFieldset} disabled={props.disabled || writing()}>
    <legend>Tags</legend>
    <div class={styles.tagEditor}>
      <TagBadges tags={props.detected} />
      <div class={styles.tagInput}>
        <label>Find or create a tag
          <input value={text()} maxLength={40}
            onInput={event => setText(event.currentTarget.value)}
            onKeyDown={event => { if (event.key === "Enter" && !event.isComposing) { event.preventDefault(); void create(); } }} />
        </label>
        <button class={controls.control} type="button" onClick={() => void create()} disabled={!text().trim()}>Add tag</button>
      </div>
      <div class={styles.tagOptions} role="group" aria-label="Tag options">
        <For each={options()}>{tag => <span>
          <FilterCheckbox label={tag.label} labelClass={styles.tagOption}
            checked={selected().has(tag.id)} onChange={event => toggle(tag, event.currentTarget.checked)} />
          <Show when={props.onTagRenamed && selected().has(tag.id) && !tag.builtin}>
            <button class={controls.control} type="button" aria-label={`Rename tag ${tag.label}`}
              onClick={() => { setNewName(tag.label); setRenaming(tag); }}>Rename tag</button>
          </Show>
        </span>}</For>
      </div>
      <Show when={renaming()}>
        <div class={styles.tagInput}>
          <label>New name for this tag in the library<input value={newName()} maxLength={40}
            onInput={event => setNewName(event.currentTarget.value)} /></label>
          <button type="button" class={controls.control} disabled={!newName().trim()} onClick={() => void rename()}>Save name</button>
          <button type="button" class={controls.control} onClick={() => setRenaming(undefined)}>Cancel rename</button>
        </div>
      </Show>
      <Show when={failed()}><small role="status">Tags are unavailable. Try again.</small></Show>
    </div>
  </fieldset>;
}
