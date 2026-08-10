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
  moveClipEndpoint,
  nearestIndexAt,
} from "../viewerUtils";
import ChampionFilter from "./ChampionFilter";
import styles from "./FullscreenOverlay.module.css";

type Props = {
  champion: string;
  localPlayerName: string | null;
  events: readonly ViewerEvent[];
  durationMs: number;
  videoTimeMs: number;
  gameClockSeconds: number;
  beforeGameStart: boolean;
  currentKda: KdaTimelinePoint | undefined;
  isPlaying: boolean;
  mediaAvailable: boolean;
  participants: readonly ReplayParticipant[];
  selectedPlayers: readonly string[];
  clipRange: ClipRange | null;
  onPlayerToggle: (summonerName: string) => void;
  onPlayerClear: () => void;
  onTogglePlayback: () => void;
  onSeek: (videoTimeMs: number) => void;
  onActivateClip: (event?: ViewerEvent) => void;
  onClipRangeChange: (range: ClipRange | null) => void;
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

  const progress = createMemo(() => clamp((props.videoTimeMs / props.durationMs) * 100, 0, 100));
  const videoSecond = createMemo(() => Math.floor(props.videoTimeMs / 1_000));
  const markerPositions = createMemo(() =>
    props.events.map((event, index) => ({
      event,
      index,
      position: clamp((event.video_time_ms / props.durationMs) * 100, 0, 100),
    })),
  );
  const clipStartPosition = createMemo(() =>
    props.clipRange
      ? clamp((props.clipRange.startMs / props.durationMs) * 100, 0, 100)
      : 0,
  );
  const clipEndPosition = createMemo(() =>
    props.clipRange
      ? clamp((props.clipRange.endMs / props.durationMs) * 100, 0, 100)
      : 100,
  );
  const minuteTicks = Array.from(
    { length: Math.floor(props.durationMs / 60_000) + 1 },
    (_, index) => ({
      minute: index,
      position: clamp(((index * 60_000) / props.durationMs) * 100, 0, 100),
      major: index % 4 === 0,
    }),
  );
  const timelineLabels = [0, 8 * 60_000, 16 * 60_000, 24 * 60_000, props.durationMs]
    .filter((value, index, values) => value <= props.durationMs && values.indexOf(value) === index)
    .map((value) => ({
      value,
      position: clamp((value / props.durationMs) * 100, 0, 100),
    }));

  const nearestEventIndex = createMemo(() => nearestIndexAt(props.events, props.videoTimeMs));
  const proximityEventIndex = createMemo(() => {
    const index = nearestEventIndex();
    return index >= 0 && Math.abs(props.events[index].video_time_ms - props.videoTimeMs) <= 1_000
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
    const event = props.events[index];
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
    props.onSeek(((event.clientX - bounds.left) / bounds.width) * props.durationMs);
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: number | undefined;
    if (event.key === "ArrowLeft" || event.key === "ArrowDown") target = props.videoTimeMs - 5_000;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") target = props.videoTimeMs + 5_000;
    if (event.key === "PageDown") target = props.videoTimeMs - 30_000;
    if (event.key === "PageUp") target = props.videoTimeMs + 30_000;
    if (event.key === "Home") target = 0;
    if (event.key === "End") target = props.durationMs;
    if (target === undefined) return;
    event.preventDefault();
    props.onSeek(target);
  };

  const updateClipEndpoint = (endpoint: "start" | "end", requestedMs: number) => {
    if (!props.clipRange) return;
    props.onClipRangeChange(
      moveClipEndpoint(
        props.clipRange,
        endpoint,
        requestedMs,
        props.durationMs,
        props.events,
      ),
    );
  };

  const beginClipDrag = (
    event: PointerEvent & { currentTarget: HTMLButtonElement },
    endpoint: "start" | "end",
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    const update = (pointerEvent: PointerEvent) => {
      const bounds = fullscreenRail.getBoundingClientRect();
      if (bounds.width <= 0) return;
      updateClipEndpoint(
        endpoint,
        ((pointerEvent.clientX - bounds.left) / bounds.width) * props.durationMs,
      );
    };
    const finish = (pointerEvent: PointerEvent) => {
      update(pointerEvent);
      handle.removeEventListener("pointermove", update);
      handle.removeEventListener("pointerup", finish);
      handle.removeEventListener("pointercancel", finish);
      if (handle.hasPointerCapture(pointerEvent.pointerId)) {
        handle.releasePointerCapture(pointerEvent.pointerId);
      }
    };
    handle.setPointerCapture(event.pointerId);
    handle.addEventListener("pointermove", update);
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", finish);
  };

  const handleClipEndpointKey = (event: KeyboardEvent, endpoint: "start" | "end") => {
    if (!props.clipRange) return;
    const current = endpoint === "start" ? props.clipRange.startMs : props.clipRange.endMs;
    const step = event.shiftKey ? 2_000 : 500;
    let requested = current;
    if (event.key === "ArrowLeft" || event.key === "ArrowDown") requested -= step;
    else if (event.key === "ArrowRight" || event.key === "ArrowUp") requested += step;
    else if (event.key === "Home") {
      requested = endpoint === "start" ? 0 : props.clipRange.startMs + 5_000;
    } else if (event.key === "End") {
      requested = endpoint === "end" ? props.durationMs : props.clipRange.endMs - 5_000;
    } else return;
    event.preventDefault();
    event.stopPropagation();
    updateClipEndpoint(endpoint, requested);
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
          <strong>{props.beforeGameStart ? "--:--" : formatDuration(props.gameClockSeconds * 1_000)}</strong>
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
        aria-label="Fullscreen replay controls"
        data-testid="fullscreen-scrubber-zone"
      >
        <div class={styles.fullscreenTimeline}>
          <div class={styles.minuteTicks} aria-hidden="true">
            <For each={minuteTicks}>{(tick) => <i classList={{ [styles.majorTick]: tick.major }} style={`left:${tick.position}%`} />}</For>
          </div>
          <div class={styles.timelineLabels} aria-hidden="true">
            <For each={timelineLabels}>{(label) => <span style={`left:${label.position}%`}>{formatDuration(label.value)}</span>}</For>
          </div>
          <div
            ref={fullscreenRail}
            class={styles.fullscreenRail}
            role="slider"
            tabIndex={0}
            aria-label="Fullscreen replay position"
            aria-valuemin={0}
            aria-valuemax={Math.floor(props.durationMs / 1_000)}
            aria-valuenow={videoSecond()}
            aria-valuetext={`${formatDuration(videoSecond() * 1_000)} of ${formatDuration(props.durationMs)}`}
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
                aria-label={`Clip starts at ${formatDuration(props.clipRange!.startMs)}`}
                onPointerDown={(event) => beginClipDrag(event, "start")}
                onKeyDown={(event) => handleClipEndpointKey(event, "start")}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleEnd}`}
                style={`left:${clipEndPosition()}%`}
                type="button"
                aria-label={`Clip ends at ${formatDuration(props.clipRange!.endMs)}`}
                onPointerDown={(event) => beginClipDrag(event, "end")}
                onKeyDown={(event) => handleClipEndpointKey(event, "end")}
              />
            </Show>
            <span class={styles.railHead} style={`left:${progress()}%`} />
            <For each={markerPositions()}>
              {(marker) => (
                <button
                  class={`${styles.fullscreenMarker} ${styles[`marker${marker.event.relation}`]}`}
                  style={`left:${marker.position}%`}
                  type="button"
                  aria-label={`Create clip from ${eventTitle(marker.event)} at ${formatDuration(marker.event.game_time_ms)}`}
                  onPointerDown={(event) => event.stopPropagation()}
                  onClick={(event) => { event.stopPropagation(); props.onActivateClip(marker.event); }}
                />
              )}
            </For>
          </div>
        </div>
        <div class={styles.fullscreenControls}>
          <button class={styles.fullscreenPlay} type="button" disabled={!props.mediaAvailable} onClick={props.onTogglePlayback} aria-label={props.isPlaying ? "Pause fullscreen replay" : "Play fullscreen replay"}>
            <span aria-hidden="true">{props.isPlaying ? "Ⅱ" : "▶"}</span> {props.isPlaying ? "PAUSE" : "PLAY"}
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
          <span class={styles.fullscreenTime}><strong>{formatDuration(videoSecond() * 1_000)}</strong> / {formatDuration(props.durationMs)}</span>
        </div>
        <span class={styles.ambientTime}>{formatDuration(videoSecond() * 1_000)}</span>
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
