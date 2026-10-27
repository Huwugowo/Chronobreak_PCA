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
  loadReplayDescriptor,
  loadServerMetrics,
} from "../api";
import {
  buildScenarioActions,
  emitReplayBenchmarkEvent,
  replayBenchmarkObserver,
  isOpeningScenario,
  openingPayload,
  type OpeningCycle,
  type ScenarioAction,
} from "../benchmark";
import { formatDate, formatDuration } from "../format";
import type {
  ClipDraft,
  ClipRange,
  GameSummary,
  ReplayDescriptor,
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
} from "../viewerUtils";

import {
  wholeTimelineViewport,
  type TimelineViewport,
} from "../replayTimelineGeometry";
import { createPlaybackController, type PlaybackController, type PlaybackEvent, type PlaybackSnapshot } from "../playbackController";
import { createHtmlVideoPlaybackAdapter } from "../htmlVideoPlaybackAdapter";
import { createPlaybackDiagnostics } from "../playbackDiagnostics";
import { createBenchmarkSeekEvidence } from "../benchmarkSeekEvidence";
import { runBenchmarkDesktopResize } from "../benchmarkResize";
import { waitForBenchmarkRenderedReplay } from "../benchmarkRender";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./ViewerScreen.module.css";
import { libraryError, type ReplayRead } from "../libraryController";
import { indexRecording, loadRecordingFacts, queryOverlayPage, queryReplayWindow, sourceScopes } from "../leagueIndex";
import { ReplayOverlayController } from "../replayOverlayController";
import { ReplayDataController } from "../replayDataController";
import { participantKey, sameSource, sourceKey } from "../leagueSource";
import { replayEvent, replayParticipants, recordedGameClock, overlayParticipantStates } from "../replayProjection";

type Props = {
  ddragonAssetBaseUrl?: string | null;
  gameTimestamp: string;
  game: GameSummary;
  readReplay: ReplayRead;
  onBack: () => void;
  onBenchmarkViewerCycleCompleted?: () => void;
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
  | "render-preconditioning"
  | "seek-preconditioning"
  | "benchmark-scrub"
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

const hasReplayTick = <T extends { replay_tick?: ReplayTick }>(
  value: T,
): value is T & { replay_tick: ReplayTick } => value.replay_tick !== undefined;

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

const millisecondsToReplayTick = (milliseconds: number, replayEnd: ReplayTick): ReplayTick =>
  Math.round(
    clamp(milliseconds, 0, replayTickToMilliseconds(replayEnd)) * REPLAY_TICKS_PER_SECOND / 1_000,
  ) as ReplayTick;

function ViewerScreen(props: Props) {
  const observer = replayBenchmarkObserver();
  const opening = observer && isOpeningScenario(observer.scenario())
    ? observer.openingCycle(props.gameTimestamp) : undefined;
  let alive = true;
  onCleanup(() => { alive = false; });
  const [descriptor, { refetch }] = createResource(() => props.gameTimestamp,
    async id => {
      if (opening) observer!.emitOpening("opening_descriptor_requested", opening);
      const value = await props.readReplay(token => loadReplayDescriptor(id, token));
      if (opening && alive) observer!.emitOpening("opening_descriptor_ready", opening);
      return value;
    });
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
              opening={opening}
            />
          )}
        </Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: Props & { descriptor: ReplayDescriptor; opening?: OpeningCycle }) {
  const mediaTimeline = props.descriptor.media_timeline;
  const replayEnd = mediaTimeline.video.replayEnd;
  const durationMs = replayTickToMilliseconds(replayEnd);
  let primaryVideo: HTMLVideoElement | undefined;
  let benchmarkVideo: HTMLVideoElement | undefined;
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
  let benchmarkEndpointSequence = 0;
  const benchmarkAbortController = new AbortController();
  const dispatchObservers = new Map<string, () => void>();
  const benchmarkSeekObservers = new Set<(event: PlaybackEvent) => void>();
  let admitDetails!: () => void;
  const admittedDetails = new Promise<void>(resolve => { admitDetails = resolve; });

  const replayData = new ReplayDataController({ gameTimestamp: props.gameTimestamp,
    mediaId: mediaTimeline.mediaId, readReplay: props.readReplay,
    loadRecordingFacts, indexRecording, sourceScopes, queryReplayWindow });
  const [data, setData] = createSignal(replayData.value);
  const unsubscribeData = replayData.subscribe(setData);
  const overlayData = new ReplayOverlayController({ gameTimestamp: props.gameTimestamp, replayEnd: replayEnd as ReplayTick,
    readReplay: props.readReplay, queryOverlayPage });
  const [overlay, setOverlay] = createSignal(overlayData.value);
  const unsubscribeOverlay = overlayData.subscribe(setOverlay);
  const [selectedSource, setSelectedSource] = createSignal<string | null>(null);
  const activeScope = createMemo(() => data().scopes.find(scope => sourceKey(scope.source) === selectedSource())
    ?? data().scopes[data().scopes.length - 1]);
  const activeWindow = createMemo(() => {
    const window = data().activeWindow, scope = activeScope();
    return window && scope && sameSource(window.source, scope.source) ? window : null;
  });
  const roster = () => activeWindow()?.roster ?? null;
  createEffect(() => overlayData.select(activeScope()));
  const detailsError = () => data().factsError ?? data().windowError ?? overlay().error ?? data().error;
  const details = () => data().facts;
  const detailsReady = () => Boolean(details() && (activeWindow() || data().indexing === "unavailable"
    || (data().indexing === "ready" && !data().loading.scopes && data().scopes.length === 0))
    && (!activeScope() || (overlay().scope && sameSource(overlay().scope!.source, activeScope()!.source)
      && overlay().status === "ready")));
  const retryDetails = () => { overlayData.retry(); return replayData.retry(); };
  const clearOpeningReadinessTimeout = () => {
    if (props.opening && benchmarkScenarioStarted && detailsReady() && benchmarkMediaReadyTimerId !== undefined) {
      window.clearTimeout(benchmarkMediaReadyTimerId);
      benchmarkMediaReadyTimerId = undefined;
    }
  };
  const failOpening = (phase: string, reason: string) => {
    if (benchmarkScenarioFinished || disposed) return;
    benchmarkScenarioFinished = true;
    benchmarkAbortController.abort();
    controller.dispose();
    if (benchmarkMediaReadyTimerId !== undefined) window.clearTimeout(benchmarkMediaReadyTimerId);
    const observer = replayBenchmarkObserver()!;
    const payload = { ...openingPayload(props.opening!), phase, reason };
    observer.emit("scenario_failed", payload, { generation: mediaGeneration, required: true });
    void observer.complete("failed", reason, payload);
  };
  let detailsPaint: number | undefined;
  let payloadReported = false;
  let detailsFailureReported = false;
  onMount(() => {
    detailsPaint = requestAnimationFrame(() => {
      detailsPaint = requestAnimationFrame(() => { if (!disposed) void replayData.initialize(); });
    });
  });
  onCleanup(() => {
    if (detailsPaint !== undefined) cancelAnimationFrame(detailsPaint);
    unsubscribeData(); replayData.dispose();
    unsubscribeOverlay(); overlayData.dispose();
  });
  createEffect(() => {
    if (detailsError()) {
      const observer = replayBenchmarkObserver();
      if (observer && !detailsFailureReported) {
        detailsFailureReported = true;
        const reason = libraryError(detailsError());
        if (props.opening) {
          failOpening("league_replay_window", reason);
          return;
        }
        observer.emit("scenario_failed", { phase: "league_replay_window", reason }, { required: true });
        void observer.complete("failed", reason, { phase: "league_replay_window" });
      }
      return;
    }
    const loaded = details();
    if (!loaded || !detailsReady() || payloadReported || (props.opening && benchmarkScenarioFinished)) return;
    payloadReported = true;
    if (props.opening) replayBenchmarkObserver()!.emitOpening("opening_details_ready", props.opening);
    admitDetails();
    clearOpeningReadinessTimeout();
    emitReplayBenchmarkEvent("playback_payload_ready", {
      event_count: activeWindow()?.events.length ?? 0, participant_count: roster()?.players.length ?? 0,
      duration_ms: durationMs, payload_scope: "selected_source_window",
    }, { required: true });
    queueMicrotask(() => { if (!disposed) void runBenchmarkScenario(); });
  });

  const events = createMemo<MappedViewerEvent[]>(() => {
    const window = activeWindow();
    return (window?.events ?? []).map(row => replayEvent(row, window!, window!.roster, window!.identities))
      .filter(hasReplayTick).sort((left, right) => left.replay_tick - right.replay_tick);
  });
  const participants = createMemo(() => {
    const scope = activeScope(), source = overlay().scope;
    return scope && source && sameSource(scope.source, source.source) ? replayParticipants(scope, overlay().identities) : [];
  }, undefined, {
    // A timed roster refresh must not recreate unchanged identity slots/assets.
    equals: (previous, next) => JSON.stringify(previous) === JSON.stringify(next),
  });

  const [isFullscreen, setIsFullscreen] = createSignal(false);
  const [selectedPlayers, setSelectedPlayers] = createSignal<readonly string[]>([]);
  const [presentedPosition, setPresentedPosition] = createSignal<ReplayTick | null>(null);
  const [observedPosition, setObservedPosition] = createSignal<ReplayTick | null>(null);
  const [playback, setPlayback] = createSignal<PlaybackSnapshot>();
  const [replayPosition, setReplayPosition] = createSignal<ReplayTick>(0 as ReplayTick);
  const [timelineViewport, setTimelineViewport] = createSignal<TimelineViewport>(
    wholeTimelineViewport(replayEnd as ReplayTick),
  );
  const requestCurrentWindow = () => {
    const scope = activeScope();
    if (!scope) return;
    replayData.requestWindow({ scope: scope.source, ...timelineViewport(), at: 0 as ReplayTick });
  };
  createEffect(requestCurrentWindow);
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
  const visibleEvents = createMemo(() => {
    const selected = selectedPlayers();
    if (selected.length === 0) return events();
    return events().filter((event) =>
      selected.some((player) => eventInvolvesPlayer(event, player)),
    );
  });
  const overlaySample = createMemo(() => {
    overlay(); // Page arrivals can make the current presented position complete.
    return overlayData.sampleAt(observedPosition());
  });
  const recordedScores = () => overlaySample()?.local_scores;
  const recordedState = createMemo(() => {
    const sample = overlaySample();
    if (!sample) return null;
    const player = sample.players.find(player => player.participant === activeScope()?.local_participant);
    const scores = sample.local_scores;
    return { kills: scores?.kills ?? undefined, deaths: scores?.deaths ?? undefined, assists: scores?.assists ?? undefined,
      creep_score: scores?.creep_score ?? undefined, level: player?.level ?? undefined, clock: recordedGameClock(sample.game_tick) };
  });
  const participantStates = createMemo(() => {
    const scope = activeScope(), source = overlay().scope;
    return scope && source && sameSource(scope.source, source.source)
      ? overlayParticipantStates(scope, overlaySample(), observedPosition(), overlay().initialLoadout) : new Map();
  });
  createEffect(() => { activeScope(); setSelectedPlayers([]); });
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
    // A seek request does not replace the displayed frame. In particular, its
    // RVFC may arrive before seeked; a later queue update must not erase it.
    if (value.readiness !== "ready") setObservedPosition(null);
    if (value.queue !== "IDLE" && value.seek.requestedPreview !== null) setReplayPosition(value.seek.requestedPreview);
  };

  const onPlaybackEvent = (event: PlaybackEvent) => {
    mediaGeneration = event.generation;
    emitReplayBenchmarkEvent(event.kind, { ...event.payload,
      ...(props.opening ? openingPayload(props.opening) : {}),
    }, { generation: event.generation, actionId: event.actionId, required: true });
    benchmarkSeekObservers.forEach(observe => observe(event));
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

  const seekTo = (target: ReplayTick, options: { reason?: SeekReason; playAfter?: boolean; onBenchmarkDispatch?: () => void; awaitBenchmarkEvidence?: boolean } = {}): Promise<void> => {
    if (!controller) return Promise.resolve();
    controller.setLoopRange(clipRange());
    const observer = replayBenchmarkObserver();
    const actionId = observer?.nextActionId("seek");
    const evidence = actionId && options.awaitBenchmarkEvidence
      ? createBenchmarkSeekEvidence(mediaGeneration, actionId) : undefined;
    if (evidence) benchmarkSeekObservers.add(evidence.observe);
    if (actionId && options.onBenchmarkDispatch) dispatchObservers.set(actionId, options.onBenchmarkDispatch);
    const operation = controller.seek(target, { reason: options.reason, playAfter: options.playAfter, actionId });
    const preview = controller.snapshot().seek.requestedPreview;
    if (preview !== null) setReplayPosition(preview);
    const settled = operation.then((result) => {
      if (actionId) dispatchObservers.delete(actionId);
      if (observer && (result.status === "failed" || result.status === "unavailable")) throw new Error(result.reason ?? result.status);
      if (evidence && result.status === "superseded") throw new Error("benchmark final seek was superseded");
    });
    if (!evidence) return settled;
    return withBenchmarkTimeout(Promise.all([settled, evidence.complete]), "seek evidence")
      .then(() => {})
      .finally(() => benchmarkSeekObservers.delete(evidence.observe));
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

  const withBenchmarkTimeout = async <T,>(promise: Promise<T>, label: string, timeoutMs = SEEK_TIMEOUT_MS * 4): Promise<T> => {
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
            timeoutMs,
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
          { ...(props.opening ? openingPayload(props.opening) : {}), media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          
          await playNativeVideo(true);
          if (controller.snapshot().media.paused || controller.snapshot().media.ended) throw new Error("native playback did not start");
          emitReplayBenchmarkEvent(
            "play_complete",
            { ...(props.opening ? openingPayload(props.opening) : {}), media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
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
          { ...(props.opening ? openingPayload(props.opening) : {}), media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {
          
          controller.pause();
          if (!controller.snapshot().media.paused) throw new Error("native playback did not pause");
          emitReplayBenchmarkEvent(
            "pause_complete",
            { ...(props.opening ? openingPayload(props.opening) : {}), media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
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
            ...(props.opening ? openingPayload(props.opening) : {}),
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
            action.reason === "endpoint-edit" || action.reason === "event-jump" ||
            action.reason === "render-preconditioning" || action.reason === "seek-preconditioning"
              ? action.reason
              : "benchmark";
          let target = millisecondsToReplayTick(action.targetMs, replayEnd);
          if (reason === "event-jump" && events().length > 0) {
            const eventIndex = nearestIndexAt(events(), target);
            if (eventIndex >= 0) {
              const selected = events()[eventIndex];
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
            seekTo(target, { reason, awaitBenchmarkEvidence: true }),
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
        for (const [index, targetMs] of action.targetsMs.entries()) {
          settle = seekTo(millisecondsToReplayTick(targetMs, replayEnd), {
            reason: replayBenchmarkObserver()?.scenario().kind === "whole_app" ? "benchmark-scrub" : "benchmark",
            awaitBenchmarkEvidence: index === action.targetsMs.length - 1,
          });
          if (index < action.targetsMs.length - 1) await waitForBenchmark(action.intervalMs);
        }
        await withBenchmarkTimeout(settle, "scrub settle");
        emitReplayBenchmarkEvent(
          "scrub_settled",
          { request_count: action.targetsMs.length, media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, required: true },
        );
        return;
      }
      case "desktop-resize":
        if (!benchmarkVideo) throw new Error("resize video is unavailable");
        await runBenchmarkDesktopResize(replayBenchmarkObserver()!, controller, benchmarkVideo,
          benchmarkAbortController.signal, waitForBenchmark);
        return;
      case "render-ready": {
        const video = benchmarkVideo;
        const root = video?.closest("section")?.querySelector('[data-testid="fullscreen-overlay"]');
        if (!video || !root || !props.ddragonAssetBaseUrl) throw new Error("render readiness surface/assets unavailable");
        const initial = controller.snapshot();
        const initialSource = activeScope()?.source;
        if (!initialSource) throw new Error("render readiness source unavailable");
        const base = props.ddragonAssetBaseUrl;
        await waitForBenchmarkRenderedReplay(replayBenchmarkObserver()!, benchmarkAbortController.signal, () => {
          const current = controller.snapshot();
          if (!video.isConnected || current.generation !== initial.generation || current.sessionToken !== initial.sessionToken ||
              current.sourceUrl !== initial.sourceUrl || current.recoveryCount !== initial.recoveryCount || !current.media.paused) {
            throw new Error("render readiness changed media load/playback state");
          }
          const source = activeScope()?.source, overlaySource = overlay().scope?.source;
          if (source && !sameSource(source, initialSource)) throw new Error("render readiness source changed");
          const tick = presentedPosition(), sample = overlaySample();
          if (!source || !overlaySource || !sameSource(source, overlaySource) || !activeWindow() ||
              !sameSource(source, activeWindow()!.source) || !sample || sample.players.length !== 10 || tick === null ||
              Math.abs(tick - 52.433 * REPLAY_TICKS_PER_SECOND) > REPLAY_TICKS_PER_SECOND / 60) return null;
          const states = participantStates();
          const players = participants().flatMap(player => {
            const state = player.identity && states.get(participantKey(player.identity));
            return state?.level === undefined ? [] : [{ name: player.summoner_name, level: state.level,
              portraitUrl: `${base}/champion/${encodeURIComponent(player.champion)}`, spells: state.spells }];
          });
          return { generation: current.generation, mediaTick: tick, sourceEpoch: source.epoch,
            sourceToken: sourceKey(source), eventCount: events().length, players };
        }, root);
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
              awaitBenchmarkEvidence: true,
            });
            await withBenchmarkTimeout(Promise.race([dispatched, settle]), "layout seek dispatch");
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
      disposed || !observer || benchmarkScenarioFinished ||
      (!props.opening && !detailsReady()) || detailsError() ||
      benchmarkScenarioStarted ||
      mediaState() !== "ready" ||
      canPlayGeneration !== mediaGeneration ||
      firstPresentedGeneration !== mediaGeneration
    ) {
      return;
    }
    benchmarkScenarioStarted = true;
    clearOpeningReadinessTimeout();
    if (!props.opening && benchmarkMediaReadyTimerId !== undefined) {
      window.clearTimeout(benchmarkMediaReadyTimerId);
      benchmarkMediaReadyTimerId = undefined;
    }
    try {
      const scenario = observer.scenario();
      const actions = buildScenarioActions(scenario, durationMs);
      for (const action of actions) await runBenchmarkAction(action);
      controller.pause();
      if (props.opening) {
        await withBenchmarkTimeout(admittedDetails, "opening details", BENCHMARK_MEDIA_READY_TIMEOUT_MS);
        if (disposed || benchmarkScenarioFinished || detailsError() || !detailsReady()) return;
        if (benchmarkMediaReadyTimerId !== undefined) window.clearTimeout(benchmarkMediaReadyTimerId);
      }
      await updateMetrics();
      if (props.opening && (disposed || benchmarkScenarioFinished)) return;
      const quality = controller.sampleMetrics().quality;
      const completionKind =
        scenario.kind === "whole_app" || scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
          ? "viewer_cycle_completed"
          : "scenario_completed";
      const completionPayload = {
        ...(props.opening ? openingPayload(props.opening) : {}),
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
        scenario.kind === "whole_app" || scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
      ) {
        props.onBenchmarkViewerCycleCompleted?.();
        return;
      }
      await observer.complete("complete", null, { action_count: actions.length });
    } catch (error) {
      if (benchmarkAbortController.signal.aborted || disposed) return;
      const reason = error instanceof Error ? error.message : String(error);
      if (props.opening) {
        failOpening("actions", reason);
        return;
      }
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
    if (replayBenchmarkObserver()) benchmarkVideo = primaryVideo;
    primaryVideo = undefined;
    decoderDiagnostics = createPlaybackDiagnostics((value) => {
      emitReplayBenchmarkEvent("decoder_observation", { status: value.status, reason: value.reason,
        decoder_name: value.decoder_name, platform_decoder: value.platform_decoder, runtime: value.runtime,
        protocol: value.protocol, associated: value.associated, transitions: value.transitions },
        { generation: value.generation ?? mediaGeneration, required: true });
    });
    const unsubscribe = controller.subscribe(syncSnapshot);
    const unsubscribeFrames = controller.subscribeFrames((tick, authority) => {
      setObservedPosition(tick);
      if (authority === "rvfc") setPresentedPosition(tick);
      if (!editingEndpoint()) {
        const range = clipRange();
        setReplayPosition((range ? clamp(tick, clipStartTick(range), clipEndTick(range)) : tick) as ReplayTick);
      }
    });
    const openMedia = () => {
      if (disposed || !props.descriptor.video_url) return;
      if (props.opening) replayBenchmarkObserver()!.emitOpening("opening_media_open_requested", props.opening);
      void controller.open({ url: props.descriptor.video_url, mediaId: mediaTimeline.mediaId, timeline: mediaTimeline });
      if (props.opening) replayBenchmarkObserver()!.emitOpening("opening_media_open_bound", props.opening,
        { generation: controller.snapshot().generation });
      const initial = clipRange();
      if (initial) { controller.setLoopRange(initial); void seekTo(clipStartTick(initial)); }
    };
    // Targeted production decoder checks subscribe before opening their media.
    // Ordinary user opening remains independent of diagnostic readiness.
    if (replayBenchmarkObserver() && !props.opening) void decoderDiagnostics.ready.then(openMedia);
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
        if ((!props.opening && benchmarkScenarioStarted) || benchmarkScenarioFinished || disposed) return;
        if (props.opening) {
          failOpening("media_readiness", "opening readiness/details timed out");
          return;
        }
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
        const reason = "viewer disposed before benchmark completion";
        if (props.opening) {
          const payload = { ...openingPayload(props.opening), phase: "viewer_disposed", reason };
          observer.emit("scenario_failed", payload, { generation: mediaGeneration, required: true });
          void observer.complete("failed", reason, payload);
        } else void observer.complete("failed", reason);
      }
      benchmarkAbortController.abort();
      benchmarkVideo = undefined;
      dispatchObservers.clear();
      benchmarkSeekObservers.clear();
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
          <Show when={details()?.league_match}>{(context) =>
            <p data-testid="league-match-context">
              {details()?.league_result ? "Match confirmed" : "Match provisional · Result unknown"}
              {` · GAME ${context().game_id} · QUEUE ${context().queue_id}`}
            </p>
          }</Show>
          <Show when={details()?.league_result}>{(result) =>
            <p data-testid="league-result">
              EOG · {result().outcome === "win" ? "WIN" : result().outcome === "loss" ? "LOSS" : "Outcome unknown"}
              {Object.values(result().coverage).some(v => v !== "present") ? " · Partial final facts" : " · Final facts complete"}
              {result().ended_early ? " · Ended early" : ""}
            </p>
          }</Show>
        </div>
        <dl>
          <Show when={details()?.league_result?.local_player}>{(player) =>
            <div>
              <dt>FINAL EOG K / D / A</dt>
              <dd>{player().stats.kills ?? "—"} / {player().stats.deaths ?? "—"} / {player().stats.assists ?? "—"}</dd>
            </div>
          }</Show>
          <div>
            <dt>RECORDED K / D / A</dt>
            <dd>
              {recordedScores()?.kills ?? "—"} / {recordedScores()?.deaths ?? "—"} / {recordedScores()?.assists ?? "—"}
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

      <Show when={!detailsReady() || detailsError() || data().indexing === "unavailable"}>
        <section role="status" data-testid="replay-details-state">
          {detailsError() || data().indexing === "unavailable" ? "Replay details unavailable. Playback remains available." : "Loading replay details..."}
          <Show when={detailsError() || data().indexing === "unavailable"}>
            <span>{libraryError(detailsError())}</span>
            <button type="button" onClick={() => void retryDetails()}>Retry replay details</button>
          </Show>
        </section>
      </Show>

      <Show when={data().scopes.length > 1}>
        <label>Recorded source
          <select aria-label="Recorded source" value={activeScope() ? sourceKey(activeScope()!.source) : ""}
            onChange={event => setSelectedSource(event.currentTarget.value)}>
            <For each={data().scopes}>{(scope, index) => <option value={sourceKey(scope.source)}>
              Source {index() + 1} · epoch {scope.source.epoch} · {scope.source.run_id.slice(0, 8)}
            </option>}</For>
          </select>
        </label>
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
                spellAssets={overlay().spellAssets}
                participantStates={participantStates()}
                ddragonAssetBaseUrl={props.ddragonAssetBaseUrl ?? null}
                champion={props.game.champion}
                recordedState={recordedState()}
                localPlayerName={details()?.local_player_name ?? null}
                events={visibleEvents()}
                mediaTimeline={mediaTimeline}
                replayTick={replayPosition()}
                timelineViewport={timelineViewport()}
                onTimelineViewportChange={setTimelineViewport}
                hasMoreEvents={Boolean(activeWindow()?.next)}
                hasEarlierEvents={replayData.isFollowingPage(activeWindow())}
                eventsLoading={data().loading.window}
                onNextEvents={() => replayData.loadMore()}
                onFirstEvents={requestCurrentWindow}
                presentedTick={presentedPosition()}
                isPlaying={isPlaying()}
                playback={playback()}
                onRate={(rate) => controller?.setRate(rate)}
                onMuted={(muted) => controller?.setMuted(muted)}
                onVolume={(volume) => controller?.setVolume(volume)}
                mediaAvailable={Boolean(props.descriptor.video_url) && mediaState() === "ready"}
                participants={participants()}
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
