import type { ViewerEvent } from "./types";

export const clamp = (value: number, minimum: number, maximum: number): number =>
  Math.min(maximum, Math.max(minimum, value));

export const latestIndexAt = (
  entries: readonly { video_time_ms: number }[],
  videoTimeMs: number,
): number => {
  let low = 0;
  let high = entries.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (entries[middle].video_time_ms <= videoTimeMs) low = middle + 1;
    else high = middle;
  }
  return low - 1;
};

export const nearestIndexAt = (
  entries: readonly { video_time_ms: number }[],
  videoTimeMs: number,
): number => {
  if (entries.length === 0) return -1;
  const after = latestIndexAt(entries, videoTimeMs) + 1;
  if (after <= 0) return 0;
  if (after >= entries.length) return entries.length - 1;
  return videoTimeMs - entries[after - 1].video_time_ms <=
    entries[after].video_time_ms - videoTimeMs
    ? after - 1
    : after;
};

export const timelineValue = <T extends { video_time_ms: number }>(
  entries: readonly T[],
  videoTimeMs: number,
): T | undefined => {
  const index = latestIndexAt(entries, videoTimeMs);
  return index < 0 ? undefined : entries[index];
};

export const eventTitle = (event: ViewerEvent): string => {
  switch (event.event_type) {
    case "FirstBlood":
      return "FIRST BLOOD";
    case "ChampionKill":
      return "CHAMPION KILL";
    case "Multikill":
      return event.kill_streak ? `${event.kill_streak}X MULTIKILL` : "MULTIKILL";
    case "DragonKill":
      return `${event.dragon_type?.toUpperCase() ?? "DRAGON"} TAKEN`;
    case "BaronKill":
      return "BARON TAKEN";
    case "HeraldKill":
      return "HERALD TAKEN";
    case "Horde":
    case "HordeKill":
      return "VOID GRUBS TAKEN";
    case "TurretKilled":
      return "TURRET DESTROYED";
    case "InhibKilled":
      return "INHIBITOR DESTROYED";
    case "InhibRespawned":
      return "INHIBITOR RESPAWNED";
    case "Ace":
      return "ACE";
    case "FirstBrick":
      return "FIRST TURRET";
    case "GameStart":
      return "GAME CLOCK START";
    case "GameEnd":
      return "GAME END CAPTURED";
    case "Minions":
    case "MinionsSpawning":
      return "MINIONS SPAWNED";
    default:
      return event.event_type.replace(/([a-z])([A-Z])/g, "$1 $2").toUpperCase();
  }
};

export const eventSummary = (event: ViewerEvent): string => {
  if (event.event_type === "ChampionKill" || event.event_type === "FirstBlood") {
    return [event.killer, event.victim].filter(Boolean).join("  /  ") || "Combat event";
  }
  if (event.event_type === "Ace") return event.acer ?? "Team fight resolved";
  if (event.event_type === "GameEnd") return "Optional Live Client telemetry";
  if (event.killer) return event.killer;
  if (event.acing_team) return event.acing_team;
  return "Live Client event";
};

export const eventCategory = (event: ViewerEvent): string => {
  if (["ChampionKill", "FirstBlood", "Multikill", "Ace"].includes(event.event_type)) {
    return "ELIMINATION";
  }
  if (
    [
      "DragonKill",
      "BaronKill",
      "HeraldKill",
      "Horde",
      "HordeKill",
      "TurretKilled",
      "InhibKilled",
      "FirstBrick",
    ].includes(event.event_type)
  ) {
    return "OBJECTIVE";
  }
  return "MATCH EVENT";
};

export const formatGoldDifference = (value: number): string => {
  const rounded = Math.round(value);
  const prefix = rounded >= 0 ? "+" : "−";
  return `${prefix}${Math.abs(rounded).toLocaleString("en-US")}G`;
};
