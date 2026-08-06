import { For, Show } from "solid-js";
import { formatBytes } from "../format";
import type { AppSettings, DdragonStatus, StorageUsage } from "../types";
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
      <button class={styles.backButton} type="button" onClick={props.onBack}>
        <span aria-hidden="true">←</span> Back
      </button>

      <section class={styles.settingsIntro}>
        <p>APPLICATION SETTINGS</p>
        <h1>Storage, without surprises.</h1>
        <span>Phase 3 exposes only the controls needed to manage local recordings safely.</span>
      </section>

      <div class={styles.settingsGrid}>
        <section class={styles.settingsPanel}>
          <header>
            <span>01</span>
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
            <button type="button" data-primary onClick={props.onChooseFolder} disabled={props.saving}>
              Choose folder
            </button>
            <button type="button" onClick={props.onOpenFolder}>
              Open current folder ↗
            </button>
          </div>
        </section>

        <section class={styles.settingsPanel}>
          <header>
            <span>02</span>
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
            <button type="button" onClick={props.onCleanUp} disabled={props.saving}>
              Clean up now
            </button>
            <span>{props.saving ? "SAVING…" : "CHANGES SAVE IMMEDIATELY"}</span>
          </div>
        </section>

        <section class={`${styles.settingsPanel} ${styles.storagePanel}`}>
          <header>
            <span>03</span>
            <div>
              <p>LOCAL STORAGE</p>
              <h2>Current footprint</h2>
            </div>
          </header>
          <dl class={styles.storageBreakdown}>
            <div>
              <dt>GAMES</dt>
              <dd>{formatBytes(props.usage.games_bytes)}</dd>
              <small>{props.usage.game_count} recordings</small>
            </div>
            <div>
              <dt>CLIPS</dt>
              <dd>{formatBytes(props.usage.clips_bytes)}</dd>
              <small>{props.usage.clip_count} clips</small>
            </div>
            <div>
              <dt>TOTAL</dt>
              <dd>{formatBytes(props.usage.games_bytes + props.usage.clips_bytes)}</dd>
              <small>Stored locally</small>
            </div>
          </dl>
        </section>

        <section class={`${styles.settingsPanel} ${styles.systemPanel}`}>
          <header>
            <span>04</span>
            <div>
              <p>CAPABILITIES</p>
              <h2>Playback system</h2>
            </div>
          </header>
          <dl class={styles.capabilityList}>
            <div>
              <dt>HEVC PLAYBACK</dt>
              <dd data-state={props.settings.hevc_playback_supported ? "ready" : "limited"}>
                {props.settings.hevc_playback_supported ? "VERIFIED" : "H.264 FALLBACK"}
              </dd>
            </div>
            <div>
              <dt>DATA DRAGON</dt>
              <dd data-state={props.ddragon.state === "ready" ? "ready" : "limited"}>
                {props.ddragon.state === "ready"
                  ? `PATCH ${props.ddragon.version}`
                  : props.ddragon.state.toUpperCase()}
              </dd>
            </div>
            <div>
              <dt>STATIC CATALOG</dt>
              <dd>
                <Show when={props.ddragon.state === "ready"} fallback="PENDING">
                  {props.ddragon.champion_count} champions · {props.ddragon.item_count} items
                </Show>
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </div>
  );
}

export default SettingsScreen;
