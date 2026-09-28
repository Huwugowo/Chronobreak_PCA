import { emitReplayBenchmarkEvent, replayBenchmarkObserver, type ScenarioAction } from "./benchmark";
import type { PlaybackController, PlaybackSnapshot } from "./playbackController";
import { REPLAY_TICKS_PER_SECOND, type MediaTimelineV2, type ReplayTick } from "./replayTime";
import type { ClipRange, ViewerEvent } from "./types";
import { clamp, clipEndpointPreviewTick, moveClipEndpoint, nearestIndexAt } from "./viewerUtils";

type ClipEndpoint = "start" | "end";
type BenchmarkSeekReason = "benchmark" | "endpoint-edit" | "event-jump";

/** View-owned operations; this runner owns no media, timers or subscriptions. */
export type ViewerBenchmarkActions = {
  getController(): Pick<PlaybackController, "snapshot" | "pause" | "setRate" | "sampleMetrics">;
  getGeneration(): number;
  isAborted(): boolean;
  mediaTimeline: MediaTimelineV2;
  replayEnd: ReplayTick;
  events(): (ViewerEvent & { replay_tick: ReplayTick })[];
  clipRange(): ClipRange | null;
  setClipRange(range: ClipRange | null): void;
  setEditingEndpoint(endpoint: ClipEndpoint | null): void;
  clipRangeForAnchor(anchor: ReplayTick, event?: ViewerEvent): ClipRange;
  clipStartTick(range: ClipRange): ReplayTick;
  clipEndTick(range: ClipRange): ReplayTick;
  replayPosition(): ReplayTick;
  clockSource(): "VIDEO FRAME" | "ANIMATION FRAME";
  playNativeVideo(propagateFailure?: boolean): Promise<void>;
  seekTo(target: ReplayTick, options: { reason: BenchmarkSeekReason; onBenchmarkDispatch?: () => void }): Promise<void>;
  waitForBenchmark(durationMs: number): Promise<void>;
  waitForBenchmarkFrames(count?: number): Promise<void>;
  withBenchmarkTimeout<T>(promise: Promise<T>, label: string): Promise<T>;
  enterFullscreen(required: boolean): void;
  exitFullscreen(required: boolean): void;
  cancelClipMode(): void;
};

const replayTickToMilliseconds = (tick: ReplayTick): number =>
  (tick * 1_000) / REPLAY_TICKS_PER_SECOND;

const millisecondsToReplayTick = (milliseconds: number, replayEnd: ReplayTick): ReplayTick =>
  Math.round(
    clamp(milliseconds, 0, replayTickToMilliseconds(replayEnd)) * REPLAY_TICKS_PER_SECOND / 1_000,
  ) as ReplayTick;

export function createViewerBenchmarkActions(context: ViewerBenchmarkActions) {
  const { getController, getGeneration, isAborted, mediaTimeline, replayEnd, events,
    clipRange, setClipRange, setEditingEndpoint, clipRangeForAnchor, clipStartTick,
    clipEndTick, replayPosition, clockSource, playNativeVideo, seekTo,
    waitForBenchmark, waitForBenchmarkFrames, withBenchmarkTimeout,
    enterFullscreen, exitFullscreen, cancelClipMode } = context;
  let benchmarkEndpointSequence = 0;

  const failBenchmarkAction = (
    actionKind: string,
    actionId: string,
    error: unknown,
    generation = getGeneration(),
  ) => {
    emitReplayBenchmarkEvent(
      "action_failed",
      {
        action_kind: actionKind,
        reason: error instanceof Error ? error.message : String(error),
      },
      { generation, actionId, required: true },
    );
  };

  const runBenchmarkAction = async (action: ScenarioAction) => {
    const controller = getController();
    switch (action.kind) {
      case "wait":
        await waitForBenchmark(action.durationMs);
        return;
      case "play": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("play");
        emitReplayBenchmarkEvent(
          "play_requested",
          { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: getGeneration(), actionId, required: true },
        );
        try {

          await playNativeVideo(true);
          if (controller.snapshot().media.paused || controller.snapshot().media.ended) throw new Error("native playback did not start");
          emitReplayBenchmarkEvent(
            "play_complete",
            { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
            { generation: getGeneration(), actionId, required: true },
          );

          return;
        } catch (error) {

          if (!isAborted()) failBenchmarkAction("play", actionId, error);
          throw error;
        }
      }
      case "pause": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("pause");
        emitReplayBenchmarkEvent(
          "pause_requested",
          { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: getGeneration(), actionId, required: true },
        );
        try {

          controller.pause();
          if (!controller.snapshot().media.paused) throw new Error("native playback did not pause");
          emitReplayBenchmarkEvent(
            "pause_complete",
            { media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
            { generation: getGeneration(), actionId, required: true },
          );

          return;
        } catch (error) {

          if (!isAborted()) failBenchmarkAction("pause", actionId, error);
          throw error;
        }
      }
      case "stable-playback": {
        const generation = getGeneration();
        const startedAt = performance.now();
        const startingMediaTime = controller.snapshot().media.browserSeconds;
        const startingQuality = controller.sampleMetrics().quality;
        await waitForBenchmark(action.durationMs);
        const mediaDeltaMs = (controller.snapshot().media.browserSeconds - startingMediaTime) * 1_000;
        if (
          generation !== getGeneration() ||
          controller.snapshot().media.paused ||
          controller.snapshot().media.ended ||
          mediaDeltaMs < Math.min(100, action.durationMs * 0.1)
        ) {
          throw new Error("stable playback window did not advance current media");
        }
        const quality = controller.sampleMetrics().quality;
        emitReplayBenchmarkEvent(
          "steady_playback",
          {
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
            presentation_clock: clockSource(),
          },
          { generation, required: true },
        );
        return;
      }
      case "rate": {
        const observer = replayBenchmarkObserver();
        if (!observer) throw new Error("benchmark observer is unavailable");
        const actionId = observer.nextActionId("rate");
        const generation = getGeneration();
        const startedAt = performance.now();
        const startingMediaTime = controller.snapshot().media.browserSeconds;
        const startingQuality = controller.sampleMetrics().quality;
        emitReplayBenchmarkEvent(
          "rate_requested",
          { rate: action.rate },
          { generation, actionId, required: true },
        );
        try {
          if (controller.snapshot().media.paused || controller.snapshot().media.ended) throw new Error("rate action requires active playback");
          controller.setRate(action.rate as PlaybackSnapshot["rate"]["selected"]);
          emitReplayBenchmarkEvent(
            "rate_applied",
            { requested_rate: action.rate, actual_rate: controller.snapshot().rate.applied },
            { generation, actionId, required: true },
          );
          await waitForBenchmark(action.durationMs);
          const wallSeconds = (performance.now() - startedAt) / 1_000;
          const mediaAdvance = controller.snapshot().media.browserSeconds - startingMediaTime;
          if (generation !== getGeneration() || controller.snapshot().media.paused || controller.snapshot().media.ended || mediaAdvance <= 0) {
            throw new Error("rate window ended without advancing current media");
          }
          const quality = controller.sampleMetrics().quality;
          emitReplayBenchmarkEvent(
            "rate_observed",
            {
              requested_rate: action.rate,
              actual_rate: controller.snapshot().rate.applied,
              effective_rate: wallSeconds > 0 ? mediaAdvance / wallSeconds : null,
              muted: controller.snapshot().muted,
              volume: controller.snapshot().volume,
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
          if (!isAborted()) {
            failBenchmarkAction("rate", actionId, error, generation);
          }
          throw error;
        }
      }
      case "seek":
        {
          const reason: BenchmarkSeekReason =
            action.reason === "endpoint-edit" || action.reason === "event-jump"
              ? action.reason
              : "benchmark";
          let target = millisecondsToReplayTick(action.targetMs, replayEnd);
          if (reason === "event-jump" && events().length > 0) {
            const eventIndex = nearestIndexAt(events(), target);
            if (eventIndex >= 0) {
              const selected = events()[eventIndex];
              target = selected.replay_tick;
              if (clipRange()) setClipRange(clipRangeForAnchor(target, selected));
              emitReplayBenchmarkEvent(
                "event_jump_selected",
                {
                  event_type: selected.event_type,
                  target_ms: replayTickToMilliseconds(target),
                },
                { generation: getGeneration() },
              );
            }
          }
          if (reason === "endpoint-edit") {
            if (!clipRange()) setClipRange(clipRangeForAnchor(target));
            const endpoint: ClipEndpoint =
              benchmarkEndpointSequence++ % 2 === 0 ? "start" : "end";
            const range = clipRange();
            if (!range) throw new Error("benchmark endpoint edit could not create a clip range");
            const nextRange = moveClipEndpoint(
              range,
              endpoint,
              target,
              mediaTimeline,
            );
            setEditingEndpoint(endpoint);
            setClipRange(nextRange);
            target = clipEndpointPreviewTick(nextRange, endpoint, mediaTimeline);
            emitReplayBenchmarkEvent(
              "clip_endpoint_updated",
              {
                endpoint,
                clip_start_ms: replayTickToMilliseconds(clipStartTick(nextRange)),
                clip_end_ms: replayTickToMilliseconds(clipEndTick(nextRange)),
                target_ms: replayTickToMilliseconds(target),
              },
              { generation: getGeneration() },
            );
          }
          await withBenchmarkTimeout(
            seekTo(target, { reason }),
            `seek to ${replayTickToMilliseconds(target)}`,
          );
          if (reason === "endpoint-edit") setEditingEndpoint(null);
          await waitForBenchmark(100);
          return;
        }
      case "scrub": {
        emitReplayBenchmarkEvent(
          "scrub_started",
          { request_count: action.targetsMs.length },
          { generation: getGeneration(), required: true },
        );
        let settle = Promise.resolve();
        for (const targetMs of action.targetsMs) {
          settle = seekTo(millisecondsToReplayTick(targetMs, replayEnd), { reason: "benchmark" });
          await waitForBenchmark(action.intervalMs);
        }
        await withBenchmarkTimeout(settle, "scrub settle");
        emitReplayBenchmarkEvent(
          "scrub_settled",
          { request_count: action.targetsMs.length, media_time_ms: controller.snapshot().media.browserSeconds * 1_000 },
          { generation: getGeneration(), required: true },
        );
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
            });
            await withBenchmarkTimeout(dispatched, "layout seek dispatch");
          }
          if (action.enabled) enterFullscreen(true);
          else exitFullscreen(true);
          await waitForBenchmarkFrames();
          if (settle) await withBenchmarkTimeout(settle, "layout seek settle");
          return;
        }
      case "clip-mode":
        if (action.enabled) {
          const anchor = action.anchorMs === undefined
            ? controller.snapshot().media.tick ?? replayPosition()
            : millisecondsToReplayTick(action.anchorMs, replayEnd);
          setClipRange(clipRangeForAnchor(anchor));
        } else {
          cancelClipMode();
        }
        emitReplayBenchmarkEvent(
          "clip_mode_applied",
          { enabled: action.enabled, anchor_ms: action.anchorMs ?? null },
          { generation: getGeneration(), required: true },
        );
        return;
    }
  };

  return runBenchmarkAction;
}
