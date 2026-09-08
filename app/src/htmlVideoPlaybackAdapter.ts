import {
  browserSecondsForReplayTick,
  replayTickForBrowserSeconds,
  type MediaTimelineV2,
  type ReplayTick,
} from "./replayTime";

export type MediaObservation = Readonly<{
  source: string;
  readyState: number;
  networkState: number;
  error: Readonly<{ code: number; message: string }> | null;
  tick: ReplayTick | null;
  paused: boolean;
  ended: boolean;
  seeking: boolean;
  rate: number;
  muted: boolean;
  volume: number;
  hidden: boolean;
  // Native coordinates are only exposed for diagnostics/legacy benchmark serialization.
  browserSeconds: number;
  durationSeconds: number;
  width: number;
  height: number;
}>;

export type PlaybackQuality = Readonly<{
  totalFrames: number | null;
  droppedFrames: number | null;
  heapBytes: number | null;
}>;

export type PlaybackMediaEvent =
  | "loadstart" | "loadedmetadata" | "loadeddata" | "canplay" | "playing"
  | "play" | "pause" | "seeking" | "seeked" | "ended" | "error"
  | "waiting" | "stalled" | "timeupdate" | "ratechange" | "volumechange"
  | "visibilitychange";

/** Injection seam for the single HTML video implementation, not a backend API. */
export interface PlaybackMediaAdapter {
  readonly hasVideoFrameCallback: boolean;
  open(url: string, timeline: MediaTimelineV2): void;
  read(): MediaObservation;
  quality(): PlaybackQuality;
  seek(tick: ReplayTick): void;
  play(): Promise<void>;
  pause(): void;
  setRate(rate: number): void;
  setMuted(muted: boolean): void;
  setVolume(volume: number): void;
  listen(event: PlaybackMediaEvent, callback: () => void): () => void;
  requestFrame(callback: (tick: ReplayTick | null, atMs: number) => void): number;
  cancelFrame(handle: number): void;
  release(): void;
}

export function createHtmlVideoPlaybackAdapter(video: HTMLVideoElement): PlaybackMediaAdapter {
  let timeline: MediaTimelineV2 | undefined;
  const tickFor = (seconds: number): ReplayTick | null => {
    if (!timeline) return null;
    try {
      const tick = replayTickForBrowserSeconds(timeline, seconds);
      return tick <= timeline.video.replayEnd ? tick : null;
    } catch {
      return null;
    }
  };
  video.preload = "auto";
  video.playsInline = true;
  video.preservesPitch = true;
  return {
    hasVideoFrameCallback: typeof video.requestVideoFrameCallback === "function",
    open(url, mediaTimeline) {
      timeline = mediaTimeline;
      video.src = url;
      video.load();
    },
    read: () => ({
      source: video.currentSrc,
      readyState: video.readyState,
      networkState: video.networkState,
      error: video.error ? { code: video.error.code, message: video.error.message } : null,
      tick: tickFor(video.currentTime),
      paused: video.paused,
      ended: video.ended,
      seeking: video.seeking,
      rate: video.playbackRate,
      muted: video.muted,
      volume: video.volume,
      hidden: document.hidden,
      browserSeconds: video.currentTime,
      durationSeconds: video.duration,
      width: video.videoWidth,
      height: video.videoHeight,
    }),
    quality() {
      const quality = video.getVideoPlaybackQuality?.();
      const performanceWithMemory = performance as Performance & { memory?: { usedJSHeapSize: number } };
      return {
        totalFrames: quality?.totalVideoFrames ?? null,
        droppedFrames: quality?.droppedVideoFrames ?? null,
        heapBytes: performanceWithMemory.memory?.usedJSHeapSize ?? null,
      };
    },
    seek(tick) {
      if (!timeline) throw new Error("Playback media is closed");
      video.currentTime = browserSecondsForReplayTick(timeline, tick);
    },
    play: () => video.play(),
    pause: () => video.pause(),
    setRate: (rate) => { video.playbackRate = rate; },
    setMuted: (muted) => { video.muted = muted; },
    setVolume: (volume) => { video.volume = volume; },
    listen(event, callback) {
      const target = event === "visibilitychange" ? document : video;
      target.addEventListener(event, callback);
      return () => target.removeEventListener(event, callback);
    },
    requestFrame: (callback) => video.requestVideoFrameCallback((at, frame) => callback(tickFor(frame.mediaTime), at)),
    cancelFrame: (handle) => video.cancelVideoFrameCallback(handle),
    release() {
      video.pause();
      video.removeAttribute("src");
      video.load();
      timeline = undefined;
    },
  };
}
