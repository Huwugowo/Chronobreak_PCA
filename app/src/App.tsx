import { Match, Switch, createResource } from "solid-js";
import { isDesktopRuntime, loadPlaybackProbe } from "./api";
import PlaybackProbeView from "./components/PlaybackProbeView";
import styles from "./App.module.css";

function App() {
  const [probe] = createResource(loadPlaybackProbe);

  return (
    <main class={styles.appShell}>
      <header class={styles.header}>
        <div class={styles.brandLockup}>
          <span class={styles.brandMark} aria-hidden="true">
            LR
          </span>
          <div>
            <p class={styles.eyebrow}>LEAGUE REPLAY // PHASE 3</p>
            <h1>Playback performance proof</h1>
          </div>
        </div>
        <div class={styles.runtimeBadge} data-runtime={isDesktopRuntime() ? "desktop" : "browser"}>
          <span aria-hidden="true" />
          {isDesktopRuntime() ? "TAURI WEBVIEW" : "BROWSER PREVIEW"}
        </div>
      </header>

      <Switch>
        <Match when={probe.loading}>
          <section class={styles.statePanel} aria-live="polite">
            <span class={styles.loader} aria-hidden="true" />
            <p>Scanning local replay bundles…</p>
          </section>
        </Match>
        <Match when={probe.error}>
          <section class={styles.statePanel} role="alert">
            <p class={styles.errorLabel}>BOOTSTRAP FAILED</p>
            <h2>The playback proof could not start.</h2>
            <p>{String(probe.error)}</p>
          </section>
        </Match>
        <Match when={probe() === null}>
          <section class={styles.statePanel}>
            <p class={styles.errorLabel}>NO RECORDINGS FOUND</p>
            <h2>Record a game before opening the playback proof.</h2>
            <p>The app scans the configured LeagueReplays/games directory on launch.</p>
          </section>
        </Match>
        <Match when={probe()}>{(loadedProbe) => <PlaybackProbeView probe={loadedProbe()} />}</Match>
      </Switch>
    </main>
  );
}

export default App;
