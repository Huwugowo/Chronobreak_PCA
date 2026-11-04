import { Show } from "solid-js";
import { PLAYBACK_RATES, type PlaybackRate, type PlaybackSnapshot } from "../playbackController";
import { VolumeMuteIcon, VolumeUpIcon } from "../ui/icons";
import styles from "./PlaybackControls.module.css";

export type PlaybackControlsProps = {
  state: (Pick<PlaybackSnapshot, "rate" | "audio" | "desiredPlaying" | "readiness"> & {
    media: Pick<PlaybackSnapshot["media"], "muted" | "volume">;
  }) | undefined;
  onRate: (rate: PlaybackRate) => void;
  onMuted: (muted: boolean) => void;
  onVolume: (volume: number) => void;
  dark?: boolean;
};

export default function PlaybackControls(props: PlaybackControlsProps) {
  const rate = () => props.state?.rate;
  const rateIndex = () => PLAYBACK_RATES.indexOf(rate()?.selected ?? 1);
  const adjacentRate = (direction: -1 | 1) => PLAYBACK_RATES[rateIndex() + direction];
  const stepRate = (direction: -1 | 1) => {
    const next = adjacentRate(direction);
    if (!unavailable() && next !== undefined) props.onRate(next);
  };
  const muted = () => props.state?.media.muted ?? false;
  const volumePercent = () => Math.round((props.state?.media.volume ?? 1) * 100);
  const displayedVolumePercent = () => muted() ? 0 : volumePercent();

  const unavailable = () =>
    !props.state ||
    props.state.readiness === "closed" ||
    props.state.readiness === "disposed";

  const rateDescription = () => {
    const value = rate();

    if (!value) return "Speed unavailable";

    if (value.limitation) {
      const active =
        props.state?.desiredPlaying &&
        props.state.readiness === "ready";

      return `${value.selected}x unavailable; ${
        active ? "playing at" : "fallback"
      } ${value.effective}x`;
    }

    if (value.outcome === "unknown") return "Speed unverified";
    if (value.outcome === "suspended") return "Speed check paused";

    return value.outcome === "verified"
      ? `${value.observed!.toFixed(2)}x observed`
      : "Checking speed";
  };

  return (
    <div
      classList={{
        [styles.controls]: true,
        [styles.dark]: props.dark,
      }}
      data-playback-controls="true"
      onKeyDown={(event) => {
        if (
          [
            " ",
            "Enter",
            "ArrowLeft",
            "ArrowRight",
            "ArrowUp",
            "ArrowDown",
            "Home",
            "End",
            "PageUp",
            "PageDown",
          ].includes(event.key)
        ) {
          event.stopPropagation();
        }
      }}
    >
      <div
        class={styles.volumeGroup}
        style={`--volume-level:${displayedVolumePercent()}%`}
      >
        <button
          class={styles.muteButton}
          type="button"
          aria-label={muted() ? "Unmute replay" : "Mute replay"}
          aria-pressed={muted()}
          title={muted() ? "Unmute" : "Mute"}
          disabled={unavailable()}
          onClick={() => props.onMuted(!muted())}
        >
          {muted() ? <VolumeMuteIcon size={17} /> : <VolumeUpIcon size={17} />}
          <span class={styles.srOnly}>{muted() ? "Unmute" : "Mute"}</span>
        </button>

        <label class={styles.volume}>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            aria-label="Replay volume"
            aria-valuetext={`${displayedVolumePercent()}%`}
            title={`${displayedVolumePercent()}%`}
            value={displayedVolumePercent()}
            disabled={unavailable()}
            onInput={(event) => {
              const nextVolume = Number(event.currentTarget.value) / 100;

              if (muted() && nextVolume > 0) {
                props.onMuted(false);
              }

              props.onVolume(nextVolume);

              // Keep the thumb aligned to the applied reactive value if the
              // controller rejects or clamps a requested change.
              event.currentTarget.value = String(displayedVolumePercent());
            }}
          />
          <span class={styles.srOnly}>{displayedVolumePercent()}%</span>
        </label>
      </div>

      <div class={styles.speed} role="group" aria-label="Replay speed">
        <button
          class={styles.speedButton}
          type="button"
          aria-label="Decrease replay speed"
          title={adjacentRate(-1) === undefined ? "Minimum speed" : `Decrease speed to ${adjacentRate(-1)}x`}
          disabled={unavailable() || adjacentRate(-1) === undefined}
          onClick={() => stepRate(-1)}
        >
          <span aria-hidden="true">−</span>
        </button>
        <button
          class={styles.speedButton}
          type="button"
          aria-label="Increase replay speed"
          title={adjacentRate(1) === undefined ? "Maximum speed" : `Increase speed to ${adjacentRate(1)}x`}
          disabled={unavailable() || adjacentRate(1) === undefined}
          onClick={() => stepRate(1)}
        >
          <span aria-hidden="true">+</span>
        </button>
        <output
          class={styles.speedValue}
          aria-label="Selected replay speed"
          aria-live="polite"
          title={rateDescription()}
        >
          {rate()?.selected ?? 1}x
        </output>
      </div>

      <span
        classList={{
          [styles.status]: true,
          [styles.limited]: Boolean(rate()?.limitation),
        }}
        role="status"
        title={rate()?.limitation ?? undefined}
      >
        {rateDescription()}
      </span>

      <Show when={props.state?.audio === "runtime-limited"}>
        <span
          class={`${styles.runtimeStatus} ${styles.limited}`}
          role="status"
        >
          Sound controls unavailable
        </span>
      </Show>
    </div>
  );
}
