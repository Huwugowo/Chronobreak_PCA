import { createResource, createSignal, onCleanup, onMount, Show } from "solid-js";
import { loadAutostartStatus, loadRecorderStatus, retryRecorder, setAutostart } from "../api";
import controlStyles from "../ui/controls.module.css";
import styles from "./Library.module.css";

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
  onMount(() => {
    const timer = window.setInterval(() => void refetch(), 5000);
    onCleanup(() => window.clearInterval(timer));
  });
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
