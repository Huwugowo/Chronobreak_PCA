import { createResource, createSignal, Show } from "solid-js";
import { loadRecordingPreferences, saveRecordingPreferences } from "../api";
import type { RecordingPreferencesUpdate } from "../types";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

export default function RecordingPreferences() {
  const [saved, { mutate }] = createResource(loadRecordingPreferences);
  const [profile, setProfile] = createSignal<RecordingPreferencesUpdate["profile"] | "">("");
  const [codec, setCodec] = createSignal<RecordingPreferencesUpdate["codec"] | "">("");
  const [busy, setBusy] = createSignal(false);
  const [message, setMessage] = createSignal("");
  const [error, setError] = createSignal("");
  const selectedProfile = () => profile() || saved()?.profile;
  const selectedCodec = () => codec() || saved()?.codec;
  const save = async () => {
    const nextProfile = selectedProfile();
    const nextCodec = selectedCodec();
    if ((nextProfile !== "auto" && nextProfile !== "high") || (nextCodec !== "auto" && nextCodec !== "h264")) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      mutate(await saveRecordingPreferences({ profile: nextProfile, codec: nextCodec }));
      setMessage("Recording preferences saved.");
    } catch (failure) {
      setError(String(failure));
    } finally {
      setBusy(false);
    }
  };
  return <section class={styles.settingsPanel}>
    <header><div><p>QUALITY</p><h2>Recording preferences</h2></div></header>
    <Show when={!saved.error && saved()} fallback={<p role="status">{saved.error ? "Recording preferences could not be loaded." : "Loading recording preferences…"}</p>}>
      {(configured) => <>
        <p class={styles.settingDetail}>Configured profile: {configured().profile}. Configured codec: {configured().codec}.</p>
        <label class={styles.selectField}>
          <span>Recording profile</span>
          <select value={selectedProfile()} disabled={busy()} onChange={(event) => {
            setProfile(event.currentTarget.value === "high" ? "high" : "auto");
            setMessage("");
          }}>
            <Show when={configured().profile !== "auto" && configured().profile !== "high"}>
              <option value={configured().profile} disabled>Unsupported saved profile</option>
            </Show>
            <option value="auto">Automatic</option>
            <option value="high">High (1080p, 60 fps)</option>
          </select>
        </label>
        <label class={styles.selectField}>
          <span>Recording codec</span>
          <select value={selectedCodec()} disabled={busy()} onChange={(event) => {
            setCodec(event.currentTarget.value === "h264" ? "h264" : "auto");
            setMessage("");
          }}>
            <Show when={configured().codec !== "auto" && configured().codec !== "h264"}>
              <option value={configured().codec} disabled>Unsupported saved codec</option>
            </Show>
            <option value="auto">Automatic</option>
            <option value="h264">H.264</option>
          </select>
        </label>
        <p class={styles.settingDetail}>Automatic currently uses High quality and H.264 on Windows.</p>
        <button class={controlStyles.control} type="button"
          disabled={busy() || !["auto", "high"].includes(selectedProfile() ?? "") || !["auto", "h264"].includes(selectedCodec() ?? "")}
          onClick={() => void save()}>Save recording preferences</button>
        <Show when={message()}><p role="status">{message()}</p></Show>
        <Show when={error()}><p role="alert">{error()}</p></Show>
      </>}
    </Show>
  </section>;
}
