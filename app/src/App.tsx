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
  exportClip,
  loadDdragonStatus,
  refreshLibrary,
  loadPlaybackProbe,
  loadSettings,
  openClipsFolder,
  openOutputFolder,
  persistSettings,
  removeClip,
  removeGame,
  resolveItemName,
  setGameSaved,
} from "./api";
import { REPLAY_TICKS_PER_SECOND, parseReplayTick, projectReplayIntervalToClipRange } from "./replayTime";
import {
  initializeReplayBenchmark,
  REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT,
  type BenchmarkFixture,
  type BenchmarkScenario,
  type ReplayBenchmarkObserver,
  GAMES_LIBRARY_USABLE_CONTRACT,
} from "./benchmark";
import AppHeader from "./components/AppHeader";
import { runDeliveryRouteProbe } from "./deliveryRouteProbe";
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
  ClipExportPreset,
} from "./types";
import { LibraryController, type LibraryOrigin } from "./libraryController";
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
  const [viewerSelection, setViewerSelection] = createSignal<{ game: GameSummary; origin: LibraryOrigin }>();
  const currentViewer = createMemo(() => {
    libraryState();
    const selected = viewerSelection();
    return selected && controller.mayPublish(selected.origin) ? selected : null;
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
  const [benchmarkHevcCapability, setBenchmarkHevcCapability] =
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
  let benchmarkNavigationStarted = false;
  let benchmarkLibraryReadinessStarted = false;
  let benchmarkLibraryUsefulEmitted = false;
  let benchmarkGamesLibraryUsableEmitted = false;
  let benchmarkViewerCycles = 0;
  let benchmarkFailureStarted = false;
  let benchmarkHevcCapabilityEmitted = false;
  let benchmarkReopenTimer: number | undefined;
  let benchmarkScenarioTimer: number | undefined;
  let benchmarkUsefulFrame: number | undefined;
  let benchmarkUsefulPaintFrame: number | undefined;

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

  const loading = createMemo(
    () => (libraryState().snapshot === null && libraryState().refreshing) ||
      (settings() === undefined && settings.loading),
  );
  const loadError = createMemo(() => libraryState().error ?? settings.error);

  const runBenchmarkExport = async (
    observer: ReplayBenchmarkObserver,
    scenario: BenchmarkScenario,
    fixture: BenchmarkFixture,
  ) => {
    const actionId = "export-1";
    const exportOrigin = controller.origin();
    const exportToken = exportOrigin?.token ?? "";
    let backendAttempted = false;
    try {
      if (!exportOrigin) throw new Error("export fixture requires a current library snapshot");
      const game = games()?.find((candidate) => candidate.timestamp === fixture.game_timestamp);
      if (!game) throw new Error("export fixture is absent from the benchmark library");
      const probe = await controller.readReplay(exportOrigin, fixture.game_timestamp,
        token => loadPlaybackProbe(fixture.game_timestamp, token));
      const timeline = probe.media_timeline;
      const rawStart = typeof scenario.clip_start_ms === "number" ? scenario.clip_start_ms : 10_000;
      const rawEnd =
        typeof scenario.clip_end_ms === "number"
          ? scenario.clip_end_ms
          : Math.min(game.duration_ms, rawStart + 20_000);
      const ticksPerMillisecond = REPLAY_TICKS_PER_SECOND / 1_000;
      const duration = timeline.video.replayEnd;
      const minimumDuration = 5_000 * ticksPerMillisecond;
      const start = Math.max(0, Math.min(rawStart * ticksPerMillisecond, duration - minimumDuration));
      const end = Math.max(start + minimumDuration, Math.min(rawEnd * ticksPerMillisecond, duration));
      const clipRange = projectReplayIntervalToClipRange(
        timeline,
        parseReplayTick(String(Math.round(start))),
        parseReplayTick(String(Math.round(end))),
      );
      const allowedPresets = new Set<ClipExportPreset>(["horizontal", "vertical", "discord"]);
      const requestedPresets = Array.isArray(scenario.export_presets)
        ? scenario.export_presets.filter(
            (preset): preset is ClipExportPreset =>
              typeof preset === "string" && allowedPresets.has(preset as ClipExportPreset),
          )
        : ["horizontal" as const];
      const presets = requestedPresets.length > 0 ? requestedPresets : ["horizontal" as const];
      const requestedMusicMode =
        typeof scenario.music_mode === "string"
          ? scenario.music_mode
          : Array.isArray(scenario.music_modes)
            ? scenario.music_modes[0]
            : undefined;
      const requestedGainMode =
        typeof scenario.gain_mode === "string"
          ? scenario.gain_mode
          : Array.isArray(scenario.gain_modes)
            ? scenario.gain_modes[0]
            : undefined;
      const useBuiltInMusic = requestedMusicMode === "built_in";
      const gameAudioVolume = requestedGainMode === "non_unity" ? 0.8 : 1;
      const builtInFilename =
        typeof scenario.built_in_music_filename === "string"
          ? scenario.built_in_music_filename
          : "momentum.mp3";
      observer.emit(
        "export_requested",
        {
          fixture_alias: fixture.alias,
          start_frame: clipRange.startFrame,
          end_frame_exclusive: clipRange.endFrameExclusive,
          presets,
          music_mode: useBuiltInMusic ? "built_in" : "none",
          game_audio_volume: gameAudioVolume,
          benchmark_scope: "backend_command",
          ui_workflow_included: false,
        },
        { actionId, required: true },
      );
      backendAttempted = true;
      const result = await exportClip(
        {
          game_timestamp: fixture.game_timestamp,
          media_id: clipRange.mediaId,
          start_frame: String(clipRange.startFrame),
          end_frame_exclusive: String(clipRange.endFrameExclusive),
          presets,
          vertical_focus: 0.72,
          vertical_position: 0.5,
          music: useBuiltInMusic
            ? { kind: "builtin", filename: builtInFilename }
            : { kind: "none" },
          game_audio_volume: gameAudioVolume,
          music_volume: 1,
        },
        (progress) =>
          observer.emit(
            "export_progress",
            {
              stage: progress.stage,
              percent: progress.percent,
              preset: progress.preset,
              completed_outputs: progress.completed_outputs,
              total_outputs: progress.total_outputs,
            },
            { actionId },
          ),
        exportToken,
      );
      observer.emit(
        "export_completed",
        {
          elapsed_ms: result.elapsed_ms,
          setup_elapsed_ms: result.setup_elapsed_ms,
          source_probe_elapsed_ms: result.source_probe_elapsed_ms,
          source_probe_strategy: result.source_probe_strategy,
          finalize_elapsed_ms: result.finalize_elapsed_ms,
          total_file_size_bytes: result.total_file_size_bytes,
          throughput_bytes_per_second:
            result.elapsed_ms > 0
              ? (result.total_file_size_bytes * 1_000) / result.elapsed_ms
              : null,
          outputs: result.outputs,
          strategy: "full_reencode",
          copy_strategy: "not_implemented",
          hybrid_strategy: "not_implemented",
          benchmark_scope: "backend_command",
          ui_workflow_included: false,
        },
        { actionId, required: true },
      );
      observer.emit("scenario_completed", { kind: scenario.kind }, { required: true });
      await observer.complete("complete", null, { output_count: result.outputs.length });
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      observer.emit(
        "action_failed",
        { action_kind: "export", benchmark_scope: "backend_command", reason },
        { actionId, required: true },
      );
      observer.emit("scenario_failed", { reason }, { required: true });
      await observer.complete("failed", reason);
    }
    if (backendAttempted && exportOrigin) controller.invalidateRoot(exportOrigin.root);
  };

  const beginBenchmarkScenario = async (
    observer: ReplayBenchmarkObserver,
    scenario: BenchmarkScenario,
  ) => {
    if (benchmarkNavigationStarted || benchmarkFailureStarted) return;
    benchmarkNavigationStarted = true;
    observer.emit("scenario_started", { kind: scenario.kind }, { required: true });
    if (scenario.kind === "app_idle") {
      const requested =
        scenario.parameters?.duration_ms ??
        (typeof scenario.idle_seconds === "number" ? scenario.idle_seconds * 1_000 : undefined);
      const durationMs =
        typeof requested === "number" && Number.isFinite(requested)
          ? Math.min(600_000, Math.max(1_000, requested))
          : 60_000;
      benchmarkScenarioTimer = window.setTimeout(() => {
        benchmarkScenarioTimer = undefined;
        observer.emit("scenario_completed", { kind: scenario.kind }, { required: true });
        void observer.complete("complete", null, { duration_ms: durationMs });
      }, durationMs);
      return;
    }
    const fixture = observer.fixtureForScenario();
    if (!fixture) {
      observer.emit("scenario_failed", { reason: "fixture_not_found" }, { required: true });
      void observer.complete("failed", "scenario fixture was not declared");
      return;
    }
    if (
      String(fixture.codec ?? "").toLowerCase() === "hevc" &&
      benchmarkHevcCapability()?.supported !== true
    ) {
      const reason = "HEVC playback is unsupported by the benchmark WebView environment";
      benchmarkFailureStarted = true;
      observer.emit(
        "scenario_failed",
        { phase: "media_capability", reason },
        { required: true },
      );
      void observer.complete("failed", reason, { phase: "media_capability" });
      return;
    }
    if (scenario.kind === "export") {
      void runBenchmarkExport(observer, scenario, fixture);
      return;
    }
    if (scenario.id === "delivery-route-probe") {
      try {
        const routes = await runDeliveryRouteProbe();
        observer.emit("delivery_route_probe", { routes, origin: location.origin }, { required: true });
      } catch {
        benchmarkFailureStarted = true;
        observer.emit("scenario_failed", { reason: "delivery_route_probe_failed" }, { required: true });
        await observer.complete("failed", "delivery route probe failed");
        return;
      }
    }
    observer.emit(
      "replay_requested",
      { fixture_alias: fixture.alias },
      { required: true },
    );
    openViewer(fixture.game_timestamp);
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

  createEffect(() => {
    const configured = settings();
    if (configured && controller.value.root !== configured.output_path) controller.setRoot(configured.output_path);
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

  createEffect(() => {
    const observer = benchmarkObserver();
    if (!observer) return;
    const error = loadError();
    if (error) {
      if (!benchmarkFailureStarted) {
        benchmarkFailureStarted = true;
        const reason = error instanceof Error ? error.message : String(error);
        observer.emit("scenario_failed", { phase: "library_load", reason }, { required: true });
        void observer.complete("failed", reason, { phase: "library_load" });
      }
      return;
    }
    if (loading() || libraryState().refreshing || !libraryState().actionable) return;
    const hevcCapability = benchmarkHevcCapability();
    if (!hevcCapability) return;
    if (!benchmarkHevcCapabilityEmitted) {
      benchmarkHevcCapabilityEmitted = true;
      observer.emit(
        "hevc_capability",
        { tested: hevcCapability.tested, supported: hevcCapability.supported },
        { required: true },
      );
    }
    if (!hevcCapability.tested) {
      if (!benchmarkFailureStarted) {
        benchmarkFailureStarted = true;
        const reason = "HEVC capability probe did not produce an authoritative result";
        observer.emit(
          "scenario_failed",
          { phase: "hevc_capability", reason },
          { required: true },
        );
        void observer.complete("failed", reason, { phase: "hevc_capability" });
      }
      return;
    }
    if (!benchmarkLibraryReadinessStarted) {
      benchmarkLibraryReadinessStarted = true;
      const scenario = observer.scenario();
      const snapshot = libraryState().snapshot;
      const currentNavigation = navigation();
      const gamesViewAtAdmission =
        currentNavigation.screen === "library" && currentNavigation.tab === "games";
      let gamesUsablePayload: Record<string, unknown> | null = null;
      const gamesUsableOrigin =
        snapshot && gamesViewAtAdmission
          ? controller.origin()
          : null;
      if (!benchmarkGamesLibraryUsableEmitted && snapshot && gamesViewAtAdmission && scenario.id.startsWith("qb010-library-v2-")) {
        const payload = {
          measurement_contract: GAMES_LIBRARY_USABLE_CONTRACT,
          refresh_request_token: String(libraryState().request),
          snapshot_token: snapshot.token,
          view_token: snapshot.token,
          game_count: snapshot.games.length,
          clip_count: snapshot.clips.length,
          storage_game_count: snapshot.usage.game_count,
          storage_clip_count: snapshot.usage.clip_count,
        };
        gamesUsablePayload = payload;
        observer.emit("library_view_admitted", payload, { required: true });
      }
      benchmarkUsefulFrame = window.requestAnimationFrame(() => {
        benchmarkUsefulFrame = undefined;
        benchmarkUsefulPaintFrame = window.requestAnimationFrame(() => {
          benchmarkUsefulPaintFrame = undefined;
          const currentOrigin = controller.origin();
          const currentNavigation = navigation();
          const gamesViewStillCurrent =
            gamesUsableOrigin !== null &&
            currentOrigin !== null &&
            currentOrigin.root === gamesUsableOrigin.root &&
            currentOrigin.rootEpoch === gamesUsableOrigin.rootEpoch &&
            currentOrigin.request === gamesUsableOrigin.request &&
            currentOrigin.token === gamesUsableOrigin.token &&
            currentOrigin.navigation === gamesUsableOrigin.navigation &&
            currentNavigation.screen === "library" &&
            currentNavigation.tab === "games" &&
            libraryState().actionable &&
            !libraryState().refreshing;
          if (gamesUsablePayload && gamesViewStillCurrent) {
            benchmarkGamesLibraryUsableEmitted = true;
            observer.emit("games_library_usable", { ...gamesUsablePayload, after_library_paint: true }, { required: true });
          }
          const emitHistoricalUseful = () => {
            benchmarkLibraryUsefulEmitted = true;
            observer.emit(
              "library_useful",
              {
                game_count: games()?.length ?? 0,
                clip_count: clips()?.length ?? 0,
                after_library_paint: true,
              },
              { required: true },
            );
            beginBenchmarkScenario(observer, observer.scenario());
          };
          if (gamesViewStillCurrent && snapshot) {
            // The old library_useful milestone means full clip details are
            // ready. Keep that meaning in benchmark mode by draining through
            // the bounded duration API after games_library_usable.
            void controller.drainDurations(snapshot.clips.map(clip => clip.filename)).then((drained) => {
              const afterDrainOrigin = controller.origin();
              const afterDrainNavigation = navigation();
              const stillCurrent = afterDrainOrigin !== null && gamesUsableOrigin !== null &&
                afterDrainOrigin.root === gamesUsableOrigin.root &&
                afterDrainOrigin.rootEpoch === gamesUsableOrigin.rootEpoch &&
                afterDrainOrigin.request === gamesUsableOrigin.request &&
                afterDrainOrigin.token === gamesUsableOrigin.token &&
                afterDrainOrigin.navigation === gamesUsableOrigin.navigation &&
                afterDrainNavigation.screen === "library" && afterDrainNavigation.tab === "games" &&
                libraryState().actionable && !libraryState().refreshing;
              if (drained && stillCurrent) emitHistoricalUseful();
              else if (!benchmarkFailureStarted) {
                benchmarkFailureStarted = true;
                const reason = "Historical library details did not complete for the admitted view";
                observer.emit("scenario_failed", { phase: "library_details", reason }, { required: true });
                void observer.complete("failed", reason, { phase: "library_details" });
              }
            });
          } else if (!benchmarkFailureStarted) {
            benchmarkFailureStarted = true;
            const reason = "Games view changed before its paint milestone";
            observer.emit("scenario_failed", { phase: "library_paint", reason }, { required: true });
            void observer.complete("failed", reason, { phase: "library_paint" });
          }
        });
      });
      return;
    }
    if (benchmarkLibraryUsefulEmitted) beginBenchmarkScenario(observer, observer.scenario());
  });

  onMount(() => {
    const unsubscribe = controller.subscribe(setLibraryState);
    setLibraryState(controller.value);
    void ensureHevcCapability()
      .then((status) => {
        setBenchmarkHevcCapability(status);
        return refetchSettings();
      })
      .catch((error) => {
        setBenchmarkHevcCapability({ tested: false, supported: false, probe_url: "" });
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

    const onBenchmarkViewerCycle = () => {
      const observer = benchmarkObserver();
      if (!observer) return;
      const scenario = observer.scenario();
      benchmarkViewerCycles += 1;
      const requestedIterations = scenario.iterations;
      const iterations =
        typeof requestedIterations === "number" && Number.isSafeInteger(requestedIterations)
          ? Math.min(10_000, Math.max(1, requestedIterations))
          : scenario.kind === "warm_open"
            ? 6
            : 20;
      if (benchmarkViewerCycles >= iterations) {
        const completeCycles = () => {
          observer.emit(
            "scenario_completed",
            { kind: scenario.kind, viewer_cycles: benchmarkViewerCycles },
            { required: true },
          );
          void observer.complete("complete", null, { viewer_cycles: benchmarkViewerCycles });
        };
        if (scenario.kind === "lifecycle") {
          observer.emit(
            "viewer_close_requested",
            { completed_cycles: benchmarkViewerCycles },
            { required: true },
          );
          setNavigation({ screen: "library", tab: "games" });
          benchmarkReopenTimer = window.setTimeout(() => {
            benchmarkReopenTimer = undefined;
            completeCycles();
          }, 0);
        } else {
          completeCycles();
        }
        return;
      }
      const nextFixture = observer.fixtureForScenario(benchmarkViewerCycles);
      if (!nextFixture) {
        void observer.complete("failed", "viewer cycle fixture is unavailable");
        return;
      }
      if (
        String(nextFixture.codec ?? "").toLowerCase() === "hevc" &&
        benchmarkHevcCapability()?.supported !== true
      ) {
        const reason = "HEVC playback is unsupported by the benchmark WebView environment";
        observer.emit(
          "scenario_failed",
          { phase: "media_capability", reason },
          { required: true },
        );
        void observer.complete("failed", reason, { phase: "media_capability" });
        return;
      }
      observer.emit(
        "viewer_reopen_requested",
        { completed_cycles: benchmarkViewerCycles, expected_cycles: iterations },
        { required: true },
      );
      setNavigation({ screen: "library", tab: "games" });
      benchmarkReopenTimer = window.setTimeout(() => {
        benchmarkReopenTimer = undefined;
        openViewer(nextFixture.game_timestamp);
      }, 100);
    };
    window.addEventListener(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT, onBenchmarkViewerCycle);

    onCleanup(() => {
      unsubscribe();
      controller.dispose();
      window.clearInterval(pollAssets);
      window.removeEventListener(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT, onBenchmarkViewerCycle);
    });
  });

  onCleanup(() => {
    if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
    if (benchmarkReopenTimer !== undefined) window.clearTimeout(benchmarkReopenTimer);
    if (benchmarkScenarioTimer !== undefined) window.clearTimeout(benchmarkScenarioTimer);
    if (benchmarkUsefulFrame !== undefined) window.cancelAnimationFrame(benchmarkUsefulFrame);
    if (benchmarkUsefulPaintFrame !== undefined) {
      window.cancelAnimationFrame(benchmarkUsefulPaintFrame);
    }
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
            }>{selected => (
            <ViewerScreen
              game={selected.game}
              readReplay={work => controller.readReplay(selected.origin, selected.game.timestamp, work)}
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
            )}</Show>
          </Match>
          <Match when={navigation().screen === "clip-export"}>
            <ClipExporterScreen
              draft={
                (returnState(navigation()) as Extract<ReturnNavigationState, { screen: "clip-export" }>).draft
              }
              outputPath={settings()?.output_path ?? "~/LeagueReplays"}
              snapshotToken={(navigation() as Extract<NavigationState, { screen: "clip-export" }>).snapshotOrigin?.token ?? ""}
              snapshotOrigin={(navigation() as Extract<NavigationState, { screen: "clip-export" }>).snapshotOrigin!}
              readReplay={work => {
                const current = navigation() as Extract<NavigationState, { screen: "clip-export" }>;
                if (!current.snapshotOrigin) return Promise.reject(new Error("Replay selection is unavailable"));
                return controller.readReplay(current.snapshotOrigin, current.draft.gameTimestamp, work);
              }}
              onBack={(draft) => openViewer(draft.gameTimestamp, draft)}
              onExported={async (exportOrigin, completed) => {
                controller.invalidateRoot(exportOrigin.root);
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
