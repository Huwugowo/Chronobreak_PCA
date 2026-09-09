import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import type {
  ClipRange,
  KdaTimelinePoint,
  ReplayParticipant,
  ViewerEvent,
} from "../types";
import {
  clamp,
  eventCategory,
  eventSummary,
  eventTitle,
  nearestIndexAt,
  replayTickAtFrameBoundary,
} from "../viewerUtils";
import {
  REPLAY_TICKS_PER_SECOND,
  type MediaTimelineV2,
  type ReplayTick,
} from "../replayTime";
import ChampionFilter from "./ChampionFilter";
import PlaybackControls, { type PlaybackControlsProps } from "./PlaybackControls";
import styles from "./FullscreenOverlay.module.css";

type Props = {
  champion: string;
  localPlayerName: string | null;
  events: readonly ViewerEvent[];
  mediaTimeline: MediaTimelineV2;
  replayTick: ReplayTick;
  presentedTick: ReplayTick | null;
  gameTick: string;
  beforeGameStart: boolean;
  currentKda: KdaTimelinePoint | undefined;
  isPlaying: boolean;
  playback: PlaybackControlsProps["state"];
  onRate: PlaybackControlsProps["onRate"];
  onMuted: PlaybackControlsProps["onMuted"];
  onVolume: PlaybackControlsProps["onVolume"];
  mediaAvailable: boolean;
  participants: readonly ReplayParticipant[];
  selectedPlayers: readonly string[];
  clipRange: ClipRange | null;
  onPlayerToggle: (summonerName: string) => void;
  onPlayerClear: () => void;
  onTogglePlayback: () => void;
  onSeek: (replayTick: ReplayTick) => void;
  onActivateClip: () => void;
  onEventSelect: (event: ViewerEvent) => void;
  onClipEndpointEditStart: (endpoint: "start" | "end") => void;
  onClipEndpointPreview: (endpoint: "start" | "end", requestedTick: ReplayTick) => void;
  onClipEndpointEditFinish: () => void;
  onClipEndpointKeyDown: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointKeyUp: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointBlur: (endpoint: "start" | "end") => void;
  onExportClip: () => void;
  onCancelClip: () => void;
  onExit: () => void;
};

function FullscreenOverlay(props: Props) {
  let fullscreenRail!: HTMLDivElement;
  let idleTimer: number | undefined;
  let dismissTimer: number | undefined;
  let unmountTimer: number | undefined;
  let lastTriggeredEvent: ViewerEvent | undefined;

  const [topBarVisible, setTopBarVisible] = createSignal(true);
  const [scrubberActive, setScrubberActive] = createSignal(false);
  const [displayedEvent, setDisplayedEvent] = createSignal<ViewerEvent | null>(null);
  const [eventCardVisible, setEventCardVisible] = createSignal(false);

  const replayEnd = createMemo(() => props.mediaTimeline.video.replayEnd);
  const replayTickToMilliseconds = (tick: ReplayTick): number =>
    Math.floor((tick * 1_000) / REPLAY_TICKS_PER_SECOND);
  const gameTickToMilliseconds = (tick: string): number => Number(BigInt(tick) / 1_000n);
  const frameBoundaryTick = (frame: ClipRange["startFrame"]): ReplayTick =>
    replayTickAtFrameBoundary(frame, props.mediaTimeline);
  const mappedEvents = createMemo(() =>
    props.events.filter(
      (event): event is ViewerEvent & { replay_tick: ReplayTick } => event.replay_tick !== undefined,
    ),
  );
  const progress = createMemo(() => clamp((props.replayTick / replayEnd()) * 100, 0, 100));
  const replaySecond = createMemo(() => Math.floor(replayTickToMilliseconds(props.replayTick) / 1_000));
  const markerPositions = createMemo(() =>
    mappedEvents().map((event, index) => ({
      event,
      index,
      position: clamp((event.replay_tick / replayEnd()) * 100, 0, 100),
    })),
  );
  const clipStartPosition = createMemo(() =>
    props.clipRange
      ? clamp((frameBoundaryTick(props.clipRange.startFrame) / replayEnd()) * 100, 0, 100)
      : 0,
  );
  const clipEndPosition = createMemo(() =>
    props.clipRange
      ? clamp((frameBoundaryTick(props.clipRange.endFrameExclusive) / replayEnd()) * 100, 0, 100)
      : 100,
  );
  const minuteTicks = Array.from(
    { length: Math.floor(replayTickToMilliseconds(replayEnd()) / 60_000) + 1 },
    (_, index) => ({
      minute: index,
      position: clamp(((index * 60 * REPLAY_TICKS_PER_SECOND) / replayEnd()) * 100, 0, 100),
      major: index % 4 === 0,
    }),
  );
  const timelineLabels = [0, 8, 16, 24]
    .map((minute) => minute * 60 * REPLAY_TICKS_PER_SECOND)
    .concat(replayEnd())
    .filter((value, index, values) => value <= replayEnd() && values.indexOf(value) === index)
    .map((value) => ({
      value,
      position: clamp((value / replayEnd()) * 100, 0, 100),
    }));

  const nearestEventIndex = createMemo(() => props.presentedTick === null ? -1 : nearestIndexAt(mappedEvents(), props.presentedTick));
  const proximityEventIndex = createMemo(() => {
    const index = nearestEventIndex();
    return index >= 0 && Math.abs(mappedEvents()[index].replay_tick - (props.presentedTick ?? 0)) <= REPLAY_TICKS_PER_SECOND
      ? index
      : -1;
  });
  const noteActivity = () => {
    setTopBarVisible(true);
    if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    idleTimer = window.setTimeout(() => setTopBarVisible(false), 3_200);
  };

  const dismissEventCard = () => {
    setEventCardVisible(false);
    if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    unmountTimer = window.setTimeout(() => setDisplayedEvent(null), 220);
  };

  const showEventCard = (event: ViewerEvent) => {
    if (dismissTimer !== undefined) window.clearTimeout(dismissTimer);
    if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    setDisplayedEvent(event);
    setEventCardVisible(false);
    requestAnimationFrame(() => requestAnimationFrame(() => setEventCardVisible(true)));
    dismissTimer = window.setTimeout(dismissEventCard, 3_500);
  };

  createEffect(() => {
    const index = proximityEventIndex();
    if (index < 0) {
      lastTriggeredEvent = undefined;
      return;
    }
    const event = mappedEvents()[index];
    if (event === lastTriggeredEvent) return;
    lastTriggeredEvent = event;
    showEventCard(event);
  });

  onMount(() => {
    noteActivity();
    window.addEventListener("mousemove", noteActivity, { passive: true });
    onCleanup(() => {
      window.removeEventListener("mousemove", noteActivity);
      if (idleTimer !== undefined) window.clearTimeout(idleTimer);
      if (dismissTimer !== undefined) window.clearTimeout(dismissTimer);
      if (unmountTimer !== undefined) window.clearTimeout(unmountTimer);
    });
  });

  const seekFromRail = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (bounds.width <= 0) return;
    props.onSeek(
      Math.round(clamp((event.clientX - bounds.left) / bounds.width, 0, 1) * replayEnd()) as ReplayTick,
    );
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: ReplayTick | undefined;
    if (event.key === "PageDown") target = (props.replayTick - 30 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    if (event.key === "PageUp") target = (props.replayTick + 30 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    if (event.key === "Home") target = 0 as ReplayTick;
    if (event.key === "End") target = replayEnd();
    if (target === undefined) return;
    event.preventDefault();
    props.onSeek(clamp(target, 0, replayEnd()) as ReplayTick);
  };

  const beginClipDrag = (
    event: PointerEvent & { currentTarget: HTMLButtonElement },
    endpoint: "start" | "end",
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    handle.focus({ preventScroll: true });
    props.onClipEndpointEditStart(endpoint);
    const update = (pointerEvent: PointerEvent) => {
      const bounds = fullscreenRail.getBoundingClientRect();
      if (bounds.width <= 0) return;
      props.onClipEndpointPreview(
        endpoint,
        Math.round(clamp((pointerEvent.clientX - bounds.left) / bounds.width, 0, 1) * replayEnd()) as ReplayTick,
      );
    };
    const cleanup = (pointerEvent: PointerEvent) => {
      handle.removeEventListener("pointermove", update);
      handle.removeEventListener("pointerup", finish);
      handle.removeEventListener("pointercancel", cancel);
      if (handle.hasPointerCapture(pointerEvent.pointerId)) {
        handle.releasePointerCapture(pointerEvent.pointerId);
      }
    };
    const finish = (pointerEvent: PointerEvent) => {
      update(pointerEvent);
      cleanup(pointerEvent);
      props.onClipEndpointEditFinish();
    };
    const cancel = (pointerEvent: PointerEvent) => {
      cleanup(pointerEvent);
      props.onClipEndpointEditFinish();
    };
    handle.setPointerCapture(event.pointerId);
    handle.addEventListener("pointermove", update);
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", cancel);
  };

  return (
    <div class={styles.overlayRoot} data-testid="fullscreen-overlay">
      <header
        classList={{
          [styles.topBar]: true,
          [styles.topBarHidden]: !topBarVisible(),
        }}
        data-testid="fullscreen-topbar"
      >
        <div class={styles.topIdentity}>
          <span class={styles.replayMark}>REPLAY</span>
          <span class={styles.topDivider} />
          <div>
            <small>{props.localPlayerName ?? "LOCAL PLAYER"}</small>
            <strong>{props.champion}</strong>
          </div>
          <span class={styles.topKda}>
            {props.currentKda?.kills ?? 0} / {props.currentKda?.deaths ?? 0} / {props.currentKda?.assists ?? 0}
          </span>
        </div>
        <div class={styles.topStatus}>
          <strong>{props.beforeGameStart ? "--:--" : formatDuration(gameTickToMilliseconds(props.gameTick))}</strong>
          <span class={styles.recStatus}><i aria-hidden="true" /> REC</span>
          <button type="button" onClick={props.onExit} aria-label="Exit fullscreen replay">EXIT <span aria-hidden="true">×</span></button>
        </div>
      </header>

      <Show when={displayedEvent()}>
        {(event) => (
          <article
            classList={{
              [styles.eventCard]: true,
              [styles.eventCardVisible]: eventCardVisible(),
              [styles.eventCardTopHidden]: !topBarVisible(),
              [styles.eventAlly]: event().relation === "ally",
              [styles.eventEnemy]: event().relation === "enemy",
              [styles.eventNeutral]: event().relation === "neutral",
            }}
            data-testid="fullscreen-event-card"
          >
            <span>{eventCategory(event())}</span>
            <strong>{eventTitle(event())}</strong>
            <small>{eventSummary(event())}</small>
          </article>
        )}
      </Show>

      <section
        classList={{ [styles.scrubberZone]: true, [styles.scrubberZoneActive]: scrubberActive() }}
        onPointerEnter={() => setScrubberActive(true)}
        onPointerLeave={() => setScrubberActive(false)}
        onFocusIn={noteActivity}
        onKeyDown={noteActivity}
        aria-label="Fullscreen replay controls"
        data-testid="fullscreen-scrubber-zone"
      >
        <div class={styles.fullscreenTimeline}>
          <div class={styles.minuteTicks} aria-hidden="true">
            <For each={minuteTicks}>{(tick) => <i classList={{ [styles.majorTick]: tick.major }} style={`left:${tick.position}%`} />}</For>
          </div>
          <div class={styles.timelineLabels} aria-hidden="true">
            <For each={timelineLabels}>{(label) => <span style={`left:${label.position}%`}>{formatDuration(replayTickToMilliseconds(label.value as ReplayTick))}</span>}</For>
          </div>
          <div
            ref={fullscreenRail}
            class={styles.fullscreenRail}
            role="slider"
            tabIndex={0}
            aria-label="Fullscreen replay position"
            aria-valuemin={0}
            aria-valuemax={Math.floor(replayTickToMilliseconds(replayEnd()) / 1_000)}
            aria-valuenow={replaySecond()}
            aria-valuetext={`${formatDuration(replaySecond() * 1_000)} of ${formatDuration(replayTickToMilliseconds(replayEnd()))}`}
            onPointerDown={seekFromRail}
            onKeyDown={handleRailKey}
            data-testid="fullscreen-scrubber"
          >
            <span class={styles.railTrack} />
            <span class={styles.railFill} style={`width:${progress()}%`} />
            <Show when={props.clipRange}>
              <span
                class={styles.clipSelection}
                style={`left:${clipStartPosition()}%;width:${clipEndPosition() - clipStartPosition()}%`}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleStart}`}
                style={`left:${clipStartPosition()}%`}
                type="button"
                aria-label={`Clip starts at ${formatDuration(replayTickToMilliseconds(frameBoundaryTick(props.clipRange!.startFrame)))}`}
                onPointerDown={(event) => beginClipDrag(event, "start")}
                onKeyDown={(event) => props.onClipEndpointKeyDown(event, "start")}
                onKeyUp={(event) => props.onClipEndpointKeyUp(event, "start")}
                onBlur={() => props.onClipEndpointBlur("start")}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleEnd}`}
                style={`left:${clipEndPosition()}%`}
                type="button"
                aria-label={`Clip ends at ${formatDuration(replayTickToMilliseconds(frameBoundaryTick(props.clipRange!.endFrameExclusive)))}`}
                onPointerDown={(event) => beginClipDrag(event, "end")}
                onKeyDown={(event) => props.onClipEndpointKeyDown(event, "end")}
                onKeyUp={(event) => props.onClipEndpointKeyUp(event, "end")}
                onBlur={() => props.onClipEndpointBlur("end")}
              />
            </Show>
            <span class={styles.railHead} style={`left:${progress()}%`} />
            <For each={markerPositions()}>
              {(marker) => (
                <button
                  class={`${styles.fullscreenMarker} ${styles[`marker${marker.event.relation}`]}`}
                  style={`left:${marker.position}%`}
                  type="button"
                  aria-label={`Seek to ${eventTitle(marker.event)} at ${formatDuration(gameTickToMilliseconds(marker.event.game_tick))}`}
                  onPointerDown={(event) => event.stopPropagation()}
                  onClick={(event) => { event.stopPropagation(); props.onEventSelect(marker.event); }}
                />
              )}
            </For>
          </div>
        </div>
        <div class={styles.fullscreenControls}>
          <button class={styles.fullscreenPlay} type="button" disabled={!props.mediaAvailable} onClick={props.onTogglePlayback} aria-label={props.isPlaying ? "Pause fullscreen replay" : props.clipRange ? "Preview selected clip" : "Play fullscreen replay"}>
            <span aria-hidden="true">{props.isPlaying ? "Ⅱ" : "▶"}</span> {props.isPlaying ? "PAUSE" : props.clipRange ? "PREVIEW CLIP" : "PLAY"}
          </button>
          <Show
            when={props.clipRange}
            fallback={
              <button class={styles.clipControl} type="button" onClick={() => props.onActivateClip()}>
                <span aria-hidden="true">✦</span> CLIP
              </button>
            }
          >
            <button class={styles.cancelClipControl} type="button" onClick={props.onCancelClip}>
              CANCEL
            </button>
            <button class={styles.exportClipControl} type="button" onClick={props.onExportClip}>
              EXPORT CLIP <span aria-hidden="true">→</span>
            </button>
          </Show>
          <span class={styles.controlsDivider} />
          <PlaybackControls dark state={props.playback} onRate={props.onRate} onMuted={props.onMuted} onVolume={props.onVolume} />
          <span class={styles.fullscreenTime}><strong>{formatDuration(replaySecond() * 1_000)}</strong> / {formatDuration(replayTickToMilliseconds(replayEnd()))}</span>
        </div>
        <span class={styles.ambientTime}>{formatDuration(replaySecond() * 1_000)}</span>
      </section>

      <ChampionFilter
        participants={props.participants}
        selectedPlayers={props.selectedPlayers}
        localPlayerName={props.localPlayerName}
        mode="rail"
        onToggle={props.onPlayerToggle}
        onClear={props.onPlayerClear}
      />
    </div>
  );
}

export default FullscreenOverlay;
