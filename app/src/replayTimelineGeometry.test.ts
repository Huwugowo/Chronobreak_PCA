import { describe, expect, it } from "vitest";
import type { ReplayTick } from "./replayTime";
import {
  clusterTimelineEvents,
  isTickInTimelineViewport,
  panTimelineViewport,
  timelineRatioAtTick,
  timelineTickAtRatio,
  timelineViewportAroundRange,
  wholeTimelineViewport,
  zoomTimelineViewport,
} from "./replayTimelineGeometry";

const tick = (value: number) => value as ReplayTick;

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

    const narrow = clusterTimelineEvents(events, viewport, 100, 8);
    expect(narrow).toHaveLength(2);
    expect(narrow[0].kind).toBe("cluster");

    if (narrow[0].kind === "cluster") {
      expect(narrow[0].events.map((event) => event.id)).toEqual(["a", "b"]);
    }

    const wide = clusterTimelineEvents(events, viewport, 1000, 8);
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
      8,
    );
    expect(whole).toHaveLength(1);
    expect(whole[0].kind).toBe("cluster");

    const zoomed = clusterTimelineEvents(
      events,
      { start: tick(75), end: tick(175) },
      100,
      8,
    );
    expect(zoomed).toHaveLength(2);
    expect(zoomed.every((item) => item.kind === "event")).toBe(true);
  });
});