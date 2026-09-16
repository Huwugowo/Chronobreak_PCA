import { formatBytes } from "../format";
import type { StorageUsage } from "../types";
import { StorageIcon } from "../ui/icons";
import styles from "./Chrome.module.css";

type Props = {
  usage: StorageUsage;
  onManage: () => void;
};

function StorageIndicator(props: Props) {
  return (
    <footer class={styles.storageIndicator}>
      <div class={styles.storageSummary}>
        <StorageIcon size={16} />
        <span>Recordings {formatBytes(props.usage.games_bytes)}</span>
        <i aria-hidden="true" />
        <span>Clips {formatBytes(props.usage.clips_bytes)}</span>
        <i aria-hidden="true" />
        <strong>Total {formatBytes(props.usage.games_bytes + props.usage.clips_bytes)}</strong>
      </div>
      <button type="button" onClick={props.onManage}>
        Manage storage
      </button>
    </footer>
  );
}

export default StorageIndicator;
