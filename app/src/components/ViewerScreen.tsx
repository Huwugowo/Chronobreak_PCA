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
  KdaTimelinePoint,
  PlaybackProbe,
  PlayerTimelinePoint,
  ServerMetrics,
} from "../types";
import { clamp, eventSummary, eventTitle, latestIndexAt, timelineValue } from "../viewerUtils";
import FullscreenOverlay from "./FullscreenOverlay";
import styles from "./ViewerScreen.module.css";

type Props = {
  gameTimestamp: string;
  onBack: () => void;
};

type ViewerTab = "replay" | "stats";

type ChromiumPerformance = Performance & {
  memory?: { usedJSHeapSize: number };
};

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
        <Match when={probe()}>{(loaded) => <PlaybackSurface probe={loaded()} onBack={props.onBack} />}</Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: { probe: PlaybackProbe; onBack: () => void }) {
  let video!: HTMLVideoElement;
  let eventFeed!: HTMLDivElement;
  let eventRows: HTMLButtonElement[] = [];
  let frameCallbackId: number | undefined;
  let animationFrameId: number | undefined;
  let metricsIntervalId: number | undefined;
  let pendingSeekStartedAt: number | undefined;
  let frameSampleStartedAt = performance.now();
  let presentedFrames = 0;
  let previousActiveEvent = -1;
  let disposed = false;

  const events = [...props.probe.events].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const playerTimeline = [...props.probe.player_timeline].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const kdaTimeline = [...props.probe.kda_timeline].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const goldTimeline = [...props.probe.gold_timeline].sort(
    (left, right) => left.video_time_ms - right.video_time_ms,
  );
  const lastEventTimeMs = events[events.length - 1]?.video_time_ms ?? 0;
  const durationMs = Math.max(props.probe.game.duration_ms, lastEventTimeMs, 1);
  const markerPositions = events.map((event, index) => ({
    event,
    index,
    position: clamp((event.video_time_ms / durationMs) * 100, 0, 100),
    tone: event.relation,
  }));

  const [activeTab, setActiveTab] = createSignal<ViewerTab>("replay");
  const [isFullscreen, setIsFullscreen] = createSignal(false);
  const [goldOpen, setGoldOpen] = createSignal(false);
  const [fullscreenEventPanelOpen, setFullscreenEventPanelOpen] = createSignal(false);
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

  const progress = createMemo(() => clamp((videoTimeMs() / durationMs) * 100, 0, 100));
  const videoSecond = createMemo(() => Math.floor(videoTimeMs() / 1_000));
  const gameClockSecond = createMemo(() =>
    Math.max(0, Math.floor((videoTimeMs() - props.probe.game_start_video_offset_ms) / 1_000)),
  );
  const beforeGameStart = createMemo(
    () => videoTimeMs() < props.probe.game_start_video_offset_ms,
  );
  const activeEventIndex = createMemo(() => latestIndexAt(events, videoTimeMs()));
  const activeEvent = createMemo(() => {
    const index = activeEventIndex();
    return index < 0 ? undefined : events[index];
  });
  const currentPlayer = createMemo<PlayerTimelinePoint | undefined>(() =>
    timelineValue(playerTimeline, videoTimeMs()),
  );
  const currentKda = createMemo<KdaTimelinePoint | undefined>(() =>
    timelineValue(kdaTimeline, videoTimeMs()),
  );

  createEffect(() => {
    const index = activeEventIndex();
    if (index === previousActiveEvent) return;
    if (previousActiveEvent >= 0) {
      const previous = eventRows[previousActiveEvent];
      previous?.classList.remove(styles.eventActive);
      previous?.removeAttribute("aria-current");
    }
    if (index >= 0) {
      const current = eventRows[index];
      current?.classList.add(styles.eventActive);
      current?.setAttribute("aria-current", "true");
      if (eventFeed && current) current.scrollIntoView({ block: "nearest" });
    }
    previousActiveEvent = index;
  });

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

  const onVideoFrame: VideoFrameRequestCallback = (_now, frame) => {
    if (disposed) return;
    setVideoTimeMs(frame.mediaTime * 1_000);
    sampleFrame();
    frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
  };

  const onAnimationFrame = () => {
    animationFrameId = undefined;
    setVideoTimeMs(video.currentTime * 1_000);
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
    const targetTimeMs = clamp(requestedTimeMs, 0, durationMs);
    pendingSeekStartedAt = performance.now();
    setVideoTimeMs(targetTimeMs);
    if (!props.probe.video_url) {
      setSeekLatencyMs(0);
      pendingSeekStartedAt = undefined;
      return;
    }
    try {
      video.currentTime = targetTimeMs / 1_000;
    } catch {
      pendingSeekStartedAt = undefined;
    }
  };

  const seekFromRail = (event: PointerEvent & { currentTarget: HTMLDivElement }) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    if (bounds.width <= 0) return;
    seekTo(((event.clientX - bounds.left) / bounds.width) * durationMs);
  };

  const handleRailKey = (event: KeyboardEvent) => {
    let target: number | undefined;
    if (event.key === "ArrowLeft" || event.key === "ArrowDown") target = videoTimeMs() - 5_000;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") target = videoTimeMs() + 5_000;
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
    if (activeTab() === "replay") setIsFullscreen(true);
  };

  const exitFullscreen = () => setIsFullscreen(false);

  const selectTab = (tab: ViewerTab) => {
    if (tab === "stats") {
      video.pause();
      exitFullscreen();
    }
    setActiveTab(tab);
  };

  createEffect(() => {
    document.body.classList.toggle("replay-fullscreen", isFullscreen());
  });

  onMount(() => {
    const updateTime = () => setVideoTimeMs(video.currentTime * 1_000);
    const onLoadedMetadata = () => {
      setMediaUnavailable(false);
      updateTime();
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
      updateTime();
      if (pendingSeekStartedAt !== undefined) {
        setSeekLatencyMs(performance.now() - pendingSeekStartedAt);
        pendingSeekStartedAt = undefined;
      }
    };
    const onError = () => setMediaUnavailable(true);
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return;
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
      if (event.key === "Escape" && isFullscreen()) {
        event.preventDefault();
        exitFullscreen();
        return;
      }
      if (event.key.toLowerCase() === "f" && activeTab() === "replay") {
        event.preventDefault();
        setIsFullscreen((fullscreen) => !fullscreen);
      }
    };

    video.addEventListener("loadedmetadata", onLoadedMetadata);
    video.addEventListener("pause", onPause);
    video.addEventListener("play", onPlay);
    video.addEventListener("ended", onPause);
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
      video.removeEventListener("ended", onPause);
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
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
    });
  });

  return (
    <section class={styles.playbackSurface} data-testid="viewer-surface">
      <div class={styles.viewerTopline}>
        <button class={styles.backButton} type="button" onClick={props.onBack}>
          <span aria-hidden="true">←</span> GAMES
        </button>
        <nav class={styles.viewerTabs} aria-label="Viewer sections" role="tablist">
          <button
            classList={{ [styles.tabActive]: activeTab() === "replay" }}
            type="button"
            role="tab"
            aria-selected={activeTab() === "replay"}
            onClick={() => selectTab("replay")}
          >
            <span aria-hidden="true">▶</span> REPLAY
          </button>
          <button
            classList={{ [styles.tabActive]: activeTab() === "stats" }}
            type="button"
            role="tab"
            aria-selected={activeTab() === "stats"}
            onClick={() => selectTab("stats")}
          >
            <span aria-hidden="true">≡</span> STATS
          </button>
        </nav>
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

      <div
        classList={{
          [styles.replayView]: true,
          [styles.replayViewHidden]: activeTab() !== "replay",
        }}
        role="tabpanel"
        aria-label="Replay"
        aria-hidden={activeTab() !== "replay"}
        data-testid="replay-panel"
      >
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
                events={events}
                goldTimeline={goldTimeline}
                durationMs={durationMs}
                videoTimeMs={videoTimeMs()}
                gameClockSeconds={gameClockSecond()}
                beforeGameStart={beforeGameStart()}
                currentKda={currentKda()}
                currentPlayer={currentPlayer()}
                isPlaying={isPlaying()}
                mediaAvailable={Boolean(props.probe.video_url) && !mediaUnavailable()}
                goldOpen={goldOpen()}
                eventPanelOpen={fullscreenEventPanelOpen()}
                onGoldOpenChange={setGoldOpen}
                onEventPanelOpenChange={setFullscreenEventPanelOpen}
                onTogglePlayback={() => void togglePlayback()}
                onSeek={seekTo}
                onExit={exitFullscreen}
              />
            </Show>
          </div>

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

            <section class={styles.eventPanel}>
              <div class={styles.eventPanelHeader}>
                <div>
                  <span>MATCH EVENTS</span>
                  <strong>{events.length.toString().padStart(2, "0")}</strong>
                </div>
                <span>GAME TIME</span>
              </div>
              <div class={styles.eventFeed} ref={eventFeed} data-testid="event-feed">
                <Show
                  when={events.length > 0}
                  fallback={<p class={styles.emptyEvents}>NO EVENTS CAPTURED</p>}
                >
                  <For each={events}>
                    {(event, index) => (
                      <button
                        ref={(element) => {
                          eventRows[index()] = element;
                        }}
                        class={`${styles.eventRow} ${styles[`eventTone${event.relation}`]}`}
                        type="button"
                        data-event-index={index()}
                        onClick={() => seekTo(event.video_time_ms)}
                      >
                        <span class={styles.eventLine} aria-hidden="true" />
                        <span class={styles.eventCopy}>
                          <strong>{eventTitle(event)}</strong>
                          <small>{eventSummary(event)}</small>
                        </span>
                        <time>{formatDuration(event.game_time_ms)}</time>
                      </button>
                    )}
                  </For>
                </Show>
              </div>
            </section>
          </aside>
        </div>

        <section class={styles.timelinePanel} aria-label="Replay timeline">
          <div
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
            <span class={styles.scrubberHead} style={`left:${progress()}%`} />
            <div class={styles.markers}>
              <For each={markerPositions}>
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
                      seekTo(marker.event.video_time_ms);
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
            <div class={styles.timeReadout} data-testid="time-readout">
              <strong>{formatDuration(videoSecond() * 1_000)}</strong>
              <span>/</span>
              <span>{formatDuration(durationMs)}</span>
            </div>
            <span class={styles.controlDivider} />
            <div class={styles.nowPlaying}>
              <span>NOW</span>
              <strong>{activeEvent() ? eventTitle(activeEvent()!) : "LOADING SCREEN"}</strong>
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

        <Show when={import.meta.env.DEV}>
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
                <dd>{events.length} EVENTS</dd>
              </div>
            </dl>
          </details>
        </Show>
      </div>

      <section
        classList={{
          [styles.statsPlaceholder]: true,
          [styles.statsPlaceholderHidden]: activeTab() !== "stats",
        }}
        role="tabpanel"
        aria-label="Stats"
        aria-hidden={activeTab() !== "stats"}
        data-testid="stats-placeholder"
      >
        <span>STATS / PHASE 06</span>
        <strong>MATCH SCOREBOARD</strong>
        <p>Stats arrives in a later phase. Your replay position is preserved.</p>
      </section>
    </section>
  );
}

export default ViewerScreen;
