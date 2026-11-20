import { For, Index, Show, createEffect, createMemo, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { createClipGrid } from "../clipGrid";
import { ddragonAsset, hideFailedImage } from "../ddragonAssets";
import { formatBytes, formatDate, formatDuration, formatMatchDuration } from "../format";
import { libraryError } from "../libraryController";
import { clipDurationMs, clipTitleError, clipLabels, readClipDetail, readClipPage, type ClipCard,
  type ClipCursor, type ClipEdit, type ClipFilters, type ClipGame, type ClipGridCard, type ClipGroup, type ClipQuery, type GroupCursor } from "../managedClips";
import { matchQueueLabel } from "../matchQueues";
import { parseReplayTick } from "../replayTime";
import { recordedGameClock } from "../replayProjection";
import type { DdragonStatus } from "../types";
import { ArrowLeftIcon, ArrowRightIcon, PlayIcon } from "../ui/icons";
import controls from "../ui/controls.module.css";
import { createModalFocus } from "../ui/createModalFocus";
import ClipsFilters from "./ClipsFilters";
import { ClipTagEditor } from "./ClipMetadataFields";
import ClipThumbnail from "./ClipThumbnail";
import ClipVideoPlayer from "./ClipVideoPlayer";
import ConfirmDialog from "./ConfirmDialog";
import libraryStyles from "./Library.module.css";
import matchStyles from "./MatchHistory.module.css";
import styles from "./ManagedClips.module.css";

const gameCaption = (game: ClipGame) => [
  game.played_at ? formatDate(game.played_at) : null, game.account,
  matchQueueLabel(game.queue_id, game.mode),
  game.outcome === "win" ? "Win" : game.outcome === "loss" ? "Loss" : null,
  game.final_kda?.map(value => value ?? "—").join("/"),
].filter(Boolean).join(" · ");

type OpenClip = (clip: ClipCard, button: HTMLElement) => void;

const spellSlots = [0, 1] as const;
const itemSlots = [0, 1, 2, 3, 4, 5, 6] as const;

function ClipChampionPortrait(props: { champion: string | null; ddragon: DdragonStatus; class: string; id?: string }) {
  const champion = () => props.champion === "Unknown" ? null : props.champion;
  const label = () => champion() ?? "Champion unavailable";
  return <span id={props.id} class={`${matchStyles.championSlot} ${props.class}`} role="img" aria-label={label()} title={label()}>
    <span aria-hidden="true">?</span>
    <Show when={ddragonAsset(props.ddragon, "champion", champion())} keyed>
      {src => <img src={src} alt="" loading="lazy" decoding="async" onError={hideFailedImage} />}
    </Show>
  </span>;
}

function ClipMatchHeader(props: { game: ClipGame; ddragon: DdragonStatus }) {
  const outcome = () => props.game.outcome;
  const result = () => outcome() === "win" ? "Victory" : outcome() === "loss" ? "Defeat" : "Unknown result";
  const queue = () => matchQueueLabel(props.game.queue_id, props.game.mode) ?? "Queue unavailable";
  const playedAt = () => props.game.played_at ?? props.game.part_recorded_at;

  return <div class={styles.matchRow} role="group" aria-label="Match summary" data-outcome={outcome() ?? undefined}>
    <div class={styles.matchContext}>
      <strong class={styles.matchResult}>{result()}</strong>
      <span class={`${styles.matchMeta} ${styles.matchQueue}`} title={queue()}>{queue()}</span>
    </div>
    <span class={matchStyles.championLoadout}>
      <ClipChampionPortrait champion={props.game.champion} ddragon={props.ddragon} class={styles.matchChampion} />
      <span class={`${matchStyles.spells} ${styles.matchSpells}`}>
        <For each={spellSlots}>{slot => {
          const spell = () => props.game.summoner_spells?.[slot];
          const label = () => spell()?.replace(/^Summoner/, "") ?? "Summoner spell unavailable";
          return <span class={matchStyles.assetIcon} role="img" title={label()} aria-label={label()}>
            <Show when={ddragonAsset(props.ddragon, "spell", spell())} keyed>
              {src => <img src={src} alt="" loading="lazy" decoding="async" onError={hideFailedImage} />}
            </Show>
          </span>;
        }}</For>
      </span>
    </span>
    <span class={`${matchStyles.kda} ${styles.matchKda}`}>
      <Show when={props.game.final_kda} fallback={<span class={styles.matchMeta}>KDA unavailable</span>}>
        {score => <><strong>{score()[0] ?? "—"} <i>/</i> {score()[1] ?? "—"} <i>/</i> {score()[2] ?? "—"}</strong><em>Final KDA</em></>}
      </Show>
    </span>
    <Show when={props.game.final_items} fallback={<span class={styles.matchMeta}>Final items unavailable</span>}>
      {items => <span class={matchStyles.items} role="group" aria-label="Final items">
        <For each={itemSlots}>{slot => <span class={matchStyles.itemSlot}>
          <Show when={ddragonAsset(props.ddragon, "item", items()[slot])} keyed>
            {src => <img src={src} alt={`Item ${items()[slot]}`} loading="lazy" decoding="async" onError={hideFailedImage} />}
          </Show>
        </span>}</For>
      </span>}
    </Show>
    <span class={styles.matchDate}>
      <Show when={playedAt()} fallback={<span>Date unavailable</span>}>
        {date => <time dateTime={date()} title={formatDate(date())}>{formatDate(date(), true)}</time>}
      </Show>
      <Show when={props.game.duration_seconds} fallback={<span class={styles.matchMeta}>Duration unavailable</span>}>
        {seconds => <span class={styles.matchMeta}>{formatMatchDuration(seconds() * 1_000)}</span>}
      </Show>
    </span>
  </div>;
}

function ClipTags(props: { tags: Pick<ClipGridCard, "custom_tags" | "predefined_tags">; class?: string; limit?: number; id?: string }) {
  const predefined = () => props.tags.predefined_tags;
  const custom = createMemo(() => {
    const names = new Set(predefined().map(kind => clipLabels[kind].toLowerCase()));
    return props.tags.custom_tags.filter(tag => !names.has(tag.toLowerCase()));
  });
  const tagCount = () => predefined().length + custom().length;
  const limit = () => props.limit ?? tagCount();
  const remaining = () => Math.max(0, tagCount() - limit());
  return <Show when={tagCount()}>
    <span id={props.id} class={props.class ? `${styles.clipTags} ${props.class}` : styles.clipTags}>
      <For each={predefined().slice(0, limit())}>{tag => <span class={styles.highlightBadge}>{clipLabels[tag]}</span>}</For>
      <For each={custom().slice(0, Math.max(0, limit() - predefined().length))}>
        {tag => <span class={styles.customBadge}>{tag}</span>}
      </For>
      <Show when={remaining()}>
        <span class={styles.customBadge} title={`${remaining()} more tags`}>+{remaining()}</span>
      </Show>
    </span>
  </Show>;
}

function ClipStrip(props: { children: JSX.Element }) {
  let viewport!: HTMLDivElement;
  let track!: HTMLDivElement;
  const [canScrollLeft, setCanScrollLeft] = createSignal(false);
  const [canScrollRight, setCanScrollRight] = createSignal(false);

  const updateArrows = () => {
    setCanScrollLeft(viewport.scrollLeft > 1);
    setCanScrollRight(viewport.clientWidth > 0 && viewport.scrollWidth - viewport.clientWidth - viewport.scrollLeft > 1);
  };

  onMount(() => {
    updateArrows();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(updateArrows);
    observer.observe(viewport);
    observer.observe(track);
    onCleanup(() => observer.disconnect());
  });

  const scrollTo = (left: number) => viewport.scrollTo({
    left,
    behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
  });

  const scroll = (direction: -1 | 1) => {
    if (direction > 0 ? !canScrollRight() : !canScrollLeft()) return;
    const [first, second] = track.children as HTMLCollectionOf<HTMLElement>;
    const stride = second ? second.offsetLeft - first.offsetLeft : viewport.clientWidth;
    if (stride <= 0) return;
    const tilesPerPage = Math.max(1, Math.floor(viewport.clientWidth / stride));
    const currentTile = Math.round(viewport.scrollLeft / stride);
    scrollTo((currentTile + direction * tilesPerPage) * stride);
  };

  return <div class={styles.carousel}>
    <button type="button" class={`${styles.stripArrow} ${styles.stripPrevious}`} aria-label="Scroll clips left"
      aria-disabled={!canScrollLeft()} tabIndex={canScrollLeft() ? 0 : -1} onClick={() => scroll(-1)}>
      <ArrowLeftIcon size={18} />
    </button>
    <button type="button" class={`${styles.stripArrow} ${styles.stripNext}`} aria-label="Scroll clips right"
      aria-disabled={!canScrollRight()} tabIndex={canScrollRight() ? 0 : -1} onClick={() => scroll(1)}>
      <ArrowRightIcon size={18} />
    </button>
    <div ref={viewport} class={styles.stripViewport} onScroll={updateArrows}>
      <div ref={track} class={styles.strip}>{props.children}</div>
    </div>
  </div>;
}

function ClipCards(props: { clips: ClipCard[]; onOpen: OpenClip }) {
  return <ClipStrip>
    <For each={props.clips}>{clip =>
      <article class={libraryStyles.clipCard}>
        <button class={libraryStyles.clipOpen} type="button"
          data-managed-clip={clip.header.id}
          onClick={event => props.onOpen(clip, event.currentTarget)} aria-label={`Play ${clip.title}`}>
          <span class={libraryStyles.clipArtwork}>
            <ClipThumbnail src={clip.thumbnail_url} />
            <em>{formatDuration(clipDurationMs(clip.header.duration))}</em>
            <i class={libraryStyles.playGlyph} aria-hidden="true"><PlayIcon size={14} /></i>
          </span>
          <span class={`${libraryStyles.clipContent} ${styles.cardContent}`}>
            <strong>{clip.title}</strong>
            <small>{clip.game_clock === undefined ? "Game time unavailable" : `Game ≈${recordedGameClock(clip.game_clock)}`}</small>
          </span>
          <ClipTags tags={clip.header} class={styles.cardTags} />
        </button>
      </article>
    }</For>
  </ClipStrip>;
}

/** One owner for a page read and its explicit continuations. Superseded results stay local. */
export function ClipGroups(props: {
  token: string; ready: boolean; query: ClipQuery; onOpen: OpenClip; inline: true;
}) {
  const inputs = createMemo(
    () => ({ token: props.token, query: props.query, ready: props.ready }),
    undefined,
    { equals: (previous, next) => previous.token === next.token && previous.query === next.query && previous.ready === next.ready },
  );
  const [groups, setGroups] = createSignal<ClipGroup[]>([]);
  const [next, setNext] = createSignal<GroupCursor | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [retry, setRetry] = createSignal(0);
  let generation = 0;
  let active = false;
  let pending: (() => void) | null = null;
  const schedule = (work: () => void) => { if (active) pending = work; else work(); };
  const load = (mode: "first" | "more" | "group", cursor?: GroupCursor, group?: { key: string; after: ClipCursor }) => {
    const id = generation;
    const { token, query: filters } = inputs();
    const query = { ...filters, after: cursor, group_key: group?.key, clip_after: group?.after };
    setBusy(true);
    setError(null);
    schedule(() => {
      if (id !== generation) return;
      active = true;
      void readClipPage(token, query).then(page => {
        if (id !== generation) return;
        if (mode === "group") {
          const added = page.groups[0];
          if (added) setGroups(old => old.map(item => item.key === added.key
            ? { ...item, clips: [...item.clips, ...added.clips], next: added.next } : item));
        } else {
          setGroups(old => mode === "more" ? [...old, ...page.groups] : page.groups);
          setNext(page.next);
        }
      }).catch(error => {
        if (id === generation) setError(libraryError(error));
      }).finally(() => {
        active = false;
        if (id === generation) setBusy(false);
        const queued = pending;
        pending = null;
        queued?.();
      });
    });
  };
  createEffect(() => {
    retry();
    const { ready } = inputs();
    generation++;
    setGroups([]);
    setNext(null);
    setBusy(false);
    setError(null);
    if (ready) load("first");
  });
  onCleanup(() => { generation++; pending = null; });
  return <div class={styles.inlinePanel} aria-busy={busy() || !props.ready}>
    <Show when={!props.ready || (busy() && groups().length === 0)}><p role="status">Loading clips…</p></Show>
    <Show when={error()}>{message => <p role="alert">{message()} <button class={controls.control} type="button" onClick={() => setRetry(n => n + 1)}>Try again</button></p>}</Show>
    <Show when={props.ready && !busy() && !error() && groups().length === 0}><p>No clips found.</p></Show>
    {/* Pagination updates groups in place so each strip can keep its scroll position. */}
    <Index each={groups()}>{group => {
      const moreClips = (after: ClipCursor) => load("group", undefined, { key: group().key, after });
      return <section class={styles.group}>
        <ClipCards clips={group().clips} onOpen={props.onOpen} />
        <Show when={group().next}>{cursor => <button class={controls.control} type="button" disabled={busy()}
          onClick={() => moreClips(cursor())}>More clips from this game</button>}</Show>
      </section>;
    }}</Index>
    <Show when={next()}>{cursor => <button class={controls.control} type="button" disabled={busy()}
      onClick={() => load("more", cursor())}>More games</button>}</Show>
  </div>;
}

type OpenGridClip = (clip: ClipGridCard, button: HTMLButtonElement) => void;

function ClipGrid(props: {
  model: ReturnType<typeof createClipGrid>; indexError: string | null; onRetryIndex: () => void; ddragon: DdragonStatus;
  onOpen: OpenGridClip; openingId?: string;
}) {
  let grid!: HTMLUListElement;
  const { state, current, append, retry } = props.model;
  const load = async (trigger: HTMLButtonElement, action: typeof append) => {
    const wasFocused = document.activeElement === trigger;
    const index = await action();
    if (index !== undefined && wasFocused && (document.activeElement === trigger ||
        (!trigger.isConnected && document.activeElement === document.body))) {
      grid.children.item(index)?.querySelector("button")?.focus();
    }
  };

  return <div class={styles.gridRegion}>
    <Show when={!props.indexError && !state.failure && state.clips.length === 0}>
      <Show when={current()} fallback={<p role="status">Loading clips…</p>}><p>No clips found.</p></Show>
    </Show>
    <ul ref={grid} class={styles.grid} aria-label="Clips" aria-busy={state.busy}>
      <For each={state.clips}>{clip => {
        const id = `grid-clip-${clip.id}`;
        const tagsId = () => clip.predefined_tags.length || clip.custom_tags.length ? `${id}-tags` : undefined;
        return <li>
          <button class={`${libraryStyles.clipOpen} ${styles.gridOpen}`} type="button"
            data-managed-clip={clip.id} aria-label={`Play ${clip.title}`}
            aria-describedby={[`${id}-champion`, `${id}-duration`, tagsId()].filter(Boolean).join(" ")}
            aria-busy={props.openingId === clip.id}
            aria-disabled={!current()}
            onClick={event => { if (current()) props.onOpen(clip, event.currentTarget); }}>
            <span class={`${libraryStyles.clipArtwork} ${styles.gridArtwork}`}>
              <ClipThumbnail src={clip.thumbnail_url} />
              <em id={`${id}-duration`}>{formatDuration(clipDurationMs(clip.duration))}</em>
            </span>
            <span class={styles.gridMetadata}>
              <ClipChampionPortrait id={`${id}-champion`} champion={clip.champion}
                ddragon={props.ddragon} class={styles.gridChampion} />
              <span class={styles.gridText}>
                <strong class={styles.gridTitle} title={clip.title}>{clip.title}</strong>
                <ClipTags id={tagsId()} tags={clip} limit={2} />
              </span>
            </span>
          </button>
        </li>;
      }}</For>
    </ul>
    <Show when={props.indexError}>{error =>
      <p role="alert">{error()} <button class={controls.control} type="button" onClick={props.onRetryIndex}>Try again</button></p>
    }</Show>
    <Show when={state.failure}>{failed =>
      <p role="alert">{failed().message} <button class={controls.control} type="button" aria-disabled={state.busy}
        onClick={event => void load(event.currentTarget, retry)}>Try again</button></p>
    }</Show>
    <Show when={!state.failure && state.next}>
      <button class={`${controls.control} ${styles.gridMore}`} type="button" aria-disabled={!current() || state.busy}
        onClick={event => void load(event.currentTarget, append)}>
        {state.busy && current() ? "Loading clips…" : "More clips"}
      </button>
    </Show>
  </div>;
}

export function ManagedClips(props: {
  root: string | null; token: string; ready: boolean; indexError: string | null; onRetry: () => void;
  ddragon: DdragonStatus;
  filters: ClipFilters; onFilters: (query: ClipFilters) => void;
  onOpen: OpenGridClip; openingId?: string; onOpenFolder: () => void;
}) {
  const [query, setQuery] = createSignal(props.filters);
  const grid = createClipGrid(() => ({ root: props.root, token: props.token, ready: props.ready, query: query() }));
  createEffect(() => {
    const next = { ...props.filters, search: props.filters.search?.trim() || undefined };
    const timer = window.setTimeout(() => setQuery(previous =>
      JSON.stringify(previous) === JSON.stringify(next) ? previous : next), 180);
    onCleanup(() => window.clearTimeout(timer));
  });
  return <section class={styles.panel} aria-label="Clips library">
    <ClipsFilters library={props.root} filters={props.filters} onChange={props.onFilters} token={props.ready ? props.token : undefined}
      championPortrait={name => ddragonAsset(props.ddragon, "champion", name)} onOpenFolder={props.onOpenFolder} />
    <ClipGrid model={grid} indexError={props.indexError} onRetryIndex={props.onRetry}
      onOpen={props.onOpen} openingId={props.openingId} ddragon={props.ddragon} />
  </section>;
}

export function ManagedClipPlayer(props: {
  clip: ClipCard; token: string; onClose: () => void; onEdit: (edit: ClipEdit) => Promise<boolean>;
  sourceAvailable?: boolean; onSource?: () => void; opener?: HTMLElement;
  library?: { fileSizeBytes: number; ddragon: DdragonStatus };
}) {
  let dialog!: HTMLElement;
  let deleteButton!: HTMLButtonElement;
  let closeButton!: HTMLButtonElement;
  const [editing, setEditing] = createSignal<"title" | "tags" | null>(null);
  const [deleting, setDeleting] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [title, setTitle] = createSignal(props.clip.title);
  const [tags, setTags] = createSignal(props.clip.header.custom_tags);
  const [predefinedTags, setPredefinedTags] = createSignal(props.clip.header.predefined_tags);
  const [detail, setDetail] = createSignal<Awaited<ReturnType<typeof readClipDetail>>>();
  const [detailError, setDetailError] = createSignal(false);
  const [reading, setReading] = createSignal(false);
  let alive = true;
  const loadEvents = () => {
    if (detail() || reading()) return;
    setReading(true);
    setDetailError(false);
    void readClipDetail(props.token, props.clip.header.id).then(value => {
      if (alive) setDetail(value);
    }, () => { if (alive) setDetailError(true); }).finally(() => { if (alive) setReading(false); });
  };
  onCleanup(() => { alive = false; });
  createModalFocus({
    dialog: () => dialog,
    initial: () => closeButton,
    opener: () => props.opener,
    onEscape: () => {
      if (busy()) return;
      if (editing()) setEditing(null); else props.onClose();
    },
  });
  const submit = async (edit: ClipEdit) => {
    if (busy()) return;
    setBusy(true);
    try {
      if (await props.onEdit(edit)) { setEditing(null); setDeleting(false); }
    } finally { setBusy(false); }
  };
  const caption = createMemo(() => gameCaption(props.clip.header.game));
  const duration = () => formatDuration(clipDurationMs(props.clip.header.duration));
  const close = () => <button ref={closeButton} class={controls.control} type="button"
    onClick={props.onClose} disabled={busy()}>Close</button>;
  return <>
    <div classList={{ [libraryStyles.clipModalBackdrop]: true, [styles.libraryBackdrop]: !!props.library }}
      role="presentation" onMouseDown={() => !busy() && props.onClose()}>
      <section ref={dialog} tabIndex={-1} classList={{
        [libraryStyles.clipModal]: true, [styles.player]: true, [styles.libraryPlayer]: !!props.library,
      }} role="dialog" aria-modal="true"
        aria-labelledby="managed-clip-title" onMouseDown={event => event.stopPropagation()}>
        <Show when={props.library} fallback={<header>
          <div><h2 id="managed-clip-title">{props.clip.title}</h2>
            <span>{[props.clip.header.game.champion, caption(), duration()].filter(Boolean).join(" · ")}</span></div>
          {close()}
        </header>}>
          {library => <div class={styles.playerTop}>
            <ClipMatchHeader game={props.clip.header.game} ddragon={library().ddragon} />
            {close()}
          </div>}
        </Show>
        <ClipVideoPlayer src={props.clip.video_url} compact={!props.library} />
        <div class={styles.details}>
          <Show when={props.library}>{library => <div class={styles.playerClipInfo}>
            <h2 id="managed-clip-title">{props.clip.title}</h2>
            <p><span>{duration()}</span><span aria-hidden="true"> · </span><span>{formatBytes(library().fileSizeBytes)}</span></p>
          </div>}</Show>
          <ClipTags tags={props.clip.header} />
          <div class={styles.actions}>
            <Show when={props.onSource}><button class={controls.control} type="button" disabled={!props.sourceAvailable || busy()}
              onClick={props.onSource}>{props.sourceAvailable ? "Open source replay" : "Source replay unavailable"}</button></Show>
            <button class={controls.control} type="button" disabled={busy()} onClick={() => setEditing("title")}>Rename</button>
            <button class={controls.control} type="button" disabled={busy()} onClick={() => setEditing("tags")}>Edit tags</button>
            <button class={controls.control} type="button" disabled={busy()} onClick={() => void submit({ kind: "reveal" })}>Show in folder</button>
            <button ref={deleteButton} class={controls.control} type="button" disabled={busy()} onClick={() => setDeleting(true)}>Delete clip</button>
          </div>
          <Show when={editing() === "title"}>
            <form class={styles.actions} onSubmit={event => {
              event.preventDefault();
              if (!clipTitleError(title().trim())) void submit({ kind: "rename", title: title().trim() });
            }}>
              <label class={styles.title}>Clip title<input value={title()} maxLength={120} required
                onInput={event => setTitle(event.currentTarget.value)} /></label>
              <Show when={title().trim() && clipTitleError(title().trim())}>{error => <small role="alert">{error()}</small>}</Show>
              <button class={controls.control} type="submit" disabled={busy() || !!clipTitleError(title().trim())}>Save title</button>
              <button class={controls.control} type="button" onClick={() => setEditing(null)}>Cancel</button>
            </form>
          </Show>
          <Show when={editing() === "tags"}>
            <ClipTagEditor token={props.token} customTags={tags()} predefinedTags={predefinedTags()}
              onCustomTagsChange={setTags} onPredefinedTagsChange={setPredefinedTags} disabled={busy()} />
            <div class={styles.actions}>
              <button class={controls.control} type="button" disabled={busy()}
                onClick={() => void submit({ kind: "tags", custom_tags: tags(), predefined_tags: predefinedTags() })}>Save tags</button>
              <button class={controls.control} type="button" onClick={() => setEditing(null)}>Cancel</button>
            </div>
          </Show>
          <details onToggle={event => { if (event.currentTarget.open) loadEvents(); }}>
            <summary>Events in this clip</summary>
            <Show when={reading()}><p role="status">Loading events…</p></Show>
            <Show when={detailError() || detail()?.events.availability === "unavailable"}><p>Event data is unavailable.</p></Show>
            <Show when={detail()?.events.availability === "partial"}><p>Some event data is unavailable.</p></Show>
            <Show when={detail()?.events.events.length}>
              <ul class={styles.eventList}>
                <For each={detail()?.events.events}>{event => <li>
                  {formatDuration(Number(parseReplayTick(event.replay_tick)) / 48_000)} · {event.event.name?.replace(/([a-z])([A-Z])/g, "$1 $2")}
                  {event.actor ? ` · ${event.actor}` : ""}
                </li>}</For>
              </ul>
            </Show>
            <Show when={detail()?.events.availability === "available" && !detail()?.events.events.length}><p>No recorded events in this clip.</p></Show>
          </details>
        </div>
      </section>
    </div>
    <Show when={deleting()}><ConfirmDialog title="Delete this clip?" message={`“${props.clip.title}” and its clip details will be removed from this computer.`}
      confirmLabel="Delete clip" busy={busy()} opener={deleteButton} onCancel={() => { if (!busy()) setDeleting(false); }}
      onConfirm={() => void submit({ kind: "delete" })} /></Show>
  </>;
}
