import type { ClipRange, ViewerEvent } from "./types";

export const MINIMUM_CLIP_MS = 5_000;
export const CLIP_SNAP_MS = 2_000;

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

const boundedClipRange = (
  anchorMs: number,
  preRollMs: number,
  postRollMs: number,
  durationMs: number,
): ClipRange => {
  let startMs = clamp(anchorMs - preRollMs, 0, durationMs);
  let endMs = clamp(anchorMs + postRollMs, 0, durationMs);
  if (endMs - startMs < MINIMUM_CLIP_MS) {
    if (endMs >= MINIMUM_CLIP_MS) startMs = endMs - MINIMUM_CLIP_MS;
    else endMs = Math.min(durationMs, startMs + MINIMUM_CLIP_MS);
  }
  return { startMs: Math.round(startMs), endMs: Math.round(endMs) };
};

export const defaultClipRange = (
  anchorMs: number,
  durationMs: number,
  events: readonly ViewerEvent[],
  anchorEvent?: ViewerEvent,
): ClipRange => {
  let preRollMs = 8_000;
  let postRollMs = 5_000;
  if (anchorEvent?.event_type === "ChampionKill" || anchorEvent?.event_type === "FirstBlood") {
    if (anchorEvent.assisters.length > 0) {
      preRollMs = 15_000;
      postRollMs = 8_000;
    }
    const multikill = events
      .filter(
        (event) =>
          event.event_type === "Multikill" &&
          event.killer === anchorEvent.killer &&
          Math.abs(event.video_time_ms - anchorEvent.video_time_ms) <= 5_000,
      )
      .reduce((largest, event) => Math.max(largest, event.kill_streak ?? 0), 0);
    if (multikill >= 5) {
      preRollMs = 25_000;
      postRollMs = 15_000;
    } else if (multikill >= 2) {
      preRollMs = 20_000;
      postRollMs = 10_000;
    }
  }
  return boundedClipRange(anchorMs, preRollMs, postRollMs, durationMs);
};

export const moveClipEndpoint = (
  range: ClipRange,
  endpoint: "start" | "end",
  requestedMs: number,
  durationMs: number,
  events: readonly ViewerEvent[],
): ClipRange => {
  const nearest = nearestIndexAt(events, requestedMs);
  const snapped =
    nearest >= 0 && Math.abs(events[nearest].video_time_ms - requestedMs) <= CLIP_SNAP_MS
      ? events[nearest].video_time_ms
      : requestedMs;
  if (endpoint === "start") {
    return {
      startMs: Math.round(clamp(snapped, 0, range.endMs - MINIMUM_CLIP_MS)),
      endMs: range.endMs,
    };
  }
  return {
    startMs: range.startMs,
    endMs: Math.round(clamp(snapped, range.startMs + MINIMUM_CLIP_MS, durationMs)),
  };
};

export const timelineValue = <T extends { video_time_ms: number }>(
  entries: readonly T[],
  videoTimeMs: number,
): T | undefined => {
  const index = latestIndexAt(entries, videoTimeMs);
  return index < 0 ? undefined : entries[index];
};

export const normalizedPlayerName = (player: string): string =>
  player.split("#", 1)[0].trim().toLowerCase();

export const samePlayer = (left: string, right: string): boolean =>
  normalizedPlayerName(left) === normalizedPlayerName(right);

export const eventInvolvesPlayer = (event: ViewerEvent, player: string): boolean =>
  [event.killer, event.victim, event.acer]
    .some((candidate) => candidate !== null && samePlayer(candidate, player)) ||
  event.assisters.some((assister) => samePlayer(assister, player));

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
