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
    pub media_time_base_numerator: u32,
    pub media_time_base_denominator: u32,
    pub first_source_qpc_100ns: i64,
    pub latest_source_qpc_100ns: i64,
    pub scheduled_ticks: u64,
    pub source_discards: u64,
    pub duplicate_ticks: u64,
    pub late_ticks: u64,
    pub catch_up_ticks: u64,
    pub maximum_lateness_100ns: u64,
    pub latest_source_age_100ns: u64,
    pub maximum_source_age_100ns: u64,
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
    late_ticks: u64,
    catch_up_ticks: u64,
    maximum_lateness_100ns: u64,
    latest_source_age_100ns: u64,
    maximum_source_age_100ns: u64,
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
            late_ticks: 0,
            catch_up_ticks: 0,
            maximum_lateness_100ns: 0,
            latest_source_age_100ns: 0,
            maximum_source_age_100ns: 0,
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

    pub(crate) fn record_source_discards(&mut self, count: u64) {
        self.source_discards = self.source_discards.saturating_add(count);
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

    /// Inspect the next scheduled tick without advancing the media clock.
    ///
    /// Callers must commit the returned tick only after the downstream encoder
    /// has accepted the corresponding frame. A temporarily full surface ring
    /// therefore leaves the same media tick due for the next scheduler pass.
    pub fn peek_due(&self, now: Instant) -> Result<Option<NativeCfrTick>> {
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
        Ok(Some(NativeCfrTick {
            index,
            qpc_100ns,
            deadline,
            duplicate,
        }))
    }

    /// Commit a tick after its frame has been accepted by the encoder.
    pub fn commit(&mut self, tick: NativeCfrTick, committed_at: Instant) -> Result<()> {
        let expected = self
            .peek_due(committed_at)?
            .context("native CFR tick was committed before its deadline")?;
        ensure!(
            tick == expected,
            "native CFR commit did not match the current due tick"
        );

        let lateness_100ns = duration_100ns(committed_at.saturating_duration_since(tick.deadline));
        if lateness_100ns > 0 {
            self.late_ticks = self.late_ticks.saturating_add(1);
            self.maximum_lateness_100ns = self.maximum_lateness_100ns.max(lateness_100ns);
        }
        let source_age_100ns = tick
            .qpc_100ns
            .saturating_sub(self.latest_source_qpc_100ns)
            .max(0) as u64;
        self.latest_source_age_100ns = source_age_100ns;
        self.maximum_source_age_100ns = self.maximum_source_age_100ns.max(source_age_100ns);
        if tick.duplicate {
            self.duplicate_ticks = self.duplicate_ticks.saturating_add(1);
        }
        self.source_pending = false;
        self.next_tick_index = self
            .next_tick_index
            .checked_add(1)
            .context("native CFR tick index overflowed")?;
        if self.next_deadline()? <= committed_at {
            self.catch_up_ticks = self.catch_up_ticks.saturating_add(1);
        }
        Ok(())
    }

    #[cfg(test)]
    fn emit_due(&mut self, now: Instant) -> Result<Option<NativeCfrTick>> {
        let Some(tick) = self.peek_due(now)? else {
            return Ok(None);
        };
        self.commit(tick, now)?;
        Ok(Some(tick))
    }

    pub fn telemetry(&self) -> NativeCfrTelemetrySnapshot {
        NativeCfrTelemetrySnapshot {
            frames_per_second: self.frames_per_second,
            media_time_base_numerator: 1,
            media_time_base_denominator: self.frames_per_second,
            first_source_qpc_100ns: self.first_source_qpc_100ns,
            latest_source_qpc_100ns: self.latest_source_qpc_100ns,
            scheduled_ticks: self.next_tick_index,
            source_discards: self.source_discards,
            duplicate_ticks: self.duplicate_ticks,
            late_ticks: self.late_ticks,
            catch_up_ticks: self.catch_up_ticks,
            maximum_lateness_100ns: self.maximum_lateness_100ns,
            latest_source_age_100ns: self.latest_source_age_100ns,
            maximum_source_age_100ns: self.maximum_source_age_100ns,
        }
    }
}

fn duration_100ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos() / 100).unwrap_or(u64::MAX)
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

    #[test]
    fn delayed_ticks_report_lateness_and_catch_up_pressure() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        let now = anchor + tick_duration(3, 60).unwrap();

        clock.emit_due(now).unwrap().unwrap();

        let telemetry = clock.telemetry();
        assert_eq!(telemetry.late_ticks, 1);
        assert_eq!(telemetry.catch_up_ticks, 1);
        assert_eq!(telemetry.maximum_lateness_100ns, 500_000);
    }

    #[test]
    fn peeking_a_due_tick_does_not_advance_or_record_lateness() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        let now = anchor + tick_duration(3, 60).unwrap();

        let first = clock.peek_due(now).unwrap().unwrap();
        let second = clock.peek_due(now).unwrap().unwrap();

        assert_eq!(first, second);
        assert_eq!(first.index, 0);
        assert_eq!(clock.telemetry().scheduled_ticks, 0);
        assert_eq!(clock.telemetry().late_ticks, 0);
        assert_eq!(clock.next_deadline().unwrap(), anchor);

        clock.commit(first, now).unwrap();
        assert_eq!(clock.telemetry().scheduled_ticks, 1);
        assert_eq!(clock.telemetry().late_ticks, 1);
    }

    #[test]
    fn stale_or_mismatched_ticks_cannot_be_committed() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        let tick = clock.peek_due(anchor).unwrap().unwrap();
        clock.commit(tick, anchor).unwrap();

        assert!(clock.commit(tick, anchor).is_err());
        assert_eq!(clock.telemetry().scheduled_ticks, 1);
    }

    #[test]
    fn selected_source_age_is_separate_from_media_tick_time() {
        let anchor = Instant::now();
        let mut clock = NativeCfrClock::start(60, 1_000_000, anchor).unwrap();
        clock.emit_due(anchor).unwrap().unwrap();
        clock.observe_source(1_050_000).unwrap();

        clock
            .emit_due(anchor + tick_duration(1, 60).unwrap())
            .unwrap()
            .unwrap();

        let telemetry = clock.telemetry();
        assert_eq!(telemetry.latest_source_age_100ns, 116_666);
        assert_eq!(telemetry.maximum_source_age_100ns, 116_666);
        assert_eq!(telemetry.media_time_base_numerator, 1);
        assert_eq!(telemetry.media_time_base_denominator, 60);
    }

    #[test]
    fn deterministic_source_rates_preserve_the_sixty_hz_media_time_base() {
        for source_rate in [60_u32, 144, 240] {
            let (arrivals, telemetry) = simulate_source_rate(source_rate, 60);
            assert_eq!(telemetry.scheduled_ticks, 60, "source rate {source_rate}");
            assert_eq!(telemetry.duplicate_ticks, 0, "source rate {source_rate}");
            assert_eq!(
                telemetry.source_discards,
                arrivals.saturating_sub(telemetry.scheduled_ticks),
                "source rate {source_rate}"
            );
            assert_eq!(telemetry.media_time_base_numerator, 1);
            assert_eq!(telemetry.media_time_base_denominator, 60);
        }
    }

    fn simulate_source_rate(
        source_rate: u32,
        output_ticks: u64,
    ) -> (u64, NativeCfrTelemetrySnapshot) {
        let anchor = Instant::now();
        let first_qpc = 10_000_000_i64;
        let mut clock = NativeCfrClock::start(60, first_qpc, anchor).unwrap();
        let mut arrivals = 1_u64;
        let mut source_index = 1_u64;

        for tick_index in 0..output_ticks {
            let tick_nanos = u128::from(tick_index) * NANOS_PER_SECOND / 60;
            loop {
                let source_nanos =
                    u128::from(source_index) * NANOS_PER_SECOND / u128::from(source_rate);
                if source_nanos > tick_nanos {
                    break;
                }
                let source_offset =
                    u128::from(source_index) * QPC_100NS_PER_SECOND / u128::from(source_rate);
                let source_qpc =
                    i64::try_from(i128::from(first_qpc) + i128::try_from(source_offset).unwrap())
                        .unwrap();
                clock.observe_source(source_qpc).unwrap();
                arrivals = arrivals.saturating_add(1);
                source_index = source_index.saturating_add(1);
            }
            clock
                .emit_due(anchor + tick_duration(tick_index, 60).unwrap())
                .unwrap()
                .unwrap();
        }

        (arrivals, clock.telemetry())
    }
}
