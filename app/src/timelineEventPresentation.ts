import type { ViewerEvent } from "./types";
import { samePlayer } from "./viewerUtils";

export type TimelineEventKind =
  | "kill"
  | "death"
  | "tower"
  | "inhibitor"
  | "dragon"
  | "baron"
  | "herald"
  | "void-grubs"
  | "match-event";

export type TimelineEventIconKey =
  | "swords"
  | "skull"
  | "tower"
  | "inhibitor"
  | "dragon"
  | "dragon-air"
  | "dragon-earth"
  | "dragon-fire"
  | "dragon-water"
  | "dragon-hextech"
  | "dragon-chemtech"
  | "dragon-elder"
  | "baron"
  | "herald"
  | "void-grubs"
  | "generic";

export type TimelineEventLocalRole =
  | "victim"
  | "killer"
  | "assister"
  | "actor"
  | null;

export type TimelineEventPresentation = {
  kind: TimelineEventKind;
  icon: TimelineEventIconKey;
  relation: ViewerEvent["relation"];
  localRole: TimelineEventLocalRole;
  localPriority: number;
  clusterKey: string;
  multiplier: number | null;
};

export type TimelineClusterPresentation = {
  homogeneous: boolean;
  icon: TimelineEventIconKey;
  relation: ViewerEvent["relation"];
};

const normalizedDragonType = (value: string | null): string =>
  (value ?? "")
    .trim()
    .toLowerCase()
    .replace(/[\s_-]+/g, "");

export const dragonTimelineIcon = (
  dragonType: string | null,
): TimelineEventIconKey => {
  switch (normalizedDragonType(dragonType)) {
    case "air":
    case "cloud":
    case "clouddrake":
      return "dragon-air";

    case "earth":
    case "mountain":
    case "mountaindrake":
      return "dragon-earth";

    case "fire":
    case "infernal":
    case "infernaldrake":
      return "dragon-fire";

    case "water":
    case "ocean":
    case "oceandrake":
      return "dragon-water";

    case "hextech":
    case "hextechdrake":
      return "dragon-hextech";

    case "chemtech":
    case "chemtechdrake":
      return "dragon-chemtech";

    case "elder":
    case "elderdragon":
    case "elderdrake":
      return "dragon-elder";

    default:
      return "dragon";
  }
};

export const timelineEventLocalRole = (
  event: ViewerEvent,
  localPlayerName: string | null,
): TimelineEventLocalRole => {
  if (!localPlayerName) return null;

  if (
    event.victim !== null &&
    samePlayer(event.victim, localPlayerName)
  ) {
    return "victim";
  }

  if (
    event.killer !== null &&
    samePlayer(event.killer, localPlayerName)
  ) {
    return "killer";
  }

  if (
    event.assisters.some((assister) =>
      samePlayer(assister, localPlayerName),
    )
  ) {
    return "assister";
  }

  if (
    event.acer !== null &&
    samePlayer(event.acer, localPlayerName)
  ) {
    return "actor";
  }

  return null;
};

const localPriorityForRole = (
  role: TimelineEventLocalRole,
): number => {
  switch (role) {
    case "victim":
      return 4;

    case "killer":
      return 3;

    case "actor":
      return 2;

    case "assister":
      return 0;

    default:
      return 0;
  }
};

const presentation = (
  event: ViewerEvent,
  kind: TimelineEventKind,
  icon: TimelineEventIconKey,
  localRole: TimelineEventLocalRole,
  multiplier: number | null = null,
): TimelineEventPresentation => ({
  kind,
  icon,
  relation: event.relation,
  localRole,
  localPriority: localPriorityForRole(localRole),
  clusterKey: icon,
  multiplier,
});

export const timelineEventPresentation = (
  event: ViewerEvent,
  localPlayerName: string | null,
): TimelineEventPresentation => {
  const localRole = timelineEventLocalRole(
    event,
    localPlayerName,
  );

  switch (event.event_type) {
    case "ChampionKill":
    case "FirstBlood":
      return localRole === "victim"
        ? presentation(
            event,
            "death",
            "skull",
            localRole,
          )
        : presentation(
            event,
            "kill",
            "swords",
            localRole,
          );

    case "Multikill":
      return presentation(
        event,
        "kill",
        "swords",
        localRole,
        event.kill_streak,
      );

    case "Ace":
      return presentation(
        event,
        "kill",
        "swords",
        localRole,
      );

    case "TurretKilled":
    case "FirstBrick":
      return presentation(
        event,
        "tower",
        "tower",
        localRole,
      );

    case "InhibKilled":
    case "InhibRespawned":
      return presentation(
        event,
        "inhibitor",
        "inhibitor",
        localRole,
      );

    case "DragonKill": {
      const icon = dragonTimelineIcon(event.dragon_type);

      return presentation(
        event,
        "dragon",
        icon,
        localRole,
      );
    }

    case "BaronKill":
      return presentation(
        event,
        "baron",
        "baron",
        localRole,
      );

    case "HeraldKill":
      return presentation(
        event,
        "herald",
        "herald",
        localRole,
      );

    case "Horde":
    case "HordeKill":
      return presentation(
        event,
        "void-grubs",
        "void-grubs",
        localRole,
      );

    default:
      return presentation(
        event,
        "match-event",
        "generic",
        localRole,
      );
  }
};

export const timelineClusterPresentation = (
  events: readonly ViewerEvent[],
  localPlayerName: string | null,
): TimelineClusterPresentation => {
  if (events.length === 0) {
    return {
      homogeneous: false,
      icon: "generic",
      relation: "neutral",
    };
  }

  const presentations = events.map((event) =>
    timelineEventPresentation(event, localPlayerName),
  );

  const first = presentations[0];
  const homogeneous = presentations.every(
    (item) => item.clusterKey === first.clusterKey,
  );
  const sameRelation = presentations.every(
    (item) => item.relation === first.relation,
  );

  return {
    homogeneous,
    icon: homogeneous ? first.icon : "generic",
    relation:
      homogeneous && sameRelation
        ? first.relation
        : "neutral",
  };
};

export const timelineEventsAreHomogeneous = (
  events: readonly ViewerEvent[],
  localPlayerName: string | null,
): boolean =>
  events.length <= 1 ||
  timelineClusterPresentation(
    events,
    localPlayerName,
  ).homogeneous;

export const highestPriorityTimelineEvent = <
  T extends ViewerEvent,
>(
  events: readonly T[],
  localPlayerName: string | null,
): T | null => {
  let selected: T | null = null;
  let selectedPriority = 0;

  for (const event of events) {
    const priority = timelineEventPresentation(
      event,
      localPlayerName,
    ).localPriority;

    if (priority > selectedPriority) {
      selected = event;
      selectedPriority = priority;
    }
  }

  return selected;
};
