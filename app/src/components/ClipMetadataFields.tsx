import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { MAX_MANUAL_TAGS, createTag, renameTag, visibleTagAssignments, type Tag, type TagAssignment, type TagDefinitionWriter } from "../managedClips";
import controls from "../ui/controls.module.css";
import styles from "./ManagedClips.module.css";
import { TagPicker } from "./TagPicker";

export function TagBadges(props: { tags: TagAssignment[]; class?: string; id?: string }) {
  const tags = createMemo(() => visibleTagAssignments(props.tags));
  return <Show when={tags().length}>
    <span id={props.id} classList={{ [styles.clipTags]: true, [props.class ?? ""]: !!props.class }}>
      <For each={tags()}>
        {tag => <span class={tag.source === "detected" ? styles.detectedBadge : styles.manualBadge}>{tag.label}</span>}
      </For>
    </span>
  </Show>;
}

export function TagEditor(props: {
  token: string; tags: Tag[]; onTagsChange: (tags: Tag[]) => void;
  writeTag: TagDefinitionWriter;
  detected: TagAssignment[]; disabled?: boolean; onTagRenamed?: (tag: Tag) => void;
  onPendingChange: (pending: boolean) => void;
}) {
  const [failed, setFailed] = createSignal(false);
  const [writing, setWriting] = createSignal(false);
  const [renaming, setRenaming] = createSignal<Tag>();
  const [newName, setNewName] = createSignal("");
  const detected = createMemo(() => new Set(props.detected.map(tag => tag.id)));
  const manualTags = createMemo(() => props.tags.filter(tag => !detected().has(tag.id)));
  const selected = createMemo(() => manualTags().map(tag => tag.id));
  let alive = true;
  const pending = (value: boolean) => { setWriting(value); props.onPendingChange(value); };
  onCleanup(() => { alive = false; props.onPendingChange(false); });
  createEffect(() => {
    if (manualTags().length !== props.tags.length) props.onTagsChange(manualTags());
  });
  const toggle = (tag: Tag, checked = !selected().includes(tag.id)) => {
    if (detected().has(tag.id)) return;
    props.onTagsChange(checked
      ? [...manualTags().filter(current => tag.tier === null || current.tier === null), tag]
      : manualTags().filter(current => current.id !== tag.id));
  };
  const create = async (label: string) => {
    pending(true);
    try {
      const tag = await props.writeTag(token => createTag(token, label));
      if (alive && !selected().includes(tag.id)) toggle(tag, true);
    } finally { if (alive) pending(false); }
  };
  const rename = async () => {
    const tag = renaming();
    const label = newName().trim();
    if (!tag || !label || writing()) return;
    pending(true);
    try {
      const updated = await props.writeTag(token => renameTag(token, tag.id, label));
      if (!alive) return;
      props.onTagsChange(manualTags().map(current => current.id === tag.id ? updated : current));
      props.onTagRenamed?.(updated);
      setRenaming(undefined);
      setFailed(false);
    } catch { if (alive) setFailed(true); } finally { if (alive) pending(false); }
  };
  return <fieldset class={styles.tagFieldset} disabled={props.disabled}>
    <legend>Tags</legend>
    <div class={styles.tagEditor}>
      <div class={styles.assignedTags}>
        <TagBadges tags={props.detected} />
        <For each={manualTags()}>{tag => <span>
          <button class={styles.tagPill} type="button" aria-label={`Remove tag ${tag.label}`}
            aria-pressed="true" disabled={writing()} onClick={() => toggle(tag, false)}>{tag.label} <span aria-hidden="true">×</span></button>
          <Show when={props.onTagRenamed && !tag.builtin}>
            <button class={styles.tagRename} type="button" aria-label={`Rename tag ${tag.label}`} disabled={writing()}
              onClick={() => { setNewName(tag.label); setRenaming(tag); }}>Rename</button>
          </Show>
        </span>}</For>
      </div>
      <TagPicker token={props.token} selected={selected()} onToggle={toggle} include={tag => !detected().has(tag.id)}
        known={[...props.detected, ...manualTags()]} onCreate={create} disabled={props.disabled || (writing() && !!renaming())}
        canAdd={tag => manualTags().length < MAX_MANUAL_TAGS || (tag?.tier != null && manualTags().some(current => current.tier != null))} />
      <Show when={renaming()}>
        <div class={styles.tagInput}>
          <label>New name for this tag in the library<input value={newName()} maxLength={40}
            onInput={event => setNewName(event.currentTarget.value)} /></label>
          <button type="button" class={controls.control} disabled={writing() || !newName().trim()} onClick={() => void rename()}>Save name</button>
          <button type="button" class={controls.control} disabled={writing()} onClick={() => setRenaming(undefined)}>Cancel rename</button>
        </div>
      </Show>
      <Show when={failed()}><small role="status">Tags are unavailable. Try again.</small></Show>
    </div>
  </fieldset>;
}
