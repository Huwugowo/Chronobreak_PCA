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
  on,
  untrack,
} from "solid-js";
import {
  chooseOutputFolder,
  cleanUpNow,
  ensureHevcCapability,
  exportClip,
  loadDdragonStatus,
  refreshLibrary,
  loadReplayDescriptor,
  loadSettings,
  openClipsFolder,
  openOutputFolder,
  persistSettings,
  removeClip,
  removeGame,
  resolveItemName,
  setGameSaved,
} from "./api";
import { initializeReplayBenchmark } from "./benchmark";
import { createBenchmarkApplication, type BenchmarkApplication, type BenchmarkApplicationContext, type BenchmarkApplicationHost } from "./benchmarkApplication";
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
  ClipDraft,
  HevcProbeStatus,
  LibraryTab,
  NavigationState,
  ReturnNavigationState,
} from "./types";
import { LibraryController, type LibraryOrigin } from "./libraryController";
import { HistoryController } from "./historyController";
import { observeBenchmarkHistory } from "./benchmarkSearch";
import { indexCatalog, indexRecording, queryHistory, resetLeagueIndex } from "./leagueIndex";
import { reconcileSavedOverlay, isSaveCompletionCurrent, type SavedOverlay } from "./savedOverlay";
import styles from "./App.module.css";

type DeleteTarget =
  | { kind: "game"; game: GameSummary; origin: LibraryOrigin }
  | { kind: "clip"; clip: ClipSummary; origin: LibraryOrigin };

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
  const controller = new LibraryController({
    refresh: refreshLibrary,
    durations: (token, ids, retry) => import("./api").then(api => api.resolveClipDurations(token, ids, retry)),
  });
  const [libraryState, setLibraryState] = createSignal(controller.value);
  const history = new HistoryController({ current: origin => controller.mayPublish(origin), catalog: indexCatalog, recording: indexRecording, history: queryHistory,
    observeQuery: event => observeBenchmarkHistory(event, generation => history.isQueryCurrent(generation) &&
      navigation().screen === "library" && (navigation() as Extract<NavigationState, { screen: "library" }>).tab === "games"),
  });
  const [historyState, setHistoryState] = createSignal(history.value);
  const unsubscribeHistory = history.subscribe(setHistoryState);
  onCleanup(() => { unsubscribeHistory(); history.close(); });
  const [viewerSelection, setViewerSelection] = createSignal<{ game: GameSummary; origin: LibraryOrigin }>();
  const currentViewer = createMemo(() => {
    libraryState();
    const selected = viewerSelection();
    return selected && controller.mayPublish(selected.origin) ? selected : null;
  });
  const exportAdmissionError = createMemo(() => {
    const state = libraryState();
    const current = navigation();
    if (current.screen !== "clip-export") return null;
    if (state.refreshing) return "The library is refreshing. Please wait before exporting again.";
    if (state.error) return `The library could not be refreshed: ${state.error}. Return to games to reload it.`;
    if (!current.snapshotOrigin || !controller.mayPublish(current.snapshotOrigin) ||
        !state.snapshot?.games.some(game => game.timestamp === current.draft.gameTimestamp)) {
      return "The library or source recording changed. Return to games and select the clip again.";
    }
    return null;
  });
  const [saveOverlay, setSaveOverlay] = createSignal<SavedOverlay | null>(null);
  createEffect(() => {
    const state = libraryState();
    setSaveOverlay(current => reconcileSavedOverlay(current, controller.origin(), state));
  });
  const games = createMemo(() => {
    const canonical = libraryState().snapshot?.games;
    const overlay = saveOverlay();
    return overlay ? canonical?.map(game => game.timestamp === overlay.game
      ? { ...game, saved: overlay.saved } : game) : canonical;
  });
  const clips = createMemo(() => libraryState().snapshot?.clips);
  const usage = createMemo(() => libraryState().snapshot?.usage);
  const [settings, { refetch: refetchSettings }] = createResource(loadSettings);
  const [ddragon, { refetch: refetchDdragon }] = createResource(loadDdragonStatus);
  const [benchmarkObserver] = createResource(initializeReplayBenchmark);
  const [hevcCapability, setHevcCapability] =
    createSignal<HevcProbeStatus | null>(null);
  const [busyId, setBusyId] = createSignal<string | null>(null);

  const [deleteTarget, setDeleteTarget] = createSignal<DeleteTarget | null>(null);
  const activeClip = createMemo(() => {
    const id = libraryState().activeClip;
    const clip = clips()?.find(clip => clip.filename === id);
    if (!clip) return null;
    const duration = libraryState().durations[clip.filename];
    return duration?.state === "available" ? { ...clip, duration_ms: duration.duration_ms } : clip;
  });
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

  const reloadLibrary = () => controller.refresh();

  const openViewer = (gameTimestamp: string, clipDraft?: ClipDraft) => {
    controller.navigate(false);
    controller.selectGame(gameTimestamp);
    const origin = controller.origin();
    const game = controller.value.snapshot?.games.find(item => item.timestamp === gameTimestamp);
    if (!origin || !game || !controller.mayPublish(origin)) {
      setViewerSelection(undefined);
      setNavigation({ screen: "library", tab: "games" });
      showError("Refresh the library before opening a recording.");
      return;
    }
    setViewerSelection({ game, origin });
    setNavigation({ screen: "viewer", gameTimestamp, clipDraft });
  };

  const openTab = (tab: LibraryTab) => {
    const returningToLibrary = navigation().screen !== "library";
    controller.navigate(tab === "clips");
    setNavigation({ screen: "library", tab });
    if (returningToLibrary) controller.refresh();
  };

  const openSettings = () => {
    const current = navigation();
    if (current.screen === "settings") return;
    controller.navigate(false);
    setNavigation({ screen: "settings", returnTo: current });
  };

  const closeSettings = () => {
    const current = navigation();
    if (current.screen !== "settings") {
      setNavigation({ screen: "library", tab: "games" });
      return;
    }
    const destination = current.returnTo;
    if (destination.screen === "library") {
      controller.navigate(destination.tab === "clips");
      setNavigation(destination);
      controller.refresh();
    } else if (destination.screen === "viewer") {
      const selected = viewerSelection();
      if (selected && controller.mayStart(selected.origin)) openViewer(destination.gameTimestamp, destination.clipDraft);
      else openTab("games");
    } else {
      if (!destination.snapshotOrigin || !controller.mayStart(destination.snapshotOrigin)) {
        openTab("games");
        return;
      }
      controller.navigate(false);
      setNavigation({ ...destination, snapshotOrigin: controller.origin()! });
    }
  };

  const toggleSaved = async (game: GameSummary, origin: LibraryOrigin | null) => {
    if (!origin || !controller.mayPublish(origin) || saveOverlay() || controller.value.busy) return;
    const overlay: SavedOverlay = { origin, snapshot: controller.value.snapshot!, game: game.timestamp, saved: !game.saved, phase: "pending" };
    setSaveOverlay(overlay);
    const result = await controller.mutate(origin, game.timestamp,
      token => setGameSaved(game.timestamp, overlay.saved, token));
    // Mutation reconciliation remains mandatory even if this UI intent went stale.
    if (!isSaveCompletionCurrent(overlay, controller.origin(), controller.value)) return;
    if (result.admitted) showNotice(overlay.saved ? "Recording saved." : "Recording removed from saved games.");
    else {
      setSaveOverlay(current => current?.origin === origin ? null : current);
      if (result.error) showError(result.error);
    }
  };
  const confirmDelete = async () => {
    const target = deleteTarget();
    if (!target) return;
    const id = target.kind === "game" ? target.game.timestamp : target.clip.filename;
    setBusyId(id);
    try {
      const result = target.kind === "game"
        ? await controller.mutate(target.origin, id, token => removeGame(id, token))
        : await controller.mutate(target.origin, id, token => removeClip(id, token));
      if (!result.admitted) throw new Error(result.error ?? "Delete request became stale.");
      if (activeClip()?.filename === id) controller.selectClip(null);
      setDeleteTarget(null);
      showNotice(target.kind === "game" ? "Recording deleted." : "Clip deleted.");
    } catch (error) {
      showError(error);
    } finally {
      setBusyId(null);
    }
  };

  const saveSettings = async (outputPath: string, retentionDays: number): Promise<boolean> => {
    const origin = controller.origin();
    if (!origin) { showError("Settings require a current library snapshot."); return false; }
    setBusyId("settings");
    try {
      const result = await controller.saveSettings(origin,
        token => persistSettings({ output_path: outputPath, auto_delete_days: retentionDays }, token));
      await refetchSettings();
      if (!result.admitted) throw new Error(result.error ?? "Settings request became stale.");
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
    const origin = controller.origin();
    if (!origin) { showError("Cleanup requires a current library snapshot."); return; }
    setBusyId("settings");
    try {
      const operation = await controller.mutate(origin, "retention", token => cleanUpNow(token));
      if (!operation.admitted) throw new Error(operation.error ?? "Cleanup request became stale.");
      const result = operation.value;
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

  const rebuildLeagueData = async () => {
    const origin = controller.origin();
    if (!origin) { showError("Rebuilding requires a current library snapshot."); return; }
    setBusyId("settings");
    try {
      const operation = await controller.mutate(origin, "league-index", resetLeagueIndex);
      if (!operation.admitted) {
        if (operation.error) showError(operation.error);
        return;
      }
      showNotice("Searchable League data reset. Recordings will be indexed as you browse.");
    } catch (error) {
      showError(error);
    } finally {
      setBusyId(null);
    }
  };

  const loading = createMemo(
    () => (libraryState().snapshot === null && libraryState().refreshing) ||
      (settings() === undefined && settings.loading),
  );
  const loadError = createMemo(() => libraryState().error ?? settings.error);

  createEffect(() => {
    const current = navigation();
    if (current.screen !== "library") return;
    try {
      localStorage.setItem("league-replay-library-tab", current.tab);
    } catch {
      // A blocked persistence store does not affect library navigation.
    }
  });

  createEffect(() => {
    const configured = settings();
    if (configured && controller.value.root !== configured.output_path) controller.setRoot(configured.output_path);
  });

  createEffect(() => {
    const current = navigation();
    const state = libraryState();
    history.setContext(current.screen === "library" && current.tab === "games" && state.actionable && !state.busy ? controller.origin() : null, state.snapshot?.games);
  });

  createEffect(() => {
    const current = navigation();
    if (current.screen === "library" && current.tab === "clips" && libraryState().actionable) {
      const ids = (clips() ?? []).slice(0, 8).map(clip => clip.filename);
      const activeId = libraryState().activeClip;
      if (activeId && !ids.includes(activeId)) ids[ids.length === 8 ? 7 : ids.length] = activeId;
      controller.requestDurations(ids);
    }
  });

  const host: BenchmarkApplicationHost = {
    readState: () => ({ library: libraryState(), history: historyState(), origin: controller.origin(),
      navigation: navigation(), capability: hevcCapability(), loading: loading(), error: loadError() }),
    captureContext: () => ({ origin: controller.origin(), navigation: navigation() }),
    isContextCurrent: context => !!context.origin && controller.mayPublish(context.origin) && navigation() === context.navigation,
    history,
    openViewer: (timestamp, context) => {
      if (!host.isContextCurrent(context)) return false;
      openViewer(timestamp);
      return navigation().screen === "viewer";
    },
    publishBenchmarkGamesReturn: context => {
      if (!host.isContextCurrent(context)) return false;
      setNavigation({ screen: "library", tab: "games" });
      return true;
    },
    readReplayDescriptor: (origin, timestamp) => controller.readReplay(origin, timestamp, token => loadReplayDescriptor(timestamp, token)),
    drainDurations: (ids, context) => host.isContextCurrent(context) ? controller.drainDurations(ids) : Promise.resolve(false),
    exportClip,
    invalidateRoot: root => { void controller.invalidateRoot(root); },
  };
  const [benchmarkApplication, setBenchmarkApplication] = createSignal<BenchmarkApplication>();
  createEffect(on(benchmarkObserver, observer => {
    if (!observer) { setBenchmarkApplication(undefined); return; }
    const application = untrack(() => createBenchmarkApplication(observer, host));
    setBenchmarkApplication(application);
    onCleanup(() => { application.dispose(); setBenchmarkApplication(undefined); });
  }));
  createEffect(() => benchmarkApplication()?.update());

  onMount(() => {
    const unsubscribe = controller.subscribe(setLibraryState);
    setLibraryState(controller.value);
    void ensureHevcCapability()
      .then((status) => {
        setHevcCapability(status);
        return refetchSettings();
      })
      .catch((error) => {
        setHevcCapability({ tested: false, supported: false, probe_url: "" });
        showError(error);
      });

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

    onCleanup(() => {
      unsubscribe();
      controller.dispose();
      window.clearInterval(pollAssets);
    });
  });

  onCleanup(() => {
    if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  });

  const returnState = (state: NavigationState): ReturnNavigationState =>
    state.screen === "settings" ? state.returnTo : state;

  return (
    <div class={styles.appShell} data-screen={navigation().screen}>
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
          <Match when={loadError() && !libraryState().snapshot}>
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
            <Show when={loadError() && libraryState().snapshot}>
              <p class={styles.errorLabel} role="alert">{String(loadError())}</p>
            </Show>
            <LibraryScreen
              tab={(navigation() as Extract<NavigationState, { screen: "library" }>).tab}
              games={games() ?? []}
              history={historyState()}
              onHistoryQuery={(query, actionId) => history.setQuery(query, actionId)}
              onHistoryNext={() => history.nextPage()}
              onHistoryFirst={() => history.firstPage()}
              clips={clips() ?? []}
              actionable={libraryState().actionable}
              durationStates={libraryState().durations}
              onRetryDuration={(filename) => controller.requestDurations([filename], true)}
              snapshotOrigin={controller.origin()}
              ddragon={ddragon() ?? emptyDdragon}
              busyId={libraryState().busy ?? busyId()}
              onOpenGame={(timestamp, origin) => {
                  if (origin && controller.mayPublish(origin)) openViewer(timestamp);
                }}
              onToggleSaved={(game, origin) => void toggleSaved(game, origin)}
              onDeleteGame={(game, origin) => {
                if (origin) { controller.selectDeletion("game", game.timestamp); setDeleteTarget({ kind: "game", game, origin }); }
              }}
              onOpenClip={(clip) => { controller.selectClip(clip.filename); }}
              onDeleteClip={(clip, origin) => {
                if (origin) { controller.selectDeletion("clip", clip.filename); setDeleteTarget({ kind: "clip", clip, origin }); }
              }}
              onOpenClipsFolder={() => void openClipsFolder().catch(showError)}
            />
          </Match>
          <Match when={navigation().screen === "viewer"}>
            <Show when={currentViewer()} keyed fallback={
              <section role="status">The library changed. <button type="button" onClick={() => openTab("games")}>Return to games</button></section>
            }>{selected => {
              const application = untrack(benchmarkApplication);
              const context: BenchmarkApplicationContext = untrack(host.captureContext);
              const onBenchmarkViewerCycleCompleted = application
                ? () => application.viewerCycleCompleted(context) : undefined;
              return <ViewerScreen
              onBenchmarkViewerCycleCompleted={onBenchmarkViewerCycleCompleted}
              game={selected.game}
              readReplay={(work, lane) => controller.readReplay(selected.origin, selected.game.timestamp, work, lane)}
              ddragonAssetBaseUrl={ddragon()?.asset_base_url ?? null}
              gameTimestamp={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "viewer" }>).gameTimestamp
              }
              onBack={() => openTab("games")}
              initialClipDraft={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "viewer" }>).clipDraft
              }
              onExportClip={(draft) => {
                controller.navigate(false);
                const snapshotOrigin = controller.origin();
                if (snapshotOrigin && controller.mayPublish(snapshotOrigin)) setNavigation({ screen: "clip-export", draft, snapshotOrigin });
              }}
            />
            }}</Show>
          </Match>
          <Match when={navigation().screen === "clip-export"}>
            <ClipExporterScreen
              game={games()?.find(game => game.timestamp === (navigation() as Extract<NavigationState, { screen: "clip-export" }>).draft.gameTimestamp)}
              draft={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "clip-export" }>).draft
              }
              outputPath={settings()?.output_path ?? "~/LeagueReplays"}
              snapshotToken={(navigation() as Extract<NavigationState, { screen: "clip-export" }>).snapshotOrigin?.token ?? ""}
              snapshotOrigin={(navigation() as Extract<NavigationState, { screen: "clip-export" }>).snapshotOrigin!}
              admissionError={exportAdmissionError()}
              readReplay={work => {
                const current = navigation() as Extract<NavigationState, { screen: "clip-export" }>;
                if (!current.snapshotOrigin) return Promise.reject(new Error("Replay selection is unavailable"));
                return controller.readReplay(current.snapshotOrigin, current.draft.gameTimestamp, work);
              }}
              onBack={(draft) => openViewer(draft.gameTimestamp, draft)}
              onExported={async (exportOrigin, completed) => {
                const editor = navigation();
                const fresh = await controller.invalidateRoot(exportOrigin.root);
                if (editor.screen !== "clip-export" || navigation() !== editor ||
                    editor.snapshotOrigin !== exportOrigin || !fresh ||
                    fresh.rootEpoch !== exportOrigin.rootEpoch ||
                    fresh.navigation !== exportOrigin.navigation || !controller.mayPublish(fresh) ||
                    !controller.value.snapshot?.games.some(game => game.timestamp === editor.draft.gameTimestamp)) return;
                setNavigation({ ...editor, snapshotOrigin: fresh });
                if (completed) showNotice("Clip exported.");
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
              onRebuildLeagueData={() => void rebuildLeagueData()}
              canRebuildLeagueData={libraryState().actionable && !libraryState().busy}
            />
          </Match>
        </Switch>
      </main>

      <Show when={navigation().screen === "library" || navigation().screen === "settings"}>
        <Show when={usage()}>
          {(loaded) => <StorageIndicator usage={loaded()} onManage={openSettings} />}
        </Show>
      </Show>
      <Show when={activeClip()}>{(clip) => <ClipModal clip={clip()} onClose={() => controller.selectClip(null)} />}</Show>
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
