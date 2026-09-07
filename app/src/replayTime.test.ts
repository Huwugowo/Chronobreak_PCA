import { describe, expect, it } from "vitest";
import golden from "../../fixtures/replay-time/v2/golden.json";
import {
  MAX_REPLAY_TICK,
  REPLAY_TICKS_PER_SECOND,
  browserSecondsForReplayTick,
  checkedScale,
  createClipRange,
  frameBoundaryToReplayTick,
  parseFrameBoundary,
  parseMediaId,
  parseRational,
  parseReplayTick,
  parseSignedReplayTick,
  projectReplayIntervalToClipRange,
  replayTickForBrowserSeconds,
  validateMediaTimeline,
} from "./replayTime";

const timelineWire = () => ({
  schema_version: 2,
  replay_ticks_per_second: "48000000",
  media_id: "11111111-2222-4333-8444-555555555555",
  video: {
    codec: "h264",
    profile: "High",
    time_base: { numerator: "1", denominator: "15360" },
    first_pts: "15360",
    frame_rate: { numerator: "60", denominator: "1" },
    frame_count: "120",
    one_past_last_pts: "46080",
    replay_end: "96000000",
    exact_cfr: true,
  },
  audio: {
    present: false,
    codec: null,
    sample_rate: null,
    time_base: null,
    first_pts: null,
    replay_start: null,
    replay_end: null,
  },
  container: {
    start_seconds: { numerator: "1", denominator: "1" },
    duration_seconds: { numerator: "2", denominator: "1" },
  },
  producer: {
    backend: "native",
    expected_frame_rate: { numerator: "60", denominator: "1" },
    expected_frame_count: "120",
    media_runtime_id: "queueback-r6",
  },
});

describe("replay-time mirror", () => {
  it("consumes the shared decimal, frame, and rounding fixtures", () => {
    expect(golden.schema_version).toBe(2);
    expect(Number(golden.replay_ticks_per_second)).toBe(REPLAY_TICKS_PER_SECOND);
    expect(Number(golden.maximum_replay_tick)).toBe(MAX_REPLAY_TICK);
    for (const value of golden.invalid_unsigned_decimals) {
      expect(() => parseReplayTick(value)).toThrow();
    }
    for (const value of golden.invalid_signed_decimals) {
      expect(() => parseSignedReplayTick(value)).toThrow();
    }
    for (const testCase of golden.rounding_cases) {
      expect(
        checkedScale(
          BigInt(testCase.value),
          BigInt(testCase.numerator),
          BigInt(testCase.denominator),
          testCase.rounding as "exact" | "floor" | "ceil" | "nearest_ties_to_even",
        ),
        testCase.id,
      ).toBe(BigInt(testCase.expected));
    }
    for (const testCase of golden.frame_cases) {
      expect(
        frameBoundaryToReplayTick(
          parseFrameBoundary(testCase.frame_boundary),
          parseRational(testCase.frame_rate),
        ),
        testCase.id,
      ).toBe(Number(testCase.expected_replay_tick));
    }
  });

  it("rejects noncanonical IDs, malformed schema v2, and schema v1", () => {
    expect(() => parseMediaId("aaaaaaaa-2222-4333-8444-555555555555".toUpperCase())).toThrow();
    expect(() => parseMediaId("00000000-0000-0000-0000-000000000000")).toThrow();
    expect(() => validateMediaTimeline({ ...timelineWire(), schema_version: 1 })).toThrow();
    expect(() => validateMediaTimeline({ ...timelineWire(), video: { ...timelineWire().video, exact_cfr: false } })).toThrow();
  });

  it("maps a nonzero browser origin and projects exact half-open frame ranges", () => {
    const timeline = validateMediaTimeline(timelineWire());
    expect(browserSecondsForReplayTick(timeline, 0 as ReturnType<typeof parseReplayTick>)).toBe(1);
    expect(replayTickForBrowserSeconds(timeline, 1.5)).toBe(24_000_000);
    const range = projectReplayIntervalToClipRange(
      timeline,
      parseReplayTick("1"),
      parseReplayTick("800001"),
    );
    expect(range).toMatchObject({ startFrame: 0, endFrameExclusive: 2 });
    expect(createClipRange(timeline, 119 as typeof range.startFrame, 120 as typeof range.endFrameExclusive)).toMatchObject({ endFrameExclusive: 120 });
  });
});
