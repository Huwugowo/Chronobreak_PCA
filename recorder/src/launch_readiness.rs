//! Passive admission only: no capture/device/encoder objects belong in this module.
use std::time::Duration;

use chronobreak_league_data::observation::{Endpoint, GapReason, Payload, decode};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info;

use crate::platform::{CaptureSource, CaptureTarget};
use crate::poller::acquisition::Source;
use crate::watcher::LeagueProcess;

const CADENCE: Duration = Duration::from_secs(2);
const STABLE_SPAN: Duration = Duration::from_secs(4);
const MAX_GAP: Duration = Duration::from_secs(5);
const MAX_RTT: Duration = Duration::from_millis(500);
const CLOCK_TOLERANCE_US: u128 = 250_000;

/// Semantic identity; descriptor generation changes on every discovery.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowKey {
    pid: u32,
    hwnd: u64,
    width: u32,
    height: u32,
    dpi: u32,
    adapter_luid: u64,
    output: String,
}

impl WindowKey {
    fn of(target: &CaptureTarget, process: LeagueProcess) -> Option<Self> {
        match &target.source {
            CaptureSource::WindowsGraphicsCapture {
                pid,
                hwnd,
                width,
                height,
                dpi,
                adapter_luid,
                output_name,
                ..
            } if *pid == process.pid && *hwnd != 0 && *width >= 320 && *height >= 200 => {
                Some(Self {
                    pid: *pid,
                    hwnd: *hwnd,
                    width: *width,
                    height: *height,
                    dpi: *dpi,
                    adapter_luid: *adapter_luid,
                    output: output_name.clone(),
                })
            }
            _ => None,
        }
    }
}

struct GameSignal {
    tick_us: i64,
    mode: String,
    map: u64,
}

fn game_signal(bytes: &[u8]) -> Option<GameSignal> {
    let Payload::GameStats(stats) = decode::live(Endpoint::GameStats, bytes).ok()? else {
        return None;
    };
    Some(GameSignal {
        tick_us: stats.game_tick?.get(),
        mode: stats.game_mode?,
        map: stats.map_number?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    WindowUnavailable,
    WindowChanged,
    SignalUnavailable,
    SignalInvalid,
    SlowResponse,
    ClockUnstable,
    Stale,
    Collecting,
    Ready,
}

struct Sample {
    window: WindowKey,
    game: GameSignal,
    midpoint: Duration,
}

#[derive(Default)]
struct Readiness {
    previous: Option<Sample>,
    first: Duration,
    count: u32,
}

impl Readiness {
    fn reset(&mut self, reason: Decision) -> Decision {
        *self = Self::default();
        reason
    }

    fn observe(&mut self, sample: Sample, round_trip: Duration) -> Decision {
        if round_trip > MAX_RTT {
            return self.reset(Decision::SlowResponse);
        }
        if sample.game.tick_us < 5_000_000 || sample.game.mode.is_empty() {
            return self.reset(Decision::ClockUnstable);
        }
        if let Some(previous) = &self.previous {
            if sample.window != previous.window {
                return self.reset(Decision::WindowChanged);
            }
            let elapsed = sample.midpoint.saturating_sub(previous.midpoint);
            if elapsed > MAX_GAP || elapsed.is_zero() {
                return self.reset(Decision::Stale);
            }
            let game_delta = sample.game.tick_us - previous.game.tick_us;
            if sample.game.mode != previous.game.mode
                || sample.game.map != previous.game.map
                || game_delta <= 0
                || u128::from(game_delta.unsigned_abs()).abs_diff(elapsed.as_micros())
                    > CLOCK_TOLERANCE_US
            {
                return self.reset(Decision::ClockUnstable);
            }
        } else {
            self.first = sample.midpoint;
        }
        self.count = self.count.saturating_add(1);
        let ready = self.count >= 3 && sample.midpoint.saturating_sub(self.first) >= STABLE_SPAN;
        self.previous = Some(sample);
        if ready {
            Decision::Ready
        } else {
            Decision::Collecting
        }
    }
}

/// Returns only a freshly revalidated target. `None` means cancellation, never
/// permission to fall back to time/window-only admission.
pub(crate) async fn wait<S: Source>(
    process: LeagueProcess,
    source: &S,
    mut target: impl FnMut() -> anyhow::Result<CaptureTarget>,
    mut cancellation: watch::Receiver<bool>,
) -> Option<CaptureTarget> {
    let started = Instant::now();
    let mut readiness = Readiness::default();
    let mut last_decision = None;
    loop {
        let observation = async {
            let before = target().ok().and_then(|t| WindowKey::of(&t, process));
            let Some(before) = before else {
                return (Decision::WindowUnavailable, None);
            };
            let request_started = Instant::now();
            let response = source.fetch(Endpoint::GameStats).await;
            let finished = Instant::now();
            let round_trip = finished.saturating_duration_since(request_started);
            let after = target().ok();
            if after
                .as_ref()
                .and_then(|t| WindowKey::of(t, process))
                .as_ref()
                != Some(&before)
            {
                return (Decision::WindowChanged, after);
            }
            if finished.elapsed() > MAX_RTT {
                return (Decision::Stale, after);
            }
            let Ok(bytes) = response else {
                return (Decision::SignalUnavailable, after);
            };
            let Some(game) = game_signal(&bytes) else {
                return (Decision::SignalInvalid, after);
            };
            let midpoint = request_started.duration_since(started) + round_trip / 2;
            (
                readiness.observe(
                    Sample {
                        window: before,
                        game,
                        midpoint,
                    },
                    round_trip,
                ),
                after,
            )
        };
        let (decision, final_target) = tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => return None,
            observed = observation => observed,
        };
        if !matches!(decision, Decision::Collecting | Decision::Ready) {
            readiness.reset(decision);
        }
        if last_decision != Some(decision) {
            // Never include endpoint bodies, player identities or error chains.
            info!(pid = process.pid, process_started_at = process.started_at,
                elapsed_ms = started.elapsed().as_millis() as u64,
                ?decision, samples = readiness.count,
                game_tick_us = readiness.previous.as_ref().map(|sample| sample.game.tick_us),
                hwnd = final_target.as_ref().and_then(CaptureTarget::windows_hwnd),
                dimensions = ?final_target.as_ref().and_then(CaptureTarget::dimensions),
                "League pre-capture readiness");
            last_decision = Some(decision);
        }
        if decision == Decision::Ready {
            if let Some(CaptureTarget {
                source:
                    CaptureSource::WindowsGraphicsCapture {
                        generation,
                        adapter_luid,
                        ..
                    },
            }) = &final_target
            {
                info!(
                    generation,
                    adapter_luid, "admitting verified capture descriptor"
                );
            }
            return final_target;
        }
        tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => return None,
            _ = tokio::time::sleep(CADENCE) => {}
        }
    }
}

async fn cancelled(cancellation: &mut watch::Receiver<bool>) {
    loop {
        if *cancellation.borrow() || cancellation.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) enum ReadinessSource {
    Live(crate::poller::acquisition::HttpSource),
    #[cfg(feature = "native-failure-injection")]
    GeneratedFixture {
        started: Instant,
    },
}

#[cfg(target_os = "windows")]
impl Source for ReadinessSource {
    async fn fetch(&self, endpoint: Endpoint) -> Result<Vec<u8>, GapReason> {
        match self {
            Self::Live(source) => source.fetch(endpoint).await,
            #[cfg(feature = "native-failure-injection")]
            Self::GeneratedFixture { started } => Ok(format!(
                r#"{{"gameTime":{},"gameMode":"CLASSIC","mapNumber":11}}"#,
                10.0 + started.elapsed().as_secs_f64()
            )
            .into_bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn process() -> LeagueProcess {
        LeagueProcess {
            pid: 42,
            started_at: 100,
        }
    }

    fn target() -> CaptureTarget {
        CaptureTarget {
            source: CaptureSource::WindowsGraphicsCapture {
                pid: 42,
                generation: 1,
                hwnd: 7,
                width: 1920,
                height: 1080,
                dpi: 96,
                window_title: None,
                adapter_index: 0,
                adapter_luid: 9,
                adapter_name: String::new(),
                output_name: "fixture".into(),
            },
        }
    }

    fn sample(seconds: u64, game: i64) -> Sample {
        Sample {
            window: WindowKey::of(&target(), process()).unwrap(),
            game: GameSignal {
                tick_us: game * 1_000_000,
                mode: "CLASSIC".into(),
                map: 11,
            },
            midpoint: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn requires_three_advancing_samples_and_four_seconds() {
        let mut gate = Readiness::default();
        assert_eq!(
            gate.observe(sample(0, 5), Duration::ZERO),
            Decision::Collecting
        );
        assert_eq!(
            gate.observe(sample(2, 7), Duration::ZERO),
            Decision::Collecting
        );
        assert_eq!(gate.observe(sample(4, 9), Duration::ZERO), Decision::Ready);
        let mut gate = Readiness::default();
        for seconds in 0..4 {
            assert_eq!(
                gate.observe(sample(seconds, 5 + seconds as i64), Duration::ZERO),
                Decision::Collecting
            );
        }
        assert_eq!(gate.observe(sample(4, 9), Duration::ZERO), Decision::Ready);
    }

    #[test]
    fn rejects_loading_frozen_regressed_jumped_and_stale_clocks() {
        for (seconds, tick, expected) in [
            (2, 0, Decision::ClockUnstable),
            (2, 5, Decision::ClockUnstable),
            (2, 4, Decision::ClockUnstable),
            (2, 20, Decision::ClockUnstable),
            (6, 11, Decision::Stale),
        ] {
            let mut gate = Readiness::default();
            gate.observe(sample(0, 5), Duration::ZERO);
            assert_eq!(
                gate.observe(sample(seconds, tick), Duration::ZERO),
                expected
            );
            assert!(gate.previous.is_none());
        }
        let mut gate = Readiness::default();
        assert_eq!(
            gate.observe(sample(0, 5), MAX_RTT + Duration::from_millis(1)),
            Decision::SlowResponse
        );
    }

    #[test]
    fn resets_for_window_and_game_identity_changes() {
        for change in 0..7 {
            let mut gate = Readiness::default();
            gate.observe(sample(0, 5), Duration::ZERO);
            let mut next = sample(2, 7);
            match change {
                0 => next.window.hwnd += 1,
                1 => next.window.pid += 1,
                2 => next.window.width += 2,
                3 => next.window.adapter_luid += 1,
                4 => next.window.output.push('2'),
                5 => next.game.map += 1,
                _ => next.game.mode = "OTHER".into(),
            }
            assert_ne!(gate.observe(next, Duration::ZERO), Decision::Ready);
            assert!(gate.previous.is_none());
            assert_eq!(
                gate.observe(sample(4, 9), Duration::ZERO),
                Decision::Collecting
            );
        }
        let mut tiny = target();
        if let CaptureSource::WindowsGraphicsCapture { width, height, .. } = &mut tiny.source {
            *width = 1;
            *height = 1;
        }
        assert!(WindowKey::of(&tiny, process()).is_none());
        let mut rediscovered = target();
        if let CaptureSource::WindowsGraphicsCapture { generation, .. } = &mut rediscovered.source {
            *generation = 200;
        }
        assert_eq!(
            WindowKey::of(&target(), process()),
            WindowKey::of(&rediscovered, process())
        );
    }

    #[test]
    fn missing_malformed_or_ambiguous_game_stats_never_mean_ready() {
        for bytes in [
            b"{}".as_slice(),
            br#"{"gameTime":null}"#,
            br#"{"gameTime":5,"gameTime":8}"#,
            b"invalid",
        ] {
            assert!(game_signal(bytes).is_none());
        }
    }

    struct FixtureSource {
        started: Instant,
        calls: AtomicUsize,
        fail_first: usize,
    }
    impl Source for FixtureSource {
        async fn fetch(&self, _: Endpoint) -> Result<Vec<u8>, GapReason> {
            if self.calls.fetch_add(1, Ordering::SeqCst) < self.fail_first {
                return Err(GapReason::Transport);
            }
            Ok(format!(
                r#"{{"gameTime":{},"gameMode":"CLASSIC","mapNumber":11}}"#,
                10.0 + self.started.elapsed().as_secs_f64()
            )
            .into_bytes())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_and_hidden_windows_defer_then_earn_fresh_readiness() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: 3,
        };
        let (_cancel, cancellation) = watch::channel(false);
        let mut discoveries = 0;
        let result = wait(
            process(),
            &source,
            || {
                discoveries += 1;
                if discoveries <= 2 {
                    anyhow::bail!("hidden/closed target")
                }
                Ok(target())
            },
            cancellation,
        )
        .await;
        assert!(result.is_some());
        assert!(source.started.elapsed() >= Duration::from_secs(14));
        assert_eq!(source.calls.load(Ordering::SeqCst), 6);
    }

    #[tokio::test(start_paused = true)]
    async fn replacement_during_request_discards_its_signal_and_returns_fresh_target() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: 0,
        };
        let (_cancel, cancellation) = watch::channel(false);
        let mut discoveries = 0;
        let result = wait(
            process(),
            &source,
            || {
                discoveries += 1;
                let mut t = target();
                if let CaptureSource::WindowsGraphicsCapture {
                    hwnd, generation, ..
                } = &mut t.source
                {
                    if discoveries >= 2 {
                        *hwnd = 8;
                    }
                    *generation = discoveries;
                }
                Ok(t)
            },
            cancellation,
        )
        .await
        .unwrap();
        assert_eq!(result.windows_hwnd(), Some(8));
        assert_eq!(source.calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_pending_request_without_admission() {
        struct Pending;
        impl Source for Pending {
            async fn fetch(&self, _: Endpoint) -> Result<Vec<u8>, GapReason> {
                std::future::pending().await
            }
        }
        let (cancel, cancellation) = watch::channel(false);
        let waiting = wait(process(), &Pending, || Ok(target()), cancellation);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(1), &mut waiting)
                .await
                .is_err()
        );
        cancel.send(true).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), waiting)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_signal_never_admits_even_after_a_long_wait() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: usize::MAX,
        };
        let (cancel, cancellation) = watch::channel(false);
        let waiting = wait(process(), &source, || Ok(target()), cancellation);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_secs(600), &mut waiting)
                .await
                .is_err()
        );
        assert!(source.calls.load(Ordering::SeqCst) <= 301);
        cancel.send(true).unwrap();
        assert!(waiting.await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn temporary_one_pixel_window_resets_readiness_until_restored() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: 0,
        };
        let (_cancel, cancellation) = watch::channel(false);
        let mut discoveries = 0;
        let result = wait(
            process(),
            &source,
            || {
                discoveries += 1;
                let mut t = target();
                if discoveries == 3
                    && let CaptureSource::WindowsGraphicsCapture { width, height, .. } =
                        &mut t.source
                {
                    *width = 1;
                    *height = 1;
                }
                Ok(t)
            },
            cancellation,
        )
        .await;
        assert!(result.is_some());
        assert!(source.started.elapsed() >= Duration::from_secs(8));
        assert_eq!(source.calls.load(Ordering::SeqCst), 4);
    }
}
