use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};

const QPC_100NS_PER_SECOND: u128 = 10_000_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCfrTick {
    pub index: u64,
    pub qpc_100ns: i64,
    pub deadline: Instant,
    pub duplicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeCfrTelemetrySnapshot {
    pub frames_per_second: u32,
    pub first_source_qpc_100ns: i64,
    pub latest_source_qpc_100ns: i64,
    pub scheduled_ticks: u64,
    pub source_discards: u64,
    pub duplicate_ticks: u64,
}

/// Rational, first-frame-anchored CFR scheduler for the native GPU worker.
///
/// The scheduler uses integer rational offsets instead of repeatedly adding a
/// truncated 16.6666 ms duration. Sixty tick boundaries therefore advance by
/// exactly one second and cannot accumulate long-recording drift.
pub struct NativeCfrClock {
    frames_per_second: u32,
    first_source_qpc_100ns: i64,
    latest_source_qpc_100ns: i64,
    anchor_deadline: Instant,
    next_tick_index: u64,
    source_pending: bool,
    source_discards: u64,
    duplicate_ticks: u64,
}

impl NativeCfrClock {
    pub fn start(
        frames_per_second: u32,
        first_source_qpc_100ns: i64,
        anchor_deadline: Instant,
    ) -> Result<Self> {
        ensure!(
            frames_per_second > 0 && frames_per_second <= 240,
            "native CFR rate must be in 1..=240"
        );
        ensure!(
            first_source_qpc_100ns > 0,
            "native CFR first-source timestamp must be positive"
        );
        Ok(Self {
            frames_per_second,
            first_source_qpc_100ns,
            latest_source_qpc_100ns: first_source_qpc_100ns,
            anchor_deadline,
            next_tick_index: 0,
            source_pending: true,
            source_discards: 0,
            duplicate_ticks: 0,
        })
    }

    /// Record a freshly staged GPU source snapshot. If another source was
    /// already waiting for the same output tick, that older image is an
    /// intentional pre-conversion CFR discard.
    pub fn observe_source(&mut self, qpc_100ns: i64) -> Result<()> {
        ensure!(
            qpc_100ns > self.latest_source_qpc_100ns,
            "native CFR source timestamp did not advance monotonically"
        );
        if self.source_pending {
            self.source_discards = self.source_discards.saturating_add(1);
        }
        self.latest_source_qpc_100ns = qpc_100ns;
        self.source_pending = true;
        Ok(())
    }

    pub fn next_deadline(&self) -> Result<Instant> {
        self.anchor_deadline
            .checked_add(tick_duration(self.next_tick_index, self.frames_per_second)?)
            .context("native CFR deadline exceeded the monotonic clock range")
    }

    pub fn deadline_after(&self, duration: Duration) -> Result<Instant> {
        self.anchor_deadline
            .checked_add(duration)
            .context("native CFR recording deadline exceeded the monotonic clock range")
    }

    /// Produce one scheduled tick only when its monotonic deadline is due.
    pub fn emit_due(&mut self, now: Instant) -> Result<Option<NativeCfrTick>> {
        let deadline = self.next_deadline()?;
        if now < deadline {
            return Ok(None);
        }
        let index = self.next_tick_index;
        let qpc_offset = tick_qpc_offset(index, self.frames_per_second)?;
        let qpc_100ns = i128::from(self.first_source_qpc_100ns)
            .checked_add(i128::from(qpc_offset))
            .and_then(|value| i64::try_from(value).ok())
            .context("native CFR QPC timestamp overflowed")?;
        let duplicate = !self.source_pending;
        if duplicate {
            self.duplicate_ticks = self.duplicate_ticks.saturating_add(1);
        }
        self.source_pending = false;
        self.next_tick_index = self
            .next_tick_index
            .checked_add(1)
            .context("native CFR tick index overflowed")?;
        Ok(Some(NativeCfrTick {
            index,
            qpc_100ns,
            deadline,
            duplicate,
        }))
    }

    pub fn telemetry(&self) -> NativeCfrTelemetrySnapshot {
        NativeCfrTelemetrySnapshot {
            frames_per_second: self.frames_per_second,
            first_source_qpc_100ns: self.first_source_qpc_100ns,
            latest_source_qpc_100ns: self.latest_source_qpc_100ns,
            scheduled_ticks: self.next_tick_index,
            source_discards: self.source_discards,
            duplicate_ticks: self.duplicate_ticks,
        }
    }
}

fn tick_qpc_offset(index: u64, frames_per_second: u32) -> Result<u64> {
    let offset = u128::from(index)
        .checked_mul(QPC_100NS_PER_SECOND)
        .and_then(|value| value.checked_div(u128::from(frames_per_second)))
        .context("native CFR QPC offset overflowed")?;
    u64::try_from(offset).context("native CFR QPC offset exceeds u64")
}

fn tick_duration(index: u64, frames_per_second: u32) -> Result<Duration> {
    let nanos = u128::from(index)
        .checked_mul(NANOS_PER_SECOND)
        .and_then(|value| value.checked_div(u128::from(frames_per_second)))
        .context("native CFR deadline offset overflowed")?;
    Ok(Duration::from_nanos(
        u64::try_from(nanos).context("native CFR deadline offset exceeds Duration")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_hz_uses_exact_rational_tick_boundaries() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 50_000_000, anchor).unwrap();
        let expected_offsets = [0, 166_666, 333_333, 500_000, 666_666, 833_333, 1_000_000];
        for (index, expected) in expected_offsets.into_iter().enumerate() {
            let now = anchor + tick_duration(index as u64, 60).unwrap();
            let tick = clock.emit_due(now).unwrap().unwrap();
            assert_eq!(tick.index, index as u64);
            assert_eq!(tick.qpc_100ns - 50_000_000, expected);
        }
    }

    #[test]
    fn sixty_ticks_advance_exactly_one_second_without_accumulated_rounding() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 90_000_000, anchor).unwrap();
        for index in 0..60 {
            let now = anchor + tick_duration(index, 60).unwrap();
            assert!(clock.emit_due(now).unwrap().is_some());
        }
        assert_eq!(
            clock.next_deadline().unwrap(),
            anchor + Duration::from_secs(1)
        );
    }

    #[test]
    fn superseded_sources_and_missing_sources_are_accounted_separately() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        clock.observe_source(1_000_100).unwrap();
        let fresh = clock.emit_due(anchor).unwrap().unwrap();
        assert!(!fresh.duplicate);

        let duplicate = clock
            .emit_due(anchor + tick_duration(1, 60).unwrap())
            .unwrap()
            .unwrap();
        assert!(duplicate.duplicate);
        let telemetry = clock.telemetry();
        assert_eq!(telemetry.source_discards, 1);
        assert_eq!(telemetry.duplicate_ticks, 1);
    }

    #[test]
    fn source_timestamps_must_advance() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        assert!(clock.observe_source(1_000_000).is_err());
        assert!(clock.observe_source(999_999).is_err());
        clock.observe_source(1_000_001).unwrap();
    }
}
