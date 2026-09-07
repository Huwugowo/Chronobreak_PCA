import {
  REPLAY_TICKS_PER_SECOND,
  createClipRange,
  frameBoundaryToReplayTick,
  projectReplayIntervalToClipRange,
  replayTickToFrameBoundary,
  type ClipRange,
  type FrameBoundary,
  type MediaTimelineV2,
  type ReplayTick,
  type RoundingMode,
} from "./replayTime";
import type { ViewerEvent } from "./types";

export const MINIMUM_CLIP_DURATION_TICKS = (5 * REPLAY_TICKS_PER_SECOND) as ReplayTick;

export const clamp = (value: number, minimum: number, maximum: number): number =>
  Math.min(maximum, Math.max(minimum, value));

type ReplayTimed = { replay_tick: ReplayTick };

export const latestIndexAt = (entries: readonly ReplayTimed[], replayTick: ReplayTick): number => {
  let low = 0;
  let high = entries.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (entries[middle].replay_tick <= replayTick) low = middle + 1;
    else high = middle;
  }
  return low - 1;
};

export const nearestIndexAt = (entries: readonly ReplayTimed[], replayTick: ReplayTick): number => {
  if (entries.length === 0) return -1;
  const after = latestIndexAt(entries, replayTick) + 1;
  if (after <= 0) return 0;
  if (after >= entries.length) return entries.length - 1;
  return replayTick - entries[after - 1].replay_tick <= entries[after].replay_tick - replayTick
    ? after - 1
    : after;
};

export const frameBoundaryAt = (
  replayTick: ReplayTick,
  timeline: MediaTimelineV2,
  rounding: RoundingMode,
): FrameBoundary => replayTickToFrameBoundary(replayTick, timeline.video.frameRate, rounding);

export const replayTickAtFrameBoundary = (
  frame: FrameBoundary,
  timeline: MediaTimelineV2,
): ReplayTick => frameBoundaryToReplayTick(frame, timeline.video.frameRate);

const minimumClipFrames = (timeline: MediaTimelineV2): FrameBoundary =>
  frameBoundaryAt(MINIMUM_CLIP_DURATION_TICKS, timeline, "ceil");

const assertMinimumClipRange = (range: ClipRange, timeline: MediaTimelineV2): ClipRange => {
  if (range.endFrameExclusive - range.startFrame < minimumClipFrames(timeline)) {
    throw new Error("minimum_clip_duration");
  }
  return range;
};

/** Validates identity, bounds, and the minimum duration on the source frame grid. */
export const alignClipRangeToFrames = (range: ClipRange, timeline: MediaTimelineV2): ClipRange =>
  assertMinimumClipRange(
    createClipRange(timeline, range.startFrame, range.endFrameExclusive, range.mediaId),
    timeline,
  );

const boundedClipRange = (
  anchor: ReplayTick,
  preRollTicks: ReplayTick,
  postRollTicks: ReplayTick,
  timeline: MediaTimelineV2,
): ClipRange => {
  let start = clamp(anchor - preRollTicks, 0, timeline.video.replayEnd) as ReplayTick;
  let end = clamp(anchor + postRollTicks, 0, timeline.video.replayEnd) as ReplayTick;
  if (end - start < MINIMUM_CLIP_DURATION_TICKS) {
    if (end >= MINIMUM_CLIP_DURATION_TICKS) {
      start = (end - MINIMUM_CLIP_DURATION_TICKS) as ReplayTick;
    } else {
      end = Math.min(timeline.video.replayEnd, start + MINIMUM_CLIP_DURATION_TICKS) as ReplayTick;
    }
  }
  return alignClipRangeToFrames(projectReplayIntervalToClipRange(timeline, start, end), timeline);
};

export const defaultClipRange = (
  anchor: ReplayTick,
  timeline: MediaTimelineV2,
  events: readonly ViewerEvent[],
  anchorEvent?: ViewerEvent,
): ClipRange => {
  let preRollTicks = (8 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
  let postRollTicks = (5 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
  if (anchorEvent?.event_type === "ChampionKill" || anchorEvent?.event_type === "FirstBlood") {
    if (anchorEvent.assisters.length > 0) {
      preRollTicks = (15 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
      postRollTicks = (8 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    }
    const multikill = events
      .filter(
        (event) =>
          event.event_type === "Multikill" &&
          event.killer === anchorEvent.killer &&
          event.replay_tick !== undefined &&
          anchorEvent.replay_tick !== undefined &&
          Math.abs(event.replay_tick - anchorEvent.replay_tick) <= 5 * REPLAY_TICKS_PER_SECOND,
      )
      .reduce((largest, event) => Math.max(largest, event.kill_streak ?? 0), 0);
    if (multikill >= 5) {
      preRollTicks = (25 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
      postRollTicks = (15 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    } else if (multikill >= 2) {
      preRollTicks = (20 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
      postRollTicks = (10 * REPLAY_TICKS_PER_SECOND) as ReplayTick;
    }
  }
  return boundedClipRange(anchor, preRollTicks, postRollTicks, timeline);
};

export const moveClipEndpoint = (
  range: ClipRange,
  endpoint: "start" | "end",
  requested: ReplayTick,
  timeline: MediaTimelineV2,
): ClipRange => {
  const current = alignClipRangeToFrames(range, timeline);
  const minimumFrames = minimumClipFrames(timeline);
  const requestedFrame = frameBoundaryAt(requested, timeline, endpoint === "start" ? "floor" : "ceil");
  if (endpoint === "start") {
    return createClipRange(
      timeline,
      clamp(requestedFrame, 0, current.endFrameExclusive - minimumFrames) as FrameBoundary,
      current.endFrameExclusive,
      current.mediaId,
    );
  }
  return createClipRange(
    timeline,
    current.startFrame,
    clamp(requestedFrame, current.startFrame + minimumFrames, timeline.video.frameCount) as FrameBoundary,
    current.mediaId,
  );
};

/** The end preview is the final included source frame of a half-open range. */
export const clipEndpointPreviewTick = (
  range: ClipRange,
  endpoint: "start" | "end",
  timeline: MediaTimelineV2,
): ReplayTick => {
  const current = alignClipRangeToFrames(range, timeline);
  return replayTickAtFrameBoundary(
    (endpoint === "start" ? current.startFrame : current.endFrameExclusive - 1) as FrameBoundary,
    timeline,
  );
};

export const timelineValue = <T extends ReplayTimed>(
  entries: readonly T[],
  replayTick: ReplayTick,
): T | undefined => {
  const index = latestIndexAt(entries, replayTick);
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
    case "FirstBlood": return "FIRST BLOOD";
    case "ChampionKill": return "CHAMPION KILL";
    case "Multikill": return event.kill_streak ? `${event.kill_streak}X MULTIKILL` : "MULTIKILL";
    case "DragonKill": return `${event.dragon_type?.toUpperCase() ?? "DRAGON"} TAKEN`;
    case "BaronKill": return "BARON TAKEN";
    case "HeraldKill": return "HERALD TAKEN";
    case "Horde": case "HordeKill": return "VOID GRUBS TAKEN";
    case "TurretKilled": return "TURRET DESTROYED";
    case "InhibKilled": return "INHIBITOR DESTROYED";
    case "InhibRespawned": return "INHIBITOR RESPAWNED";
    case "Ace": return "ACE";
    case "FirstBrick": return "FIRST TURRET";
    case "GameStart": return "GAME CLOCK START";
    case "GameEnd": return "GAME END CAPTURED";
    case "Minions": case "MinionsSpawning": return "MINIONS SPAWNED";
    default: return event.event_type.replace(/([a-z])([A-Z])/g, "$1 $2").toUpperCase();
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
  if (["ChampionKill", "FirstBlood", "Multikill", "Ace"].includes(event.event_type)) return "ELIMINATION";
  if (["DragonKill", "BaronKill", "HeraldKill", "Horde", "HordeKill", "TurretKilled", "InhibKilled", "FirstBrick"].includes(event.event_type)) return "OBJECTIVE";
  return "MATCH EVENT";
};
