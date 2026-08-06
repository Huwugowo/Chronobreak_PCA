import { For, Show, createEffect, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import type {
  ClipRange,
  GoldTimelinePoint,
  KdaTimelinePoint,
  ReplayParticipant,
  ViewerEvent,
} from "../types";
import {
  clamp,
  eventCategory,
  eventSummary,
  eventTitle,
  formatGoldDifference,
  moveClipEndpoint,
  nearestIndexAt,
  timelineValue,
} from "../viewerUtils";
import ChampionFilter from "./ChampionFilter";
import styles from "./FullscreenOverlay.module.css";

type Props = {
  champion: string;
  localPlayerName: string | null;
  events: readonly ViewerEvent[];
  goldTimeline: readonly GoldTimelinePoint[];
  durationMs: number;
  videoTimeMs: number;
  gameClockSeconds: number;
  beforeGameStart: boolean;
  currentKda: KdaTimelinePoint | undefined;
  isPlaying: boolean;
  mediaAvailable: boolean;
  goldOpen: boolean;
  participants: readonly ReplayParticipant[];
  selectedPlayers: readonly string[];
  clipRange: ClipRange | null;
  onGoldOpenChange: (open: boolean) => void;
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

const GRAPH_WIDTH = 1_000;
const GRAPH_HEIGHT = 136;
const GRAPH_LIMIT = 4_500;

const graphY = (goldDiff: number): number =>
  ((GRAPH_LIMIT - clamp(goldDiff, -GRAPH_LIMIT, GRAPH_LIMIT)) / (GRAPH_LIMIT * 2)) *
  GRAPH_HEIGHT;

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
  const currentGold = createMemo(() =>
    timelineValue(props.goldTimeline, props.videoTimeMs) ?? props.goldTimeline[0],
  );
  const graph = (() => {
    if (props.goldTimeline.length === 0) return null;
    const finalGameTime = Math.max(
      props.goldTimeline[props.goldTimeline.length - 1].game_time_ms,
      1,
    );
    const points = props.goldTimeline.map((point) => ({
      x: (point.game_time_ms / finalGameTime) * GRAPH_WIDTH,
      y: graphY(point.gold_diff),
    }));
    const line = points.map((point, index) => `${index === 0 ? "M" : "L"}${point.x},${point.y}`).join(" ");
    const zeroY = graphY(0);
    const area = `M${points[0].x},${zeroY} ${points
      .map((point) => `L${point.x},${point.y}`)
      .join(" ")} L${points[points.length - 1].x},${zeroY} Z`;
    const labels = [0, 8 * 60_000, 16 * 60_000, 24 * 60_000, finalGameTime]
      .filter((value, index, values) => value <= finalGameTime && values.indexOf(value) === index)
      .map((value) => ({
        text: formatDuration(value),
        position: (value / finalGameTime) * 100,
      }));
    return { finalGameTime, line, area, labels };
  })();
  const graphCursor = createMemo(() => {
    if (!graph || props.goldTimeline.length === 0) return null;
    const firstPoint = props.goldTimeline[0];
    const videoOffsetMs = firstPoint.video_time_ms - firstPoint.game_time_ms;
    const gameTimeMs = Math.max(0, props.videoTimeMs - videoOffsetMs);
    const point = currentGold() ?? props.goldTimeline[0];
    return {
      x: clamp((gameTimeMs / graph.finalGameTime) * GRAPH_WIDTH, 0, GRAPH_WIDTH),
      y: graphY(point.gold_diff),
      positive: point.gold_diff >= 0,
    };
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
          <Show when={currentGold()}>
            {(point) => (
              <span classList={{ [styles.goldPositive]: point().gold_diff >= 0, [styles.goldNegative]: point().gold_diff < 0 }}>
                {formatGoldDifference(point().gold_diff)}
              </span>
            )}
          </Show>
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

      <Show when={graph && props.goldTimeline.length > 0}>
        <section
          classList={{
            [styles.goldDrawer]: true,
            [styles.goldDrawerOpen]: props.goldOpen,
            [styles.goldDrawerRaised]: scrubberActive(),
          }}
          aria-hidden={!props.goldOpen}
          data-testid="gold-drawer"
        >
          <div class={styles.goldHeader}>
            <div><span>POST-GAME TIMELINE</span><strong>GOLD DIFFERENTIAL</strong></div>
            <Show when={currentGold()}>
              {(point) => (
                <div classList={{ [styles.goldPositive]: point().gold_diff >= 0, [styles.goldNegative]: point().gold_diff < 0 }}>
                  <strong>{formatGoldDifference(point().gold_diff)}</strong>
                  <span>{point().gold_diff >= 0 ? "ADVANTAGE" : "DEFICIT"}</span>
                </div>
              )}
            </Show>
          </div>
          <div class={styles.goldChart}>
            <div class={styles.yLabels}><span>+4K</span><span>+2K</span><span>0</span><span>−2K</span><span>−4K</span></div>
            <svg viewBox={`0 0 ${GRAPH_WIDTH} ${GRAPH_HEIGHT}`} preserveAspectRatio="none" aria-label="Gold differential graph">
              <defs>
                <linearGradient id="gold-line-gradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0" stop-color="#4A70E0" />
                  <stop offset="49.8%" stop-color="#4A70E0" />
                  <stop offset="50.2%" stop-color="#C2001C" />
                  <stop offset="100%" stop-color="#C2001C" />
                </linearGradient>
              </defs>
              <line class={styles.zeroLine} x1="0" x2={GRAPH_WIDTH} y1={GRAPH_HEIGHT / 2} y2={GRAPH_HEIGHT / 2} />
              <path class={styles.goldArea} d={graph!.area} />
              <path class={styles.goldLine} d={graph!.line} />
              <Show when={graphCursor()}>
                {(cursor) => (
                  <g classList={{ [styles.cursorPositive]: cursor().positive, [styles.cursorNegative]: !cursor().positive }}>
                    <line class={styles.goldCursorLine} x1={cursor().x} x2={cursor().x} y1="0" y2={GRAPH_HEIGHT} />
                    <rect class={styles.goldCursorPoint} x={cursor().x - 4} y={cursor().y - 4} width="8" height="8" transform={`rotate(45 ${cursor().x} ${cursor().y})`} />
                  </g>
                )}
              </Show>
            </svg>
            <div class={styles.graphLabels}>
              <For each={graph!.labels}>{(label) => <span style={`left:${label.position}%`}>{label.text}</span>}</For>
            </div>
          </div>
        </section>

        <button
          classList={{
            [styles.goldTab]: true,
            [styles.goldTabActive]: scrubberActive() || props.goldOpen,
            [styles.goldTabRaised]: scrubberActive(),
          }}
          type="button"
          onClick={() => props.onGoldOpenChange(!props.goldOpen)}
          aria-expanded={props.goldOpen}
          data-testid="gold-tab"
        >
          <span aria-hidden="true">{props.goldOpen ? "▾" : "▴"}</span> GOLD
          <Show when={currentGold()}>
            {(point) => <strong classList={{ [styles.goldPositive]: point().gold_diff >= 0, [styles.goldNegative]: point().gold_diff < 0 }}>{formatGoldDifference(point().gold_diff)}</strong>}
          </Show>
        </button>
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
          <Show when={props.goldTimeline.length > 0}>
            <button class={styles.goldControl} type="button" onClick={() => props.onGoldOpenChange(!props.goldOpen)} aria-expanded={props.goldOpen}> {props.goldOpen ? "▾" : "▴"} GOLD</button>
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
