import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  loadPlaybackProbe,
  loadServerMetrics,
} from "../api";
import {
  buildScenarioActions,
  emitReplayBenchmarkEvent,
  REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT,
  replayBenchmarkObserver,
  type ScenarioAction,
} from "../benchmark";
import { formatBytes, formatDate, formatDuration } from "../format";
import type {
  ClipDraft,
  ClipRange,
  KdaTimelinePoint,
  PlaybackProbe,
  PlayerTimelinePoint,
  ServerMetrics,
  ViewerEvent,
} from "../types";
import {
  REPLAY_TICKS_PER_SECOND,
  type FrameBoundary,
  type ReplayTick,
} from "../replayTime";
import {
  alignClipRangeToFrames,
  clamp,
  clipEndpointPreviewTick,
  defaultClipRange,
  eventInvolvesPlayer,
  eventTitle,
  latestIndexAt,
  moveClipEndpoint,
  nearestIndexAt,
  replayTickAtFrameBoundary,
  timelineValue,
} from "../viewerUtils";
import { createPlaybackController, type PlaybackController, type PlaybackEvent, type PlaybackSnapshot } from "../playbackController";
import { createHtmlVideoPlaybackAdapter } from "../htmlVideoPlaybackAdapter";
import { createPlaybackDiagnostics, type DecoderSnapshot } from "../playbackDiagnostics";
import ChampionFilter from "./ChampionFilter";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./ViewerScreen.module.css";

type Props = {
  gameTimestamp: string;
  onBack: () => void;
  initialClipDraft?: ClipDraft;
  onExportClip: (draft: ClipDraft) => void;
};

type ClipEndpoint = "start" | "end";

type MediaPreviewState = "loading" | "ready" | "recovering" | "degraded";

type SeekReason =
  | "navigation"
  | "endpoint-edit"
  | "clip-preview"
  | "clip-loop"
  | "recovery"
  | "event-jump"
  | "benchmark";

type MediaDiagnosticEvent = {
  at: number;
  kind: string;
  detail: string;
};

type EndpointHoldState = {
  endpoint: ClipEndpoint;
  key: "ArrowLeft" | "ArrowRight";
  direction: -1 | 1;
  startedAt: number;
  startingFrame: ClipRange["startFrame"];
  animationFrameId: number;
};

const ENDPOINT_HOLD_THRESHOLD_MS = 200;
const SEEK_TIMEOUT_MS = 1_500;
const BENCHMARK_MEDIA_READY_TIMEOUT_MS = 30_000;

type MappedViewerEvent = ViewerEvent & { replay_tick: ReplayTick };
type MappedPlayerTimelinePoint = PlayerTimelinePoint & { replay_tick: ReplayTick };
type MappedKdaTimelinePoint = KdaTimelinePoint & { replay_tick: ReplayTick };

const hasReplayTick = <T extends { replay_tick?: ReplayTick }>(
  value: T,
): value is T & { replay_tick: ReplayTick } => value.replay_tick !== undefined;

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

const millisecondsToReplayTick = (milliseconds: number, replayEnd: ReplayTick): ReplayTick =>
  Math.round(
    clamp(milliseconds, 0, replayTickToMilliseconds(replayEnd)) * REPLAY_TICKS_PER_SECOND / 1_000,
  ) as ReplayTick;

const gameTickToMilliseconds = (gameTick: string): number => Number(BigInt(gameTick) / 1_000n);

function ViewerScreen(props: Props) {
  const [probe] = createResource(() => props.gameTimestamp, loadPlaybackProbe);
  let payloadReported = false;
  let failureReported = false;

  createEffect(() => {
    if (probe.error || payloadReported) return;
    const loaded = probe();
    if (!loaded) return;
    payloadReported = true;
    emitReplayBenchmarkEvent(
      "playback_payload_ready",
      {
        event_count: loaded.events.length,
        participant_count: loaded.participants.length,
        duration_ms: loaded.game.duration_ms,
      },
      { required: true },
    );
  });

  createEffect(() => {
    const error = probe.error;
    const observer = replayBenchmarkObserver();
    if (!error || !observer || failureReported) return;
    failureReported = true;
    const reason = error instanceof Error ? error.message : String(error);
    observer.emit("scenario_failed", { phase: "playback_probe", reason }, { required: true });
    void observer.complete("failed", reason, { phase: "playback_probe" });
  });

  return (
    <div class={styles.viewerScreen}>
      <Switch>
        <Match when={probe.loading}>
          <section class={styles.viewerState} aria-live="polite">
            Loading recording...
          </section>
        </Match>
        <Match when={probe.error}>
          <section class={styles.viewerState} role="alert">
            <strong>Recording could not be opened.</strong>
            <span>{String(probe.error)}</span>
            <button type="button" onClick={props.onBack}>
              Return to games
            </button>
          </section>
        </Match>
        <Match when={probe()}>
          {(loaded) => (
            <PlaybackSurface
              {...props}
              probe={loaded()}
            />
          )}
        </Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: Props & { probe: PlaybackProbe }) {
  const mediaTimeline = props.probe.media_timeline;
  const replayEnd = mediaTimeline.video.replayEnd;
  const durationMs = replayTickToMilliseconds(replayEnd);
  let primaryVideo: HTMLVideoElement | undefined;
  let controller!: PlaybackController;
  let decoderDiagnostics: ReturnType<typeof createPlaybackDiagnostics> | undefined;
  let windowedRail!: HTMLDivElement;
  let metricsIntervalId: number | undefined;
  let benchmarkMediaReadyTimerId: number | undefined;
  let mediaGeneration = replayBenchmarkObserver()?.nextMediaGeneration() ?? 0;
  let disposed = false;
  let endpointHold: EndpointHoldState | undefined;
  let firstPresentedGeneration = -1;
  let canPlayGeneration = -1;
  let benchmarkScenarioStarted = false;
  let benchmarkScenarioFinished = false;
  let benchmarkEndpointSequence = 0;
  const benchmarkAbortController = new AbortController();
  const dispatchObservers = new Map<string, () => void>();

  const events: MappedViewerEvent[] = props.probe.events
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick);
  const playerTimeline: MappedPlayerTimelinePoint[] = props.probe.player_timeline
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick);
  const kdaTimeline: MappedKdaTimelinePoint[] = props.probe.kda_timeline
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick);

  const [isFullscreen, setIsFullscreen] = createSignal(false);
  const [selectedPlayers, setSelectedPlayers] = createSignal<readonly string[]>([]);
  const [presentedPosition, setPresentedPosition] = createSignal<ReplayTick | null>(null);
  const [playback, setPlayback] = createSignal<PlaybackSnapshot>();
  const [decoder, setDecoder] = createSignal<DecoderSnapshot>();
  const [replayPosition, setReplayPosition] = createSignal<ReplayTick>(0 as ReplayTick);
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [mediaState, setMediaState] = createSignal<MediaPreviewState>("loading");
  const [mediaError, setMediaError] = createSignal<string | null>(null);
  const [editingEndpoint, setEditingEndpoint] = createSignal<ClipEndpoint | null>(null);
  const [diagnosticEvents, setDiagnosticEvents] = createSignal<MediaDiagnosticEvent[]>([]);
  const [seekQueueLabel, setSeekQueueLabel] = createSignal("IDLE");
  const [recoveryCount, setRecoveryCount] = createSignal(0);
  const [presentedFps, setPresentedFps] = createSignal(0);
  const [clockSource, setClockSource] = createSignal<"VIDEO FRAME" | "ANIMATION FRAME">(
    "VIDEO FRAME",
  );
  const [droppedFrames, setDroppedFrames] = createSignal(0);
  const [totalFrames, setTotalFrames] = createSignal(0);
  const [heapBytes, setHeapBytes] = createSignal<number | null>(null);
  const [seekLatencyMs, setSeekLatencyMs] = createSignal<number | null>(null);
  const [serverMetrics, setServerMetrics] = createSignal<ServerMetrics>({
    requests: 0,
    range_requests: 0,
    response_bytes: 0,
    completed_streams: 0,
    cancelled_streams: 0,
  });
  const initialClipRange = (): ClipRange | null => {
    const draft = props.initialClipDraft;
    if (
      !draft ||
      draft.gameTimestamp !== props.probe.game.timestamp ||
      draft.mediaId !== mediaTimeline.mediaId
    ) {
      return null;
    }
    try {
      return alignClipRangeToFrames(
        {
          mediaId: draft.mediaId,
          startFrame: draft.startFrame,
          endFrameExclusive: draft.endFrameExclusive,
        },
        mediaTimeline,
      );
    } catch {
      return null;
    }
  };
  const [clipRange, setClipRange] = createSignal<ClipRange | null>(initialClipRange());

  const clipBoundaryTick = (frame: ClipRange["startFrame"]): ReplayTick =>
    replayTickAtFrameBoundary(frame, mediaTimeline);
  const clipStartTick = (range: ClipRange): ReplayTick => clipBoundaryTick(range.startFrame);
  const clipEndTick = (range: ClipRange): ReplayTick => clipBoundaryTick(range.endFrameExclusive);
  const progress = createMemo(() => clamp((replayPosition() / replayEnd) * 100, 0, 100));
  const videoSecond = createMemo(() => Math.floor(replayPosition() / REPLAY_TICKS_PER_SECOND));
  const calibrationPoint = events[0] ?? playerTimeline[0];
  const replayTickAtGameZero = calibrationPoint
    ? BigInt(calibrationPoint.replay_tick) - BigInt(calibrationPoint.game_tick) * 48n
    : null;
  const currentGameTick = createMemo(() =>
    replayTickAtGameZero === null
      ? "0"
      : ((BigInt(presentedPosition() ?? 0) - replayTickAtGameZero) / 48n).toString(),
  );
  const beforeGameStart = createMemo(
    () => presentedPosition() === null || replayTickAtGameZero === null || BigInt(currentGameTick()) < 0n,
  );
  const gameClockSecond = createMemo(() =>
    Math.max(0, Math.floor(gameTickToMilliseconds(currentGameTick()) / 1_000)),
  );
  const visibleEvents = createMemo(() => {
    const selected = selectedPlayers();
    if (selected.length === 0) return events;
    return events.filter((event) =>
      selected.some((player) => eventInvolvesPlayer(event, player)),
    );
  });
  const markerPositions = createMemo(() =>
    visibleEvents().map((event, index) => ({
      event,
      index,
      position: clamp((event.replay_tick / replayEnd) * 100, 0, 100),
      tone: event.relation,
    })),
  );
  const clipStartPosition = createMemo(() =>
    clipRange() ? clamp((clipStartTick(clipRange()!) / replayEnd) * 100, 0, 100) : 0,
  );
  const clipEndPosition = createMemo(() =>
    clipRange() ? clamp((clipEndTick(clipRange()!) / replayEnd) * 100, 0, 100) : 100,
  );
  const activeEventIndex = createMemo(() => presentedPosition() === null ? -1 : latestIndexAt(visibleEvents(), presentedPosition()!));
  const activeEvent = createMemo(() => {
    const index = activeEventIndex();
    return index < 0 ? undefined : visibleEvents()[index];
  });
  const currentPlayer = createMemo<PlayerTimelinePoint | undefined>(() =>
    presentedPosition() === null ? undefined : timelineValue(playerTimeline, presentedPosition()!),
  );
  const currentKda = createMemo<KdaTimelinePoint | undefined>(() =>
    presentedPosition() === null ? undefined : timelineValue(kdaTimeline, presentedPosition()!),
  );
  const lastDiagnostic = createMemo(() => {
    const events = diagnosticEvents();
    return events[events.length - 1];
  });
  const togglePlayerFilter = (summonerName: string) => {
    setSelectedPlayers((selected) =>
      selected.includes(summonerName)
        ? selected.filter((player) => player !== summonerName)
        : [...selected, summonerName],
    );
  };

  const syncSnapshot = (value: PlaybackSnapshot) => {
    setPlayback(value);
    if (value.readiness === "disposed") { decoderDiagnostics?.dispose(); setDecoder(undefined); }
    else decoderDiagnostics?.bind(value, mediaTimeline);
    mediaGeneration = value.generation;
    setMediaState(value.readiness === "closed" || value.readiness === "disposed" ? "degraded" : value.readiness);
    setIsPlaying(value.desiredPlaying && !value.media.paused);
    setMediaError(value.error);
    setSeekQueueLabel(value.queue);
    setRecoveryCount(value.recoveryCount);
    setPresentedFps(value.presentedFps);
    setDroppedFrames(value.quality.droppedFrames ?? 0);
    setTotalFrames(value.quality.totalFrames ?? 0);
    setHeapBytes(value.quality.heapBytes);
    setSeekLatencyMs(value.seekLatencyMs);
    setClockSource(value.authority === "rvfc" ? "VIDEO FRAME" : "ANIMATION FRAME");
    setDiagnosticEvents(value.diagnostics.map((event) => ({ at: event.atMs, kind: event.kind, detail: JSON.stringify(event.payload) })));
    if (value.seek.presented === null) setPresentedPosition(null);
    if (value.queue !== "IDLE" && value.seek.requestedPreview !== null) setReplayPosition(value.seek.requestedPreview);
  };

  const onPlaybackEvent = (event: PlaybackEvent) => {
    mediaGeneration = event.generation;
    emitReplayBenchmarkEvent(event.kind, { ...event.payload }, { generation: event.generation, actionId: event.actionId, required: true });
    if (event.actionId && (event.kind === "seek_dispatched" || event.kind === "seek_deduped")) {
      dispatchObservers.get(event.actionId)?.(); dispatchObservers.delete(event.actionId);
    }
    if (event.kind === "first_presented_frame") firstPresentedGeneration = event.generation;
    if (event.kind === "media_canplay") canPlayGeneration = event.generation;
    if (event.kind === "first_presented_frame" || event.kind === "media_canplay") queueMicrotask(() => void runBenchmarkScenario());
  };

  const updateMetrics = async () => {
    if (!controller || disposed) return;
    syncSnapshot(controller.sampleMetrics());
    try {
      const metrics = await loadServerMetrics();
      if (!disposed) setServerMetrics(metrics);
    } catch { /* Server diagnostics must not interrupt playback. */ }
  };

  const seekTo = (target: ReplayTick, options: { reason?: SeekReason; playAfter?: boolean; onBenchmarkDispatch?: () => void } = {}): Promise<void> => {
    if (!controller) return Promise.resolve();
    controller.setLoopRange(clipRange());
    const observer = replayBenchmarkObserver();
    const actionId = observer?.nextActionId("seek");
    if (actionId && options.onBenchmarkDispatch) dispatchObservers.set(actionId, options.onBenchmarkDispatch);
    const operation = controller.seek(target, { reason: options.reason, playAfter: options.playAfter, actionId });
    const preview = controller.snapshot().seek.requestedPreview;
    if (preview !== null) setReplayPosition(preview);
    return operation.then((result) => {
      if (actionId) dispatchObservers.delete(actionId);
      if (observer && (result.status === "failed" || result.status === "unavailable")) throw new Error(result.reason ?? result.status);
    });
  };

  const playNativeVideo = async (propagateFailure = false) => {
    const result = await controller.play();
    if (propagateFailure && result.status !== "playing") throw new Error(result.reason ?? result.status);
  };
  const retryPreview = () => controller.retry();
  createEffect(() => { const range = clipRange(); if (controller) controller.setLoopRange(range); });

  const clipRangeForAnchor = (anchor: ReplayTick, anchorEvent?: ViewerEvent) =>
    defaultClipRange(anchor, mediaTimeline, events, anchorEvent);

  const activateClip = () => {
    const kills = events.filter(
      (candidate) =>
        candidate.event_type === "ChampionKill" || candidate.event_type === "FirstBlood",
    );
    const nearest = nearestIndexAt(kills, replayPosition());
    const anchorEvent =
      nearest >= 0 &&
      Math.abs(kills[nearest].replay_tick - replayPosition()) <= 10 * REPLAY_TICKS_PER_SECOND
        ? kills[nearest]
        : undefined;
    const anchor = anchorEvent?.replay_tick ?? replayPosition();
    const nextRange = clipRangeForAnchor(anchor, anchorEvent);
    setClipRange(nextRange);
    seekTo(replayPosition());
  };

  const selectEvent = (event: ViewerEvent) => {
    setEditingEndpoint(null);
    if (clipRange()) {
      if (event.replay_tick === undefined) return;
      const nextRange = clipRangeForAnchor(event.replay_tick, event);
      setClipRange(nextRange);
    }
    if (event.replay_tick !== undefined) seekTo(event.replay_tick);
  };

  const exportSelectedClip = () => {
    const range = clipRange();
    if (!range) return;
    clearEndpointHold();
    setEditingEndpoint(null);
    controller.pause();
    setIsFullscreen(false);
    props.onExportClip({
      gameTimestamp: props.probe.game.timestamp,
      mediaId: range.mediaId,
      startFrame: range.startFrame,
      endFrameExclusive: range.endFrameExclusive,
    });
  };

  const updateClipEndpoint = (
    endpoint: ClipEndpoint,
    requestedTick: ReplayTick,
  ) => {
    const range = clipRange();
    if (!range) return null;
    const nextRange = moveClipEndpoint(
      range,
      endpoint,
      requestedTick,
      mediaTimeline,
    );
    setClipRange(nextRange);
    const previewTick = clipEndpointPreviewTick(nextRange, endpoint, mediaTimeline);
    seekTo(previewTick, { reason: "endpoint-edit" });
    return nextRange;
  };

  const clearEndpointHold = () => {
    if (!endpointHold) return;
    cancelAnimationFrame(endpointHold.animationFrameId);
    endpointHold = undefined;
  };

  const beginClipEndpointEdit = (endpoint: ClipEndpoint) => {
    const range = clipRange();
    if (!range) return;
    clearEndpointHold();
    setEditingEndpoint(endpoint);
    controller.pause();
    const previewTick = clipEndpointPreviewTick(range, endpoint, mediaTimeline);
    seekTo(previewTick, { reason: "endpoint-edit" });
  };

  const finishClipEndpointEdit = () => {
    clearEndpointHold();
    const range = clipRange();
    if (!range) return;
    const endpoint = editingEndpoint();
    if (!endpoint) return;
    const previewTick = clipEndpointPreviewTick(range, endpoint, mediaTimeline);
    seekTo(previewTick, { reason: "endpoint-edit" });
  };

  const beginClipDrag = (
    event: PointerEvent & { currentTarget: HTMLButtonElement },
    endpoint: ClipEndpoint,
    rail: HTMLDivElement,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    handle.focus({ preventScroll: true });
    beginClipEndpointEdit(endpoint);
    const update = (pointerEvent: PointerEvent) => {
      const bounds = rail.getBoundingClientRect();
      if (bounds.width <= 0) return;
      updateClipEndpoint(
        endpoint,
        Math.round(
          clamp((pointerEvent.clientX - bounds.left) / bounds.width, 0, 1) * replayEnd,
        ) as ReplayTick,
      );
    };
    const cleanup = (pointerEvent: PointerEvent) => {
      handle.removeEventListener("pointermove", update);
      handle.removeEventListener("pointerup", finish);
      handle.removeEventListener("pointercancel", cancel);
      if (handle.hasPointerCapture(pointerEvent.pointerId)) {
        handle.releasePointerCapture(pointerEvent.pointerId);
      }
    };
    const finish = (pointerEvent: PointerEvent) => {
      update(pointerEvent);
      cleanup(pointerEvent);
      finishClipEndpointEdit();
    };
    const cancel = (pointerEvent: PointerEvent) => {
      cleanup(pointerEvent);
      finishClipEndpointEdit();
    };
    handle.setPointerCapture(event.pointerId);
    handle.addEventListener("pointermove", update);
    handle.addEventListener("pointerup", finish);
    handle.addEventListener("pointercancel", cancel);
  };

  const applyEndpointHoldTime = (hold: EndpointHoldState, now: number) => {
    if (now - hold.startedAt < ENDPOINT_HOLD_THRESHOLD_MS) return;
    const frameRate = mediaTimeline.video.frameRate;
    const elapsedFrames = Math.max(
      1,
      Math.round(
        ((now - hold.startedAt) * Number(frameRate.numerator)) /
          (1_000 * Number(frameRate.denominator)),
      ),
    );
    const requestedFrame = clamp(
      hold.startingFrame + hold.direction * elapsedFrames,
      0,
      mediaTimeline.video.frameCount,
    ) as FrameBoundary;
    updateClipEndpoint(
      hold.endpoint,
      replayTickAtFrameBoundary(requestedFrame, mediaTimeline),
    );
  };

  const handleClipEndpointKeyDown = (event: KeyboardEvent, endpoint: ClipEndpoint) => {
    if (event.key === " ") {
      event.preventDefault();
      event.stopPropagation();
      if (!event.repeat) void togglePlayback();
      return;
    }
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat || endpointHold) return;
    const range = clipRange();
    if (!range) return;
    const direction = event.key === "ArrowLeft" ? -1 : 1;
    const startingFrame = endpoint === "start" ? range.startFrame : range.endFrameExclusive;
    const startedAt = performance.now();
    beginClipEndpointEdit(endpoint);
    const requestedFrame = clamp(
      startingFrame + direction,
      0,
      mediaTimeline.video.frameCount,
    ) as FrameBoundary;
    updateClipEndpoint(
      endpoint,
      replayTickAtFrameBoundary(requestedFrame, mediaTimeline),
    );

    const advanceHeldEndpoint = (now: number) => {
      if (!endpointHold) return;
      applyEndpointHoldTime(endpointHold, now);
      if (endpointHold) {
        endpointHold.animationFrameId = requestAnimationFrame(advanceHeldEndpoint);
      }
    };
    endpointHold = {
      endpoint,
      key: event.key,
      direction,
      startedAt,
      startingFrame,
      animationFrameId: requestAnimationFrame(advanceHeldEndpoint),
    };
  };

  const handleClipEndpointKeyUp = (event: KeyboardEvent, endpoint: ClipEndpoint) => {
    const hold = endpointHold;
    if (!hold || hold.endpoint !== endpoint || hold.key !== event.key) return;
    event.preventDefault();
    event.stopPropagation();
    applyEndpointHoldTime(hold, performance.now());
    finishClipEndpointEdit();
  };

  const handleClipEndpointBlur = (endpoint: ClipEndpoint) => {
    if (editingEndpoint() !== endpoint) return;
    if (endpointHold) applyEndpointHoldTime(endpointHold, performance.now());
    finishClipEndpointEdit();
  };

  const cancelClipMode = () => {
    clearEndpointHold();
    setEditingEndpoint(null);
    setClipRange(null);
  };

  const seekFromRail = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (bounds.width <= 0) return;
    setEditingEndpoint(null);
    seekTo(
      Math.round(
        clamp((event.clientX - bounds.left) / bounds.width, 0, 1) * replayEnd,
      ) as ReplayTick,
    );
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: number | undefined;
    if (event.key === "PageDown") target = replayPosition() - 30 * REPLAY_TICKS_PER_SECOND;
    if (event.key === "PageUp") target = replayPosition() + 30 * REPLAY_TICKS_PER_SECOND;
    if (event.key === "Home") target = 0;
    if (event.key === "End") target = replayEnd;
    if (target === undefined) return;
    event.preventDefault();
    setEditingEndpoint(null);
    seekTo(clamp(target, 0, replayEnd) as ReplayTick);
  };

  const togglePlayback = async () => {
    if (!props.probe.video_url || mediaState() !== "ready") return;
    if (!controller.snapshot().media.paused) {
      controller.pause();
      return;
    }
    const range = clipRange();
    setEditingEndpoint(null);
    if (range) {
      seekTo(clipStartTick(range), { reason: "clip-preview", playAfter: true });
    } else {
      await playNativeVideo();
    }
  };

  const benchmarkAbortError = () => new DOMException("benchmark scenario disposed", "AbortError");

  const waitForBenchmark = (durationMs: number) =>
    new Promise<void>((resolve, reject) => {
      const signal = benchmarkAbortController.signal;
      if (signal.aborted) {
        reject(benchmarkAbortError());
        return;
      }
      const timeoutId = window.setTimeout(() => {
        signal.removeEventListener("abort", onAbort);
        resolve();
      }, durationMs);
      const onAbort = () => {
        window.clearTimeout(timeoutId);
        reject(benchmarkAbortError());
      };
      signal.addEventListener("abort", onAbort, { once: true });
    });

  const waitForBenchmarkFrames = (count = 2) =>
    new Promise<void>((resolve, reject) => {
      const signal = benchmarkAbortController.signal;
      let frameId: number | undefined;
      let remaining = count;
      const onAbort = () => {
        if (frameId !== undefined) cancelAnimationFrame(frameId);
        reject(benchmarkAbortError());
      };
      const onFrame = () => {
        frameId = undefined;
        remaining -= 1;
        if (remaining <= 0) {
          signal.removeEventListener("abort", onAbort);
          resolve();
          return;
        }
        frameId = requestAnimationFrame(onFrame);
      };
      if (signal.aborted) {
        reject(benchmarkAbortError());
        return;
      }
      signal.addEventListener("abort", onAbort, { once: true });
      frameId = requestAnimationFrame(onFrame);
    });

  const releaseBenchmarkMedia = async () => {
    controller.dispose();
    await waitForBenchmark(100);
  };

  const withBenchmarkTimeout = async <T,>(promise: Promise<T>, label: string): Promise<T> => {
    let timeoutId: number | undefined;
    const signal = benchmarkAbortController.signal;
    let rejectOnAbort: ((reason?: unknown) => void) | undefined;
    const onAbort = () => rejectOnAbort?.(benchmarkAbortError());
    try {
      return await Promise.race([
        promise,
        new Promise<never>((_, reject) => {
          timeoutId = window.setTimeout(
            () => reject(new Error(`${label} timed out`)),
            SEEK_TIMEOUT_MS * 4,
          );
        }),
        new Promise<never>((_, reject) => {
          rejectOnAbort = reject;
          if (signal.aborted) reject(benchmarkAbortError());
          else signal.addEventListener("abort", onAbort, { once: true });
        }),
      ]);
    } finally {
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
      signal.removeEventListener("abort", onAbort);
    }
  };

  const failBenchmarkAction = (
    actionKind: string,
    actionId: string,
    error: unknown,
    generation = mediaGeneration,
  ) => {
    emitReplayBenchmarkEvent(
      "action_failed",
      {
        action_kind: actionKind,
        reason: error instanceof Error ? error.message : String(error),
      },
      { generation, actionId, required: true },
    );
  };

  const runBenchmarkAction = async (action: ScenarioAction) => {
    switch (action.kind) {
      case "wait":
        await waitForBenchmark(action.durationMs);
        return;
      case "play": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("play");
        emitReplayBenchmarkEvent(
          "play_requested",
          { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          
          await playNativeVideo(true);
          if (controller.snapshot().media.paused || controller.snapshot().media.ended) throw new Error("native playback did not start");
          emitReplayBenchmarkEvent(
            "play_complete",
            { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );
          
          return;
        } catch (error) {
          
          if (!benchmarkAbortController.signal.aborted) failBenchmarkAction("play", actionId, error);
          throw error;
        }
      }
      case "pause": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("pause");
        emitReplayBenchmarkEvent(
          "pause_requested",
          { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          
          controller.pause();
          if (!controller.snapshot().media.paused) throw new Error("native playback did not pause");
          emitReplayBenchmarkEvent(
            "pause_complete",
            { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );
          
          return;
        } catch (error) {
          
          if (!benchmarkAbortController.signal.aborted) failBenchmarkAction("pause", actionId, error);
          throw error;
        }
      }
      case "stable-playback": {
        const generation = mediaGeneration;
        const startedAt = performance.now();
        const startingMediaTime = controller.snapshot().media.browserSeconds;
        const startingQuality = controller.sampleMetrics().quality;
        await waitForBenchmark(action.durationMs);
        const mediaDeltaMs = (controller.snapshot().media.browserSeconds - startingMediaTime) * 1_000;
        if (
          generation !== mediaGeneration ||
          controller.snapshot().media.paused ||
          controller.snapshot().media.ended ||
          mediaDeltaMs < Math.min(100, action.durationMs * 0.1)
        ) {
          throw new Error("stable playback window did not advance current media");
        }
        const quality = controller.sampleMetrics().quality;
        emitReplayBenchmarkEvent(
          "steady_playback",
          {
            duration_ms: performance.now() - startedAt,
            media_advance_ms: mediaDeltaMs,
            total_frame_delta:
              quality.totalFrames !== null && startingQuality.totalFrames !== null
                ? quality.totalFrames - startingQuality.totalFrames
                : null,
            dropped_frame_delta:
              quality.droppedFrames !== null && startingQuality.droppedFrames !== null
                ? quality.droppedFrames - startingQuality.droppedFrames
                : null,
            presentation_clock: clockSource(),
          },
          { generation, required: true },
        );
        return;
      }
      case "rate": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("rate");
        const generation = mediaGeneration;
        const startedAt = performance.now();
        const startingMediaTime = controller.snapshot().media.browserSeconds;
        const startingQuality = controller.sampleMetrics().quality;
        emitReplayBenchmarkEvent(
          "rate_requested",
          { rate: action.rate },
          { generation, actionId, required: true },
        );
        try {
          if (controller.snapshot().media.paused || controller.snapshot().media.ended) throw new Error("rate action requires active playback");
          controller.setRate(action.rate as PlaybackSnapshot["rate"]["selected"]);
          emitReplayBenchmarkEvent(
            "rate_applied",
            { requested_rate: action.rate, actual_rate: controller.snapshot().rate.applied },
            { generation, actionId, required: true },
          );
          await waitForBenchmark(action.durationMs);
          const wallSeconds = (performance.now() - startedAt) / 1_000;
          const mediaAdvance = controller.snapshot().media.browserSeconds - startingMediaTime;
          if (generation !== mediaGeneration || controller.snapshot().media.paused || controller.snapshot().media.ended || mediaAdvance <= 0) {
            throw new Error("rate window ended without advancing current media");
          }
          const quality = controller.sampleMetrics().quality;
          emitReplayBenchmarkEvent(
            "rate_observed",
            {
              requested_rate: action.rate,
              actual_rate: controller.snapshot().rate.applied,
              effective_rate: wallSeconds > 0 ? mediaAdvance / wallSeconds : null,
              muted: controller.snapshot().muted,
              volume: controller.snapshot().volume,
              dropped_frames: quality?.droppedFrames ?? null,
              dropped_frame_delta:
                quality.droppedFrames !== null && startingQuality.droppedFrames !== null
                  ? quality.droppedFrames - startingQuality.droppedFrames
                  : null,
            },
            { generation, actionId, required: true },
          );
          return;
        } catch (error) {
          if (!benchmarkAbortController.signal.aborted) {
            failBenchmarkAction("rate", actionId, error, generation);
          }
          throw error;
        }
      }
      case "seek":
        {
          const reason: SeekReason =
            action.reason === "endpoint-edit" || action.reason === "event-jump"
              ? action.reason
              : "benchmark";
          let target = millisecondsToReplayTick(action.targetMs, replayEnd);
          if (reason === "event-jump" && events.length > 0) {
            const eventIndex = nearestIndexAt(events, target);
            if (eventIndex >= 0) {
              const selected = events[eventIndex];
              target = selected.replay_tick;
              if (clipRange()) setClipRange(clipRangeForAnchor(target, selected));
              emitReplayBenchmarkEvent(
                "event_jump_selected",
                {
                  event_type: selected.event_type,
                  target_ms: replayTickToMilliseconds(target),
                },
                { generation: mediaGeneration },
              );
            }
          }
          if (reason === "endpoint-edit") {
            if (!clipRange()) setClipRange(clipRangeForAnchor(target));
            const endpoint: ClipEndpoint =
              benchmarkEndpointSequence++ % 2 === 0 ? "start" : "end";
            const range = clipRange();
            if (!range) throw new Error("benchmark endpoint edit could not create a clip range");
            const nextRange = moveClipEndpoint(
              range,
              endpoint,
              target,
              mediaTimeline,
            );
            setEditingEndpoint(endpoint);
            setClipRange(nextRange);
            target = clipEndpointPreviewTick(nextRange, endpoint, mediaTimeline);
            emitReplayBenchmarkEvent(
              "clip_endpoint_updated",
              {
                endpoint,
                clip_start_ms: replayTickToMilliseconds(clipStartTick(nextRange)),
                clip_end_ms: replayTickToMilliseconds(clipEndTick(nextRange)),
                target_ms: replayTickToMilliseconds(target),
              },
              { generation: mediaGeneration },
            );
          }
          await withBenchmarkTimeout(
            seekTo(target, { reason }),
            `seek to ${replayTickToMilliseconds(target)}`,
          );
          if (reason === "endpoint-edit") setEditingEndpoint(null);
          await waitForBenchmark(100);
          return;
        }
      case "scrub": {
        emitReplayBenchmarkEvent(
          "scrub_started",
          { request_count: action.targetsMs.length },
          { generation: mediaGeneration, required: true },
        );
        let settle = Promise.resolve();
        for (const targetMs of action.targetsMs) {
          settle = seekTo(millisecondsToReplayTick(targetMs, replayEnd), { reason: "benchmark" });
          await waitForBenchmark(action.intervalMs);
        }
        await withBenchmarkTimeout(settle, "scrub settle");
        emitReplayBenchmarkEvent(
          "scrub_settled",
          { request_count: action.targetsMs.length, media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, required: true },
        );
        return;
      }
      case "fullscreen":
        {
          let settle: Promise<void> | undefined;
          if (action.seekTargetMs !== undefined) {
            let markDispatched!: () => void;
            const dispatched = new Promise<void>((resolve) => {
              markDispatched = resolve;
            });
            settle = seekTo(millisecondsToReplayTick(action.seekTargetMs, replayEnd), {
              reason: "benchmark",
              onBenchmarkDispatch: markDispatched,
            });
            await withBenchmarkTimeout(dispatched, "layout seek dispatch");
          }
          if (action.enabled) enterFullscreen(true);
          else exitFullscreen(true);
          await waitForBenchmarkFrames();
          if (settle) await withBenchmarkTimeout(settle, "layout seek settle");
          return;
        }
      case "clip-mode":
        if (action.enabled) {
          const anchor = action.anchorMs === undefined
            ? controller.snapshot().media.tick ?? replayPosition()
            : millisecondsToReplayTick(action.anchorMs, replayEnd);
          setClipRange(clipRangeForAnchor(anchor));
        } else {
          cancelClipMode();
        }
        emitReplayBenchmarkEvent(
          "clip_mode_applied",
          { enabled: action.enabled, anchor_ms: action.anchorMs ?? null },
          { generation: mediaGeneration, required: true },
        );
        return;
    }
  };

  const runBenchmarkScenario = async () => {
    const observer = replayBenchmarkObserver();
    if (
      !observer ||
      benchmarkScenarioStarted ||
      mediaState() !== "ready" ||
      canPlayGeneration !== mediaGeneration ||
      firstPresentedGeneration !== mediaGeneration
    ) {
      return;
    }
    benchmarkScenarioStarted = true;
    if (benchmarkMediaReadyTimerId !== undefined) {
      window.clearTimeout(benchmarkMediaReadyTimerId);
      benchmarkMediaReadyTimerId = undefined;
    }
    try {
      const scenario = observer.scenario();
      const actions = buildScenarioActions(scenario, durationMs);
      for (const action of actions) await runBenchmarkAction(action);
      controller.pause();
      await updateMetrics();
      const quality = controller.sampleMetrics().quality;
      const completionKind =
        scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
          ? "viewer_cycle_completed"
          : "scenario_completed";
      const completionPayload = {
        kind: scenario.kind,
        media_time_ms: controller.snapshot().media.browserSeconds * 1_000,
        ready_state: controller.snapshot().media.readyState,
        network_state: controller.snapshot().media.networkState,
        playback_rate: controller.snapshot().rate.applied,
        total_frames: quality?.totalFrames ?? null,
        dropped_frames: quality?.droppedFrames ?? null,
        server_metrics: serverMetrics(),
      };
      await releaseBenchmarkMedia();
      observer.emit(completionKind, completionPayload, {
        generation: mediaGeneration,
        required: true,
      });
      benchmarkScenarioFinished = true;
      if (
        scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
      ) {
        window.dispatchEvent(new CustomEvent(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT));
        return;
      }
      await observer.complete("complete", null, { action_count: actions.length });
    } catch (error) {
      if (benchmarkAbortController.signal.aborted || disposed) return;
      const reason = error instanceof Error ? error.message : String(error);
      observer.emit(
        "scenario_failed",
        { reason },
        { generation: mediaGeneration, required: true },
      );
      await releaseBenchmarkMedia().catch(() => undefined);
      benchmarkScenarioFinished = true;
      await observer.complete("failed", reason);
    }
  };

  let benchmarkFullscreenEventRequired = false;

  const enterFullscreen = (required = false) => {
    benchmarkFullscreenEventRequired ||= required;
    emitReplayBenchmarkEvent(
      "fullscreen_requested",
      { enabled: true },
      { generation: mediaGeneration, required },
    );
    setIsFullscreen(true);
  };

  const exitFullscreen = (required = false) => {
    benchmarkFullscreenEventRequired ||= required;
    emitReplayBenchmarkEvent(
      "fullscreen_requested",
      { enabled: false },
      { generation: mediaGeneration, required },
    );
    setIsFullscreen(false);
  };

  let previousFullscreen = isFullscreen();
  createEffect(() => {
    const fullscreen = isFullscreen();
    document.body.classList.toggle("replay-fullscreen", fullscreen);
    if (fullscreen !== previousFullscreen) {
      previousFullscreen = fullscreen;
      emitReplayBenchmarkEvent(
        "fullscreen_applied",
        { enabled: fullscreen, media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
        { generation: mediaGeneration, required: benchmarkFullscreenEventRequired },
      );
      benchmarkFullscreenEventRequired = false;
    }
  });

  onMount(() => {
    if (!primaryVideo) return;
    controller = createPlaybackController(createHtmlVideoPlaybackAdapter(primaryVideo), { generation: mediaGeneration, eventSink: onPlaybackEvent });
    primaryVideo = undefined;
    decoderDiagnostics = createPlaybackDiagnostics((value) => {
      setDecoder(value);
      emitReplayBenchmarkEvent("decoder_observation", { status: value.status, reason: value.reason,
        decoder_name: value.decoder_name, platform_decoder: value.platform_decoder, runtime: value.runtime,
        protocol: value.protocol, associated: value.associated, transitions: value.transitions },
        { generation: value.generation ?? mediaGeneration, required: true });
    });
    const unsubscribe = controller.subscribe(syncSnapshot);
    const unsubscribeFrames = controller.subscribeFrames((tick, authority) => {
      if (authority === "rvfc") setPresentedPosition(tick);
      if (!editingEndpoint()) {
        const range = clipRange();
        setReplayPosition((range ? clamp(tick, clipStartTick(range), clipEndTick(range)) : tick) as ReplayTick);
      }
    });
    const openMedia = () => {
      if (disposed || !props.probe.video_url) return;
      void controller.open({ url: props.probe.video_url, mediaId: mediaTimeline.mediaId, timeline: mediaTimeline });
      const initial = clipRange();
      if (initial) { controller.setLoopRange(initial); void seekTo(clipStartTick(initial)); }
    };
    // Targeted production decoder checks subscribe before opening their media.
    // Ordinary user opening remains independent of diagnostic readiness.
    if (replayBenchmarkObserver()) void decoderDiagnostics.ready.then(openMedia);
    else openMedia();
    emitReplayBenchmarkEvent("viewer_mounted", { duration_ms: durationMs,
      frame_rate_numerator: mediaTimeline.video.frameRate.numerator.toString(),
      frame_rate_denominator: mediaTimeline.video.frameRate.denominator.toString(), has_video_url: Boolean(props.probe.video_url) },
      { generation: mediaGeneration, required: true });
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const target = event.target;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target instanceof HTMLSelectElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      if (event.key === " ") {
        if (target instanceof HTMLButtonElement) return;
        event.preventDefault();
        if (!event.repeat) void togglePlayback();
        return;
      }
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        if (event.repeat) return;
        const direction = event.key === "ArrowLeft" ? -1 : 1;
        setEditingEndpoint(null);
        seekTo(
          clamp(
            replayPosition() +
              direction * (clipRange() ? 5 : 15) * REPLAY_TICKS_PER_SECOND,
            0,
            replayEnd,
          ) as ReplayTick,
        );
        return;
      }
      if (event.repeat) return;
      if (event.key === "Escape" && isFullscreen()) {
        event.preventDefault();
        exitFullscreen();
        return;
      }
      if (event.key === "Escape" && clipRange()) {
        event.preventDefault();
        cancelClipMode();
        return;
      }
      if (event.key.toLowerCase() === "f") {
        event.preventDefault();
        setIsFullscreen((fullscreen) => !fullscreen);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    metricsIntervalId = window.setInterval(() => void updateMetrics(), 1_000);
    const observer = replayBenchmarkObserver();
    if (observer && props.probe.video_url) {
      benchmarkMediaReadyTimerId = window.setTimeout(() => {
        if (benchmarkScenarioStarted || benchmarkScenarioFinished || disposed) return;
        benchmarkScenarioFinished = true;
        const reason = "media readiness timed out";
        observer.emit("scenario_failed", { phase: "media_readiness", reason }, { generation: mediaGeneration, required: true });
        void releaseBenchmarkMedia().then(() => observer.complete("failed", reason));
      }, BENCHMARK_MEDIA_READY_TIMEOUT_MS);
    }
    onCleanup(() => {
      disposed = true;
      if (observer && !benchmarkScenarioFinished) {
        benchmarkScenarioFinished = true;
        void observer.complete("failed", "viewer disposed before benchmark completion");
      }
      benchmarkAbortController.abort();
      dispatchObservers.clear();
      unsubscribe(); unsubscribeFrames(); controller.dispose();
      decoderDiagnostics?.dispose();
      window.removeEventListener("keydown", onKeyDown);
      document.body.classList.remove("replay-fullscreen");
      clearEndpointHold();
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
      if (benchmarkMediaReadyTimerId !== undefined) window.clearTimeout(benchmarkMediaReadyTimerId);
    });
  });

  return (
    <section class={styles.playbackSurface} data-testid="viewer-surface">
      <div class={styles.viewerTopline}>
        <button class={styles.backButton} type="button" onClick={props.onBack}>
          <span aria-hidden="true">←</span> GAMES
        </button>
        <span class={styles.replayLabel}><span aria-hidden="true">▶</span> REPLAY</span>
      </div>

      <header class={styles.gameHeader}>
        <div class={styles.gameIdentity}>
          <p>LOCAL RECORDING / {props.probe.game.game_mode}</p>
          <h1>{props.probe.game.champion}</h1>
          <span>{props.probe.local_player_name ?? "LOCAL PLAYER"}</span>
        </div>
        <dl>
          <div>
            <dt>FINAL K / D / A</dt>
            <dd>
              {props.probe.game.kills} / {props.probe.game.deaths} / {props.probe.game.assists}
            </dd>
          </div>
          <div>
            <dt>VIDEO</dt>
            <dd>{formatDuration(durationMs)}</dd>
          </div>
          <div>
            <dt>CAPTURED</dt>
            <dd>{formatDate(props.probe.game.recorded_at, true)}</dd>
          </div>
        </dl>
      </header>

      <div class={styles.replayView} aria-label="Replay" data-testid="replay-panel">
        <div class={styles.windowedGrid}>
          <div
            classList={{
              [styles.videoFrame]: true,
              [styles.videoFrameFullscreen]: isFullscreen(),
            }}
            data-testid="video-frame"
            data-fullscreen={isFullscreen()}
          >
            <video
              ref={(element) => { primaryVideo = element; }}
              playsinline
              preload="auto"
              aria-label={`${props.probe.game.champion} replay video`}
              onClick={() => void togglePlayback()}
              onDblClick={(event) => {
                event.preventDefault();
                enterFullscreen();
              }}
            />
            <Show
              when={
                !props.probe.video_url ||
                mediaState() === "recovering" ||
                mediaState() === "degraded"
              }
            >
              <div
                classList={{
                  [styles.previewPlaceholder]: true,
                  [styles.previewPlaceholderInteractive]: mediaState() === "degraded",
                }}
              >
                <span class={styles.previewGlyph} aria-hidden="true">LR</span>
                <strong>
                  {mediaState() === "recovering"
                    ? "RECOVERING PREVIEW"
                    : mediaState() === "degraded"
                      ? "PREVIEW UNAVAILABLE"
                      : "INTERACTIVE PREVIEW"}
                </strong>
                <span>
                  {mediaState() === "recovering"
                    ? "Reloading the local recording without leaving this replay."
                    : mediaState() === "degraded"
                      ? mediaError() ?? "The local video preview is temporarily unavailable."
                    : "Timeline data is active; local video attaches in the desktop app."}
                </span>
                <Show when={mediaState() === "degraded"}>
                  <button type="button" onClick={retryPreview}>RETRY PREVIEW</button>
                </Show>
              </div>
            </Show>
            <Show when={playback()?.activationRequired}>
              <button type="button" onClick={() => void controller.play()}>Click to resume playback</button>
            </Show>
            <Show when={!isFullscreen()}>
              <div class={styles.videoClock}>
                <span>{beforeGameStart() ? "PRE-GAME" : "GAME TIME"}</span>
                <strong>{beforeGameStart() ? "--:--" : formatDuration(gameClockSecond() * 1_000)}</strong>
              </div>
            </Show>
            <Show when={isFullscreen()}>
              <FullscreenOverlay
                champion={props.probe.game.champion}
                localPlayerName={props.probe.local_player_name}
                events={visibleEvents()}
                mediaTimeline={mediaTimeline}
                replayTick={replayPosition()}
                presentedTick={presentedPosition()}
                gameTick={currentGameTick()}
                beforeGameStart={beforeGameStart()}
                currentKda={currentKda()}
                isPlaying={isPlaying()}
                mediaAvailable={Boolean(props.probe.video_url) && mediaState() === "ready"}
                participants={props.probe.participants}
                selectedPlayers={selectedPlayers()}
                clipRange={clipRange()}
                onPlayerToggle={togglePlayerFilter}
                onPlayerClear={() => setSelectedPlayers([])}
                onTogglePlayback={() => void togglePlayback()}
                onSeek={seekTo}
                onActivateClip={activateClip}
                onEventSelect={selectEvent}
                onClipEndpointEditStart={beginClipEndpointEdit}
                onClipEndpointPreview={(endpoint, requestedTick) => {
                  updateClipEndpoint(endpoint, requestedTick);
                }}
                onClipEndpointEditFinish={finishClipEndpointEdit}
                onClipEndpointKeyDown={handleClipEndpointKeyDown}
                onClipEndpointKeyUp={handleClipEndpointKeyUp}
                onClipEndpointBlur={handleClipEndpointBlur}
                onExportClip={exportSelectedClip}
                onCancelClip={cancelClipMode}
                onExit={() => exitFullscreen()}
              />
            </Show>
          </div>

          <Show when={!isFullscreen()}>
          <aside class={styles.sidePanel} aria-label="Synchronized game data">
            <section class={styles.liveStats} data-testid="live-stats">
              <div class={styles.panelLabel}>
                <span>LIVE STATE</span>
                <i aria-hidden="true" />
              </div>
              <div class={styles.championBlock}>
                <span class={styles.championMonogram} aria-hidden="true">
                  {props.probe.game.champion.slice(0, 2).toUpperCase()}
                </span>
                <div>
                  <p>CHAMPION</p>
                  <strong>{props.probe.game.champion}</strong>
                </div>
              </div>
              <dl class={styles.liveStatGrid}>
                <div class={styles.kdaStat}>
                  <dt>K / D / A</dt>
                  <dd data-testid="current-kda">
                    {currentKda()?.kills ?? 0}
                    <span>/</span>
                    {currentKda()?.deaths ?? 0}
                    <span>/</span>
                    {currentKda()?.assists ?? 0}
                  </dd>
                </div>
                <div>
                  <dt>CS</dt>
                  <dd data-testid="current-cs">{currentPlayer()?.cs ?? 0}</dd>
                </div>
                <div>
                  <dt>LEVEL</dt>
                  <dd data-testid="current-level">{currentPlayer()?.level ?? 1}</dd>
                </div>
                <div>
                  <dt>GAME CLOCK</dt>
                  <dd>{beforeGameStart() ? "PRE" : formatDuration(gameClockSecond() * 1_000)}</dd>
                </div>
              </dl>
            </section>

            <ChampionFilter
              participants={props.probe.participants}
              selectedPlayers={selectedPlayers()}
              localPlayerName={props.probe.local_player_name}
              mode="panel"
              onToggle={togglePlayerFilter}
              onClear={() => setSelectedPlayers([])}
            />
          </aside>
          </Show>
        </div>

        <Show when={!isFullscreen()}>
        <section class={styles.timelinePanel} aria-label="Replay timeline">
          <div
            ref={windowedRail}
            class={styles.scrubber}
            role="slider"
            tabIndex={0}
            aria-label="Replay position"
            aria-valuemin={0}
            aria-valuemax={Math.floor(durationMs / 1_000)}
            aria-valuenow={videoSecond()}
            aria-valuetext={`${formatDuration(videoSecond() * 1_000)} of ${formatDuration(durationMs)}`}
            onPointerDown={seekFromRail}
            onKeyDown={handleRailKey}
            data-testid="scrubber"
          >
            <span class={styles.scrubberTrack} />
            <span class={styles.scrubberFill} style={`width:${progress()}%`} />
            <Show when={clipRange()}>
              <span
                class={styles.clipSelection}
                style={`left:${clipStartPosition()}%;width:${clipEndPosition() - clipStartPosition()}%`}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleStart}`}
                style={`left:${clipStartPosition()}%`}
                type="button"
                aria-label={`Clip starts at ${formatDuration(replayTickToMilliseconds(clipStartTick(clipRange()!)))}`}
                onPointerDown={(event) => beginClipDrag(event, "start", windowedRail)}
                onKeyDown={(event) => handleClipEndpointKeyDown(event, "start")}
                onKeyUp={(event) => handleClipEndpointKeyUp(event, "start")}
                onBlur={() => handleClipEndpointBlur("start")}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleEnd}`}
                style={`left:${clipEndPosition()}%`}
                type="button"
                aria-label={`Clip ends at ${formatDuration(replayTickToMilliseconds(clipEndTick(clipRange()!)))}`}
                onPointerDown={(event) => beginClipDrag(event, "end", windowedRail)}
                onKeyDown={(event) => handleClipEndpointKeyDown(event, "end")}
                onKeyUp={(event) => handleClipEndpointKeyUp(event, "end")}
                onBlur={() => handleClipEndpointBlur("end")}
              />
            </Show>
            <span class={styles.scrubberHead} style={`left:${progress()}%`} />
            <div class={styles.markers}>
              <For each={markerPositions()}>
                {(marker) => (
                  <button
                    class={`${styles.marker} ${styles[`marker${marker.tone}`]}`}
                    style={`left:${marker.position}%`}
                    type="button"
                    aria-label={`Seek to ${eventTitle(marker.event)} at ${formatDuration(gameTickToMilliseconds(marker.event.game_tick))}`}
                    title={`${eventTitle(marker.event)} · ${formatDuration(gameTickToMilliseconds(marker.event.game_tick))}`}
                    data-event-index={marker.index}
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={(event) => {
                      event.stopPropagation();
                      selectEvent(marker.event);
                    }}
                  />
                )}
              </For>
            </div>
          </div>
          <div class={styles.controlsRow}>
            <button
              class={styles.playButton}
              type="button"
              disabled={!props.probe.video_url || mediaState() !== "ready"}
              onClick={() => void togglePlayback()}
              aria-label={
                isPlaying() ? "Pause replay" : clipRange() ? "Preview selected clip" : "Play replay"
              }
            >
              <span aria-hidden="true">{isPlaying() ? "Ⅱ" : "▶"}</span>
              {isPlaying() ? "PAUSE" : clipRange() ? "PREVIEW CLIP" : "PLAY"}
            </button>
            <Show
              when={clipRange()}
              fallback={
                <button class={styles.clipButton} type="button" onClick={() => activateClip()}>
                  <span aria-hidden="true">✦</span> CLIP
                </button>
              }
            >
              <div class={styles.clipActions}>
                <button class={styles.cancelClipButton} type="button" onClick={cancelClipMode}>
                  CANCEL
                </button>
                <button class={styles.exportClipButton} type="button" onClick={exportSelectedClip}>
                  EXPORT CLIP <span aria-hidden="true">→</span>
                </button>
              </div>
            </Show>
            <div class={styles.timeReadout} data-testid="time-readout">
              <strong>{formatDuration(videoSecond() * 1_000)}</strong>
              <span>/</span>
              <span>{formatDuration(durationMs)}</span>
            </div>
            <span class={styles.controlDivider} />
            <div class={styles.nowPlaying}>
              <span>NOW</span>
              <strong>
                {activeEvent()
                  ? eventTitle(activeEvent()!)
                  : selectedPlayers().length > 0
                    ? "NO MATCHING EVENT"
                    : "LOADING SCREEN"}
              </strong>
            </div>
            <button
              class={styles.fullscreenButton}
              type="button"
              onClick={() => enterFullscreen()}
              title="Open fullscreen replay"
            >
              <span aria-hidden="true">⛶</span> FULLSCREEN
            </button>
          </div>
        </section>
        </Show>

        <Show when={!isFullscreen()}>
          <details class={styles.diagnostics}>
            <summary>
              <span>PLAYBACK DIAGNOSTICS</span>
              <strong>{presentedFps().toFixed(1)} FPS</strong>
            </summary>
            <dl>
              <div>
                <dt>DECODER</dt>
                <dd title={decoder()?.reason}>{decoder()?.status ?? "unknown"}: {decoder()?.decoder_name ?? "unobserved"}</dd>
              </div>
              <div>
                <dt>CLOCK</dt>
                <dd>{clockSource()}</dd>
              </div>
              <div>
                <dt>DROPPED</dt>
                <dd>{droppedFrames()} / {totalFrames()}</dd>
              </div>
              <div>
                <dt>LAST SEEK</dt>
                <dd>{seekLatencyMs() === null ? "—" : `${seekLatencyMs()!.toFixed(0)} ms`}</dd>
              </div>
              <div>
                <dt>STREAMED</dt>
                <dd>{formatBytes(serverMetrics().response_bytes)}</dd>
              </div>
              <div>
                <dt>RANGES</dt>
                <dd>{serverMetrics().range_requests}</dd>
              </div>
              <div>
                <dt>STREAMS</dt>
                <dd>{serverMetrics().completed_streams} OK / {serverMetrics().cancelled_streams} CANCEL</dd>
              </div>
              <div>
                <dt>JS HEAP</dt>
                <dd>{heapBytes() === null ? "—" : formatBytes(heapBytes()!)}</dd>
              </div>
              <div>
                <dt>ACTIVE EVENT</dt>
                <dd>{activeEvent()?.event_type ?? "—"}</dd>
              </div>
              <div>
                <dt>INDEXED</dt>
                <dd>{visibleEvents().length} / {events.length} EVENTS</dd>
              </div>
              <div>
                <dt>MEDIA</dt>
                <dd>{mediaState().toUpperCase()}</dd>
              </div>
              <div>
                <dt>SEEK QUEUE</dt>
                <dd>{seekQueueLabel()}</dd>
              </div>
              <div>
                <dt>RECOVERIES</dt>
                <dd>{recoveryCount()}</dd>
              </div>
              <div>
                <dt>LAST MEDIA EVENT</dt>
                <dd title={lastDiagnostic()?.detail}>
                  {lastDiagnostic()?.kind ?? "—"}
                </dd>
              </div>
            </dl>
          </details>
        </Show>
      </div>

    </section>
  );
}

export default ViewerScreen;
