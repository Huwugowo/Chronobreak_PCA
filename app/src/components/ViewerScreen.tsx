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
  emitReplayBenchmarkEvent,
  replayBenchmarkObserver,
  openingPayload,
  type OpeningCycle,
  type BenchmarkEventMeta,
  type ReplayBenchmarkObserver,
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
import { createPlaybackController, type PlaybackController, type PlaybackEvent, type PlaybackSnapshot, type SeekOutcome } from "../playbackController";
import { createHtmlVideoPlaybackAdapter } from "../htmlVideoPlaybackAdapter";
import { createPlaybackDiagnostics } from "../playbackDiagnostics";
import { createBenchmarkViewer, benchmarkViewerOpening, type BenchmarkViewer } from "../benchmarkViewer";
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

type MappedViewerEvent = ViewerEvent & { replay_tick: ReplayTick };

const hasReplayTick = <T extends { replay_tick?: ReplayTick }>(
  value: T,
): value is T & { replay_tick: ReplayTick } => value.replay_tick !== undefined;

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

function ViewerScreen(props: Props) {
  const observer = replayBenchmarkObserver();
  const opening = benchmarkViewerOpening(observer, props.gameTimestamp);
  let alive = true;
  onCleanup(() => { alive = false; });
  const [descriptor, { refetch }] = createResource(() => props.gameTimestamp,
    async id => {
      if (opening && replayBenchmarkObserver() === observer) observer!.emitOpening("opening_descriptor_requested", opening);
      const value = await props.readReplay(token => loadReplayDescriptor(id, token));
      if (opening && alive && replayBenchmarkObserver() === observer) observer!.emitOpening("opening_descriptor_ready", opening);
      return value;
    });
  let failureReported = false;

  createEffect(() => {
    const error = descriptor.error;
    if (!error || !observer || replayBenchmarkObserver() !== observer || failureReported) return;
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
              benchmarkObserver={observer}
            />
          )}
        </Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: Props & { descriptor: ReplayDescriptor; opening?: OpeningCycle; benchmarkObserver: ReplayBenchmarkObserver | null }) {
  const observer = props.benchmarkObserver;
  let benchmarkViewer: BenchmarkViewer | undefined;
  const emitViewerBenchmarkEvent = (kind: string, payload: Record<string, unknown> = {}, meta: BenchmarkEventMeta = {}) =>
    replayBenchmarkObserver() === observer ? emitReplayBenchmarkEvent(kind, payload, meta) : false;
  const mediaTimeline = props.descriptor.media_timeline;
  const replayEnd = mediaTimeline.video.replayEnd;
  const durationMs = replayTickToMilliseconds(replayEnd);
  let primaryVideo: HTMLVideoElement | undefined;
  let controller!: PlaybackController;
  let decoderDiagnostics: ReturnType<typeof createPlaybackDiagnostics> | undefined;
  let metricsIntervalId: number | undefined;
  let mediaGeneration = observer?.nextMediaGeneration() ?? 0;
  let disposed = false;
  let endpointHold: EndpointHoldState | undefined;
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
      if (observer && !detailsFailureReported && benchmarkViewer) {
        detailsFailureReported = true;
        benchmarkViewer.detailsFailed(libraryError(detailsError()));
      }
      return;
    }
    const loaded = details();
    if (!loaded || !detailsReady() || payloadReported || (props.opening && benchmarkViewer?.isFinished())) return;
    payloadReported = true;
    if (props.opening && replayBenchmarkObserver() === observer) observer!.emitOpening("opening_details_ready", props.opening);
    benchmarkViewer?.detailsAdmitted();
    emitViewerBenchmarkEvent("playback_payload_ready", {
      event_count: activeWindow()?.events.length ?? 0, participant_count: roster()?.players.length ?? 0,
      duration_ms: durationMs, payload_scope: "selected_source_window",
    }, { required: true });
    queueMicrotask(() => { if (!disposed) benchmarkViewer?.updateReadiness(); });
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
    emitViewerBenchmarkEvent(event.kind, { ...event.payload,
      ...(props.opening ? openingPayload(props.opening) : {}),
    }, { generation: event.generation, actionId: event.actionId, required: true });
    benchmarkViewer?.onPlaybackEvent(event);
  };

  const updateMetrics = async () => {
    if (!controller || disposed) return;
    syncSnapshot(controller.sampleMetrics());
    try {
      const metrics = await loadServerMetrics();
      if (!disposed) setServerMetrics(metrics);
    } catch { /* Server diagnostics must not interrupt playback. */ }
  };

  // Shared production operation: current clip loop and requested preview are
  // applied identically for user and harness seeks. Evidence policy stays outside.
  const requestSeek = (target: ReplayTick, options: { reason?: string; playAfter?: boolean; actionId?: string } = {}): Promise<SeekOutcome> => {
    controller.setLoopRange(clipRange());
    const operation = controller.seek(target, options);
    const preview = controller.snapshot().seek.requestedPreview;
    if (preview !== null) setReplayPosition(preview);
    return operation;
  };

  const seekTo = (target: ReplayTick, options: { reason?: SeekReason; playAfter?: boolean } = {}): Promise<void> => {
    if (!controller) return Promise.resolve();
    const currentObserver = replayBenchmarkObserver();
    const actionId = currentObserver?.nextActionId("seek");
    return requestSeek(target, { reason: options.reason, playAfter: options.playAfter, actionId }).then(result => {
      if (currentObserver && (result.status === "failed" || result.status === "unavailable")) throw new Error(result.reason ?? result.status);
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

  const prepareEventSelection = (event: ViewerEvent) => {
    setEditingEndpoint(null);
    if (clipRange()) {
      if (event.replay_tick === undefined) return undefined;
      setClipRange(clipRangeForAnchor(event.replay_tick, event));
    }
    return event.replay_tick;
  };
  const selectEvent = (event: ViewerEvent) => {
    const target = prepareEventSelection(event);
    if (target !== undefined) void seekTo(target);
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

  const applyClipEndpoint = (endpoint: ClipEndpoint, requestedTick: ReplayTick, editing = false) => {
    const range = clipRange();
    if (!range) return null;
    const nextRange = moveClipEndpoint(range, endpoint, requestedTick, mediaTimeline);
    if (editing) setEditingEndpoint(endpoint);
    setClipRange(nextRange);
    return { range: nextRange, target: clipEndpointPreviewTick(nextRange, endpoint, mediaTimeline) };
  };
  const updateClipEndpoint = (endpoint: ClipEndpoint, requestedTick: ReplayTick) => {
    const updated = applyClipEndpoint(endpoint, requestedTick);
    if (!updated) return null;
    void seekTo(updated.target, { reason: "endpoint-edit" });
    return updated.range;
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

  let benchmarkFullscreenEventRequired = false;

  const enterFullscreen = (required = false) => {
    benchmarkFullscreenEventRequired ||= required;
    emitViewerBenchmarkEvent(
      "fullscreen_requested",
      { enabled: true },
      { generation: mediaGeneration, required },
    );
    setIsFullscreen(true);
  };

  const exitFullscreen = (required = false) => {
    benchmarkFullscreenEventRequired ||= required;
    emitViewerBenchmarkEvent(
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
      emitViewerBenchmarkEvent(
        "fullscreen_applied",
        { enabled: fullscreen, media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
        { generation: mediaGeneration, required: benchmarkFullscreenEventRequired },
      );
      benchmarkFullscreenEventRequired = false;
    }
  });

  createEffect(() => {
    playback(); detailsReady(); detailsError();
    benchmarkViewer?.updateReadiness();
  });

  onMount(() => {
    if (!primaryVideo) return;
    const video = primaryVideo;
    controller = createPlaybackController(createHtmlVideoPlaybackAdapter(video), { generation: mediaGeneration, eventSink: onPlaybackEvent });
    if (observer && replayBenchmarkObserver() === observer) {
      const mountToken = {}, elementToken = {};
      const completed = props.onBenchmarkViewerCycleCompleted;
      benchmarkViewer = createBenchmarkViewer(observer, {
        snapshot: controller.snapshot,
        details: () => ({ ready: detailsReady(), error: detailsError() }),
        presentationClock: clockSource,
        play: controller.play, pause: controller.pause, setRate: controller.setRate,
        requestSeek,
        prepareEventJump: target => {
          const selected = events()[nearestIndexAt(events(), target)];
          if (!selected) return { target };
          return { target: prepareEventSelection(selected)!, eventType: selected.event_type };
        },
        editClipEndpoint: (endpoint, target) => {
          if (!clipRange()) setClipRange(clipRangeForAnchor(target));
          const updated = applyClipEndpoint(endpoint, target, true);
          if (!updated) throw new Error("benchmark endpoint edit could not create a clip range");
          return { target: updated.target, start: clipStartTick(updated.range), end: clipEndTick(updated.range) };
        },
        finishClipEndpointEdit: () => setEditingEndpoint(null),
        setClipSelection: (enabled, anchor) => {
          if (enabled) setClipRange(clipRangeForAnchor(anchor ?? controller.snapshot().media.tick ?? replayPosition()));
          else cancelClipMode();
        },
        setFullscreen: (enabled, required) => { if (enabled) enterFullscreen(required); else exitFullscreen(required); },
        refreshMetrics: updateMetrics, sampleMetrics: controller.sampleMetrics, serverMetrics,
        release: controller.dispose, cycleCompleted: () => completed?.(),
        resize: {
          snapshot: controller.snapshot, play: controller.play, pause: controller.pause, subscribeFrames: controller.subscribeFrames,
          readMediaIdentity: () => ({ elementToken, connected: video.isConnected, currentSrc: video.currentSrc }),
        },
        render: {
          root: () => video.closest("section")?.querySelector('[data-testid="fullscreen-overlay"]') ?? null,
          read: () => {
            const current = controller.snapshot(), source = activeScope()?.source, tick = presentedPosition(), sample = overlaySample();
            const states = participantStates(), base = props.ddragonAssetBaseUrl;
            return { elementToken, connected: video.isConnected, assetsAvailable: Boolean(base),
              generation: current.generation, sessionToken: current.sessionToken, sourceUrl: current.sourceUrl,
              recoveryCount: current.recoveryCount, paused: current.media.paused,
              activeSource: source ?? null, overlaySource: overlay().scope?.source ?? null,
              windowSource: activeWindow()?.source ?? null, presentedTick: tick,
              rendered: tick !== null && sample && source && activeWindow() ? {
                generation: current.generation, mediaTick: tick, sourceEpoch: source.epoch, sourceToken: sourceKey(source),
                eventCount: events().length, samplePlayerCount: sample.players.length,
                players: participants().flatMap(player => {
                  const state = player.identity && states.get(participantKey(player.identity));
                  return state?.level === undefined ? [] : [{ name: player.summoner_name, level: state.level,
                    portraitUrl: `${base}/champion/${encodeURIComponent(player.champion)}`, spells: state.spells }];
                }),
              } : null };
          },
        },
      }, { replayEnd: replayEnd as ReplayTick, opening: props.opening, mountToken,
        isCurrentMount: token => token === mountToken && !disposed });
      if (detailsError()) {
        detailsFailureReported = true;
        benchmarkViewer.detailsFailed(libraryError(detailsError()));
      }
    }
    primaryVideo = undefined;
    decoderDiagnostics = createPlaybackDiagnostics((value) => {
      emitViewerBenchmarkEvent("decoder_observation", { status: value.status, reason: value.reason,
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
      if (props.opening && replayBenchmarkObserver() === observer) observer!.emitOpening("opening_media_open_requested", props.opening);
      void controller.open({ url: props.descriptor.video_url, mediaId: mediaTimeline.mediaId, timeline: mediaTimeline });
      if (props.opening && replayBenchmarkObserver() === observer) observer!.emitOpening("opening_media_open_bound", props.opening,
        { generation: controller.snapshot().generation });
      const initial = clipRange();
      if (initial) { controller.setLoopRange(initial); void seekTo(clipStartTick(initial)); }
    };
    // Targeted production decoder checks subscribe before opening their media.
    // Ordinary user opening remains independent of diagnostic readiness.
    if (observer && !props.opening) void decoderDiagnostics.ready.then(openMedia);
    else openMedia();
    emitViewerBenchmarkEvent("viewer_mounted", { duration_ms: durationMs,
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
    if (props.descriptor.video_url) benchmarkViewer?.armReadinessDeadline();
    onCleanup(() => {
      benchmarkViewer?.dispose();
      disposed = true;
      unsubscribe(); unsubscribeFrames(); controller.dispose();
      decoderDiagnostics?.dispose();
      window.removeEventListener("keydown", onKeyDown);
      document.body.classList.remove("replay-fullscreen");
      clearEndpointHold();
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
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
