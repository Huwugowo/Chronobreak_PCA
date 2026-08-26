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
  isCurrentMediaGeneration,
  isLatestBenchmarkAction,
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
  alignClipRangeToFrames,
  clamp,
  clipEndpointPreviewMs,
  defaultClipRange,
  eventInvolvesPlayer,
  eventTitle,
  frameIndexAt,
  frameTimeMs,
  latestIndexAt,
  moveClipEndpoint,
  nearestIndexAt,
  timelineValue,
} from "../viewerUtils";
import ChampionFilter from "./ChampionFilter";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./ViewerScreen.module.css";

type Props = {
  gameTimestamp: string;
  onBack: () => void;
  initialClipDraft?: ClipDraft;
  onExportClip: (draft: ClipDraft) => void;
};

type ChromiumPerformance = Performance & {
  memory?: { usedJSHeapSize: number };
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

type ScheduledSeek = {
  generation: number;
  targetMs: number;
  reason: SeekReason;
  playAfter: boolean;
  benchmark?: {
    actionId: string;
    requestedAtMs: number;
    fromMs: number;
    settle: () => void;
    cancelled: boolean;
    onDispatch?: () => void;
  };
};

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
  startingFrame: number;
  animationFrameId: number;
};

const ENDPOINT_HOLD_THRESHOLD_MS = 200;
const SEEK_INTERVAL_MS = 100;
const SEEK_TIMEOUT_MS = 1_500;
const BENCHMARK_MEDIA_READY_TIMEOUT_MS = 30_000;
const MAX_DIAGNOSTIC_EVENTS = 50;
const UNTRACKED_SEEK_COMPLETION = Promise.resolve();

function ViewerScreen(props: Props) {
  const [probe] = createResource(() => props.gameTimestamp, loadPlaybackProbe);
  let payloadReported = false;
  let failureReported = false;

  createEffect(() => {
    const loaded = probe();
    if (!loaded || payloadReported) return;
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
  let video!: HTMLVideoElement;
  let windowedRail!: HTMLDivElement;
  let frameCallbackId: number | undefined;
  let animationFrameId: number | undefined;
  let metricsIntervalId: number | undefined;
  let seekDispatchTimerId: number | undefined;
  let seekTimeoutId: number | undefined;
  let recoveryTimerId: number | undefined;
  let benchmarkMediaReadyTimerId: number | undefined;
  let frameSampleStartedAt = performance.now();
  let lastSeekDispatchedAt = Number.NEGATIVE_INFINITY;
  let mediaGeneration = replayBenchmarkObserver()?.nextMediaGeneration() ?? 0;
  let recoveryTargetMs = 0;
  let inFlightSeek: ScheduledSeek | undefined;
  let pendingSeek: ScheduledSeek | undefined;
  let recoveryAttempts: number[] = [];
  let presentedFrames = 0;
  let disposed = false;
  let clipLoopSeekPending = false;
  let endpointHold: EndpointHoldState | undefined;
  let pendingBenchmarkPlayActionId: string | undefined;
  let pendingBenchmarkPauseActionId: string | undefined;
  let latestBenchmarkSeekActionId: string | undefined;
  let awaitingPresentedSeek: ScheduledSeek | undefined;
  let firstPresentedGeneration = -1;
  let canPlayGeneration = -1;
  let benchmarkScenarioStarted = false;
  let benchmarkScenarioFinished = false;
  let benchmarkEndpointSequence = 0;
  const benchmarkAbortController = new AbortController();

  const events = [...props.probe.events].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const playerTimeline = [...props.probe.player_timeline].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const kdaTimeline = [...props.probe.kda_timeline].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const lastEventTimeMs = events[events.length - 1]?.video_time_ms ?? 0;
  const durationMs = Math.max(props.probe.game.duration_ms, lastEventTimeMs, 1);

  const [isFullscreen, setIsFullscreen] = createSignal(false);
  const [selectedPlayers, setSelectedPlayers] = createSignal<readonly string[]>([]);
  const [videoTimeMs, setVideoTimeMs] = createSignal(0);
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
      draft.clipStartMs < 0 ||
      draft.clipEndMs > durationMs ||
      draft.clipEndMs - draft.clipStartMs < 5_000
    ) {
      return null;
    }
    return alignClipRangeToFrames(
      { startMs: draft.clipStartMs, endMs: draft.clipEndMs },
      durationMs,
      props.probe.recording_fps,
    );
  };
  const [clipRange, setClipRange] = createSignal<ClipRange | null>(initialClipRange());

  const progress = createMemo(() => clamp((videoTimeMs() / durationMs) * 100, 0, 100));
  const videoSecond = createMemo(() => Math.floor(videoTimeMs() / 1_000));
  const gameClockSecond = createMemo(() =>
    Math.max(0, Math.floor((videoTimeMs() - props.probe.game_start_video_offset_ms) / 1_000)),
  );
  const beforeGameStart = createMemo(
    () => videoTimeMs() < props.probe.game_start_video_offset_ms,
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
      position: clamp((event.video_time_ms / durationMs) * 100, 0, 100),
      tone: event.relation,
    })),
  );
  const clipStartPosition = createMemo(() =>
    clipRange() ? clamp((clipRange()!.startMs / durationMs) * 100, 0, 100) : 0,
  );
  const clipEndPosition = createMemo(() =>
    clipRange() ? clamp((clipRange()!.endMs / durationMs) * 100, 0, 100) : 100,
  );
  const activeEventIndex = createMemo(() => latestIndexAt(visibleEvents(), videoTimeMs()));
  const activeEvent = createMemo(() => {
    const index = activeEventIndex();
    return index < 0 ? undefined : visibleEvents()[index];
  });
  const currentPlayer = createMemo<PlayerTimelinePoint | undefined>(() =>
    timelineValue(playerTimeline, videoTimeMs()),
  );
  const currentKda = createMemo<KdaTimelinePoint | undefined>(() =>
    timelineValue(kdaTimeline, videoTimeMs()),
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

  const sampleFrame = () => {
    presentedFrames += 1;
    const now = performance.now();
    const elapsed = now - frameSampleStartedAt;
    if (elapsed >= 1_000) {
      setPresentedFps((presentedFrames * 1_000) / elapsed);
      frameSampleStartedAt = now;
      presentedFrames = 0;
    }
  };

  const observePresentedFrame = (presentedTimeMs: number) => {
    if (firstPresentedGeneration !== mediaGeneration) {
      firstPresentedGeneration = mediaGeneration;
      emitReplayBenchmarkEvent(
        "first_presented_frame",
        {
          media_time_ms: presentedTimeMs,
          ready_state: video.readyState,
          presentation_clock: clockSource(),
          authoritative: typeof video.requestVideoFrameCallback === "function",
        },
        { generation: mediaGeneration, required: true },
      );
      queueMicrotask(() => void runBenchmarkScenario());
    }
    const completed = awaitingPresentedSeek;
    const benchmark = completed?.benchmark;
    if (
      completed &&
      benchmark &&
      isCurrentMediaGeneration(completed.generation, mediaGeneration) &&
      isLatestBenchmarkAction(
        benchmark.actionId,
        latestBenchmarkSeekActionId,
        benchmark.cancelled,
      )
    ) {
      awaitingPresentedSeek = undefined;
      emitReplayBenchmarkEvent(
        "seek_presented",
        {
          target_ms: completed.targetMs,
          presented_media_time_ms: presentedTimeMs,
          target_error_ms: presentedTimeMs - completed.targetMs,
          request_to_presented_ms: performance.now() - benchmark.requestedAtMs,
          reason: completed.reason,
          presentation_clock: clockSource(),
          authoritative: typeof video.requestVideoFrameCallback === "function",
        },
        {
          generation: completed.generation,
          actionId: benchmark.actionId,
          required: true,
        },
      );
      benchmark.settle();
    }
  };

  const syncPresentedTime = (presentedTimeMs: number) => {
    if (editingEndpoint()) return;
    const range = clipRange();
    if (range && !video.paused && presentedTimeMs >= range.endMs) {
      if (!clipLoopSeekPending) {
        clipLoopSeekPending = true;
        seekTo(range.startMs, { reason: "clip-loop" });
      }
      return;
    }
    setVideoTimeMs(
      range
        ? clamp(presentedTimeMs, range.startMs, range.endMs)
        : clamp(presentedTimeMs, 0, durationMs),
    );
  };

  const onVideoFrame: VideoFrameRequestCallback = (_now, frame) => {
    if (disposed) return;
    const presentedTimeMs = frame.mediaTime * 1_000;
    observePresentedFrame(presentedTimeMs);
    syncPresentedTime(presentedTimeMs);
    sampleFrame();
    frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
  };

  const onAnimationFrame = () => {
    animationFrameId = undefined;
    const presentedTimeMs = video.currentTime * 1_000;
    if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) {
      observePresentedFrame(presentedTimeMs);
    }
    syncPresentedTime(presentedTimeMs);
    sampleFrame();
    if (!video.paused) animationFrameId = requestAnimationFrame(onAnimationFrame);
  };

  const updateMetrics = async () => {
    if (typeof video.getVideoPlaybackQuality === "function") {
      const quality = video.getVideoPlaybackQuality();
      setDroppedFrames(quality.droppedVideoFrames);
      setTotalFrames(quality.totalVideoFrames);
    }
    setHeapBytes((performance as ChromiumPerformance).memory?.usedJSHeapSize ?? null);
    try {
      setServerMetrics(await loadServerMetrics());
    } catch {
      // Diagnostics must never interrupt playback.
    }
  };

  const addDiagnostic = (kind: string, detail: string) => {
    setDiagnosticEvents((events) => [
      ...events.slice(-(MAX_DIAGNOSTIC_EVENTS - 1)),
      { at: Date.now(), kind, detail },
    ]);
  };

  const mediaErrorDescription = () => {
    const error = video.error;
    if (!error) return "The local preview encountered an unknown media error.";
    const label =
      error.code === MediaError.MEDIA_ERR_ABORTED
        ? "Playback aborted"
        : error.code === MediaError.MEDIA_ERR_NETWORK
          ? "Local stream error"
          : error.code === MediaError.MEDIA_ERR_DECODE
            ? "Decode error"
            : error.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED
              ? "Unsupported recording"
              : "Media error";
    return `${label} (${error.code})${error.message ? `: ${error.message}` : ""}`;
  };

  const describeTimeRanges = (ranges: TimeRanges) => {
    const values: string[] = [];
    for (let index = 0; index < Math.min(ranges.length, 3); index += 1) {
      values.push(`${ranges.start(index).toFixed(2)}-${ranges.end(index).toFixed(2)}`);
    }
    return values.length > 0 ? values.join(",") : "none";
  };

  const mediaSnapshot = () =>
    `time=${video.currentTime.toFixed(3)}; ready=${video.readyState}; network=${video.networkState}; buffered=${describeTimeRanges(video.buffered)}; seekable=${describeTimeRanges(video.seekable)}`;

  const clearSeekDispatchTimer = () => {
    if (seekDispatchTimerId === undefined) return;
    window.clearTimeout(seekDispatchTimerId);
    seekDispatchTimerId = undefined;
  };

  const clearSeekTimeout = () => {
    if (seekTimeoutId === undefined) return;
    window.clearTimeout(seekTimeoutId);
    seekTimeoutId = undefined;
  };

  const resetSeekScheduler = () => {
    clearSeekDispatchTimer();
    clearSeekTimeout();
    inFlightSeek?.benchmark?.settle();
    pendingSeek?.benchmark?.settle();
    awaitingPresentedSeek?.benchmark?.settle();
    inFlightSeek = undefined;
    pendingSeek = undefined;
    awaitingPresentedSeek = undefined;
    latestBenchmarkSeekActionId = undefined;
    clipLoopSeekPending = false;
    setSeekQueueLabel("IDLE");
  };

  const handlePlayRejection = (reason?: unknown) => {
    addDiagnostic(
      "play-rejected",
      `${reason instanceof Error ? reason.name : "unknown"}; mediaError=${video.error?.code ?? "none"}`,
    );
    // Pausing for a new endpoint edit intentionally rejects an outstanding
    // Chromium play() promise with AbortError. A MediaError is the source of
    // truth for an actual loading or decoding failure.
    if (video.error !== null) attemptRecovery(mediaErrorDescription());
  };

  const playNativeVideo = async (propagateFailure = false) => {
    if (!props.probe.video_url || mediaState() !== "ready") {
      if (propagateFailure) throw new Error("media is not ready for playback");
      return;
    }
    try {
      await video.play();
    } catch (error) {
      handlePlayRejection(error);
      if (propagateFailure) throw error;
    }
  };

  const cancelBenchmarkSeek = (
    request: ScheduledSeek | undefined,
    replacementTargetMs: number,
    phase: string,
    kind = "action_cancelled_superseded",
  ) => {
    const benchmark = request?.benchmark;
    if (!request || !benchmark || benchmark.cancelled) return;
    benchmark.cancelled = true;
    emitReplayBenchmarkEvent(
      kind,
      {
        phase,
        target_ms: request.targetMs,
        replacement_target_ms: replacementTargetMs,
      },
      {
        generation: request.generation,
        actionId: benchmark.actionId,
        required: true,
      },
    );
    benchmark.settle();
  };

  const dispatchPendingSeek = () => {
    clearSeekDispatchTimer();
    if (inFlightSeek || !pendingSeek || mediaState() !== "ready" || video.readyState === 0) {
      return;
    }
    const delay = Math.max(0, lastSeekDispatchedAt + SEEK_INTERVAL_MS - performance.now());
    if (delay > 0) {
      seekDispatchTimerId = window.setTimeout(dispatchPendingSeek, delay);
      return;
    }

    const request = pendingSeek;
    pendingSeek = undefined;
    const benchmark = request.benchmark;
    const frameToleranceMs = 500 / props.probe.recording_fps;
    if (Math.abs(video.currentTime * 1_000 - request.targetMs) <= frameToleranceMs) {
      addDiagnostic("seek-deduped", `${request.reason}@${request.targetMs.toFixed(1)}`);
      if (benchmark && !benchmark.cancelled) {
        emitReplayBenchmarkEvent(
          "seek_deduped",
          {
            target_ms: request.targetMs,
            actual_media_time_ms: video.currentTime * 1_000,
            reason: request.reason,
          },
          { generation: request.generation, actionId: benchmark.actionId, required: true },
        );
      }
      if (request.playAfter && request.generation === mediaGeneration) void playNativeVideo();
      benchmark?.settle();
      setSeekQueueLabel("IDLE");
      dispatchPendingSeek();
      return;
    }

    inFlightSeek = request;
    setSeekQueueLabel("1 ACTIVE");
    lastSeekDispatchedAt = performance.now();
    addDiagnostic("seek-start", `${request.reason}@${request.targetMs.toFixed(1)}`);
    if (benchmark && !benchmark.cancelled) {
      const distanceMs = request.targetMs - benchmark.fromMs;
      emitReplayBenchmarkEvent(
        "seek_dispatched",
        {
          target_ms: request.targetMs,
          from_ms: benchmark.fromMs,
          distance_ms: distanceMs,
          direction: distanceMs < 0 ? "backward" : "forward",
          distance_class: Math.abs(distanceMs) <= 10_000 ? "near" : "far",
          request_to_dispatch_ms: performance.now() - benchmark.requestedAtMs,
          reason: request.reason,
        },
        { generation: request.generation, actionId: benchmark.actionId, required: true },
      );
    }
    try {
      video.currentTime = request.targetMs / 1_000;
      benchmark?.onDispatch?.();
    } catch (error) {
      inFlightSeek = undefined;
      if (benchmark && !benchmark.cancelled) {
        emitReplayBenchmarkEvent(
          "action_failed",
          { phase: "seek_assignment", reason: String(error), target_ms: request.targetMs },
          { generation: request.generation, actionId: benchmark.actionId, required: true },
        );
      }
      benchmark?.settle();
      addDiagnostic("seek-assignment-failed", String(error));
      attemptRecovery("The local preview rejected a seek request.");
      return;
    }
    clearSeekTimeout();
    seekTimeoutId = window.setTimeout(() => {
      const timedOut = inFlightSeek;
      if (!timedOut || timedOut.generation !== mediaGeneration) return;
      addDiagnostic("seek-timeout", `${timedOut.reason}@${timedOut.targetMs.toFixed(1)}`);
      const timedOutBenchmark = timedOut.benchmark;
      if (timedOutBenchmark && !timedOutBenchmark.cancelled) {
        emitReplayBenchmarkEvent(
          "seek_timeout",
          { target_ms: timedOut.targetMs, reason: timedOut.reason },
          {
            generation: timedOut.generation,
            actionId: timedOutBenchmark.actionId,
            required: true,
          },
        );
      }
      attemptRecovery(`Preview seek timed out after ${SEEK_TIMEOUT_MS} ms.`);
    }, SEEK_TIMEOUT_MS);
  };

  const seekTo = (
    requestedTimeMs: number,
    options: {
      reason?: SeekReason;
      playAfter?: boolean;
      onBenchmarkDispatch?: () => void;
    } = {},
  ): Promise<void> => {
    const range = clipRange();
    const targetTimeMs = range
      ? clamp(requestedTimeMs, range.startMs, range.endMs)
      : clamp(requestedTimeMs, 0, durationMs);
    setVideoTimeMs(targetTimeMs);
    const reason = options.reason ?? "navigation";
    const observer = replayBenchmarkObserver();

    const enqueue = (settle?: () => void) => {
      const benchmark = observer && settle
        ? {
            actionId: observer.nextActionId("seek"),
            requestedAtMs: performance.now(),
            fromMs: video.currentTime * 1_000,
            settle,
            cancelled: false,
            onDispatch: options.onBenchmarkDispatch,
          }
        : undefined;
      if (benchmark) {
        latestBenchmarkSeekActionId = benchmark.actionId;
        emitReplayBenchmarkEvent(
          "seek_requested",
          {
            requested_preview_ms: targetTimeMs,
            raw_requested_ms: requestedTimeMs,
            from_ms: benchmark.fromMs,
            reason,
          },
          { generation: mediaGeneration, actionId: benchmark.actionId, required: true },
        );
        cancelBenchmarkSeek(inFlightSeek, targetTimeMs, "native_seek_in_flight");
        cancelBenchmarkSeek(awaitingPresentedSeek, targetTimeMs, "awaiting_presented_frame");
        if (awaitingPresentedSeek?.benchmark?.cancelled) awaitingPresentedSeek = undefined;
      }
      if (!props.probe.video_url) {
        setSeekLatencyMs(0);
        clipLoopSeekPending = false;
        if (benchmark) {
          emitReplayBenchmarkEvent(
            "action_failed",
            { phase: "seek_request", reason: "recording has no playback URL" },
            { generation: mediaGeneration, actionId: benchmark.actionId, required: true },
          );
          benchmark.settle();
        }
        return;
      }
      if (pendingSeek) {
        cancelBenchmarkSeek(pendingSeek, targetTimeMs, "pending_dispatch", "seek_pending_replaced");
      }
      pendingSeek = {
        generation: mediaGeneration,
        targetMs: targetTimeMs,
        reason,
        playAfter: options.playAfter ?? false,
        benchmark,
      };
      setSeekQueueLabel(inFlightSeek ? "1 ACTIVE + LATEST" : "1 PENDING");
      dispatchPendingSeek();
    };

    if (!observer) {
      enqueue();
      return UNTRACKED_SEEK_COMPLETION;
    }
    return new Promise<void>((settle) => enqueue(settle));
  };

  const attemptRecovery = (reason: string, manual = false) => {
    if (!props.probe.video_url || disposed) return;
    const now = performance.now();
    recoveryAttempts = manual
      ? []
      : recoveryAttempts.filter((attempt) => now - attempt <= 10_000);
    if (recoveryAttempts.length >= 2) {
      resetSeekScheduler();
      setMediaState("degraded");
      setMediaError(reason);
      addDiagnostic("preview-degraded", reason);
      emitReplayBenchmarkEvent(
        "preview_degraded",
        { reason },
        { generation: mediaGeneration, required: true },
      );
      return;
    }

    recoveryAttempts.push(now);
    setRecoveryCount(recoveryAttempts.length);
    recoveryTargetMs = videoTimeMs();
    mediaGeneration = replayBenchmarkObserver()?.nextMediaGeneration() ?? mediaGeneration + 1;
    firstPresentedGeneration = -1;
    canPlayGeneration = -1;
    resetSeekScheduler();
    video.pause();
    setVideoTimeMs(recoveryTargetMs);
    setMediaState("recovering");
    setMediaError(reason);
    addDiagnostic(
      "recovery-start",
      `attempt=${recoveryAttempts.length}; target=${recoveryTargetMs.toFixed(1)}; ${reason}; ${mediaSnapshot()}`,
    );
    emitReplayBenchmarkEvent(
      "recovery_started",
      {
        attempt: recoveryAttempts.length,
        target_ms: recoveryTargetMs,
        reason,
      },
      { generation: mediaGeneration, required: true },
    );
    if (recoveryTimerId !== undefined) window.clearTimeout(recoveryTimerId);
    recoveryTimerId = window.setTimeout(() => {
      recoveryTimerId = undefined;
      try {
        video.load();
      } catch (error) {
        attemptRecovery(`Preview reload failed: ${String(error)}`);
      }
    }, 0);
  };

  const retryPreview = () => attemptRecovery("Manual preview retry requested.", true);

  const clipRangeForAnchor = (anchorMs: number, anchorEvent?: ViewerEvent) =>
    defaultClipRange(
      anchorMs,
      durationMs,
      events,
      props.probe.recording_fps,
      anchorEvent,
    );

  const activateClip = () => {
    const kills = events.filter(
      (candidate) =>
        candidate.event_type === "ChampionKill" || candidate.event_type === "FirstBlood",
    );
    const nearest = nearestIndexAt(kills, videoTimeMs());
    const anchorEvent =
      nearest >= 0 && Math.abs(kills[nearest].video_time_ms - videoTimeMs()) <= 10_000
        ? kills[nearest]
        : undefined;
    const anchorMs = anchorEvent?.video_time_ms ?? videoTimeMs();
    const nextRange = clipRangeForAnchor(anchorMs, anchorEvent);
    setClipRange(nextRange);
    seekTo(videoTimeMs());
  };

  const selectEvent = (event: ViewerEvent) => {
    setEditingEndpoint(null);
    if (clipRange()) {
      const nextRange = clipRangeForAnchor(event.video_time_ms, event);
      setClipRange(nextRange);
    }
    seekTo(event.video_time_ms);
  };

  const exportSelectedClip = () => {
    const range = clipRange();
    if (!range) return;
    clearEndpointHold();
    setEditingEndpoint(null);
    video.pause();
    setIsFullscreen(false);
    props.onExportClip({
      gameTimestamp: props.probe.game.timestamp,
      clipStartMs: range.startMs,
      clipEndMs: range.endMs,
    });
  };

  const updateClipEndpoint = (
    endpoint: ClipEndpoint,
    requestedMs: number,
  ) => {
    const range = clipRange();
    if (!range) return null;
    const nextRange = moveClipEndpoint(
      range,
      endpoint,
      requestedMs,
      durationMs,
      props.probe.recording_fps,
    );
    setClipRange(nextRange);
    const previewMs = clipEndpointPreviewMs(nextRange, endpoint, props.probe.recording_fps);
    seekTo(previewMs, { reason: "endpoint-edit" });
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
    video.pause();
    const previewMs = clipEndpointPreviewMs(range, endpoint, props.probe.recording_fps);
    seekTo(previewMs, { reason: "endpoint-edit" });
  };

  const finishClipEndpointEdit = () => {
    clearEndpointHold();
    const range = clipRange();
    if (!range) return;
    const endpoint = editingEndpoint();
    if (!endpoint) return;
    const previewMs = clipEndpointPreviewMs(range, endpoint, props.probe.recording_fps);
    seekTo(previewMs, { reason: "endpoint-edit" });
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
        ((pointerEvent.clientX - bounds.left) / bounds.width) * durationMs,
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
    const elapsedFrames = Math.max(
      1,
      Math.round(((now - hold.startedAt) * props.probe.recording_fps) / 1_000),
    );
    updateClipEndpoint(
      hold.endpoint,
      frameTimeMs(
        hold.startingFrame + hold.direction * elapsedFrames,
        props.probe.recording_fps,
      ),
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
    const current = endpoint === "start" ? range.startMs : range.endMs;
    const startingFrame = frameIndexAt(current, props.probe.recording_fps);
    const startedAt = performance.now();
    beginClipEndpointEdit(endpoint);
    updateClipEndpoint(
      endpoint,
      frameTimeMs(startingFrame + direction, props.probe.recording_fps),
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
    clipLoopSeekPending = false;
    setClipRange(null);
  };

  const seekFromRail = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (bounds.width <= 0) return;
    setEditingEndpoint(null);
    seekTo(((event.clientX - bounds.left) / bounds.width) * durationMs);
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: number | undefined;
    if (event.key === "PageDown") target = videoTimeMs() - 30_000;
    if (event.key === "PageUp") target = videoTimeMs() + 30_000;
    if (event.key === "Home") target = 0;
    if (event.key === "End") target = durationMs;
    if (target === undefined) return;
    event.preventDefault();
    setEditingEndpoint(null);
    seekTo(target);
  };

  const togglePlayback = async () => {
    if (!props.probe.video_url || mediaState() !== "ready") return;
    if (!video.paused) {
      video.pause();
      return;
    }
    const range = clipRange();
    setEditingEndpoint(null);
    if (range) {
      seekTo(range.startMs, { reason: "clip-preview", playAfter: true });
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
    video.pause();
    video.removeAttribute("src");
    video.load();
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
          { media_time_ms: video.currentTime * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          pendingBenchmarkPlayActionId = actionId;
          await playNativeVideo(true);
          if (video.paused || video.ended) throw new Error("native playback did not start");
          emitReplayBenchmarkEvent(
            "play_complete",
            { media_time_ms: video.currentTime * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );
          pendingBenchmarkPlayActionId = undefined;
          return;
        } catch (error) {
          pendingBenchmarkPlayActionId = undefined;
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
          { media_time_ms: video.currentTime * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          pendingBenchmarkPauseActionId = actionId;
          video.pause();
          if (!video.paused) throw new Error("native playback did not pause");
          emitReplayBenchmarkEvent(
            "pause_complete",
            { media_time_ms: video.currentTime * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );
          pendingBenchmarkPauseActionId = undefined;
          return;
        } catch (error) {
          pendingBenchmarkPauseActionId = undefined;
          if (!benchmarkAbortController.signal.aborted) failBenchmarkAction("pause", actionId, error);
          throw error;
        }
      }
      case "stable-playback": {
        const generation = mediaGeneration;
        const startedAt = performance.now();
        const startingMediaTime = video.currentTime;
        const startingQuality = video.getVideoPlaybackQuality?.();
        await waitForBenchmark(action.durationMs);
        const mediaDeltaMs = (video.currentTime - startingMediaTime) * 1_000;
        if (
          generation !== mediaGeneration ||
          video.paused ||
          video.ended ||
          mediaDeltaMs < Math.min(100, action.durationMs * 0.1)
        ) {
          throw new Error("stable playback window did not advance current media");
        }
        const quality = video.getVideoPlaybackQuality?.();
        emitReplayBenchmarkEvent(
          "steady_playback",
          {
            duration_ms: performance.now() - startedAt,
            media_advance_ms: mediaDeltaMs,
            total_frame_delta:
              quality && startingQuality
                ? quality.totalVideoFrames - startingQuality.totalVideoFrames
                : null,
            dropped_frame_delta:
              quality && startingQuality
                ? quality.droppedVideoFrames - startingQuality.droppedVideoFrames
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
        const startingMediaTime = video.currentTime;
        const startingQuality = video.getVideoPlaybackQuality?.();
        emitReplayBenchmarkEvent(
          "rate_requested",
          { rate: action.rate },
          { generation, actionId, required: true },
        );
        try {
          if (video.paused || video.ended) throw new Error("rate action requires active playback");
          video.playbackRate = action.rate;
          emitReplayBenchmarkEvent(
            "rate_applied",
            { requested_rate: action.rate, actual_rate: video.playbackRate },
            { generation, actionId, required: true },
          );
          await waitForBenchmark(action.durationMs);
          const wallSeconds = (performance.now() - startedAt) / 1_000;
          const mediaAdvance = video.currentTime - startingMediaTime;
          if (generation !== mediaGeneration || video.paused || video.ended || mediaAdvance <= 0) {
            throw new Error("rate window ended without advancing current media");
          }
          const quality = video.getVideoPlaybackQuality?.();
          emitReplayBenchmarkEvent(
            "rate_observed",
            {
              requested_rate: action.rate,
              actual_rate: video.playbackRate,
              effective_rate: wallSeconds > 0 ? mediaAdvance / wallSeconds : null,
              muted: video.muted,
              volume: video.volume,
              dropped_frames: quality?.droppedVideoFrames ?? null,
              dropped_frame_delta:
                quality && startingQuality
                  ? quality.droppedVideoFrames - startingQuality.droppedVideoFrames
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
          let targetMs = action.targetMs;
          if (reason === "event-jump" && events.length > 0) {
            const eventIndex = nearestIndexAt(events, action.targetMs);
            if (eventIndex >= 0) {
              const selected = events[eventIndex];
              targetMs = selected.video_time_ms;
              if (clipRange()) setClipRange(clipRangeForAnchor(targetMs, selected));
              emitReplayBenchmarkEvent(
                "event_jump_selected",
                { event_type: selected.event_type, target_ms: targetMs },
                { generation: mediaGeneration },
              );
            }
          }
          if (reason === "endpoint-edit") {
            if (!clipRange()) setClipRange(clipRangeForAnchor(targetMs));
            const endpoint: ClipEndpoint =
              benchmarkEndpointSequence++ % 2 === 0 ? "start" : "end";
            const range = clipRange();
            if (!range) throw new Error("benchmark endpoint edit could not create a clip range");
            const nextRange = moveClipEndpoint(
              range,
              endpoint,
              targetMs,
              durationMs,
              props.probe.recording_fps,
            );
            setEditingEndpoint(endpoint);
            setClipRange(nextRange);
            targetMs = clipEndpointPreviewMs(
              nextRange,
              endpoint,
              props.probe.recording_fps,
            );
            emitReplayBenchmarkEvent(
              "clip_endpoint_updated",
              {
                endpoint,
                clip_start_ms: nextRange.startMs,
                clip_end_ms: nextRange.endMs,
                target_ms: targetMs,
              },
              { generation: mediaGeneration },
            );
          }
          await withBenchmarkTimeout(
            seekTo(targetMs, { reason }),
            `seek to ${targetMs}`,
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
          settle = seekTo(targetMs, { reason: "benchmark" });
          await waitForBenchmark(action.intervalMs);
        }
        await withBenchmarkTimeout(settle, "scrub settle");
        emitReplayBenchmarkEvent(
          "scrub_settled",
          { request_count: action.targetsMs.length, media_time_ms: video.currentTime * 1_000 },
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
            settle = seekTo(action.seekTargetMs, {
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
          setClipRange(clipRangeForAnchor(action.anchorMs ?? video.currentTime * 1_000));
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
      video.pause();
      await updateMetrics();
      const quality = video.getVideoPlaybackQuality?.();
      const completionKind =
        scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
          ? "viewer_cycle_completed"
          : "scenario_completed";
      const completionPayload = {
        kind: scenario.kind,
        media_time_ms: video.currentTime * 1_000,
        ready_state: video.readyState,
        network_state: video.networkState,
        playback_rate: video.playbackRate,
        total_frames: quality?.totalVideoFrames ?? null,
        dropped_frames: quality?.droppedVideoFrames ?? null,
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
      resetSeekScheduler();
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
        { enabled: fullscreen, media_time_ms: video.currentTime * 1_000 },
        { generation: mediaGeneration, required: benchmarkFullscreenEventRequired },
      );
      benchmarkFullscreenEventRequired = false;
    }
  });

  onMount(() => {
    emitReplayBenchmarkEvent(
      "viewer_mounted",
      {
        duration_ms: durationMs,
        recording_fps: props.probe.recording_fps,
        has_video_url: Boolean(props.probe.video_url),
      },
      { generation: mediaGeneration, required: true },
    );
    const updateTime = () => syncPresentedTime(video.currentTime * 1_000);
    const onLoadStart = () =>
      emitReplayBenchmarkEvent(
        "media_loadstart",
        { network_state: video.networkState },
        { generation: mediaGeneration, required: true },
      );
    const onLoadedData = () =>
      emitReplayBenchmarkEvent(
        "media_loadeddata",
        { ready_state: video.readyState },
        { generation: mediaGeneration },
      );
    const onCanPlay = () => {
      canPlayGeneration = mediaGeneration;
      emitReplayBenchmarkEvent(
        "media_canplay",
        { ready_state: video.readyState },
        { generation: mediaGeneration, required: true },
      );
      if (clockSource() === "ANIMATION FRAME" && animationFrameId === undefined) {
        animationFrameId = requestAnimationFrame(onAnimationFrame);
      }
      queueMicrotask(() => void runBenchmarkScenario());
    };
    const onLoadedMetadata = () => {
      const recovered = mediaState() === "recovering";
      setMediaState("ready");
      setMediaError(null);
      addDiagnostic(
        recovered ? "recovery-ready" : "metadata-ready",
        `duration=${(video.duration * 1_000).toFixed(1)}; readyState=${video.readyState}`,
      );
      emitReplayBenchmarkEvent(
        recovered ? "recovery_ready" : "metadata_ready",
        {
          duration_ms: video.duration * 1_000,
          ready_state: video.readyState,
          video_width: video.videoWidth,
          video_height: video.videoHeight,
        },
        { generation: mediaGeneration, required: true },
      );
      const range = clipRange();
      if (recovered) {
        seekTo(recoveryTargetMs, { reason: "recovery" });
      } else if (pendingSeek) {
        dispatchPendingSeek();
      } else if (range) {
        seekTo(range.startMs);
      } else {
        updateTime();
      }
    };
    const onPlay = () => {
      setIsPlaying(true);
      if (clockSource() === "ANIMATION FRAME" && animationFrameId === undefined) {
        animationFrameId = requestAnimationFrame(onAnimationFrame);
      }
      emitReplayBenchmarkEvent(
        "native_play",
        { media_time_ms: video.currentTime * 1_000, rate: video.playbackRate },
        {
          generation: mediaGeneration,
          actionId: pendingBenchmarkPlayActionId,
          required: true,
        },
      );
      pendingBenchmarkPlayActionId = undefined;
    };
    const onPause = () => {
      setIsPlaying(false);
      if (!editingEndpoint() && mediaState() !== "recovering") updateTime();
      emitReplayBenchmarkEvent(
        "native_pause",
        { media_time_ms: video.currentTime * 1_000 },
        {
          generation: mediaGeneration,
          actionId: pendingBenchmarkPauseActionId,
          required: true,
        },
      );
      pendingBenchmarkPauseActionId = undefined;
    };
    const onSeeking = () => {
      if (inFlightSeek) {
        addDiagnostic("media-seeking", `${inFlightSeek.reason}@${inFlightSeek.targetMs.toFixed(1)}`);
        const benchmark = inFlightSeek.benchmark;
        if (benchmark && !benchmark.cancelled) {
          emitReplayBenchmarkEvent(
            "native_seeking",
            { target_ms: inFlightSeek.targetMs, reason: inFlightSeek.reason },
            {
              generation: inFlightSeek.generation,
              actionId: benchmark.actionId,
              required: true,
            },
          );
        }
      }
    };
    const onSeeked = () => {
      clipLoopSeekPending = false;
      const completed = inFlightSeek;
      clearSeekTimeout();
      inFlightSeek = undefined;
      setSeekQueueLabel(pendingSeek ? "1 PENDING" : "IDLE");
      if (!editingEndpoint()) updateTime();
      if (completed) {
        const latency = performance.now() - lastSeekDispatchedAt;
        const benchmark = completed.benchmark;
        setSeekLatencyMs(latency);
        addDiagnostic(
          "seek-complete",
          `${completed.reason}@${completed.targetMs.toFixed(1)} in ${latency.toFixed(0)}ms`,
        );
        if (benchmark && !benchmark.cancelled) {
          emitReplayBenchmarkEvent(
            "seeked",
            {
              target_ms: completed.targetMs,
              actual_media_time_ms: video.currentTime * 1_000,
              request_to_seeked_ms: performance.now() - benchmark.requestedAtMs,
              dispatch_to_seeked_ms: latency,
              reason: completed.reason,
            },
            {
              generation: completed.generation,
              actionId: benchmark.actionId,
              required: true,
            },
          );
          awaitingPresentedSeek = completed;
        } else {
          benchmark?.settle();
        }
        if (completed.generation === mediaGeneration && completed.playAfter) {
          void playNativeVideo();
        }
        if (benchmark && !benchmark.cancelled && typeof video.requestVideoFrameCallback !== "function") {
          requestAnimationFrame(() => observePresentedFrame(video.currentTime * 1_000));
        }
      }
      dispatchPendingSeek();
    };
    const onEnded = () => {
      const range = clipRange();
      if (!range) {
        onPause();
        return;
      }
      clipLoopSeekPending = false;
      seekTo(range.startMs, { reason: "clip-loop", playAfter: true });
    };
    const onError = () => {
      const description = mediaErrorDescription();
      addDiagnostic(
        "media-error",
        `${description}; ${mediaSnapshot()}`,
      );
      emitReplayBenchmarkEvent(
        "media_error",
        {
          description,
          code: video.error?.code ?? null,
          message: video.error?.message ?? null,
        },
        { generation: mediaGeneration, required: true },
      );
      attemptRecovery(description);
    };
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
        seekTo(videoTimeMs() + direction * (clipRange() ? 5_000 : 15_000));
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

    video.addEventListener("loadstart", onLoadStart);
    video.addEventListener("loadeddata", onLoadedData);
    video.addEventListener("canplay", onCanPlay);
    video.addEventListener("loadedmetadata", onLoadedMetadata);
    video.addEventListener("pause", onPause);
    video.addEventListener("play", onPlay);
    video.addEventListener("ended", onEnded);
    video.addEventListener("seeking", onSeeking);
    video.addEventListener("seeked", onSeeked);
    video.addEventListener("error", onError);
    window.addEventListener("keydown", onKeyDown);
    if (typeof video.requestVideoFrameCallback === "function") {
      if (props.probe.video_url) frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
    } else {
      setClockSource("ANIMATION FRAME");
      video.addEventListener("timeupdate", updateTime);
      if (props.probe.video_url) animationFrameId = requestAnimationFrame(onAnimationFrame);
    }
    metricsIntervalId = window.setInterval(() => void updateMetrics(), 1_000);
    const observer = replayBenchmarkObserver();
    if (observer && props.probe.video_url) {
      benchmarkMediaReadyTimerId = window.setTimeout(() => {
        benchmarkMediaReadyTimerId = undefined;
        if (benchmarkScenarioStarted || benchmarkScenarioFinished || disposed) return;
        benchmarkScenarioFinished = true;
        const reason =
          `media readiness timed out after ${BENCHMARK_MEDIA_READY_TIMEOUT_MS} ms`;
        observer.emit(
          "scenario_failed",
          { phase: "media_readiness", reason },
          { generation: mediaGeneration, required: true },
        );
        void releaseBenchmarkMedia()
          .catch(() => undefined)
          .then(() => observer.complete("failed", reason, { phase: "media_readiness" }));
      }, BENCHMARK_MEDIA_READY_TIMEOUT_MS);
    }
    if (video.readyState >= HTMLMediaElement.HAVE_METADATA) onLoadedMetadata();
    if (video.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA) onCanPlay();

    onCleanup(() => {
      disposed = true;
      const observer = replayBenchmarkObserver();
      if (observer && !benchmarkScenarioFinished) {
        const reason = "viewer disposed before benchmark scenario completion";
        benchmarkScenarioFinished = true;
        observer.emit(
          "scenario_failed",
          { phase: "viewer_disposed", reason },
          { generation: mediaGeneration, required: true },
        );
        void observer.complete("failed", reason, { phase: "viewer_disposed" });
      }
      benchmarkAbortController.abort();
      emitReplayBenchmarkEvent(
        "viewer_disposed",
        { media_time_ms: video.currentTime * 1_000 },
        { generation: mediaGeneration, required: true },
      );
      video.removeEventListener("loadstart", onLoadStart);
      video.removeEventListener("loadeddata", onLoadedData);
      video.removeEventListener("canplay", onCanPlay);
      video.removeEventListener("loadedmetadata", onLoadedMetadata);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("ended", onEnded);
      video.removeEventListener("seeking", onSeeking);
      video.removeEventListener("seeked", onSeeked);
      video.removeEventListener("error", onError);
      video.removeEventListener("timeupdate", updateTime);
      window.removeEventListener("keydown", onKeyDown);
      document.body.classList.remove("replay-fullscreen");
      if (frameCallbackId !== undefined && typeof video.cancelVideoFrameCallback === "function") {
        video.cancelVideoFrameCallback(frameCallbackId);
      }
      if (animationFrameId !== undefined) cancelAnimationFrame(animationFrameId);
      clearEndpointHold();
      resetSeekScheduler();
      if (recoveryTimerId !== undefined) window.clearTimeout(recoveryTimerId);
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
      if (benchmarkMediaReadyTimerId !== undefined) {
        window.clearTimeout(benchmarkMediaReadyTimerId);
      }
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
              ref={video}
              src={props.probe.video_url || undefined}
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
                durationMs={durationMs}
                videoTimeMs={videoTimeMs()}
                gameClockSeconds={gameClockSecond()}
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
                onClipEndpointPreview={(endpoint, requestedMs) => {
                  updateClipEndpoint(endpoint, requestedMs);
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
                aria-label={`Clip starts at ${formatDuration(clipRange()!.startMs)}`}
                onPointerDown={(event) => beginClipDrag(event, "start", windowedRail)}
                onKeyDown={(event) => handleClipEndpointKeyDown(event, "start")}
                onKeyUp={(event) => handleClipEndpointKeyUp(event, "start")}
                onBlur={() => handleClipEndpointBlur("start")}
              />
              <button
                class={`${styles.clipHandle} ${styles.clipHandleEnd}`}
                style={`left:${clipEndPosition()}%`}
                type="button"
                aria-label={`Clip ends at ${formatDuration(clipRange()!.endMs)}`}
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
                    aria-label={`Seek to ${eventTitle(marker.event)} at ${formatDuration(marker.event.game_time_ms)}`}
                    title={`${eventTitle(marker.event)} · ${formatDuration(marker.event.game_time_ms)}`}
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

        <Show when={import.meta.env.DEV && !isFullscreen()}>
          <details class={styles.diagnostics}>
            <summary>
              <span>PLAYBACK DIAGNOSTICS</span>
              <strong>{presentedFps().toFixed(1)} FPS</strong>
            </summary>
            <dl>
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
