import { describe, expect, it } from "vitest";
import {
  REPLAY_TICKS_PER_SECOND,
  type ReplayTick,
} from "./replayTime";
import type { ViewerEvent } from "./types";
import {
  timelineDisplayEvents,
  type MappedTimelineEvent,
} from "./timelineEventDisplay";

const tick = (seconds: number): ReplayTick =>
  Math.round(
    seconds * REPLAY_TICKS_PER_SECOND,
  ) as ReplayTick;

const event = (
  eventType: string,
  seconds: number,
  overrides: Partial<ViewerEvent> = {},
): MappedTimelineEvent => ({
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

describe("timeline display events", () => {
  it("collapses FirstBlood into its matching ChampionKill", () => {
    const firstBlood = event(
      "FirstBlood",
      100,
      {
        killer: "Me#EUW",
        victim: "Enemy",
        relation: "ally",
      },
    );

    const championKill = event(
      "ChampionKill",
      100,
      {
        killer: "Me#EUW",
        victim: "Enemy",
        assisters: ["Ally"],
        relation: "ally",
      },
    );

    const display = timelineDisplayEvents([
      firstBlood,
      championKill,
    ]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(
      championKill,
    );
  });

  it("keeps standalone FirstBlood when no ChampionKill exists", () => {
    const firstBlood = event(
      "FirstBlood",
      100,
      {
        killer: "Me",
        victim: "Enemy",
        relation: "ally",
      },
    );

    const display =
      timelineDisplayEvents([firstBlood]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(
      firstBlood,
    );
  });

  it("attaches Multikill multiplicity to the preceding kill instead of adding another marker", () => {
    const kill = event(
      "ChampionKill",
      100,
      {
        killer: "Me#EUW",
        victim: "Enemy",
        relation: "ally",
      },
    );

    const multikill = event(
      "Multikill",
      104,
      {
        killer: "Me#TEST",
        kill_streak: 3,
        relation: "ally",
      },
    );

    const display = timelineDisplayEvents([
      kill,
      multikill,
    ]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(kill);
    expect(display[0].multiplier).toBe(3);
  });

  it("keeps standalone Multikill telemetry when no base kill is available", () => {
    const multikill = event(
      "Multikill",
      100,
      {
        killer: "Me",
        kill_streak: 2,
        relation: "ally",
      },
    );

    const display =
      timelineDisplayEvents([multikill]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(
      multikill,
    );
    expect(display[0].multiplier).toBe(2);
  });

  it("suppresses an Ace already represented by its nearby team fight kill", () => {
    const kill = event(
      "ChampionKill",
      100,
      {
        killer: "Me",
        victim: "Enemy",
        relation: "ally",
      },
    );

    const ace = event(
      "Ace",
      102,
      {
        acer: "Me",
        relation: "ally",
      },
    );

    const display = timelineDisplayEvents([
      kill,
      ace,
    ]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(kill);
  });

  it("keeps an Ace when no nearby kill can represent it", () => {
    const ace = event(
      "Ace",
      100,
      {
        acer: "Me",
        relation: "ally",
      },
    );

    const display =
      timelineDisplayEvents([ace]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(ace);
  });

  it("collapses FirstBrick into the matching TurretKilled event", () => {
    const firstBrick = event(
      "FirstBrick",
      100,
      {
        killer: "Me",
        turret: "T1",
        relation: "ally",
      },
    );

    const turret = event(
      "TurretKilled",
      100,
      {
        killer: "Me",
        turret: "T1",
        relation: "ally",
      },
    );

    const display = timelineDisplayEvents([
      firstBrick,
      turret,
    ]);

    expect(display).toHaveLength(1);
    expect(display[0].source).toBe(turret);
  });

  it("does not associate a distant Multikill with an unrelated old kill", () => {
    const kill = event(
      "ChampionKill",
      100,
      {
        killer: "Me",
        victim: "Enemy",
        relation: "ally",
      },
    );

    const multikill = event(
      "Multikill",
      110,
      {
        killer: "Me",
        kill_streak: 2,
        relation: "ally",
      },
    );

    const display = timelineDisplayEvents([
      kill,
      multikill,
    ]);

    expect(display).toHaveLength(2);
    expect(display[0].source).toBe(kill);
    expect(display[1].source).toBe(
      multikill,
    );
  });
});
