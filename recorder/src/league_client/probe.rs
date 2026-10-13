//! Sanitized, bounded diagnostic runner. It never authorizes production association.

use std::io::Write;
use std::time::{Duration, Instant};

use chronobreak_replay_time::MediaId;
use serde::Serialize;

use super::association::{CADENCE, PairComparison, Previous, advance, fresh};
use super::transport::{self, Metrics, Transport};
use super::{Comparison, compare, evidence};
use crate::poller::IdentityProbeSource;

pub struct Options {
    pub seconds: u64,
    pub scenario: Scenario,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Startup,
    MidMatch,
    ConsecutiveGames,
    RecorderRestart,
    NormalClosure,
    AppClosed,
    Practice,
    Reconnect,
}

impl Scenario {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "startup" => Self::Startup,
            "mid_match" => Self::MidMatch,
            "consecutive_games" => Self::ConsecutiveGames,
            "recorder_restart" => Self::RecorderRestart,
            "normal_closure" => Self::NormalClosure,
            "app_closed" => Self::AppClosed,
            "practice" => Self::Practice,
            "reconnect" => Self::Reconnect,
            _ => return None,
        })
    }
}

#[derive(Serialize)]
struct Report<'a> {
    schema_version: u32,
    kind: &'static str,
    contract: &'static str,
    // Operator intent only; actual coverage requires observation review.
    scenario_requested: Scenario,
    elapsed_ms: u64,
    live_sequence: Option<u64>,
    live_identity_available: bool,
    live_request_span_ms: Option<u64>,
    lcu_request_span_ms: u64,
    oldest_response_age_ms: Option<u64>,
    fresh: bool,
    credential_epoch_unchanged: bool,
    unavailable: Option<&'static str>,
    comparison: Option<Comparison>,
    provisional_round_coherent: bool,
    observed_coherent_game_count: u64,
    game_changed_since_last_coherent: bool,
    #[serde(flatten)]
    pair: PairComparison,
    lcu_totals: &'a Metrics,
    // Neither a diagnostic pair nor its scenario label authorizes integration.
    feasibility_pass: bool,
    authoritative_confirmed: bool,
}

// Diagnostic lifecycle coverage across unavailable rounds; never used for admission.
// Retains one ID only, never a history or a serialized identifier.
#[derive(Default)]
struct Lifecycle {
    last_coherent_game_id: Option<u64>,
    observed_coherent_game_count: u64,
}

impl Lifecycle {
    fn observe(&mut self, game_id: Option<u64>) -> bool {
        let Some(game_id) = game_id else {
            return false;
        };
        if self.last_coherent_game_id == Some(game_id) {
            return false;
        }
        let changed = self.last_coherent_game_id.is_some();
        self.last_coherent_game_id = Some(game_id);
        self.observed_coherent_game_count += 1;
        changed
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn emit(out: &mut impl Write, value: &impl Serialize) -> Result<(), &'static str> {
    serde_json::to_writer(&mut *out, value).map_err(|_| "report_write")?;
    writeln!(out).map_err(|_| "report_write")?;
    out.flush().map_err(|_| "report_write")
}

/// Runs without recording or application configuration. Only aggregate reports escape.
/// OS file/process workers are always awaited; duration bounds async network work,
/// not an uninterruptible OS call. At most one such worker exists at a time.
pub async fn run(options: Options, out: &mut impl Write) -> Result<(), &'static str> {
    if !(1..=3600).contains(&options.seconds) {
        return Err("probe_options");
    }
    let transport = Transport::new()?;
    let media_id = MediaId::new_v4();
    let mut live_source = IdentityProbeSource::new(media_id.clone()).map_err(|_| "live_client")?;
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(options.seconds);
    let mut metrics = Metrics::default();
    let mut previous: Option<Previous> = None;
    let mut lifecycle = Lifecycle::default();
    let mut rounds = 0_u64;
    let mut cancelled = false;
    let stop = tokio::signal::ctrl_c();
    tokio::pin!(stop);
    loop {
        let live = tokio::select! {
            biased;
            _ = &mut stop => { cancelled = true; break; }
            _ = tokio::time::sleep_until(deadline) => break,
            observation = live_source.observe() => observation,
        };
        let mut report = Report {
            schema_version: 2,
            kind: "round",
            contract: "provisional_v3",
            scenario_requested: options.scenario,
            elapsed_ms: millis(started.elapsed()),
            live_sequence: live.as_ref().map(|o| o.sequence),
            live_identity_available: live.as_ref().is_some_and(|o| o.identity.is_some()),
            live_request_span_ms: live
                .as_ref()
                .map(|o| millis(o.finished.saturating_duration_since(o.started))),
            lcu_request_span_ms: 0,
            oldest_response_age_ms: None,
            fresh: false,
            credential_epoch_unchanged: false,
            unavailable: None,
            comparison: None,
            provisional_round_coherent: false,
            observed_coherent_game_count: 0,
            game_changed_since_last_coherent: false,
            pair: PairComparison::default(),
            lcu_totals: &Metrics::default(),
            feasibility_pass: false,
            authoritative_confirmed: false,
        };
        // Join discovery even if duration expires during OS I/O.
        let discovered = transport::discover().await;
        let lcu_started = Instant::now();
        let mut epoch = None;
        let round = match discovered {
            Ok(discovered) => {
                epoch = Some(discovered.epoch());
                let round = tokio::select! {
                    biased;
                    _ = &mut stop => { cancelled = true; Err("cancelled") }
                    _ = tokio::time::sleep_until(deadline) => Err("duration_elapsed"),
                    round = transport.round(&discovered, &mut metrics) => round,
                };
                report.lcu_request_span_ms = millis(lcu_started.elapsed());
                // Recheck worker remains owned and joined, including at stop.
                match transport::unchanged(discovered).await {
                    Ok(true) => {
                        report.credential_epoch_unchanged = true;
                        round
                    }
                    Ok(false) => Err("credentials_changed"),
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        };
        // Cancellation may arrive while an uninterruptible OS recheck is joined.
        // Latch it before admitting evidence, without polling a completed waiter.
        if !cancelled {
            tokio::select! {
                biased;
                _ = &mut stop => cancelled = true,
                _ = std::future::ready(()) => {}
            }
        }
        let now = Instant::now();
        let mut current = None;
        match round {
            Ok(round) => {
                let identity = live.as_ref().and_then(|o| o.identity.as_ref());
                report.comparison = Some(compare(&round, identity));
                if let Some(live) = &live {
                    report.oldest_response_age_ms = Some(millis(
                        now.saturating_duration_since(live.started.min(lcu_started)),
                    ));
                    // Recheck completion is a conservative upper bound on the LCU window.
                    report.fresh = fresh(live, &media_id, started, lcu_started, now, now)
                        && !cancelled
                        && tokio::time::Instant::now() < deadline;
                    if report.fresh
                        && let Some(evidence) = evidence(&round, identity)
                        && let Some(epoch) = epoch
                    {
                        report.provisional_round_coherent = true;
                        current = Some(Previous {
                            evidence,
                            epoch,
                            live_sequence: live.sequence,
                            live_started: live.started,
                            lcu_started,
                            oldest: live.started.min(lcu_started),
                        });
                    }
                }
            }
            Err(error) => report.unavailable = Some(error),
        }
        report.game_changed_since_last_coherent =
            lifecycle.observe(current.as_ref().map(|p| p.evidence.game_id));
        report.observed_coherent_game_count = lifecycle.observed_coherent_game_count;
        report.pair = advance(&mut previous, current, now);
        rounds += 1;
        report.elapsed_ms = millis(started.elapsed());
        report.lcu_totals = &metrics;
        emit(out, &report)?;
        if cancelled || tokio::time::Instant::now() >= deadline {
            break;
        }
        // Delay after completed work: no catch-up bursts.
        tokio::select! {
            _ = &mut stop => { cancelled = true; break; }
            _ = tokio::time::sleep_until(deadline) => break,
            _ = tokio::time::sleep(CADENCE) => {}
        }
    }
    emit(
        out,
        &serde_json::json!({
            "schema_version": 2, "kind": "summary", "contract": "provisional_v3",
            "scenario_requested": options.scenario,
            "rounds": rounds, "elapsed_ms": millis(started.elapsed()), "cancelled": cancelled,
            "lcu_totals": metrics, "feasibility_pass": false, "authoritative_confirmed": false,
            "observed_coherent_game_count": lifecycle.observed_coherent_game_count,
            "limitation": "diagnostic_only_requires_observed_lifecycle_review"
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league_client::LiveObservation;

    #[test]
    fn lifecycle_counter_survives_unavailable_gap_without_restoring_pair_history() {
        let mut lifecycle = Lifecycle::default();
        assert!(!lifecycle.observe(None));
        assert!(!lifecycle.observe(Some(100)));
        assert!(!lifecycle.observe(Some(100)));
        assert!(!lifecycle.observe(None));
        assert_eq!(lifecycle.observed_coherent_game_count, 1);
        assert!(lifecycle.observe(Some(200)));
        assert_eq!(lifecycle.observed_coherent_game_count, 2);
        assert!(!lifecycle.observe(Some(200)));

        let base = Instant::now();
        let mut previous = Some(current(base, 1, 0, false));
        advance(&mut previous, None, base + Duration::from_secs(1));
        assert!(
            !advance(
                &mut previous,
                Some(current(base, 2, 10, false)),
                base + Duration::from_secs(10)
            )
            .two_rounds_coherent
        );
    }

    fn current(base: Instant, sequence: u64, offset: u64, rotation: bool) -> Previous {
        let (round, live) = crate::league_client::tests::fixture();
        Previous {
            evidence: evidence(&round, Some(&live)).unwrap(),
            epoch: transport::fixture_epoch(rotation),
            live_sequence: sequence,
            live_started: base + Duration::from_secs(offset),
            lcu_started: base + Duration::from_secs(offset),
            oldest: base + Duration::from_secs(offset),
        }
    }

    #[test]
    fn pair_requires_two_coherent_distinct_fresh_same_epoch_rounds() {
        let base = Instant::now();
        let mut previous = None;
        assert!(
            !advance(&mut previous, Some(current(base, 1, 0, false)), base).two_rounds_coherent
        );
        let now = base + Duration::from_secs(10);
        assert!(advance(&mut previous, Some(current(base, 2, 10, false)), now).two_rounds_coherent);

        let result = advance(
            &mut previous,
            Some(current(base, 3, 20, true)),
            base + Duration::from_secs(20),
        );
        assert_eq!(result.credential_epoch_matches_previous, Some(false));
        assert!(!result.two_rounds_coherent);
        // The new epoch needs its own subsequent coherent round.
        assert!(
            advance(
                &mut previous,
                Some(current(base, 4, 30, true)),
                base + Duration::from_secs(30)
            )
            .two_rounds_coherent
        );
        advance(&mut previous, None, base + Duration::from_secs(31));
        assert!(previous.is_none());
        assert!(
            !advance(
                &mut previous,
                Some(current(base, 5, 40, true)),
                base + Duration::from_secs(40)
            )
            .two_rounds_coherent
        );
    }

    #[test]
    fn repeated_sequence_bad_order_short_gap_and_stale_pair_do_not_agree() {
        let base = Instant::now();
        for (sequence, offset, now_offset) in
            [(1, 10, 10), (2, 0, 0), (2, 1, 1), (2, 16, 16), (2, 31, 31)]
        {
            let mut previous = Some(current(base, 1, 0, false));
            let result = advance(
                &mut previous,
                Some(current(base, sequence, offset, false)),
                base + Duration::from_secs(now_offset),
            );
            assert!(!result.two_rounds_coherent);
        }
    }

    #[test]
    fn changed_candidate_resets_pair_instead_of_carrying_old_confirmation() {
        let base = Instant::now();
        for change in 0..4 {
            let mut previous = Some(current(base, 1, 0, false));
            let mut changed = current(base, 2, 10, false);
            match change {
                0 => changed.evidence.game_id += 1,
                1 => changed.evidence.queue_id += 1,
                2 => changed.evidence.riot_id = "Different#TAG".into(),
                _ => changed.evidence.live_mode = "OTHER".into(),
            }
            let result = advance(&mut previous, Some(changed), base + Duration::from_secs(10));
            assert_eq!(result.same_static_evidence_as_previous, Some(false));
            assert!(!result.two_rounds_coherent);
        }
    }

    #[test]
    fn freshness_covers_media_both_request_windows_and_probe_start() {
        let base = Instant::now();
        let media_id = MediaId::new_v4();
        let mut live = LiveObservation {
            media_id: media_id.clone(),
            sequence: 1,
            started: base,
            finished: base + Duration::from_secs(1),
            identity: None,
        };
        let end = base + Duration::from_secs(2);
        assert!(fresh(&live, &media_id, base, base, end, end));
        assert!(!fresh(&live, &MediaId::new_v4(), base, base, end, end));
        assert!(!fresh(&live, &media_id, end, base, end, end));
        assert!(!fresh(&live, &media_id, base, end, base, end));
        assert!(!fresh(
            &live,
            &media_id,
            base,
            base,
            end,
            base + Duration::from_secs(16)
        ));
        live.finished = base + Duration::from_secs(3);
        assert!(!fresh(&live, &media_id, base, base, end, end));
        live.finished = base;
        live.started = base + Duration::from_secs(1);
        assert!(!fresh(&live, &media_id, base, base, end, end));
    }
}
