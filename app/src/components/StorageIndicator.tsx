import { formatBytes } from "../format";
import type { StorageUsage } from "../types";
import styles from "../App.module.css";

type Props = {
  usage: StorageUsage;
  onManage: () => void;
};

function StorageIndicator(props: Props) {
  return (
    <footer class={styles.storageIndicator}>
      <div>
        <span>RECORDINGS {formatBytes(props.usage.games_bytes)}</span>
        <i aria-hidden="true" />
        <span>CLIPS {formatBytes(props.usage.clips_bytes)}</span>
        <i aria-hidden="true" />
        <strong>TOTAL {formatBytes(props.usage.games_bytes + props.usage.clips_bytes)}</strong>
      </div>
      <button type="button" onClick={props.onManage}>
        Manage storage <span aria-hidden="true">→</span>
      </button>
    </footer>
  );
}

export default StorageIndicator;
