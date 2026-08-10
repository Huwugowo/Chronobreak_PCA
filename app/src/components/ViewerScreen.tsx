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
import { loadPlaybackProbe, loadServerMetrics } from "../api";
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

type EndpointHoldState = {
  endpoint: ClipEndpoint;
  key: "ArrowLeft" | "ArrowRight";
  direction: -1 | 1;
  startedAt: number;
  startingFrame: number;
  animationFrameId: number;
};

const ENDPOINT_HOLD_THRESHOLD_MS = 200;

function ViewerScreen(props: Props) {
  const [probe] = createResource(() => props.gameTimestamp, loadPlaybackProbe);

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
  let pendingSeekStartedAt: number | undefined;
  let frameSampleStartedAt = performance.now();
  let presentedFrames = 0;
  let disposed = false;
  let clipLoopSeekPending = false;
  let endpointEdit: ClipEndpoint | undefined;
  let endpointHold: EndpointHoldState | undefined;

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
  const [mediaUnavailable, setMediaUnavailable] = createSignal(false);
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

  const syncPresentedTime = (presentedTimeMs: number) => {
    const range = clipRange();
    if (range && !video.paused && presentedTimeMs >= range.endMs) {
      if (!clipLoopSeekPending) {
        clipLoopSeekPending = true;
        seekTo(range.startMs);
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
    syncPresentedTime(frame.mediaTime * 1_000);
    sampleFrame();
    frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
  };

  const onAnimationFrame = () => {
    animationFrameId = undefined;
    syncPresentedTime(video.currentTime * 1_000);
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

  const seekTo = (requestedTimeMs: number) => {
    const range = clipRange();
    const targetTimeMs = range
      ? clamp(requestedTimeMs, range.startMs, range.endMs)
      : clamp(requestedTimeMs, 0, durationMs);
    pendingSeekStartedAt = performance.now();
    setVideoTimeMs(targetTimeMs);
    if (!props.probe.video_url) {
      setSeekLatencyMs(0);
      pendingSeekStartedAt = undefined;
      clipLoopSeekPending = false;
      return;
    }
    try {
      video.currentTime = targetTimeMs / 1_000;
    } catch {
      pendingSeekStartedAt = undefined;
    }
  };

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
    setClipRange(clipRangeForAnchor(anchorMs, anchorEvent));
    seekTo(videoTimeMs());
  };

  const selectEvent = (event: ViewerEvent) => {
    if (clipRange()) {
      setClipRange(clipRangeForAnchor(event.video_time_ms, event));
    }
    seekTo(event.video_time_ms);
  };

  const exportSelectedClip = () => {
    const range = clipRange();
    if (!range) return;
    clearEndpointHold();
    endpointEdit = undefined;
    video.pause();
    setIsFullscreen(false);
    props.onExportClip({
      gameTimestamp: props.probe.game.timestamp,
      clipStartMs: range.startMs,
      clipEndMs: range.endMs,
    });
  };

  const updateClipEndpoint = (endpoint: ClipEndpoint, requestedMs: number) => {
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
    seekTo(clipEndpointPreviewMs(nextRange, endpoint, props.probe.recording_fps));
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
    endpointEdit = endpoint;
    video.pause();
    seekTo(clipEndpointPreviewMs(range, endpoint, props.probe.recording_fps));
  };

  const finishClipEndpointEdit = () => {
    clearEndpointHold();
    endpointEdit = undefined;
    const range = clipRange();
    if (!range) return;
    seekTo(range.startMs);
    if (!props.probe.video_url || mediaUnavailable()) return;
    void video.play().catch(() => setMediaUnavailable(true));
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
    if (endpointEdit !== endpoint) return;
    if (endpointHold) applyEndpointHoldTime(endpointHold, performance.now());
    finishClipEndpointEdit();
  };

  const cancelClipMode = () => {
    clearEndpointHold();
    endpointEdit = undefined;
    clipLoopSeekPending = false;
    setClipRange(null);
  };

  const seekFromRail = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (bounds.width <= 0) return;
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
    seekTo(target);
  };

  const togglePlayback = async () => {
    if (!props.probe.video_url || mediaUnavailable()) return;
    if (video.paused) {
      const range = clipRange();
      const currentTimeMs = video.currentTime * 1_000;
      if (range && (currentTimeMs < range.startMs || currentTimeMs >= range.endMs)) {
        seekTo(range.startMs);
      }
      try {
        await video.play();
      } catch {
        setMediaUnavailable(true);
      }
    } else {
      video.pause();
    }
  };

  const enterFullscreen = () => {
    setIsFullscreen(true);
  };

  const exitFullscreen = () => setIsFullscreen(false);

  createEffect(() => {
    document.body.classList.toggle("replay-fullscreen", isFullscreen());
  });

  onMount(() => {
    const updateTime = () => syncPresentedTime(video.currentTime * 1_000);
    const onLoadedMetadata = () => {
      setMediaUnavailable(false);
      const range = clipRange();
      if (range) seekTo(range.startMs);
      else updateTime();
    };
    const onPlay = () => {
      setIsPlaying(true);
      if (clockSource() === "ANIMATION FRAME" && animationFrameId === undefined) {
        animationFrameId = requestAnimationFrame(onAnimationFrame);
      }
    };
    const onPause = () => {
      setIsPlaying(false);
      updateTime();
    };
    const onSeeking = () => {
      pendingSeekStartedAt ??= performance.now();
    };
    const onSeeked = () => {
      clipLoopSeekPending = false;
      updateTime();
      if (pendingSeekStartedAt !== undefined) {
        setSeekLatencyMs(performance.now() - pendingSeekStartedAt);
        pendingSeekStartedAt = undefined;
      }
    };
    const onEnded = () => {
      const range = clipRange();
      if (!range) {
        onPause();
        return;
      }
      clipLoopSeekPending = false;
      seekTo(range.startMs);
      void video.play().catch(() => setMediaUnavailable(true));
    };
    const onError = () => setMediaUnavailable(true);
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
    }
    metricsIntervalId = window.setInterval(() => void updateMetrics(), 1_000);

    onCleanup(() => {
      disposed = true;
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
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
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
              preload="metadata"
              aria-label={`${props.probe.game.champion} replay video`}
              onClick={() => void togglePlayback()}
              onDblClick={(event) => {
                event.preventDefault();
                enterFullscreen();
              }}
            />
            <Show when={!props.probe.video_url || mediaUnavailable()}>
              <div class={styles.previewPlaceholder}>
                <span class={styles.previewGlyph} aria-hidden="true">LR</span>
                <strong>{mediaUnavailable() ? "VIDEO UNAVAILABLE" : "INTERACTIVE PREVIEW"}</strong>
                <span>
                  {mediaUnavailable()
                    ? "The local recording could not be decoded."
                    : "Timeline data is active; local video attaches in the desktop app."}
                </span>
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
                mediaAvailable={Boolean(props.probe.video_url) && !mediaUnavailable()}
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
                onExit={exitFullscreen}
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
              disabled={!props.probe.video_url || mediaUnavailable()}
              onClick={() => void togglePlayback()}
              aria-label={isPlaying() ? "Pause replay" : "Play replay"}
            >
              <span aria-hidden="true">{isPlaying() ? "Ⅱ" : "▶"}</span>
              {isPlaying() ? "PAUSE" : "PLAY"}
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
              onClick={enterFullscreen}
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
            </dl>
          </details>
        </Show>
      </div>

    </section>
  );
}

export default ViewerScreen;
