import type { DdragonStatus, LibraryTab, NavigationState } from "../types";
import { ClipsIcon, MatchHistoryIcon, SettingsIcon } from "../ui/icons";
import styles from "./Chrome.module.css";

type Props = {
  navigation: NavigationState;
  ddragon: DdragonStatus;
  onHome: () => void;
  onTab: (tab: LibraryTab) => void;
  onSettings: () => void;
};

function AppHeader(props: Props) {
  const activeTab = () => (props.navigation.screen === "library" ? props.navigation.tab : null);
  const settingsActive = () => props.navigation.screen === "settings";

  return (
    <header class={styles.header}>
      <button class={styles.brandLockup} type="button" onClick={props.onHome} aria-label="Open Match History">
        <strong class={styles.brandWordmark}>CHRONOBREAK</strong>
        <small>LEAGUE REPLAY</small>
      </button>

      <nav class={styles.tabs} aria-label="Library">
        <button
          type="button"
          classList={{ [styles.activeTab]: activeTab() === "games" }}
          aria-current={activeTab() === "games" ? "page" : undefined}
          onClick={() => props.onTab("games")}
        >
          <MatchHistoryIcon size={16} />
          <span>Match History</span>
        </button>
        <button
          type="button"
          classList={{ [styles.activeTab]: activeTab() === "clips" }}
          aria-current={activeTab() === "clips" ? "page" : undefined}
          onClick={() => props.onTab("clips")}
        >
          <ClipsIcon size={16} />
          <span>Clips</span>
        </button>
      </nav>

      <div class={styles.headerActions}>
        <span
          class={styles.assetStatus}
          data-state={props.ddragon.state}
          title={props.ddragon.error ?? "Data Dragon assets"}
        >
          <i aria-hidden="true" />
          {props.ddragon.state === "ready"
            ? `ASSETS ${props.ddragon.version ?? "READY"}`
            : props.ddragon.state.toUpperCase()}
        </span>
        <button
          class={styles.settingsButton}
          classList={{ [styles.settingsActive]: settingsActive() }}
          type="button"
          onClick={props.onSettings}
          aria-label="Open settings"
          aria-current={settingsActive() ? "page" : undefined}
          title="Settings"
        >
          <SettingsIcon size={16} />
          <span>Settings</span>
        </button>
      </div>
    </header>
  );
}

export default AppHeader;
