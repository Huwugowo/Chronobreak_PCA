import {
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  chooseOutputFolder,
  cleanUpNow,
  ensureHevcCapability,
  loadClips,
  loadDdragonStatus,
  loadGames,
  loadSettings,
  loadStorageUsage,
  openClipsFolder,
  openOutputFolder,
  persistSettings,
  removeClip,
  removeGame,
  resolveItemName,
  setGameSaved,
} from "./api";
import AppHeader from "./components/AppHeader";
import ClipExporterScreen from "./components/ClipExporterScreen";
import ClipModal from "./components/ClipModal";
import ConfirmDialog from "./components/ConfirmDialog";
import LibraryScreen from "./components/LibraryScreen";
import SettingsScreen from "./components/SettingsScreen";
import StorageIndicator from "./components/StorageIndicator";
import ViewerScreen from "./components/ViewerScreen";
import type {
  ClipSummary,
  DdragonStatus,
  GameSummary,
  LibraryTab,
  NavigationState,
  ReturnNavigationState,
} from "./types";
import styles from "./App.module.css";

type DeleteTarget =
  | { kind: "game"; game: GameSummary }
  | { kind: "clip"; clip: ClipSummary };

const emptyDdragon: DdragonStatus = {
  state: "loading",
  version: null,
  asset_base_url: null,
  item_count: 0,
  champion_count: 0,
  cache_directory: "",
  error: null,
};

const initialTab = (): LibraryTab => {
  try {
    return localStorage.getItem("league-replay-library-tab") === "clips" ? "clips" : "games";
  } catch {
    return "games";
  }
};

function App() {
  const [navigation, setNavigation] = createSignal<NavigationState>({
    screen: "library",
    tab: initialTab(),
  });
  const [games, { refetch: refetchGames }] = createResource(loadGames);
  const [clips, { refetch: refetchClips }] = createResource(loadClips);
  const [usage, { refetch: refetchUsage }] = createResource(loadStorageUsage);
  const [settings, { refetch: refetchSettings }] = createResource(loadSettings);
  const [ddragon, { refetch: refetchDdragon }] = createResource(loadDdragonStatus);
  const [busyId, setBusyId] = createSignal<string | null>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<DeleteTarget | null>(null);
  const [activeClip, setActiveClip] = createSignal<ClipSummary | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  let noticeTimer: number | undefined;

  const showNotice = (message: string) => {
    setNotice(message);
    if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
    noticeTimer = window.setTimeout(() => setNotice(null), 3_200);
  };

  const showError = (error: unknown) => {
    showNotice(error instanceof Error ? error.message : String(error));
  };

  const reloadLibrary = async () => {
    await Promise.all([refetchGames(), refetchClips(), refetchUsage()]);
  };

  const openTab = (tab: LibraryTab) => setNavigation({ screen: "library", tab });

  const openSettings = () => {
    const current = navigation();
    if (current.screen === "settings") return;
    setNavigation({ screen: "settings", returnTo: current });
  };

  const closeSettings = () => {
    const current = navigation();
    setNavigation(current.screen === "settings" ? current.returnTo : { screen: "library", tab: "games" });
  };

  const toggleSaved = async (game: GameSummary) => {
    setBusyId(game.timestamp);
    try {
      await setGameSaved(game.timestamp, !game.saved);
      await refetchGames();
      showNotice(game.saved ? "Recording removed from saved games." : "Recording saved.");
    } catch (error) {
      showError(error);
    } finally {
      setBusyId(null);
    }
  };

  const confirmDelete = async () => {
    const target = deleteTarget();
    if (!target) return;
    const id = target.kind === "game" ? target.game.timestamp : target.clip.filename;
    setBusyId(id);
    try {
      if (target.kind === "game") await removeGame(target.game.timestamp);
      else await removeClip(target.clip.filename);
      if (activeClip()?.filename === id) setActiveClip(null);
      setDeleteTarget(null);
      await reloadLibrary();
      showNotice(target.kind === "game" ? "Recording deleted." : "Clip deleted.");
    } catch (error) {
      showError(error);
    } finally {
      setBusyId(null);
    }
  };

  const saveSettings = async (outputPath: string, retentionDays: number): Promise<boolean> => {
    setBusyId("settings");
    try {
      await persistSettings({ output_path: outputPath, auto_delete_days: retentionDays });
      await Promise.all([refetchSettings(), reloadLibrary()]);
      showNotice("Settings saved.");
      return true;
    } catch (error) {
      showError(error);
      return false;
    } finally {
      setBusyId(null);
    }
  };

  const chooseFolder = async () => {
    const current = settings();
    if (!current) return;
    try {
      const selected = await chooseOutputFolder(current.output_path);
      if (selected && selected !== current.output_path) {
        const saved = await saveSettings(selected, current.auto_delete_days);
        const currentNavigation = navigation();
        if (
          saved &&
          currentNavigation.screen === "settings" &&
          currentNavigation.returnTo.screen === "viewer"
        ) {
          setNavigation({
            screen: "settings",
            returnTo: { screen: "library", tab: "games" },
          });
        }
      }
    } catch (error) {
      showError(error);
    }
  };

  const cleanStorage = async () => {
    setBusyId("settings");
    try {
      const result = await cleanUpNow();
      await reloadLibrary();
      showNotice(
        result.deleted_count === 0
          ? "No recordings qualified for cleanup."
          : `${result.deleted_count} recording${result.deleted_count === 1 ? "" : "s"} deleted.`,
      );
    } catch (error) {
      showError(error);
    } finally {
      setBusyId(null);
    }
  };

  createEffect(() => {
    const current = navigation();
    if (current.screen !== "library") return;
    try {
      localStorage.setItem("league-replay-library-tab", current.tab);
    } catch {
      // A blocked persistence store does not affect library navigation.
    }
  });

  onMount(() => {
    void ensureHevcCapability()
      .then(() => refetchSettings())
      .catch(showError);

    const pollAssets = window.setInterval(async () => {
      const status = await refetchDdragon();
      if (status?.state !== "loading") {
        window.clearInterval(pollAssets);
        if (status?.state === "ready") {
          const example = await resolveItemName("1001").catch(() => null);
          console.info(`[League Replay] Data Dragon item 1001: ${example ?? "unresolved"}`);
        }
      }
    }, 700);

    onCleanup(() => window.clearInterval(pollAssets));
  });

  onCleanup(() => {
    if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  });

  const loading = createMemo(
    () =>
      (games() === undefined && games.loading) ||
      (clips() === undefined && clips.loading) ||
      (usage() === undefined && usage.loading) ||
      (settings() === undefined && settings.loading),
  );
  const loadError = createMemo(() => games.error ?? clips.error ?? usage.error ?? settings.error);
  const returnState = (state: NavigationState): ReturnNavigationState =>
    state.screen === "settings" ? state.returnTo : state;

  return (
    <div class={styles.appShell}>
      <AppHeader
        navigation={navigation()}
        ddragon={ddragon() ?? emptyDdragon}
        onHome={() => openTab("games")}
        onTab={openTab}
        onSettings={openSettings}
      />

      <main class={styles.mainContent}>
        <Switch>
          <Match when={loading()}>
            <section class={styles.statePanel} aria-live="polite">
              <span class={styles.loader} aria-hidden="true" />
              <p>Scanning your local replay archive...</p>
            </section>
          </Match>
          <Match when={loadError()}>
            <section class={styles.statePanel} role="alert">
              <p class={styles.errorLabel}>LIBRARY UNAVAILABLE</p>
              <h1>League Replay could not read the archive.</h1>
              <p>{String(loadError())}</p>
              <button type="button" onClick={() => void reloadLibrary()}>
                Try again
              </button>
            </section>
          </Match>
          <Match when={navigation().screen === "library"}>
            <LibraryScreen
              tab={(navigation() as Extract<NavigationState, { screen: "library" }>).tab}
              games={games() ?? []}
              clips={clips() ?? []}
              ddragon={ddragon() ?? emptyDdragon}
              busyId={busyId()}
              onOpenGame={(gameTimestamp) => setNavigation({ screen: "viewer", gameTimestamp })}
              onToggleSaved={(game) => void toggleSaved(game)}
              onDeleteGame={(game) => setDeleteTarget({ kind: "game", game })}
              onOpenClip={setActiveClip}
              onDeleteClip={(clip) => setDeleteTarget({ kind: "clip", clip })}
              onOpenClipsFolder={() => void openClipsFolder().catch(showError)}
            />
          </Match>
          <Match when={navigation().screen === "viewer"}>
            <ViewerScreen
              gameTimestamp={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "viewer" }>).gameTimestamp
              }
              onBack={() => openTab("games")}
              initialClipDraft={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "viewer" }>).clipDraft
              }
              onExportClip={(draft) => setNavigation({ screen: "clip-export", draft })}
            />
          </Match>
          <Match when={navigation().screen === "clip-export"}>
            <ClipExporterScreen
              draft={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "clip-export" }>).draft
              }
              outputPath={settings()?.output_path ?? "~/LeagueReplays"}
              onBack={(draft) =>
                setNavigation({
                  screen: "viewer",
                  gameTimestamp: draft.gameTimestamp,
                  clipDraft: draft,
                })
              }
              onExported={async () => {
                await Promise.all([refetchClips(), refetchUsage()]);
                showNotice("Clip exported.");
              }}
              onOpenClips={() => openTab("clips")}
              onOpenFolder={() => void openClipsFolder().catch(showError)}
            />
          </Match>
          <Match when={navigation().screen === "settings" && settings() && usage()}>
            <SettingsScreen
              settings={settings()!}
              usage={usage()!}
              ddragon={ddragon() ?? emptyDdragon}
              saving={busyId() === "settings"}
              onBack={closeSettings}
              onChooseFolder={() => void chooseFolder()}
              onOpenFolder={() => void openOutputFolder().catch(showError)}
              onRetention={(days) => void saveSettings(settings()!.output_path, days)}
              onCleanUp={() => void cleanStorage()}
            />
          </Match>
        </Switch>
      </main>

      <Show when={navigation().screen === "library" || navigation().screen === "settings"}>
        <Show when={usage()}>
          {(loaded) => <StorageIndicator usage={loaded()} onManage={openSettings} />}
        </Show>
      </Show>
      <Show when={activeClip()}>{(clip) => <ClipModal clip={clip()} onClose={() => setActiveClip(null)} />}</Show>
      <Show when={deleteTarget()}>
        {(target) => (
          <ConfirmDialog
            title={target().kind === "game" ? "Delete this recording?" : "Delete this clip?"}
            message={
              target().kind === "game"
                ? "The video and its game data will be removed from this computer."
                : "The clip video and its thumbnail will be removed from this computer."
            }
            confirmLabel={target().kind === "game" ? "Delete recording" : "Delete clip"}
            busy={busyId() !== null}
            onCancel={() => setDeleteTarget(null)}
            onConfirm={() => void confirmDelete()}
          />
        )}
      </Show>
      <Show when={notice()}>{(message) => <div class={styles.notice} role="status">{message()}</div>}</Show>
    </div>
  );
}

export default App;
