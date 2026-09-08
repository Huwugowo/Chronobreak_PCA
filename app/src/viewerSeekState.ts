import type { ReplayTick } from "./replayTime";

export type PresentationAuthority = "rvfc" | "media-clock-approximate";

export type SeekRequest = Readonly<{
  generation: number;
  epoch: number;
  target: ReplayTick;
  toleranceTicks: number;
}>;

export type ViewerSeekState = Readonly<{
  generation: number;
  nextEpoch: number;
  requestedPreview: ReplayTick | null;
  dispatched: SeekRequest | null;
  lastDispatched: SeekRequest | null;
  seekedObservation: ReplayTick | null;
  presented: ReplayTick | null;
}>;

export const createViewerSeekState = (generation = 0): ViewerSeekState => ({
  generation,
  nextEpoch: 0,
  requestedPreview: null,
  dispatched: null,
  lastDispatched: null,
  seekedObservation: null,
  presented: null,
});

export const requestPreview = (state: ViewerSeekState, target: ReplayTick): ViewerSeekState => ({
  ...state,
  requestedPreview: target,
});

/** A dispatch supersedes all older observations, even inside one media generation. */
export const dispatchSeek = (
  state: ViewerSeekState,
  target: ReplayTick,
  toleranceTicks: number,
): ViewerSeekState => {
  if (!Number.isSafeInteger(toleranceTicks) || toleranceTicks < 0) {
    throw new RangeError("seek tolerance must be a nonnegative safe integer");
  }
  const epoch = state.nextEpoch + 1;
  return {
    ...state,
    nextEpoch: epoch,
    requestedPreview: target,
    dispatched: { generation: state.generation, epoch, target, toleranceTicks },
    lastDispatched: { generation: state.generation, epoch, target, toleranceTicks },
    seekedObservation: null,
  };
};

const qualifying = (request: SeekRequest, generation: number, observed: ReplayTick): boolean =>
  request.generation === generation && Math.abs(observed - request.target) <= request.toleranceTicks;

export const observeSeeked = (
  state: ViewerSeekState,
  generation: number,
  epoch: number,
  observed: ReplayTick,
): ViewerSeekState => {
  const request = state.dispatched ?? state.lastDispatched;
  if (!request || request.epoch !== epoch || !qualifying(request, generation, observed)) return state;
  return { ...state, seekedObservation: observed };
};

/** Only RVFC presentation may advance the authoritative replay position. */
export const observePresentation = (
  state: ViewerSeekState,
  generation: number,
  epoch: number | null,
  observed: ReplayTick,
  authority: PresentationAuthority,
): ViewerSeekState => {
  if (authority !== "rvfc") return state;
  const request = state.dispatched;
  if (request && (epoch !== request.epoch || !qualifying(request, generation, observed))) return state;
  if (!request && generation !== state.generation) return state;
  return {
    ...state,
    presented: observed,
    dispatched: request ? null : state.dispatched,
  };
};

/** Recovery discards every browser observation and starts a new media generation. */
export const beginRecovery = (state: ViewerSeekState): ViewerSeekState => ({
  generation: state.generation + 1,
  nextEpoch: 0,
  requestedPreview: state.requestedPreview,
  dispatched: null,
  lastDispatched: null,
  seekedObservation: null,
  presented: null,
});
