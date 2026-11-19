import { For, Index, Show, createEffect, createMemo, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { ddragonAsset, hideFailedImage } from "../ddragonAssets";
import { formatDate, formatDuration, formatMatchDuration } from "../format";
import { libraryError } from "../libraryController";
import { actorLabels, clipDurationMs, clipTitleError, highlightLabel, readClipDetail, readClipPage, type Attribution, type ClipCard,
  type ClipCursor, type ClipEdit, type ClipGame, type ClipGroup, type ClipMatchFacts, type ClipPage, type ClipQuery, type GroupCursor } from "../managedClips";
import { matchQueueLabel } from "../matchQueues";
import { parseReplayTick } from "../replayTime";
import { recordedGameClock } from "../replayProjection";
import type { DdragonStatus } from "../types";
import { ArrowLeftIcon, ArrowRightIcon, PlayIcon } from "../ui/icons";
import controls from "../ui/controls.module.css";
import { createModalFocus } from "../ui/createModalFocus";
import { ClipHighlights, ClipTagInput } from "./ClipMetadataFields";
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
  game.final_kda?.join("/"),
].filter(Boolean).join(" · ");

type OpenClip = (clip: ClipCard, button: HTMLElement) => void;

const spellSlots = [0, 1] as const;
const itemSlots = [0, 1, 2, 3, 4, 5, 6] as const;

function ClipMatchHeader(props: { game: ClipGame; facts?: ClipMatchFacts; ddragon: DdragonStatus }) {
  const outcome = () => props.facts?.outcome ?? props.game.outcome;
  const result = () => outcome() === "win" ? "Victory" : outcome() === "loss" ? "Defeat" : "Unknown result";
  const queue = () => matchQueueLabel(props.game.queue_id, props.game.mode) ?? "Queue unavailable";
  const champion = () => {
    const name = props.facts?.champion === "Unknown" ? props.game.champion : props.facts?.champion ?? props.game.champion;
    return name === "Unknown" ? null : name;
  };
  const championName = () => champion() ?? "Champion unavailable";
  const playedAt = () => props.game.played_at ?? props.game.part_recorded_at;
  const duration = () => props.facts?.duration_seconds;
  const kda = () => props.facts?.final_kda ?? props.game.final_kda;

  return <header class={styles.matchRow} data-outcome={outcome() ?? undefined}>
    <div class={styles.matchContext}>
      <h2 class={styles.matchResult} aria-label={`${championName()} · ${result()}`}>{result()}</h2>
      <span class={`${styles.matchMeta} ${styles.matchQueue}`} title={queue()}>{queue()}</span>
    </div>
    <span class={matchStyles.championLoadout}>
      <span class={`${matchStyles.championSlot} ${styles.matchChampion}`} role="img" aria-label={championName()} title={championName()}>
        <span aria-hidden="true">?</span>
        <Show when={ddragonAsset(props.ddragon, "champion", champion())} keyed>
          {src => <img src={src} alt="" loading="lazy" decoding="async" onError={hideFailedImage} />}
        </Show>
      </span>
      <span class={`${matchStyles.spells} ${styles.matchSpells}`}>
        <For each={spellSlots}>{slot => {
          const spell = () => props.facts?.summoner_spells?.[slot];
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
      <Show when={kda()} fallback={<span class={styles.matchMeta}>KDA unavailable</span>}>
        {score => <><strong>{score()[0] ?? "—"} <i>/</i> {score()[1] ?? "—"} <i>/</i> {score()[2] ?? "—"}</strong><em>Final KDA</em></>}
      </Show>
    </span>
    <Show when={props.facts?.final_items} fallback={<span class={styles.matchMeta}>Final items unavailable</span>}>
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
      <Show when={duration()} fallback={<span class={styles.matchMeta}>Duration unavailable</span>}>
        {seconds => <span class={styles.matchMeta}>{formatMatchDuration(seconds() * 1_000)}</span>}
      </Show>
    </span>
  </header>;
}

function ClipTags(props: { header: ClipCard["header"]; class?: string }) {
  return <Show when={props.header.highlights.length || props.header.custom_tags.length}>
    <span class={props.class ? `${styles.clipTags} ${props.class}` : styles.clipTags}>
      <For each={props.header.highlights}>{tag => <span class={styles.highlightBadge}>{highlightLabel(tag)}</span>}</For>
      <For each={props.header.custom_tags}>{tag => <span class={styles.customBadge}>{tag}</span>}</For>
    </span>
  </Show>;
}

function ClipStrip(props: { children: JSX.Element; clipCount: number; onMore?: () => void; busy?: boolean }) {
  let viewport!: HTMLDivElement;
  let track!: HTMLDivElement;
  let appendFrom: number | undefined;
  const [canScrollLeft, setCanScrollLeft] = createSignal(false);
  const [canScrollRight, setCanScrollRight] = createSignal(false);
  const canGoRight = () => canScrollRight() || (props.onMore !== undefined && !props.busy);

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

  createEffect(() => {
    const count = props.clipCount;
    const busy = props.busy;
    if (appendFrom === undefined) return;
    if (count > appendFrom) {
      const [first, next] = [track.children[0], track.children[appendFrom]] as HTMLElement[];
      appendFrom = undefined;
      updateArrows();
      scrollTo(next.offsetLeft - first.offsetLeft);
    } else if (!busy) {
      appendFrom = undefined;
    }
  });

  const scroll = (direction: -1 | 1) => {
    if (direction > 0 && !canScrollRight()) {
      if (props.onMore && !props.busy) {
        appendFrom = props.clipCount;
        props.onMore();
      }
      return;
    }
    if (direction < 0 && !canScrollLeft()) return;
    if (direction < 0) appendFrom = undefined;
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
      aria-disabled={!canGoRight()} tabIndex={canGoRight() ? 0 : -1} onClick={() => scroll(1)}>
      <ArrowRightIcon size={18} />
    </button>
    <div ref={viewport} class={styles.stripViewport} onScroll={updateArrows}>
      <div ref={track} class={styles.strip}>{props.children}</div>
    </div>
  </div>;
}

function ClipCards(props: { clips: ClipCard[]; onOpen: OpenClip; inline?: boolean; onMore?: () => void; busy?: boolean }) {
  return <ClipStrip clipCount={props.clips.length} onMore={props.onMore} busy={props.busy}>
    <For each={props.clips}>{clip =>
      <article class={libraryStyles.clipCard}>
        <button class={libraryStyles.clipOpen} type="button"
          data-managed-clip={clip.header.id}
          onClick={event => props.onOpen(clip, event.currentTarget)} aria-label={`Play ${clip.title}`}>
          <span class={libraryStyles.clipArtwork}>
            <ClipThumbnail src={clip.thumbnail_url} />
            <em>{formatDuration(clipDurationMs(clip))}</em>
            <i class={libraryStyles.playGlyph} aria-hidden="true"><PlayIcon size={14} /></i>
          </span>
          <span class={`${libraryStyles.clipContent} ${styles.cardContent}`}>
            <strong>{clip.title}</strong>
            <Show when={props.inline} fallback={<small>{clip.header.preset}</small>}>
              <small>{clip.game_clock === undefined ? "Game time unavailable" : `Game ≈${recordedGameClock(clip.game_clock)}`}</small>
            </Show>
          </span>
          <ClipTags header={clip.header} class={styles.cardTags} />
        </button>
      </article>
    }</For>
  </ClipStrip>;
}

/** One owner for a page read and its explicit continuations. Superseded results stay local. */
export function ClipGroups(props: {
  token: string; ready: boolean; query: ClipQuery; onOpen: OpenClip;
} & ({ inline: true; ddragon?: never } | { inline?: false; ddragon: DdragonStatus })) {
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
      void readClipPage(token, query).then((page: ClipPage) => {
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
  return <div class={props.inline ? styles.inlinePanel : styles.panel} aria-busy={busy() || !props.ready}>
    <Show when={!props.ready || (busy() && groups().length === 0)}><p role="status">Loading clips…</p></Show>
    <Show when={error()}>{message => <p role="alert">{message()} <button class={controls.control} type="button" onClick={() => setRetry(n => n + 1)}>Try again</button></p>}</Show>
    <Show when={props.ready && !busy() && !error() && groups().length === 0}><p>No clips found.</p></Show>
    {/* Pagination updates groups in place so each strip can keep its scroll position. */}
    <Index each={groups()}>{group => {
      const game = () => group().clips[0]?.header.game;
      const moreClips = (after: ClipCursor) => load("group", undefined, { key: group().key, after });
      const continueStrip = () => {
        const cursor = group().next;
        return !props.inline && cursor ? () => moreClips(cursor) : undefined;
      };
      return <section class={styles.group}>
        <Show when={!props.inline && game()}>{value =>
          <ClipMatchHeader game={value()} facts={group().match_facts} ddragon={props.ddragon!} />
        }</Show>
        <ClipCards clips={group().clips} onOpen={props.onOpen} inline={props.inline} busy={busy()}
          onMore={continueStrip()} />
        <Show when={props.inline && group().next}>{cursor => <button class={controls.control} type="button" disabled={busy()}
          onClick={() => moreClips(cursor())}>More clips from this game</button>}</Show>
      </section>;
    }}</Index>
    <Show when={next()}>{cursor => <button class={controls.control} type="button" disabled={busy()}
      onClick={() => load("more", cursor())}>More games</button>}</Show>
  </div>;
}

const filterOptions = [
  ["2", "Double kill or higher"], ["3", "Triple kill or higher"], ["4", "Quadra kill or higher"],
  ["5", "Pentakill"], ["first blood", "First Blood"], ["dragon steal", "Dragon steal"],
  ["baron steal", "Baron steal"], ["herald steal", "Herald steal"],
] as const;

export function ManagedClips(props: {
  token: string; ready: boolean; indexError: string | null; onRetry: () => void;
  ddragon: DdragonStatus;
  filters: ClipQuery; onFilters: (query: ClipQuery) => void; onOpen: OpenClip; onOpenFolder: () => void;
}) {
  const [query, setQuery] = createSignal(props.filters);
  createEffect(() => {
    const next = props.filters;
    const timer = window.setTimeout(() => setQuery(next), 180);
    onCleanup(() => window.clearTimeout(timer));
  });
  const setFilter = <K extends keyof ClipQuery>(key: K, value: ClipQuery[K]) =>
    props.onFilters({ ...props.filters, [key]: value || undefined });
  return <section class={styles.panel} aria-label="Clips library">
    <div class={styles.search}>
      <label>Search clips<input type="search" maxLength={80} value={props.filters.search ?? ""}
        onInput={event => setFilter("search", event.currentTarget.value)} placeholder="Title, tag, champion or highlight" /></label>
      <button class={controls.control} type="button" onClick={props.onOpenFolder}>Open clips folder</button>
    </div>
    <div class={styles.filters}>
      <label>Champion<input maxLength={50} value={props.filters.champion ?? ""}
        onInput={event => setFilter("champion", event.currentTarget.value.trim())} /></label>
      <label>Queue or mode<input maxLength={50} value={props.filters.queue ?? ""}
        onInput={event => setFilter("queue", event.currentTarget.value)} /></label>
      <label>Played from<input type="date" value={props.filters.from ?? ""}
        onInput={event => setFilter("from", event.currentTarget.value)} /></label>
      <label>Played through<input type="date" value={props.filters.to ?? ""}
        onInput={event => setFilter("to", event.currentTarget.value)} /></label>
      <label>Result<select value={props.filters.outcome ?? ""} onChange={event => setFilter("outcome", event.currentTarget.value)}>
        <option value="">Any result</option><option value="win">Win</option><option value="loss">Loss</option>
      </select></label>
      <label>Highlight<select value={props.filters.highlight ?? ""} onChange={event => setFilter("highlight", event.currentTarget.value)}>
        <option value="">Any highlight</option><For each={filterOptions}>{([id, label]) => <option value={id}>{label}</option>}</For>
      </select></label>
      <label>Actor<select value={props.filters.actor ?? ""} onChange={event => setFilter("actor", event.currentTarget.value as Attribution)}>
        <option value="">Any actor</option><For each={Object.entries(actorLabels)}>{([id, label]) => <option value={id}>{label}</option>}</For>
      </select></label>
      <Show when={Object.values(props.filters).some(Boolean)}><button class={controls.control} type="button" onClick={() => props.onFilters({})}>Clear filters</button></Show>
    </div>
    <Show when={props.indexError} fallback={<ClipGroups token={props.token} ready={props.ready} query={query()}
      onOpen={props.onOpen} ddragon={props.ddragon} />}>
      {error => <p role="alert">{error()} <button class={controls.control} type="button" onClick={props.onRetry}>Try again</button></p>}
    </Show>
  </section>;
}

export function ManagedClipPlayer(props: {
  clip: ClipCard; token: string; onClose: () => void; onEdit: (edit: ClipEdit) => Promise<boolean>;
  sourceAvailable?: boolean; onSource?: () => void; opener?: HTMLElement;
}) {
  let dialog!: HTMLElement;
  let deleteButton!: HTMLButtonElement;
  let closeButton!: HTMLButtonElement;
  const [editing, setEditing] = createSignal<"title" | "tags" | null>(null);
  const [deleting, setDeleting] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [title, setTitle] = createSignal(props.clip.title);
  const [tags, setTags] = createSignal(props.clip.header.custom_tags);
  const [selected, setSelected] = createSignal(props.clip.header.highlights.map(tag => tag.event_index));
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
  return <>
    <div class={libraryStyles.clipModalBackdrop} role="presentation" onMouseDown={() => !busy() && props.onClose()}>
      <section ref={dialog} tabIndex={-1} class={`${libraryStyles.clipModal} ${styles.player}`} role="dialog" aria-modal="true"
        aria-labelledby="managed-clip-title" onMouseDown={event => event.stopPropagation()}>
        <header>
          <div><h2 id="managed-clip-title">{props.clip.title}</h2>
            <span>{[props.clip.header.game.champion, caption(), formatDuration(clipDurationMs(props.clip))].filter(Boolean).join(" · ")}</span></div>
          <button ref={closeButton} class={controls.control} type="button" onClick={props.onClose} disabled={busy()}>Close</button>
        </header>
        <ClipVideoPlayer src={props.clip.video_url} compact />
        <div class={styles.details}>
          <ClipTags header={props.clip.header} />
          <div class={styles.actions}>
            <Show when={props.onSource}><button class={controls.control} type="button" disabled={!props.sourceAvailable || busy()}
              onClick={props.onSource}>{props.sourceAvailable ? "Open source replay" : "Source replay unavailable"}</button></Show>
            <button class={controls.control} type="button" disabled={busy()} onClick={() => setEditing("title")}>Rename</button>
            <button class={controls.control} type="button" disabled={busy()} onClick={() => { setEditing("tags"); loadEvents(); }}>Edit tags</button>
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
            <ClipTagInput token={props.token} tags={tags()} onChange={setTags} disabled={busy()} />
            <ClipHighlights candidates={detail()?.candidates ?? []} availability={detail()?.events.availability ?? props.clip.header.events}
              selected={selected()} onChange={setSelected} disabled={busy() || reading()} />
            <div class={styles.actions}>
              <button class={controls.control} type="button" disabled={busy() || reading()}
                onClick={() => void submit({ kind: "tags", custom_tags: tags(), highlights: selected() })}>Save tags</button>
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
