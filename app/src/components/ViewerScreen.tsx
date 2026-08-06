import {
  Match,
  Show,
  Switch,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { loadPlaybackProbe, loadServerMetrics } from "../api";
import { formatBytes, formatDate, formatDuration } from "../format";
import type { EventMarker, PlaybackProbe, ServerMetrics } from "../types";
import styles from "./ViewerScreen.module.css";

type Props = {
  gameTimestamp: string;
  onBack: () => void;
};

type ChromiumPerformance = Performance & {
  memory?: { usedJSHeapSize: number };
};

const nearestMarkerIndex = (markers: EventMarker[], timeMs: number): number => {
  if (markers.length === 0) return -1;
  let low = 0;
  let high = markers.length - 1;
  while (low <= high) {
    const middle = (low + high) >>> 1;
    if (markers[middle].video_time_ms < timeMs) low = middle + 1;
    else high = middle - 1;
  }
  if (low === 0) return 0;
  if (low === markers.length) return markers.length - 1;
  return timeMs - markers[low - 1].video_time_ms <= markers[low].video_time_ms - timeMs
    ? low - 1
    : low;
};

function ViewerScreen(props: Props) {
  const [probe] = createResource(() => props.gameTimestamp, loadPlaybackProbe);

  return (
    <div class={styles.viewerScreen}>
      <button class={styles.backButton} type="button" onClick={props.onBack}>
        <span aria-hidden="true">←</span> Games
      </button>
      <Switch>
        <Match when={probe.loading}>
          <section class={styles.viewerState} aria-live="polite">
            Loading recording…
          </section>
        </Match>
        <Match when={probe.error}>
          <section class={styles.viewerState} role="alert">
            <strong>Recording could not be opened.</strong>
            <span>{String(probe.error)}</span>
          </section>
        </Match>
        <Match when={probe()}>{(loaded) => <PlaybackSurface probe={loaded()} />}</Match>
      </Switch>
    </div>
  );
}

function PlaybackSurface(props: { probe: PlaybackProbe }) {
  let video!: HTMLVideoElement;
  let frameCallbackId: number | undefined;
  let animationFrameId: number | undefined;
  let metricsIntervalId: number | undefined;
  let pendingSeekStartedAt: number | undefined;
  let frameSampleStartedAt = performance.now();
  let presentedFrames = 0;

  const markers = createMemo(() =>
    [...props.probe.markers].sort((left, right) => left.video_time_ms - right.video_time_ms),
  );
  const [videoTimeMs, setVideoTimeMs] = createSignal(0);
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
  const activeMarker = createMemo(() => {
    const index = nearestMarkerIndex(markers(), videoTimeMs());
    return index >= 0 ? markers()[index] : undefined;
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
      // Playback diagnostics must never interrupt the player.
    }
  };

  onMount(() => {
    if (!props.probe.video_url) return;
    const updateTime = () => setVideoTimeMs(video.currentTime * 1_000);
    const onPlay = () => {
      if (clockSource() === "ANIMATION FRAME" && animationFrameId === undefined) {
        animationFrameId = requestAnimationFrame(onAnimationFrame);
      }
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

    video.addEventListener("loadedmetadata", updateTime);
    video.addEventListener("pause", updateTime);
    video.addEventListener("play", onPlay);
    video.addEventListener("seeking", onSeeking);
    video.addEventListener("seeked", onSeeked);
    if (typeof video.requestVideoFrameCallback === "function") {
      frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
    } else {
      setClockSource("ANIMATION FRAME");
    }
    metricsIntervalId = window.setInterval(updateMetrics, 1_000);

    onCleanup(() => {
      video.removeEventListener("loadedmetadata", updateTime);
      video.removeEventListener("pause", updateTime);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("seeking", onSeeking);
      video.removeEventListener("seeked", onSeeked);
      if (frameCallbackId !== undefined && typeof video.cancelVideoFrameCallback === "function") {
        video.cancelVideoFrameCallback(frameCallbackId);
      }
      if (animationFrameId !== undefined) cancelAnimationFrame(animationFrameId);
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
    });
  });

  return (
    <section class={styles.playbackSurface}>
      <header class={styles.gameHeader}>
        <div>
          <p>LOCAL RECORDING</p>
          <h1>{props.probe.game.champion}</h1>
        </div>
        <dl>
          <div>
            <dt>K / D / A</dt>
            <dd>
              {props.probe.game.kills} / {props.probe.game.deaths} / {props.probe.game.assists}
            </dd>
          </div>
          <div>
            <dt>MODE</dt>
            <dd>{props.probe.game.game_mode}</dd>
          </div>
          <div>
            <dt>DURATION</dt>
            <dd>{formatDuration(props.probe.game.duration_ms)}</dd>
          </div>
          <div>
            <dt>CAPTURED</dt>
            <dd>{formatDate(props.probe.game.recorded_at, true)}</dd>
          </div>
        </dl>
      </header>

      <div class={styles.videoFrame}>
        <Show
          when={props.probe.video_url}
          fallback={
            <div class={styles.previewPlaceholder}>
              <strong>VIDEO PREVIEW</strong>
              <span>Open the Tauri app to attach a local recording.</span>
            </div>
          }
        >
          <video ref={video} src={props.probe.video_url} controls playsinline preload="metadata" />
        </Show>
      </div>

      <details class={styles.diagnostics} open={import.meta.env.DEV}>
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
            <dd>
              {droppedFrames()} / {totalFrames()}
            </dd>
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
            <dt>NEAREST EVENT</dt>
            <dd>{activeMarker()?.event_type ?? "—"}</dd>
          </div>
          <div>
            <dt>MARKERS INDEXED</dt>
            <dd>{markers().length}</dd>
          </div>
        </dl>
      </details>
    </section>
  );
}

export default ViewerScreen;
