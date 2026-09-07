import type { ReplayTick } from "./replayTime";

export type ClipPreviewSeek = Readonly<{
  generation: number;
  epoch: number;
  target: ReplayTick;
  toleranceTicks: number;
}>;

/**
 * The primary video alone establishes clip-preview position. Secondary media is
 * allowed to follow only a qualifying presented primary frame, never a seek
 * assignment or its `seeked` observation.
 */
export type ClipPreviewSyncState = Readonly<{
  generation: number;
  nextEpoch: number;
  dispatched: ClipPreviewSeek | null;
  presentedTick: ReplayTick | null;
  playing: boolean;
  ended: boolean;
}>;

export const createClipPreviewSyncState = (generation = 0): ClipPreviewSyncState => ({
  generation,
  nextEpoch: 0,
  dispatched: null,
  presentedTick: null,
  playing: false,
  ended: false,
});

export const dispatchClipPreviewSeek = (
  state: ClipPreviewSyncState,
  target: ReplayTick,
  toleranceTicks: number,
): ClipPreviewSyncState => {
  if (!Number.isSafeInteger(toleranceTicks) || toleranceTicks < 0) {
    throw new RangeError("clip preview seek tolerance must be a nonnegative safe integer");
  }
  const epoch = state.nextEpoch + 1;
  return {
    ...state,
    nextEpoch: epoch,
    dispatched: { generation: state.generation, epoch, target, toleranceTicks },
    ended: false,
  };
};

/** Ignores stale/off-target RVFC observations while a seek is in flight. */
export const observeClipPreviewPresentation = (
  state: ClipPreviewSyncState,
  generation: number,
  epoch: number | null,
  observed: ReplayTick,
): ClipPreviewSyncState => {
  if (generation !== state.generation) return state;
  const dispatched = state.dispatched;
  if (
    dispatched &&
    (epoch !== dispatched.epoch ||
      Math.abs(observed - dispatched.target) > dispatched.toleranceTicks)
  ) {
    return state;
  }
  if (!dispatched && epoch !== null) return state;
  if (!dispatched && state.presentedTick === observed) return state;
  return { ...state, dispatched: null, presentedTick: observed, ended: false };
};

export const setClipPreviewPlaying = (
  state: ClipPreviewSyncState,
  playing: boolean,
): ClipPreviewSyncState => ({ ...state, playing, ended: false });

export const endClipPreview = (state: ClipPreviewSyncState): ClipPreviewSyncState => ({
  ...state,
  playing: false,
  ended: true,
});

/** Maps the primary replay coordinate into a looping music track coordinate. */
export const musicLoopTickForPrimaryTick = (
  primaryTick: ReplayTick,
  clipStartTick: ReplayTick,
  musicDurationTicks: ReplayTick | null,
): ReplayTick => {
  const clipRelativeTick = Math.max(0, primaryTick - clipStartTick);
  const looped = musicDurationTicks && musicDurationTicks > 0
    ? clipRelativeTick % musicDurationTicks
    : clipRelativeTick;
  return looped as ReplayTick;
};
