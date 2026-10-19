//! Recording-owned acquisition. Optional telemetry never owns video lifecycle.
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chronobreak_league_data::observation::{identity::IdentityRound, *};
use chronobreak_replay_time::MediaId;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{
    PollerSummary,
    acquisition::{self, HttpSource, Sink, Source},
    persistence::{QueueSink, WriterSession},
};
use crate::league_client::{LiveIdentity, LiveObservation, full_identity, sidecar::DirectoryGuard};

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct PollerSession {
    cancellation: watch::Sender<bool>,
    task: Option<JoinHandle<(acquisition::Summary, PollerSummary)>>,
    writer: Option<WriterSession>,
    identity: watch::Receiver<Option<LiveObservation>>,
}

struct RecordingSink {
    queue: QueueSink,
    origin: Instant,
    identity: watch::Sender<Option<LiveObservation>>,
    summary: PollerSummary,
    local: Option<ParticipantId>,
}

impl Sink for RecordingSink {
    fn try_records(&mut self, rows: Vec<Record>) -> Result<(), Vec<Record>> {
        // Stage the small presentation context before transferring ownership.
        // Publish it only after atomic queue admission succeeds.
        let mut local = self.local;
        let mut summary = self.summary.clone();
        let mut identity = None;
        for row in &rows {
            if matches!(row, Record::SourceReset { .. }) {
                local = None;
                summary = PollerSummary::default();
                identity = Some(None);
            }
            if let Record::LocalParticipant { participant, .. } = row {
                local = Some(*participant);
            }
        }
        for row in &rows {
            match row {
                Record::IdentityRound { round } => {
                    identity = Some(live_round(round, self.origin));
                }
                Record::Observation { observation } => {
                    if let Observation::Success { payload, .. } = observation.as_ref() {
                        match payload {
                            Payload::GameStats(gs) => summary.game_mode = gs.game_mode.clone(),
                            Payload::PlayerList(roster) => {
                                if let Some(player) = roster
                                    .iter()
                                    .find(|p| p.participant.is_some() && p.participant == local)
                                {
                                    summary.local_player_summoner_name = player.riot_id.clone();
                                    summary.local_player_champion = player.champion_name.clone();
                                    summary.local_player_team = player.team.clone();
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        self.queue.try_records(rows)?;
        self.local = local;
        self.summary = summary;
        if let Some(identity) = identity {
            self.identity.send_replace(identity);
        }
        Ok(())
    }
}

fn live_round(round: &IdentityRound, origin: Instant) -> Option<LiveObservation> {
    let ap = &round.active_player;
    let start = round.game_stats.as_ref().map_or(ap.timing.start_us, |gs| {
        gs.timing.start_us.min(ap.timing.start_us)
    });
    let end = round.game_stats.as_ref().map_or(ap.timing.end_us, |gs| {
        gs.timing.end_us.max(ap.timing.end_us)
    });
    let identity = round
        .riot_id
        .as_ref()
        .filter(|id| full_identity(id))
        .map(|id| LiveIdentity {
            active: id.clone(),
            map_id: round.map_number.and_then(|n| u32::try_from(n).ok()),
            game_mode: round.game_mode.clone(),
        });
    Some(LiveObservation {
        media_id: ap.source.media_id.clone(),
        sequence: round.sequence,
        started: origin.checked_add(Duration::from_micros(start))?,
        finished: origin.checked_add(Duration::from_micros(end))?,
        identity,
    })
}

impl PollerSession {
    pub(crate) async fn start(
        directory: DirectoryGuard,
        media_id: MediaId,
        video_started: Instant,
        identity_open: watch::Receiver<bool>,
    ) -> Result<Self> {
        let source = HttpSource::local().context("Live Client HTTP initialization")?;
        let scope = SourceEpoch {
            media_id,
            run_id: MediaId::new_v4(),
            epoch: 0,
        };
        let (writer, queue) = WriterSession::start(directory, scope.clone()).await?;
        let (identity_tx, identity) = watch::channel(None);
        let (cancellation, stop) = watch::channel(false);
        let mut sink = RecordingSink {
            queue,
            origin: video_started,
            identity: identity_tx,
            summary: PollerSummary::default(),
            local: None,
        };
        let task = tokio::spawn(async move {
            let summary = acquisition::run(
                Arc::new(source),
                &mut sink,
                scope,
                video_started.into(),
                stop,
                identity_open,
            )
            .await;
            (summary, sink.summary)
        });
        Ok(Self {
            cancellation,
            task: Some(task),
            writer: Some(writer),
            identity,
        })
    }

    pub(crate) fn identity_observations(&self) -> watch::Receiver<Option<LiveObservation>> {
        self.identity.clone()
    }

    pub(crate) async fn stop(mut self) -> PollerSummary {
        self.cancellation.send_replace(true);
        let mut summary = PollerSummary::default();
        if let Some(task) = self.task.as_mut() {
            match tokio::time::timeout(STOP_TIMEOUT, &mut *task).await {
                Ok(Ok((diagnostics, value))) => {
                    tracing::info!(
                        maximum_in_flight = diagnostics.maximum_in_flight,
                        unreported_gaps = diagnostics.unreported_gaps,
                        unreported_source_resets = diagnostics.unreported_source_resets,
                        unreported_identity_rounds = diagnostics.unreported_identity_rounds,
                        "League acquisition stopped"
                    );
                    summary = value;
                }
                Ok(Err(error)) => tracing::warn!(%error, "League acquisition task failed"),
                Err(_) => {
                    task.abort();
                    let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
                    tracing::warn!("League acquisition shutdown deadline exceeded");
                }
            }
        }
        self.task.take();
        if let Some(writer) = self.writer.take() {
            let report = writer.stop().await;
            tracing::info!(
                committed_records = report.committed_records,
                journal_bytes = report.journal_bytes,
                queue_high_water_bytes = report.queue_high_water_bytes,
                rejected_batches = report.rejected_batches,
                checkpoint_failures = report.checkpoint_failures,
                manifest_failures = report.manifest_failures,
                failed = report.failed,
                "League journal writer stopped"
            );
        }
        summary
    }
}

impl Drop for PollerSession {
    fn drop(&mut self) {
        self.cancellation.send_replace(true);
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Standalone diagnostic identity reads use the same bounded GS/AP transport and
/// parser, with no roster requests, persistence or capture ownership.
pub(crate) struct IdentityProbeSource {
    source: HttpSource,
    media_id: MediaId,
    sequence: u64,
}
impl IdentityProbeSource {
    pub(crate) fn new(media_id: MediaId) -> Result<Self> {
        Ok(Self {
            source: HttpSource::local()?,
            media_id,
            sequence: 0,
        })
    }
    pub(crate) async fn observe(&mut self) -> Option<LiveObservation> {
        let started = Instant::now();
        let (ap, gs) = tokio::join!(
            self.source.fetch(Endpoint::ActivePlayer),
            self.source.fetch(Endpoint::GameStats)
        );
        let finished = Instant::now();
        self.sequence = self.sequence.checked_add(1)?;
        let identity = (|| {
            let Payload::ActivePlayer(ap) = decode::live(Endpoint::ActivePlayer, &ap.ok()?).ok()?
            else {
                return None;
            };
            let Payload::GameStats(gs) = decode::live(Endpoint::GameStats, &gs.ok()?).ok()? else {
                return None;
            };
            Some(LiveIdentity {
                active: ap.riot_id.filter(|id| full_identity(id))?,
                map_id: gs.map_number.and_then(|n| u32::try_from(n).ok()),
                game_mode: gs.game_mode,
            })
        })();
        Some(LiveObservation {
            media_id: self.media_id.clone(),
            sequence: self.sequence,
            started,
            finished,
            identity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_session(dropped: tokio::sync::oneshot::Sender<()>) -> PollerSession {
        struct Signal(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Signal {
            fn drop(&mut self) {
                if let Some(tx) = self.0.take() {
                    let _ = tx.send(());
                }
            }
        }
        let signal = Signal(Some(dropped));
        let task = tokio::spawn(async move {
            let _signal = signal;
            std::future::pending::<(acquisition::Summary, PollerSummary)>().await
        });
        PollerSession {
            cancellation: watch::channel(false).0,
            task: Some(task),
            writer: None,
            identity: watch::channel(None).1,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_deadline_aborts_and_reaps_an_unresponsive_acquisition() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let session = pending_session(tx);
        let result = tokio::time::timeout(Duration::from_secs(7), session.stop())
            .await
            .unwrap();
        assert_eq!(result, PollerSummary::default());
        rx.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_session_or_its_stop_future_keeps_acquisition_owned() {
        for during_stop in [false, true] {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let session = pending_session(tx);
            tokio::task::yield_now().await;
            if during_stop {
                let mut stopping = Box::pin(session.stop());
                tokio::select! { biased; _ = &mut stopping => panic!("unresponsive task completed"), _ = tokio::task::yield_now() => {} }
                drop(stopping);
            } else {
                drop(session);
            }
            tokio::time::timeout(Duration::from_secs(1), rx)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[test]
    fn collector_adapter_preserves_independent_request_bounds_and_exact_identity() {
        let origin = Instant::now();
        let evidence = Evidence {
            id: ObservationId(4),
            source: SourceEpoch {
                media_id: MediaId::new_v4(),
                run_id: MediaId::new_v4(),
                epoch: 0,
            },
            endpoint: Endpoint::ActivePlayer,
            timing: RequestTiming {
                start_us: 100,
                end_us: 400,
                received_at: "fixture".into(),
            },
            game_stats: None,
            calibration: None,
        };
        let mut gs = evidence.clone();
        gs.endpoint = Endpoint::GameStats;
        gs.timing.start_us = 50;
        gs.timing.end_us = 500;
        let mut round = IdentityRound {
            sequence: 2,
            active_player: evidence,
            game_stats: Some(gs),
            riot_id: Some("Fixture#TEST".into()),
            map_number: Some(11),
            game_mode: Some("CLASSIC".into()),
        };
        let live = live_round(&round, origin).unwrap();
        assert_eq!(live.sequence, 2);
        assert_eq!(live.started, origin + Duration::from_micros(50));
        assert_eq!(live.finished, origin + Duration::from_micros(500));
        assert_eq!(live.identity.unwrap().active, "Fixture#TEST");
        round.riot_id = Some("Fixture".into());
        assert!(live_round(&round, origin).unwrap().identity.is_none());
    }
}
