import { describe, expect, it } from "vitest";
import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "./replayTime";
import {
  clusterTimelineEvents,
  isTickInTimelineViewport,
  panTimelineViewport,
  timelineRatioAtTick,
  timelineRulerTicks,
  timelineTickAtRatio,
  timelineViewportAroundRange,
  wholeTimelineViewport,
  zoomTimelineViewport,
} from "./replayTimelineGeometry";

const tick = (value: number) => value as ReplayTick;

const fixedCollision = (
  visualWidthPx: number,
  allowedOverlapRatio = 0,
) => ({
  visualWidthOf: () => visualWidthPx,
  allowedOverlapRatio,
});

describe("replay timeline geometry", () => {
  it("maps ticks relative to the visible viewport without pinning offscreen time to an edge", () => {
    const viewport = { start: tick(200), end: tick(600) };

    expect(timelineRatioAtTick(tick(200), viewport)).toBe(0);
    expect(timelineRatioAtTick(tick(400), viewport)).toBe(0.5);
    expect(timelineRatioAtTick(tick(600), viewport)).toBe(1);
    expect(timelineRatioAtTick(tick(100), viewport)).toBe(-0.25);
    expect(timelineRatioAtTick(tick(700), viewport)).toBe(1.25);

    expect(isTickInTimelineViewport(tick(400), viewport)).toBe(true);
    expect(isTickInTimelineViewport(tick(700), viewport)).toBe(false);

    expect(timelineTickAtRatio(0.25, viewport)).toBe(tick(300));
    expect(timelineTickAtRatio(-1, viewport)).toBe(tick(200));
    expect(timelineTickAtRatio(2, viewport)).toBe(tick(600));
  });

  it("keeps the timestamp under the zoom anchor stationary", () => {
    const viewport = wholeTimelineViewport(tick(1000));
    const zoomed = zoomTimelineViewport(
      viewport,
      tick(1000),
      tick(250),
      0.5,
      tick(50),
    );

    expect(zoomed).toEqual({ start: tick(125), end: tick(625) });
    expect(timelineRatioAtTick(tick(250), zoomed)).toBe(0.25);
  });

  it("pans without changing viewport span and stops at match bounds", () => {
    const viewport = { start: tick(200), end: tick(500) };

    expect(panTimelineViewport(viewport, tick(1000), tick(100))).toEqual({
      start: tick(300),
      end: tick(600),
    });
    expect(panTimelineViewport(viewport, tick(1000), tick(-500))).toEqual({
      start: tick(0),
      end: tick(300),
    });
    expect(panTimelineViewport(viewport, tick(1000), tick(900))).toEqual({
      start: tick(700),
      end: tick(1000),
    });
  });

  it("reframes a dense event range with padding", () => {
    expect(
      timelineViewportAroundRange(
        tick(400),
        tick(500),
        tick(1000),
        tick(200),
        0.5,
      ),
    ).toEqual({
      start: tick(350),
      end: tick(550),
    });
  });

  it("clusters by screen-space density and resolves as width increases", () => {
    const events = [
      { id: "a", replay_tick: tick(100) },
      { id: "b", replay_tick: tick(150) },
      { id: "c", replay_tick: tick(300) },
    ];
    const viewport = wholeTimelineViewport(tick(1000));

    const narrow = clusterTimelineEvents(events, viewport, 100, fixedCollision(8));
    expect(narrow).toHaveLength(2);
    expect(narrow[0].kind).toBe("cluster");

    if (narrow[0].kind === "cluster") {
      expect(narrow[0].events.map((event) => event.id)).toEqual(["a", "b"]);
    }

    const wide = clusterTimelineEvents(events, viewport, 1000, fixedCollision(8));
    expect(wide).toHaveLength(3);
    expect(wide.every((item) => item.kind === "event")).toBe(true);
  });

  it("recomputes clustering from the visible time range", () => {
    const events = [
      { id: "a", replay_tick: tick(100) },
      { id: "b", replay_tick: tick(150) },
    ];

    const whole = clusterTimelineEvents(
      events,
      wholeTimelineViewport(tick(1000)),
      100,
      fixedCollision(8),
    );
    expect(whole).toHaveLength(1);
    expect(whole[0].kind).toBe("cluster");

    const zoomed = clusterTimelineEvents(
      events,
      { start: tick(75), end: tick(175) },
      100,
      fixedCollision(8),
    );
    expect(zoomed).toHaveLength(2);
    expect(zoomed.every((item) => item.kind === "event")).toBe(true);
  });

  it("preserves a priority event when the remaining cluster can fit beside it", () => {
    const events = [
      {
        id: "local-death",
        replay_tick: tick(100),
        priority: 4,
      },
      {
        id: "fight-a",
        replay_tick: tick(170),
        priority: 0,
      },
      {
        id: "fight-b",
        replay_tick: tick(190),
        priority: 0,
      },
    ];

    const clustered = clusterTimelineEvents(
      events,
      wholeTimelineViewport(tick(1000)),
      100,
      fixedCollision(8),
      (event) => event.priority,
    );

    expect(clustered).toHaveLength(2);
    expect(clustered[0].kind).toBe("event");
    expect(clustered[1].kind).toBe("cluster");

    if (clustered[0].kind === "event") {
      expect(clustered[0].event.id).toBe(
        "local-death",
      );
    }

    if (clustered[1].kind === "cluster") {
      expect(
        clustered[1].events.map(
          (event) => event.id,
        ),
      ).toEqual(["fight-a", "fight-b"]);
    }
  });

  it("does not force a priority event out of a cluster when there is no room", () => {
    const events = [
      {
        id: "local-death",
        replay_tick: tick(100),
        priority: 4,
      },
      {
        id: "nearby-kill",
        replay_tick: tick(150),
        priority: 0,
      },
    ];

    const clustered = clusterTimelineEvents(
      events,
      wholeTimelineViewport(tick(1000)),
      100,
      fixedCollision(8),
      (event) => event.priority,
    );

    expect(clustered).toHaveLength(1);
    expect(clustered[0].kind).toBe(
      "cluster",
    );
  });
  it("does not transitively chain evenly spaced events into one mega-cluster", () => {
    const events = [
      {
        id: "a",
        replay_tick: tick(100),
      },
      {
        id: "b",
        replay_tick: tick(130),
      },
      {
        id: "c",
        replay_tick: tick(160),
      },
      {
        id: "d",
        replay_tick: tick(190),
      },
    ];

    const clustered =
      clusterTimelineEvents(
        events,
        wholeTimelineViewport(
          tick(1000),
        ),
        1000,
        fixedCollision(36),
      );

    expect(clustered).toHaveLength(2);

    expect(
      clustered.map((item) =>
        item.kind === "cluster"
          ? item.events.length
          : 1,
      ),
    ).toEqual([2, 2]);

    expect(
      clustered.every(
        (item) => item.kind === "cluster",
      ),
    ).toBe(true);
  });
  it("allows up to the configured fraction of visual overlap", () => {
    const viewport = wholeTimelineViewport(tick(1000));

    const atLimit = clusterTimelineEvents(
      [
        { id: "a", replay_tick: tick(100) },
        { id: "b", replay_tick: tick(113) },
      ],
      viewport,
      1000,
      fixedCollision(20, 0.35),
    );

    expect(atLimit).toHaveLength(2);
    expect(
      atLimit.every((item) => item.kind === "event"),
    ).toBe(true);

    const beyondLimit = clusterTimelineEvents(
      [
        { id: "a", replay_tick: tick(100) },
        { id: "b", replay_tick: tick(112) },
      ],
      viewport,
      1000,
      fixedCollision(20, 0.35),
    );

    expect(beyondLimit).toHaveLength(1);
    expect(beyondLimit[0].kind).toBe("cluster");
  });

  it("automatically adapts clustering when rendered icon size changes", () => {
    const events = [
      { id: "a", replay_tick: tick(100) },
      { id: "b", replay_tick: tick(110) },
    ];
    const viewport = wholeTimelineViewport(tick(1000));

    const smallIcons = clusterTimelineEvents(
      events,
      viewport,
      1000,
      fixedCollision(12, 0.35),
    );

    expect(
      smallIcons.every((item) => item.kind === "event"),
    ).toBe(true);

    const largeIcons = clusterTimelineEvents(
      events,
      viewport,
      1000,
      fixedCollision(20, 0.35),
    );

    expect(largeIcons).toHaveLength(1);
    expect(largeIcons[0].kind).toBe("cluster");
  });
  it("chooses ruler intervals from visible time density", () => {
    const fullMatch = timelineRulerTicks(
      {
        start: tick(0),
        end: tick(30 * 60 * REPLAY_TICKS_PER_SECOND),
      },
      1_800,
    );

    expect(fullMatch.length).toBeGreaterThan(2);
    expect(fullMatch[1] - fullMatch[0]).toBe(
      120 * REPLAY_TICKS_PER_SECOND,
    );

    const precise = timelineRulerTicks(
      {
        start: tick(100 * REPLAY_TICKS_PER_SECOND),
        end: tick(105 * REPLAY_TICKS_PER_SECOND),
      },
      1_000,
    );

    expect(precise.length).toBeGreaterThan(2);
    expect(precise[1] - precise[0]).toBe(
      0.5 * REPLAY_TICKS_PER_SECOND,
    );
  });

  it("does not produce ruler ticks without usable screen geometry", () => {
    expect(
      timelineRulerTicks(
        {
          start: tick(0),
          end: tick(60 * REPLAY_TICKS_PER_SECOND),
        },
        0,
      ),
    ).toEqual([]);
  });
});