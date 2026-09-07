/** Exact frontend mirror of the schema-v2 replay-time contract. */

export const REPLAY_TIME_SCHEMA_VERSION = 2;
export const REPLAY_TICKS_PER_SECOND = 48_000_000;
export const MAX_REPLAY_DURATION_SECONDS = 24 * 60 * 60;
export const MAX_REPLAY_TICK = REPLAY_TICKS_PER_SECOND * MAX_REPLAY_DURATION_SECONDS;

const MAX_DECIMAL_LENGTH = 20;
const MAX_SAFE_TICK = BigInt(MAX_REPLAY_TICK);
const MAX_SAFE_INTEGER_BIGINT = BigInt(Number.MAX_SAFE_INTEGER);
const MAX_U64 = (1n << 64n) - 1n;
const MAX_I64 = (1n << 63n) - 1n;
const MIN_I64 = -(1n << 63n);

declare const replayTickBrand: unique symbol;
declare const frameBoundaryBrand: unique symbol;
declare const mediaIdBrand: unique symbol;

export type ReplayTick = number & { readonly [replayTickBrand]: "ReplayTick" };
export type FrameBoundary = number & { readonly [frameBoundaryBrand]: "FrameBoundary" };
export type MediaId = string & { readonly [mediaIdBrand]: "MediaId" };
export type RoundingMode = "exact" | "floor" | "ceil" | "nearest_ties_to_even";

export type RationalWire = { numerator: string; denominator: string };
export type Rational = Readonly<{ numerator: bigint; denominator: bigint }>;

export type VideoTimelineV2Wire = {
  codec: string;
  profile: string | null;
  time_base: RationalWire;
  first_pts: string;
  frame_rate: RationalWire;
  frame_count: string;
  one_past_last_pts: string;
  replay_end: string;
  exact_cfr: boolean;
};

export type AudioTimelineV2Wire = {
  present: boolean;
  codec: string | null;
  sample_rate: number | null;
  time_base: RationalWire | null;
  first_pts: string | null;
  replay_start: string | null;
  replay_end: string | null;
};

export type ContainerTimelineV2Wire = {
  start_seconds: RationalWire;
  duration_seconds: RationalWire;
};

export type ProducerEvidenceV2Wire = {
  backend: string;
  expected_frame_rate: RationalWire;
  expected_frame_count: string;
  media_runtime_id: string;
};

export type MediaTimelineV2Wire = {
  schema_version: number;
  replay_ticks_per_second: string;
  media_id: string;
  video: VideoTimelineV2Wire;
  audio: AudioTimelineV2Wire;
  container: ContainerTimelineV2Wire;
  producer: ProducerEvidenceV2Wire;
  capture?: unknown | null;
};

export type MediaTimelineV2 = Readonly<{
  mediaId: MediaId;
  video: Readonly<{
    codec: string;
    profile: string | null;
    timeBase: Rational;
    firstPts: bigint;
    frameRate: Rational;
    frameCount: FrameBoundary;
    onePastLastPts: bigint;
    replayEnd: ReplayTick;
  }>;
  audio: Readonly<{ present: boolean }>;
}>;

export type ClipRange = Readonly<{
  mediaId: MediaId;
  startFrame: FrameBoundary;
  endFrameExclusive: FrameBoundary;
}>;

export class ReplayTimeError extends Error {
  constructor(readonly code: string, detail?: string) {
    super(detail ? `${code}: ${detail}` : code);
    this.name = "ReplayTimeError";
  }
}

const fail = (code: string, detail?: string): never => {
  throw new ReplayTimeError(code, detail);
};

const integerWire = (value: unknown, field: string): string => {
  if (typeof value === "string") return value;
  return fail("invalid_decimal", field);
};

export const parseUnsignedDecimal = (input: unknown, field = "unsigned_decimal"): bigint => {
  const value = integerWire(input, field);
  if (
    value.length === 0 ||
    value.length > MAX_DECIMAL_LENGTH ||
    !/^(0|[1-9][0-9]*)$/.test(value)
  ) {
    fail("invalid_decimal", field);
  }
  return BigInt(value);
};

export const parseSignedDecimal = (input: unknown, field = "signed_decimal"): bigint => {
  const value = integerWire(input, field);
  if (
    value.length === 0 ||
    value.length > MAX_DECIMAL_LENGTH ||
    !/^(0|-?[1-9][0-9]*)$/.test(value)
  ) {
    fail("invalid_decimal", field);
  }
  return BigInt(value);
};

const boundedUnsigned = (input: unknown, maximum: bigint, field: string): bigint => {
  const value = parseUnsignedDecimal(input, field);
  if (value > maximum) fail("out_of_range", field);
  return value;
};

const boundedSigned = (input: unknown, maximum: bigint, field: string): bigint => {
  const value = parseSignedDecimal(input, field);
  if (value < -maximum || value > maximum) fail("out_of_range", field);
  return value;
};

const parseI64 = (input: unknown, field: string): bigint => {
  const value = parseSignedDecimal(input, field);
  if (value < MIN_I64 || value > MAX_I64) fail("out_of_range", field);
  return value;
};

const hotNumber = (value: bigint, field: string): number => {
  if (value < 0n || value > MAX_SAFE_TICK || value > MAX_SAFE_INTEGER_BIGINT) {
    fail("out_of_range", field);
  }
  return Number(value);
};

export const parseReplayTick = (input: unknown, field = "replay_tick"): ReplayTick =>
  hotNumber(boundedUnsigned(input, MAX_SAFE_TICK, field), field) as ReplayTick;

/** Parses a signed v2 replay coordinate retained outside video coverage. */
export const parseSignedReplayTick = (
  input: unknown,
  field = "signed_replay_tick",
): bigint => boundedSigned(input, MAX_SAFE_TICK, field);

export const parseFrameBoundary = (input: unknown, field = "frame_boundary"): FrameBoundary =>
  hotNumber(boundedUnsigned(input, MAX_SAFE_TICK, field), field) as FrameBoundary;

export const parseMediaId = (input: unknown): MediaId => {
  if (
    typeof input !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(input) ||
    input === "00000000-0000-0000-0000-000000000000"
  ) {
    fail("invalid_media_id");
  }
  return input as MediaId;
};

const gcd = (left: bigint, right: bigint): bigint => {
  let a = left < 0n ? -left : left;
  let b = right < 0n ? -right : right;
  while (b !== 0n) [a, b] = [b, a % b];
  return a;
};

export const parseRational = (wire: unknown, field = "rational"): Rational => {
  if (!wire || typeof wire !== "object" || Array.isArray(wire)) fail("invalid_rational", field);
  const record = wire as Record<string, unknown>;
  if (Object.keys(record).length !== 2 || !("numerator" in record) || !("denominator" in record)) {
    fail("invalid_rational", field);
  }
  const numerator = parseI64(record.numerator, `${field}.numerator`);
  const denominator = boundedUnsigned(record.denominator, MAX_U64, `${field}.denominator`);
  if (denominator === 0n) fail("zero_denominator", field);
  const divisor = gcd(numerator, denominator);
  return { numerator: numerator / divisor, denominator: denominator / divisor };
};

export const positiveRational = (wire: unknown, field: string): Rational => {
  const rational = parseRational(wire, field);
  if (rational.numerator <= 0n) fail("nonpositive_rational", field);
  return rational;
};

export const rationalEquals = (left: Rational, right: Rational): boolean =>
  left.numerator === right.numerator && left.denominator === right.denominator;

export const checkedScale = (
  value: bigint,
  numerator: bigint,
  denominator: bigint,
  rounding: RoundingMode,
): bigint => {
  if (denominator <= 0n) fail("zero_denominator");
  const scaled = value * numerator;
  const floor = scaled / denominator;
  const remainder = scaled % denominator;
  // BigInt division truncates toward zero. Convert to Euclidean floor/remainder.
  const euclideanFloor = remainder < 0n ? floor - 1n : floor;
  const euclideanRemainder = remainder < 0n ? remainder + denominator : remainder;
  if (euclideanRemainder === 0n) return euclideanFloor;
  if (rounding === "exact") fail("inexact_conversion");
  if (rounding === "floor") return euclideanFloor;
  if (rounding === "ceil") return euclideanFloor + 1n;
  const doubled = euclideanRemainder * 2n;
  return doubled < denominator || (doubled === denominator && euclideanFloor % 2n === 0n)
    ? euclideanFloor
    : euclideanFloor + 1n;
};

const frameBoundaryBigIntToReplayTick = (
  frame: bigint,
  frameRate: Rational,
  rounding: RoundingMode,
): bigint => {
  if (frameRate.numerator <= 0n) fail("nonpositive_rational", "frame_rate");
  return checkedScale(
    frame,
    BigInt(REPLAY_TICKS_PER_SECOND) * frameRate.denominator,
    frameRate.numerator,
    rounding,
  );
};

export const frameBoundaryToReplayTick = (
  frame: FrameBoundary,
  frameRate: Rational,
  rounding: RoundingMode = "exact",
): ReplayTick => {
  const result = frameBoundaryBigIntToReplayTick(BigInt(frame), frameRate, rounding);
  if (result < 0n || result > MAX_SAFE_TICK) fail("out_of_range", "replay_tick");
  return Number(result) as ReplayTick;
};

export const replayTickToFrameBoundary = (
  tick: ReplayTick,
  frameRate: Rational,
  rounding: RoundingMode,
): FrameBoundary => {
  if (frameRate.numerator <= 0n) fail("nonpositive_rational", "frame_rate");
  const result = checkedScale(
    BigInt(tick),
    frameRate.numerator,
    BigInt(REPLAY_TICKS_PER_SECOND) * frameRate.denominator,
    rounding,
  );
  if (result < 0n || result > MAX_SAFE_TICK) fail("out_of_range", "frame_boundary");
  return Number(result) as FrameBoundary;
};

export const validateMediaTimeline = (wire: unknown): MediaTimelineV2 => {
  if (!wire || typeof wire !== "object" || Array.isArray(wire)) fail("invalid_media_timeline");
  const timeline = wire as MediaTimelineV2Wire;
  if (timeline.schema_version !== REPLAY_TIME_SCHEMA_VERSION) fail("unsupported_schema_version");
  if (boundedUnsigned(timeline.replay_ticks_per_second, MAX_SAFE_INTEGER_BIGINT, "replay_ticks_per_second") !== BigInt(REPLAY_TICKS_PER_SECOND)) {
    fail("replay_scale_mismatch");
  }
  const mediaId = parseMediaId(timeline.media_id);
  const video = timeline.video;
  const producer = timeline.producer;
  if (!video || !producer || typeof video.codec !== "string" || !video.codec.trim() || typeof producer.backend !== "string" || !producer.backend.trim() || typeof producer.media_runtime_id !== "string" || !producer.media_runtime_id.trim()) {
    fail("invalid_media_timeline", "required identifiers");
  }
  const timeBase = positiveRational(video.time_base, "video.time_base");
  const frameRate = positiveRational(video.frame_rate, "video.frame_rate");
  const expectedFrameRate = positiveRational(producer.expected_frame_rate, "producer.expected_frame_rate");
  const container = timeline.container;
  if (!container) fail("invalid_media_timeline", "container");
  positiveRational(container.duration_seconds, "container.duration_seconds");
  if (!rationalEquals(frameRate, expectedFrameRate) || video.exact_cfr !== true) {
    fail("invalid_media_timeline", "exact CFR producer evidence");
  }
  const frameCount = parseFrameBoundary(video.frame_count, "video.frame_count");
  if (frameCount === 0) fail("invalid_media_timeline", "empty frame grid");
  if (parseFrameBoundary(producer.expected_frame_count, "producer.expected_frame_count") !== frameCount) {
    fail("invalid_media_timeline", "frame count evidence");
  }
  const replayEnd = parseReplayTick(video.replay_end, "video.replay_end");
  if (frameBoundaryToReplayTick(frameCount, frameRate) !== replayEnd) {
    fail("invalid_media_timeline", "frame coverage");
  }
  const firstPts = parseI64(video.first_pts, "media_pts");
  const lastPts = parseI64(video.one_past_last_pts, "media_pts");
  const ptsDelta = lastPts - firstPts;
  const ptsEnd = checkedScale(
    ptsDelta,
    timeBase.numerator * BigInt(REPLAY_TICKS_PER_SECOND),
    timeBase.denominator,
    "exact",
  );
  if (ptsEnd < 0n || ptsEnd !== BigInt(replayEnd)) fail("invalid_media_timeline", "PTS coverage");
  const audio = timeline.audio;
  if (!audio || typeof audio.present !== "boolean") fail("invalid_media_timeline", "audio");
  if (audio.present) {
    if (!audio.codec?.trim() || !Number.isInteger(audio.sample_rate) || (audio.sample_rate ?? 0) <= 0 || !audio.time_base || audio.first_pts === null || audio.replay_start === null || audio.replay_end === null) {
      fail("invalid_media_timeline", "audio facts");
    }
    positiveRational(audio.time_base, "audio.time_base");
    const start = boundedSigned(audio.replay_start, MAX_SAFE_TICK, "signed_replay_tick");
    const end = boundedSigned(audio.replay_end, MAX_SAFE_TICK, "signed_replay_tick");
    if (end <= start) fail("invalid_media_timeline", "audio coverage");
  } else if (audio.codec !== null || audio.sample_rate !== null || audio.time_base !== null || audio.first_pts !== null || audio.replay_start !== null || audio.replay_end !== null) {
    fail("invalid_media_timeline", "absent audio facts");
  }
  return {
    mediaId,
    video: {
      codec: video.codec,
      profile: video.profile,
      timeBase,
      firstPts,
      frameRate,
      frameCount,
      onePastLastPts: lastPts,
      replayEnd,
    },
    audio: { present: audio.present },
  };
};

export const browserSecondsForReplayTick = (timeline: MediaTimelineV2, tick: ReplayTick): number =>
  Number(timeline.video.firstPts) * Number(timeline.video.timeBase.numerator) /
    Number(timeline.video.timeBase.denominator) + tick / REPLAY_TICKS_PER_SECOND;

export const replayTickForBrowserSeconds = (
  timeline: MediaTimelineV2,
  browserSeconds: number,
): ReplayTick => {
  if (!Number.isFinite(browserSeconds)) fail("invalid_floating_point", "browser_media_seconds");
  const origin = browserSecondsForReplayTick(timeline, 0 as ReplayTick);
  const rounded = Math.round((browserSeconds - origin) * REPLAY_TICKS_PER_SECOND);
  if (!Number.isSafeInteger(rounded) || rounded < 0 || rounded > MAX_REPLAY_TICK) {
    fail("out_of_range", "replay_tick");
  }
  return rounded as ReplayTick;
};

export const createClipRange = (
  timeline: MediaTimelineV2,
  startFrame: FrameBoundary,
  endFrameExclusive: FrameBoundary,
  mediaId = timeline.mediaId,
): ClipRange => {
  if (mediaId !== timeline.mediaId) fail("media_identity_mismatch");
  if (startFrame >= endFrameExclusive || endFrameExclusive > timeline.video.frameCount) {
    fail("invalid_clip_range");
  }
  return { mediaId, startFrame, endFrameExclusive };
};

export const projectReplayIntervalToClipRange = (
  timeline: MediaTimelineV2,
  start: ReplayTick,
  end: ReplayTick,
): ClipRange => {
  if (start >= end) fail("invalid_clip_range");
  const startFrame = replayTickToFrameBoundary(start, timeline.video.frameRate, "floor");
  const endFrame = replayTickToFrameBoundary(end, timeline.video.frameRate, "ceil");
  return createClipRange(
    timeline,
    Math.min(startFrame, timeline.video.frameCount) as FrameBoundary,
    Math.min(endFrame, timeline.video.frameCount) as FrameBoundary,
  );
};
