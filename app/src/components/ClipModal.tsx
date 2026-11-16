import { Show, onCleanup, onMount } from "solid-js";
import { formatDate, formatDuration } from "../format";
import type { ClipSummary } from "../types";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";
import ClipVideoPlayer from "./ClipVideoPlayer";

type Props = {
  clip: ClipSummary;
  onClose: () => void;
};

function ClipModal(props: Props) {
  let closeButton!: HTMLButtonElement;

  onMount(() => {
    closeButton.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented && !event.repeat) props.onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  return (
    <div class={styles.clipModalBackdrop} role="presentation" onMouseDown={props.onClose}>
      <section
        class={styles.clipModal}
        role="dialog"
        aria-modal="true"
        aria-labelledby="clip-modal-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <p>LOCAL CLIP</p>
            <h2 id="clip-modal-title">{props.clip.source_champion ?? "Exported moment"}</h2>
            <span>
              {props.clip.source_date ? formatDate(props.clip.source_date) : "Source game unavailable"}
              {` / ${props.clip.duration_ms === null ? "Duration unavailable" : formatDuration(props.clip.duration_ms)}`}
            </span>
          </div>
          <button class={controlStyles.control} ref={closeButton} type="button" onClick={props.onClose} aria-label="Close clip player">
            Close
          </button>
        </header>
        <Show when={props.clip.video_url} keyed fallback={
          <div class={styles.clipPreviewPlaceholder}>
            <strong>CLIP PREVIEW</strong>
            <span>Open the Tauri app to attach a local clip.</span>
          </div>
        }>
          {src => <ClipVideoPlayer src={src} />}
        </Show>
      </section>
    </div>
  );
}

export default ClipModal;
