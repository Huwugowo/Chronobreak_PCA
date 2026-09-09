import {
  REPLAY_TICKS_PER_SECOND,
  createClipRange,
  frameBoundaryToReplayTick,
  replayTickToFrameBoundary,
  type ClipRange,
  type FrameBoundary,
  type MediaId,
  type MediaTimelineV2,
  type ReplayTick,
} from "./replayTime";
import {
  beginRecovery, createViewerSeekState, dispatchSeek, observePresentation,
  observeSeeked, requestPreview, type ViewerSeekState,
} from "./viewerSeekState";
import type {
  MediaObservation, PlaybackMediaAdapter, PlaybackMediaEvent, PlaybackQuality,
} from "./htmlVideoPlaybackAdapter";

export const PLAYBACK_RATES = [0.25, 0.5, 1, 2, 4, 8] as const;
export type PlaybackRate = typeof PLAYBACK_RATES[number];
export type PlaybackReadiness = "closed" | "loading" | "ready" | "recovering" | "degraded" | "disposed";
export type PlaybackOutcome = Readonly<{
  status: "ready" | "playing" | "paused" | "superseded" | "unavailable" | "failed" | "activation-required";
  reason?: string;
}>;
export type SeekOutcome = Readonly<{
  status: "presented" | "deduplicated" | "superseded" | "unavailable" | "failed";
  target: ReplayTick;
  presented?: ReplayTick;
  reason?: string;
}>;
export type RateState = Readonly<{
  selected: PlaybackRate;
  applied: number;
  effective: PlaybackRate;
  observed: number | null;
  outcome: "pending" | "verified" | "limited" | "unsupported" | "suspended" | "unknown";
  limitation: string | null;
}>;
export type PlaybackEvent = Readonly<{
  kind: string;
  generation: number;
  actionId?: string;
  atMs: number;
  payload: Readonly<Record<string, unknown>>;
}>;
export type PlaybackSnapshot = Readonly<{
  readiness: PlaybackReadiness;
  mediaId: MediaId | null;
  generation: number;
  sessionToken: string | null;
  sourceUrl: string | null;
  seek: ViewerSeekState;
  media: MediaObservation;
  desiredPlaying: boolean;
  activationRequired: boolean;
  error: string | null;
  rate: RateState;
  muted: boolean;
  volume: number;
  audio: "user-muted" | "zero-volume" | "available-but-unverified" | "runtime-limited" | "unknown";
  quality: PlaybackQuality;
  presentedFps: number;
  seekLatencyMs: number | null;
  recoveryCount: number;
  queue: "IDLE" | "1 ACTIVE" | "1 ACTIVE + LATEST" | "1 PENDING" | "AWAITING FRAME";
  authority: "rvfc" | "media-clock-approximate";
  diagnostics: readonly PlaybackEvent[];
  owned: Readonly<{ timers: number; listeners: number; frames: number; activeSeeks: number; pendingSeeks: number }>;
}>;
export type PlaybackClock = {
  now(): number;
  setTimeout(callback: () => void, delayMs: number): number;
  clearTimeout(handle: number): void;
  token(): string;
};
const browserClock: PlaybackClock = {
  now: () => performance.now(),
  setTimeout: (callback, delay) => globalThis.setTimeout(callback, delay),
  clearTimeout: (handle) => globalThis.clearTimeout(handle),
  token: () => crypto.randomUUID(),
};
type SeekOptions = { reason?: string; playAfter?: boolean; actionId?: string };
type PendingSeek = {
  target: ReplayTick;
  preview: ReplayTick;
  reason: string;
  actionId?: string;
  generation: number;
  intent: number;
  epoch: number;
  requestedAt: number;
  dispatchedAt: number;
  from: ReplayTick;
  presented: ReplayTick | null;
  settled: boolean;
  resolve(outcome: SeekOutcome): void;
};
const zero = 0 as ReplayTick;
const ms = (tick: number) => tick * 1_000 / REPLAY_TICKS_PER_SECOND;
const failureText = (error: unknown) => error instanceof Error ? error.message : String(error);
const EVENTS: PlaybackMediaEvent[] = [
  "loadstart", "loadedmetadata", "loadeddata", "canplay", "playing", "play", "pause",
  "seeking", "seeked", "ended", "error", "waiting", "stalled", "timeupdate",
  "ratechange", "volumechange", "visibilitychange",
];

/** Sole primary-media lifecycle owner. Scenario/layout/IPC policy stays outside. */
export function createPlaybackController(
  media: PlaybackMediaAdapter,
  options: { clock?: PlaybackClock; eventSink?: (event: PlaybackEvent) => void; generation?: number } = {},
) {
  const clock = options.clock ?? browserClock;
  const timers = new Map<string, number>();
  const listeners: Array<() => void> = [];
  const subscribers = new Set<(snapshot: PlaybackSnapshot) => void>();
  const frameSubscribers = new Set<(tick: ReplayTick, authority: PlaybackSnapshot["authority"]) => void>();
  let readiness: PlaybackReadiness = "closed";
  let timeline: MediaTimelineV2 | undefined;
  let originalUrl = "";
  let sourceUrl: string | null = null;
  let sessionToken: string | null = null;
  let state = createViewerSeekState(options.generation ?? 0);
  let desiredPlaying = false;
  let activationRequired = false;
  let error: string | null = null;
  let muted = false;
  let volume = 1;
  let muteAssignmentFailed = false;
  let volumeAssignmentFailed = false;
  let rate: RateState = { selected: 1, applied: 1, effective: 1, observed: null, outcome: "pending", limitation: null };
  let quality: PlaybackQuality = { totalFrames: null, droppedFrames: null, heapBytes: null };
  let diagnostics: PlaybackEvent[] = [];
  let loop: ClipRange | null = null;
  let active: PendingSeek | undefined;
  let pending: PendingSeek | undefined;
  let waiter: PendingSeek | undefined;
  let intent = 0;
  let frameVersion = 0;
  let frameHandle: number | undefined;
  let playToken = 0;
  let nudgeToken = 0;
  let finishPlay: ((value: PlaybackOutcome) => void) | undefined;
  let finishOpen: ((value: PlaybackOutcome) => void) | undefined;
  let nudge: PendingSeek | undefined;
  let lastDispatchAt = -Infinity;
  let loadStarted = false;
  let metadataReady = false;
  let recoveryTarget: ReplayTick | null = null;
  let recoveryCount = 0;
  let consecutiveFailures = 0;
  let recoveryAttempts: number[] = [];
  let healthySince: number | null = null;
  let lastAdvancingAt = clock.now();
  let lastFrameTick: ReplayTick | null = null;
  let frames = 0;
  let presentedFps = 0;
  let metricsAt = clock.now();
  let seekLatencyMs: number | null = null;
  let firstPresented = false;
  let waitingSince: number | null = null;
  let rateToken = 0;
  let lastVerifiedRate: PlaybackRate | null = null;
  let fallbackReloaded = false;
  let limitedWindows = 0;
  let capabilityStartedAt: number | null = null;
  let rateWindow: {
    token: number; startedAt: number; progressAt: number; lastTick: ReplayTick | null;
    first: { tick: ReplayTick; at: number } | null; samples: number;
  } | null = null;

  const isLive = () => readiness !== "disposed" && readiness !== "closed";
  const currentSource = () => media.read().source === sourceUrl;
  const clearTimer = (name: string) => {
    const handle = timers.get(name);
    if (handle !== undefined) clock.clearTimeout(handle);
    timers.delete(name);
  };
  const later = (name: string, delay: number, callback: () => void) => {
    clearTimer(name);
    const generation = state.generation;
    const handle = clock.setTimeout(() => {
      if (timers.get(name) !== handle) return;
      timers.delete(name);
      if (generation === state.generation && isLive()) callback();
    }, delay);
    timers.set(name, handle);
  };
  const emit = (kind: string, payload: Record<string, unknown> = {}, request?: { actionId?: string }) => {
    const event = Object.freeze({ kind, payload: Object.freeze(payload), generation: state.generation,
      atMs: clock.now(), actionId: request?.actionId });
    diagnostics = [...diagnostics.slice(-49), event];
    options.eventSink?.(event);
  };
  const snapshot = (): PlaybackSnapshot => Object.freeze({
    readiness, mediaId: timeline?.mediaId ?? null, generation: state.generation, sessionToken, sourceUrl,
    seek: Object.freeze({ ...state }), media: Object.freeze(media.read()), desiredPlaying,
    activationRequired, error, rate: Object.freeze({ ...rate }), muted, volume,
    audio: muteAssignmentFailed || volumeAssignmentFailed || media.read().muted !== muted || media.read().volume !== volume
      ? "runtime-limited" : muted ? "user-muted" : volume === 0 ? "zero-volume"
        : !timeline?.audio.present ? "unknown" : "available-but-unverified",
    quality, presentedFps, seekLatencyMs, recoveryCount,
    queue: active ? pending ? "1 ACTIVE + LATEST" : "1 ACTIVE" : pending ? "1 PENDING" : waiter ? "AWAITING FRAME" : "IDLE",
    authority: media.hasVideoFrameCallback ? "rvfc" : "media-clock-approximate",
    diagnostics: Object.freeze([...diagnostics]),
    owned: Object.freeze({ timers: timers.size, listeners: listeners.length, frames: frameHandle === undefined ? 0 : 1,
      activeSeeks: active ? 1 : 0, pendingSeeks: pending ? 1 : 0 }),
  });
  const publish = () => { const value = snapshot(); subscribers.forEach((subscriber) => subscriber(value)); };
  const settle = (request: PendingSeek | undefined, status: SeekOutcome["status"], reason?: string) => {
    if (!request || request.settled) return;
    request.settled = true;
    request.resolve({ status, target: request.target, presented: request.presented ?? undefined, reason });
    if (status === "superseded") emit("action_cancelled_superseded", { target_ms: ms(request.target), reason }, request);
    if (status === "failed" || status === "unavailable") emit("action_failed", { phase: "seek_presentation", reason, target_ms: ms(request.target) }, request);
  };
  const cancelFrames = () => {
    frameVersion++;
    if (frameHandle !== undefined) media.cancelFrame(frameHandle);
    frameHandle = undefined;
  };
  const applyEffectiveRate = () => {
    try {
      media.setRate(rate.effective);
      rate = { ...rate, applied: media.read().rate };
      if (rate.applied !== rate.effective) failRate("unsupported", "The browser did not apply the requested speed");
    } catch (reason) {
      rate = { ...rate, applied: media.read().rate };
      failRate("unsupported", `The browser rejected this speed: ${failureText(reason)}`);
    }
  };
  const applyMuted = () => {
    muteAssignmentFailed = false;
    try { media.setMuted(muted); }
    catch (reason) { muteAssignmentFailed = true; emit("audio_control_limited", { control: "mute", reason: failureText(reason) }); }
  };
  const applyVolume = () => {
    volumeAssignmentFailed = false;
    try { media.setVolume(volume); }
    catch (reason) { volumeAssignmentFailed = true; emit("audio_control_limited", { control: "volume", reason: failureText(reason) }); }
  };
  const stopNudge = () => {
    if (!nudge) return;
    nudge = undefined;
    clearTimer("nudge");
    nudgeToken++;
    if (!desiredPlaying) media.pause();
    applyEffectiveRate();
    resetRateObservation();
  };
  const cancelPlay = (outcome: PlaybackOutcome = { status: "superseded" }) => {
    playToken++;
    clearTimer("play");
    const finish = finishPlay;
    finishPlay = undefined;
    finish?.(outcome);
  };
  const cancelWork = (status: SeekOutcome["status"] = "superseded", reason = "Media generation ended") => {
    for (const name of [...timers.keys()]) clearTimer(name);
    cancelFrames();
    stopNudge();
    listeners.splice(0).forEach((remove) => remove());
    settle(active, status, reason);
    settle(pending, status, reason);
    settle(waiter, status, reason);
    active = pending = waiter = undefined;
    lastDispatchAt = -Infinity;
    cancelPlay({ status: status === "failed" ? "failed" : "superseded", reason });
    finishOpen?.({ status: status === "failed" ? "failed" : "superseded", reason });
    finishOpen = undefined;
    loadStarted = metadataReady = false;
    waitingSince = healthySince = null;
    rateWindow = null;
    capabilityStartedAt = null;
    limitedWindows = 0;
    rateToken++;
    rate = { ...rate, observed: null, outcome: rate.limitation ? rate.outcome : "suspended" };
    lastFrameTick = null;
    firstPresented = false;
    intent++;
  };
  const degrade = (reason: string) => {
    cancelWork("failed", reason);
    media.pause();
    readiness = "degraded";
    error = reason;
    emit("preview_degraded", { reason });
    publish();
  };
  const frameTicks = () => timeline ? frameBoundaryToReplayTick(1 as FrameBoundary, timeline.video.frameRate) : 1;
  const tolerance = () => frameTicks() + REPLAY_TICKS_PER_SECOND / 1_000_000;

  function resetRateObservation(preserveCapabilityAttempt = false) {
    clearTimer("rate");
    rateWindow = null;
    if (!preserveCapabilityAttempt) capabilityStartedAt = null;
    limitedWindows = 0;
    rateToken++;
    rate = { ...rate, observed: null, outcome: rate.limitation ? rate.outcome : "suspended" };
  }

  function failRate(outcome: "limited" | "unsupported" | "unknown", reason: string) {
    if (rate.limitation) {
      // A failed fallback gets one reload in this capability attempt, then stops.
      if (fallbackReloaded) { degrade(`Fallback playback failed: ${reason}`); return; }
      fallbackReloaded = true;
      recover(`Fallback playback did not advance: ${reason}`);
      return;
    }
    const fallback = lastVerifiedRate !== null && lastVerifiedRate !== rate.effective ? lastVerifiedRate : 1;
    emit("rate_limited", { selected: rate.selected, applied: rate.applied, observed: rate.observed, outcome, fallback, reason });
    resetRateObservation();
    rate = { ...rate, effective: fallback, outcome, limitation: reason };
    applyEffectiveRate();
    cancelFrames(); armFrame();
    publish();
  }

  function rateEligible() {
    const observed = media.read();
    if (!timeline || readiness !== "ready" || !metadataReady || !currentSource() || !desiredPlaying
      || activationRequired || observed.paused || observed.ended || observed.hidden || observed.seeking
      || active || pending || waiter || nudge || waitingSince !== null || observed.readyState < 3) return false;
    const end = loop ? frameBoundaryToReplayTick(loop.endFrameExclusive, timeline.video.frameRate) : timeline.video.replayEnd;
    // There must be room for a full window before an intentional discontinuity.
    return observed.tick !== null && end - observed.tick > rate.effective * 3 * REPLAY_TICKS_PER_SECOND + 2 * frameTicks();
  }

  function watchRate() {
    clearTimer("rate");
    if (!rateEligible()) {
      if (rateWindow || (!rate.limitation && rate.outcome !== "suspended")) {
        resetRateObservation(waitingSince !== null || media.read().readyState < 3);
      }
      return;
    }
    if (!media.hasVideoFrameCallback) {
      rate = { ...rate, observed: null, outcome: rate.limitation ? rate.outcome : "unknown" };
      return;
    }
    const now = clock.now();
    if (rate.observed === null) capabilityStartedAt ??= now;
    if (capabilityStartedAt !== null && now - capabilityStartedAt >= 10_000) {
      failRate("unknown", "Speed could not be verified within 10 seconds of interrupted playback");
      return;
    }
    if (!rateWindow) {
      rateWindow = { token: rateToken, startedAt: now, progressAt: now, lastTick: null, first: null, samples: 0 };
      if (!rate.limitation) rate = { ...rate, outcome: "pending" };
    }
    const window = rateWindow;
    if (now - window.startedAt >= 2_000 && window.lastTick !== null && now - window.progressAt >= 1_500) {
      failRate("limited", "Presented video stopped advancing for 1500 ms");
    } else if (now - window.startedAt >= 5_000 && window.samples < 3 && rate.observed === null) {
      // Missing evidence is unknown, not a measured stall. It still has a finite
      // deadline so a failed fallback cannot wait forever for its first callback.
      failRate("unknown", "Speed could not be verified: no presented-frame evidence within 5000 ms");
    }
    if (readiness === "ready" && rateEligible()) {
      const token = rateToken;
      later("rate", 250, () => {
        if (token !== rateToken) return;
        const before = rate;
        watchRate();
        if (rate !== before) publish();
      });
    }
  }

  function observeRateFrame(tick: ReplayTick, at: number, token: number) {
    if (token !== rateToken || !rateEligible()) return;
    if (!rateWindow) watchRate();
    const window = rateWindow;
    if (!window || window.token !== token) return;
    if (window.lastTick !== null && tick < window.lastTick) {
      failRate("limited", "Presented video moved backwards during continuous playback"); return;
    }
    if (window.lastTick === null || tick > window.lastTick) window.progressAt = at;
    window.lastTick = tick;
    if (at - window.startedAt < 2_000) return;
    window.first ??= { tick, at };
    window.samples++;
    const elapsed = at - window.first.at;
    if (elapsed < 3_000 || window.samples < 3) return;
    const advancement = tick - window.first.tick;
    const expected = elapsed / 1_000 * REPLAY_TICKS_PER_SECOND * rate.effective;
    const observed = advancement / REPLAY_TICKS_PER_SECOND / (elapsed / 1_000);
    const verified = advancement > 0 && Math.abs(advancement - expected) <= Math.max(expected * 0.1, 2 * frameTicks());
    rate = { ...rate, observed, outcome: rate.limitation ? rate.outcome : verified ? "verified" : "pending" };
    emit("playback_rate_observed", { selected: rate.selected, applied: rate.applied, effective: rate.effective, observed, verified,
      samples: window.samples, window_ms: elapsed });
    if (verified) { lastVerifiedRate = rate.effective; limitedWindows = 0; capabilityStartedAt = null; }
    else if (++limitedWindows >= 2) {
      failRate("limited", "Presented video could not sustain this speed in two observation windows"); return;
    }
    window.first = { tick, at }; window.samples = 1;
    publish();
  }
  const resumeDesired = () => {
    if (desiredPlaying && readiness === "ready") void startPlay();
    else if (!nudge) media.pause();
  };

  function startPlay(actionId?: string): Promise<PlaybackOutcome> {
    cancelPlay();
    const token = ++playToken;
    const generation = state.generation;
    if (readiness !== "ready") return Promise.resolve({ status: "unavailable", reason: "Media is not ready" });
    let resolve!: (value: PlaybackOutcome) => void;
    const result = new Promise<PlaybackOutcome>((done) => { resolve = done; });
    finishPlay = resolve;
    const current = () => token === playToken && generation === state.generation && finishPlay === resolve && isLive();
    const complete = (outcome: PlaybackOutcome) => {
      if (!current()) return;
      clearTimer("play");
      finishPlay = undefined;
      resolve(outcome);
      publish();
    };
    const rejected = (reason: unknown) => {
      if (!current()) return;
      const name = reason instanceof Error ? reason.name : "unknown";
      activationRequired = name === "NotAllowedError";
      emit("play_rejected", { name, reason: failureText(reason) }, { actionId });
      complete({ status: activationRequired ? "activation-required" : name === "AbortError" ? "superseded" : "failed", reason: failureText(reason) });
      if (media.read().error) recover("Native media error after play rejection");
    };
    later("play", 5_000, () => {
      if (!current()) return;
      complete({ status: "failed", reason: "Playback start timed out" });
      recover("Playback start timed out");
    });
    // Invoke synchronously, before any await/microtask, to retain user activation.
    try {
      const native = media.play();
      native.then(() => {
        if (!current()) return;
        activationRequired = false;
        watchRate();
        complete({ status: media.read().paused ? "failed" : "playing" });
      }, rejected);
    } catch (reason) { rejected(reason); }
    return result;
  }

  function armFrame() {
    if (!media.hasVideoFrameCallback || frameHandle !== undefined || !isLive() || readiness === "degraded") return;
    const generation = state.generation;
    const version = frameVersion;
    const owner = intent;
    const rateOwner = rateToken;
    const epoch = state.dispatched?.epoch ?? null;
    frameHandle = media.requestFrame((tick, at) => {
      if (generation !== state.generation || version !== frameVersion || owner !== intent || !isLive()) return;
      frameHandle = undefined;
      if (metadataReady && currentSource() && tick !== null && !pending) {
        const request = active ?? waiter;
        const eligible = !request || request.intent === owner;
        if (eligible) {
          const next = observePresentation(state, generation, epoch, tick, "rvfc");
          if (next !== state) {
            state = next;
            frames++;
            if (!firstPresented) {
              firstPresented = true;
              emit("first_presented_frame", { media_time_ms: ms(tick), presentation_clock: "VIDEO FRAME", authoritative: true, ready_state: media.read().readyState });
              publish();
            }
            if (request && !request.settled && Math.abs(request.target - tick) <= tolerance()) {
              request.presented = tick;
              emit("seek_presented", { target_ms: ms(request.target), presented_media_time_ms: ms(tick),
                target_error_ms: ms(tick - request.target), request_to_presented_ms: clock.now() - request.requestedAt,
                reason: request.reason, authoritative: true, presentation_clock: "VIDEO FRAME" }, request);
              settle(request, "presented");
              if (waiter === request) waiter = undefined;
              clearTimer("presentation");
              stopNudge();
              if (!active) resumeDesired();
              publish();
            }
            frameSubscribers.forEach((subscriber) => subscriber(tick, "rvfc"));
            if (lastFrameTick !== null && tick > lastFrameTick && desiredPlaying && !active && !waiter && !nudge) {
              lastAdvancingAt = at;
              healthySince ??= at;
              if (at - healthySince >= 10_000) consecutiveFailures = 0;
            } else if (lastFrameTick !== null && tick < lastFrameTick) healthySince = null;
            lastFrameTick = tick;
            if (!request) observeRateFrame(tick, at, rateOwner);
            maybeLoop(tick);
          }
        }
      }
      armFrame();
    });
  }

  function dispatchPending() {
    clearTimer("dispatch");
    if (active || !pending || readiness !== "ready" || !metadataReady) return;
    const delay = lastDispatchAt + 100 - clock.now();
    if (delay > 0) { later("dispatch", delay, dispatchPending); return; }
    const request = pending;
    pending = undefined;
    const current = media.read().tick;
    if (current !== null && Math.abs(current - request.target) <= Math.ceil(frameTicks() / 2)) {
      state = { ...state, dispatched: null };
      emit("seek_deduped", { target_ms: ms(request.target), actual_media_time_ms: media.read().browserSeconds * 1_000, reason: request.reason }, request);
      settle(request, "deduplicated");
      cancelFrames(); armFrame(); resumeDesired(); publish(); return;
    }
    active = request;
    state = requestPreview(dispatchSeek(state, request.target, Math.ceil(tolerance())), request.preview);
    request.epoch = state.nextEpoch;
    request.dispatchedAt = lastDispatchAt = clock.now();
    const distance = ms(request.target - request.from);
    emit("seek_dispatched", { target_ms: ms(request.target), from_ms: ms(request.from), distance_ms: distance,
      direction: distance < 0 ? "backward" : "forward", distance_class: Math.abs(distance) <= 10_000 ? "near" : "far",
      request_to_dispatch_ms: clock.now() - request.requestedAt, reason: request.reason }, request);
    media.pause();
    cancelFrames(); armFrame();
    later("native", 1_500, () => {
      if (active !== request) return;
      emit("seek_timeout", { target_ms: ms(request.target), phase: "native", reason: request.reason }, request);
      settle(request, "failed", "Native seek timed out after 1500 ms");
      recover("Native seek timed out after 1500 ms");
    });
    try { media.seek(request.target); }
    catch (reason) { settle(request, "failed", failureText(reason)); recover("Native seek assignment failed"); }
    publish();
  }

  function seek(target: ReplayTick, settings: SeekOptions = {}): Promise<SeekOutcome> {
    if (!timeline || !isLive() || readiness === "degraded") return Promise.resolve({ status: "unavailable", target });
    if (!Number.isSafeInteger(target)) throw new RangeError("Seek target must be exact replay ticks");
    const reason = settings.reason ?? "navigation";
    if (reason === "clip-loop" && (active || pending || waiter)) return Promise.resolve({ status: "superseded", target });
    const minimum = loop ? frameBoundaryToReplayTick(loop.startFrame, timeline.video.frameRate) : zero;
    const maximum = loop ? frameBoundaryToReplayTick(loop.endFrameExclusive, timeline.video.frameRate) : timeline.video.replayEnd;
    const preview = Math.max(minimum, Math.min(maximum, target)) as ReplayTick;
    const frame = Math.max(loop?.startFrame ?? 0, Math.min((loop?.endFrameExclusive ?? timeline.video.frameCount) - 1,
      replayTickToFrameBoundary(preview, timeline.video.frameRate, "nearest_ties_to_even"))) as FrameBoundary;
    const dispatched = frameBoundaryToReplayTick(frame, timeline.video.frameRate);
    cancelPlay({ status: "superseded", reason: "Newer seek intent" });
    if (settings.playAfter !== undefined) desiredPlaying = settings.playAfter;
    settle(active, "superseded", "Newer seek intent");
    settle(waiter, "superseded", "Newer seek intent");
    if (pending) emit("seek_pending_replaced", { target_ms: ms(pending.target), replacement_target_ms: ms(dispatched) }, pending);
    settle(pending, "superseded", "Newer seek intent");
    stopNudge(); waiter = undefined;
    clearTimer("presentation");
    cancelFrames();
    intent++;
    healthySince = null;
    resetRateObservation();
    state = requestPreview(state, preview);
    const result = new Promise<SeekOutcome>((resolve) => {
      pending = { target: dispatched, preview, reason, actionId: settings.actionId, generation: state.generation,
        intent, epoch: 0, requestedAt: clock.now(), dispatchedAt: 0, from: media.read().tick ?? state.presented ?? zero,
        presented: null, settled: false, resolve };
    });
    emit("seek_requested", { requested_preview_ms: ms(preview), raw_requested_ms: ms(target), from_ms: ms(pending!.from), reason }, pending);
    if (readiness === "recovering") recoveryTarget = preview;
    dispatchPending(); publish(); return result;
  }

  function onSeeked() {
    const request = active;
    const observed = media.read();
    if (!request || observed.seeking || observed.tick === null || Math.abs(observed.tick - request.target) > tolerance()) return;
    state = observeSeeked(state, request.generation, request.epoch, observed.tick);
    clearTimer("native"); active = undefined;
    seekLatencyMs = clock.now() - request.dispatchedAt;
    emit("seeked", { target_ms: ms(request.target), actual_media_time_ms: observed.browserSeconds * 1_000,
      request_to_seeked_ms: clock.now() - request.requestedAt, dispatch_to_seeked_ms: seekLatencyMs, reason: request.reason }, request);
    if (pending) { dispatchPending(); publish(); return; }
    if (request.settled) { resumeDesired(); publish(); return; }
    if (!media.hasVideoFrameCallback) {
      settle(request, "unavailable", "requestVideoFrameCallback is unavailable");
      state = { ...state, dispatched: null };
      resumeDesired(); publish(); return;
    }
    waiter = request;
    later("presentation", 1_500, () => {
      if (waiter !== request) return;
      settle(request, "failed", "First presentation timed out after 1500 ms");
      recover("First presentation timed out after 1500 ms");
    });
    nudge = request;
    try { media.setRate(1 / 16); }
    catch {
      nudge = undefined;
      media.pause();
      emit("nudge_unavailable", { reason: "Native playback rejected the temporary nudge rate" });
      publish(); return;
    }
    const version = ++nudgeToken;
    const generation = state.generation;
    later("nudge", 750, stopNudge);
    try {
      void media.play().catch((reason: unknown) => {
        if (version !== nudgeToken || generation !== state.generation || waiter !== request) return;
        if (reason instanceof Error && reason.name === "NotAllowedError") activationRequired = true;
        stopNudge(); publish();
      });
    } catch { stopNudge(); }
    publish();
  }

  function maybeLoop(tick: ReplayTick) {
    if (!loop || !timeline || !desiredPlaying || active || pending || waiter) return;
    const end = frameBoundaryToReplayTick(loop.endFrameExclusive, timeline.video.frameRate);
    if (tick >= end || media.read().ended) void seek(frameBoundaryToReplayTick(loop.startFrame, timeline.video.frameRate), { reason: "clip-loop" });
  }

  function scheduleMetrics() {
    later("metrics", 1_000, () => {
      quality = Object.freeze(media.quality());
      presentedFps = frames * 1_000 / Math.max(1, clock.now() - metricsAt);
      metricsAt = clock.now(); frames = 0;
      if (!media.hasVideoFrameCallback && metadataReady && !media.read().paused) {
        const tick = media.read().tick;
        if (tick !== null) frameSubscribers.forEach((subscriber) => subscriber(tick, "media-clock-approximate"));
      }
      const observed = media.read();
      if (metadataReady && desiredPlaying && !observed.hidden && !observed.paused && observed.readyState < 3) waitingSince ??= clock.now();
      if (waitingSince !== null && clock.now() - Math.max(waitingSince, lastAdvancingAt) >= 5_000
        && desiredPlaying && !observed.hidden && !active && !pending && !waiter && !nudge) {
        const reason = "Buffering did not recover within 5000 ms";
        if (rate.limitation) { failRate("limited", reason); return; }
        if (rate.selected !== 1 && rate.selected !== lastVerifiedRate) {
          failRate("limited", reason);
          if (readiness !== "ready") return;
          fallbackReloaded = true;
        }
        recover(reason); return;
      }
      watchRate();
      publish(); scheduleMetrics();
    });
  }

  function nativeEvent(event: PlaybackMediaEvent) {
    if (event === "loadstart") {
      loadStarted = true; emit("media_loadstart", { network_state: media.read().networkState }); return;
    }
    if (!loadStarted || !currentSource()) return;
    const observed = media.read();
    if (event === "loadedmetadata" && observed.readyState >= 1 && !metadataReady) {
      metadataReady = true;
      clearTimer("readiness");
      const recovered = readiness === "recovering";
      readiness = "ready"; error = null;
      emit(recovered ? "recovery_ready" : "metadata_ready", { duration_ms: observed.durationSeconds * 1_000,
        ready_state: observed.readyState, video_width: observed.width, video_height: observed.height });
      finishOpen?.({ status: "ready" }); finishOpen = undefined;
      if (pending) dispatchPending();
      else if (recoveryTarget !== null) { const target = recoveryTarget; recoveryTarget = null; void seek(target, { reason: "recovery" }); }
      else resumeDesired();
      armFrame(); publish(); return;
    }
    if (!metadataReady && event !== "error") return;
    switch (event) {
      case "seeked": onSeeked(); break;
      case "seeking": if (active) emit("native_seeking", { target_ms: ms(active.target), reason: active.reason }, active); break;
      case "canplay": case "playing":
        waitingSince = null; emit(event === "canplay" ? "media_canplay" : "native_playing", { ready_state: observed.readyState }); break;
      case "loadeddata": emit("media_loadeddata", { ready_state: observed.readyState }); break;
      case "waiting": case "stalled": waitingSince ??= clock.now(); healthySince = null; resetRateObservation(true); break;
      case "play": emit("native_play", { media_time_ms: observed.browserSeconds * 1_000, rate: observed.rate }); break;
      case "pause": emit("native_pause", { media_time_ms: observed.browserSeconds * 1_000 }); healthySince = null; resetRateObservation(); break;
      case "ended":
        if (loop && desiredPlaying) maybeLoop(observed.tick ?? timeline!.video.replayEnd);
        else desiredPlaying = false;
        break;
      case "error":
        if (observed.error) {
          emit("media_error", { code: observed.error.code, message: observed.error.message });
          recover(`Media error (${observed.error.code}): ${observed.error.message}`);
        }
        break;
      case "timeupdate":
        if (!media.hasVideoFrameCallback && observed.tick !== null) {
          frameSubscribers.forEach((subscriber) => subscriber(observed.tick!, "media-clock-approximate"));
          maybeLoop(observed.tick);
        }
        return;
      case "ratechange": if (!nudge) rate = { ...rate, applied: observed.rate }; break;
      case "visibilitychange": waitingSince = null; resetRateObservation(); break;
    }
    watchRate();
    publish();
  }

  function beginLoad() {
    if (!timeline) return;
    sessionToken = clock.token();
    const url = new URL(originalUrl);
    url.searchParams.set("qb_playback_session", sessionToken);
    sourceUrl = url.href;
    const generation = state.generation;
    for (const event of EVENTS) listeners.push(media.listen(event, () => {
      if (generation === state.generation && isLive() && readiness !== "degraded") nativeEvent(event);
    }));
    emit("media_open");
    later("readiness", 5_000, () => {
      finishOpen?.({ status: "failed", reason: "Metadata readiness timed out after 5000 ms" }); finishOpen = undefined;
      recover("Metadata readiness timed out after 5000 ms");
    });
    scheduleMetrics();
    try {
      media.pause();
      media.open(sourceUrl, timeline);
      applyEffectiveRate(); applyMuted(); applyVolume();
      armFrame();
    } catch (reason) { recover(`Media open failed: ${failureText(reason)}`); }
    publish();
  }

  function recover(reason: string, manual = false): PlaybackOutcome {
    if (!timeline || !isLive()) return { status: "unavailable" };
    if (manual) { consecutiveFailures = 0; recoveryAttempts = []; }
    const now = clock.now();
    recoveryAttempts = recoveryAttempts.filter((attempt) => now - attempt <= 10_000);
    if (recoveryAttempts.length >= 2 || consecutiveFailures >= 2) {
      degrade(reason); return { status: "failed", reason };
    }
    const explicit = pending ?? waiter ?? active;
    recoveryTarget = explicit?.preview ?? state.presented ?? recoveryTarget ?? zero;
    cancelWork("failed", reason);
    media.pause();
    state = beginRecovery(state);
    recoveryAttempts.push(now); consecutiveFailures++; recoveryCount++;
    readiness = "recovering"; error = reason;
    emit("recovery_started", { attempt: consecutiveFailures, target_ms: ms(recoveryTarget), reason });
    beginLoad(); return { status: "ready" };
  }

  return {
    snapshot,
    sampleMetrics() { quality = Object.freeze(media.quality()); return snapshot(); },
    subscribe(callback: (value: PlaybackSnapshot) => void) {
      if (readiness === "disposed") return () => {};
      subscribers.add(callback); callback(snapshot()); return () => { subscribers.delete(callback); };
    },
    subscribeFrames(callback: (tick: ReplayTick, authority: PlaybackSnapshot["authority"]) => void) {
      if (readiness === "disposed") return () => {};
      frameSubscribers.add(callback); return () => { frameSubscribers.delete(callback); };
    },
    open(input: { url: string; mediaId: MediaId; timeline: MediaTimelineV2; muted?: boolean; volume?: number }): Promise<PlaybackOutcome> {
      if (readiness === "disposed") return Promise.resolve({ status: "unavailable" });
      const nextTimeline = input.timeline;
      const nextUrl = new URL(input.url).href;
      const nextMuted = input.muted ?? false;
      const nextVolume = input.volume ?? 1;
      if (input.mediaId !== nextTimeline.mediaId) throw new Error("Playback media identity mismatch");
      if (!Number.isFinite(nextVolume) || nextVolume < 0 || nextVolume > 1) throw new RangeError("Volume must be between 0 and 1");
      cancelWork();
      state = createViewerSeekState(state.generation + 1);
      timeline = nextTimeline; originalUrl = nextUrl;
      desiredPlaying = false; activationRequired = false; error = null; loop = null;
      recoveryCount = consecutiveFailures = 0; recoveryAttempts = []; recoveryTarget = null;
      muted = nextMuted; volume = nextVolume;
      rate = { selected: 1, applied: 1, effective: 1, observed: null, outcome: "pending", limitation: null };
      lastVerifiedRate = null; fallbackReloaded = false;
      readiness = "loading";
      const result = new Promise<PlaybackOutcome>((resolve) => { finishOpen = resolve; });
      beginLoad(); return result;
    },
    play(actionId?: string) {
      if (!isLive()) return Promise.resolve<PlaybackOutcome>({ status: "unavailable" });
      desiredPlaying = true; stopNudge();
      lastAdvancingAt = clock.now();
      return startPlay(actionId);
    },
    pause(): PlaybackOutcome {
      if (!isLive()) return { status: "unavailable" };
      desiredPlaying = false; cancelPlay();
      stopNudge(); media.pause(); publish(); return { status: "paused" };
    },
    seek,
    setRate(selected: PlaybackRate) {
      if (!PLAYBACK_RATES.includes(selected)) throw new RangeError("Unsupported replay speed selection");
      if (!isLive()) return;
      resetRateObservation(); fallbackReloaded = false;
      rate = { selected, applied: rate.applied, effective: selected, observed: null, outcome: "pending", limitation: null };
      if (!nudge) applyEffectiveRate();
      cancelFrames(); armFrame(); watchRate();
      publish();
    },
    setMuted(value: boolean) { if (isLive()) { muted = value; applyMuted(); publish(); } },
    setVolume(value: number) {
      if (!Number.isFinite(value) || value < 0 || value > 1) throw new RangeError("Volume must be between 0 and 1");
      if (isLive()) { volume = value; applyVolume(); publish(); }
    },
    setLoopRange(range: ClipRange | null) {
      if (!timeline || !isLive()) return;
      loop = range ? createClipRange(timeline, range.startFrame, range.endFrameExclusive, range.mediaId) : null;
      resetRateObservation(); watchRate(); publish();
    },
    retry() { return recover("Manual preview retry requested", true); },
    dispose() {
      if (readiness === "disposed") return;
      cancelWork(); readiness = "disposed"; desiredPlaying = false;
      sourceUrl = sessionToken = null; media.release();
      emit("viewer_disposed", { owned_timers: timers.size, owned_listeners: listeners.length });
      publish(); subscribers.clear(); frameSubscribers.clear();
    },
  };
}

export type PlaybackController = ReturnType<typeof createPlaybackController>;
