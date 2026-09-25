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
import { timelineDisplayEvents } from "../timelineEventDisplay";
import {
  timelineClusterPresentation,
  timelineEventPresentation,
  type TimelineEventIconKey,
} from "../timelineEventPresentation";
import { SkullIcon, SwordIcon } from "../ui/icons";
import { eventTitle } from "../viewerUtils";
import {
  clusterTimelineEvents,
  isTickInTimelineViewport,
  normalizeTimelineViewport,
  panTimelineViewport,
  timelineRatioAtTick,
  timelineRulerTicks,
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
  localPlayerName: string | null;
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
const TIMELINE_EVENT_GLYPH_SIZE_PX = 15;
const TIMELINE_EVENT_FALLBACK_SIZE_PX = 8;
const TIMELINE_EVENT_ASSET_MAX_SIZE_PX = 18;
const EVENT_ALLOWED_OVERLAP_RATIO = 0.35;
const SCRUB_HOVER_MIN_Y_PX = 24;
const WHEEL_HORIZONTAL_PAN_RATIO = 1.5;
const WHEEL_ZOOM_SENSITIVITY = 0.0025;
const MAX_WHEEL_ZOOM_DELTA_PX = 160;
const RULER_TARGET_SPACING_PX = 100;
const FLOATING_TIME_HALF_WIDTH_PX = 28;

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

const clampRatio = (ratio: number): number =>
  Math.min(1, Math.max(0, ratio));

const formatTimelineTick = (
  tick: ReplayTick,
  visibleSpan: ReplayTick,
): string => {
  const milliseconds = replayTickToMilliseconds(tick);

  if (visibleSpan > 5 * REPLAY_TICKS_PER_SECOND) {
    return formatDuration(milliseconds);
  }

  const totalTenths = Math.round(milliseconds / 100);
  const minutes = Math.floor(totalTenths / 600);
  const secondsTenths = totalTenths - minutes * 600;
  const seconds = (secondsTenths / 10).toFixed(1).padStart(4, "0");

  return `${minutes}:${seconds}`;
};


type TimelineEventGlyphProps = {
  icon: TimelineEventIconKey;
  relation: ViewerEvent["relation"];
  size?: number;
};

const timelineEventAsset = (
  icon: TimelineEventIconKey,
  relation: ViewerEvent["relation"],
): string | null => {
  switch (icon) {
    case "dragon":
      return "/timeline-events/dragon.png";

    case "dragon-air":
      return "/timeline-events/dragon-air.png";

    case "dragon-earth":
      return "/timeline-events/dragon-earth.png";

    case "dragon-fire":
      return "/timeline-events/dragon-fire.png";

    case "dragon-water":
      return "/timeline-events/dragon-water.png";

    case "dragon-hextech":
      return "/timeline-events/dragon-hextech.png";

    case "dragon-chemtech":
      return "/timeline-events/dragon-chemtech.png";

    case "dragon-elder":
      return "/timeline-events/dragon-elder.png";

    case "baron":
      return "/timeline-events/baron.png";

    case "herald":
      return "/timeline-events/herald.png";

    case "tower":
      if (relation === "ally") {
        return "/timeline-events/tower-blue.png";
      }

      if (relation === "enemy") {
        return "/timeline-events/tower-red.png";
      }

      return null;

    case "inhibitor":
      if (relation === "ally") {
        return "/timeline-events/inhibitor-blue.png";
      }

      if (relation === "enemy") {
        return "/timeline-events/inhibitor-red.png";
      }

      return null;

    default:
      return null;
  }
};

const timelineEventAssetScale = (
  icon: TimelineEventIconKey,
): number => {
  switch (icon) {
    case "tower":
      return 1.19;

    case "herald":
      return 1.13;

    case "dragon-water":
      return 1.09;

    case "dragon":
    case "dragon-air":
    case "dragon-earth":
    case "dragon-elder":
      return 1.05;

    case "dragon-chemtech":
      return 1.03;

    case "inhibitor":
      return 0.92;

    default:
      return 1;
  }
};

const timelineEventVisualWidth = (
  icon: TimelineEventIconKey,
  relation: ViewerEvent["relation"],
): number => {
  if (icon === "swords" || icon === "skull") {
    return TIMELINE_EVENT_GLYPH_SIZE_PX;
  }

  if (timelineEventAsset(icon, relation) !== null) {
    return Math.min(
      TIMELINE_EVENT_ASSET_MAX_SIZE_PX,
      TIMELINE_EVENT_GLYPH_SIZE_PX *
        timelineEventAssetScale(icon),
    );
  }

  return TIMELINE_EVENT_FALLBACK_SIZE_PX;
};
function TimelineEventGlyph(props: TimelineEventGlyphProps) {
  if (props.icon === "swords") {
    return <SwordIcon size={props.size ?? TIMELINE_EVENT_GLYPH_SIZE_PX} />;
  }

  if (props.icon === "skull") {
    return <SkullIcon size={props.size ?? TIMELINE_EVENT_GLYPH_SIZE_PX} />;
  }

  const asset = () =>
    timelineEventAsset(
      props.icon,
      props.relation,
    );

  const assetSize = () =>
    (props.size ?? TIMELINE_EVENT_GLYPH_SIZE_PX) *
    timelineEventAssetScale(props.icon);

  return (
    <Show
      when={asset()}
      fallback={
        <span
          class={styles.markerFallback}
          aria-hidden="true"
        />
      }
    >
      {(src) => (
        <img
          class={styles.markerAsset}
          src={src()}
          alt=""
          aria-hidden="true"
          draggable={false}
          style={`width:${assetSize()}px;height:${assetSize()}px`}
        />
      )}
    </Show>
  );
}

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

  const displayEvents = createMemo(() =>
    timelineDisplayEvents(props.events),
  );

  const anchorEvents = createMemo(() =>
    displayEvents().filter((event) =>
      isTickInTimelineViewport(
        event.replay_tick,
        viewport(),
      ),
    ),
  );

  const clusters = createMemo(() =>
    clusterTimelineEvents(
      displayEvents(),
      viewport(),
      railWidth(),
      {
        visualWidthOf: (event) => {
          const presentation = timelineEventPresentation(
            event.source,
            props.localPlayerName ?? null,
          );

          return timelineEventVisualWidth(
            presentation.icon,
            presentation.relation,
          );
        },
        allowedOverlapRatio: EVENT_ALLOWED_OVERLAP_RATIO,
      },
      (event) =>
        timelineEventPresentation(
          event.source,
          props.localPlayerName ?? null,
        ).localPriority,
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

  const rulerTicks = createMemo(() =>
    timelineRulerTicks(
      viewport(),
      railWidth(),
      RULER_TARGET_SPACING_PX,
    ),
  );

  const floatingLabelRatio = (ratio: number): number => {
    const width = railWidth();
    const bounded = clampRatio(ratio);

    if (width <= 0) return bounded;

    const edgeRatio = Math.min(
      0.5,
      FLOATING_TIME_HALF_WIDTH_PX / width,
    );

    return Math.min(
      1 - edgeRatio,
      Math.max(edgeRatio, bounded),
    );
  };

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
    const ratio = ratioAtPointer(event.clientX);

    if (seeking()) {
      setHoverRatio(ratio);
      seekAtPointer(event.clientX);
      return;
    }

    const bounds = rail.getBoundingClientRect();
    const localY = event.clientY - bounds.top;

    setHoverRatio(
      localY >= SCRUB_HOVER_MIN_Y_PX
        ? ratio
        : null,
    );
  };

  const finishSeek = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (!seeking()) return;

    const ratio = ratioAtPointer(event.clientX);
    const bounds = rail.getBoundingClientRect();
    const localY = event.clientY - bounds.top;

    seekAtPointer(event.clientX);
    setSeeking(false);
    setHoverRatio(
      localY >= SCRUB_HOVER_MIN_Y_PX
        ? ratio
        : null,
    );

    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  const handleWheel = (
    event: WheelEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (railWidth() <= 0) return;

    const deltaX = event.deltaX;
    const deltaY = event.deltaY;
    const absX = Math.abs(deltaX);
    const absY = Math.abs(deltaY);

    /*
     * Precision touchpads frequently emit a little horizontal
     * movement during an intended vertical two-finger gesture.
     *
     * Only treat it as horizontal panning when that intent is
     * clearly dominant. Browser trackpad pinch is generally
     * represented as ctrl+wheel and must always remain zoom.
     */
    const horizontalIntent =
      !event.ctrlKey &&
      (
        event.shiftKey ||
        absX >
          absY *
            WHEEL_HORIZONTAL_PAN_RATIO
      );

    if (horizontalIntent) {
      event.preventDefault();

      const pixelDelta =
        event.shiftKey
          ? (
              absX > absY
                ? deltaX
                : deltaY
            )
          : deltaX;

      if (pixelDelta === 0) return;

      const tickDelta = Math.round(
        (pixelDelta / railWidth()) *
          viewportSpan(),
      ) as ReplayTick;

      setViewport(
        panTimelineViewport(
          viewport(),
          props.duration,
          tickDelta,
        ),
      );

      return;
    }

    if (deltaY === 0) return;

    event.preventDefault();

    const current = viewport();

    const anchor =
      timelineTickAtRatio(
        ratioAtPointer(event.clientX),
        current,
      );

    /*
     * Continuous exponential scaling works for both devices:
     *
     * - tiny trackpad deltas create tiny zoom changes
     * - ordinary mouse-wheel deltas remain meaningfully sized
     *
     * Clamp extreme deltas so one unusual wheel event cannot
     * jump across several zoom levels.
     */
    const boundedDelta =
      Math.max(
        -MAX_WHEEL_ZOOM_DELTA_PX,
        Math.min(
          MAX_WHEEL_ZOOM_DELTA_PX,
          deltaY,
        ),
      );

    const scale =
      Math.exp(
        boundedDelta *
          WHEEL_ZOOM_SENSITIVITY,
      );

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
        <div class={styles.ruler} aria-hidden="true" data-testid="timeline-ruler">
          <For each={rulerTicks()}>
            {(tick) => {
              const ratio = () => timelineRatioAtTick(tick, viewport());

              return (
                <span
                  classList={{
                    [styles.rulerTick]: true,
                    [styles.rulerTickStart]: ratio() < 0.04,
                    [styles.rulerTickEnd]: ratio() > 0.96,
                  }}
                  style={`left:${ratio() * 100}%`}
                >
                  {formatTimelineTick(tick, viewportSpan())}
                </span>
              );
            }}
          </For>
        </div>

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
          <For each={anchorEvents()}>
            {(event) => (
              <span
                class={styles.eventAnchor}
                style={`left:${timelineRatioAtTick(
                  event.replay_tick,
                  viewport(),
                ) * 100}%`}
                data-event-anchor="true"
                data-elapsed={
                  event.replay_tick <= props.playhead
                    ? "true"
                    : undefined
                }
                aria-hidden="true"
              />
            )}
          </For>

          <For each={clusters()}>
            {(item) => {
              if (item.kind === "event") {
                const source = () => item.event.source;

                const presentation = () =>
                  timelineEventPresentation(
                    source(),
                    props.localPlayerName ?? null,
                  );

                const multiplier = () =>
                  item.event.multiplier ??
                  presentation().multiplier;

                const label = () =>
                  presentation().kind === "death"
                    ? "Your death"
                    : eventTitle(source());

                const locallyEmphasized = () =>
                  presentation().localRole === "victim" ||
                  presentation().localRole === "killer" ||
                  presentation().localRole === "actor";

                return (
                  <button
                    classList={{
                      [styles.marker]: true,
                      [styles[`event${presentation().relation}`]]: true,
                      [styles.markerLocal]: locallyEmphasized(),
                    }}
                    style={`left:${railWidth() > 0 ? (item.x / railWidth()) * 100 : 0}%;--event-hit-width:${timelineEventVisualWidth(presentation().icon, presentation().relation)}px`}
                    type="button"
                    aria-label={`Seek to ${label()} at ${formatDuration(
                      replayTickToMilliseconds(
                        item.event.replay_tick,
                      ),
                    )}`}
                    title={`${label()} / ${formatDuration(
                      replayTickToMilliseconds(
                        item.event.replay_tick,
                      ),
                    )}`}
                    data-event-icon={presentation().icon}
                    data-local-role={
                      presentation().localRole ??
                      undefined
                    }
                    data-local-emphasis={
                      locallyEmphasized()
                        ? "true"
                        : undefined
                    }
                    onPointerDown={(event) =>
                      event.stopPropagation()
                    }
                    onClick={(event) => {
                      event.stopPropagation();
                      props.onEventSelect(source());
                    }}
                  >
                    <span class={styles.markerGlyph}>
                      <TimelineEventGlyph
                        icon={presentation().icon}
                        relation={presentation().relation}
                      />
                    </span>

                    <Show
                      when={
                        multiplier() !== null &&
                        multiplier()! > 1
                      }
                    >
                      <span
                        class={styles.markerMultiplier}
                      >
                        {multiplier()}
                      </span>
                    </Show>
                  </button>
                );
              }

              const sources = () =>
                item.events.map(
                  (event) => event.source,
                );

              const presentation = () =>
                timelineClusterPresentation(
                  sources(),
                  props.localPlayerName ?? null,
                );

              return (
                <button
                  classList={{
                    [styles.cluster]: true,
                    [styles[`event${presentation().relation}`]]: true,
                  }}
                  style={`left:${railWidth() > 0 ? (item.x / railWidth()) * 100 : 0}%`}
                  type="button"
                  aria-label={`Zoom into ${item.events.length} events`}
                  title={`${item.events.length} events · click to zoom`}
                  data-cluster-kind={
                    presentation().homogeneous
                      ? "homogeneous"
                      : "mixed"
                  }
                  data-cluster-icon={
                    presentation().homogeneous
                      ? presentation().icon
                      : undefined
                  }
                  onPointerDown={(event) =>
                    event.stopPropagation()
                  }
                  onClick={(event) => {
                    event.stopPropagation();
                    zoomCluster(
                      item.start,
                      item.end,
                    );
                  }}
                >
                  <Show
                    when={presentation().homogeneous}
                    fallback={
                      <span
                        class={styles.clusterMixed}
                      >
                        +{item.events.length}
                      </span>
                    }
                  >
                    <span
                      class={styles.clusterGlyph}
                    >
                      <TimelineEventGlyph
                        icon={presentation().icon}
                        relation={presentation().relation}
                        size={13}
                      />
                    </span>

                    <span
                      class={styles.clusterCount}
                    >
                      {item.events.length}
                    </span>
                  </Show>
                </button>
              );
            }}
          </For>
        </div>

        <Show when={hoveredTick() !== null}>
          <span
            class={styles.hoverTime}
            style={`left:${floatingLabelRatio(hoverRatio() ?? 0) * 100}%`}
          >
            {formatTimelineTick(hoveredTick()!, viewportSpan())}
          </span>
        </Show>
      </div>

      <Show when={isZoomed()}>
        <div class={styles.overview} data-testid="timeline-viewport-readout">
          <span
            class={`${styles.viewportBoundary} ${styles.viewportBoundaryStart}`}
          >
            {formatTimelineTick(viewport().start, viewportSpan())}
          </span>

          <button
            type="button"
            class={styles.overviewTrack}
            onClick={resetViewport}
            aria-label="Show full match"
            title="Show full match"
          >
            <span
              class={styles.overviewWindow}
              style={`left:${viewportOverviewLeft()}%;width:${viewportOverviewWidth()}%`}
            />
            <span
              class={styles.overviewPlayhead}
              style={`left:${playheadOverviewLeft()}%`}
            />
          </button>

          <span
            class={`${styles.viewportBoundary} ${styles.viewportBoundaryEnd}`}
          >
            {formatTimelineTick(viewport().end, viewportSpan())}
          </span>
        </div>
      </Show>
    </div>
  );
}

export default ReplayTimeline;
