//! Exact, versioned replay-time primitives shared by the recorder and replay app.
//!
//! Replay time is video-owned and uses a fixed 48 MHz integer coordinate. Values
//! that cross a JSON boundary use canonical decimal strings; callers must select
//! a rounding policy for every non-exact rational conversion.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use serde::de::{self};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Schema version of the canonical recording time contract.
pub const REPLAY_TIME_SCHEMA_VERSION: u32 = 2;
/// Number of canonical replay ticks in one second.
pub const REPLAY_TICKS_PER_SECOND: u64 = 48_000_000;
/// Maximum duration accepted by the v2 contract, in seconds.
pub const MAX_REPLAY_DURATION_SECONDS: u64 = 24 * 60 * 60;
/// Maximum canonical replay position accepted by the v2 contract.
pub const MAX_REPLAY_TICK: u64 = REPLAY_TICKS_PER_SECOND * MAX_REPLAY_DURATION_SECONDS;
/// Number of League game-clock ticks in one second.
pub const GAME_TICKS_PER_SECOND: i64 = 1_000_000;
/// Canonical replay ticks in one League game-clock microsecond.
pub const REPLAY_TICKS_PER_GAME_TICK: i64 = 48;

const MAX_DECIMAL_LEN: usize = 20;

/// Failure produced while parsing, converting, or validating replay-time data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReplayTimeError {
    /// A decimal string was not in canonical grammar.
    InvalidDecimal {
        /// Semantic field whose wire value was invalid.
        field: &'static str,
    },
    /// A value exceeded the documented range for its semantic type.
    OutOfRange {
        /// Semantic field whose value exceeded its bound.
        field: &'static str,
    },
    /// Checked integer arithmetic overflowed.
    Overflow {
        /// Checked operation that could not be represented.
        operation: &'static str,
    },
    /// A rational denominator was zero.
    ZeroDenominator,
    /// A conversion requested exactness but had a nonzero remainder.
    InexactConversion,
    /// A rational that must be positive was zero or negative.
    NonPositiveRational {
        /// Semantic field that required a positive rational.
        field: &'static str,
    },
    /// The media-generation identifier was not a canonical non-nil UUID.
    InvalidMediaId,
    /// A versioned contract used an unsupported version.
    UnsupportedSchemaVersion {
        /// Unsupported version found at the boundary.
        found: u32,
    },
    /// A persisted replay scale did not match the v2 constant.
    ReplayScaleMismatch {
        /// Unexpected persisted replay-ticks-per-second value.
        found: u64,
    },
    /// Media facts contradict one another.
    InvalidMediaTimeline {
        /// Stable explanation of the violated cross-field invariant.
        reason: &'static str,
    },
    /// Two related values refer to different media generations.
    MediaIdentityMismatch,
    /// A clip range is empty, reversed, or outside the source frame coverage.
    InvalidClipRange,
    /// A floating-point source observation was nonfinite or outside its range.
    InvalidFloatingPoint {
        /// Source observation that was nonfinite or outside its range.
        field: &'static str,
    },
    /// The bounded ffprobe summary was malformed or contradicted expectations.
    InvalidMediaProbe {
        /// Stable explanation of the rejected probe fact.
        reason: &'static str,
    },
}

impl fmt::Display for ReplayTimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDecimal { field } => {
                write!(formatter, "{field} is not a canonical decimal string")
            }
            Self::OutOfRange { field } => write!(formatter, "{field} is out of range"),
            Self::Overflow { operation } => {
                write!(formatter, "integer overflow while {operation}")
            }
            Self::ZeroDenominator => formatter.write_str("rational denominator is zero"),
            Self::InexactConversion => formatter.write_str("conversion is not exact"),
            Self::NonPositiveRational { field } => {
                write!(formatter, "{field} must be a positive rational")
            }
            Self::InvalidMediaId => formatter.write_str("media_id is not a canonical non-nil UUID"),
            Self::UnsupportedSchemaVersion { found } => {
                write!(formatter, "unsupported replay-time schema version {found}")
            }
            Self::ReplayScaleMismatch { found } => {
                write!(formatter, "unexpected replay tick scale {found}")
            }
            Self::InvalidMediaTimeline { reason } => {
                write!(formatter, "invalid media timeline: {reason}")
            }
            Self::MediaIdentityMismatch => formatter.write_str("media identity mismatch"),
            Self::InvalidClipRange => formatter.write_str("invalid clip frame range"),
            Self::InvalidFloatingPoint { field } => {
                write!(formatter, "{field} is not a finite in-range value")
            }
            Self::InvalidMediaProbe { reason } => {
                write!(formatter, "invalid media probe: {reason}")
            }
        }
    }
}

impl std::error::Error for ReplayTimeError {}

fn parse_unsigned_decimal(
    input: &str,
    field: &'static str,
    maximum: u64,
) -> Result<u64, ReplayTimeError> {
    if input.is_empty()
        || input.len() > MAX_DECIMAL_LEN
        || (input.len() > 1 && input.starts_with('0'))
        || !input.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ReplayTimeError::InvalidDecimal { field });
    }

    let value = input
        .parse::<u64>()
        .map_err(|_| ReplayTimeError::OutOfRange { field })?;
    if value > maximum {
        return Err(ReplayTimeError::OutOfRange { field });
    }
    Ok(value)
}

fn parse_signed_decimal(
    input: &str,
    field: &'static str,
    minimum: i64,
    maximum: i64,
) -> Result<i64, ReplayTimeError> {
    let digits = input.strip_prefix('-').unwrap_or(input);
    if input.is_empty()
        || input.starts_with('+')
        || input == "-0"
        || digits.is_empty()
        || digits.len() > MAX_DECIMAL_LEN
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ReplayTimeError::InvalidDecimal { field });
    }

    let value = input
        .parse::<i64>()
        .map_err(|_| ReplayTimeError::OutOfRange { field })?;
    if !(minimum..=maximum).contains(&value) {
        return Err(ReplayTimeError::OutOfRange { field });
    }
    Ok(value)
}

fn serialize_display<S, T>(value: T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: fmt::Display,
{
    serializer.collect_str(&value)
}

macro_rules! unsigned_decimal_newtype {
    ($(#[$meta:meta])* $name:ident, $field:literal, $maximum:expr) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// Creates a validated value.
            ///
            /// # Errors
            ///
            /// Returns [`ReplayTimeError::OutOfRange`] when `value` exceeds the
            /// contract bound for this semantic type.
            pub fn new(value: u64) -> Result<Self, ReplayTimeError> {
                if value > $maximum {
                    Err(ReplayTimeError::OutOfRange { field: $field })
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the validated integer value.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = ReplayTimeError;

            fn from_str(input: &str) -> Result<Self, Self::Err> {
                parse_unsigned_decimal(input, $field, $maximum).map(Self)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serialize_display(self.0, serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let input = String::deserialize(deserializer)?;
                input.parse::<Self>().map_err(de::Error::custom)
            }
        }
    };
}

unsigned_decimal_newtype!(
    /// An exact instant or boundary in the canonical replay coordinate.
    ReplayTick,
    "replay_tick",
    MAX_REPLAY_TICK
);
unsigned_decimal_newtype!(
    /// A zero-based presented-video frame index.
    FrameIndex,
    "frame_index",
    MAX_REPLAY_TICK
);
unsigned_decimal_newtype!(
    /// A boundary in the half-open video frame grid.
    FrameBoundary,
    "frame_boundary",
    MAX_REPLAY_TICK
);
unsigned_decimal_newtype!(
    /// A zero-based encoded-audio sample index.
    AudioSampleIndex,
    "audio_sample_index",
    u64::MAX
);

/// A signed canonical replay coordinate used for affine offsets and out-of-media mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignedReplayTick(i64);

impl SignedReplayTick {
    /// Creates a signed replay tick bounded to the v2 duration range.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayTimeError::OutOfRange`] when the absolute coordinate
    /// exceeds 24 hours.
    pub fn new(value: i64) -> Result<Self, ReplayTimeError> {
        let maximum = i64::try_from(MAX_REPLAY_TICK).map_err(|_| ReplayTimeError::Overflow {
            operation: "converting maximum replay tick",
        })?;
        if (-maximum..=maximum).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ReplayTimeError::OutOfRange {
                field: "signed_replay_tick",
            })
        }
    }

    /// Returns the validated integer value.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for SignedReplayTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for SignedReplayTick {
    type Err = ReplayTimeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let maximum = i64::try_from(MAX_REPLAY_TICK).map_err(|_| ReplayTimeError::Overflow {
            operation: "converting maximum replay tick",
        })?;
        parse_signed_decimal(input, "signed_replay_tick", -maximum, maximum).map(Self)
    }
}

impl Serialize for SignedReplayTick {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_display(self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for SignedReplayTick {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        input.parse().map_err(de::Error::custom)
    }
}

/// A League game-clock observation expressed in integer microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameTick(i64);

impl GameTick {
    /// Creates a game tick within the signed 24-hour v2 bound.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayTimeError::OutOfRange`] when `value` is outside the bound.
    pub fn new(value: i64) -> Result<Self, ReplayTimeError> {
        let maximum = i64::try_from(MAX_REPLAY_DURATION_SECONDS)
            .ok()
            .and_then(|seconds| seconds.checked_mul(GAME_TICKS_PER_SECOND))
            .ok_or(ReplayTimeError::Overflow {
                operation: "computing maximum game tick",
            })?;
        if (-maximum..=maximum).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ReplayTimeError::OutOfRange { field: "game_tick" })
        }
    }

    /// Converts one finite Live Client seconds observation to integer microseconds
    /// using nearest, ties-to-even rounding.
    ///
    /// # Errors
    ///
    /// Returns an error for nonfinite or out-of-range values.
    pub fn from_seconds(seconds: f64) -> Result<Self, ReplayTimeError> {
        if !seconds.is_finite() {
            return Err(ReplayTimeError::InvalidFloatingPoint {
                field: "game_time_seconds",
            });
        }
        let scaled = seconds * GAME_TICKS_PER_SECOND as f64;
        if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
            return Err(ReplayTimeError::InvalidFloatingPoint {
                field: "game_time_seconds",
            });
        }
        Self::new(round_f64_ties_even(scaled)?)
    }

    /// Returns the validated integer microseconds.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for GameTick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for GameTick {
    type Err = ReplayTimeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let maximum = i64::try_from(MAX_REPLAY_DURATION_SECONDS)
            .ok()
            .and_then(|seconds| seconds.checked_mul(GAME_TICKS_PER_SECOND))
            .ok_or(ReplayTimeError::Overflow {
                operation: "computing maximum game tick",
            })?;
        parse_signed_decimal(input, "game_tick", -maximum, maximum).map(Self)
    }
}

impl Serialize for GameTick {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_display(self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for GameTick {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        input.parse().map_err(de::Error::custom)
    }
}

/// An integer presentation timestamp in a source media time base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MediaPts(i64);

impl MediaPts {
    /// Creates a media PTS. Every `i64` value is representable.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Returns the integer presentation timestamp.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl fmt::Display for MediaPts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for MediaPts {
    type Err = ReplayTimeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        parse_signed_decimal(input, "media_pts", i64::MIN, i64::MAX).map(Self)
    }
}

impl Serialize for MediaPts {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_display(self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for MediaPts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        input.parse().map_err(de::Error::custom)
    }
}

/// One opaque recording-generation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaId(String);

impl MediaId {
    /// Generates a fresh random media-generation UUID.
    #[must_use]
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4().hyphenated().to_string())
    }

    /// Parses a lowercase hyphenated, non-nil UUID.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayTimeError::InvalidMediaId`] for every noncanonical value.
    pub fn parse(input: &str) -> Result<Self, ReplayTimeError> {
        let parsed = uuid::Uuid::parse_str(input).map_err(|_| ReplayTimeError::InvalidMediaId)?;
        if parsed.is_nil() || parsed.hyphenated().to_string() != input {
            return Err(ReplayTimeError::InvalidMediaId);
        }
        Ok(Self(input.to_owned()))
    }

    /// Returns the canonical UUID string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MediaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for MediaId {
    type Err = ReplayTimeError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

impl Serialize for MediaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MediaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(de::Error::custom)
    }
}

/// An exact normalized rational number with a positive denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "RawRational", deny_unknown_fields)]
pub struct Rational {
    numerator: i64,
    denominator: u64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRational {
    #[serde(deserialize_with = "deserialize_i64_decimal")]
    numerator: i64,
    #[serde(deserialize_with = "deserialize_u64_decimal")]
    denominator: u64,
}

impl TryFrom<RawRational> for Rational {
    type Error = ReplayTimeError;

    fn try_from(raw: RawRational) -> Result<Self, Self::Error> {
        Self::new(raw.numerator, raw.denominator)
    }
}

impl Rational {
    /// Creates and normalizes a rational value.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayTimeError::ZeroDenominator`] for a zero denominator and
    /// [`ReplayTimeError::Overflow`] when normalization cannot fit the public form.
    pub fn new(numerator: i64, denominator: u64) -> Result<Self, ReplayTimeError> {
        if denominator == 0 {
            return Err(ReplayTimeError::ZeroDenominator);
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }
        let magnitude = numerator.unsigned_abs();
        let divisor = gcd(magnitude, denominator);
        let normalized_denominator = denominator / divisor;
        let normalized_magnitude = magnitude / divisor;
        let normalized_numerator = if numerator.is_negative() {
            let magnitude_i128 = i128::from(normalized_magnitude);
            i64::try_from(-magnitude_i128).map_err(|_| ReplayTimeError::Overflow {
                operation: "normalizing rational numerator",
            })?
        } else {
            i64::try_from(normalized_magnitude).map_err(|_| ReplayTimeError::Overflow {
                operation: "normalizing rational numerator",
            })?
        };
        Ok(Self {
            numerator: normalized_numerator,
            denominator: normalized_denominator,
        })
    }

    /// Creates a normalized positive rational.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is zero or negative.
    pub fn positive(
        numerator: i64,
        denominator: u64,
        field: &'static str,
    ) -> Result<Self, ReplayTimeError> {
        let value = Self::new(numerator, denominator)?;
        if value.numerator <= 0 {
            return Err(ReplayTimeError::NonPositiveRational { field });
        }
        Ok(value)
    }

    /// Returns the normalized numerator.
    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    /// Returns the normalized positive denominator.
    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    /// Returns whether this rational is strictly positive.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.numerator > 0
    }

    /// Converts the rational to an `f64` at a final display or DOM boundary.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl Serialize for Rational {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Rational", 2)?;
        state.serialize_field("numerator", &self.numerator.to_string())?;
        state.serialize_field("denominator", &self.denominator.to_string())?;
        state.end()
    }
}

fn deserialize_u64_decimal<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let input = String::deserialize(deserializer)?;
    parse_unsigned_decimal(&input, "unsigned_decimal", u64::MAX).map_err(de::Error::custom)
}

fn deserialize_i64_decimal<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let input = String::deserialize(deserializer)?;
    parse_signed_decimal(&input, "signed_decimal", i64::MIN, i64::MAX).map_err(de::Error::custom)
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Required rounding behavior for a rational conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// Reject any nonzero remainder.
    Exact,
    /// Round toward negative infinity.
    Floor,
    /// Round toward positive infinity.
    Ceil,
    /// Round to the nearest integer and resolve exact ties toward an even result.
    NearestTiesToEven,
}

/// Multiplies `value` by `numerator / denominator` with checked arithmetic.
///
/// # Errors
///
/// Returns an error for a zero denominator, overflow, or an inexact conversion
/// when [`RoundingMode::Exact`] is requested.
pub fn checked_scale(
    value: i128,
    numerator: i128,
    denominator: i128,
    rounding: RoundingMode,
) -> Result<i128, ReplayTimeError> {
    if denominator <= 0 {
        return Err(ReplayTimeError::ZeroDenominator);
    }
    let scaled = value
        .checked_mul(numerator)
        .ok_or(ReplayTimeError::Overflow {
            operation: "scaling rational value",
        })?;
    round_div(scaled, denominator, rounding)
}

fn round_div(
    numerator: i128,
    denominator: i128,
    rounding: RoundingMode,
) -> Result<i128, ReplayTimeError> {
    if denominator <= 0 {
        return Err(ReplayTimeError::ZeroDenominator);
    }
    let floor = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    if remainder == 0 {
        return Ok(floor);
    }
    match rounding {
        RoundingMode::Exact => Err(ReplayTimeError::InexactConversion),
        RoundingMode::Floor => Ok(floor),
        RoundingMode::Ceil => floor.checked_add(1).ok_or(ReplayTimeError::Overflow {
            operation: "rounding rational value up",
        }),
        RoundingMode::NearestTiesToEven => {
            let doubled = remainder.checked_mul(2).ok_or(ReplayTimeError::Overflow {
                operation: "comparing rational remainder",
            })?;
            if doubled < denominator || (doubled == denominator && floor.rem_euclid(2) == 0) {
                Ok(floor)
            } else {
                floor.checked_add(1).ok_or(ReplayTimeError::Overflow {
                    operation: "rounding rational value to nearest",
                })
            }
        }
    }
}

fn round_f64_ties_even(value: f64) -> Result<i64, ReplayTimeError> {
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return Err(ReplayTimeError::InvalidFloatingPoint {
            field: "floating_point_value",
        });
    }
    let floor = value.floor();
    let fraction = value - floor;
    let rounded = if fraction < 0.5 {
        floor
    } else if fraction > 0.5 {
        floor + 1.0
    } else {
        let floor_integer = floor as i64;
        if floor_integer.rem_euclid(2) == 0 {
            floor
        } else {
            floor + 1.0
        }
    };
    if rounded < i64::MIN as f64 || rounded > i64::MAX as f64 {
        return Err(ReplayTimeError::InvalidFloatingPoint {
            field: "floating_point_value",
        });
    }
    Ok(rounded as i64)
}

/// Converts a frame boundary to canonical replay ticks.
///
/// `frame_rate` is expressed as frames per second.
///
/// # Errors
///
/// Returns an error for a nonpositive frame rate, overflow, out-of-range output,
/// or an inexact result under [`RoundingMode::Exact`].
pub fn frame_boundary_to_replay_tick(
    boundary: FrameBoundary,
    frame_rate: Rational,
    rounding: RoundingMode,
) -> Result<ReplayTick, ReplayTimeError> {
    if !frame_rate.is_positive() {
        return Err(ReplayTimeError::NonPositiveRational {
            field: "frame_rate",
        });
    }
    let numerator = i128::from(REPLAY_TICKS_PER_SECOND)
        .checked_mul(i128::from(frame_rate.denominator()))
        .ok_or(ReplayTimeError::Overflow {
            operation: "building frame duration scale",
        })?;
    let denominator = i128::from(frame_rate.numerator());
    let value = checked_scale(i128::from(boundary.get()), numerator, denominator, rounding)?;
    let value = u64::try_from(value).map_err(|_| ReplayTimeError::OutOfRange {
        field: "replay_tick",
    })?;
    ReplayTick::new(value)
}

/// Converts a canonical replay tick to a frame-grid boundary.
///
/// # Errors
///
/// Returns an error for a nonpositive frame rate, overflow, or an inexact result
/// under [`RoundingMode::Exact`].
pub fn replay_tick_to_frame_boundary(
    tick: ReplayTick,
    frame_rate: Rational,
    rounding: RoundingMode,
) -> Result<FrameBoundary, ReplayTimeError> {
    if !frame_rate.is_positive() {
        return Err(ReplayTimeError::NonPositiveRational {
            field: "frame_rate",
        });
    }
    let denominator = i128::from(REPLAY_TICKS_PER_SECOND)
        .checked_mul(i128::from(frame_rate.denominator()))
        .ok_or(ReplayTimeError::Overflow {
            operation: "building replay-to-frame scale",
        })?;
    let value = checked_scale(
        i128::from(tick.get()),
        i128::from(frame_rate.numerator()),
        denominator,
        rounding,
    )?;
    let value = u64::try_from(value).map_err(|_| ReplayTimeError::OutOfRange {
        field: "frame_boundary",
    })?;
    FrameBoundary::new(value)
}

/// Converts a raw media PTS delta to a signed canonical replay coordinate.
///
/// # Errors
///
/// Returns an error for an invalid time base, overflow, an out-of-range output,
/// or an inexact result under [`RoundingMode::Exact`].
pub fn media_pts_delta_to_replay_tick(
    pts_delta: i64,
    time_base: Rational,
    rounding: RoundingMode,
) -> Result<SignedReplayTick, ReplayTimeError> {
    if !time_base.is_positive() {
        return Err(ReplayTimeError::NonPositiveRational {
            field: "media_time_base",
        });
    }
    let numerator = i128::from(time_base.numerator())
        .checked_mul(i128::from(REPLAY_TICKS_PER_SECOND))
        .ok_or(ReplayTimeError::Overflow {
            operation: "building media PTS scale",
        })?;
    let value = checked_scale(
        i128::from(pts_delta),
        numerator,
        i128::from(time_base.denominator()),
        rounding,
    )?;
    let value = i64::try_from(value).map_err(|_| ReplayTimeError::OutOfRange {
        field: "signed_replay_tick",
    })?;
    SignedReplayTick::new(value)
}

/// Raw integer media presentation time with its source time base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMediaPts {
    /// Integer source presentation timestamp.
    pub value: MediaPts,
    /// Seconds represented by one source timestamp tick.
    pub time_base: Rational,
}

/// Exact facts for the authoritative video stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoTimelineV2 {
    /// Lowercase codec identifier reported by the packaged ffprobe.
    pub codec: String,
    /// Optional codec profile reported by the packaged ffprobe.
    pub profile: Option<String>,
    /// Source MP4 presentation timestamp time base.
    pub time_base: Rational,
    /// First presentation timestamp; this boundary maps to replay zero.
    pub first_pts: MediaPts,
    /// Exact selected frame rate in frames per second.
    pub frame_rate: Rational,
    /// Number of presented source video frames.
    pub frame_count: FrameBoundary,
    /// Presentation boundary immediately after the final frame.
    pub one_past_last_pts: MediaPts,
    /// Canonical half-open replay coverage end.
    pub replay_end: ReplayTick,
    /// Whether recorder evidence and finalized facts prove an exact CFR grid.
    pub exact_cfr: bool,
}

/// Exact facts for the encoded audio stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioTimelineV2 {
    /// Whether an encoded audio stream exists.
    pub present: bool,
    /// Lowercase codec identifier when audio exists.
    pub codec: Option<String>,
    /// Encoded sample rate when audio exists.
    pub sample_rate: Option<u32>,
    /// Source audio presentation timestamp time base when audio exists.
    pub time_base: Option<Rational>,
    /// First audio presentation timestamp when audio exists.
    pub first_pts: Option<MediaPts>,
    /// Audio coverage start mapped into replay coordinates.
    pub replay_start: Option<SignedReplayTick>,
    /// Audio coverage end mapped into replay coordinates.
    pub replay_end: Option<SignedReplayTick>,
}

/// Non-authoritative container-level probe facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerTimelineV2 {
    /// Container start time in exact seconds.
    pub start_seconds: Rational,
    /// Container duration in exact seconds.
    pub duration_seconds: Rational,
}

/// Recorder-owned evidence reconciled with finalized media facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerEvidenceV2 {
    /// Stable backend identifier.
    pub backend: String,
    /// Exact rate selected by the producer.
    pub expected_frame_rate: Rational,
    /// Final accepted/muxed frame count expected by the producer.
    pub expected_frame_count: FrameBoundary,
    /// Packaged media runtime identity used for production and validation.
    pub media_runtime_id: String,
}

/// Exact `-show_entries` selection used by the bounded production ffprobe.
pub const FFPROBE_SUMMARY_SHOW_ENTRIES: &str = concat!(
    "stream=index,codec_type,codec_name,profile,time_base,start_pts,duration_ts,",
    "nb_frames,avg_frame_rate,sample_rate:",
    "format=start_time,duration"
);

/// Recorder-owned expectations supplied to bounded finalized-media validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizationExpectationsV2 {
    /// Opaque identity allocated before capture begins.
    pub media_id: MediaId,
    /// Required finalized video codec.
    pub expected_video_codec: String,
    /// Required finalized audio codec, or `None` when the recording must have no audio.
    pub expected_audio_codec: Option<String>,
    /// Producer cadence and runtime evidence.
    pub producer: ProducerEvidenceV2,
    /// Optional capture-clock provenance.
    pub capture: Option<CaptureAnchorsV2>,
    /// Whether recorder-produced media must begin at source presentation timestamp zero.
    pub require_zero_video_start: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ProbeInteger {
    String(String),
    Signed(i64),
    Unsigned(u64),
}

impl ProbeInteger {
    fn as_decimal(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Signed(value) => value.to_string(),
            Self::Unsigned(value) => value.to_string(),
        }
    }
}

fn deserialize_empty_probe_wrapper<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    if values.is_empty() {
        Ok(())
    } else {
        Err(de::Error::custom(
            "ffprobe programs and stream groups must be empty",
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeDocument {
    #[serde(
        default,
        rename = "programs",
        deserialize_with = "deserialize_empty_probe_wrapper"
    )]
    _programs: (),
    #[serde(
        default,
        rename = "stream_groups",
        deserialize_with = "deserialize_empty_probe_wrapper"
    )]
    _stream_groups: (),
    streams: Vec<ProbeStream>,
    format: ProbeFormat,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeStream {
    index: u32,
    codec_type: String,
    codec_name: String,
    profile: Option<String>,
    time_base: String,
    start_pts: Option<ProbeInteger>,
    duration_ts: Option<ProbeInteger>,
    nb_frames: Option<String>,
    avg_frame_rate: Option<String>,
    sample_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeFormat {
    start_time: String,
    duration: String,
}

/// Parses and validates the small fixed ffprobe JSON summary used at publication.
///
/// This function never scans media itself. The caller is responsible for invoking
/// the packaged ffprobe under the documented timeout/output bounds with
/// [`FFPROBE_SUMMARY_SHOW_ENTRIES`].
///
/// # Errors
///
/// Returns an error for malformed JSON, missing/extra streams, unsupported or
/// contradictory codecs/rates/counts/coverage, noncanonical numeric facts, or
/// any timeline that fails [`MediaTimelineV2::validate`].
pub fn parse_ffprobe_media_timeline(
    json: &str,
    expectations: FinalizationExpectationsV2,
) -> Result<MediaTimelineV2, ReplayTimeError> {
    let document: ProbeDocument =
        serde_json::from_str(json).map_err(|_| ReplayTimeError::InvalidMediaProbe {
            reason: "summary is not the fixed JSON shape",
        })?;

    let mut video_streams = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "video");
    let video = video_streams
        .next()
        .ok_or(ReplayTimeError::InvalidMediaProbe {
            reason: "exactly one video stream is required",
        })?;
    if video_streams.next().is_some() {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "exactly one video stream is required",
        });
    }
    let mut audio_streams = document
        .streams
        .iter()
        .filter(|stream| stream.codec_type == "audio");
    let audio = audio_streams.next();
    if audio_streams.next().is_some() {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "at most one audio stream is supported",
        });
    }
    if document
        .streams
        .iter()
        .any(|stream| stream.codec_type != "video" && stream.codec_type != "audio")
    {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "unsupported stream type is present",
        });
    }
    if video.index > 255 {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "video stream index is outside the bounded contract",
        });
    }
    if video.codec_name != expectations.expected_video_codec {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "video codec does not match recorder expectations",
        });
    }

    let video_time_base = parse_probe_rational(&video.time_base, "video time base")?;
    let frame_rate = parse_probe_rational(
        video
            .avg_frame_rate
            .as_deref()
            .ok_or(ReplayTimeError::InvalidMediaProbe {
                reason: "video average frame rate is missing",
            })?,
        "video frame rate",
    )?;
    if frame_rate != expectations.producer.expected_frame_rate {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "video frame rate does not match recorder expectations",
        });
    }
    let first_video_pts = parse_probe_i64(
        &video
            .start_pts
            .as_ref()
            .ok_or(ReplayTimeError::InvalidMediaProbe {
                reason: "video start PTS is missing",
            })?
            .as_decimal(),
        "video start PTS",
    )?;
    if expectations.require_zero_video_start && first_video_pts != 0 {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "recorder output video does not start at PTS zero",
        });
    }
    let video_duration_ts = parse_probe_i64(
        &video
            .duration_ts
            .as_ref()
            .ok_or(ReplayTimeError::InvalidMediaProbe {
                reason: "video duration_ts is missing",
            })?
            .as_decimal(),
        "video duration_ts",
    )?;
    if video_duration_ts <= 0 {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "video duration_ts must be positive",
        });
    }
    let probed_frame_count = parse_probe_u64(
        video
            .nb_frames
            .as_deref()
            .ok_or(ReplayTimeError::InvalidMediaProbe {
                reason: "video frame count is missing",
            })?,
        "video frame count",
    )?;
    if probed_frame_count != expectations.producer.expected_frame_count.get() {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "video frame count does not match recorder expectations",
        });
    }
    let one_past_last_video_pts =
        first_video_pts
            .checked_add(video_duration_ts)
            .ok_or(ReplayTimeError::Overflow {
                operation: "computing one-past-last video PTS",
            })?;
    let replay_end = frame_boundary_to_replay_tick(
        expectations.producer.expected_frame_count,
        frame_rate,
        RoundingMode::Exact,
    )?;

    let expected_audio_codec = expectations.expected_audio_codec.as_deref();
    let audio_timeline = match (audio, expected_audio_codec) {
        (None, None) => AudioTimelineV2 {
            present: false,
            codec: None,
            sample_rate: None,
            time_base: None,
            first_pts: None,
            replay_start: None,
            replay_end: None,
        },
        (None, Some(_)) => {
            return Err(ReplayTimeError::InvalidMediaProbe {
                reason: "required audio stream is missing",
            });
        }
        (Some(_), None) => {
            return Err(ReplayTimeError::InvalidMediaProbe {
                reason: "unexpected audio stream is present",
            });
        }
        (Some(audio), Some(expected_codec)) => {
            if audio.index > 255 || audio.index == video.index || audio.codec_name != expected_codec
            {
                return Err(ReplayTimeError::InvalidMediaProbe {
                    reason: "audio stream identity or codec is invalid",
                });
            }
            let time_base = parse_probe_rational(&audio.time_base, "audio time base")?;
            let sample_rate = u32::try_from(parse_probe_u64(
                audio
                    .sample_rate
                    .as_deref()
                    .ok_or(ReplayTimeError::InvalidMediaProbe {
                        reason: "audio sample rate is missing",
                    })?,
                "audio sample rate",
            )?)
            .map_err(|_| ReplayTimeError::InvalidMediaProbe {
                reason: "audio sample rate is outside the supported range",
            })?;
            if sample_rate == 0 || sample_rate > 384_000 {
                return Err(ReplayTimeError::InvalidMediaProbe {
                    reason: "audio sample rate is outside the supported range",
                });
            }
            let first_audio_pts = parse_probe_i64(
                &audio
                    .start_pts
                    .as_ref()
                    .ok_or(ReplayTimeError::InvalidMediaProbe {
                        reason: "audio start PTS is missing",
                    })?
                    .as_decimal(),
                "audio start PTS",
            )?;
            let audio_duration_ts = parse_probe_i64(
                &audio
                    .duration_ts
                    .as_ref()
                    .ok_or(ReplayTimeError::InvalidMediaProbe {
                        reason: "audio duration_ts is missing",
                    })?
                    .as_decimal(),
                "audio duration_ts",
            )?;
            if audio_duration_ts <= 0 {
                return Err(ReplayTimeError::InvalidMediaProbe {
                    reason: "audio duration_ts must be positive",
                });
            }
            let audio_end_pts = first_audio_pts.checked_add(audio_duration_ts).ok_or(
                ReplayTimeError::Overflow {
                    operation: "computing audio end PTS",
                },
            )?;
            let replay_start = map_pts_between_time_bases(
                first_audio_pts,
                time_base,
                first_video_pts,
                video_time_base,
            )?;
            let replay_audio_end = map_pts_between_time_bases(
                audio_end_pts,
                time_base,
                first_video_pts,
                video_time_base,
            )?;
            AudioTimelineV2 {
                present: true,
                codec: Some(audio.codec_name.clone()),
                sample_rate: Some(sample_rate),
                time_base: Some(time_base),
                first_pts: Some(MediaPts::new(first_audio_pts)),
                replay_start: Some(replay_start),
                replay_end: Some(replay_audio_end),
            }
        }
    };

    let timeline = MediaTimelineV2 {
        schema_version: REPLAY_TIME_SCHEMA_VERSION,
        replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
        media_id: expectations.media_id,
        video: VideoTimelineV2 {
            codec: video.codec_name.clone(),
            profile: video.profile.clone(),
            time_base: video_time_base,
            first_pts: MediaPts::new(first_video_pts),
            frame_rate,
            frame_count: FrameBoundary::new(probed_frame_count)?,
            one_past_last_pts: MediaPts::new(one_past_last_video_pts),
            replay_end,
            exact_cfr: true,
        },
        audio: audio_timeline,
        container: ContainerTimelineV2 {
            start_seconds: parse_probe_decimal(&document.format.start_time, "container start")?,
            duration_seconds: parse_probe_decimal(&document.format.duration, "container duration")?,
        },
        producer: expectations.producer,
        capture: expectations.capture,
    };
    timeline.validate()?;
    Ok(timeline)
}

fn parse_probe_i64(input: &str, field: &'static str) -> Result<i64, ReplayTimeError> {
    parse_signed_decimal(input, field, i64::MIN, i64::MAX).map_err(|_| {
        ReplayTimeError::InvalidMediaProbe {
            reason: "probe integer is malformed or out of range",
        }
    })
}

fn parse_probe_u64(input: &str, field: &'static str) -> Result<u64, ReplayTimeError> {
    parse_unsigned_decimal(input, field, u64::MAX).map_err(|_| ReplayTimeError::InvalidMediaProbe {
        reason: "probe unsigned integer is malformed or out of range",
    })
}

fn parse_probe_rational(input: &str, field: &'static str) -> Result<Rational, ReplayTimeError> {
    let (numerator, denominator) =
        input
            .split_once('/')
            .ok_or(ReplayTimeError::InvalidMediaProbe {
                reason: "probe rational is missing its denominator",
            })?;
    let numerator = parse_probe_i64(numerator, field)?;
    let denominator = parse_probe_u64(denominator, field)?;
    Rational::positive(numerator, denominator, field).map_err(|_| {
        ReplayTimeError::InvalidMediaProbe {
            reason: "probe rational is not positive",
        }
    })
}

fn parse_probe_decimal(input: &str, field: &'static str) -> Result<Rational, ReplayTimeError> {
    let (negative, unsigned) = input
        .strip_prefix('-')
        .map_or((false, input), |value| (true, value));
    if unsigned.is_empty() || input.starts_with('+') {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "probe decimal is malformed",
        });
    }
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 9
    {
        return Err(ReplayTimeError::InvalidMediaProbe {
            reason: "probe decimal is malformed or too precise",
        });
    }
    let denominator = 10_u64
        .checked_pow(
            u32::try_from(fraction.len()).map_err(|_| ReplayTimeError::Overflow {
                operation: "converting probe decimal precision",
            })?,
        )
        .ok_or(ReplayTimeError::Overflow {
            operation: "building probe decimal denominator",
        })?;
    let whole_value = parse_unsigned_decimal(whole, field, u64::MAX).map_err(|_| {
        ReplayTimeError::InvalidMediaProbe {
            reason: "probe decimal whole part is invalid",
        }
    })?;
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u64>()
            .map_err(|_| ReplayTimeError::InvalidMediaProbe {
                reason: "probe decimal fraction is invalid",
            })?
    };
    let magnitude = u128::from(whole_value)
        .checked_mul(u128::from(denominator))
        .and_then(|value| value.checked_add(u128::from(fraction_value)))
        .ok_or(ReplayTimeError::Overflow {
            operation: "combining probe decimal",
        })?;
    let magnitude = i128::try_from(magnitude).map_err(|_| ReplayTimeError::Overflow {
        operation: "converting probe decimal",
    })?;
    let signed = if negative { -magnitude } else { magnitude };
    let numerator = i64::try_from(signed).map_err(|_| ReplayTimeError::InvalidMediaProbe {
        reason: "probe decimal is outside the supported range",
    })?;
    Rational::new(numerator, denominator)
}

fn map_pts_between_time_bases(
    value_pts: i64,
    value_time_base: Rational,
    origin_pts: i64,
    origin_time_base: Rational,
) -> Result<SignedReplayTick, ReplayTimeError> {
    let value_ticks = checked_scale(
        i128::from(value_pts),
        i128::from(value_time_base.numerator())
            .checked_mul(i128::from(REPLAY_TICKS_PER_SECOND))
            .ok_or(ReplayTimeError::Overflow {
                operation: "building value PTS mapping",
            })?,
        i128::from(value_time_base.denominator()),
        RoundingMode::Exact,
    )?;
    let origin_ticks = checked_scale(
        i128::from(origin_pts),
        i128::from(origin_time_base.numerator())
            .checked_mul(i128::from(REPLAY_TICKS_PER_SECOND))
            .ok_or(ReplayTimeError::Overflow {
                operation: "building origin PTS mapping",
            })?,
        i128::from(origin_time_base.denominator()),
        RoundingMode::Exact,
    )?;
    let mapped = value_ticks
        .checked_sub(origin_ticks)
        .ok_or(ReplayTimeError::Overflow {
            operation: "subtracting media presentation origins",
        })?;
    SignedReplayTick::new(
        i64::try_from(mapped).map_err(|_| ReplayTimeError::OutOfRange {
            field: "mapped_media_pts",
        })?,
    )
}

/// Capture-clock anchors retained only as named provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureAnchorsV2 {
    /// First accepted capture QPC value in 100-nanosecond units.
    #[serde(
        deserialize_with = "deserialize_i64_decimal",
        serialize_with = "serialize_i64_decimal"
    )]
    pub first_qpc_100ns: i64,
    /// RFC 3339 wall-clock label for the recording start.
    pub recorded_at: String,
}

fn serialize_i64_decimal<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serialize_display(*value, serializer)
}

/// Complete v2 media-owned replay timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawMediaTimelineV2", deny_unknown_fields)]
pub struct MediaTimelineV2 {
    /// Contract schema version. Always [`REPLAY_TIME_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Fixed canonical scale. Always [`REPLAY_TICKS_PER_SECOND`].
    #[serde(
        deserialize_with = "deserialize_u64_decimal",
        serialize_with = "serialize_u64_decimal"
    )]
    pub replay_ticks_per_second: u64,
    /// Opaque media-generation identity.
    pub media_id: MediaId,
    /// Authoritative video facts.
    pub video: VideoTimelineV2,
    /// Audio mapping into the video-owned timeline.
    pub audio: AudioTimelineV2,
    /// Non-authoritative container facts.
    pub container: ContainerTimelineV2,
    /// Recorder-owned expected cadence and provenance.
    pub producer: ProducerEvidenceV2,
    /// Optional named capture-clock provenance.
    pub capture: Option<CaptureAnchorsV2>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMediaTimelineV2 {
    schema_version: u32,
    #[serde(deserialize_with = "deserialize_u64_decimal")]
    replay_ticks_per_second: u64,
    media_id: MediaId,
    video: VideoTimelineV2,
    audio: AudioTimelineV2,
    container: ContainerTimelineV2,
    producer: ProducerEvidenceV2,
    capture: Option<CaptureAnchorsV2>,
}

impl TryFrom<RawMediaTimelineV2> for MediaTimelineV2 {
    type Error = ReplayTimeError;

    fn try_from(raw: RawMediaTimelineV2) -> Result<Self, Self::Error> {
        let timeline = Self {
            schema_version: raw.schema_version,
            replay_ticks_per_second: raw.replay_ticks_per_second,
            media_id: raw.media_id,
            video: raw.video,
            audio: raw.audio,
            container: raw.container,
            producer: raw.producer,
            capture: raw.capture,
        };
        timeline.validate()?;
        Ok(timeline)
    }
}

fn serialize_u64_decimal<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serialize_display(*value, serializer)
}

impl MediaTimelineV2 {
    /// Validates all cross-field v2 timeline invariants.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the schema, scale, cadence, coverage, audio,
    /// producer, or container facts contradict the canonical contract.
    pub fn validate(&self) -> Result<(), ReplayTimeError> {
        if self.schema_version != REPLAY_TIME_SCHEMA_VERSION {
            return Err(ReplayTimeError::UnsupportedSchemaVersion {
                found: self.schema_version,
            });
        }
        if self.replay_ticks_per_second != REPLAY_TICKS_PER_SECOND {
            return Err(ReplayTimeError::ReplayScaleMismatch {
                found: self.replay_ticks_per_second,
            });
        }
        if self.video.codec.trim().is_empty() || self.producer.backend.trim().is_empty() {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "codec and backend identifiers must be nonempty",
            });
        }
        if !self.video.time_base.is_positive()
            || !self.video.frame_rate.is_positive()
            || !self.producer.expected_frame_rate.is_positive()
            || !self.container.duration_seconds.is_positive()
        {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "time base, frame rates, and duration must be positive",
            });
        }
        if !self.video.exact_cfr || self.video.frame_count.get() == 0 {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "video must have a nonempty proven exact-CFR grid",
            });
        }
        if self.video.frame_rate != self.producer.expected_frame_rate
            || self.video.frame_count != self.producer.expected_frame_count
        {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "producer evidence does not match finalized video",
            });
        }
        let expected_replay_end = frame_boundary_to_replay_tick(
            self.video.frame_count,
            self.video.frame_rate,
            RoundingMode::Exact,
        )?;
        if expected_replay_end != self.video.replay_end {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "video replay coverage does not match its exact frame grid",
            });
        }
        let pts_delta = self
            .video
            .one_past_last_pts
            .get()
            .checked_sub(self.video.first_pts.get())
            .ok_or(ReplayTimeError::Overflow {
                operation: "subtracting video presentation boundaries",
            })?;
        let pts_replay_end =
            media_pts_delta_to_replay_tick(pts_delta, self.video.time_base, RoundingMode::Exact)?;
        if pts_replay_end.get() < 0
            || u64::try_from(pts_replay_end.get()).ok() != Some(self.video.replay_end.get())
        {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "source PTS coverage does not match replay coverage",
            });
        }
        if self.producer.media_runtime_id.trim().is_empty() {
            return Err(ReplayTimeError::InvalidMediaTimeline {
                reason: "media runtime identity must be nonempty",
            });
        }
        match self.audio.present {
            true => {
                let (
                    Some(codec),
                    Some(sample_rate),
                    Some(time_base),
                    Some(_first_pts),
                    Some(start),
                    Some(end),
                ) = (
                    self.audio.codec.as_deref(),
                    self.audio.sample_rate,
                    self.audio.time_base,
                    self.audio.first_pts,
                    self.audio.replay_start,
                    self.audio.replay_end,
                )
                else {
                    return Err(ReplayTimeError::InvalidMediaTimeline {
                        reason: "present audio is missing required facts",
                    });
                };
                if codec.trim().is_empty()
                    || sample_rate == 0
                    || !time_base.is_positive()
                    || end <= start
                {
                    return Err(ReplayTimeError::InvalidMediaTimeline {
                        reason: "present audio facts are invalid",
                    });
                }
            }
            false => {
                if self.audio.codec.is_some()
                    || self.audio.sample_rate.is_some()
                    || self.audio.time_base.is_some()
                    || self.audio.first_pts.is_some()
                    || self.audio.replay_start.is_some()
                    || self.audio.replay_end.is_some()
                {
                    return Err(ReplayTimeError::InvalidMediaTimeline {
                        reason: "absent audio must not carry stream facts",
                    });
                }
            }
        }
        Ok(())
    }

    /// Converts a replay tick to the browser media timeline by adding the
    /// validated video presentation origin.
    #[must_use]
    pub fn browser_seconds_for_replay_tick(&self, tick: ReplayTick) -> f64 {
        self.video.first_pts.get() as f64 * self.video.time_base.to_f64()
            + tick.get() as f64 / REPLAY_TICKS_PER_SECOND as f64
    }

    /// Converts a browser media-time observation to a signed replay tick.
    ///
    /// # Errors
    ///
    /// Returns an error for a nonfinite observation or one outside the supported
    /// signed replay range.
    pub fn replay_tick_for_browser_seconds(
        &self,
        browser_seconds: f64,
    ) -> Result<SignedReplayTick, ReplayTimeError> {
        if !browser_seconds.is_finite() {
            return Err(ReplayTimeError::InvalidFloatingPoint {
                field: "browser_media_seconds",
            });
        }
        let origin = self.video.first_pts.get() as f64 * self.video.time_base.to_f64();
        let scaled = (browser_seconds - origin) * REPLAY_TICKS_PER_SECOND as f64;
        SignedReplayTick::new(round_f64_ties_even(scaled)?)
    }
}

/// Status of a persisted game-to-replay calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationStatus {
    /// A validated affine mapping is available.
    Available,
    /// The Live Client clock has not yielded a sufficient bounded sample window.
    TemporarilyUnavailable,
    /// A prior mapping was invalidated by a frozen, regressing, or restarted clock.
    Invalidated,
}

/// Persisted bounded evidence for the League-game to replay affine mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameCalibrationV2 {
    /// Current calibration state.
    pub status: CalibrationStatus,
    /// Number of accepted request-midpoint observations.
    pub sample_count: u32,
    /// First accepted game-clock sample.
    pub first_game_tick: Option<GameTick>,
    /// Last accepted game-clock sample.
    pub last_game_tick: Option<GameTick>,
    /// Mapping intercept for `replay_tick = game_tick * 48 + intercept`.
    pub replay_tick_at_game_zero: Option<SignedReplayTick>,
    /// Maximum accepted request round-trip time, in game-clock microseconds.
    #[serde(
        deserialize_with = "deserialize_u64_decimal",
        serialize_with = "serialize_u64_decimal"
    )]
    pub maximum_rtt_game_ticks: u64,
    /// Maximum fitted residual, in game-clock microseconds.
    #[serde(
        deserialize_with = "deserialize_u64_decimal",
        serialize_with = "serialize_u64_decimal"
    )]
    pub maximum_residual_game_ticks: u64,
    /// Conservative mapping uncertainty, in game-clock microseconds.
    #[serde(
        deserialize_with = "deserialize_u64_decimal",
        serialize_with = "serialize_u64_decimal"
    )]
    pub uncertainty_game_ticks: u64,
}

impl GameCalibrationV2 {
    /// Validates that evidence fields agree with the reported state.
    ///
    /// # Errors
    ///
    /// Returns an error when an available fit lacks required samples/anchors or
    /// when an unavailable state claims an affine intercept.
    pub fn validate(&self) -> Result<(), ReplayTimeError> {
        match self.status {
            CalibrationStatus::Available => {
                if self.sample_count < 5
                    || self.first_game_tick.is_none()
                    || self.last_game_tick.is_none()
                    || self.replay_tick_at_game_zero.is_none()
                    || self.first_game_tick >= self.last_game_tick
                {
                    return Err(ReplayTimeError::InvalidMediaTimeline {
                        reason: "available calibration lacks a valid sample span",
                    });
                }
            }
            CalibrationStatus::TemporarilyUnavailable | CalibrationStatus::Invalidated => {
                if self.replay_tick_at_game_zero.is_some() {
                    return Err(ReplayTimeError::InvalidMediaTimeline {
                        reason: "unavailable calibration must not carry an affine intercept",
                    });
                }
            }
        }
        Ok(())
    }

    /// Maps one game observation into the bounded media coverage.
    ///
    /// # Errors
    ///
    /// Returns an error if the calibration itself is internally inconsistent or
    /// if checked affine arithmetic overflows.
    pub fn map_game_tick(
        &self,
        game_tick: GameTick,
        video_end: ReplayTick,
    ) -> Result<MappedReplayTime, ReplayTimeError> {
        self.validate()?;
        let Some(intercept) = self.replay_tick_at_game_zero else {
            let reason = match self.status {
                CalibrationStatus::TemporarilyUnavailable => {
                    MappingUnavailableReason::CalibrationUnavailable
                }
                CalibrationStatus::Invalidated => MappingUnavailableReason::CalibrationInvalidated,
                CalibrationStatus::Available => {
                    unreachable!("validated available calibration has an intercept")
                }
            };
            return Ok(MappedReplayTime::Unavailable { reason });
        };
        let mapped = i128::from(game_tick.get())
            .checked_mul(i128::from(REPLAY_TICKS_PER_GAME_TICK))
            .and_then(|value| value.checked_add(i128::from(intercept.get())))
            .ok_or(ReplayTimeError::Overflow {
                operation: "mapping game tick into replay time",
            })?;
        let mapped_i64 = i64::try_from(mapped).map_err(|_| ReplayTimeError::OutOfRange {
            field: "mapped_replay_tick",
        })?;
        if mapped < 0 {
            return Ok(MappedReplayTime::BeforeMedia {
                replay_tick: SignedReplayTick::new(mapped_i64)?,
            });
        }
        if mapped >= i128::from(video_end.get()) {
            return Ok(MappedReplayTime::AfterMedia {
                replay_tick: SignedReplayTick::new(mapped_i64)?,
            });
        }
        Ok(MappedReplayTime::InsideMedia {
            replay_tick: ReplayTick::new(u64::try_from(mapped).map_err(|_| {
                ReplayTimeError::OutOfRange {
                    field: "mapped_replay_tick",
                }
            })?)?,
        })
    }
}

/// Reason a game-clock observation has no current replay mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingUnavailableReason {
    /// No sufficiently precise calibration exists yet.
    CalibrationUnavailable,
    /// A previously usable calibration was invalidated.
    CalibrationInvalidated,
}

/// Explicit result of mapping a League game observation into media time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MappedReplayTime {
    /// No current game-to-replay mapping exists.
    Unavailable {
        /// Why a mapping is unavailable.
        reason: MappingUnavailableReason,
    },
    /// The exact mapped coordinate lies before replay zero.
    BeforeMedia {
        /// Exact signed coordinate retained without saturation.
        replay_tick: SignedReplayTick,
    },
    /// The exact mapped coordinate lies within video coverage.
    InsideMedia {
        /// Exact canonical replay tick.
        replay_tick: ReplayTick,
    },
    /// The exact mapped coordinate lies at or after the video coverage end.
    AfterMedia {
        /// Exact signed coordinate retained without saturation.
        replay_tick: SignedReplayTick,
    },
}

/// An immutable media-bound replay position for future indexes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayTimeReference {
    /// Media generation containing the position.
    pub media_id: MediaId,
    /// Exact position in that media generation.
    pub replay_tick: ReplayTick,
}

/// Reserved mapping needed before an independent media segment can enter match time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentTimeReference {
    /// Independent media-generation identity.
    pub media_id: MediaId,
    /// Match-relative time corresponding to this segment's replay zero.
    pub match_time_at_replay_zero: GameTick,
}

/// Exact half-open source-frame interval bound to one media generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawClipRange", deny_unknown_fields)]
pub struct ClipRange {
    /// Source media generation.
    pub media_id: MediaId,
    /// First included source frame.
    pub start_frame: FrameBoundary,
    /// First excluded source frame.
    pub end_frame_exclusive: FrameBoundary,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClipRange {
    media_id: MediaId,
    start_frame: FrameBoundary,
    end_frame_exclusive: FrameBoundary,
}

impl TryFrom<RawClipRange> for ClipRange {
    type Error = ReplayTimeError;

    fn try_from(raw: RawClipRange) -> Result<Self, Self::Error> {
        if raw.start_frame >= raw.end_frame_exclusive {
            return Err(ReplayTimeError::InvalidClipRange);
        }
        Ok(Self {
            media_id: raw.media_id,
            start_frame: raw.start_frame,
            end_frame_exclusive: raw.end_frame_exclusive,
        })
    }
}

impl ClipRange {
    /// Creates a validated half-open range within `frame_count`.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayTimeError::InvalidClipRange`] for an empty, reversed, or
    /// out-of-coverage range.
    pub fn new(
        media_id: MediaId,
        start_frame: FrameBoundary,
        end_frame_exclusive: FrameBoundary,
        frame_count: FrameBoundary,
    ) -> Result<Self, ReplayTimeError> {
        if start_frame >= end_frame_exclusive || end_frame_exclusive > frame_count {
            return Err(ReplayTimeError::InvalidClipRange);
        }
        Ok(Self {
            media_id,
            start_frame,
            end_frame_exclusive,
        })
    }

    /// Returns the exact number of included frames.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        self.end_frame_exclusive.get() - self.start_frame.get()
    }

    /// Verifies this range against a loaded media timeline.
    ///
    /// # Errors
    ///
    /// Returns an identity or range error for a stale or invalid request.
    pub fn validate_for(&self, timeline: &MediaTimelineV2) -> Result<(), ReplayTimeError> {
        if self.media_id != timeline.media_id {
            return Err(ReplayTimeError::MediaIdentityMismatch);
        }
        if self.start_frame >= self.end_frame_exclusive
            || self.end_frame_exclusive > timeline.video.frame_count
        {
            return Err(ReplayTimeError::InvalidClipRange);
        }
        Ok(())
    }
}

/// Projects an exact replay interval onto the containing half-open frame range.
///
/// The start is floored to the containing frame and the end is ceiled to the
/// first excluded boundary. The result is clamped only to video coverage.
///
/// # Errors
///
/// Returns an error for an empty interval, arithmetic failure, or a result with
/// no covered frame.
pub fn project_replay_interval_to_clip_range(
    timeline: &MediaTimelineV2,
    start: ReplayTick,
    end: ReplayTick,
) -> Result<ClipRange, ReplayTimeError> {
    if start >= end {
        return Err(ReplayTimeError::InvalidClipRange);
    }
    let start_boundary =
        replay_tick_to_frame_boundary(start, timeline.video.frame_rate, RoundingMode::Floor)?;
    let end_boundary =
        replay_tick_to_frame_boundary(end, timeline.video.frame_rate, RoundingMode::Ceil)?;
    let clamped_start =
        FrameBoundary::new(start_boundary.get().min(timeline.video.frame_count.get()))?;
    let clamped_end = FrameBoundary::new(end_boundary.get().min(timeline.video.frame_count.get()))?;
    ClipRange::new(
        timeline.media_id.clone(),
        clamped_start,
        clamped_end,
        timeline.video.frame_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn rational(numerator: i64, denominator: u64) -> Rational {
        Rational::new(numerator, denominator).expect("test rational must be valid")
    }

    fn media_id() -> MediaId {
        MediaId::parse("11111111-2222-4333-8444-555555555555").expect("test media id must be valid")
    }

    fn timeline() -> MediaTimelineV2 {
        MediaTimelineV2 {
            schema_version: REPLAY_TIME_SCHEMA_VERSION,
            replay_ticks_per_second: REPLAY_TICKS_PER_SECOND,
            media_id: media_id(),
            video: VideoTimelineV2 {
                codec: "h264".to_owned(),
                profile: Some("High".to_owned()),
                time_base: rational(1, 15_360),
                first_pts: MediaPts::new(1_536),
                frame_rate: rational(60, 1),
                frame_count: FrameBoundary::new(300).expect("frame count"),
                one_past_last_pts: MediaPts::new(78_336),
                replay_end: ReplayTick::new(240_000_000).expect("replay end"),
                exact_cfr: true,
            },
            audio: AudioTimelineV2 {
                present: true,
                codec: Some("aac".to_owned()),
                sample_rate: Some(48_000),
                time_base: Some(rational(1, 48_000)),
                first_pts: Some(MediaPts::new(4_800)),
                replay_start: Some(SignedReplayTick::new(0).expect("audio start")),
                replay_end: Some(SignedReplayTick::new(240_000_000).expect("audio end")),
            },
            container: ContainerTimelineV2 {
                start_seconds: rational(1, 10),
                duration_seconds: rational(5, 1),
            },
            producer: ProducerEvidenceV2 {
                backend: "native-wgc-d3d11-nvenc".to_owned(),
                expected_frame_rate: rational(60, 1),
                expected_frame_count: FrameBoundary::new(300).expect("expected frames"),
                media_runtime_id: "queueback-test-runtime".to_owned(),
            },
            capture: None,
        }
    }

    fn finalization_expectations() -> FinalizationExpectationsV2 {
        FinalizationExpectationsV2 {
            media_id: media_id(),
            expected_video_codec: "h264".to_owned(),
            expected_audio_codec: Some("aac".to_owned()),
            producer: ProducerEvidenceV2 {
                backend: "native-wgc-d3d11-nvenc".to_owned(),
                expected_frame_rate: rational(60, 1),
                expected_frame_count: FrameBoundary::new(300).expect("expected frames"),
                media_runtime_id: "queueback-test-runtime".to_owned(),
            },
            capture: None,
            require_zero_video_start: false,
        }
    }

    fn ffprobe_summary() -> Value {
        json!({
            "streams": [
                {
                    "index": 0,
                    "codec_type": "video",
                    "codec_name": "h264",
                    "profile": "High",
                    "time_base": "1/15360",
                    "start_pts": "1536",
                    "duration_ts": "76800",
                    "nb_frames": "300",
                    "avg_frame_rate": "60/1",
                    "sample_rate": null
                },
                {
                    "index": 1,
                    "codec_type": "audio",
                    "codec_name": "aac",
                    "profile": "LC",
                    "time_base": "1/48000",
                    "start_pts": "4800",
                    "duration_ts": "240000",
                    "nb_frames": "236",
                    "avg_frame_rate": "0/0",
                    "sample_rate": "48000"
                }
            ],
            "format": { "start_time": "0.100000", "duration": "5.000000" }
        })
    }

    #[test]
    fn canonical_decimal_grammar_rejects_noncanonical_spellings() {
        for invalid in ["", "00", "01", "+1", " 1", "1 ", "1.0", "1e3", "-1"] {
            assert!(
                invalid.parse::<ReplayTick>().is_err(),
                "accepted {invalid:?}"
            );
        }
        for invalid in ["", "-0", "+1", "01", "-01", " 1", "1.0", "1e3"] {
            assert!(
                invalid.parse::<SignedReplayTick>().is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn rational_is_normalized_and_rejects_zero_denominator() {
        assert_eq!(rational(60_000, 2_000), rational(30, 1));
        assert_eq!(rational(-2, 4), rational(-1, 2));
        assert_eq!(rational(0, 99), rational(0, 1));
        assert_eq!(Rational::new(1, 0), Err(ReplayTimeError::ZeroDenominator));
    }

    #[test]
    fn signed_rounding_modes_handle_negative_values_and_ties() {
        assert_eq!(checked_scale(-3, 1, 2, RoundingMode::Floor), Ok(-2));
        assert_eq!(checked_scale(-3, 1, 2, RoundingMode::Ceil), Ok(-1));
        assert_eq!(
            checked_scale(-3, 1, 2, RoundingMode::NearestTiesToEven),
            Ok(-2)
        );
        assert_eq!(
            checked_scale(-1, 1, 2, RoundingMode::NearestTiesToEven),
            Ok(0)
        );
        assert_eq!(
            checked_scale(3, 1, 2, RoundingMode::NearestTiesToEven),
            Ok(2)
        );
        assert_eq!(
            checked_scale(5, 1, 2, RoundingMode::NearestTiesToEven),
            Ok(2)
        );
        assert_eq!(
            checked_scale(1, 1, 2, RoundingMode::Exact),
            Err(ReplayTimeError::InexactConversion)
        );
    }

    #[test]
    fn supported_frame_grids_are_exact() {
        for (numerator, denominator, expected_frame_ticks) in [
            (30, 1, 1_600_000),
            (60, 1, 800_000),
            (30_000, 1_001, 1_601_600),
        ] {
            let rate = rational(numerator, denominator);
            let tick = frame_boundary_to_replay_tick(
                FrameBoundary::new(1).expect("frame one"),
                rate,
                RoundingMode::Exact,
            )
            .expect("supported rate must be exact");
            assert_eq!(tick.get(), expected_frame_ticks);
            assert_eq!(
                replay_tick_to_frame_boundary(tick, rate, RoundingMode::Exact)
                    .expect("round trip")
                    .get(),
                1
            );
        }
    }

    #[test]
    fn media_timeline_validates_nonzero_origin_and_exact_coverage() {
        let timeline = timeline();
        timeline.validate().expect("timeline must validate");
        assert!(
            (timeline
                .browser_seconds_for_replay_tick(ReplayTick::new(48_000_000).expect("one second"))
                - 1.1)
                .abs()
                < 1e-12
        );
        assert_eq!(
            timeline
                .replay_tick_for_browser_seconds(0.1)
                .expect("origin maps")
                .get(),
            0
        );
    }

    #[test]
    fn strict_media_timeline_rejects_unknown_fields_and_schema_v1() {
        let serialized = serde_json::to_value(timeline()).expect("serialize timeline");
        let mut with_unknown = serialized.clone();
        with_unknown
            .as_object_mut()
            .expect("object")
            .insert("legacy_duration_ms".to_owned(), json!(5000));
        assert!(serde_json::from_value::<MediaTimelineV2>(with_unknown).is_err());

        let mut schema_v1 = serialized;
        schema_v1
            .as_object_mut()
            .expect("object")
            .insert("schema_version".to_owned(), json!(1));
        assert!(serde_json::from_value::<MediaTimelineV2>(schema_v1).is_err());
    }

    #[test]
    fn bounded_ffprobe_summary_builds_valid_nonzero_origin_timeline() {
        let timeline = parse_ffprobe_media_timeline(
            &serde_json::to_string(&ffprobe_summary()).expect("probe JSON"),
            finalization_expectations(),
        )
        .expect("valid probe must produce a timeline");
        assert_eq!(timeline.video.first_pts, MediaPts::new(1_536));
        assert_eq!(timeline.video.replay_end.get(), 240_000_000);
        assert_eq!(
            timeline.audio.replay_start,
            Some(SignedReplayTick::new(0).expect("zero audio start"))
        );
        assert_eq!(
            timeline.audio.replay_end,
            Some(SignedReplayTick::new(240_000_000).expect("audio end"))
        );
    }

    #[test]
    fn bounded_ffprobe_summary_accepts_empty_wrappers_and_numeric_timestamps() {
        let mut summary = ffprobe_summary();
        summary
            .as_object_mut()
            .expect("document")
            .insert("programs".to_owned(), json!([]));
        summary
            .as_object_mut()
            .expect("document")
            .insert("stream_groups".to_owned(), json!([]));
        summary["streams"][0]["start_pts"] = json!(1536);
        summary["streams"][0]["duration_ts"] = json!(76800);
        summary["streams"][1]["start_pts"] = json!(4800);
        summary["streams"][1]["duration_ts"] = json!(240000);

        let timeline = parse_ffprobe_media_timeline(
            &serde_json::to_string(&summary).expect("probe JSON"),
            finalization_expectations(),
        )
        .expect("packaged ffprobe shape must produce a timeline");

        assert_eq!(timeline.video.first_pts, MediaPts::new(1_536));
        assert_eq!(timeline.video.replay_end.get(), 240_000_000);
    }

    #[test]
    fn bounded_ffprobe_summary_rejects_nonempty_wrappers() {
        let mut summary = ffprobe_summary();
        summary
            .as_object_mut()
            .expect("document")
            .insert("programs".to_owned(), json!([{}]));

        assert!(
            parse_ffprobe_media_timeline(
                &serde_json::to_string(&summary).expect("probe JSON"),
                finalization_expectations(),
            )
            .is_err()
        );
    }

    #[test]
    fn bounded_ffprobe_summary_rejects_duplicate_stream_indices() {
        let mut summary = ffprobe_summary();
        summary["streams"][1]["index"] = summary["streams"][0]["index"].clone();

        assert!(
            parse_ffprobe_media_timeline(
                &serde_json::to_string(&summary).expect("probe JSON"),
                finalization_expectations(),
            )
            .is_err()
        );
    }

    #[test]
    fn bounded_ffprobe_summary_rejects_unknown_wrong_rate_and_missing_count() {
        let mut unknown = ffprobe_summary();
        unknown
            .as_object_mut()
            .expect("document")
            .insert("packets".to_owned(), json!([]));
        assert!(
            parse_ffprobe_media_timeline(
                &serde_json::to_string(&unknown).expect("unknown JSON"),
                finalization_expectations(),
            )
            .is_err()
        );

        let mut wrong_rate = ffprobe_summary();
        wrong_rate["streams"][0]["avg_frame_rate"] = json!("30/1");
        assert!(
            parse_ffprobe_media_timeline(
                &serde_json::to_string(&wrong_rate).expect("wrong-rate JSON"),
                finalization_expectations(),
            )
            .is_err()
        );

        let mut missing_count = ffprobe_summary();
        missing_count["streams"][0]
            .as_object_mut()
            .expect("video stream")
            .remove("nb_frames");
        assert!(
            parse_ffprobe_media_timeline(
                &serde_json::to_string(&missing_count).expect("missing-count JSON"),
                finalization_expectations(),
            )
            .is_err()
        );
    }

    #[test]
    fn game_mapping_distinguishes_before_inside_after_and_unavailable() {
        let available = GameCalibrationV2 {
            status: CalibrationStatus::Available,
            sample_count: 5,
            first_game_tick: Some(GameTick::new(0).expect("first tick")),
            last_game_tick: Some(GameTick::new(1_000_000).expect("last tick")),
            replay_tick_at_game_zero: Some(SignedReplayTick::new(-24_000_000).expect("intercept")),
            maximum_rtt_game_ticks: 5_000,
            maximum_residual_game_ticks: 2_000,
            uncertainty_game_ticks: 4_500,
        };
        let video_end = ReplayTick::new(240_000_000).expect("video end");
        assert!(matches!(
            available.map_game_tick(GameTick::new(0).expect("before"), video_end),
            Ok(MappedReplayTime::BeforeMedia { .. })
        ));
        assert!(matches!(
            available.map_game_tick(GameTick::new(1_000_000).expect("inside"), video_end),
            Ok(MappedReplayTime::InsideMedia { .. })
        ));
        assert!(matches!(
            available.map_game_tick(GameTick::new(6_000_000).expect("after"), video_end),
            Ok(MappedReplayTime::AfterMedia { .. })
        ));

        let unavailable = GameCalibrationV2 {
            status: CalibrationStatus::TemporarilyUnavailable,
            sample_count: 0,
            first_game_tick: None,
            last_game_tick: None,
            replay_tick_at_game_zero: None,
            maximum_rtt_game_ticks: 0,
            maximum_residual_game_ticks: 0,
            uncertainty_game_ticks: 0,
        };
        assert_eq!(
            unavailable.map_game_tick(GameTick::new(0).expect("tick"), video_end),
            Ok(MappedReplayTime::Unavailable {
                reason: MappingUnavailableReason::CalibrationUnavailable,
            })
        );
    }

    #[test]
    fn clip_projection_uses_floor_start_ceil_end_and_allows_coverage_end() {
        let timeline = timeline();
        let range = project_replay_interval_to_clip_range(
            &timeline,
            ReplayTick::new(800_001).expect("start"),
            timeline.video.replay_end,
        )
        .expect("projected range");
        assert_eq!(range.start_frame.get(), 1);
        assert_eq!(range.end_frame_exclusive, timeline.video.frame_count);
    }

    #[test]
    fn shared_golden_cases_match_rust_implementation_and_schema() {
        let fixture_text = include_str!("../../fixtures/replay-time/v2/golden.json");
        let fixture: Value = serde_json::from_str(fixture_text).expect("golden JSON");
        assert_eq!(fixture["schema_version"], json!(2));

        for case in fixture["frame_cases"].as_array().expect("frame cases") {
            let rate = Rational::new(
                case["frame_rate"]["numerator"]
                    .as_str()
                    .expect("rate numerator")
                    .parse()
                    .expect("integer numerator"),
                case["frame_rate"]["denominator"]
                    .as_str()
                    .expect("rate denominator")
                    .parse()
                    .expect("integer denominator"),
            )
            .expect("valid rate");
            let boundary = case["frame_boundary"]
                .as_str()
                .expect("boundary")
                .parse::<FrameBoundary>()
                .expect("valid boundary");
            let expected = case["expected_replay_tick"]
                .as_str()
                .expect("expected tick")
                .parse::<ReplayTick>()
                .expect("valid expected tick");
            assert_eq!(
                frame_boundary_to_replay_tick(boundary, rate, RoundingMode::Exact),
                Ok(expected),
                "case {}",
                case["id"]
            );
        }

        for invalid in fixture["invalid_unsigned_decimals"]
            .as_array()
            .expect("invalid unsigned")
        {
            assert!(
                invalid
                    .as_str()
                    .expect("string")
                    .parse::<ReplayTick>()
                    .is_err()
            );
        }
        for invalid in fixture["invalid_signed_decimals"]
            .as_array()
            .expect("invalid signed")
        {
            assert!(
                invalid
                    .as_str()
                    .expect("string")
                    .parse::<SignedReplayTick>()
                    .is_err()
            );
        }

        let schema: Value = serde_json::from_str(include_str!(
            "../../fixtures/replay-time/v2/golden.schema.json"
        ))
        .expect("golden schema JSON");
        let validator = jsonschema::validator_for(&schema).expect("valid JSON Schema");
        assert!(validator.validate(&fixture).is_ok());
    }
}
