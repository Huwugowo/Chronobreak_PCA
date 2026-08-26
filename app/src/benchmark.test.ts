import { describe, expect, it } from "vitest";
import {
  BoundedEventBuffer,
  buildScenarioActions,
  deterministicSeekTargets,
  isCurrentMediaGeneration,
  isBenchmarkWindowInteractive,
  isLatestBenchmarkAction,
  percentile,
  ReplayBenchmarkObserver,
  sanitizePayload,
  shouldRecordBenchmarkEvent,
  type BenchmarkScenario,
  type BenchmarkSessionInfo,
} from "./benchmark";

const scenario = (overrides: Partial<BenchmarkScenario>): BenchmarkScenario => ({
  id: "scenario-1",
  trial_id: "trial-1",
  kind: "seek",
  fixture_ids: ["short-h264"],
  ...overrides,
});

describe("BoundedEventBuffer", () => {
  it("preserves FIFO order and accounts for overflow", () => {
    const buffer = new BoundedEventBuffer<number>(2);
    expect(buffer.push(1)).toBe(true);
    expect(buffer.push(2)).toBe(true);
    expect(buffer.push(3)).toBe(false);
    expect(buffer.stats()).toEqual({
      capacity: 2,
      size: 2,
      highWaterMark: 2,
      dropped: 1,
    });
    expect(buffer.drain(1)).toEqual([1]);
    expect(buffer.drain(2)).toEqual([2]);
  });

  it("requeues failed batches ahead of newer records without exceeding capacity", () => {
    const buffer = new BoundedEventBuffer<number>(3);
    buffer.push(3);
    buffer.prepend([1, 2]);
    expect(buffer.drain(3)).toEqual([1, 2, 3]);
    buffer.push(5);
    buffer.prepend([1, 2, 3]);
    expect(buffer.drain(3)).toEqual([2, 3, 5]);
    expect(buffer.stats().dropped).toBe(1);
  });
});

describe("benchmark statistics", () => {
  it("uses nearest-rank percentiles and ignores non-finite values", () => {
    expect(percentile([1, 2, 3, 4, Number.NaN], 95)).toBe(4);
    expect(percentile([4, 1, 3, 2], 50)).toBe(2);
    expect(percentile([], 50)).toBeNull();
  });
});

describe("observer reduction", () => {
  it("keeps action and media identities unique across viewer remounts", () => {
    const session: BenchmarkSessionInfo = {
      schema_version: 1,
      run_id: "run-1",
      observer_profile: "full",
      harness_initialization_ms: 0,
      session_elapsed_ms: 0,
      fixtures: [{ id: "fixture-1", alias: "fixture-1", game_timestamp: "1" }],
      scenarios: [scenario({})],
      ddragon_mode: "offline",
      ddragon_cache_fingerprint: "fixture",
      event_queue: {
        capacity: 256,
        high_water_mark: 0,
        accepted_records: 0,
        dropped_records: 0,
      },
    };
    const observer = new ReplayBenchmarkObserver(session, 0);

    expect(observer.nextActionId("play")).toBe("play-1");
    expect(observer.nextActionId("seek")).toBe("seek-2");
    expect(observer.nextMediaGeneration()).toBe(0);
    expect(observer.nextMediaGeneration()).toBe(1);
  });

  it("keeps reconciliation-critical outcomes in the minimal profile", () => {
    for (const kind of [
      "seek_pending_replaced",
      "seek_deduped",
      "action_cancelled_superseded",
      "rate_observed",
      "play_complete",
      "pause_complete",
      "scenario_completed",
    ]) {
      expect(shouldRecordBenchmarkEvent("minimal", kind)).toBe(true);
    }
    expect(shouldRecordBenchmarkEvent("minimal", "native_seeking")).toBe(false);
    expect(shouldRecordBenchmarkEvent("minimal", "native_seeking", true)).toBe(true);
    expect(shouldRecordBenchmarkEvent("full", "native_seeking")).toBe(true);
  });

  it("rejects stale presented-frame generations", () => {
    expect(isCurrentMediaGeneration(7, 7)).toBe(true);
    expect(isCurrentMediaGeneration(6, 7)).toBe(false);
    expect(isCurrentMediaGeneration(undefined, 7)).toBe(false);
  });

  it("credits only the latest non-cancelled benchmark action", () => {
    expect(isLatestBenchmarkAction("seek-2", "seek-2", false)).toBe(true);
    expect(isLatestBenchmarkAction("seek-1", "seek-2", false)).toBe(false);
    expect(isLatestBenchmarkAction("seek-2", "seek-2", true)).toBe(false);
    expect(isLatestBenchmarkAction(undefined, "seek-2", false)).toBe(false);
  });

  it("requires a visible focused benchmark window", () => {
    expect(isBenchmarkWindowInteractive("visible", true)).toBe(true);
    expect(isBenchmarkWindowInteractive("visible", false)).toBe(false);
    expect(isBenchmarkWindowInteractive("hidden", true)).toBe(false);
  });
});

describe("scenario scripts", () => {
  it("keeps open scenarios alive through a real stable playback window", () => {
    const actions = buildScenarioActions(
      scenario({ kind: "cold_open", warmup_seconds: 2, stable_playback_ms: 750 }),
      120_000,
    );
    expect(actions).toEqual([
      { kind: "play" },
      { kind: "wait", durationMs: 2_000 },
      { kind: "stable-playback", durationMs: 750 },
      { kind: "pause" },
    ]);
  });

  it("paces deterministic seeks across the declared observer window", () => {
    const actions = buildScenarioActions(
      scenario({
        kind: "seek",
        warmup_seconds: 5,
        duration_seconds: 60,
        target_times_ms: [10_000, 30_000, 90_000],
      }),
      1_800_000,
    );
    expect(actions[0]).toEqual({ kind: "play" });
    expect(actions[1]).toEqual({ kind: "wait", durationMs: 5_000 });
    expect(actions.filter((action) => action.kind === "seek")).toHaveLength(3);
    expect(
      actions
        .filter((action) => action.kind === "wait")
        .reduce((total, action) => total + action.durationMs, 0),
    ).toBe(65_000);
    expect(actions[actions.length - 1]).toEqual({ kind: "pause" });
  });

  it("can warm playback before a paused deterministic seek control", () => {
    const actions = buildScenarioActions(
      scenario({
        kind: "seek",
        warmup_seconds: 5,
        duration_seconds: 60,
        seek_playback_mode: "paused",
        target_times_ms: [10_000, 30_000, 90_000],
      }),
      1_800_000,
    );
    expect(actions.slice(0, 3)).toEqual([
      { kind: "play" },
      { kind: "wait", durationMs: 5_000 },
      { kind: "pause" },
    ]);
    expect(actions.filter((action) => action.kind === "seek")).toHaveLength(3);
    expect(actions.filter((action) => action.kind === "play")).toHaveLength(4);
    expect(actions.filter((action) => action.kind === "pause")).toHaveLength(4);
    expect(actions.slice(3, 7)).toEqual([
      { kind: "play" },
      { kind: "seek", targetMs: 10_000, reason: "benchmark" },
      { kind: "pause" },
      { kind: "wait", durationMs: 20_000 },
    ]);
  });

  it("produces stable bounded seek targets for a fixed seed", () => {
    const first = deterministicSeekTargets(1_800_000, 42, { count: 160 });
    const second = deterministicSeekTargets(1_800_000, 42, { count: 160 });
    expect(first).toEqual(second);
    expect(first).toHaveLength(160);
    expect(first.every((target) => target >= 1_000 && target <= 1_799_000)).toBe(true);
    expect(new Set(first).size).toBeGreaterThan(100);
    const strata = { nearForward: 0, farForward: 0, nearBackward: 0, farBackward: 0 };
    let previous = 0;
    for (const target of first) {
      const distance = target - previous;
      const near = Math.abs(distance) <= 10_000;
      if (near && distance >= 0) strata.nearForward += 1;
      if (!near && distance >= 0) strata.farForward += 1;
      if (near && distance < 0) strata.nearBackward += 1;
      if (!near && distance < 0) strata.farBackward += 1;
      previous = target;
    }
    expect(strata).toEqual({
      nearForward: 40,
      farForward: 40,
      nearBackward: 40,
      farBackward: 40,
    });
  });

  it("covers the complete native playback-rate ladder", () => {
    const actions = buildScenarioActions(
      scenario({ kind: "rate", rates: [0.25, 0.5, 1, 2, 4, 8], duration_seconds: 1 }),
      120_000,
    );
    const rates = actions
      .filter((action) => action.kind === "rate")
      .map((action) => action.rate);
    expect(new Set(rates)).toEqual(new Set([0.25, 0.5, 1, 2, 4, 8]));
  });

  it("builds deterministic scrub bursts with a bounded request count", () => {
    const actions = buildScenarioActions(
      scenario({
        kind: "scrub",
        seed: 7,
        count: 40,
        request_rate_hz: 60,
        warmup_seconds: 1,
      }),
      600_000,
    );
    const requests = actions
      .filter((action) => action.kind === "scrub")
      .reduce((total, action) => total + action.targetsMs.length, 0);
    expect(requests).toBe(40);
    expect(
      actions
        .filter((action) => action.kind === "scrub")
        .every((action) => action.intervalMs === 17),
    ).toBe(true);
    expect(actions.slice(0, 2)).toEqual([
      { kind: "play" },
      { kind: "wait", durationMs: 1_000 },
    ]);
    expect(actions[actions.length - 1]).toEqual({ kind: "pause" });
  });

  it("covers playing, paused, seeking, and clip-mode layout states", () => {
    const actions = buildScenarioActions(
      scenario({ kind: "layout", cycles: 1, seed: 11 }),
      600_000,
    );
    expect(actions.some((action) => action.kind === "play")).toBe(true);
    expect(actions.some((action) => action.kind === "pause")).toBe(true);
    const seekingLayoutIndex = actions.findIndex(
      (action) => action.kind === "fullscreen" && action.seekTargetMs !== undefined,
    );
    expect(seekingLayoutIndex).toBeGreaterThan(0);
    expect(actions[seekingLayoutIndex - 1]).toEqual({ kind: "play" });
    expect(actions[seekingLayoutIndex + 2]).toEqual({ kind: "pause" });
    expect(actions).toContainEqual(expect.objectContaining({ kind: "clip-mode", enabled: true }));
    expect(actions).toContainEqual(expect.objectContaining({ kind: "clip-mode", enabled: false }));
  });
});

describe("payload sanitization", () => {
  it("redacts paths and local player identities recursively", () => {
    expect(
      sanitizePayload({
        output_path: "C:\\Users\\Example\\recording.mp4",
        nested: { summoner_name: "Player#TAG", safe: "h264" },
      }),
    ).toEqual({
      output_path: "<benchmark-path>",
      nested: { summoner_name: "<identity>", safe: "h264" },
    });
  });
});
