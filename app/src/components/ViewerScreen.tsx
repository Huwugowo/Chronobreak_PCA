import {
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
  loadReplayDescriptor,
  assertReplayProbeMatches,
  loadServerMetrics,
} from "../api";
import {
  buildScenarioActions,
  emitReplayBenchmarkEvent,
  REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT,
  replayBenchmarkObserver,
} from "../benchmark";
import { createViewerBenchmarkActions } from "../viewerBenchmarkActions";
import { formatDate, formatDuration } from "../format";
import type {
  ClipDraft,
  ClipRange,
  KdaTimelinePoint,
  GameSummary,
  ReplayDescriptor,
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
  moveClipEndpoint,
  nearestIndexAt,
  replayTickAtFrameBoundary,
  timelineValue,
} from "../viewerUtils";

import {
  wholeTimelineViewport,
  type TimelineViewport,
} from "../replayTimelineGeometry";
import { createPlaybackController, type PlaybackController, type PlaybackEvent, type PlaybackSnapshot } from "../playbackController";
import { createHtmlVideoPlaybackAdapter } from "../htmlVideoPlaybackAdapter";
import { createPlaybackDiagnostics } from "../playbackDiagnostics";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./ViewerScreen.module.css";
import { libraryError, type ReplayRead } from "../libraryController";

type Props = {
  ddragonAssetBaseUrl?: string | null;
  gameTimestamp: string;
  game: GameSummary;
  readReplay: ReplayRead;
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

function ViewerScreen(props: Props) {
  const [descriptor, { refetch }] = createResource(() => props.gameTimestamp,
    id => props.readReplay(token => loadReplayDescriptor(id, token)));
  let failureReported = false;

  createEffect(() => {
    const error = descriptor.error;
    const observer = replayBenchmarkObserver();
    if (!error || !observer || failureReported) return;
    failureReported = true;
    const reason = error instanceof Error ? error.message : String(error);
    observer.emit("scenario_failed", { phase: "replay_descriptor", reason }, { required: true });
    void observer.complete("failed", reason, { phase: "replay_descriptor" });
  });

  return (
    <div class={styles.viewerScreen}>
      <Switch>
        <Match when={descriptor.loading}>
          <section class={styles.viewerState} aria-live="polite">
            Loading recording...
          </section>
        </Match>
        <Match when={descriptor.error}>
          <section class={styles.viewerState} role="alert">
            <strong>Recording could not be opened.</strong>
            <span>{libraryError(descriptor.error)}</span>
            <button type="button" onClick={() => void refetch()}>Retry recording</button>
            <button type="button" onClick={props.onBack}>
              Return to games
            </button>
          </section>
        </Match>
        <Match when={descriptor()}>
          {(loaded) => (
            <PlaybackSurface
              {...props}
              descriptor={loaded()}
            />
          )}
        </Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: Props & { descriptor: ReplayDescriptor }) {
  const mediaTimeline = props.descriptor.media_timeline;
  const replayEnd = mediaTimeline.video.replayEnd;
  const durationMs = replayTickToMilliseconds(replayEnd);
  let primaryVideo: HTMLVideoElement | undefined;
  let controller!: PlaybackController;
  let decoderDiagnostics: ReturnType<typeof createPlaybackDiagnostics> | undefined;
  let metricsIntervalId: number | undefined;
  let benchmarkMediaReadyTimerId: number | undefined;
  let mediaGeneration = replayBenchmarkObserver()?.nextMediaGeneration() ?? 0;
  let disposed = false;
  let endpointHold: EndpointHoldState | undefined;
  let firstPresentedGeneration = -1;
  let canPlayGeneration = -1;
  let benchmarkScenarioStarted = false;
  let benchmarkScenarioFinished = false;
  const benchmarkAbortController = new AbortController();
  const dispatchObservers = new Map<string, () => void>();

  const [detailsStarted, setDetailsStarted] = createSignal(false);
  const [detailsResource, { refetch: retryDetails }] = createResource(
    () => detailsStarted() && props.gameTimestamp,
    async id => assertReplayProbeMatches(props.descriptor,
      await props.readReplay(token => loadPlaybackProbe(id, token))),
  );
  const detailsError = () => detailsResource.error;
  const details = () => detailsError() ? undefined : detailsResource();
  let detailsPaint: number | undefined;
  let payloadReported = false;
  let detailsFailureReported = false;
  onMount(() => {
    detailsPaint = requestAnimationFrame(() => {
      detailsPaint = requestAnimationFrame(() => { if (!disposed) setDetailsStarted(true); });
    });
  });
  onCleanup(() => { if (detailsPaint !== undefined) cancelAnimationFrame(detailsPaint); });
  createEffect(() => {
    if (detailsError()) {
      const observer = replayBenchmarkObserver();
      if (observer && !detailsFailureReported) {
        detailsFailureReported = true;
        const reason = libraryError(detailsError());
        observer.emit("scenario_failed", { phase: "playback_probe", reason }, { required: true });
        void observer.complete("failed", reason, { phase: "playback_probe" });
      }
      return;
    }
    const loaded = details();
    if (!loaded || payloadReported) return;
    payloadReported = true;
    emitReplayBenchmarkEvent("playback_payload_ready", {
      event_count: loaded.events.length, participant_count: loaded.participants.length,
      duration_ms: loaded.game.duration_ms,
    }, { required: true });
    queueMicrotask(() => { if (!disposed) void runBenchmarkScenario(); });
  });

  const events = createMemo<MappedViewerEvent[]>(() => (details()?.events ?? [])
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick));
  const playerTimeline = createMemo<MappedPlayerTimelinePoint[]>(() => (details()?.player_timeline ?? [])
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick));
  const kdaTimeline = createMemo<MappedKdaTimelinePoint[]>(() => (details()?.kda_timeline ?? [])
    .filter(hasReplayTick)
    .sort((left, right) => left.replay_tick - right.replay_tick));

  const [isFullscreen, setIsFullscreen] = createSignal(false);
  const [selectedPlayers, setSelectedPlayers] = createSignal<readonly string[]>([]);
  const [presentedPosition, setPresentedPosition] = createSignal<ReplayTick | null>(null);
  const [playback, setPlayback] = createSignal<PlaybackSnapshot>();
  const [replayPosition, setReplayPosition] = createSignal<ReplayTick>(0 as ReplayTick);
  const [timelineViewport, setTimelineViewport] = createSignal<TimelineViewport>(
    wholeTimelineViewport(replayEnd as ReplayTick),
  );
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [mediaState, setMediaState] = createSignal<MediaPreviewState>("loading");
  const [mediaError, setMediaError] = createSignal<string | null>(null);
  const [editingEndpoint, setEditingEndpoint] = createSignal<ClipEndpoint | null>(null);
  const [clockSource, setClockSource] = createSignal<"VIDEO FRAME" | "ANIMATION FRAME">(
    "VIDEO FRAME",
  );
  const [serverMetrics, setServerMetrics] = createSignal<ServerMetrics>({
    requests: 0,
    range_requests: 0,
    response_bytes: 0,
    completed_streams: 0,
    cancelled_streams: 0,
    active_connections: 0,
    peak_connections: 0,
    rejected_connections: 0,
    rejected_requests: 0,
    active_streams: 0,
    peak_streams: 0,
  });
  const initialClipRange = (): ClipRange | null => {
    const draft = props.initialClipDraft;
    if (
      !draft ||
      draft.gameTimestamp !== props.game.timestamp ||
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
  const replayTickAtGameZero = createMemo(() => {
    const point = events()[0] ?? playerTimeline()[0];
    return point ? BigInt(point.replay_tick) - BigInt(point.game_tick) * 48n : null;
  });
  const currentGameTick = createMemo(() =>
    replayTickAtGameZero() === null
      ? "0"
      : ((BigInt(presentedPosition() ?? 0) - replayTickAtGameZero()!) / 48n).toString(),
  );
  const beforeGameStart = createMemo(
    () => presentedPosition() === null || replayTickAtGameZero() === null || BigInt(currentGameTick()) < 0n,
  );
  const visibleEvents = createMemo(() => {
    const selected = selectedPlayers();
    if (selected.length === 0) return events();
    return events().filter((event) =>
      selected.some((player) => eventInvolvesPlayer(event, player)),
    );
  });
  const currentKda = createMemo<KdaTimelinePoint | undefined>(() =>
    presentedPosition() === null ? undefined : timelineValue(kdaTimeline(), presentedPosition()!),
  );
  const togglePlayerFilter = (summonerName: string) => {
    setSelectedPlayers((selected) =>
      selected.includes(summonerName)
        ? selected.filter((player) => player !== summonerName)
        : [...selected, summonerName],
    );
  };

  const syncSnapshot = (value: PlaybackSnapshot) => {
    setPlayback(value);
    if (value.readiness === "disposed") decoderDiagnostics?.dispose();
    else decoderDiagnostics?.bind(value, mediaTimeline);
    mediaGeneration = value.generation;
    setMediaState(value.readiness === "closed" || value.readiness === "disposed" ? "degraded" : value.readiness);
    setIsPlaying(value.desiredPlaying && !value.media.paused);
    setMediaError(value.error);
    setClockSource(value.authority === "rvfc" ? "VIDEO FRAME" : "ANIMATION FRAME");
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
    defaultClipRange(anchor, mediaTimeline, events(), anchorEvent);

  const activateClip = () => {
    const kills = events().filter(
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
      gameTimestamp: props.game.timestamp,
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

  const togglePlayback = async () => {
    if (!props.descriptor.video_url || mediaState() !== "ready") return;
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

  const runBenchmarkAction = createViewerBenchmarkActions({
    getController: () => controller,
    getGeneration: () => mediaGeneration,
    isAborted: () => benchmarkAbortController.signal.aborted,
    mediaTimeline, replayEnd, events, clipRange, setClipRange, setEditingEndpoint,
    clipRangeForAnchor, clipStartTick, clipEndTick, replayPosition, clockSource,
    playNativeVideo, seekTo, waitForBenchmark, waitForBenchmarkFrames,
    withBenchmarkTimeout, cancelClipMode,
    enterFullscreen: required => enterFullscreen(required),
    exitFullscreen: required => exitFullscreen(required),
  });

  const runBenchmarkScenario = async () => {
    const observer = replayBenchmarkObserver();
    if (
      disposed || !observer ||
      !details() || detailsError() ||
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
      if (disposed || !props.descriptor.video_url) return;
      void controller.open({ url: props.descriptor.video_url, mediaId: mediaTimeline.mediaId, timeline: mediaTimeline });
      const initial = clipRange();
      if (initial) { controller.setLoopRange(initial); void seekTo(clipStartTick(initial)); }
    };
    // Targeted production decoder checks subscribe before opening their media.
    // Ordinary user opening remains independent of diagnostic readiness.
    if (replayBenchmarkObserver()) void decoderDiagnostics.ready.then(openMedia);
    else openMedia();
    emitReplayBenchmarkEvent("viewer_mounted", { duration_ms: durationMs,
      frame_rate_numerator: mediaTimeline.video.frameRate.numerator.toString(),
      frame_rate_denominator: mediaTimeline.video.frameRate.denominator.toString(), has_video_url: Boolean(props.descriptor.video_url) },
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
    if (observer && props.descriptor.video_url) {
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
      <header class={styles.gameHeader}>
        <div class={styles.gameIdentity}>
          <p>LOCAL RECORDING / {props.game.game_mode}</p>
          <h1>{props.game.champion}</h1>
          <span>{details()?.local_player_name ?? "LOCAL PLAYER"}</span>
        </div>
        <dl>
          <div>
            <dt>FINAL K / D / A</dt>
            <dd>
              {props.game.kills} / {props.game.deaths} / {props.game.assists}
            </dd>
          </div>
          <div>
            <dt>VIDEO</dt>
            <dd>{formatDuration(durationMs)}</dd>
          </div>
          <div>
            <dt>CAPTURED</dt>
            <dd>{formatDate(props.game.recorded_at, true)}</dd>
          </div>
        </dl>
      </header>

      <Show when={!details()}>
        <section role="status" data-testid="replay-details-state">
          {detailsError() ? "Replay details unavailable. Playback remains available." : "Loading replay details..."}
          <Show when={detailsError()}>
            <span>{libraryError(detailsError())}</span>
            <button type="button" onClick={() => void retryDetails()}>Retry replay details</button>
          </Show>
        </section>
      </Show>

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
              data-qb-primary-playback="true"
              ref={(element) => { primaryVideo = element; }}
              playsinline
              preload="auto"
              aria-label={`${props.game.champion} replay video`}
              onClick={() => void togglePlayback()}
              onDblClick={(event) => {
                event.preventDefault();
                enterFullscreen();
              }}
            />
            <Show
              when={
                !props.descriptor.video_url ||
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

            <FullscreenOverlay
                ddragonAssetBaseUrl={props.ddragonAssetBaseUrl ?? null}
                champion={props.game.champion}
                localPlayerName={details()?.local_player_name ?? null}
                events={visibleEvents()}
                mediaTimeline={mediaTimeline}
                replayTick={replayPosition()}
                timelineViewport={timelineViewport()}
                onTimelineViewportChange={setTimelineViewport}
                presentedTick={presentedPosition()}
                gameTick={currentGameTick()}
                beforeGameStart={beforeGameStart()}
                currentKda={currentKda()}
                isPlaying={isPlaying()}
                playback={playback()}
                onRate={(rate) => controller?.setRate(rate)}
                onMuted={(muted) => controller?.setMuted(muted)}
                onVolume={(volume) => controller?.setVolume(volume)}
                mediaAvailable={Boolean(props.descriptor.video_url) && mediaState() === "ready"}
                participants={(details()?.participants ?? [])}
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
                fullscreen={isFullscreen()}
                onFullscreenToggle={() =>
                  isFullscreen() ? exitFullscreen() : enterFullscreen()
                }
              />
          </div>

        </div>


      </div>

    </section>
  );
}

export default ViewerScreen;
