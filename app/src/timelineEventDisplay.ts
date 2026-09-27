import {
  REPLAY_TICKS_PER_SECOND,
  type ReplayTick,
} from "./replayTime";
import type { ViewerEvent } from "./types";
import { samePlayer } from "./viewerUtils";

export type MappedTimelineEvent =
  ViewerEvent & { replay_tick: ReplayTick };

export type TimelineDisplayEvent<
  T extends MappedTimelineEvent = MappedTimelineEvent,
> = {
  replay_tick: ReplayTick;
  source: T;
  multiplier: number | null;
};

const META_ASSOCIATION_WINDOW =
  5 * REPLAY_TICKS_PER_SECOND;

const sameOptionalPlayer = (
  left: string | null,
  right: string | null,
): boolean => {
  if (left === null || right === null) {
    return left === right;
  }

  return samePlayer(left, right);
};

const compatibleOptionalPlayer = (
  left: string | null,
  right: string | null,
): boolean =>
  left === null ||
  right === null ||
  samePlayer(left, right);

const compatibleOptionalValue = (
  left: string | null,
  right: string | null,
): boolean =>
  left === null ||
  right === null ||
  left === right;

const isKillEvent = (
  event: ViewerEvent,
): boolean =>
  event.event_type === "ChampionKill" ||
  event.event_type === "FirstBlood";

const duplicatesChampionKill = <
  T extends MappedTimelineEvent,
>(
  event: T,
  events: readonly T[],
): boolean =>
  event.event_type === "FirstBlood" &&
  events.some(
    (candidate) =>
      candidate.event_type === "ChampionKill" &&
      candidate.replay_tick === event.replay_tick &&
      sameOptionalPlayer(
        candidate.killer,
        event.killer,
      ) &&
      sameOptionalPlayer(
        candidate.victim,
        event.victim,
      ),
  );

const duplicatesTurretKill = <
  T extends MappedTimelineEvent,
>(
  event: T,
  events: readonly T[],
): boolean =>
  event.event_type === "FirstBrick" &&
  events.some(
    (candidate) =>
      candidate.event_type === "TurretKilled" &&
      candidate.replay_tick === event.replay_tick &&
      candidate.relation === event.relation &&
      compatibleOptionalPlayer(
        candidate.killer,
        event.killer,
      ) &&
      compatibleOptionalValue(
        candidate.turret,
        event.turret,
      ),
  );

const nearestPreviousKill = <
  T extends MappedTimelineEvent,
>(
  displayEvents: readonly TimelineDisplayEvent<T>[],
  metaEvent: T,
  predicate: (candidate: T) => boolean,
): TimelineDisplayEvent<T> | undefined => {
  let selected:
    | TimelineDisplayEvent<T>
    | undefined;

  for (const candidate of displayEvents) {
    if (!isKillEvent(candidate.source)) continue;
    if (!predicate(candidate.source)) continue;
    if (candidate.replay_tick > metaEvent.replay_tick) {
      continue;
    }

    const distance =
      metaEvent.replay_tick - candidate.replay_tick;

    if (
      distance < 0 ||
      distance > META_ASSOCIATION_WINDOW
    ) {
      continue;
    }

    if (
      !selected ||
      candidate.replay_tick > selected.replay_tick
    ) {
      selected = candidate;
    }
  }

  return selected;
};

/**
 * Converts the raw Live Client event stream into timeline events.
 *
 * Riot exposes semantic/meta events such as FirstBlood,
 * Multikill, Ace and FirstBrick alongside the underlying
 * gameplay events. The timeline must not render those as
 * duplicate physical moments.
 *
 * Raw events remain untouched elsewhere in the viewer.
 */
export const timelineDisplayEvents = <
  T extends MappedTimelineEvent,
>(
  events: readonly T[],
): readonly TimelineDisplayEvent<T>[] => {
  const ordered = [...events].sort(
    (left, right) =>
      left.replay_tick - right.replay_tick,
  );

  const result: TimelineDisplayEvent<T>[] =
    ordered
      .filter(
        (event) =>
          event.event_type !== "Multikill" &&
          event.event_type !== "Ace" &&
          !duplicatesChampionKill(
            event,
            ordered,
          ) &&
          !duplicatesTurretKill(
            event,
            ordered,
          ),
      )
      .map((source) => ({
        replay_tick: source.replay_tick,
        source,
        multiplier: null,
      }));

  for (const metaEvent of ordered) {
    if (metaEvent.event_type === "Multikill") {
      const multiplier =
        metaEvent.kill_streak ?? 0;

      const target =
        metaEvent.killer === null
          ? undefined
          : nearestPreviousKill(
              result,
              metaEvent,
              (candidate) =>
                candidate.killer !== null &&
                samePlayer(
                  candidate.killer,
                  metaEvent.killer!,
                ),
            );

      if (target) {
        if (multiplier > 1) {
          target.multiplier = Math.max(
            target.multiplier ?? 0,
            multiplier,
          );
        }

        continue;
      }

      /*
       * Keep standalone Multikill telemetry when the
       * corresponding ChampionKill event is unavailable.
       */
      result.push({
        replay_tick: metaEvent.replay_tick,
        source: metaEvent,
        multiplier:
          multiplier > 1
            ? multiplier
            : null,
      });

      continue;
    }

    if (metaEvent.event_type === "Ace") {
      const target = nearestPreviousKill(
        result,
        metaEvent,
        (candidate) =>
          candidate.relation ===
          metaEvent.relation,
      );

      /*
       * Ace describes the outcome of the fight rather than
       * another physical kill. Suppress it when its fight is
       * already represented by a nearby kill.
       */
      if (target) continue;

      result.push({
        replay_tick: metaEvent.replay_tick,
        source: metaEvent,
        multiplier: null,
      });
    }
  }

  return result.sort(
    (left, right) =>
      left.replay_tick - right.replay_tick,
  );
};
