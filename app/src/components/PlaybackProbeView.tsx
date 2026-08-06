import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { loadServerMetrics } from "../api";
import type { EventMarker, PlaybackProbe, ServerMetrics } from "../types";
import styles from "./PlaybackProbeView.module.css";

type Props = {
  probe: PlaybackProbe;
};

type ChromiumPerformance = Performance & {
  memory?: {
    usedJSHeapSize: number;
  };
};

const formatDuration = (milliseconds: number): string => {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`
    : `${minutes}:${seconds.toString().padStart(2, "0")}`;
};

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = -1;
  do {
    value /= 1024;
    unit += 1;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[unit]}`;
};

const nearestMarkerIndex = (markers: EventMarker[], videoTimeMs: number): number => {
  if (markers.length === 0) return -1;
  let low = 0;
  let high = markers.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (markers[middle].video_time_ms < videoTimeMs) low = middle + 1;
    else high = middle;
  }
  if (low > 0) {
    const previousDistance = Math.abs(markers[low - 1].video_time_ms - videoTimeMs);
    const currentDistance = Math.abs(markers[low].video_time_ms - videoTimeMs);
    if (previousDistance <= currentDistance) return low - 1;
  }
  return low;
};

const formatDate = (value: string): string => {
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? "UNKNOWN DATE"
    : new Intl.DateTimeFormat(undefined, {
        day: "2-digit",
        month: "short",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      })
        .format(date)
        .toUpperCase();
};

function PlaybackProbeView(props: Props) {
  let video!: HTMLVideoElement;
  let frameCallbackId: number | undefined;
  let animationFrameId: number | undefined;
  let metricsIntervalId: number | undefined;
  let pendingSeekStartedAt: number | undefined;
  let presentedFrames = 0;
  let frameSampleStartedAt = performance.now();

  const sortedMarkers = createMemo(() =>
    [...props.probe.markers].sort((left, right) => left.video_time_ms - right.video_time_ms),
  );
  const [videoTimeMs, setVideoTimeMs] = createSignal(0);
  const [durationMs, setDurationMs] = createSignal(props.probe.game.duration_ms);
  const [playing, setPlaying] = createSignal(false);
  const [clockSource, setClockSource] = createSignal<"VIDEO FRAME" | "ANIMATION FRAME">(
    "VIDEO FRAME",
  );
  const [presentedFps, setPresentedFps] = createSignal(0);
  const [droppedFrames, setDroppedFrames] = createSignal(0);
  const [totalFrames, setTotalFrames] = createSignal(0);
  const [heapBytes, setHeapBytes] = createSignal<number | null>(null);
  const [seekLatencyMs, setSeekLatencyMs] = createSignal<number | null>(null);
  const [serverMetrics, setServerMetrics] = createSignal<ServerMetrics>({
    requests: 0,
    range_requests: 0,
    response_bytes: 0,
  });

  const progress = createMemo(() =>
    durationMs() > 0 ? Math.min(1, Math.max(0, videoTimeMs() / durationMs())) : 0,
  );
  const activeMarkerIndex = createMemo(() => nearestMarkerIndex(sortedMarkers(), videoTimeMs()));
  const activeMarker = createMemo(() => {
    const index = activeMarkerIndex();
    return index >= 0 ? sortedMarkers()[index] : undefined;
  });
  const stressMarkers = createMemo(() =>
    import.meta.env.DEV
      ? Array.from({ length: 480 }, (_, index) => ({
          position: ((index * 73) % 479) / 478,
          hostile: index % 3 === 0,
        }))
      : [],
  );

  const sampleFrames = () => {
    presentedFrames += 1;
    const now = performance.now();
    const elapsed = now - frameSampleStartedAt;
    if (elapsed >= 1_000) {
      setPresentedFps((presentedFrames * 1_000) / elapsed);
      presentedFrames = 0;
      frameSampleStartedAt = now;
    }
  };

  const onVideoFrame: VideoFrameRequestCallback = (_now, frame) => {
    setVideoTimeMs(frame.mediaTime * 1_000);
    sampleFrames();
    frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
  };

  const onAnimationFrame = () => {
    animationFrameId = undefined;
    setVideoTimeMs(video.currentTime * 1_000);
    sampleFrames();
    if (!video.paused) animationFrameId = requestAnimationFrame(onAnimationFrame);
  };

  const startClock = () => {
    if (typeof video.requestVideoFrameCallback === "function") {
      setClockSource("VIDEO FRAME");
      frameCallbackId = video.requestVideoFrameCallback(onVideoFrame);
      return;
    }
    setClockSource("ANIMATION FRAME");
    animationFrameId = requestAnimationFrame(onAnimationFrame);
  };

  const updateStationaryTime = () => setVideoTimeMs(video.currentTime * 1_000);

  const updatePlaybackQuality = async () => {
    if (typeof video.getVideoPlaybackQuality === "function") {
      const quality = video.getVideoPlaybackQuality();
      setDroppedFrames(quality.droppedVideoFrames);
      setTotalFrames(quality.totalVideoFrames);
    }
    const memory = (performance as ChromiumPerformance).memory;
    setHeapBytes(memory?.usedJSHeapSize ?? null);
    try {
      setServerMetrics(await loadServerMetrics());
    } catch {
      // Diagnostics must never interrupt playback.
    }
  };

  const seekBy = (seconds: number) => {
    if (!props.probe.video_url || !Number.isFinite(video.duration)) return;
    pendingSeekStartedAt = performance.now();
    video.currentTime = Math.min(video.duration, Math.max(0, video.currentTime + seconds));
  };

  onMount(() => {
    if (!props.probe.video_url) return;
    const onMetadata = () => {
      if (Number.isFinite(video.duration)) setDurationMs(video.duration * 1_000);
      updateStationaryTime();
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      setPlaying(false);
      updateStationaryTime();
    };
    const onSeeked = () => {
      updateStationaryTime();
      if (pendingSeekStartedAt !== undefined) {
        setSeekLatencyMs(performance.now() - pendingSeekStartedAt);
        pendingSeekStartedAt = undefined;
      }
    };

    video.addEventListener("loadedmetadata", onMetadata);
    video.addEventListener("durationchange", onMetadata);
    video.addEventListener("play", onPlay);
    video.addEventListener("pause", onPause);
    video.addEventListener("seeked", onSeeked);
    startClock();
    metricsIntervalId = window.setInterval(updatePlaybackQuality, 1_000);

    onCleanup(() => {
      video.removeEventListener("loadedmetadata", onMetadata);
      video.removeEventListener("durationchange", onMetadata);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("seeked", onSeeked);
      if (frameCallbackId !== undefined && typeof video.cancelVideoFrameCallback === "function") {
        video.cancelVideoFrameCallback(frameCallbackId);
      }
      if (animationFrameId !== undefined) cancelAnimationFrame(animationFrameId);
      if (metricsIntervalId !== undefined) window.clearInterval(metricsIntervalId);
    });
  });

  createEffect(() => {
    if (clockSource() === "ANIMATION FRAME" && playing() && animationFrameId === undefined) {
      animationFrameId = requestAnimationFrame(onAnimationFrame);
    }
  });

  return (
    <section class={styles.probeGrid}>
      <div class={styles.viewerColumn}>
        <div class={styles.gameHeader}>
          <div>
            <p class={styles.sectionLabel}>LARGEST LOCAL RECORDING</p>
            <h2>{props.probe.game.champion}</h2>
          </div>
          <dl class={styles.gameFacts}>
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
              <dt>FILE</dt>
              <dd>{formatBytes(props.probe.game.video_size_bytes)}</dd>
            </div>
            <div>
              <dt>CAPTURED</dt>
              <dd>{formatDate(props.probe.game.recorded_at)}</dd>
            </div>
          </dl>
        </div>

        <div class={styles.videoShell} data-playing={playing()}>
          <Show
            when={props.probe.video_url}
            fallback={
              <div class={styles.previewPlaceholder}>
                <p>UI PREVIEW</p>
                <span>Open through Tauri to attach the local video stream.</span>
              </div>
            }
          >
            <video
              ref={video}
              class={styles.video}
              src={props.probe.video_url}
              controls
              playsinline
              preload="metadata"
            />
          </Show>

          <div class={styles.videoHud} aria-hidden="true">
            <span>{formatDuration(videoTimeMs())}</span>
            <span>{playing() ? "PLAY" : "HOLD"}</span>
          </div>
          <div class={styles.hoverCard}>
            <span>NEAREST EVENT</span>
            <strong>{activeMarker()?.event_type ?? "NO EVENT"}</strong>
            <small>
              {activeMarker() ? formatDuration(activeMarker()!.video_time_ms) : "—"}
            </small>
          </div>
        </div>

        <div class={styles.timeline} aria-label="Playback performance timeline">
          <div class={styles.timelineFill} style={{ transform: `scaleX(${progress()})` }} />
          <div class={styles.stressLayer} aria-hidden="true">
            <For each={stressMarkers()}>
              {(marker) => (
                <i
                  classList={{ [styles.stressHostile]: marker.hostile }}
                  style={{ left: `${marker.position * 100}%` }}
                />
              )}
            </For>
          </div>
          <For each={sortedMarkers()}>
            {(marker, index) => (
              <span
                class={styles.eventMarker}
                classList={{ [styles.activeMarker]: index() === activeMarkerIndex() }}
                style={{ left: `${(marker.video_time_ms / Math.max(durationMs(), 1)) * 100}%` }}
                title={`${marker.event_type} · ${formatDuration(marker.video_time_ms)}`}
              />
            )}
          </For>
          <span class={styles.playhead} style={{ transform: `scaleX(${progress()})` }} />
        </div>

        <div class={styles.transportRow}>
          <button type="button" onClick={() => seekBy(-60)} disabled={!props.probe.video_url}>
            − 60 SEC
          </button>
          <button type="button" onClick={() => seekBy(60)} disabled={!props.probe.video_url}>
            + 60 SEC
          </button>
          <p>
            {formatDuration(videoTimeMs())} <span>/</span> {formatDuration(durationMs())}
          </p>
        </div>
      </div>

      <aside class={styles.diagnostics}>
        <div class={styles.diagnosticHeader}>
          <div>
            <p class={styles.sectionLabel}>LIVE INSTRUMENTATION</p>
            <h2>Hot path</h2>
          </div>
          <span class={styles.statusDot} title="Diagnostics active" />
        </div>

        <div class={styles.metricHero}>
          <strong>{presentedFps().toFixed(1)}</strong>
          <span>PRESENTED FPS</span>
        </div>

        <dl class={styles.metrics}>
          <div>
            <dt>CLOCK SOURCE</dt>
            <dd>{clockSource()}</dd>
          </div>
          <div>
            <dt>DROPPED FRAMES</dt>
            <dd>
              {droppedFrames()} <small>/ {totalFrames()}</small>
            </dd>
          </div>
          <div>
            <dt>LAST SEEK</dt>
            <dd>{seekLatencyMs() === null ? "—" : `${seekLatencyMs()!.toFixed(0)} ms`}</dd>
          </div>
          <div>
            <dt>JS HEAP</dt>
            <dd>{heapBytes() === null ? "UNAVAILABLE" : formatBytes(heapBytes()!)}</dd>
          </div>
          <div>
            <dt>RANGE REQUESTS</dt>
            <dd>
              {serverMetrics().range_requests} <small>/ {serverMetrics().requests}</small>
            </dd>
          </div>
          <div>
            <dt>RESPONSE BYTES</dt>
            <dd>{formatBytes(serverMetrics().response_bytes)}</dd>
          </div>
          <div>
            <dt>REAL MARKERS</dt>
            <dd>{sortedMarkers().length}</dd>
          </div>
          <div>
            <dt>DEV STRESS NODES</dt>
            <dd>{stressMarkers().length}</dd>
          </div>
        </dl>

        <div class={styles.invariantList}>
          <p>PERFORMANCE INVARIANTS</p>
          <ul>
            <li>One persistent native video node</li>
            <li>Byte-range streaming from Rust</li>
            <li>Frame callback drives only hot DOM</li>
            <li>Transform and opacity motion</li>
          </ul>
        </div>
      </aside>
    </section>
  );
}

export default PlaybackProbeView;
