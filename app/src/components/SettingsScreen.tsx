import { For } from "solid-js";
import type { AppSettings, DdragonStatus, StorageUsage } from "../types";
import BackButton from "../ui/BackButton";
import { ExternalLinkIcon } from "../ui/icons";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

type Props = {
  settings: AppSettings;
  usage: StorageUsage;
  ddragon: DdragonStatus;
  saving: boolean;
  onBack: () => void;
  onChooseFolder: () => void;
  onOpenFolder: () => void;
  onRetention: (days: number) => void;
  onCleanUp: () => void;
};

const retentionOptions = [
  { value: 7, label: "7 days" },
  { value: 14, label: "14 days" },
  { value: 30, label: "30 days" },
  { value: 60, label: "60 days" },
  { value: 90, label: "90 days" },
  { value: 0, label: "Never" },
];

function SettingsScreen(props: Props) {
  return (
    <div class={styles.settingsScreen}>
      <BackButton class={styles.backButton} label="Back" onClick={props.onBack} />


      <div class={styles.settingsGrid}>
        <section class={styles.settingsPanel}>
          <header>
            <div>
              <p>OUTPUT</p>
              <h2>Recording folder</h2>
            </div>
          </header>
          <p class={styles.settingDetail}>
            New recordings and the library use this location. Changes apply from the next recording.
          </p>
          <label class={styles.pathField}>
            <span>OUTPUT PATH</span>
            <input value={props.settings.output_path} readOnly aria-label="Recording output path" />
          </label>
          <div class={styles.settingActions}>
            <button
              class={controlStyles.control}
              type="button"
              onClick={props.onChooseFolder}
              disabled={props.saving}
            >
              Choose folder
            </button>
            <button
              class={`${controlStyles.control} ${controlStyles.iconLabel}`}
              type="button"
              onClick={props.onOpenFolder}
            >
              <span>Open current folder</span>
              <ExternalLinkIcon size={14} />
            </button>
          </div>
        </section>

        <section class={styles.settingsPanel}>
          <header>
            <div>
              <p>RETENTION</p>
              <h2>Automatic cleanup</h2>
            </div>
          </header>
          <p class={styles.settingDetail}>
            Unsaved recordings older than this limit are removed on launch. Saved games and clips are
            never touched.
          </p>
          <label class={styles.selectField}>
            <span>AUTO-DELETE AFTER</span>
            <select
              value={props.settings.auto_delete_days}
              onChange={(event) => props.onRetention(Number(event.currentTarget.value))}
              disabled={props.saving}
            >
              <For each={retentionOptions}>
                {(option) => <option value={option.value}>{option.label}</option>}
              </For>
            </select>
          </label>
          <div class={styles.settingActions}>
            <button
              class={controlStyles.control}
              type="button"
              onClick={props.onCleanUp}
              disabled={props.saving}
            >
              Clean up now
            </button>
            <span>{props.saving ? "SAVING…" : "CHANGES SAVE IMMEDIATELY"}</span>
          </div>
        </section>

        <div class={styles.settingsStatus}>
          <span>HEVC playback</span>
          <strong data-state={props.settings.hevc_playback_supported ? "ready" : "limited"}>
            {props.settings.hevc_playback_supported ? "Verified" : "H.264 fallback"}
          </strong>
        </div>
      </div>
    </div>
  );
}

export default SettingsScreen;
