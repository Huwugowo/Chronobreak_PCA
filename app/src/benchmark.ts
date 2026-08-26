import { invoke, isTauri } from "@tauri-apps/api/core";

export type ObserverProfile = "minimal" | "full";

export type BenchmarkFixture = {
  id: string;
  alias: string;
  game_timestamp: string;
  [key: string]: unknown;
};

export type BenchmarkScenario = {
  id: string;
  trial_id: string;
  kind: string;
  fixture_alias?: string;
  fixture_ids?: string[];
  game_timestamp?: string;
  seed?: number;
  parameters?: Record<string, unknown>;
  [key: string]: unknown;
};

export type QueueStats = {
  capacity: number;
  high_water_mark: number;
  accepted_records: number;
  dropped_records: number;
};

export type BenchmarkSessionInfo = {
  schema_version: 1;
  run_id: string;
  observer_profile: ObserverProfile;
  harness_initialization_ms: number;
  session_elapsed_ms: number;
  fixtures: BenchmarkFixture[];
  scenarios: BenchmarkScenario[];
  ddragon_mode: string;
  ddragon_cache_fingerprint: string;
  event_queue: QueueStats;
};

export type BenchmarkEventInput = {
  scenario_id: string;
  trial_id: string;
  monotonic_ms: number;
  source: "frontend";
  kind: string;
  generation?: number;
  action_id?: string;
  payload: Record<string, unknown>;
};

export type RequestTelemetrySnapshot = {
  capacity: number;
  high_water_mark: number;
  overwritten_records: number;
  active_streams: number;
  peak_active_streams: number;
  pending_records: number;
  requests: unknown[];
};

export type BenchmarkEventMeta = {
  generation?: number;
  actionId?: string;
  required?: boolean;
  monotonicMs?: number;
};

export type LocalBufferStats = {
  capacity: number;
  size: number;
  highWaterMark: number;
  dropped: number;
};

const LOCAL_EVENT_CAPACITY = 1024;
const FLUSH_BATCH_SIZE = 128;
const SERVER_FLUSH_INTERVAL_MS = 1_000;
export const REPLAY_BENCHMARK_VIEWER_CYCLE_EVENT = "chronobreak:replay-benchmark-viewer-cycle";

const MINIMAL_EVENT_KINDS = new Set([
  "frontend_initialized",
  "library_requested",
  "library_useful",
  "replay_requested",
  "playback_payload_ready",
  "viewer_mounted",
  "metadata_ready",
  "first_presented_frame",
  "steady_playback",
  "seek_requested",
  "seek_dispatched",
  "seek_pending_replaced",
  "seek_deduped",
  "seeked",
  "seek_presented",
  "action_cancelled_superseded",
  "action_failed",
  "seek_timeout",
  "rate_requested",
  "rate_applied",
  "rate_observed",
  "play_requested",
  "play_complete",
  "pause_requested",
  "pause_complete",
  "export_requested",
  "export_completed",
  "media_error",
  "recovery_started",
  "recovery_ready",
  "recovery_succeeded",
  "recovery_exhausted",
  "preview_degraded",
  "scenario_started",
  "scenario_completed",
  "scenario_failed",
  "observer_reconciliation",
]);

export const shouldRecordBenchmarkEvent = (
  profile: ObserverProfile,
  kind: string,
  required = false,
): boolean => required || profile === "full" || MINIMAL_EVENT_KINDS.has(kind);

export const isCurrentMediaGeneration = (
  eventGeneration: number | undefined,
  currentGeneration: number,
): boolean => eventGeneration === currentGeneration;

export const isLatestBenchmarkAction = (
  actionId: string | undefined,
  latestActionId: string | undefined,
  cancelled: boolean,
): boolean => Boolean(actionId) && !cancelled && actionId === latestActionId;

export const isBenchmarkWindowInteractive = (
  visibilityState: string,
  hasFocus: boolean,
): boolean => visibilityState === "visible" && hasFocus;

export class BoundedEventBuffer<T> {
  readonly capacity: number;
  private values: T[] = [];
  private highWaterMark = 0;
  private dropped = 0;

  constructor(capacity: number) {
    if (!Number.isSafeInteger(capacity) || capacity <= 0) {
      throw new Error("event buffer capacity must be a positive safe integer");
    }
    this.capacity = capacity;
  }

  push(value: T): boolean {
    if (this.values.length >= this.capacity) {
      this.dropped += 1;
      return false;
    }
    this.values.push(value);
    this.highWaterMark = Math.max(this.highWaterMark, this.values.length);
    return true;
  }

  prepend(values: readonly T[]): void {
    if (values.length === 0) return;
    const available = Math.max(0, this.capacity - this.values.length);
    const accepted = values.slice(Math.max(0, values.length - available));
    this.dropped += values.length - accepted.length;
    this.values = [...accepted, ...this.values];
    this.highWaterMark = Math.max(this.highWaterMark, this.values.length);
  }

  drain(maximum: number): T[] {
    if (!Number.isSafeInteger(maximum) || maximum <= 0) return [];
    return this.values.splice(0, maximum);
  }

  get size(): number {
    return this.values.length;
  }

  stats(): LocalBufferStats {
    return {
      capacity: this.capacity,
      size: this.values.length,
      highWaterMark: this.highWaterMark,
      dropped: this.dropped,
    };
  }
}

export class ReplayBenchmarkObserver {
  readonly session: BenchmarkSessionInfo;
  private readonly buffer = new BoundedEventBuffer<BenchmarkEventInput>(LOCAL_EVENT_CAPACITY);
  private readonly monotonicOffsetMs: number;
  private flushChain: Promise<void> = Promise.resolve();
  private periodicFlushChain: Promise<void> = Promise.resolve();
  private serverTimer: number | undefined;
  private stopped = false;
  private remoteDropped = 0;
  private lastError: string | null = null;
  private scenarioActive = false;
  private actionSequence = 0;
  private mediaGeneration = -1;
  private readonly windowStateEvents = new Set<string>();

  private readonly handleWindowBlur = (): void => {
    this.recordWindowStateEvent(
      "benchmark_window_focus_lost",
      "benchmark window lost focus during the measured scenario",
      false,
    );
  };

  private readonly handleVisibilityChange = (): void => {
    if (document.visibilityState === "visible") return;
    this.recordWindowStateEvent(
      "benchmark_document_hidden",
      `benchmark document became ${document.visibilityState} during the measured scenario`,
      true,
    );
  };

  constructor(session: BenchmarkSessionInfo, receivedAtMs = performance.now()) {
    this.session = session;
    this.monotonicOffsetMs = session.session_elapsed_ms - receivedAtMs;
  }

  scenario(): BenchmarkScenario {
    const scenario = this.session.scenarios[0];
    if (!scenario) throw new Error("benchmark session has no scenario");
    return scenario;
  }

  fixtureForScenario(cycleIndex = 0): BenchmarkFixture | undefined {
    const scenario = this.scenario();
    if (scenario.game_timestamp) {
      return this.session.fixtures.find(
        (fixture) => fixture.game_timestamp === scenario.game_timestamp,
      );
    }
    if (scenario.fixture_alias) {
      return this.session.fixtures.find((fixture) => fixture.alias === scenario.fixture_alias);
    }
    const fixtureIds = scenario.fixture_ids ?? [];
    const fixtureId = fixtureIds.length > 0 ? fixtureIds[cycleIndex % fixtureIds.length] : undefined;
    return this.session.fixtures.find((fixture) => fixture.id === fixtureId);
  }

  now(): number {
    return performance.now() + this.monotonicOffsetMs;
  }

  nextActionId(kind: "play" | "pause" | "rate" | "seek" | "export"): string {
    this.actionSequence += 1;
    return `${kind}-${this.actionSequence}`;
  }

  nextMediaGeneration(): number {
    this.mediaGeneration += 1;
    return this.mediaGeneration;
  }

  private recordWindowStateEvent(kind: string, reason: string, invalidating: boolean): void {
    if (!this.scenarioActive || this.stopped || this.windowStateEvents.has(kind)) return;
    this.windowStateEvents.add(kind);
    if (invalidating) this.lastError ??= reason;
    this.emit(
      kind,
      {
        reason,
        visibility_state:
          typeof document === "undefined" ? "unavailable" : document.visibilityState,
      },
      { required: true },
    );
  }

  emit(
    kind: string,
    payload: Record<string, unknown> = {},
    meta: BenchmarkEventMeta = {},
  ): boolean {
    if (this.stopped) return false;
    if (!shouldRecordBenchmarkEvent(this.session.observer_profile, kind, meta.required)) {
      return true;
    }
    const scenario = this.scenario();
    const accepted = this.buffer.push({
      scenario_id: scenario.id,
      trial_id: scenario.trial_id,
      monotonic_ms: meta.monotonicMs ?? this.now(),
      source: "frontend",
      kind,
      generation: meta.generation,
      action_id: meta.actionId,
      payload: sanitizePayload(payload),
    });
    if (kind === "scenario_started") {
      this.scenarioActive = true;
      if (
        typeof window !== "undefined" &&
        typeof document !== "undefined" &&
        document.visibilityState !== "visible"
      ) {
        this.recordWindowStateEvent(
          "benchmark_window_not_interactive",
          "benchmark document was not visible when the measured scenario started",
          true,
        );
      } else if (
        typeof window !== "undefined" &&
        typeof document !== "undefined" &&
        !isBenchmarkWindowInteractive(document.visibilityState, document.hasFocus())
      ) {
        this.recordWindowStateEvent(
          "benchmark_window_focus_lost",
          "benchmark window was not focused when the measured scenario started",
          false,
        );
      }
    } else if (kind === "scenario_completed" || kind === "scenario_failed") {
      this.scenarioActive = false;
    }
    if (this.buffer.size >= FLUSH_BATCH_SIZE) void this.flush().catch(() => undefined);
    return accepted;
  }

  start(): void {
    this.emit(
      "frontend_initialized",
      {
        observer_profile: this.session.observer_profile,
        harness_initialization_ms: this.session.harness_initialization_ms,
      },
      { required: true },
    );
    if (typeof window !== "undefined") {
      window.addEventListener("blur", this.handleWindowBlur);
      document.addEventListener("visibilitychange", this.handleVisibilityChange);
      this.serverTimer = window.setInterval(
        () => {
          this.periodicFlushChain = this.periodicFlushChain
            .catch(() => undefined)
            .then(async () => {
              await this.flush();
              await this.flushServerRequests();
            })
            .catch((error) => {
              this.lastError = error instanceof Error ? error.message : String(error);
            });
        },
        SERVER_FLUSH_INTERVAL_MS,
      );
    }
  }

  flush(): Promise<void> {
    this.flushChain = this.flushChain.catch(() => undefined).then(async () => {
      while (this.buffer.size > 0) {
        const events = this.buffer.drain(FLUSH_BATCH_SIZE);
        try {
          const result = await invoke<{ accepted: boolean; queue: QueueStats }>(
            "record_replay_benchmark_events",
            { events },
          );
          if (!result.accepted) this.remoteDropped += events.length;
        } catch (error) {
          this.buffer.prepend(events);
          this.lastError = error instanceof Error ? error.message : String(error);
          throw error;
        }
      }
    });
    return this.flushChain;
  }

  async flushServerRequests(): Promise<RequestTelemetrySnapshot> {
    const snapshot = await invoke<RequestTelemetrySnapshot>(
      "flush_replay_benchmark_server_requests",
    );
    if (snapshot.overwritten_records > 0) {
      this.lastError = `server telemetry overwrote ${snapshot.overwritten_records} records`;
    }
    return snapshot;
  }

  async complete(
    status: "complete" | "failed" | "invalid",
    reason: string | null = null,
    payload: Record<string, unknown> = {},
  ): Promise<void> {
    if (this.stopped) return;
    if (this.serverTimer !== undefined) window.clearInterval(this.serverTimer);
    await this.periodicFlushChain;
    try {
      const deadline = performance.now() + 2_000;
      let snapshot: RequestTelemetrySnapshot;
      do {
        snapshot = await this.flushServerRequests();
        if (snapshot.active_streams === 0 && snapshot.pending_records === 0) break;
        await new Promise<void>((resolve) => window.setTimeout(resolve, 50));
      } while (performance.now() < deadline);
      if (snapshot.active_streams !== 0 || snapshot.pending_records !== 0) {
        this.lastError =
          `server telemetry did not quiesce: active=${snapshot.active_streams}, ` +
          `pending=${snapshot.pending_records}`;
      }
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    }
    try {
      await this.flush();
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    }
    let remoteQueue: QueueStats | null = null;
    try {
      remoteQueue = await invoke<QueueStats>("get_replay_benchmark_queue_stats");
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    }
    const localBeforeReconciliation = this.buffer.stats();
    const reconciliationAccepted = this.emit(
      "observer_reconciliation",
      {
        local_capacity: localBeforeReconciliation.capacity,
        local_high_water_mark: localBeforeReconciliation.highWaterMark,
        local_dropped: localBeforeReconciliation.dropped,
        remote_dropped: remoteQueue?.dropped_records ?? this.remoteDropped,
        remote_high_water_mark: remoteQueue?.high_water_mark ?? null,
        remote_capacity: remoteQueue?.capacity ?? null,
        last_error: this.lastError,
      },
      { required: true },
    );
    if (!reconciliationAccepted) {
      this.lastError = "frontend observer could not retain terminal reconciliation";
    }
    try {
      await this.flush();
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    }
    try {
      remoteQueue = await invoke<QueueStats>("get_replay_benchmark_queue_stats");
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error);
    }
    const remoteDropped = Math.max(remoteQueue?.dropped_records ?? 0, this.remoteDropped);
    const finalLocal = this.buffer.stats();
    const effectiveStatus =
      status === "complete" && (finalLocal.dropped > 0 || remoteDropped > 0 || this.lastError)
        ? "invalid"
        : status;
    const effectiveReason =
      effectiveStatus === "invalid" && !reason
        ? this.lastError ?? "benchmark observer lost required telemetry"
        : reason;
    this.stopped = true;
    const scenario = this.scenario();
    await invoke("complete_replay_benchmark", {
      terminal: {
        scenario_id: scenario.id,
        trial_id: scenario.trial_id,
        status: effectiveStatus,
        reason: effectiveReason,
        payload: sanitizePayload(payload),
      },
    });
  }
}

let initialization: Promise<ReplayBenchmarkObserver | null> | undefined;
let activeObserver: ReplayBenchmarkObserver | null = null;

export const initializeReplayBenchmark = (): Promise<ReplayBenchmarkObserver | null> => {
  if (initialization) return initialization;
  initialization = (async () => {
    if (!isTauri()) return null;
    const requestedAt = performance.now();
    const session = await invoke<BenchmarkSessionInfo | null>("get_replay_benchmark_session");
    if (!session) return null;
    validateSession(session);
    const receivedAt = performance.now();
    activeObserver = new ReplayBenchmarkObserver(session, (requestedAt + receivedAt) / 2);
    activeObserver.emit(
      "library_requested",
      { captured_before_app_render: true },
      {
        required: true,
        monotonicMs:
          requestedAt + session.session_elapsed_ms - (requestedAt + receivedAt) / 2,
      },
    );
    activeObserver.start();
    return activeObserver;
  })();
  return initialization;
};

export const replayBenchmarkObserver = (): ReplayBenchmarkObserver | null => activeObserver;

export const emitReplayBenchmarkEvent = (
  kind: string,
  payload: Record<string, unknown> = {},
  meta: BenchmarkEventMeta = {},
): boolean => activeObserver?.emit(kind, payload, meta) ?? true;

export const sanitizePayload = (
  payload: Record<string, unknown>,
): Record<string, unknown> => sanitizeValue(payload) as Record<string, unknown>;

const sanitizeValue = (value: unknown, key = ""): unknown => {
  if (Array.isArray(value)) return value.map((item) => sanitizeValue(item, key));
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([childKey, childValue]) => [
        childKey,
        sanitizeValue(childValue, childKey),
      ]),
    );
  }
  if (typeof value !== "string") return value;
  const lowered = key.toLowerCase();
  if (lowered.includes("path") || lowered.includes("directory")) return "<benchmark-path>";
  if (lowered.includes("summoner") || lowered.includes("player_name")) return "<identity>";
  return value;
};

export const percentile = (values: readonly number[], percentileValue: number): number | null => {
  const valid = values.filter(Number.isFinite).sort((left, right) => left - right);
  if (valid.length === 0) return null;
  const bounded = Math.min(100, Math.max(0, percentileValue));
  const rank = Math.ceil((bounded / 100) * valid.length) - 1;
  return valid[Math.max(0, rank)];
};

export type ScenarioAction =
  | { kind: "wait"; durationMs: number }
  | { kind: "play" }
  | { kind: "pause" }
  | { kind: "stable-playback"; durationMs: number }
  | { kind: "rate"; rate: number; durationMs: number }
  | { kind: "seek"; targetMs: number; reason: string }
  | { kind: "scrub"; targetsMs: number[]; intervalMs: number }
  | { kind: "fullscreen"; enabled: boolean; seekTargetMs?: number }
  | { kind: "clip-mode"; enabled: boolean; anchorMs?: number };

export const buildScenarioActions = (
  scenario: BenchmarkScenario,
  durationMs: number,
): ScenarioAction[] => {
  const parameters = { ...scenario, ...(scenario.parameters ?? {}) };
  const warmupMs =
    typeof parameters.warmup_seconds === "number"
      ? boundedNumber(parameters.warmup_seconds * 1_000, 5_000, 0, 3_600_000)
      : boundedNumber(parameters.warmup_ms, 5_000, 0, 3_600_000);
  const measuredMs =
    typeof parameters.duration_seconds === "number"
      ? boundedNumber(parameters.duration_seconds * 1_000, 30_000, 100, 86_400_000)
      : boundedNumber(parameters.measured_ms, 30_000, 100, 86_400_000);
  const stablePlaybackMs = boundedNumber(parameters.stable_playback_ms, 1_000, 500, 10_000);
  switch (scenario.kind) {
    case "replay_open":
    case "cold_open":
    case "warm_open":
      return [
        { kind: "play" },
        { kind: "wait", durationMs: warmupMs },
        { kind: "stable-playback", durationMs: stablePlaybackMs },
        { kind: "pause" },
      ];
    case "play_pause":
      return [
        { kind: "play" },
        { kind: "wait", durationMs: warmupMs + measuredMs },
        { kind: "pause" },
        { kind: "wait", durationMs: 500 },
        { kind: "play" },
        { kind: "wait", durationMs: 1_000 },
        { kind: "pause" },
      ];
    case "rate_ladder":
    case "rate": {
      const rates = Array.isArray(parameters.rates)
        ? parameters.rates.filter(
            (rate): rate is number => typeof rate === "number" && [0.25, 0.5, 1, 2, 4, 8].includes(rate),
          )
        : [0.25, 0.5, 1, 2, 4, 8];
      return [
        { kind: "play" },
        { kind: "wait", durationMs: warmupMs },
        ...rates.map((rate) => ({ kind: "rate" as const, rate, durationMs: measuredMs })),
        ...Array.from({ length: 5 }, (_, index) => [
          { kind: "rate" as const, rate: index % 2 === 0 ? 0.25 : 8, durationMs: 250 },
          { kind: "rate" as const, rate: index % 2 === 0 ? 8 : 0.25, durationMs: 250 },
        ]).flat(),
        { kind: "pause" },
      ];
    }
    case "seek": {
      const pausedSeekControl = parameters.seek_playback_mode === "paused";
      const targets = (
        Array.isArray(parameters.target_times_ms)
          ? parameters.target_times_ms.filter(
              (target): target is number =>
                typeof target === "number" && Number.isFinite(target) && target >= 0,
            )
          : deterministicSeekTargets(durationMs, scenario.seed ?? 1, parameters)
      );
      const reason =
        parameters.seek_reason === "endpoint-edit" || parameters.seek_reason === "event-jump"
          ? parameters.seek_reason
          : "benchmark";
      const pacingMs = Math.max(1, Math.floor(measuredMs / Math.max(1, targets.length)));
      const seekActions = targets.flatMap((targetMs) =>
        pausedSeekControl
          ? [
              { kind: "play" as const },
              { kind: "seek" as const, targetMs, reason },
              { kind: "pause" as const },
              { kind: "wait" as const, durationMs: pacingMs },
            ]
          : [
              { kind: "seek" as const, targetMs, reason },
              { kind: "wait" as const, durationMs: pacingMs },
            ],
      );
      return [
        { kind: "play" },
        { kind: "wait", durationMs: warmupMs },
        ...(pausedSeekControl ? [{ kind: "pause" as const }] : []),
        ...seekActions,
        ...(pausedSeekControl ? [] : [{ kind: "pause" as const }]),
      ];
    }
    case "scrub": {
      const targets = deterministicSeekTargets(durationMs, scenario.seed ?? 1, {
        ...parameters,
        count: boundedNumber(parameters.count, 40, 4, 200),
      });
      const burstSize = boundedNumber(parameters.burst_size, 8, 2, 50);
      const requestRateHz = boundedNumber(parameters.request_rate_hz, 60, 1, 1_000);
      const intervalMs = Math.max(1, Math.round(1_000 / requestRateHz));
      const actions: ScenarioAction[] = [
        { kind: "play" },
        { kind: "wait", durationMs: warmupMs },
      ];
      for (let index = 0; index < targets.length; index += burstSize) {
        actions.push({
          kind: "scrub",
          targetsMs: targets.slice(index, index + burstSize),
          intervalMs,
        });
      }
      actions.push({ kind: "pause" });
      return actions;
    }
    case "fullscreen":
    case "layout": {
      const cycles = boundedNumber(parameters.cycles, 5, 1, 50);
      const targets = deterministicSeekTargets(durationMs, scenario.seed ?? 1, {
        count: Math.max(4, cycles * 4),
      });
      return Array.from({ length: cycles }, (_, index) => {
        const targetMs = targets[index % targets.length];
        return [
          { kind: "play" as const },
          { kind: "wait" as const, durationMs: 250 },
          { kind: "fullscreen" as const, enabled: true },
          { kind: "fullscreen" as const, enabled: false },
          { kind: "pause" as const },
          { kind: "fullscreen" as const, enabled: true },
          { kind: "fullscreen" as const, enabled: false },
          { kind: "play" as const },
          { kind: "fullscreen" as const, enabled: true, seekTargetMs: targetMs },
          { kind: "fullscreen" as const, enabled: false },
          { kind: "pause" as const },
          { kind: "clip-mode" as const, enabled: true, anchorMs: targetMs },
          { kind: "fullscreen" as const, enabled: true },
          { kind: "fullscreen" as const, enabled: false },
          { kind: "clip-mode" as const, enabled: false },
        ];
      }).flat();
    }
    case "lifecycle": {
      if (typeof scenario.duration_seconds !== "number") return [];
      const targets = deterministicSeekTargets(durationMs, scenario.seed ?? 1, {
        count: Math.max(4, Math.ceil(measuredMs / 10_000)),
      });
      const actions: ScenarioAction[] = [{ kind: "play" }, { kind: "wait", durationMs: warmupMs }];
      let remainingMs = measuredMs;
      for (const [index, targetMs] of targets.entries()) {
        if (remainingMs <= 0) break;
        actions.push({ kind: "seek", targetMs, reason: "benchmark" });
        const windowMs = Math.min(10_000, remainingMs);
        actions.push({ kind: "rate", rate: index % 2 === 0 ? 0.5 : 2, durationMs: windowMs });
        if (index % 6 === 0) {
          actions.push({ kind: "fullscreen", enabled: true });
          actions.push({ kind: "fullscreen", enabled: false });
        }
        remainingMs -= windowMs;
      }
      actions.push({ kind: "pause" });
      return actions;
    }
    default:
      throw new Error(`unsupported replay benchmark scenario kind: ${scenario.kind}`);
  }
};

export const deterministicSeekTargets = (
  durationMs: number,
  seed: number,
  parameters: Record<string, unknown> = {},
): number[] => {
  const count = boundedNumber(parameters.count, 160, 4, 2_000);
  const endpointGuardMs = Math.min(1_000, Math.max(1, durationMs * 0.02));
  const maximumTargetMs = Math.max(endpointGuardMs, durationMs - endpointGuardMs);
  let state = (seed >>> 0) || 1;
  const random = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return (state >>> 0) / 0x1_0000_0000;
  };
  const clampTarget = (targetMs: number) =>
    Math.round(Math.min(maximumTargetMs, Math.max(endpointGuardMs, targetMs)));
  const targets: number[] = [];
  let lowAnchorMs = 0;
  while (targets.length < count) {
    const nearDeltaMs = 2_000 + random() * 5_000;
    const nearForwardMs = clampTarget(lowAnchorMs + nearDeltaMs);
    const highAnchorMs = clampTarget(durationMs * (0.72 + random() * 0.16));
    const nearBackwardMs = clampTarget(highAnchorMs - nearDeltaMs);
    const nextLowAnchorMs = clampTarget(2_000 + random() * 5_000);
    targets.push(nearForwardMs, highAnchorMs, nearBackwardMs, nextLowAnchorMs);
    lowAnchorMs = nextLowAnchorMs;
  }
  return targets.slice(0, count);
};

const boundedNumber = (
  value: unknown,
  fallback: number,
  minimum: number,
  maximum: number,
): number =>
  typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, Math.round(value)))
    : fallback;

const validateSession = (session: BenchmarkSessionInfo): void => {
  if (session.schema_version !== 1) throw new Error("unsupported replay benchmark session schema");
  if (!session.run_id || session.scenarios.length !== 1 || session.fixtures.length === 0) {
    throw new Error("replay benchmark session must contain one scenario and at least one fixture");
  }
  const scenario = session.scenarios[0];
  if (!scenario.id || !scenario.trial_id || !scenario.kind) {
    throw new Error("replay benchmark scenario identity is incomplete");
  }
};
