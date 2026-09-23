import type { ReplayTick } from "./replayTime";

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

export const clusterTimelineEvents = <T extends { replay_tick: ReplayTick }>(
  events: readonly T[],
  viewport: TimelineViewport,
  widthPx: number,
  minimumGapPx: number,
): readonly TimelineCluster<T>[] => {
  if (widthPx <= 0 || timelineViewportSpan(viewport) <= 0) return [];

  const gapPx = Math.max(0, minimumGapPx);
  const visible = events
    .filter((event) => isTickInTimelineViewport(event.replay_tick, viewport))
    .map((event) => ({
      event,
      x: timelineRatioAtTick(event.replay_tick, viewport) * widthPx,
    }))
    .sort((left, right) => left.event.replay_tick - right.event.replay_tick);

  const groups: Array<Array<(typeof visible)[number]>> = [];

  for (const item of visible) {
    const group = groups[groups.length - 1];
    const previous = group?.[group.length - 1];

    if (!group || !previous || item.x - previous.x >= gapPx) {
      groups.push([item]);
    } else {
      group.push(item);
    }
  }

  return groups.map((group) => {
    if (group.length === 1) {
      return {
        kind: "event" as const,
        event: group[0].event,
        x: group[0].x,
      };
    }

    const first = group[0];
    const last = group[group.length - 1];

    return {
      kind: "cluster" as const,
      events: group.map((item) => item.event),
      start: first.event.replay_tick,
      end: last.event.replay_tick,
      x: (first.x + last.x) / 2,
    };
  });
};