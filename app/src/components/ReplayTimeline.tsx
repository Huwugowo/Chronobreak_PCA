import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { formatDuration } from "../format";
import {
  REPLAY_TICKS_PER_SECOND,
  type ReplayTick,
} from "../replayTime";
import type { ViewerEvent } from "../types";
import { eventTitle } from "../viewerUtils";
import {
  clusterTimelineEvents,
  isTickInTimelineViewport,
  normalizeTimelineViewport,
  panTimelineViewport,
  timelineRatioAtTick,
  timelineTickAtRatio,
  timelineViewportAroundRange,
  timelineViewportSpan,
  wholeTimelineViewport,
  zoomTimelineViewport,
  type TimelineViewport,
} from "../replayTimelineGeometry";
import styles from "./ReplayTimeline.module.css";

type TimelineEvent = ViewerEvent & { replay_tick: ReplayTick };

type ClipWindow = {
  start: ReplayTick;
  end: ReplayTick;
};

type Props = {
  duration: ReplayTick;
  playhead: ReplayTick;
  viewport: TimelineViewport;
  onViewportChange: (viewport: TimelineViewport) => void;
  events: readonly TimelineEvent[];
  clip: ClipWindow | null;
  onSeek: (tick: ReplayTick) => void;
  onEventSelect: (event: TimelineEvent) => void;
  onClipEndpointEditStart: (endpoint: "start" | "end") => void;
  onClipEndpointPreview: (endpoint: "start" | "end", tick: ReplayTick) => void;
  onClipEndpointEditFinish: () => void;
  onClipEndpointKeyDown: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointKeyUp: (event: KeyboardEvent, endpoint: "start" | "end") => void;
  onClipEndpointBlur: (endpoint: "start" | "end") => void;
};

const MINIMUM_VIEWPORT_SPAN = REPLAY_TICKS_PER_SECOND as ReplayTick;
const EVENT_GAP_PX = 18;

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

const clampRatio = (ratio: number): number =>
  Math.min(1, Math.max(0, ratio));

function ReplayTimeline(props: Props) {
  let rail!: HTMLDivElement;

  const viewport = () => props.viewport;
  const setViewport = (
    next: TimelineViewport | ((current: TimelineViewport) => TimelineViewport),
  ) => {
    const resolved =
      typeof next === "function" ? next(props.viewport) : next;
    props.onViewportChange(resolved);
  };

  const [railWidth, setRailWidth] = createSignal(0);
  const [seeking, setSeeking] = createSignal(false);
  const [hoverRatio, setHoverRatio] = createSignal<number | null>(null);

  createEffect(() => {
    const current = props.viewport;
    const normalized = normalizeTimelineViewport(current, props.duration);
    if (
      normalized.start !== current.start ||
      normalized.end !== current.end
    ) {
      props.onViewportChange(normalized);
    }
  });

  onMount(() => {
    const measure = () => setRailWidth(rail.getBoundingClientRect().width);
    measure();

    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(rail);
    onCleanup(() => observer.disconnect());
  });

  const viewportSpan = createMemo(() => timelineViewportSpan(viewport()));
  const isZoomed = createMemo(
    () => viewport().start > 0 || viewport().end < props.duration,
  );

  const clusters = createMemo(() =>
    clusterTimelineEvents(
      props.events,
      viewport(),
      railWidth(),
      EVENT_GAP_PX,
    ),
  );

  const playheadRatio = createMemo(() =>
    timelineRatioAtTick(props.playhead, viewport()),
  );

  const playheadVisible = createMemo(() =>
    isTickInTimelineViewport(props.playhead, viewport()),
  );

  const fillRatio = createMemo(() =>
    clampRatio(playheadRatio()),
  );

  const hoveredTick = createMemo(() => {
    const ratio = hoverRatio();
    return ratio === null ? null : timelineTickAtRatio(ratio, viewport());
  });

  const viewportOverviewLeft = createMemo(() =>
    props.duration <= 0 ? 0 : (viewport().start / props.duration) * 100,
  );

  const viewportOverviewWidth = createMemo(() =>
    props.duration <= 0 ? 100 : (viewportSpan() / props.duration) * 100,
  );

  const playheadOverviewLeft = createMemo(() =>
    props.duration <= 0 ? 0 : clampRatio(props.playhead / props.duration) * 100,
  );

  const clipGeometry = createMemo(() => {
    const clip = props.clip;
    if (!clip) return null;

    const startRatio = timelineRatioAtTick(clip.start, viewport());
    const endRatio = timelineRatioAtTick(clip.end, viewport());
    const visibleStart = clampRatio(startRatio);
    const visibleEnd = clampRatio(endRatio);

    if (visibleEnd < 0 || visibleStart > 1 || endRatio < 0 || startRatio > 1) {
      return null;
    }

    return {
      startRatio,
      endRatio,
      visibleStart,
      visibleEnd,
    };
  });

  const ratioAtPointer = (clientX: number): number => {
    const bounds = rail.getBoundingClientRect();
    if (bounds.width <= 0) return 0;
    return clampRatio((clientX - bounds.left) / bounds.width);
  };

  const seekAtPointer = (clientX: number) => {
    props.onSeek(timelineTickAtRatio(ratioAtPointer(clientX), viewport()));
  };

  const handlePointerDown = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    setSeeking(true);
    event.currentTarget.setPointerCapture(event.pointerId);
    seekAtPointer(event.clientX);
  };

  const handlePointerMove = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    setHoverRatio(ratioAtPointer(event.clientX));
    if (seeking()) seekAtPointer(event.clientX);
  };

  const finishSeek = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (!seeking()) return;
    seekAtPointer(event.clientX);
    setSeeking(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const handleWheel = (
    event: WheelEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (railWidth() <= 0) return;
    event.preventDefault();

    const current = viewport();

    if (event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
      const pixelDelta = event.shiftKey ? event.deltaY : event.deltaX;
      const tickDelta = Math.round(
        (pixelDelta / railWidth()) * viewportSpan(),
      ) as ReplayTick;

      setViewport(
        panTimelineViewport(current, props.duration, tickDelta),
      );
      return;
    }

    if (event.deltaY === 0) return;

    const anchor = timelineTickAtRatio(
      ratioAtPointer(event.clientX),
      current,
    );
    const scale = event.deltaY < 0 ? 0.8 : 1.25;

    setViewport(
      zoomTimelineViewport(
        current,
        props.duration,
        anchor,
        scale,
        MINIMUM_VIEWPORT_SPAN,
      ),
    );
  };

  const zoomCluster = (
    start: ReplayTick,
    end: ReplayTick,
  ) => {
    const minimumSpan = Math.max(
      MINIMUM_VIEWPORT_SPAN,
      Math.round(viewportSpan() * 0.25),
    ) as ReplayTick;

    setViewport(
      timelineViewportAroundRange(
        start,
        end,
        props.duration,
        minimumSpan,
        0.5,
      ),
    );
  };

  const resetViewport = () => {
    setViewport(wholeTimelineViewport(props.duration));
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: ReplayTick | undefined;

    if (event.key === "PageDown") {
      target = (props.playhead - 30 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    }
    if (event.key === "PageUp") {
      target = (props.playhead + 30 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    }
    if (event.key === "Home") target = 0 as ReplayTick;
    if (event.key === "End") target = props.duration;

    if (target !== undefined) {
      event.preventDefault();
      props.onSeek(
        Math.min(props.duration, Math.max(0, target)) as ReplayTick,
      );
      return;
    }

    if (event.key === "0") {
      event.preventDefault();
      resetViewport();
      return;
    }

    if (event.key === "+" || event.key === "=" || event.key === "-") {
      event.preventDefault();
      const anchor = isTickInTimelineViewport(props.playhead, viewport())
        ? props.playhead
        : timelineTickAtRatio(0.5, viewport());

      setViewport(
        zoomTimelineViewport(
          viewport(),
          props.duration,
          anchor,
          event.key === "-" ? 1.25 : 0.8,
          MINIMUM_VIEWPORT_SPAN,
        ),
      );
      return;
    }

    if (
      event.shiftKey &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")
    ) {
      event.preventDefault();
      const direction = event.key === "ArrowLeft" ? -1 : 1;
      const delta = Math.round(viewportSpan() * 0.15 * direction) as ReplayTick;
      setViewport(
        panTimelineViewport(viewport(), props.duration, delta),
      );
    }
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
      props.onClipEndpointPreview(
        endpoint,
        timelineTickAtRatio(
          ratioAtPointer(pointerEvent.clientX),
          viewport(),
        ),
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
    <div class={styles.timelineRoot}>
      <div
        ref={rail}
        class={styles.rail}
        role="slider"
        tabIndex={0}
        aria-label="Replay position"
        aria-valuemin={0}
        aria-valuemax={Math.floor(replayTickToMilliseconds(props.duration) / 1_000)}
        aria-valuenow={Math.floor(replayTickToMilliseconds(props.playhead) / 1_000)}
        aria-valuetext={`${formatDuration(replayTickToMilliseconds(props.playhead))} of ${formatDuration(replayTickToMilliseconds(props.duration))}`}
        title="Wheel to zoom / Shift+wheel to pan / 0 to show full match"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={finishSeek}
        onPointerCancel={finishSeek}
        onPointerLeave={() => {
          if (!seeking()) setHoverRatio(null);
        }}
        onWheel={handleWheel}
        onKeyDown={handleRailKey}
        data-testid="replay-timeline"
      >
        <span class={styles.track} />
        <span
          class={styles.fill}
          style={`width:${fillRatio() * 100}%`}
        />

        <Show when={clipGeometry()}>
          {(geometry) => (
            <>
              <span
                class={styles.clipSelection}
                style={`left:${geometry().visibleStart * 100}%;width:${Math.max(0, geometry().visibleEnd - geometry().visibleStart) * 100}%`}
              />
              <Show when={geometry().startRatio >= 0 && geometry().startRatio <= 1}>
                <button
                  class={`${styles.clipHandle} ${styles.clipHandleStart}`}
                  style={`left:${geometry().startRatio * 100}%`}
                  type="button"
                  aria-label={`Clip starts at ${formatDuration(replayTickToMilliseconds(props.clip!.start))}`}
                  onPointerDown={(event) => beginClipDrag(event, "start")}
                  onKeyDown={(event) => props.onClipEndpointKeyDown(event, "start")}
                  onKeyUp={(event) => props.onClipEndpointKeyUp(event, "start")}
                  onBlur={() => props.onClipEndpointBlur("start")}
                />
              </Show>
              <Show when={geometry().endRatio >= 0 && geometry().endRatio <= 1}>
                <button
                  class={`${styles.clipHandle} ${styles.clipHandleEnd}`}
                  style={`left:${geometry().endRatio * 100}%`}
                  type="button"
                  aria-label={`Clip ends at ${formatDuration(replayTickToMilliseconds(props.clip!.end))}`}
                  onPointerDown={(event) => beginClipDrag(event, "end")}
                  onKeyDown={(event) => props.onClipEndpointKeyDown(event, "end")}
                  onKeyUp={(event) => props.onClipEndpointKeyUp(event, "end")}
                  onBlur={() => props.onClipEndpointBlur("end")}
                />
              </Show>
            </>
          )}
        </Show>

        <Show when={playheadVisible()}>
          <span
            class={styles.head}
            style={`left:${playheadRatio() * 100}%`}
          />
        </Show>

        <div class={styles.markers}>
          <For each={clusters()}>
            {(item) => (
              item.kind === "event" ? (
                <button
                  class={`${styles.marker} ${styles[`marker${item.event.relation}`]}`}
                  style={`left:${railWidth() > 0 ? (item.x / railWidth()) * 100 : 0}%`}
                  type="button"
                  aria-label={`Seek to ${eventTitle(item.event)} at ${formatDuration(replayTickToMilliseconds(item.event.replay_tick))}`}
                  title={`${eventTitle(item.event)} / ${formatDuration(replayTickToMilliseconds(item.event.replay_tick))}`}
                  onPointerDown={(event) => event.stopPropagation()}
                  onClick={(event) => {
                    event.stopPropagation();
                    props.onEventSelect(item.event);
                  }}
                />
              ) : (
                <button
                  class={styles.cluster}
                  style={`left:${railWidth() > 0 ? (item.x / railWidth()) * 100 : 0}%`}
                  type="button"
                  aria-label={`Zoom into ${item.events.length} events`}
                  title={`${item.events.length} events · click to zoom`}
                  onPointerDown={(event) => event.stopPropagation()}
                  onClick={(event) => {
                    event.stopPropagation();
                    zoomCluster(item.start, item.end);
                  }}
                >
                  {item.events.length}
                </button>
              )
            )}
          </For>
        </div>

        <Show when={hoveredTick() !== null}>
          <span
            class={styles.hoverTime}
            style={`left:${(hoverRatio() ?? 0) * 100}%`}
          >
            {formatDuration(replayTickToMilliseconds(hoveredTick()!))}
          </span>
        </Show>
      </div>

      <Show when={isZoomed()}>
        <div class={styles.overview}>
          <button type="button" class={styles.resetZoom} onClick={resetViewport}>
            FULL MATCH
          </button>
          <div class={styles.overviewTrack} aria-hidden="true">
            <span
              class={styles.overviewWindow}
              style={`left:${viewportOverviewLeft()}%;width:${viewportOverviewWidth()}%`}
            />
            <span
              class={styles.overviewPlayhead}
              style={`left:${playheadOverviewLeft()}%`}
            />
          </div>
          <span class={styles.viewportLabel}>
            {formatDuration(replayTickToMilliseconds(viewport().start))}
            {" - "}
            {formatDuration(replayTickToMilliseconds(viewport().end))}
          </span>
        </div>
      </Show>
    </div>
  );
}

export default ReplayTimeline;