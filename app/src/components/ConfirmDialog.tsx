import { onCleanup, onMount } from "solid-js";
import styles from "../App.module.css";

type Props = {
  title: string;
  message: string;
  confirmLabel: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
};

function ConfirmDialog(props: Props) {
  let cancelButton!: HTMLButtonElement;
  onMount(() => {
    cancelButton.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !props.busy) props.onCancel();
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  return (
    <div class={styles.dialogBackdrop} role="presentation" onMouseDown={props.onCancel}>
      <section
        class={styles.confirmDialog}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        aria-describedby="confirm-message"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <p>IRREVERSIBLE ACTION</p>
        <h2 id="confirm-title">{props.title}</h2>
        <span id="confirm-message">{props.message}</span>
        <div>
          <button ref={cancelButton} type="button" onClick={props.onCancel} disabled={props.busy}>
            Cancel
          </button>
          <button type="button" data-danger onClick={props.onConfirm} disabled={props.busy}>
            {props.busy ? "Working…" : props.confirmLabel}
          </button>
        </div>
      </section>
    </div>
  );
}

export default ConfirmDialog;
