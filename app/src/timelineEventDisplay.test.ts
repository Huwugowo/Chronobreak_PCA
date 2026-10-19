import { describe, expect, it } from "vitest";
import {
  REPLAY_TICKS_PER_SECOND,
  parseMediaId,
  type ReplayTick,
} from "./replayTime";
import type { ViewerEvent } from "./types";
import type { EventActors, SourceEpoch } from "./leagueSource";
import {
  timelineDisplayEvents,
  type MappedTimelineEvent,
} from "./timelineEventDisplay";

const tick = (seconds: number): ReplayTick =>
  Math.round(
    seconds * REPLAY_TICKS_PER_SECOND,
  ) as ReplayTick;

const source = (overrides: Partial<SourceEpoch> = {}): SourceEpoch => ({
  media_id: parseMediaId("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"),
  run_id: parseMediaId("11111111-1111-1111-1111-111111111111"),
  epoch: 1,
  ...overrides,
});

const resolved = (participant: number): EventActors["killer"] => [
  { participant, status: "resolved" },
];

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
  stolen: null,
  recipient: null,
  kill_streak: null,
  acer: null,
  acing_team: null,
  turret: null,
  inhibitor: null,
  result: null,
  relation: "neutral",
  source: source(),
  local_participant: 1,
  actors: {},
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
        actors: { killer: resolved(1), victim: resolved(2) },
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
        actors: { killer: resolved(1), victim: resolved(2) },
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
        actors: { killer: resolved(1), victim: resolved(2) },
      },
    );

    const multikill = event(
      "Multikill",
      104,
      {
        killer: "Me#TEST",
        kill_streak: 3,
        relation: "ally",
        actors: { killer: resolved(1) },
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
        actors: { killer: resolved(1), victim: resolved(2) },
      },
    );

    const ace = event(
      "Ace",
      102,
      {
        acer: "Me",
        relation: "ally",
        actors: { acer: resolved(1) },
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
        actors: { killer: resolved(1) },
      },
    );

    const turret = event(
      "TurretKilled",
      100,
      {
        killer: "Me",
        turret: "T1",
        relation: "ally",
        actors: { killer: resolved(1) },
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
        actors: { killer: resolved(1), victim: resolved(2) },
      },
    );

    const multikill = event(
      "Multikill",
      110,
      {
        killer: "Me",
        kill_streak: 2,
        relation: "ally",
        actors: { killer: resolved(1) },
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

  it("does not merge same base name actors with distinct participant identities", () => {
    const kill = event("ChampionKill", 100, {
      killer: "Me#EUW",
      victim: "Enemy",
      actors: { killer: resolved(1), victim: resolved(2) },
    });
    const multikill = event("Multikill", 104, {
      killer: "Me#TEST",
      kill_streak: 3,
      actors: { killer: resolved(3) },
    });

    const display = timelineDisplayEvents([kill, multikill]);

    expect(display.map(({ source }) => source)).toEqual([kill, multikill]);
  });

  it.each([
    ["media", source({ media_id: parseMediaId("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb") })],
    ["run", source({ run_id: parseMediaId("22222222-2222-2222-2222-222222222222") })],
    ["epoch", source({ epoch: 2 })],
  ])("does not merge a multikill from a different %s source", (_scope, otherSource) => {
    const kill = event("ChampionKill", 100, {
      actors: { killer: resolved(1), victim: resolved(2) },
    });
    const multikill = event("Multikill", 104, {
      kill_streak: 3,
      source: otherSource,
      actors: { killer: resolved(1) },
    });

    expect(timelineDisplayEvents([kill, multikill]).map(({ source }) => source)).toEqual([kill, multikill]);
  });

  it("does not merge unresolved actor telemetry", () => {
    const kill = event("ChampionKill", 100, {
      actors: { killer: resolved(1), victim: resolved(2) },
    });
    const multikill = event("Multikill", 104, {
      kill_streak: 3,
      actors: { killer: [{ participant: null, status: "unresolved" }] },
    });

    expect(timelineDisplayEvents([kill, multikill]).map(({ source }) => source)).toEqual([kill, multikill]);
  });
});
