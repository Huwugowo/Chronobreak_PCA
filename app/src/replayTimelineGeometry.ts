import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "./replayTime";

export type TimelineViewport = {
  start: ReplayTick;
  end: ReplayTick;
};

export type TimelineCluster<T extends { replay_tick: ReplayTick }> =
  | {
      kind: "event";
      event: T;
      x: number;
    }
  | {
      kind: "cluster";
      events: readonly T[];
      start: ReplayTick;
      end: ReplayTick;
      x: number;
    };

const clampNumber = (value: number, minimum: number, maximum: number): number =>
  Math.min(maximum, Math.max(minimum, value));

const asReplayTick = (value: number): ReplayTick => Math.round(value) as ReplayTick;

export const wholeTimelineViewport = (duration: ReplayTick): TimelineViewport => ({
  start: 0 as ReplayTick,
  end: Math.max(0, duration) as ReplayTick,
});

export const timelineViewportSpan = (viewport: TimelineViewport): ReplayTick =>
  Math.max(0, viewport.end - viewport.start) as ReplayTick;

const TIMELINE_RULER_INTERVAL_SECONDS = [
  0.1,
  0.2,
  0.5,
  1,
  2,
  5,
  10,
  15,
  30,
  60,
  120,
  180,
  300,
  600,
  900,
  1_800,
  3_600,
] as const;

export const timelineRulerTicks = (
  viewport: TimelineViewport,
  widthPx: number,
  targetSpacingPx = 100,
): readonly ReplayTick[] => {
  const span = timelineViewportSpan(viewport);

  if (
    span <= 0 ||
    !Number.isFinite(widthPx) ||
    widthPx <= 0 ||
    !Number.isFinite(targetSpacingPx) ||
    targetSpacingPx <= 0
  ) {
    return [];
  }

  const desiredIntervalTicks = (span * targetSpacingPx) / widthPx;

  const intervalTicks =
    TIMELINE_RULER_INTERVAL_SECONDS
      .map((seconds) =>
        Math.max(1, Math.round(seconds * REPLAY_TICKS_PER_SECOND)),
      )
      .find((candidate) => candidate >= desiredIntervalTicks) ??
    Math.max(
      1,
      Math.round(
        TIMELINE_RULER_INTERVAL_SECONDS[
          TIMELINE_RULER_INTERVAL_SECONDS.length - 1
        ] * REPLAY_TICKS_PER_SECOND,
      ),
    );

  const firstTick =
    Math.ceil(viewport.start / intervalTicks) * intervalTicks;

  const ticks: ReplayTick[] = [];

  for (
    let tick = firstTick;
    tick <= viewport.end && ticks.length < 200;
    tick += intervalTicks
  ) {
    ticks.push(Math.round(tick) as ReplayTick);
  }

  return ticks;
};

export const normalizeTimelineViewport = (
  viewport: TimelineViewport,
  duration: ReplayTick,
): TimelineViewport => {
  const boundedDuration = Math.max(0, duration);
  if (boundedDuration === 0) return wholeTimelineViewport(0 as ReplayTick);

  const rawStart = Math.min(viewport.start, viewport.end);
  const rawEnd = Math.max(viewport.start, viewport.end);
  const span = clampNumber(rawEnd - rawStart, 1, boundedDuration);
  const start = clampNumber(rawStart, 0, boundedDuration - span);

  return {
    start: asReplayTick(start),
    end: asReplayTick(start + span),
  };
};

/**
 * Maps a replay tick into the current viewport without clamping.
 *
 * Values below 0 or above 1 deliberately mean that the timestamp is outside
 * the visible viewport. Rendering code must decide whether to hide it or show
 * an offscreen indication rather than pinning it to an edge.
 */
export const timelineRatioAtTick = (
  tick: ReplayTick,
  viewport: TimelineViewport,
): number => {
  const span = timelineViewportSpan(viewport);
  if (span <= 0) return 0;
  return (tick - viewport.start) / span;
};

export const isTickInTimelineViewport = (
  tick: ReplayTick,
  viewport: TimelineViewport,
): boolean => tick >= viewport.start && tick <= viewport.end;

/**
 * Pointer coordinates are constrained to the physical timeline, so ratio
 * input is intentionally clamped to the current visible range.
 */
export const timelineTickAtRatio = (
  ratio: number,
  viewport: TimelineViewport,
): ReplayTick =>
  asReplayTick(
    viewport.start +
      clampNumber(ratio, 0, 1) * timelineViewportSpan(viewport),
  );

export const zoomTimelineViewport = (
  viewport: TimelineViewport,
  duration: ReplayTick,
  anchor: ReplayTick,
  scale: number,
  minimumSpan: ReplayTick,
): TimelineViewport => {
  const current = normalizeTimelineViewport(viewport, duration);
  const currentSpan = timelineViewportSpan(current);

  if (duration <= 0 || currentSpan <= 0) return current;

  const safeScale = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const boundedMinimumSpan = clampNumber(minimumSpan, 1, duration);
  const nextSpan = clampNumber(
    currentSpan * safeScale,
    boundedMinimumSpan,
    duration,
  );

  const boundedAnchor = clampNumber(anchor, current.start, current.end);
  const anchorRatio = (boundedAnchor - current.start) / currentSpan;

  let start = boundedAnchor - anchorRatio * nextSpan;
  let end = start + nextSpan;

  if (start < 0) {
    end -= start;
    start = 0;
  }

  if (end > duration) {
    start -= end - duration;
    end = duration;
  }

  return normalizeTimelineViewport(
    {
      start: asReplayTick(start),
      end: asReplayTick(end),
    },
    duration,
  );
};

export const panTimelineViewport = (
  viewport: TimelineViewport,
  duration: ReplayTick,
  delta: ReplayTick,
): TimelineViewport => {
  const current = normalizeTimelineViewport(viewport, duration);
  const span = timelineViewportSpan(current);

  if (duration <= 0 || span <= 0) return current;

  const start = clampNumber(current.start + delta, 0, duration - span);

  return {
    start: asReplayTick(start),
    end: asReplayTick(start + span),
  };
};

export const timelineViewportAroundRange = (
  start: ReplayTick,
  end: ReplayTick,
  duration: ReplayTick,
  minimumSpan: ReplayTick,
  paddingRatio = 0.5,
): TimelineViewport => {
  if (duration <= 0) return wholeTimelineViewport(0 as ReplayTick);

  const rangeStart = Math.min(start, end);
  const rangeEnd = Math.max(start, end);
  const rangeSpan = Math.max(1, rangeEnd - rangeStart);
  const safePaddingRatio = Math.max(0, paddingRatio);
  const paddedSpan = Math.max(
    Math.max(1, minimumSpan),
    rangeSpan * (1 + 2 * safePaddingRatio),
  );
  const center = (rangeStart + rangeEnd) / 2;
  const targetSpan = Math.min(duration, paddedSpan);

  return normalizeTimelineViewport(
    {
      start: asReplayTick(center - targetSpan / 2),
      end: asReplayTick(center + targetSpan / 2),
    },
    duration,
  );
};

export type TimelineEventCollisionPolicy<T> = {
  visualWidthOf: (event: T) => number;
  allowedOverlapRatio: number;
};

export const clusterTimelineEvents = <T extends { replay_tick: ReplayTick }>(
  events: readonly T[],
  viewport: TimelineViewport,
  widthPx: number,
  collision: TimelineEventCollisionPolicy<T>,
  priorityOf: (event: T) => number = () => 0,
): readonly TimelineCluster<T>[] => {
  if (widthPx <= 0 || timelineViewportSpan(viewport) <= 0) return [];

  const rawOverlapRatio = collision.allowedOverlapRatio;
  const allowedOverlapRatio = Number.isFinite(rawOverlapRatio)
    ? clampNumber(rawOverlapRatio, 0, 1)
    : 0;

  const visualWidthOf = (event: T): number => {
    const rawWidth = collision.visualWidthOf(event);

    return Number.isFinite(rawWidth)
      ? Math.max(1, rawWidth)
      : 1;
  };

  const minimumCenterDistance = (
    leftWidth: number,
    rightWidth: number,
  ): number =>
    Math.max(
      0,
      leftWidth / 2 +
        rightWidth / 2 -
        Math.min(leftWidth, rightWidth) *
          allowedOverlapRatio,
    );

  const visible = events
    .filter((event) =>
      isTickInTimelineViewport(event.replay_tick, viewport),
    )
    .map((event) => ({
      event,
      x:
        timelineRatioAtTick(
          event.replay_tick,
          viewport,
        ) * widthPx,
      visualWidth: visualWidthOf(event),
    }))
    .sort(
      (left, right) =>
        left.event.replay_tick -
        right.event.replay_tick,
    );

  const groups: Array<Array<(typeof visible)[number]>> = [];

  const groupVisualWidth = (
    group: Array<(typeof visible)[number]>,
  ): number =>
    group.reduce(
      (maximum, item) =>
        Math.max(maximum, item.visualWidth),
      1,
    );

  for (const item of visible) {
    const group = groups[groups.length - 1];

    if (!group || group.length === 0) {
      groups.push([item]);
      continue;
    }

    /*
     * Compare with the position where the current group is
     * represented, rather than chaining from the latest event.
     *
     * This preserves the anti-mega-chain behavior while making
     * collision dependent on actual visual footprints.
     */
    const representativeX =
      (group[0].x + group[group.length - 1].x) / 2;

    const requiredDistance = minimumCenterDistance(
      groupVisualWidth(group),
      item.visualWidth,
    );

    if (item.x - representativeX >= requiredDistance) {
      groups.push([item]);
    } else {
      group.push(item);
    }
  }

  const eventItem = (
    item: (typeof visible)[number],
  ): TimelineCluster<T> => ({
    kind: "event",
    event: item.event,
    x: item.x,
  });

  const clusterItem = (
    group: Array<(typeof visible)[number]>,
  ): TimelineCluster<T> => {
    if (group.length === 1) {
      return eventItem(group[0]);
    }

    const first = group[0];
    const last = group[group.length - 1];

    return {
      kind: "cluster",
      events: group.map((item) => item.event),
      start: first.event.replay_tick,
      end: last.event.replay_tick,
      x: (first.x + last.x) / 2,
    };
  };

  const result: TimelineCluster<T>[] = [];

  for (const group of groups) {
    if (group.length === 1) {
      result.push(eventItem(group[0]));
      continue;
    }

    let protectedItem:
      | (typeof visible)[number]
      | undefined;
    let protectedPriority = 0;

    for (const item of group) {
      const rawPriority = priorityOf(item.event);
      const priority = Number.isFinite(rawPriority)
        ? Math.max(0, rawPriority)
        : 0;

      if (priority > protectedPriority) {
        protectedPriority = priority;
        protectedItem = item;
      }
    }

    if (protectedItem && protectedPriority > 0) {
      const remainder = group.filter(
        (item) => item !== protectedItem,
      );

      const remainderItem = clusterItem(remainder);

      const requiredDistance = minimumCenterDistance(
        protectedItem.visualWidth,
        groupVisualWidth(remainder),
      );

      if (
        Math.abs(
          remainderItem.x - protectedItem.x,
        ) >= requiredDistance
      ) {
        result.push(
          eventItem(protectedItem),
          remainderItem,
        );
        continue;
      }
    }

    result.push(clusterItem(group));
  }

  return result.sort(
    (left, right) => left.x - right.x,
  );
};