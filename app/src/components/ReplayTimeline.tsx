import {
  For,
  Show,
  createEffect,
  createMemo,
  createSelector,
  createSignal,
  createUniqueId,
  on,
  onCleanup,
  onMount,
} from "solid-js";
import { formatDuration } from "../format";
import type { SeekOptions } from "../playbackController";
import {
  REPLAY_TICKS_PER_SECOND,
  type ReplayTick,
} from "../replayTime";
import type { ReplayParticipant, ViewerEvent } from "../types";
import { timelineDisplayEvents, type TimelineDisplayEvent } from "../timelineEventDisplay";
import { combatSide, timelineParticipantIndex } from "../timelineCombat";
import { sameSource } from "../leagueSource";
import TimelineCombatCard from "./TimelineCombatCard";
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
  participants?: readonly ReplayParticipant[];
  assetBase?: string | null;
  localPlayerName: string | null;
  clip: ClipWindow | null;
  onSeek: (tick: ReplayTick, options?: Pick<SeekOptions, "scrubCommit">) => void;
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

  if (props.icon === "dot") {
    return <span class={styles.combatDot} aria-hidden="true" />;
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
  let positionControl!: HTMLDivElement;

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
  let seekPointer: { element: HTMLDivElement; id: number } | null = null;
  let endClipDrag: (() => void) | undefined;
  const cancelSeek = () => {
    const pointer = seekPointer;
    seekPointer = null;
    setSeeking(false);
    if (pointer?.element.hasPointerCapture(pointer.id)) pointer.element.releasePointerCapture(pointer.id);
  };
  onCleanup(() => { cancelSeek(); endClipDrag?.(); });
  const [hoverRatio, setHoverRatio] = createSignal<number | null>(null);
  const [hoveredCombat, setHoveredCombat] = createSignal<TimelineDisplayEvent | null>(null);
  const [highlightedCombat, setHighlightedCombat] = createSignal<ViewerEvent | null>(null);
  const isCombatHighlighted = createSelector(highlightedCombat);
  const combatCardId = createUniqueId();
  const participantIndex = createMemo(() => timelineParticipantIndex(props.participants ?? []));
  let combatCloseTimer: ReturnType<typeof setTimeout> | undefined;
  let combatFocusReturn: HTMLButtonElement | undefined;
  const keepCombatOpen = () => { clearTimeout(combatCloseTimer); };
  const closeCombat = () => {
    keepCombatOpen();
    setHoveredCombat(null);
    setHighlightedCombat(null);
  };
  const scheduleCombatClose = () => {
    keepCombatOpen();
    combatCloseTimer = setTimeout(() => {
      if (!rail.querySelector('[data-combat-card]')?.contains(document.activeElement)) closeCombat();
    }, 140);
  };
  onCleanup(keepCombatOpen);

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

  // Display events are sorted by tick. Keep frame-rate work logarithmic and
  // notify anchor bindings only when the passed-event boundary changes.
  const elapsedAnchorCount = createMemo(() => {
    const events = anchorEvents(), playhead = props.playhead;
    let low = 0, high = events.length;
    while (low < high) {
      const middle = Math.floor((low + high) / 2);
      if (events[middle].replay_tick <= playhead) low = middle + 1;
      else high = middle;
    }
    return low;
  });
  const anchorElapsed = createSelector(elapsedAnchorCount, (index: number, count) => index < count);

  const clusters = createMemo(() =>
    clusterTimelineEvents(
      displayEvents().filter(event => event.source.event_type !== "ChampionKill"),
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

  const combatGroups = createMemo(() => clusterTimelineEvents(
    displayEvents().filter(event => event.source.event_type === "ChampionKill"),
    viewport(), railWidth(),
    // Keep the existing 9.75px midpoint collision rule for hover membership,
    // independently of the smaller dots. No marker is collapsed for combat.
    { visualWidthOf: () => TIMELINE_EVENT_GLYPH_SIZE_PX, allowedOverlapRatio: EVENT_ALLOWED_OVERLAP_RATIO },
  ).map(group => group.kind === "event" ? [group.event] : group.events));
  const combatEvents = createMemo(() => combatGroups().flat());
  const combatMembership = createMemo(() => new Map(combatGroups().flatMap(group =>
    group.map(item => [item, group] as const))));
  const hoveredCombatGroup = createMemo(() => {
    const hovered = hoveredCombat();
    return hovered ? combatMembership().get(hovered) : undefined;
  });
  createEffect(on(combatGroups, groups => {
    const hovered = hoveredCombat();
    if (!hovered) return;
    const replacement = groups.flat().find(item => item === hovered ||
      (item.source.source_id !== undefined && item.source.source_id === hovered.source.source_id &&
        item.source.source && hovered.source.source && sameSource(item.source.source, hovered.source.source)));
    if (replacement) {
      setHoveredCombat(replacement);
      setHighlightedCombat(replacement.source);
    } else closeCombat();
  }));
  const openCombat = (item: TimelineDisplayEvent) => {
    keepCombatOpen();
    setHoverRatio(null);
    setHoveredCombat(item);
    setHighlightedCombat(item.source);
  };
  const selectCombat = (item: TimelineDisplayEvent) => {
    props.onEventSelect(item.source);
    if (rail.querySelector('[data-combat-card]')?.contains(document.activeElement)) positionControl.focus({ preventScroll: true });
    closeCombat();
  };
  const nearestCombat = (clientX: number) => {
    const x = ratioAtPointer(clientX) * railWidth();
    const tick = timelineTickAtRatio(ratioAtPointer(clientX), viewport());
    let nearest: TimelineDisplayEvent | undefined;
    let distance = Infinity;
    for (const item of combatEvents()) {
      const delta = Math.abs(timelineRatioAtTick(item.replay_tick, viewport()) * railWidth() - x);
      const tickDistance = Math.abs(item.replay_tick - tick);
      // A 15px target retains the existing marker accessibility/hit area.
      if (delta <= 7.5 && tickDistance < distance) { nearest = item; distance = tickDistance; }
    }
    return nearest;
  };
  const combatCardLeft = () => {
    const item = hoveredCombat();
    const width = railWidth();
    if (!item || width <= 300) return width / 2;
    const x = timelineRatioAtTick(item.replay_tick, viewport()) * width;
    return Math.min(width - 150, Math.max(150, x));
  };

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
    closeCombat();
    event.preventDefault();
    positionControl.focus({ preventScroll: true });
    endClipDrag?.();
    cancelSeek();
    seekPointer = { element: event.currentTarget, id: event.pointerId };
    setSeeking(true);
    event.currentTarget.setPointerCapture(event.pointerId);
    seekAtPointer(event.clientX);
  };

  const handlePointerMove = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    const ratio = ratioAtPointer(event.clientX);

    if (seekPointer) {
      if (seekPointer.id !== event.pointerId) return;
      setHoverRatio(ratio);
      props.onSeek(timelineTickAtRatio(ratio, viewport()));
      return;
    }

    const bounds = rail.getBoundingClientRect();
    const localY = event.clientY - bounds.top;

    if (localY < SCRUB_HOVER_MIN_Y_PX) {
      const nearest = nearestCombat(event.clientX);
      if (nearest) { openCombat(nearest); return; }
    }
    scheduleCombatClose();

    setHoverRatio(
      localY >= SCRUB_HOVER_MIN_Y_PX
        ? ratio
        : null,
    );
  };

  const finishSeek = (
    event: PointerEvent & { currentTarget: HTMLDivElement },
  ) => {
    if (seekPointer?.id !== event.pointerId) return;

    const ratio = ratioAtPointer(event.clientX);
    const bounds = rail.getBoundingClientRect();
    const localY = event.clientY - bounds.top;

    props.onSeek(timelineTickAtRatio(ratio, viewport()), { scrubCommit: true });
    cancelSeek();
    setHoverRatio(
      localY >= SCRUB_HOVER_MIN_Y_PX
        ? ratio
        : null,
    );

  };

  const handleWheel = (
    event: WheelEvent & { currentTarget: HTMLDivElement },
  ) => {
    closeCombat();
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
    if (event.altKey || event.metaKey) return;
    let target: ReplayTick | undefined;

    if (!event.shiftKey && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      const direction = event.key === "ArrowLeft" || event.key === "ArrowDown" ? -1 : 1;
      target = (props.playhead + direction * (event.ctrlKey ? 5 : 1) * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    }

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
      event.stopPropagation();
      props.onSeek(
        Math.min(props.duration, Math.max(0, target)) as ReplayTick,
      );
      return;
    }

    if (event.key === "0") {
      event.preventDefault();
      event.stopPropagation();
      resetViewport();
      return;
    }

    if (event.key === "+" || event.key === "=" || event.key === "-") {
      event.preventDefault();
      event.stopPropagation();
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
      event.stopPropagation();
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
    if (event.button !== 0) return;
    closeCombat();
    event.preventDefault();
    event.stopPropagation();
    endClipDrag?.();
    cancelSeek();
    const handle = event.currentTarget;
    const pointerId = event.pointerId;
    handle.focus({ preventScroll: true });
    props.onClipEndpointEditStart(endpoint);

    const update = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId !== pointerId) return;
      props.onClipEndpointPreview(endpoint,
        timelineTickAtRatio(ratioAtPointer(pointerEvent.clientX), viewport()));
    };
    const cleanup = () => {
      if (endClipDrag !== cleanup) return;
      endClipDrag = undefined;
      handle.removeEventListener("pointermove", update);
      handle.removeEventListener("pointerup", finish);
      handle.removeEventListener("pointercancel", cancel);
      handle.removeEventListener("lostpointercapture", cancel);
      if (handle.hasPointerCapture(pointerId)) handle.releasePointerCapture(pointerId);
      props.onClipEndpointEditFinish();
    };
    const finish = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId !== pointerId) return;
      update(pointerEvent);
      cleanup();
    };
    const cancel = (pointerEvent: PointerEvent) => {
      if (pointerEvent.pointerId === pointerId) cleanup();
    };
    endClipDrag = cleanup;
    handle.setPointerCapture(pointerId);
    handle.addEventListener("pointermove", update);
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", cancel);
    handle.addEventListener("lostpointercapture", cancel);
  };

  return (
    <div class={styles.timelineRoot}>
      <div
        ref={rail}
        class={styles.rail}
        role="group"
        aria-label="Replay timeline"
        title="Wheel to zoom / Shift+wheel to pan / 0 to show full match"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={finishSeek}
        onPointerCancel={(event) => { if (seekPointer?.id === event.pointerId) cancelSeek(); }}
        onLostPointerCapture={(event) => { if (seekPointer?.id === event.pointerId) cancelSeek(); }}
        onPointerLeave={() => {
          if (!seeking()) setHoverRatio(null);
          scheduleCombatClose();
        }}
        onKeyDown={event => {
          if (event.key === "Escape" && hoveredCombat()) {
            event.preventDefault(); event.stopPropagation();
            combatFocusReturn?.focus({ preventScroll: true });
            closeCombat();
          }
        }}
        onFocusOut={event => { if (!rail.contains(event.relatedTarget as Node | null)) scheduleCombatClose(); }}
        onWheel={handleWheel}
        data-testid="replay-timeline"
      >
        <div
          ref={positionControl}
          class={styles.positionControl}
          role="slider"
          tabIndex={0}
          aria-label="Replay position"
          aria-description="Left/Down seek back one second; Right/Up seek forward one second. Control plus arrows seeks five seconds. Shift plus Left/Right pans. Home and End seek to the start and end. Page Up and Page Down seek thirty seconds. Plus and minus zoom; zero shows the full match."
          aria-orientation="horizontal"
          aria-valuemin={0}
          aria-valuemax={props.duration / REPLAY_TICKS_PER_SECOND}
          aria-valuenow={props.playhead / REPLAY_TICKS_PER_SECOND}
          aria-valuetext={`${formatDuration(replayTickToMilliseconds(props.playhead))} of ${formatDuration(replayTickToMilliseconds(props.duration))}`}
          title="Left/Down or Right/Up: 1 second; Ctrl+arrows: 5 seconds; Home/End: start/end; PageUp/PageDown: 30 seconds; Shift+Left/Right: pan; +/- or wheel: zoom; 0: full match"
          onKeyDown={handleRailKey}
        />
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
            {(event, index) => (
              <span
                class={styles.eventAnchor}
                style={`left:${timelineRatioAtTick(
                  event.replay_tick,
                  viewport(),
                ) * 100}%`}
                data-event-anchor="true"
                data-elapsed={
                  anchorElapsed(index())
                    ? "true"
                    : undefined
                }
                aria-hidden="true"
              />
            )}
          </For>

          <For each={combatEvents()}>{item => {
            const presentation = () => timelineEventPresentation(item.source, props.localPlayerName);
            const personal = () => presentation().localRole === "killer" || presentation().localRole === "victim";
            const time = () => formatDuration(replayTickToMilliseconds(item.replay_tick));
            return <button type="button"
              classList={{ [styles.marker]: true, [styles.combatMarker]: true,
                [styles.markerLocal]: personal(), [styles.markerHighlighted]: isCombatHighlighted(item.source),
                [styles[`side${combatSide(item.source, participantIndex())}`]]: true }}
              style={`left:${timelineRatioAtTick(item.replay_tick, viewport()) * 100}%;--event-hit-width:15px`}
              data-event-icon={presentation().icon} data-combat-marker="true"
              data-side={combatSide(item.source, participantIndex())}
              data-local-role={presentation().localRole ?? undefined}
              data-local-emphasis={personal() ? "true" : undefined}
              data-highlighted={isCombatHighlighted(item.source) ? "true" : undefined}
              aria-label={`Seek to ${presentation().kind === "death" ? "Your death" : eventTitle(item.source)} at ${time()}`}
              aria-haspopup="dialog" aria-controls={combatCardId}
              aria-expanded={Boolean(hoveredCombatGroup()?.includes(item))}
              onPointerDown={event => event.stopPropagation()}
              onPointerEnter={event => openCombat(nearestCombat(event.clientX) ?? item)}
              onPointerLeave={scheduleCombatClose}
              onFocus={event => { combatFocusReturn = event.currentTarget; openCombat(item); }}
              onKeyDown={event => {
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault(); event.stopPropagation(); openCombat(item);
                  rail.querySelector<HTMLButtonElement>('[data-combat-card] button')?.focus();
                }
              }}
              onClick={event => {
                event.stopPropagation();
                selectCombat(event.detail === 0 ? item : nearestCombat(event.clientX) ?? item);
              }}>
              <span class={styles.markerGlyph}><TimelineEventGlyph icon={presentation().icon} relation={presentation().relation} /></span>
            </button>;
          }}</For>

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

        <Show when={hoveredCombatGroup()}>{group =>
          <div id={combatCardId} data-combat-card="true" class={styles.combatPopover}
            style={`left:${combatCardLeft()}px`}
            onPointerEnter={keepCombatOpen} onPointerLeave={scheduleCombatClose}
            onPointerMove={event => event.stopPropagation()} onPointerDown={event => event.stopPropagation()}
            onClick={event => event.stopPropagation()} onWheel={event => event.stopPropagation()}>
            <TimelineCombatCard events={group()} participants={participantIndex()} assetBase={props.assetBase}
              onHighlight={setHighlightedCombat} onSelect={selectCombat} />
          </div>
        }</Show>

        <Show when={hoveredTick() !== null && !hoveredCombat()}>
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
