import { onCleanup, onMount } from "solid-js";
import { formatDate, formatDuration } from "../format";
import type { ClipSummary } from "../types";
import styles from "./Library.module.css";

type Props = {
  clip: ClipSummary;
  onClose: () => void;
};

function ClipModal(props: Props) {
  let closeButton!: HTMLButtonElement;

  onMount(() => {
    closeButton.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") props.onClose();
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
              {` / ${formatDuration(props.clip.duration_ms)}`}
            </span>
          </div>
          <button ref={closeButton} type="button" onClick={props.onClose} aria-label="Close clip player">
            Close
          </button>
        </header>
        {props.clip.video_url ? (
          <video src={props.clip.video_url} controls autoplay playsinline preload="metadata" />
        ) : (
          <div class={styles.clipPreviewPlaceholder}>
            <strong>CLIP PREVIEW</strong>
            <span>Open the Tauri app to attach a local clip.</span>
          </div>
        )}
      </section>
    </div>
  );
}

export default ClipModal;
