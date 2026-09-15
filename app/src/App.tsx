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
  loadClips,
  loadDdragonStatus,
  loadGames,
  loadPlaybackProbe,
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
import { REPLAY_TICKS_PER_SECOND, parseReplayTick, projectReplayIntervalToClipRange } from "./replayTime";
import {
  initializeReplayBenchmark,
  REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT,
  type BenchmarkFixture,
  type BenchmarkScenario,
  type ReplayBenchmarkObserver,
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
  HevcProbeStatus,
  LibraryTab,
  NavigationState,
  ReturnNavigationState,
  ClipExportPreset,
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
  const [benchmarkObserver] = createResource(initializeReplayBenchmark);
  const [benchmarkHevcCapability, setBenchmarkHevcCapability] =
    createSignal<HevcProbeStatus | null>(null);
  const [busyId, setBusyId] = createSignal<string | null>(null);
  const [deleteTarget, setDeleteTarget] = createSignal<DeleteTarget | null>(null);
  const [activeClip, setActiveClip] = createSignal<ClipSummary | null>(null);
  const [notice, setNotice] = createSignal<string | null>(null);
  let noticeTimer: number | undefined;
  let benchmarkNavigationStarted = false;
  let benchmarkLibraryUsefulEmitted = false;
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

  const loading = createMemo(
    () =>
      (games() === undefined && games.loading) ||
      (clips() === undefined && clips.loading) ||
      (usage() === undefined && usage.loading) ||
      (settings() === undefined && settings.loading),
  );
  const loadError = createMemo(() => games.error ?? clips.error ?? usage.error ?? settings.error);

  const runBenchmarkExport = async (
    observer: ReplayBenchmarkObserver,
    scenario: BenchmarkScenario,
    fixture: BenchmarkFixture,
  ) => {
    const actionId = "export-1";
    try {
      const game = games()?.find((candidate) => candidate.timestamp === fixture.game_timestamp);
      if (!game) throw new Error("export fixture is absent from the benchmark library");
      const probe = await loadPlaybackProbe(fixture.game_timestamp);
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
    setNavigation({ screen: "viewer", gameTimestamp: fixture.game_timestamp });
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
    if (loading()) return;
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
    if (!benchmarkLibraryUsefulEmitted) {
      benchmarkLibraryUsefulEmitted = true;
      benchmarkUsefulFrame = window.requestAnimationFrame(() => {
        benchmarkUsefulFrame = undefined;
        benchmarkUsefulPaintFrame = window.requestAnimationFrame(() => {
          benchmarkUsefulPaintFrame = undefined;
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
        });
      });
      return;
    }
    beginBenchmarkScenario(observer, observer.scenario());
  });

  onMount(() => {
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
        setNavigation({ screen: "viewer", gameTimestamp: nextFixture.game_timestamp });
      }, 100);
    };
    window.addEventListener(REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT, onBenchmarkViewerCycle);

    onCleanup(() => {
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
