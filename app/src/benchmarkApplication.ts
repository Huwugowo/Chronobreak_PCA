import { REPLAY_TICKS_PER_SECOND, parseReplayTick, projectReplayIntervalToClipRange } from "./replayTime";
import { GAMES_LIBRARY_USABLE_CONTRACT, OPENING_CONTRACT, isOpeningScenario,
  type BenchmarkFixture, type BenchmarkScenario, type ReplayBenchmarkObserver } from "./benchmark";
import { runDeliveryRouteProbe } from "./deliveryRouteProbe";
import { runBenchmarkIndexSearch, runBenchmarkSearchSequence, waitForBenchmarkLibraryReturn,
  waitForBenchmarkMemoryWindow, waitForBenchmarkSearchReady } from "./benchmarkSearch";
import type { HistoryController, HistoryState } from "./historyController";
import type { LibraryOrigin, LibraryState } from "./libraryController";
import type { ClipExportPreset, ClipExportRequest, ClipExportProgress, ClipExportResult,
  HevcProbeStatus, NavigationState, ReplayDescriptor } from "./types";

export type BenchmarkApplicationContext = Readonly<{ origin: LibraryOrigin | null; navigation: NavigationState }>;
export type BenchmarkApplicationState = {
  library: LibraryState; history: HistoryState; origin: LibraryOrigin | null;
  navigation: NavigationState; capability: HevcProbeStatus | null; loading: boolean; error: unknown;
};

/** Driving capabilities of the actual application; controller and player ownership stay in App. */
export type BenchmarkApplicationHost = {
  readState(): BenchmarkApplicationState;
  captureContext(): BenchmarkApplicationContext;
  isContextCurrent(context: BenchmarkApplicationContext): boolean;
  history: Pick<HistoryController, "value" | "subscribe">;
  openViewer(gameTimestamp: string, context: BenchmarkApplicationContext): boolean;
  // Historical benchmark return: publish Games only; normal mount effects still run.
  publishBenchmarkGamesReturn(context: BenchmarkApplicationContext): boolean;
  readReplayDescriptor(origin: LibraryOrigin, gameTimestamp: string): Promise<ReplayDescriptor>;
  drainDurations(ids: string[], context: BenchmarkApplicationContext): Promise<boolean>;
  exportClip(request: ClipExportRequest, progress: (value: ClipExportProgress) => void, token: string): Promise<ClipExportResult>;
  invalidateRoot(root: string): void;
};

export type BenchmarkApplication = ReturnType<typeof createBenchmarkApplication>;

export function createBenchmarkApplication(observer: ReplayBenchmarkObserver, host: BenchmarkApplicationHost) {
  const history = host.history;
  const benchmarkActionsAbort = new AbortController();
  let disposed = false;
  let finished = false;
  let pending: { context: BenchmarkApplicationContext; phase: string } | undefined;
  let expectedViewer: BenchmarkApplicationContext | undefined;
  const libraryState = () => host.readState().library;
  const navigation = () => host.readState().navigation;
  const games = () => libraryState().snapshot?.games;
  const clips = () => libraryState().snapshot?.clips;
  const benchmarkHevcCapability = () => host.readState().capability;
  const active = () => !disposed && !finished;
  const complete: ReplayBenchmarkObserver["complete"] = async (...args) => {
    if (!active()) return;
    finished = true;
    pending = undefined;
    benchmarkActionsAbort.abort();
    await observer.complete(...args);
  };
  const failContext = (phase: string, reason = "Benchmark application context changed") => {
    if (!active()) return;
    benchmarkFailureStarted = true;
    if (phase === "export") {
      // Backend-command failures retain their action-before-scenario contract.
      observer.emit("action_failed", { action_kind: "export", benchmark_scope: "backend_command", reason }, { actionId: "export-1", required: true });
      observer.emit("scenario_failed", { reason }, { required: true });
      void complete("failed", reason);
    } else {
      observer.emit("scenario_failed", { phase, reason }, { required: true });
      if (reason === "Opening Games view changed before paint") void complete("failed", reason);
      else void complete("failed", reason, { phase });
    }
  };
  const check = (context: BenchmarkApplicationContext, phase: string, reason?: string) => {
    if (!active()) return false;
    if (host.isContextCurrent(context)) return true;
    failContext(phase, reason);
    return false;
  };
  const watch = (context: BenchmarkApplicationContext, phase: string) => { pending = { context, phase }; };
  const openViewer = (timestamp: string, context: BenchmarkApplicationContext) => {
    if (!check(context, "replay_open")) return;
    pending = undefined;
    if (host.openViewer(timestamp, context)) expectedViewer = host.captureContext();
  };
  const returnToGames = (context: BenchmarkApplicationContext) => {
    if (!check(context, "viewer_cycle")) return null;
    pending = undefined;
    if (!host.publishBenchmarkGamesReturn(context)) return null;
    return host.captureContext();
  };
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

  const runBenchmarkExport = async (
    scenario: BenchmarkScenario,
    fixture: BenchmarkFixture,
  ) => {
    const context = host.captureContext();
    watch(context, "export");
    const actionId = "export-1";
    const exportOrigin = host.readState().origin;
    const exportToken = exportOrigin?.token ?? "";
    let backendAttempted = false;
    try {
      if (!exportOrigin) throw new Error("export fixture requires a current library snapshot");
      const game = games()?.find((candidate) => candidate.timestamp === fixture.game_timestamp);
      if (!game) throw new Error("export fixture is absent from the benchmark library");
      const probe = await host.readReplayDescriptor(exportOrigin, fixture.game_timestamp);
      if (!check(context, "export")) return;
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
      const result = await host.exportClip(
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
        (progress) => {
          if (!check(context, "export")) return;
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
          );
        },
        exportToken,
      );
      if (!check(context, "export")) return;
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
      await complete("complete", null, { output_count: result.outputs.length });
    } catch (error) {
      if (!check(context, "export")) return;
      const reason = error instanceof Error ? error.message : String(error);
      observer.emit(
        "action_failed",
        { action_kind: "export", benchmark_scope: "backend_command", reason },
        { actionId, required: true },
      );
      observer.emit("scenario_failed", { reason }, { required: true });
      await complete("failed", reason);
    } finally {
      // Filesystem reconciliation outlives publication eligibility and session ownership.
      if (backendAttempted && exportOrigin) host.invalidateRoot(exportOrigin.root);
    }
  };

  const beginBenchmarkScenario = async (
    scenario: BenchmarkScenario,
  ) => {
    if (!active() || benchmarkNavigationStarted || benchmarkFailureStarted) return;
    const context = host.captureContext();
    benchmarkNavigationStarted = true;
    observer.emit("scenario_started", { kind: scenario.kind }, { required: true });
    if (scenario.kind === "library_search" && scenario.id === "cold-index-search") {
      watch(context, "indexing_search");
      try {
        if (scenario.iterations !== 1) throw new Error("indexing diagnostic requires one search iteration");
        const fixture = observer.fixtureForScenario();
        const term = games()?.find(game => game.timestamp === fixture?.game_timestamp)?.champion;
        if (!term || term.length > 32) throw new Error("indexing search fixture unavailable");
        const result = await runBenchmarkIndexSearch(observer, history, term, games()?.length ?? 0, benchmarkActionsAbort.signal);
        if (!check(context, "indexing_search")) return;
        observer.emit("scenario_completed", { kind: scenario.kind, ...result }, { required: true });
        await complete("complete", null, result);
      } catch (error) {
        if (benchmarkActionsAbort.signal.aborted) return;
        const reason = error instanceof Error ? error.message : String(error);
        observer.emit("scenario_failed", { phase: "indexing_search", reason }, { required: true });
        await complete("failed", reason);
      }
      return;
    }
    if (scenario.kind === "library_search" || scenario.kind === "whole_app") {
      watch(context, "library_search");
      const signal = benchmarkActionsAbort.signal;
      try {
        observer.emit("search_preconditioning_started", {}, { required: true });
        await waitForBenchmarkSearchReady(history, signal);
        if (!check(context, "library_search")) return;
        const fixture = observer.fixtureForScenario();
        const selected = games()?.find(game => game.timestamp === fixture?.game_timestamp);
        const term = selected?.champion;
        const input = document.querySelector<HTMLInputElement>('input[aria-label="Search matches"]');
        if (!input || !term || term.length > 32) throw new Error("search workload fixture/input unavailable");
        const requested = scenario.iterations ?? 8;
        if (typeof requested !== "number" || !Number.isSafeInteger(requested) || requested < 1 || requested > 100) {
          throw new Error("search iterations must be between 1 and 100");
        }
        observer.emit("search_preconditioning_ready", { catalog_count: games()?.length, iterations: requested }, { required: true });
        if (scenario.kind === "whole_app" && (requested !== 8 || games()?.length !== 2)) {
          throw new Error("whole-app workload requires eight search iterations and the settled two-game library");
        }
        for (let iteration = 0; iteration < requested; iteration++) {
          await runBenchmarkSearchSequence(observer, input, Array.from(term, (_, index) => term.slice(0, index + 1)), "typing", signal);
          if (!check(context, "library_search")) return;
          await runBenchmarkSearchSequence(observer, input, ["zzzzzz"], "settled", signal);
          if (!check(context, "library_search")) return;
          await runBenchmarkSearchSequence(observer, input, [""], "clear", signal);
          if (!check(context, "library_search")) return;
        }
        observer.emit("search_workload_completed", { search_sequences: requested * 3 }, { required: true });
        if (scenario.kind === "library_search") {
          observer.emit("scenario_completed", { kind: scenario.kind, search_sequences: requested * 3 }, { required: true });
          await complete("complete", null, { search_sequences: requested * 3 });
          return;
        }
      } catch (error) {
        if (signal.aborted) return;
        const reason = error instanceof Error ? error.message : String(error);
        observer.emit("scenario_failed", { phase: "library_search", reason }, { required: true });
        await complete("failed", reason);
        return;
      }
    }
    if (scenario.kind === "app_idle") {
      watch(context, "app_idle");
      const requested =
        scenario.parameters?.duration_ms ??
        (typeof scenario.idle_seconds === "number" ? scenario.idle_seconds * 1_000 : undefined);
      const durationMs =
        typeof requested === "number" && Number.isFinite(requested)
          ? Math.min(600_000, Math.max(1_000, requested))
          : 60_000;
      benchmarkScenarioTimer = window.setTimeout(() => {
        benchmarkScenarioTimer = undefined;
        if (!check(context, "app_idle")) return;
        observer.emit("scenario_completed", { kind: scenario.kind }, { required: true });
        void complete("complete", null, { duration_ms: durationMs });
      }, durationMs);
      return;
    }
    const fixture = observer.fixtureForScenario();
    if (!fixture) {
      observer.emit("scenario_failed", { reason: "fixture_not_found" }, { required: true });
      void complete("failed", "scenario fixture was not declared");
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
      void complete("failed", reason, { phase: "media_capability" });
      return;
    }
    if (scenario.kind === "export") {
      void runBenchmarkExport(scenario, fixture);
      return;
    }
    if (scenario.id === "delivery-route-probe") {
      watch(context, "delivery_route");
      try {
        const routes = await runDeliveryRouteProbe();
        if (!check(context, "delivery_route")) return;
        observer.emit("delivery_route_probe", { routes, origin: location.origin }, { required: true });
      } catch {
        if (!check(context, "delivery_route")) return;
        benchmarkFailureStarted = true;
        observer.emit("scenario_failed", { reason: "delivery_route_probe_failed" }, { required: true });
        await complete("failed", "delivery route probe failed");
        return;
      }
    }
    if (scenario.kind === "lifecycle" && scenario.id === "settled-memory") {
      watch(context, "memory_baseline");
      try {
        if (scenario.iterations !== 6) throw new Error("memory check requires six viewer cycles");
        await waitForBenchmarkSearchReady(history, benchmarkActionsAbort.signal);
        if (!check(context, "memory_baseline")) return;
        await waitForBenchmarkMemoryWindow(observer, history, 0, benchmarkActionsAbort.signal);
        if (!check(context, "memory_baseline")) return;
      } catch (error) {
        if (benchmarkActionsAbort.signal.aborted) return;
        const reason = error instanceof Error ? error.message : String(error);
        observer.emit("scenario_failed", { phase: "memory_baseline", reason }, { required: true });
        await complete("failed", reason);
        return;
      }
    }
    if (isOpeningScenario(scenario)) observer.beginOpeningCycle(fixture, 0);
    observer.emit(
      "replay_requested",
      { fixture_alias: fixture.alias },
      { required: true },
    );
    openViewer(fixture.game_timestamp, context);
  };

  const update = () => {
    // Read every signal before early returns so Solid retains all live dependencies.
    const state = host.readState();
    if (!active()) return;
    if (pending && !check(pending.context, pending.phase)) return;
    const capability = state.capability;
    if (isOpeningScenario(observer.scenario()) && capability && !benchmarkHevcCapabilityEmitted) {
      benchmarkHevcCapabilityEmitted = true;
      observer.emit("hevc_capability", { tested: capability.tested, supported: capability.supported }, { required: true });
    }
    const error = state.error;
    if (error) {
      if (!benchmarkFailureStarted) {
        benchmarkFailureStarted = true;
        const reason = error instanceof Error ? error.message : String(error);
        observer.emit("scenario_failed", { phase: "library_load", reason }, { required: true });
        void complete("failed", reason, { phase: "library_load" });
      }
      return;
    }
    if (state.loading || libraryState().refreshing || !libraryState().actionable) return;
    if (observer.scenario().kind === "library_search" && observer.scenario().id === "cold-index-search") {
      // Start as soon as the normal scanned library is admitted, before import
      // completion, capability probing or the settled-search precondition.
      void beginBenchmarkScenario(observer.scenario());
      return;
    }
    if (isOpeningScenario(observer.scenario())) {
      if (benchmarkLibraryReadinessStarted) return;
      benchmarkLibraryReadinessStarted = true;
      const context = host.captureContext();
      const origin = context.origin;
      benchmarkUsefulFrame = requestAnimationFrame(() => {
        benchmarkUsefulFrame = undefined;
        if (!check(context, "library_paint", "Opening Games view changed before paint")) return;
        benchmarkUsefulPaintFrame = requestAnimationFrame(() => {
          benchmarkUsefulPaintFrame = undefined;
          if (!check(context, "library_paint", "Opening Games view changed before paint")) return;
          const current = host.readState().origin;
          const view = navigation();
          const snapshot = libraryState().snapshot;
          const fixture = observer.fixtureForScenario();
          const selected = snapshot?.games.find(game => game.timestamp === fixture?.game_timestamp);
          if (!origin || !current || !snapshot ||
              current.root !== origin.root || current.rootEpoch !== origin.rootEpoch ||
              current.request !== origin.request || current.token !== origin.token ||
              current.navigation !== origin.navigation || view.screen !== "library" ||
              view.tab !== "games" || !libraryState().actionable || libraryState().refreshing || !selected?.video_available) {
            benchmarkFailureStarted = true;
            observer.emit("scenario_failed", { phase: "library_paint", reason: "Opening Games view changed before paint" }, { required: true });
            void complete("failed", "Opening Games view changed before paint");
            return;
          }
          observer.emit("opening_library_usable", {
            measurement_contract: OPENING_CONTRACT, gate: "snapshot", after_library_paint: true,
            fixture_id: fixture!.id,
            game_count: snapshot.games.length, clip_count: snapshot.clips.length,
          }, { required: true });
          // Measure the normal Games path. Legacy scenarios retain the duration drain below.
          void beginBenchmarkScenario(observer.scenario());
        });
      });
      return;
    }
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
        void complete("failed", reason, { phase: "hevc_capability" });
      }
      return;
    }
    if (!benchmarkLibraryReadinessStarted) {
      benchmarkLibraryReadinessStarted = true;
      const context = host.captureContext();
      const scenario = observer.scenario();
      const snapshot = libraryState().snapshot;
      const currentNavigation = navigation();
      const gamesViewAtAdmission =
        currentNavigation.screen === "library" && currentNavigation.tab === "games";
      let gamesUsablePayload: Record<string, unknown> | null = null;
      const gamesUsableOrigin =
        snapshot && gamesViewAtAdmission
          ? host.readState().origin
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
        if (!check(context, "library_paint", "Games view changed before its paint milestone")) return;
        benchmarkUsefulPaintFrame = window.requestAnimationFrame(() => {
          benchmarkUsefulPaintFrame = undefined;
          if (!check(context, "library_paint", "Games view changed before its paint milestone")) return;
          const currentOrigin = host.readState().origin;
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
            beginBenchmarkScenario(observer.scenario());
          };
          if (gamesViewStillCurrent && snapshot) {
            // The old library_useful milestone means full clip details are
            // ready. Keep that meaning in benchmark mode by draining through
            // the bounded duration API after games_library_usable.
            void host.drainDurations(snapshot.clips.map(clip => clip.filename), context).then((drained) => {
              if (!active()) return;
              const afterDrainOrigin = host.readState().origin;
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
                void complete("failed", reason, { phase: "library_details" });
              }
            });
          } else if (!benchmarkFailureStarted) {
            benchmarkFailureStarted = true;
            const reason = "Games view changed before its paint milestone";
            observer.emit("scenario_failed", { phase: "library_paint", reason }, { required: true });
            void complete("failed", reason, { phase: "library_paint" });
          }
        });
      });
      return;
    }
    if (benchmarkLibraryUsefulEmitted) beginBenchmarkScenario(observer.scenario());
  };

  const viewerCycleCompleted = (context: BenchmarkApplicationContext) => {
    if (!active() || !expectedViewer || context.navigation !== expectedViewer.navigation ||
        !host.isContextCurrent(context)) return;
    expectedViewer = undefined;
    const scenario = observer.scenario();
    benchmarkViewerCycles += 1;
    if (scenario.kind === "lifecycle" && scenario.id === "settled-memory") {
      const cycle = benchmarkViewerCycles;
      observer.emit(cycle === 6 ? "viewer_close_requested" : "viewer_reopen_requested",
        { completed_cycles: cycle, expected_cycles: 6 }, { required: true });
      const returned = returnToGames(context);
      if (!returned) return;
      watch(returned, "memory_return");
      benchmarkReopenTimer = window.setTimeout(async () => {
        benchmarkReopenTimer = undefined;
        if (!check(returned, "memory_return")) return;
        try {
          await waitForBenchmarkMemoryWindow(observer, history, cycle, benchmarkActionsAbort.signal);
          if (!check(returned, "memory_return")) return;
          if (cycle === 6) {
            observer.emit("scenario_completed", { kind: scenario.kind, viewer_cycles: cycle }, { required: true });
            await complete("complete", null, { viewer_cycles: cycle });
          } else {
            const fixture = observer.fixtureForScenario();
            if (!fixture) throw new Error("memory cycle fixture unavailable");
            openViewer(fixture.game_timestamp, returned);
          }
        } catch (error) {
          if (benchmarkActionsAbort.signal.aborted) return;
          const reason = error instanceof Error ? error.message : String(error);
          observer.emit("scenario_failed", { phase: "memory_return", reason }, { required: true });
          await complete("failed", reason);
        }
      }, 0);
      return;
    }
    const requestedIterations = scenario.iterations;
    const iterations =
      scenario.kind === "whole_app" ? 1 : typeof requestedIterations === "number" && Number.isSafeInteger(requestedIterations)
        ? Math.min(10_000, Math.max(1, requestedIterations))
        : scenario.kind === "warm_open"
          ? 6
          : 20;
    if (benchmarkViewerCycles >= iterations) {
      const completeCycles = () => {
        if (!active()) return;
        observer.emit(
          "scenario_completed",
          { kind: scenario.kind, viewer_cycles: benchmarkViewerCycles,
            ...(scenario.kind === "whole_app" ? { search_sequences: 24 } : {}) },
          { required: true },
        );
        void complete("complete", null, { viewer_cycles: benchmarkViewerCycles });
      };
      if (scenario.kind === "lifecycle" || scenario.kind === "whole_app") {
        observer.emit(
          "viewer_close_requested",
          { completed_cycles: benchmarkViewerCycles },
          { required: true },
        );
        const returned = returnToGames(context);
        if (!returned) return;
        watch(returned, "library_return");
        benchmarkReopenTimer = window.setTimeout(async () => {
          benchmarkReopenTimer = undefined;
          if (!check(returned, "library_return")) return;
          if (scenario.kind === "whole_app") {
            try {
              const count = await waitForBenchmarkLibraryReturn(history, benchmarkActionsAbort.signal);
              if (!check(returned, "library_return")) return;
              observer.emit("library_return_ready", { catalog_count: count, authority: "two-animation-frame-paint-opportunity" }, { required: true });
            } catch (error) {
              if (benchmarkActionsAbort.signal.aborted) return;
              const reason = error instanceof Error ? error.message : String(error);
              observer.emit("scenario_failed", { phase: "library_return", reason }, { required: true });
              await complete("failed", reason);
              return;
            }
          }
          completeCycles();
        }, 0);
      } else {
        completeCycles();
      }
      return;
    }
    const nextFixture = observer.fixtureForScenario(benchmarkViewerCycles);
    if (!nextFixture) {
      void complete("failed", "viewer cycle fixture is unavailable");
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
      void complete("failed", reason, { phase: "media_capability" });
      return;
    }
    observer.emit(
      "viewer_reopen_requested",
      { completed_cycles: benchmarkViewerCycles, expected_cycles: iterations },
      { required: true },
    );
    if (isOpeningScenario(scenario)) observer.beginOpeningCycle(nextFixture, benchmarkViewerCycles);
    const returned = returnToGames(context);
    if (!returned) return;
    watch(returned, "viewer_reopen");
    benchmarkReopenTimer = window.setTimeout(() => {
      benchmarkReopenTimer = undefined;
      if (!check(returned, "viewer_reopen")) return;
      openViewer(nextFixture.game_timestamp, returned);
    }, 100);
  };
  return { update, viewerCycleCompleted, dispose() {
    if (disposed) return;
    disposed = true;
    expectedViewer = undefined;
    pending = undefined;
    benchmarkActionsAbort.abort();
    if (benchmarkReopenTimer !== undefined) window.clearTimeout(benchmarkReopenTimer);
    if (benchmarkScenarioTimer !== undefined) window.clearTimeout(benchmarkScenarioTimer);
    if (benchmarkUsefulFrame !== undefined) window.cancelAnimationFrame(benchmarkUsefulFrame);
    if (benchmarkUsefulPaintFrame !== undefined) window.cancelAnimationFrame(benchmarkUsefulPaintFrame);
  } };
}
