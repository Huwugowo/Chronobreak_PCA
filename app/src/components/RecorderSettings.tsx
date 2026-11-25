import { createResource, createSignal, onCleanup, onMount, Show } from "solid-js";
import {
  applyRecordingSettings, loadAutostartStatus, loadEffectiveRecordingSettings, loadRecorderStatus,
  retryRecorder, setAutostart,
} from "../api";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

const applyMessages = {
  waiting: "Waiting for the recorder to finish capture and finalization.",
  invalid_configuration: "The recorder could not load the saved settings. Check your preferences and try again.",
  destination_unavailable: "The recorder could not use the configured folder. Check its access and free space, then try again.",
  library_recovery_unavailable: "The recorder could not preserve its library recovery state. The previous settings are still active.",
  idle: "",
};

export default function RecorderSettings() {
  const [error, setError] = createSignal("");
  const [status, { refetch }] = createResource(async () => {
    try { return await loadRecorderStatus(); }
    catch { setError("Recorder status could not be loaded."); return undefined; }
  });
  const [login, { mutate: setLogin, refetch: refetchLogin }] = createResource(async () => {
    try { return await loadAutostartStatus(); }
    catch { setError("Windows startup registration could not be loaded."); return undefined; }
  });
  const [busy, setBusy] = createSignal(false);
  const [effective, { mutate: setEffective, refetch: refetchEffective }] = createResource(async () => {
    try { return await loadEffectiveRecordingSettings(); }
    catch { return null; }
  });
  onMount(() => {
    const timer = window.setInterval(() => {
      void refetch();
      if (!busy()) void refetchEffective();
    }, 5000);
    onCleanup(() => window.clearInterval(timer));
  });
  const apply = async () => {
    setBusy(true); setError("");
    try { setEffective(await applyRecordingSettings()); await refetch(); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const retry = async () => {
    setBusy(true); setError("");
    try { await retryRecorder(); await refetch(); }
    catch (failure) { setError(String(failure)); }
    finally { setBusy(false); }
  };
  const changeLogin = async (enabled: boolean) => {
    setBusy(true); setError("");
    try { setLogin(await setAutostart(enabled)); }
    catch (failure) { setError(String(failure)); await refetchLogin(); }
    finally { setBusy(false); }
  };
  return <section class={styles.settingsPanel}>
    <header><div><p>RECORDER</p><h2>Automatic recording</h2></div></header>
    <p class={styles.settingDetail} role="status">{status()?.message ?? "Checking recorder…"}</p>
    <Show when={effective()?.effective} fallback={<p class={styles.settingDetail}>Current recording settings are unavailable.</p>}>
      {(current) => <>
        <label class={styles.pathField}>
          <span>RECORDER DESTINATION</span>
          <input value={current().output_path} readOnly />
        </label>
        <p class={styles.settingDetail}>Current recording mode: High (1080p, 60 fps), H.264.</p>
      </>}
    </Show>
    <Show when={effective()?.apply_state && applyMessages[effective()!.apply_state]}>
      <p class={styles.settingDetail} role="status">{applyMessages[effective()!.apply_state]}</p>
    </Show>
    <Show when={status()?.recording_capable_here || status()?.can_retry}>
      <button class={controlStyles.control} type="button" disabled={busy() || effective()?.apply_state === "waiting"}
        onClick={() => void apply()}>Apply saved recording settings</button>
    </Show>
    <Show when={status()?.can_retry}>
      <button class={controlStyles.control} type="button" disabled={busy()} onClick={() => void retry()}>Retry recorder</button>
    </Show>
    <label class={styles.settingDetail}>
      <input type="checkbox" aria-label="Start recorder when I sign in"
        checked={login()?.actual_registration === "matching"}
        disabled={busy() || !login() || !["matching", "absent"].includes(login()!.actual_registration)}
        onChange={(event) => void changeLogin(event.currentTarget.checked)} />
      Start recorder when I sign in
    </label>
    <p class={styles.settingDetail}>
      {login()?.disabled_for_test ? "Login startup is disabled in this test session." : login()?.actual_registration === "matching" ? "Registered with Windows. Windows controls when startup runs." : login()?.actual_registration === "absent" ? "Off — not registered with Windows." : "Windows startup registration is unavailable or needs attention."}
    </p>
    <Show when={login()?.actual_registration === "matching" && login()?.desired_autostart === false}>
      <p class={styles.settingDetail}>Windows registration differs from the stored preference. Turn this off to reconcile it.</p>
    </Show>
    <Show when={login()?.error}><p role="alert">{login()?.error}</p></Show>
    <Show when={error() || status.error}><p role="alert">{error() || "Recorder status could not be loaded."}</p></Show>
  </section>;
}
