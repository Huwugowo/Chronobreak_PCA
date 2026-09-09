import { For, Show } from "solid-js";
import { PLAYBACK_RATES, type PlaybackRate, type PlaybackSnapshot } from "../playbackController";
import styles from "./PlaybackControls.module.css";

export type PlaybackControlsProps = {
  state: Pick<PlaybackSnapshot, "rate" | "muted" | "volume" | "audio" | "desiredPlaying" | "readiness"> | undefined;
  onRate: (rate: PlaybackRate) => void;
  onMuted: (muted: boolean) => void;
  onVolume: (volume: number) => void;
  dark?: boolean;
};

export default function PlaybackControls(props: PlaybackControlsProps) {
  const rate = () => props.state?.rate;
  const unavailable = () => !props.state || props.state.readiness === "closed" || props.state.readiness === "disposed";
  const rateDescription = () => {
    const value = rate();
    if (!value) return "Speed unavailable";
    if (value.limitation) {
      const active = props.state?.desiredPlaying && props.state.readiness === "ready";
      return `${value.selected}x unavailable; ${active ? "playing at" : "fallback"} ${value.effective}x`;
    }
    if (value.outcome === "unknown") return "Speed unverified";
    if (value.outcome === "suspended") return "Speed check paused";
    return value.outcome === "verified" ? `${value.observed!.toFixed(2)}x observed` : "Checking speed";
  };
  return (
    <div classList={{ [styles.controls]: true, [styles.dark]: props.dark }} data-playback-controls="true"
      onKeyDown={(event) => {
        // Preserve native select/range/button editing without triggering replay shortcuts.
        if ([" ", "Enter", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"].includes(event.key)) event.stopPropagation();
      }}>
      <label class={styles.speed}>Speed
        <select aria-label="Replay speed" value={rate()?.selected ?? 1} disabled={unavailable()}
          onChange={(event) => props.onRate(Number(event.currentTarget.value) as PlaybackRate)}>
          <For each={PLAYBACK_RATES}>{(value) => <option value={value}>{value}x</option>}</For>
        </select>
      </label>
      <button type="button" aria-label={props.state?.muted ? "Unmute replay" : "Mute replay"}
        aria-pressed={props.state?.muted ?? false} disabled={unavailable()}
        onClick={() => props.onMuted(!props.state?.muted)}>{props.state?.muted ? "Unmute" : "Mute"}</button>
      <label class={styles.volume}>Volume
        <input type="range" min="0" max="100" step="1" aria-label="Replay volume"
          aria-valuetext={`${Math.round((props.state?.volume ?? 1) * 100)}%`}
          value={Math.round((props.state?.volume ?? 1) * 100)} disabled={unavailable()}
          onInput={(event) => props.onVolume(Number(event.currentTarget.value) / 100)} />
        <span>{Math.round((props.state?.volume ?? 1) * 100)}%</span>
      </label>
      <span classList={{ [styles.status]: true, [styles.limited]: Boolean(rate()?.limitation) }}
        role="status" title={rate()?.limitation ?? undefined}>{rateDescription()}</span>
      <Show when={props.state?.audio === "runtime-limited"}>
        <span class={styles.limited} role="status">Sound controls unavailable</span>
      </Show>
    </div>
  );
}
