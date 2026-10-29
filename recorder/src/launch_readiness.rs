//! Passive admission only: no capture/device/encoder objects belong in this module.
use std::time::Duration;

use chronobreak_league_data::observation::{Endpoint, GapReason, Payload, decode};
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info;

use crate::platform::{CaptureTarget, CaptureWindowSnapshot};
use crate::poller::acquisition::Source;
use crate::watcher::LeagueProcess;

const CADENCE: Duration = Duration::from_secs(2);
const STABLE_SPAN: Duration = Duration::from_secs(4);
const MAX_GAP: Duration = Duration::from_secs(5);
const MAX_RTT: Duration = crate::poller::acquisition::NETWORK_DEADLINE;
const MAX_RESPONSE_AGE: Duration = Duration::from_millis(500);
const CLOCK_TOLERANCE_US: u128 = 250_000;

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
    window: CaptureWindowSnapshot,
    game: GameSignal,
    midpoint: Duration,
}

#[derive(Default)]
struct Readiness {
    previous: Option<Sample>,
    first: Duration,
    count: u32,
    offset_bounds: Option<(i128, i128)>,
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
            {
                return self.reset(Decision::ClockUnstable);
            }
        } else {
            self.first = sample.midpoint;
        }
        // A response may sample the game anywhere in its request interval.
        // Intersect all possible offsets, rather than compounding pairwise slack.
        let center = i128::from(sample.game.tick_us) - sample.midpoint.as_micros() as i128;
        let uncertainty = (round_trip.as_micros() + CLOCK_TOLERANCE_US) as i128 / 2;
        let mut bounds = (center - uncertainty, center + uncertainty);
        if let Some(previous) = self.offset_bounds {
            bounds = (bounds.0.max(previous.0), bounds.1.min(previous.1));
            if bounds.0 > bounds.1 {
                return self.reset(Decision::ClockUnstable);
            }
        }
        self.offset_bounds = Some(bounds);
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
    mut window: impl FnMut() -> anyhow::Result<CaptureWindowSnapshot>,
    mut resolve: impl FnMut(&CaptureWindowSnapshot) -> anyhow::Result<CaptureTarget>,
    mut cancellation: watch::Receiver<bool>,
) -> Option<CaptureTarget> {
    let started = Instant::now();
    let mut readiness = Readiness::default();
    let mut last_decision = None;
    loop {
        let observation = async {
            let before = window().ok().filter(|w| w.usable_for(process.pid));
            let Some(before) = before else {
                return (Decision::WindowUnavailable, None, None);
            };
            let request_started = Instant::now();
            let response = source.fetch(Endpoint::GameStats).await;
            let finished = Instant::now();
            let round_trip = finished.saturating_duration_since(request_started);
            let after = window().ok().filter(|w| w.usable_for(process.pid));
            if after != Some(before) {
                return (Decision::WindowChanged, after, None);
            }
            if finished.elapsed() > MAX_RESPONSE_AGE {
                return (Decision::Stale, after, None);
            }
            let Ok(bytes) = response else {
                return (Decision::SignalUnavailable, after, None);
            };
            let Some(game) = game_signal(&bytes) else {
                return (Decision::SignalInvalid, after, None);
            };
            let midpoint = request_started.duration_since(started) + round_trip / 2;
            let decision = readiness.observe(
                Sample {
                    window: before,
                    game,
                    midpoint,
                },
                round_trip,
            );
            if decision != Decision::Ready {
                return (decision, after, None);
            }
            let target = resolve(&before)
                .ok()
                .filter(|target| before.matches_target(target));
            if target.is_none() || window().ok() != Some(before) {
                return (Decision::WindowChanged, after, None);
            }
            if finished.elapsed() > MAX_RESPONSE_AGE {
                return (Decision::Stale, after, None);
            }
            (Decision::Ready, after, target)
        };
        let (decision, final_window, final_target) = tokio::select! {
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
                hwnd = final_window.map(|w| w.hwnd),
                dimensions = ?final_window.map(|w| (w.width, w.height)),
                "League pre-capture readiness");
            last_decision = Some(decision);
        }
        if decision == Decision::Ready {
            if let Some(CaptureTarget {
                source:
                    crate::platform::CaptureSource::WindowsGraphicsCapture {
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
    use crate::platform::CaptureSource;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn process() -> LeagueProcess {
        LeagueProcess {
            pid: 42,
            started_at: 100,
        }
    }

    fn target() -> CaptureWindowSnapshot {
        CaptureWindowSnapshot {
            pid: 42,
            hwnd: 7,
            width: 1920,
            height: 1080,
            dpi: 96,
            monitor: 9,
        }
    }

    fn resolve_target(window: &CaptureWindowSnapshot) -> anyhow::Result<CaptureTarget> {
        Ok(CaptureTarget {
            source: CaptureSource::WindowsGraphicsCapture {
                pid: window.pid,
                generation: 1,
                hwnd: window.hwnd,
                width: window.width & !1,
                height: window.height & !1,
                dpi: window.dpi,
                window_title: None,
                adapter_index: 0,
                adapter_luid: 9,
                adapter_name: String::new(),
                output_name: "fixture".into(),
            },
        })
    }

    fn sample(seconds: u64, game: i64) -> Sample {
        Sample {
            window: target(),
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
    fn request_timing_uncertainty_allows_latency_without_accepting_clock_jumps() {
        let mut gate = Readiness::default();
        let rtt = Duration::from_millis(800);
        for (seconds, tick_us, expected) in [
            (0, 5_000_000, Decision::Collecting),
            (2, 7_600_000, Decision::Collecting),
            (4, 9_200_000, Decision::Ready),
            (6, 13_000_000, Decision::ClockUnstable),
        ] {
            let mut next = sample(seconds, 5);
            next.game.tick_us = tick_us;
            assert_eq!(gate.observe(next, rtt), expected);
        }
    }

    #[test]
    fn clock_tolerance_cannot_accumulate_pairwise_drift() {
        let mut gate = Readiness::default();
        for (seconds, tick_us, expected) in [
            (0, 5_000_000, Decision::Collecting),
            (2, 7_200_000, Decision::Collecting),
            (4, 9_400_000, Decision::ClockUnstable),
        ] {
            let mut next = sample(seconds, 5);
            next.game.tick_us = tick_us;
            assert_eq!(gate.observe(next, Duration::ZERO), expected);
        }
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
                3 => next.window.monitor += 1,
                4 => next.window.dpi += 1,
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
        let tiny = CaptureWindowSnapshot {
            width: 1,
            height: 1,
            ..target()
        };
        assert!(!tiny.usable_for(process().pid));
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
            resolve_target,
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
                if discoveries >= 2 {
                    t.hwnd = 8;
                }
                Ok(t)
            },
            resolve_target,
            cancellation,
        )
        .await
        .unwrap();
        assert_eq!(result.windows_hwnd(), Some(8));
        assert_eq!(source.calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn resolves_adapter_only_at_admission_and_rechecks_window_after_resolution() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: 0,
        };
        let (_cancel, cancellation) = watch::channel(false);
        let hwnd = std::cell::Cell::new(7);
        let mut resolutions = 0;
        let result = wait(
            process(),
            &source,
            || {
                Ok(CaptureWindowSnapshot {
                    hwnd: hwnd.get(),
                    ..target()
                })
            },
            |window| {
                resolutions += 1;
                assert_eq!(source.calls.load(Ordering::SeqCst), resolutions * 3);
                if resolutions == 1 {
                    hwnd.set(8);
                }
                resolve_target(window)
            },
            cancellation,
        )
        .await
        .unwrap();
        assert_eq!(result.windows_hwnd(), Some(8));
        assert_eq!(resolutions, 2);
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
        let waiting = wait(
            process(),
            &Pending,
            || Ok(target()),
            resolve_target,
            cancellation,
        );
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
    async fn monitor_change_during_resolution_requires_fresh_readiness_for_same_hwnd() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: 0,
        };
        let (_cancel, cancellation) = watch::channel(false);
        let monitor = std::cell::Cell::new(target().monitor);
        let mut resolved_monitors = Vec::new();
        let admitted = wait(
            process(),
            &source,
            || {
                Ok(CaptureWindowSnapshot {
                    monitor: monitor.get(),
                    ..target()
                })
            },
            |window| {
                resolved_monitors.push(window.monitor);
                if resolved_monitors.len() == 1 {
                    monitor.set(window.monitor + 1);
                }
                resolve_target(window)
            },
            cancellation,
        )
        .await
        .unwrap();
        assert_eq!(admitted.windows_hwnd(), Some(target().hwnd));
        assert_eq!(resolved_monitors, [target().monitor, target().monitor + 1]);
        assert_eq!(source.calls.load(Ordering::SeqCst), 6);
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_signal_never_admits_even_after_a_long_wait() {
        let source = FixtureSource {
            started: Instant::now(),
            calls: AtomicUsize::new(0),
            fail_first: usize::MAX,
        };
        let (cancel, cancellation) = watch::channel(false);
        let waiting = wait(
            process(),
            &source,
            || Ok(target()),
            |_| panic!("unavailable readiness must not resolve DXGI"),
            cancellation,
        );
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
                if discoveries == 3 {
                    t.width = 1;
                    t.height = 1;
                }
                Ok(t)
            },
            resolve_target,
            cancellation,
        )
        .await;
        assert!(result.is_some());
        assert!(source.started.elapsed() >= Duration::from_secs(8));
        assert_eq!(source.calls.load(Ordering::SeqCst), 4);
    }
}
