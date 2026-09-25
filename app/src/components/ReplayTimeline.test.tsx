// @vitest-environment jsdom
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { render } from "solid-js/web";
import {
  REPLAY_TICKS_PER_SECOND,
  type ReplayTick,
} from "../replayTime";
import type { ViewerEvent } from "../types";
import {
  wholeTimelineViewport,
  type TimelineViewport,
} from "../replayTimelineGeometry";
import ReplayTimeline from "./ReplayTimeline";

type TimelineEvent =
  ViewerEvent & { replay_tick: ReplayTick };

const disposers: Array<() => void> = [];

const tick = (seconds: number): ReplayTick =>
  Math.round(
    seconds * REPLAY_TICKS_PER_SECOND,
  ) as ReplayTick;

const event = (
  eventType: string,
  seconds: number,
  overrides: Partial<ViewerEvent> = {},
): TimelineEvent => ({
  event_type: eventType,
  game_tick: "0",
  replay_tick: tick(seconds),
  killer: null,
  victim: null,
  assisters: [],
  dragon_type: null,
  kill_streak: null,
  acer: null,
  acing_team: null,
  turret: null,
  inhibitor: null,
  result: null,
  relation: "neutral",
  ...overrides,
});

const renderTimeline = (
  events: readonly TimelineEvent[],
  options: {
    localPlayerName?: string | null;
    onViewportChange?: (
      viewport: TimelineViewport,
    ) => void;
  } = {},
) => {
  const host =
    document.createElement("div");

  document.body.append(host);

  const duration = tick(100);
  const viewport =
    wholeTimelineViewport(duration);

  const onViewportChange =
    options.onViewportChange ??
    vi.fn();

  const dispose = render(
    () => (
      <ReplayTimeline
        duration={duration}
        playhead={tick(50)}
        viewport={viewport}
        onViewportChange={onViewportChange}
        events={events}
        localPlayerName={
          options.localPlayerName ??
          "Me#EUW"
        }
        clip={null}
        onSeek={() => {}}
        onEventSelect={() => {}}
        onClipEndpointEditStart={() => {}}
        onClipEndpointPreview={() => {}}
        onClipEndpointEditFinish={() => {}}
        onClipEndpointKeyDown={() => {}}
        onClipEndpointKeyUp={() => {}}
        onClipEndpointBlur={() => {}}
      />
    ),
    host,
  );

  disposers.push(dispose);

  return {
    host,
    duration,
    onViewportChange,
  };
};

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );

  vi.spyOn(
    HTMLElement.prototype,
    "getBoundingClientRect",
  ).mockImplementation(
    () => new DOMRect(0, 0, 1000, 54),
  );
});

afterEach(() => {
  while (disposers.length > 0) {
    disposers.pop()!();
  }

  document.body.replaceChildren();

  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ReplayTimeline event language", () => {
  it("renders kills as swords and the local player's death as a skull", () => {
    const { host } = renderTimeline([
      event("ChampionKill", 20, {
        killer: "Enemy Carry",
        victim: "Me#EUW",
        relation: "enemy",
      }),
      event("ChampionKill", 60, {
        killer: "Me#EUW",
        victim: "Enemy Mid",
        relation: "ally",
      }),
    ]);

    const skull = host.querySelector(
      '[data-event-icon="skull"]',
    );

    const swords = host.querySelector(
      '[data-event-icon="swords"]',
    );

    expect(skull).not.toBeNull();
    expect(swords).not.toBeNull();

    expect(
      skull?.getAttribute(
        "data-local-role",
      ),
    ).toBe("victim");

    expect(
      skull?.getAttribute(
        "data-local-emphasis",
      ),
    ).toBe("true");

    expect(
      swords?.getAttribute(
        "data-local-role",
      ),
    ).toBe("killer");

    expect(
      swords?.getAttribute(
        "data-local-emphasis",
      ),
    ).toBe("true");
  });

  it("attaches Multikill count to the underlying kill marker", () => {
    const { host } = renderTimeline([
      event("ChampionKill", 20, {
        killer: "Me#EUW",
        victim: "Enemy Carry",
        relation: "ally",
      }),
      event("Multikill", 24, {
        killer: "Me#TEST",
        kill_streak: 3,
        relation: "ally",
      }),
    ]);

    const swords = host.querySelector(
      '[data-event-icon="swords"]',
    );

    expect(swords).not.toBeNull();
    expect(
      host.querySelectorAll(
        "[data-event-icon]",
      ),
    ).toHaveLength(1);

    expect(swords?.textContent).toContain(
      "3",
    );
  });

  it("renders elemental dragon artwork from the vendored Riot asset set", () => {
    const { host } = renderTimeline([
      event("DragonKill", 20, {
        killer: "Ally Jungle",
        dragon_type: "Fire",
        relation: "ally",
      }),
    ]);

    const marker =
      host.querySelector(
        '[data-event-icon="dragon-fire"]',
      );

    expect(marker).not.toBeNull();

    const image =
      marker?.querySelector("img");

    expect(image).not.toBeNull();

    expect(
      image?.getAttribute("src"),
    ).toBe(
      "/timeline-events/dragon-fire.png",
    );
  });

  it("uses the red tower artwork for enemy-related tower events", () => {
    const { host } = renderTimeline([
      event("TurretKilled", 20, {
        killer: "Enemy Carry",
        turret: "Turret_T1_C_05_A",
        relation: "enemy",
      }),
    ]);

    const marker =
      host.querySelector(
        '[data-event-icon="tower"]',
      );

    expect(marker).not.toBeNull();

    const image =
      marker?.querySelector("img");

    expect(image).not.toBeNull();

    expect(
      image?.getAttribute("src"),
    ).toBe(
      "/timeline-events/tower-red.png",
    );
  });
  it("renders a homogeneous collision as semantic icon plus count", () => {
    const onViewportChange = vi.fn();

    const {
      host,
      duration,
    } = renderTimeline(
      [
        event("ChampionKill", 20, {
          killer: "Ally Mid",
          victim: "Enemy Mid",
          relation: "ally",
        }),
        event("ChampionKill", 20, {
          killer: "Ally Jungle",
          victim: "Enemy Jungle",
          relation: "ally",
        }),
      ],
      { onViewportChange },
    );

    const cluster =
      host.querySelector<HTMLButtonElement>(
        '[data-cluster-kind="homogeneous"]',
      );

    expect(cluster).not.toBeNull();

    expect(
      cluster?.getAttribute(
        "data-cluster-icon",
      ),
    ).toBe("swords");

    expect(cluster?.textContent).toContain(
      "2",
    );

    cluster!.click();

    expect(
      onViewportChange,
    ).toHaveBeenCalledTimes(1);

    const next =
      onViewportChange.mock.calls[0][0];

    expect(next.start).toBeLessThanOrEqual(
      tick(20),
    );

    expect(next.end).toBeGreaterThanOrEqual(
      tick(20),
    );

    expect(
      next.end - next.start,
    ).toBeLessThan(duration);
  });

  it("uses small diagonal precision-wheel movement as smooth zoom", () => {
    const onViewportChange = vi.fn();

    const {
      host,
      duration,
    } = renderTimeline(
      [],
      { onViewportChange },
    );

    const rail =
      host.querySelector<HTMLElement>(
        '[data-testid="replay-timeline"]',
      )!;

    rail.dispatchEvent(
      new WheelEvent(
        "wheel",
        {
          bubbles: true,
          cancelable: true,
          clientX: 500,
          deltaX: 4,
          deltaY: -5,
        },
      ),
    );

    expect(
      onViewportChange,
    ).toHaveBeenCalledTimes(1);

    const next =
      onViewportChange.mock.calls[0][0];

    const nextSpan =
      next.end - next.start;

    expect(nextSpan).toBeLessThan(
      duration,
    );

    /*
     * A tiny touchpad delta should produce a tiny zoom,
     * not the old fixed 20% jump.
     */
    expect(nextSpan).toBeGreaterThan(
      duration * 0.95,
    );
  });

  it("keeps ctrl-wheel trackpad pinch in zoom mode even with horizontal noise", () => {
    const onViewportChange = vi.fn();

    const {
      host,
      duration,
    } = renderTimeline(
      [],
      { onViewportChange },
    );

    const rail =
      host.querySelector<HTMLElement>(
        '[data-testid="replay-timeline"]',
      )!;

    rail.dispatchEvent(
      new WheelEvent(
        "wheel",
        {
          bubbles: true,
          cancelable: true,
          clientX: 500,
          ctrlKey: true,
          deltaX: 20,
          deltaY: -4,
        },
      ),
    );

    expect(
      onViewportChange,
    ).toHaveBeenCalledTimes(1);

    const next =
      onViewportChange.mock.calls[0][0];

    expect(
      next.end - next.start,
    ).toBeLessThan(duration);
  });
  it("renders a mixed collision as a neutral +N cluster", () => {
    const { host } = renderTimeline([
      event("ChampionKill", 20, {
        killer: "Ally Mid",
        victim: "Enemy Mid",
        relation: "ally",
      }),
      event("DragonKill", 20, {
        killer: "Ally Jungle",
        dragon_type: "Fire",
        relation: "ally",
      }),
    ]);

    const cluster =
      host.querySelector(
        '[data-cluster-kind="mixed"]',
      );

    expect(cluster).not.toBeNull();

    expect(
      cluster?.getAttribute(
        "data-cluster-icon",
      ),
    ).toBeNull();

    expect(cluster?.textContent?.trim()).toBe(
      "+2",
    );
  });
});
