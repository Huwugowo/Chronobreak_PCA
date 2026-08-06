import type { DdragonStatus, LibraryTab, NavigationState } from "../types";
import { isDesktopRuntime } from "../api";
import styles from "../App.module.css";

type Props = {
  navigation: NavigationState;
  ddragon: DdragonStatus;
  onHome: () => void;
  onTab: (tab: LibraryTab) => void;
  onSettings: () => void;
};

function AppHeader(props: Props) {
  const activeTab = () => (props.navigation.screen === "library" ? props.navigation.tab : null);

  return (
    <header class={styles.header}>
      <button class={styles.brandLockup} type="button" onClick={props.onHome} aria-label="Games library">
        <span class={styles.brandMark} aria-hidden="true">
          LR
        </span>
        <span>
          <small>LEAGUE REPLAY</small>
          <strong>Local match archive</strong>
        </span>
      </button>

      <nav class={styles.tabs} aria-label="Library">
        <button
          type="button"
          classList={{ [styles.activeTab]: activeTab() === "games" }}
          onClick={() => props.onTab("games")}
        >
          Games
        </button>
        <button
          type="button"
          classList={{ [styles.activeTab]: activeTab() === "clips" }}
          onClick={() => props.onTab("clips")}
        >
          Clips
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
        <span
          class={styles.runtimeMark}
          title={isDesktopRuntime() ? "Tauri desktop runtime" : "Browser preview"}
        >
          {isDesktopRuntime() ? "DESKTOP" : "PREVIEW"}
        </span>
        <button
          class={styles.settingsButton}
          type="button"
          onClick={props.onSettings}
          aria-label="Open settings"
          title="Settings"
        >
          <span aria-hidden="true">SETTINGS</span>
        </button>
      </div>
    </header>
  );
}

export default AppHeader;
