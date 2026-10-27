import { buildScenarioActions, isOpeningScenario, openingPayload, replayBenchmarkObserver,
  type OpeningCycle, type ReplayBenchmarkObserver } from "./benchmark";
import { REPLAY_TICKS_PER_SECOND, type ReplayTick } from "./replayTime";
import type { PlaybackEvent, PlaybackOutcome, PlaybackRate, PlaybackSnapshot, SeekOutcome } from "./playbackController";
import type { ServerMetrics } from "./types";
import { createBenchmarkSeekEvidence } from "./benchmarkSeekEvidence";
import { runBenchmarkDesktopResize, type BenchmarkResizeSubject } from "./benchmarkResize";
import { benchmarkRenderObservation, waitForBenchmarkRenderedReplay, type BenchmarkRenderSubject } from "./benchmarkRender";
import type { BenchmarkEventMeta, ScenarioAction } from "./benchmark";

type SeekRequest = Readonly<{ reason?: string; playAfter?: boolean; actionId?: string }>;
type ClipEndpoint = "start" | "end";
/** Live capabilities of the actual Viewer. Ownership stays with its mount. */
export type BenchmarkViewerHost = {
  snapshot(): PlaybackSnapshot;
  details(): Readonly<{ ready: boolean; error: unknown }>;
  presentationClock(): "VIDEO FRAME" | "ANIMATION FRAME";
  play(): Promise<PlaybackOutcome>;
  pause(): PlaybackOutcome;
  setRate(rate: PlaybackRate): void;
  requestSeek(target: ReplayTick, request: SeekRequest): Promise<SeekOutcome>;
  prepareEventJump(target: ReplayTick): Readonly<{ target: ReplayTick; eventType?: string }>;
  editClipEndpoint(endpoint: ClipEndpoint, target: ReplayTick): Readonly<{ target: ReplayTick; start: ReplayTick; end: ReplayTick }>;
  finishClipEndpointEdit(): void;
  setClipSelection(enabled: boolean, anchor?: ReplayTick): void;
  setFullscreen(enabled: boolean, required: boolean): void;
  refreshMetrics(): Promise<void>;
  sampleMetrics(): PlaybackSnapshot;
  serverMetrics(): ServerMetrics;
  release(): void;
  cycleCompleted(): void;
  resize: BenchmarkResizeSubject;
  render: BenchmarkRenderSubject;
};
export type BenchmarkViewerContext = Readonly<{
  replayEnd: ReplayTick;
  opening?: OpeningCycle;
  mountToken: object;
  isCurrentMount(token: object): boolean;
}>;
export type BenchmarkViewer = ReturnType<typeof createBenchmarkViewer>;
const BENCHMARK_MEDIA_READY_TIMEOUT_MS = 30_000;
const SEEK_TIMEOUT_MS = 1_500;
const replayTickToMilliseconds = (tick: ReplayTick) => tick * 1_000 / REPLAY_TICKS_PER_SECOND;
const millisecondsToReplayTick = (value: number, end: ReplayTick): ReplayTick =>
  Math.round(Math.max(0, Math.min(value, replayTickToMilliseconds(end))) * REPLAY_TICKS_PER_SECOND / 1_000) as ReplayTick;

export const benchmarkViewerOpening = (observer: ReplayBenchmarkObserver | null, gameTimestamp: string) =>
  observer && isOpeningScenario(observer.scenario()) ? observer.openingCycle(gameTimestamp) : undefined;

/** One experiment owner, created only for a captured active benchmark session. */
export function createBenchmarkViewer(observer: ReplayBenchmarkObserver, host: BenchmarkViewerHost, context: BenchmarkViewerContext) {
  const replayEnd = context.replayEnd;
  const durationMs = replayTickToMilliseconds(replayEnd);
  let disposed = false, benchmarkScenarioStarted = false, benchmarkScenarioFinished = false;
  let pendingFailure: { reason: string } | undefined;
  let benchmarkMediaReadyTimerId: number | undefined;
  let mediaGeneration = host.snapshot().generation;
  let firstPresentedGeneration = -1, canPlayGeneration = -1, benchmarkEndpointSequence = 0;
  const benchmarkAbortController = new AbortController();
  const dispatchObservers = new Map<string, () => void>();
  const benchmarkSeekObservers = new Set<(event: PlaybackEvent) => void>();
  let admitDetails!: () => void;
  const admittedDetails = new Promise<void>(resolve => { admitDetails = resolve; });
  if (host.details().ready) admitDetails();
  const current = () => !disposed && context.isCurrentMount(context.mountToken) && replayBenchmarkObserver() === observer;
  const eligible = () => current() && !benchmarkScenarioFinished && !pendingFailure && !benchmarkAbortController.signal.aborted;
  const assertEligible = () => { if (!eligible()) throw benchmarkAbortError(); };
  const emit = (kind: string, payload: Record<string, unknown> = {}, meta: BenchmarkEventMeta = {}) =>
    eligible() ? observer.emit(kind, payload, meta) : false;
  // Helpers can finish native cleanup after cancellation, but cannot publish it
  // into an outgoing mount/session. No full observer or player is transferred.
  const helperObserver = { emit, now: () => observer.now(), nextActionId: (kind: Parameters<ReplayBenchmarkObserver["nextActionId"]>[0]) => {
    assertEligible(); return observer.nextActionId(kind);
  } };
  const clearReadinessDeadline = () => {
    if (benchmarkMediaReadyTimerId !== undefined) window.clearTimeout(benchmarkMediaReadyTimerId);
    benchmarkMediaReadyTimerId = undefined;
  };
  const completeClaimedFailure = () => {
    if (benchmarkScenarioFinished || !pendingFailure) return;
    benchmarkScenarioFinished = true;
    void observer.complete("failed", pendingFailure.reason);
  };
  const clearOpeningReadinessTimeout = () => {
    if (context.opening && benchmarkScenarioStarted && host.details().ready) clearReadinessDeadline();
  };
  const failOpening = (phase: string, reason: string) => {
    if (!eligible()) return;
    const payload = { ...openingPayload(context.opening!), phase, reason };
    benchmarkScenarioFinished = true;
    benchmarkAbortController.abort(); clearReadinessDeadline(); host.release();
    observer.emit("scenario_failed", payload, { generation: mediaGeneration, required: true });
    void observer.complete("failed", reason, payload);
  };
  const playNativeVideo = async () => {
    const result = await host.play();
    assertEligible();
    if (result.status !== "playing") throw new Error(result.reason ?? result.status);
  };
  const benchmarkAbortError = () => new DOMException("benchmark scenario disposed", "AbortError");

  const waitForBenchmark = (durationMs: number) =>
    new Promise<void>((resolve, reject) => {
      const signal = benchmarkAbortController.signal;
      if (signal.aborted) {
        reject(benchmarkAbortError());
        return;
      }
      const timeoutId = window.setTimeout(() => {
        signal.removeEventListener("abort", onAbort);
        resolve();
      }, durationMs);
      const onAbort = () => {
        window.clearTimeout(timeoutId);
        reject(benchmarkAbortError());
      };
      signal.addEventListener("abort", onAbort, { once: true });
    });

  const waitForBenchmarkFrames = (count = 2) =>
    new Promise<void>((resolve, reject) => {
      const signal = benchmarkAbortController.signal;
      let frameId: number | undefined;
      let remaining = count;
      const onAbort = () => {
        if (frameId !== undefined) cancelAnimationFrame(frameId);
        reject(benchmarkAbortError());
      };
      const onFrame = () => {
        frameId = undefined;
        remaining -= 1;
        if (remaining <= 0) {
          signal.removeEventListener("abort", onAbort);
          resolve();
          return;
        }
        frameId = requestAnimationFrame(onFrame);
      };
      if (signal.aborted) {
        reject(benchmarkAbortError());
        return;
      }
      signal.addEventListener("abort", onAbort, { once: true });
      frameId = requestAnimationFrame(onFrame);
    });

  const releaseBenchmarkMedia = async () => {
    host.release();
    await waitForBenchmark(100);
  };

  const withBenchmarkTimeout = async <T,>(promise: Promise<T>, label: string, timeoutMs = SEEK_TIMEOUT_MS * 4): Promise<T> => {
    let timeoutId: number | undefined;
    const signal = benchmarkAbortController.signal;
    let rejectOnAbort: ((reason?: unknown) => void) | undefined;
    const onAbort = () => rejectOnAbort?.(benchmarkAbortError());
    try {
      return await Promise.race([
        promise,
        new Promise<never>((_, reject) => {
          timeoutId = window.setTimeout(
            () => reject(new Error(`${label} timed out`)),
            timeoutMs,
          );
        }),
        new Promise<never>((_, reject) => {
          rejectOnAbort = reject;
          if (signal.aborted) reject(benchmarkAbortError());
          else signal.addEventListener("abort", onAbort, { once: true });
        }),
      ]);
    } finally {
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
      signal.removeEventListener("abort", onAbort);
    }
  };

  const failBenchmarkAction = (
    actionKind: string,
    actionId: string,
    error: unknown,
    generation = mediaGeneration,
  ) => {
    emit(
      "action_failed",
      {
        action_kind: actionKind,
        reason: error instanceof Error ? error.message : String(error),
      },
      { generation, actionId, required: true },
    );
  };

  const runBenchmarkAction = async (action: ScenarioAction) => {
    assertEligible();
    switch (action.kind) {
      case "wait":
        await waitForBenchmark(action.durationMs);
        return;
      case "play": {
        const actionId = observer.nextActionId("play");
        emit(
          "play_requested",
          { ...(context.opening ? openingPayload(context.opening) : {}), media_time_ms: host.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {

          await playNativeVideo();
          if (host.snapshot().media.paused || host.snapshot().media.ended) throw new Error("native playback did not start");
          emit(
            "play_complete",
            { ...(context.opening ? openingPayload(context.opening) : {}), media_time_ms: host.snapshot().media.browserSeconds * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );

          return;
        } catch (error) {

          if (!benchmarkAbortController.signal.aborted) failBenchmarkAction("play", actionId, error);
          throw error;
        }
      }
      case "pause": {
        const actionId = observer.nextActionId("pause");
        emit(
          "pause_requested",
          { ...(context.opening ? openingPayload(context.opening) : {}), media_time_ms: host.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, actionId, required: true },
        );
        try {

          host.pause();
          if (!host.snapshot().media.paused) throw new Error("native playback did not pause");
          emit(
            "pause_complete",
            { ...(context.opening ? openingPayload(context.opening) : {}), media_time_ms: host.snapshot().media.browserSeconds * 1_000 },
            { generation: mediaGeneration, actionId, required: true },
          );

          return;
        } catch (error) {

          if (!benchmarkAbortController.signal.aborted) failBenchmarkAction("pause", actionId, error);
          throw error;
        }
      }
      case "stable-playback": {
        const generation = mediaGeneration;
        const startedAt = performance.now();
        const startingMediaTime = host.snapshot().media.browserSeconds;
        const startingQuality = host.sampleMetrics().quality;
        await waitForBenchmark(action.durationMs);
        assertEligible();
        const mediaDeltaMs = (host.snapshot().media.browserSeconds - startingMediaTime) * 1_000;
        if (
          generation !== mediaGeneration ||
          host.snapshot().media.paused ||
          host.snapshot().media.ended ||
          mediaDeltaMs < Math.min(100, action.durationMs * 0.1)
        ) {
          throw new Error("stable playback window did not advance current media");
        }
        const quality = host.sampleMetrics().quality;
        emit(
          "steady_playback",
          {
            ...(context.opening ? openingPayload(context.opening) : {}),
            duration_ms: performance.now() - startedAt,
            media_advance_ms: mediaDeltaMs,
            total_frame_delta:
              quality.totalFrames !== null && startingQuality.totalFrames !== null
                ? quality.totalFrames - startingQuality.totalFrames
                : null,
            dropped_frame_delta:
              quality.droppedFrames !== null && startingQuality.droppedFrames !== null
                ? quality.droppedFrames - startingQuality.droppedFrames
                : null,
            presentation_clock: host.presentationClock(),
          },
          { generation, required: true },
        );
        return;
      }
      case "rate": {
        const actionId = observer.nextActionId("rate");
        const generation = mediaGeneration;
        const startedAt = performance.now();
        const startingMediaTime = host.snapshot().media.browserSeconds;
        const startingQuality = host.sampleMetrics().quality;
        emit(
          "rate_requested",
          { rate: action.rate },
          { generation, actionId, required: true },
        );
        try {
          if (host.snapshot().media.paused || host.snapshot().media.ended) throw new Error("rate action requires active playback");
          host.setRate(action.rate as PlaybackSnapshot["rate"]["selected"]);
          emit(
            "rate_applied",
            { requested_rate: action.rate, actual_rate: host.snapshot().rate.applied },
            { generation, actionId, required: true },
          );
          await waitForBenchmark(action.durationMs);
          assertEligible();
          const wallSeconds = (performance.now() - startedAt) / 1_000;
          const mediaAdvance = host.snapshot().media.browserSeconds - startingMediaTime;
          if (generation !== mediaGeneration || host.snapshot().media.paused || host.snapshot().media.ended || mediaAdvance <= 0) {
            throw new Error("rate window ended without advancing current media");
          }
          const quality = host.sampleMetrics().quality;
          emit(
            "rate_observed",
            {
              requested_rate: action.rate,
              actual_rate: host.snapshot().rate.applied,
              effective_rate: wallSeconds > 0 ? mediaAdvance / wallSeconds : null,
              muted: host.snapshot().muted,
              volume: host.snapshot().volume,
              dropped_frames: quality?.droppedFrames ?? null,
              dropped_frame_delta:
                quality.droppedFrames !== null && startingQuality.droppedFrames !== null
                  ? quality.droppedFrames - startingQuality.droppedFrames
                  : null,
            },
            { generation, actionId, required: true },
          );
          return;
        } catch (error) {
          if (!benchmarkAbortController.signal.aborted) {
            failBenchmarkAction("rate", actionId, error, generation);
          }
          throw error;
        }
      }
      case "seek":
        {
          const reason =
            action.reason === "endpoint-edit" || action.reason === "event-jump" ||
            action.reason === "render-preconditioning" || action.reason === "seek-preconditioning"
              ? action.reason
              : "benchmark";
          let target = millisecondsToReplayTick(action.targetMs, replayEnd);
          if (reason === "event-jump") {
            const selected = host.prepareEventJump(target);
            target = selected.target;
            if (selected.eventType !== undefined) emit("event_jump_selected", {
              event_type: selected.eventType, target_ms: replayTickToMilliseconds(target),
            }, { generation: mediaGeneration });
          }
          if (reason === "endpoint-edit") {
            const endpoint: ClipEndpoint = benchmarkEndpointSequence++ % 2 === 0 ? "start" : "end";
            const edited = host.editClipEndpoint(endpoint, target);
            target = edited.target;
            emit("clip_endpoint_updated", { endpoint, clip_start_ms: replayTickToMilliseconds(edited.start),
              clip_end_ms: replayTickToMilliseconds(edited.end), target_ms: replayTickToMilliseconds(target) }, { generation: mediaGeneration });
          }
          await withBenchmarkTimeout(
            seekTo(target, { reason, awaitBenchmarkEvidence: true }),
            `seek to ${replayTickToMilliseconds(target)}`,
          );
          assertEligible();
          if (reason === "endpoint-edit") host.finishClipEndpointEdit();
          await waitForBenchmark(100);
          return;
        }
      case "scrub": {
        emit(
          "scrub_started",
          { request_count: action.targetsMs.length },
          { generation: mediaGeneration, required: true },
        );
        let settle = Promise.resolve();
        for (const [index, targetMs] of action.targetsMs.entries()) {
          assertEligible();
          settle = seekTo(millisecondsToReplayTick(targetMs, replayEnd), {
            reason: observer.scenario().kind === "whole_app" ? "benchmark-scrub" : "benchmark",
            awaitBenchmarkEvidence: index === action.targetsMs.length - 1,
          });
          if (index < action.targetsMs.length - 1) await waitForBenchmark(action.intervalMs);
        }
        await withBenchmarkTimeout(settle, "scrub settle");
        emit(
          "scrub_settled",
          { request_count: action.targetsMs.length, media_time_ms: host.snapshot().media.browserSeconds * 1_000 },
          { generation: mediaGeneration, required: true },
        );
        return;
      }
      case "desktop-resize":
        await runBenchmarkDesktopResize(helperObserver, host.resize, benchmarkAbortController.signal, waitForBenchmark);
        return;
      case "render-ready": {
        const { read, root } = benchmarkRenderObservation(host.render);
        await waitForBenchmarkRenderedReplay(helperObserver, benchmarkAbortController.signal, read, root);
        return;
      }
      case "fullscreen":
        {
          let settle: Promise<void> | undefined;
          if (action.seekTargetMs !== undefined) {
            let markDispatched!: () => void;
            const dispatched = new Promise<void>((resolve) => {
              markDispatched = resolve;
            });
            settle = seekTo(millisecondsToReplayTick(action.seekTargetMs, replayEnd), {
              reason: "benchmark",
              onBenchmarkDispatch: markDispatched,
              awaitBenchmarkEvidence: true,
            });
            await withBenchmarkTimeout(Promise.race([dispatched, settle]), "layout seek dispatch");
          }
          assertEligible();
          host.setFullscreen(action.enabled, true);
          await waitForBenchmarkFrames();
          if (settle) await withBenchmarkTimeout(settle, "layout seek settle");
          return;
        }
      case "clip-mode":
        host.setClipSelection(action.enabled, action.anchorMs === undefined ? undefined : millisecondsToReplayTick(action.anchorMs, replayEnd));
        emit(
          "clip_mode_applied",
          { enabled: action.enabled, anchor_ms: action.anchorMs ?? null },
          { generation: mediaGeneration, required: true },
        );
        return;
    }
  };

  const runBenchmarkScenario = async () => {
    if (
      !eligible() ||
      (!context.opening && !host.details().ready) || host.details().error ||
      benchmarkScenarioStarted ||
      host.snapshot().readiness !== "ready" ||
      canPlayGeneration !== mediaGeneration ||
      firstPresentedGeneration !== mediaGeneration
    ) {
      return;
    }
    benchmarkScenarioStarted = true;
    clearOpeningReadinessTimeout();
    if (!context.opening && benchmarkMediaReadyTimerId !== undefined) {
      window.clearTimeout(benchmarkMediaReadyTimerId);
      benchmarkMediaReadyTimerId = undefined;
    }
    try {
      const scenario = observer.scenario();
      const actions = buildScenarioActions(scenario, durationMs);
      for (const action of actions) { assertEligible(); await runBenchmarkAction(action); assertEligible(); }
      assertEligible();
      host.pause();
      if (context.opening) {
        await withBenchmarkTimeout(admittedDetails, "opening details", BENCHMARK_MEDIA_READY_TIMEOUT_MS);
        if (!eligible() || host.details().error || !host.details().ready) return;
        if (benchmarkMediaReadyTimerId !== undefined) window.clearTimeout(benchmarkMediaReadyTimerId);
      }
      await host.refreshMetrics();
      if (!eligible()) return;
      const quality = host.sampleMetrics().quality;
      const completionKind =
        scenario.kind === "whole_app" || scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
          ? "viewer_cycle_completed"
          : "scenario_completed";
      const completionPayload = {
        ...(context.opening ? openingPayload(context.opening) : {}),
        kind: scenario.kind,
        media_time_ms: host.snapshot().media.browserSeconds * 1_000,
        ready_state: host.snapshot().media.readyState,
        network_state: host.snapshot().media.networkState,
        playback_rate: host.snapshot().rate.applied,
        total_frames: quality?.totalFrames ?? null,
        dropped_frames: quality?.droppedFrames ?? null,
        server_metrics: host.serverMetrics(),
      };
      await releaseBenchmarkMedia();
      assertEligible();
      observer.emit(completionKind, completionPayload, {
        generation: mediaGeneration,
        required: true,
      });
      benchmarkScenarioFinished = true;
      if (
        scenario.kind === "whole_app" || scenario.kind === "warm_open" ||
        (scenario.kind === "lifecycle" && typeof scenario.duration_seconds !== "number")
      ) {
        host.cycleCompleted();
        return;
      }
      await observer.complete("complete", null, { action_count: actions.length });
    } catch (error) {
      if (!eligible()) return;
      const reason = error instanceof Error ? error.message : String(error);
      if (context.opening) {
        failOpening("actions", reason);
        return;
      }
      pendingFailure = { reason };
      clearReadinessDeadline();
      observer.emit(
        "scenario_failed",
        { reason },
        { generation: mediaGeneration, required: true },
      );
      if (!current()) return;
      await releaseBenchmarkMedia().catch(() => undefined);
      if (current()) completeClaimedFailure();
    }
  };


  const seekTo = (target: ReplayTick, options: { reason?: string; playAfter?: boolean; onBenchmarkDispatch?: () => void; awaitBenchmarkEvidence?: boolean } = {}): Promise<void> => {
    assertEligible();
    const actionId = observer.nextActionId("seek");
    const evidence = options.awaitBenchmarkEvidence ? createBenchmarkSeekEvidence(mediaGeneration, actionId) : undefined;
    if (evidence) benchmarkSeekObservers.add(evidence.observe);
    if (options.onBenchmarkDispatch) dispatchObservers.set(actionId, options.onBenchmarkDispatch);
    // Registration precedes the production operation: its events can be synchronous.
    const operation = host.requestSeek(target, { reason: options.reason, playAfter: options.playAfter, actionId });
    const settled = operation.then(result => {
      dispatchObservers.delete(actionId);
      if (!eligible()) throw benchmarkAbortError();
      if (result.status === "failed" || result.status === "unavailable") throw new Error(result.reason ?? result.status);
      if (evidence && result.status === "superseded") throw new Error("benchmark final seek was superseded");
    });
    if (!evidence) return settled;
    return withBenchmarkTimeout(Promise.all([settled, evidence.complete]), "seek evidence")
      .then(() => {})
      .finally(() => { benchmarkSeekObservers.delete(evidence.observe); dispatchObservers.delete(actionId); });
  };

  const queueEligibility = () => queueMicrotask(() => { if (eligible()) void runBenchmarkScenario(); });
  return {
    isFinished: () => benchmarkScenarioFinished || Boolean(pendingFailure),
    onPlaybackEvent(event: PlaybackEvent) {
      if (!current() || benchmarkScenarioFinished) return;
      mediaGeneration = event.generation;
      benchmarkSeekObservers.forEach(observe => observe(event));
      if (event.actionId && (event.kind === "seek_dispatched" || event.kind === "seek_deduped")) {
        dispatchObservers.get(event.actionId)?.(); dispatchObservers.delete(event.actionId);
      }
      if (event.kind === "first_presented_frame") firstPresentedGeneration = event.generation;
      if (event.kind === "media_canplay") canPlayGeneration = event.generation;
      if (event.kind === "first_presented_frame" || event.kind === "media_canplay") queueEligibility();
    },
    detailsAdmitted() { if (eligible()) { admitDetails(); clearOpeningReadinessTimeout(); } },
    detailsFailed(reason: string) {
      if (!eligible()) return;
      if (context.opening) { failOpening("league_replay_window", reason); return; }
      benchmarkScenarioFinished = true; benchmarkAbortController.abort(); clearReadinessDeadline();
      observer.emit("scenario_failed", { phase: "league_replay_window", reason }, { required: true });
      void observer.complete("failed", reason, { phase: "league_replay_window" });
    },
    updateReadiness() {
      if (!eligible()) return;
      mediaGeneration = host.snapshot().generation;
      clearOpeningReadinessTimeout(); queueEligibility();
    },
    armReadinessDeadline() {
      if (!eligible() || benchmarkMediaReadyTimerId !== undefined) return;
      benchmarkMediaReadyTimerId = window.setTimeout(() => {
        if (!eligible() || (!context.opening && benchmarkScenarioStarted)) return;
        if (context.opening) { failOpening("media_readiness", "opening readiness/details timed out"); return; }
        const reason = "media readiness timed out";
        pendingFailure = { reason };
        clearReadinessDeadline();
        observer.emit("scenario_failed", { phase: "media_readiness", reason }, { generation: mediaGeneration, required: true });
        if (!current()) return;
        void releaseBenchmarkMedia().then(() => {
          if (current()) completeClaimedFailure();
        }).catch(() => {});
      }, BENCHMARK_MEDIA_READY_TIMEOUT_MS);
      clearOpeningReadinessTimeout();
    },
    dispose() {
      if (disposed) return;
      const publishFailure = current() && !benchmarkScenarioFinished;
      disposed = true;
      if (publishFailure) {
        if (pendingFailure) completeClaimedFailure();
        else {
          benchmarkScenarioFinished = true;
          const reason = "viewer disposed before benchmark completion";
          if (context.opening) {
            const payload = { ...openingPayload(context.opening), phase: "viewer_disposed", reason };
            observer.emit("scenario_failed", payload, { generation: mediaGeneration, required: true });
            void observer.complete("failed", reason, payload);
          } else void observer.complete("failed", reason);
        }
      }
      benchmarkAbortController.abort(); clearReadinessDeadline();
      dispatchObservers.clear(); benchmarkSeekObservers.clear();
    },
  };
}
