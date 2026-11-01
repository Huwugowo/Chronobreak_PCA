//! Passive window admission only: no capture/device/encoder objects belong here.
use std::time::Duration;

use anyhow::Result;
use tokio::sync::watch;
use tokio::time::Instant;
use tracing::info;

use crate::platform::{CaptureTarget, CaptureWindowSnapshot};
use crate::watcher::LeagueProcess;

const CADENCE: Duration = Duration::from_secs(2);
const MAX_GAP: Duration = Duration::from_secs(5);
const MAX_RESOLUTION_AGE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    WindowUnavailable,
    WindowChanged,
    Stale,
    Collecting,
    Ready,
}

struct Sample {
    window: CaptureWindowSnapshot,
    at: Duration,
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

    fn restart(&mut self, sample: Sample, reason: Decision) -> Decision {
        self.first = sample.at;
        self.count = 1;
        self.previous = Some(sample);
        reason
    }

    fn observe(&mut self, sample: Sample) -> Decision {
        let Some(previous) = &self.previous else {
            return self.restart(sample, Decision::Collecting);
        };
        if sample.window != previous.window {
            return self.restart(sample, Decision::WindowChanged);
        }
        let gap = sample.at.saturating_sub(previous.at);
        if gap.is_zero() || gap > MAX_GAP {
            return self.restart(sample, Decision::Stale);
        }
        self.count = self.count.saturating_add(1);
        let ready = self.count >= 2 && sample.at.saturating_sub(self.first) >= CADENCE;
        self.previous = Some(sample);
        if ready {
            Decision::Ready
        } else {
            Decision::Collecting
        }
    }
}

/// Returns a freshly revalidated target, or None on cancellation. Expected
/// window absence is supplied as None; real observation/adapter errors propagate.
/// Gameplay telemetry is deliberately not a dependency of capture admission.
pub(crate) async fn wait(
    process: LeagueProcess,
    mut window: impl FnMut() -> Result<Option<CaptureWindowSnapshot>>,
    mut resolve: impl FnMut(&CaptureWindowSnapshot) -> Result<CaptureTarget>,
    mut cancellation: watch::Receiver<bool>,
) -> Result<Option<CaptureTarget>> {
    let started = Instant::now();
    let mut readiness = Readiness::default();
    let mut last_decision = None;
    loop {
        if *cancellation.borrow() || cancellation.has_changed().is_err() {
            return Ok(None);
        }
        let final_window = window()?.filter(|w| w.usable_for(process.pid));
        let observed = Instant::now();
        let mut decision = match final_window {
            Some(window) => readiness.observe(Sample {
                window,
                at: observed.duration_since(started),
            }),
            None => readiness.reset(Decision::WindowUnavailable),
        };
        let mut final_target = None;
        if decision == Decision::Ready {
            let snapshot = final_window.expect("ready observation has a usable window");
            let target = resolve(&snapshot)?;
            if !snapshot.matches_target(&target) || window()? != Some(snapshot) {
                decision = readiness.reset(Decision::WindowChanged);
            } else if observed.elapsed() > MAX_RESOLUTION_AGE {
                decision = readiness.reset(Decision::Stale);
            } else {
                final_target = Some(target);
            }
        }
        // Cancellation may arrive during synchronous window/adapter queries.
        if *cancellation.borrow() || cancellation.has_changed().is_err() {
            return Ok(None);
        }
        if last_decision != Some(decision) {
            info!(pid = process.pid, process_started_at = process.started_at,
                elapsed_ms = started.elapsed().as_millis() as u64,
                ?decision, samples = readiness.count,
                hwnd = final_window.map(|w| w.hwnd),
                dimensions = ?final_window.map(|w| (w.width, w.height)),
                dpi = final_window.map(|w| w.dpi),
                monitor = final_window.map(|w| w.monitor),
                "League pre-capture window readiness");
            last_decision = Some(decision);
        }
        if let Some(target) = final_target {
            if let crate::platform::CaptureSource::WindowsGraphicsCapture {
                generation,
                adapter_luid,
                ..
            } = &target.source
            {
                info!(
                    generation,
                    adapter_luid, "admitting verified capture descriptor"
                );
            }
            return Ok(Some(target));
        }
        tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => return Ok(None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::CaptureSource;
    use std::cell::Cell;

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

    fn resolve_target(window: &CaptureWindowSnapshot) -> Result<CaptureTarget> {
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

    fn sample(seconds: u64) -> Sample {
        Sample {
            window: target(),
            at: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn requires_matching_observations_spanning_two_seconds() {
        let mut gate = Readiness::default();
        assert_eq!(gate.observe(sample(0)), Decision::Collecting);
        assert_eq!(gate.observe(sample(1)), Decision::Collecting);
        assert_eq!(gate.observe(sample(2)), Decision::Ready);
    }

    #[test]
    fn identity_geometry_and_observer_gaps_restart_stability() {
        for change in 0..8 {
            let mut gate = Readiness::default();
            gate.observe(sample(0));
            let mut next = sample(2);
            match change {
                0 => next.window.hwnd += 1,
                1 => next.window.pid += 1,
                2 => next.window.width += 2,
                3 => next.window.height += 2,
                4 => next.window.monitor += 1,
                5 => next.window.dpi += 1,
                6 => next.at = Duration::ZERO,
                _ => next.at = MAX_GAP + Duration::from_secs(1),
            }
            let at = next.at;
            let window = next.window;
            assert!(matches!(
                gate.observe(next),
                Decision::WindowChanged | Decision::Stale
            ));
            assert_eq!(gate.count, 1);
            assert_eq!(
                gate.observe(Sample {
                    window,
                    at: at + CADENCE
                }),
                Decision::Ready
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn stable_window_admits_without_any_live_client_source() {
        let started = Instant::now();
        let (_cancel, cancellation) = watch::channel(false);
        let mut discoveries = 0;
        let admitted = wait(
            process(),
            || {
                discoveries += 1;
                Ok(Some(target()))
            },
            |window| {
                assert_eq!(started.elapsed(), CADENCE);
                resolve_target(window)
            },
            cancellation,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(admitted.windows_hwnd(), Some(target().hwnd));
        assert_eq!(discoveries, 3); // Two observations plus post-DXGI revalidation.
        assert_eq!(started.elapsed(), CADENCE);
    }

    #[tokio::test(start_paused = true)]
    async fn unavailable_tiny_and_wrong_process_windows_require_fresh_stability() {
        for invalid in [
            None,
            Some(CaptureWindowSnapshot {
                width: 1,
                height: 1,
                ..target()
            }),
            Some(CaptureWindowSnapshot {
                pid: process().pid + 1,
                ..target()
            }),
        ] {
            let started = Instant::now();
            let (_cancel, cancellation) = watch::channel(false);
            let mut discoveries = 0;
            let admitted = wait(
                process(),
                || {
                    discoveries += 1;
                    Ok(if discoveries == 2 {
                        invalid
                    } else {
                        Some(target())
                    })
                },
                resolve_target,
                cancellation,
            )
            .await
            .unwrap();
            assert!(admitted.is_some());
            assert_eq!(started.elapsed(), Duration::from_secs(6));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn resolution_rechecks_window_and_monitor_then_requires_fresh_stability() {
        for change_monitor in [false, true] {
            let started = Instant::now();
            let (_cancel, cancellation) = watch::channel(false);
            let snapshot = Cell::new(target());
            let mut resolutions = 0;
            let admitted = wait(
                process(),
                || Ok(Some(snapshot.get())),
                |window| {
                    resolutions += 1;
                    if resolutions == 1 {
                        let mut replacement = snapshot.get();
                        if change_monitor {
                            replacement.monitor += 1;
                        } else {
                            replacement.hwnd += 1;
                        }
                        snapshot.set(replacement);
                    }
                    resolve_target(window)
                },
                cancellation,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(resolutions, 2);
            assert_eq!(started.elapsed(), Duration::from_secs(6));
            assert_eq!(admitted.windows_hwnd(), Some(snapshot.get().hwnd));
        }
    }

    #[tokio::test]
    async fn slow_resolution_requires_fresh_observations() {
        let (_cancel, cancellation) = watch::channel(false);
        let mut resolutions = 0;
        let admitted = wait(
            process(),
            || Ok(Some(target())),
            |window| {
                resolutions += 1;
                if resolutions == 1 {
                    // Model a slow synchronous query without graphics resources.
                    std::thread::sleep(MAX_RESOLUTION_AGE + Duration::from_millis(10));
                }
                resolve_target(window)
            },
            cancellation,
        )
        .await
        .unwrap();
        assert!(admitted.is_some());
        assert_eq!(resolutions, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn process_exit_during_readiness_cancels_without_resolving() {
        let (cancel, cancellation) = watch::channel(false);
        let started = Instant::now();
        let mut observations = 0;
        let admitted = wait(
            process(),
            || {
                observations += 1;
                if observations == 2 {
                    cancel.send(true).unwrap();
                    return Ok(None);
                }
                Ok(Some(target()))
            },
            |_| panic!("exited process must not resolve an adapter"),
            cancellation,
        )
        .await
        .unwrap();
        assert!(admitted.is_none());
        assert_eq!(started.elapsed(), CADENCE);
        // A restarted process gets its own fresh wait/reducer.
        let restarted = LeagueProcess {
            started_at: process().started_at + 1,
            ..process()
        };
        let (_cancel, cancellation) = watch::channel(false);
        assert!(
            wait(
                restarted,
                || Ok(Some(target())),
                resolve_target,
                cancellation
            )
            .await
            .unwrap()
            .is_some()
        );
        assert_eq!(started.elapsed(), CADENCE * 2);
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_interrupts_wait_and_final_admission() {
        let (cancel, cancellation) = watch::channel(false);
        let waiting = wait(
            process(),
            || Ok(None),
            |_| panic!("no window"),
            cancellation,
        );
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_secs(1), &mut waiting)
                .await
                .is_err()
        );
        cancel.send(true).unwrap();
        assert!(waiting.await.unwrap().is_none());

        let (cancel, cancellation) = watch::channel(false);
        let admitted = wait(
            process(),
            || Ok(Some(target())),
            |window| {
                cancel.send(true).unwrap();
                resolve_target(window)
            },
            cancellation,
        )
        .await
        .unwrap();
        assert!(admitted.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn observation_and_adapter_errors_propagate_without_retry() {
        let (_cancel, cancellation) = watch::channel(false);
        let error = wait(
            process(),
            || anyhow::bail!("process query failed"),
            |_| panic!("failed observation must not resolve"),
            cancellation,
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "process query failed");

        let (_cancel, cancellation) = watch::channel(false);
        let mut resolutions = 0;
        let error = wait(
            process(),
            || Ok(Some(target())),
            |_| {
                resolutions += 1;
                anyhow::bail!("DXGI failure")
            },
            cancellation,
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "DXGI failure");
        assert_eq!(resolutions, 1);
    }
}
