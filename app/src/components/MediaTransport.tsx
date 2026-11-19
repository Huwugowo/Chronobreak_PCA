import { Show, type JSX } from "solid-js";
import { formatDuration } from "../format";
import { FullscreenExitIcon, FullscreenIcon, PauseIcon, PlayIcon } from "../ui/icons";
import PlaybackControls, { type PlaybackControlsProps } from "./PlaybackControls";
import styles from "./FullscreenOverlay.module.css";

type Props = {
  mediaLabel?: "clip";
  mediaAvailable: boolean;
  playing: boolean;
  playLabel: string;
  playTitle: string;
  playback: PlaybackControlsProps["state"];
  onTogglePlayback: () => void;
  onRate: PlaybackControlsProps["onRate"];
  onMuted: PlaybackControlsProps["onMuted"];
  onVolume: PlaybackControlsProps["onVolume"];
  currentMs: number;
  durationMs: number;
  fullscreen: boolean;
  onFullscreenToggle: () => void;
  beforeTime?: JSX.Element;
  children?: JSX.Element;
};

export default function MediaTransport(props: Props) {
  const mediaLabel = () => props.mediaLabel ?? "replay";

  return (
    <div classList={{ [styles.fullscreenControls]: true, [styles.clipTransport]: props.mediaLabel === "clip" }}>
      <button
        class={styles.fullscreenPlay}
        type="button"
        disabled={!props.mediaAvailable}
        onClick={props.onTogglePlayback}
        aria-label={props.playLabel}
        title={props.playTitle}
      >
        {props.playing ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
      </button>

      <PlaybackControls
        dark
        mediaLabel={props.mediaLabel}
        state={props.playback}
        onRate={props.onRate}
        onMuted={props.onMuted}
        onVolume={props.onVolume}
      />

      {props.beforeTime}

      <span class={styles.fullscreenTime} data-testid="time-readout">
        <strong class={styles.fullscreenTimeCurrent}>{formatDuration(props.currentMs)}</strong>
        <span class={styles.fullscreenTimeDivider}>/</span>
        <span class={styles.fullscreenTimeDuration}>{formatDuration(props.durationMs)}</span>
      </span>

      {props.children}

      <button
        class={styles.exitFullscreenControl}
        type="button"
        onClick={props.onFullscreenToggle}
        aria-label={`${props.fullscreen ? "Exit" : "Open"} fullscreen ${mediaLabel()}`}
        title={props.fullscreen ? "Exit fullscreen" : "Fullscreen"}
      >
        <Show when={props.fullscreen} fallback={<FullscreenIcon size={18} />}>
          <FullscreenExitIcon size={18} />
        </Show>
      </button>
    </div>
  );
}
