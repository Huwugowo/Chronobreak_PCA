//! Pure, recording-scoped provisional identity. No transport, writes or final facts.

use std::time::{Duration, Instant};

use chronobreak_replay_time::MediaId;
use serde::Serialize;

use super::transport::Epoch;
use super::{Evidence, LcuRound, LiveObservation, evidence};

pub(super) const CADENCE: Duration = Duration::from_secs(10);
const FRESHNESS: Duration = Duration::from_secs(15);
const MIN_SEPARATION: Duration = Duration::from_secs(2);
const MAX_SEPARATION: Duration = Duration::from_secs(30);

#[derive(Default, Serialize)]
pub(super) struct PairComparison {
    pub(super) credential_epoch_matches_previous: Option<bool>,
    pub(super) same_game_as_previous: Option<bool>,
    pub(super) same_queue_as_previous: Option<bool>,
    pub(super) same_static_evidence_as_previous: Option<bool>,
    pub(super) distinct_ordered_rounds: bool,
    pub(super) previous_round_still_fresh: bool,
    pub(super) two_rounds_coherent: bool,
}

// One coherent observation only. Sensitive values have no Debug/Serialize path.
pub(super) struct Previous {
    pub(super) evidence: Evidence,
    pub(super) epoch: Epoch,
    pub(super) live_sequence: u64,
    pub(super) live_started: Instant,
    pub(super) lcu_started: Instant,
    pub(super) oldest: Instant,
}

pub(super) fn fresh(
    live: &LiveObservation,
    media_id: &MediaId,
    probe_started: Instant,
    lcu_started: Instant,
    lcu_finished: Instant,
    now: Instant,
) -> bool {
    let oldest = live.started.min(lcu_started);
    live.sequence > 0
        && live.media_id == *media_id
        && oldest >= probe_started
        && live.started <= live.finished
        && live.finished <= now
        && lcu_started <= lcu_finished
        && lcu_finished <= now
        && now.saturating_duration_since(oldest) <= FRESHNESS
}

pub(super) fn advance(
    previous: &mut Option<Previous>,
    current: Option<Previous>,
    now: Instant,
) -> PairComparison {
    let mut result = PairComparison::default();
    let Some(current) = current else {
        *previous = None;
        return result;
    };
    if let Some(previous) = previous.as_ref() {
        result.credential_epoch_matches_previous = Some(current.epoch == previous.epoch);
        result.same_game_as_previous = Some(current.evidence.game_id == previous.evidence.game_id);
        result.same_queue_as_previous =
            Some(current.evidence.queue_id == previous.evidence.queue_id);
        result.same_static_evidence_as_previous = Some(current.evidence == previous.evidence);
        let separation = current
            .lcu_started
            .saturating_duration_since(previous.lcu_started);
        // Latest-only transport may skip a failed or contradictory Live update.
        // A gap therefore starts a new pending pair instead of proving continuity.
        result.distinct_ordered_rounds = current.live_sequence.checked_sub(previous.live_sequence)
            == Some(1)
            && current.live_started > previous.live_started
            && current.lcu_started > previous.lcu_started
            && (MIN_SEPARATION..=MAX_SEPARATION).contains(&separation);
        result.previous_round_still_fresh =
            now.saturating_duration_since(previous.oldest) <= FRESHNESS;
        result.two_rounds_coherent = result.credential_epoch_matches_previous == Some(true)
            && result.same_static_evidence_as_previous == Some(true)
            && result.distinct_ordered_rounds
            && result.previous_round_still_fresh;
    }
    *previous = Some(current);
    result
}

pub(super) struct RoundInput<'a> {
    pub lcu: Option<&'a LcuRound>,
    pub live: Option<&'a LiveObservation>,
    pub epoch: Option<&'a Epoch>,
    pub epoch_unchanged: bool,
    pub lcu_started: Instant,
    pub lcu_finished: Instant,
}

#[derive(Clone, Serialize)]
pub(crate) struct ProvisionalCandidate {
    schema_version: u32,
    status: &'static str,
    media_id: MediaId,
    game_id: String,
    queue_id: u32,
    local_puuid: String,
    local_riot_id: String,
    lcu_map_id: Option<u32>,
    lcu_queue_map_id: Option<u32>,
    lcu_map_mode: Option<String>,
    lcu_queue_mode: String,
    live_map_id: u32,
    live_mode: String,
    evidence: BindingProvenance,
}

impl ProvisionalCandidate {
    pub(super) fn result_binding(&self) -> super::result::Binding {
        super::result::Binding {
            media_id: self.media_id.clone(),
            game_id: self.game_id.clone(),
            puuid: self.local_puuid.clone(),
            riot_id: self.local_riot_id.clone(),
            map_id: self.live_map_id,
            mode: self.live_mode.clone(),
            queue_id: self.queue_id,
        }
    }
    pub(super) fn matches_media(&self, media_id: &MediaId) -> bool {
        self.media_id == *media_id
    }
}

#[cfg(test)]
pub(super) fn fixture_candidate(media_id: MediaId) -> ProvisionalCandidate {
    let base = Instant::now();
    let mut state = Association::new(media_id, base);
    tests::submit(&mut state, base, 1, 0, false, false);
    tests::submit(&mut state, base, 2, 10, false, false);
    state.finish().unwrap()
}

#[derive(Clone, Serialize)]
struct BindingProvenance {
    lcu_session_source: &'static str,
    local_identity_source: &'static str,
    live_source: &'static str,
    // Random observation nonce; credentials/PID/path are never persisted.
    credential_epoch_marker: String,
    first: RoundProvenance,
    second: RoundProvenance,
}

#[derive(Clone, Serialize)]
struct RoundProvenance {
    live_sequence: u64,
    live_started_us: u64,
    live_finished_us: u64,
    lcu_started_us: u64,
    lcu_finished_us: u64,
}

impl RoundProvenance {
    fn new(input: &RoundInput<'_>, started: Instant) -> Option<Self> {
        let live = input.live?;
        let offset = |at: Instant| -> Option<u64> {
            at.checked_duration_since(started)?
                .as_micros()
                .try_into()
                .ok()
        };
        Some(Self {
            live_sequence: live.sequence,
            live_started_us: offset(live.started)?,
            live_finished_us: offset(live.finished)?,
            lcu_started_us: offset(input.lcu_started)?,
            lcu_finished_us: offset(input.lcu_finished)?,
        })
    }
}

pub(super) struct Association {
    media_id: MediaId,
    started: Instant,
    closed: bool,
    pending: Option<Previous>,
    pending_provenance: Option<RoundProvenance>,
    candidate: Option<ProvisionalCandidate>,
}

impl Association {
    pub fn new(media_id: MediaId, started: Instant) -> Self {
        Self {
            media_id,
            started,
            closed: false,
            pending: None,
            pending_provenance: None,
            candidate: None,
        }
    }

    pub fn accepting(&self) -> bool {
        !self.closed && self.candidate.is_none()
    }

    pub fn observe(&mut self, input: RoundInput<'_>, now: Instant) -> bool {
        if !self.accepting() {
            return false;
        }
        let coherent = || {
            let live = input.live?;
            if !input.epoch_unchanged
                || !fresh(
                    live,
                    &self.media_id,
                    self.started,
                    input.lcu_started,
                    input.lcu_finished,
                    now,
                )
            {
                return None;
            }
            Some(Previous {
                evidence: evidence(input.lcu?, live.identity.as_ref())?,
                epoch: input.epoch?.clone(),
                live_sequence: live.sequence,
                live_started: live.started,
                lcu_started: input.lcu_started,
                oldest: live.started.min(input.lcu_started),
            })
        };
        let current = coherent();
        let provenance = current
            .as_ref()
            .and_then(|_| RoundProvenance::new(&input, self.started));
        let first = self.pending_provenance.take();
        let pair = advance(&mut self.pending, current, now);
        self.pending_provenance = provenance;
        if pair.two_rounds_coherent
            && let Some(first) = first
            && let Some(second) = self.pending_provenance.take()
            && let Some(current) = self.pending.take()
        {
            let e = current.evidence;
            self.candidate = Some(ProvisionalCandidate {
                schema_version: 1,
                status: "provisional",
                media_id: self.media_id.clone(),
                game_id: e.game_id.to_string(),
                queue_id: e.queue_id,
                local_puuid: e.puuid,
                local_riot_id: e.riot_id,
                lcu_map_id: e.lcu_map,
                lcu_queue_map_id: e.lcu_queue_map,
                lcu_map_mode: e.lcu_map_mode,
                lcu_queue_mode: e.lcu_queue_mode,
                live_map_id: e.live_map,
                live_mode: e.live_mode,
                evidence: BindingProvenance {
                    lcu_session_source: "/lol-gameflow/v1/session",
                    local_identity_source: "/lol-summoner/v1/current-summoner",
                    live_source: "existing_activeplayer_gamestats_round",
                    credential_epoch_marker: MediaId::new_v4().as_str().to_owned(),
                    first,
                    second,
                },
            });
            return true;
        }
        false
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.pending = None;
        self.pending_provenance = None;
    }

    pub fn finish(&mut self) -> Option<ProvisionalCandidate> {
        self.close();
        self.candidate.take()
    }

    pub(super) fn snapshot(&self) -> Option<ProvisionalCandidate> {
        self.candidate.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn submit(
        state: &mut Association,
        base: Instant,
        sequence: u64,
        offset: u64,
        changed_game: bool,
        rotated: bool,
    ) -> bool {
        let (mut lcu, identity) = crate::league_client::tests::fixture();
        if changed_game {
            lcu.gameflow.game_data.game_id = Some(123);
        }
        let at = base + Duration::from_secs(offset);
        let live = LiveObservation {
            media_id: state.media_id.clone(),
            sequence,
            started: at,
            finished: at,
            identity: Some(identity),
        };
        let epoch = super::super::transport::fixture_epoch(rotated);
        state.observe(
            RoundInput {
                lcu: Some(&lcu),
                live: Some(&live),
                epoch: Some(&epoch),
                epoch_unchanged: true,
                lcu_started: at,
                lcu_finished: at,
            },
            at,
        )
    }

    #[test]
    fn binds_once_preserves_candidate_through_closure_and_never_changes_game() {
        let base = Instant::now();
        let mut state = Association::new(MediaId::new_v4(), base);
        assert!(!submit(&mut state, base, 1, 0, false, false));
        assert!(submit(&mut state, base, 2, 10, false, false));
        assert!(!state.accepting());
        assert!(!submit(&mut state, base, 3, 20, true, false));
        state.close();
        let candidate = state.finish().unwrap();
        let json = serde_json::to_value(candidate).unwrap();
        assert_eq!(json["status"], "provisional");
        assert_eq!(json["game_id"], "9007199254740993");
        assert_eq!(json["queue_id"], 0);
        assert_eq!(json["evidence"]["first"]["live_sequence"], 1);
        assert_eq!(json["evidence"]["second"]["live_sequence"], 2);
        for secret in ["synthetic", "password", "WIN", "LOSS"] {
            assert!(!json.to_string().contains(secret));
        }
        assert!(state.finish().is_none());
    }

    #[test]
    fn incomplete_cancelled_and_changed_rounds_require_a_new_pair() {
        let base = Instant::now();
        let mut state = Association::new(MediaId::new_v4(), base);
        assert!(!submit(&mut state, base, 1, 0, false, false));
        let at = base + Duration::from_secs(1);
        assert!(!state.observe(
            RoundInput {
                lcu: None,
                live: None,
                epoch: None,
                epoch_unchanged: false,
                lcu_started: at,
                lcu_finished: at
            },
            at
        ));
        assert!(!submit(&mut state, base, 2, 10, false, false));
        assert!(!submit(&mut state, base, 3, 20, true, true));
        assert!(submit(&mut state, base, 4, 30, true, true));
        let candidate = state.finish().unwrap();
        assert_eq!(candidate.game_id, "123");

        let mut closed = Association::new(MediaId::new_v4(), base);
        assert!(!submit(&mut closed, base, 1, 0, false, false));
        closed.close();
        assert!(!submit(&mut closed, base, 2, 10, false, false));
        assert!(closed.finish().is_none());
    }

    #[test]
    fn stale_duplicate_or_epoch_changed_round_cannot_bind() {
        let base = Instant::now();
        for (sequence, offset, rotated) in [
            (1, 10, false),
            (3, 10, false),
            (2, 16, false),
            (2, 10, true),
        ] {
            let mut state = Association::new(MediaId::new_v4(), base);
            assert!(!submit(&mut state, base, 1, 0, false, false));
            assert!(!submit(&mut state, base, sequence, offset, false, rotated));
            assert!(state.finish().is_none());
        }
    }
}
