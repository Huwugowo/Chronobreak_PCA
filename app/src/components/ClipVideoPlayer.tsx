import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { formatDuration } from "../format";
import { PLAYBACK_RATES, type PlaybackRate } from "../playbackController";
import { createPlaybackHud } from "./createPlaybackHud";
import MediaTransport from "./MediaTransport";
import styles from "./ClipVideoPlayer.module.css";
import hudStyles from "./FullscreenOverlay.module.css";

export default function ClipVideoPlayer(props: { src: string; compact?: boolean }) {
  let surface!: HTMLDivElement;
  let video!: HTMLVideoElement;
  const [playing, setPlaying] = createSignal(false);
  const [fullscreen, setFullscreen] = createSignal(false);
  const [duration, setDuration] = createSignal(0);
  const [time, setTime] = createSignal(0);
  const [failed, setFailed] = createSignal(false);
  const [playback, setPlayback] = createSignal({
    rate: { selected: 1 as PlaybackRate },
    media: { muted: false, volume: 1 },
    readiness: "ready" as const,
  });

  const hud = createPlaybackHud(() => fullscreen() && playing());
  const syncPlaying = () => setPlaying(!video.paused && !video.ended && !video.error);
  const syncTime = () => setTime(video.currentTime);
  const syncDuration = () => setDuration(Number.isFinite(video.duration) ? video.duration : 0);
  const syncPlayback = () => setPlayback({
    rate: { selected: PLAYBACK_RATES.find(rate => rate === video.playbackRate) ?? 1 },
    media: { muted: video.muted, volume: video.volume },
    readiness: "ready",
  });
  const seek = (seconds: number) => {
    if (duration() <= 0) return;
    video.currentTime = Math.min(Math.max(seconds, 0), duration());
    syncTime();
  };
  const togglePlayback = () => {
    if (video.error) return;
    if (video.paused || video.ended) {
      void video.play().catch(syncPlaying);
    } else {
      video.pause();
    }
  };
  const toggleFullscreen = () => {
    const transition = document.fullscreenElement === surface
      ? document.exitFullscreen()
      : surface.requestFullscreen();
    void transition.catch(() => setFullscreen(document.fullscreenElement === surface));
  };

  onMount(() => {
    const syncFullscreen = () => setFullscreen(document.fullscreenElement === surface);
    const exitOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && document.fullscreenElement === surface) {
        event.preventDefault();
        if (!event.repeat) toggleFullscreen();
      }
    };
    document.addEventListener("fullscreenchange", syncFullscreen);
    window.addEventListener("keydown", exitOnEscape, true);
    onCleanup(() => {
      document.removeEventListener("fullscreenchange", syncFullscreen);
      window.removeEventListener("keydown", exitOnEscape, true);
    });
  });

  return (
    <div
      classList={{ [styles.surface]: true, [styles.compact]: props.compact }}
      ref={element => { surface = element; hud.setRoot(element); }}
      onKeyDown={event => {
        if (event.defaultPrevented || event.target instanceof HTMLInputElement) return;
        if (event.key.toLowerCase() === "f") {
          event.preventDefault();
          if (!event.repeat) toggleFullscreen();
        } else if (event.key === " " && event.target === video) {
          event.preventDefault();
          if (!event.repeat) togglePlayback();
        } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
          event.preventDefault();
          if (!event.repeat) seek(time() + (event.key === "ArrowLeft" ? -15 : 15));
        }
      }}
    >
      <video
        ref={video}
        src={props.src}
        autoplay
        playsinline
        preload="metadata"
        tabindex="0"
        aria-label="Clip video"
        onClick={togglePlayback}
        onPlay={syncPlaying}
        onPause={syncPlaying}
        onEnded={syncPlaying}
        onTimeUpdate={syncTime}
        onLoadedMetadata={syncDuration}
        onDurationChange={syncDuration}
        onVolumeChange={syncPlayback}
        onRateChange={syncPlayback}
        onError={() => { setFailed(true); syncPlaying(); }}
      />
      <Show when={failed()}><p class={styles.error} role="alert">Clip playback unavailable</p></Show>
      <section
        classList={{ [hudStyles.bottomHud]: true, [hudStyles.hudHidden]: !hud.visible() }}
        aria-label="Clip controls"
        data-hud-region
        {...hud.regionEvents}
      >
        <input
          class={styles.position}
          type="range"
          min="0"
          max={duration()}
          step="any"
          value={Math.min(time(), duration())}
          disabled={failed() || duration() <= 0}
          aria-label="Clip position"
          aria-valuetext={formatDuration(time() * 1_000)}
          onInput={event => seek(event.currentTarget.valueAsNumber)}
          onKeyDown={event => {
            const direction = event.key === "ArrowLeft" || event.key === "ArrowDown" ? -1
              : event.key === "ArrowRight" || event.key === "ArrowUp" ? 1 : 0;
            if (direction === 0) return;
            event.preventDefault();
            seek(time() + direction * (event.ctrlKey ? 5 : 1));
          }}
        />
        <MediaTransport
          mediaLabel="clip"
          mediaAvailable={!failed()}
          playing={playing()}
          playLabel={playing() ? "Pause clip" : "Play clip"}
          playTitle={playing() ? "Pause" : "Play"}
          playback={failed() ? undefined : playback()}
          onTogglePlayback={togglePlayback}
          onRate={rate => { video.playbackRate = rate; syncPlayback(); }}
          onMuted={muted => { video.muted = muted; syncPlayback(); }}
          onVolume={volume => { video.volume = volume; syncPlayback(); }}
          currentMs={time() * 1_000}
          durationMs={duration() * 1_000}
          fullscreen={fullscreen()}
          onFullscreenToggle={toggleFullscreen}
        />
      </section>
    </div>
  );
}
