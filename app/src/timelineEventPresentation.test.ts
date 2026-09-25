import { describe, expect, it } from "vitest";
import type { ViewerEvent } from "./types";
import {
  dragonTimelineIcon,
  highestPriorityTimelineEvent,
  timelineClusterPresentation,
  timelineEventPresentation,
  timelineEventsAreHomogeneous,
} from "./timelineEventPresentation";

const event = (
  eventType: string,
  overrides: Partial<ViewerEvent> = {},
): ViewerEvent => ({
  event_type: eventType,
  game_tick: "0",
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

describe("timeline event presentation", () => {
  it("uses swords for champion kills and skull for the local player's death", () => {
    const ownKill = timelineEventPresentation(
      event("ChampionKill", {
        killer: "Me#EUW",
        victim: "Enemy Mid#EUW",
        relation: "ally",
      }),
      "Me#TEST",
    );

    expect(ownKill.kind).toBe("kill");
    expect(ownKill.icon).toBe("swords");
    expect(ownKill.localRole).toBe("killer");
    expect(ownKill.localPriority).toBe(3);

    const ownDeath = timelineEventPresentation(
      event("ChampionKill", {
        killer: "Enemy Mid#EUW",
        victim: "Me#EUW",
        relation: "enemy",
      }),
      "Me#TEST",
    );

    expect(ownDeath.kind).toBe("death");
    expect(ownDeath.icon).toBe("skull");
    expect(ownDeath.localRole).toBe("victim");
    expect(ownDeath.localPriority).toBe(4);
  });

  it("keeps ordinary kills as swords while preserving assists as local relevance", () => {
    const assistedKill = timelineEventPresentation(
      event("ChampionKill", {
        killer: "Ally Jungle",
        victim: "Enemy Jungle",
        assisters: ["Me#EUW"],
        relation: "ally",
      }),
      "Me#TEST",
    );

    expect(assistedKill.icon).toBe("swords");
    expect(assistedKill.localRole).toBe("assister");
    expect(assistedKill.localPriority).toBe(0);

    const unrelatedKill = timelineEventPresentation(
      event("ChampionKill", {
        killer: "Enemy Top",
        victim: "Ally Top",
        relation: "enemy",
      }),
      "Me#TEST",
    );

    expect(unrelatedKill.icon).toBe("swords");
    expect(unrelatedKill.localRole).toBeNull();
    expect(unrelatedKill.localPriority).toBe(0);
  });

  it("maps League objectives to stable semantic icon keys", () => {
    expect(
      timelineEventPresentation(
        event("TurretKilled"),
        null,
      ).icon,
    ).toBe("tower");

    expect(
      timelineEventPresentation(
        event("FirstBrick"),
        null,
      ).icon,
    ).toBe("tower");

    expect(
      timelineEventPresentation(
        event("InhibKilled"),
        null,
      ).icon,
    ).toBe("inhibitor");

    expect(
      timelineEventPresentation(
        event("BaronKill"),
        null,
      ).icon,
    ).toBe("baron");

    expect(
      timelineEventPresentation(
        event("HeraldKill"),
        null,
      ).icon,
    ).toBe("herald");

    expect(
      timelineEventPresentation(
        event("HordeKill"),
        null,
      ).icon,
    ).toBe("void-grubs");
  });

  it("normalizes common dragon names into stable Riot asset identities", () => {
    expect(dragonTimelineIcon("Air")).toBe("dragon-air");
    expect(dragonTimelineIcon("Cloud")).toBe("dragon-air");

    expect(dragonTimelineIcon("Earth")).toBe("dragon-earth");
    expect(dragonTimelineIcon("Mountain")).toBe("dragon-earth");

    expect(dragonTimelineIcon("Fire")).toBe("dragon-fire");
    expect(dragonTimelineIcon("Infernal")).toBe("dragon-fire");

    expect(dragonTimelineIcon("Water")).toBe("dragon-water");
    expect(dragonTimelineIcon("Ocean")).toBe("dragon-water");

    expect(dragonTimelineIcon("Hextech")).toBe("dragon-hextech");
    expect(dragonTimelineIcon("Chemtech")).toBe("dragon-chemtech");
    expect(dragonTimelineIcon("Elder")).toBe("dragon-elder");

    expect(dragonTimelineIcon(null)).toBe("dragon");
    expect(dragonTimelineIcon("unknown")).toBe("dragon");
  });

  it("uses the final icon identity as the homogeneous-cluster key", () => {
    const kills = [
      event("ChampionKill", {
        killer: "Ally Mid",
        victim: "Enemy Mid",
      }),
      event("FirstBlood", {
        killer: "Ally Jungle",
        victim: "Enemy Jungle",
      }),
      event("Multikill", {
        killer: "Ally Carry",
        kill_streak: 3,
      }),
    ];

    expect(
      timelineEventsAreHomogeneous(kills, "Me"),
    ).toBe(true);

    expect(
      timelineEventsAreHomogeneous(
        [
          kills[0],
          event("DragonKill", {
            dragon_type: "Fire",
          }),
        ],
        "Me",
      ),
    ).toBe(false);
  });

  it("describes homogeneous and mixed clusters without overloading team color", () => {
    const allyKills = [
      event("ChampionKill", {
        killer: "Ally Mid",
        victim: "Enemy Mid",
        relation: "ally",
      }),
      event("ChampionKill", {
        killer: "Ally Jungle",
        victim: "Enemy Jungle",
        relation: "ally",
      }),
    ];

    expect(
      timelineClusterPresentation(
        allyKills,
        "Me",
      ),
    ).toEqual({
      homogeneous: true,
      icon: "swords",
      relation: "ally",
    });

    expect(
      timelineClusterPresentation(
        [
          allyKills[0],
          event("ChampionKill", {
            killer: "Enemy Carry",
            victim: "Ally Carry",
            relation: "enemy",
          }),
        ],
        "Me",
      ),
    ).toEqual({
      homogeneous: true,
      icon: "swords",
      relation: "neutral",
    });

    expect(
      timelineClusterPresentation(
        [
          allyKills[0],
          event("DragonKill", {
            dragon_type: "Fire",
            relation: "ally",
          }),
        ],
        "Me",
      ),
    ).toEqual({
      homogeneous: false,
      icon: "generic",
      relation: "neutral",
    });
  });
  it("makes a local death the highest collision priority", () => {
    const ordinaryKill = event("ChampionKill", {
      killer: "Ally Top",
      victim: "Enemy Top",
    });

    const ownAssist = event("ChampionKill", {
      killer: "Ally Jungle",
      victim: "Enemy Jungle",
      assisters: ["Me#EUW"],
    });

    const ownKill = event("ChampionKill", {
      killer: "Me#EUW",
      victim: "Enemy Mid",
    });

    const ownDeath = event("ChampionKill", {
      killer: "Enemy Carry",
      victim: "Me#EUW",
    });

    expect(
      highestPriorityTimelineEvent(
        [
          ordinaryKill,
          ownAssist,
          ownKill,
          ownDeath,
        ],
        "Me#TEST",
      ),
    ).toBe(ownDeath);
  });

  it("preserves multikill multiplicity without changing its kill identity", () => {
    const triple = timelineEventPresentation(
      event("Multikill", {
        killer: "Me",
        kill_streak: 3,
      }),
      "Me",
    );

    expect(triple.kind).toBe("kill");
    expect(triple.icon).toBe("swords");
    expect(triple.multiplier).toBe(3);
    expect(triple.clusterKey).toBe("swords");
  });
});
