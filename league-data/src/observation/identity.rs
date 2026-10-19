//! Exact within-epoch participant references. Never join by stripped names,
//! champion, roster position, team or wall-clock proximity.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct Participants {
    epoch: SourceEpoch,
    identities: BTreeMap<String, ParticipantId>,
    local: Option<ParticipantId>,
    ambiguous: BTreeSet<String>,
}

impl Participants {
    pub fn new(epoch: SourceEpoch) -> Self {
        Self {
            epoch,
            identities: BTreeMap::new(),
            local: None,
            ambiguous: BTreeSet::new(),
        }
    }
    pub fn local(&self) -> Option<ParticipantId> {
        self.local
    }

    /// A single valid AP-to-PL match is sufficient for local presentation, never
    /// for provisional game-ID association. The first such mapping is retained.
    pub fn observe(
        &mut self,
        epoch: &SourceEpoch,
        roster: &mut [RosterPlayer],
        active: Option<&str>,
    ) -> Result<(), GapReason> {
        if epoch != &self.epoch {
            return Err(GapReason::SourceReset);
        }
        if roster.len() > MAX_ROSTER {
            return Err(GapReason::Capacity);
        }
        let mut counts = BTreeMap::<String, usize>::new();
        for row in roster.iter() {
            if let Some(id) = &row.riot_id {
                *counts.entry(id.clone()).or_default() += 1;
            }
        }
        let mut eligible = BTreeSet::new();
        let mut conflicts = BTreeSet::new();
        for row in roster.iter() {
            let Some(id) = &row.riot_id else {
                continue;
            };
            let split_conflict = match (&row.riot_id_game_name, &row.riot_id_tag_line) {
                (Some(name), Some(tag)) => id != &format!("{name}#{tag}"),
                _ => false,
            };
            if !decode::full_riot_id(id)
                || row.issues.contains_key("riot_id")
                || split_conflict
                || counts[id] != 1
            {
                conflicts.insert(id.clone());
            } else {
                eligible.insert(id.clone());
            }
        }
        // Reserve room for identity changes without silently evicting old references.
        if self.identities.len()
            + eligible
                .iter()
                .filter(|id| !self.identities.contains_key(*id))
                .count()
            > MAX_ROSTER * 4
        {
            return Err(GapReason::Capacity);
        }
        // Validation above is transactional: a rejected observation cannot poison
        // prior actor mappings or partially modify the caller's roster.
        self.ambiguous.extend(
            conflicts
                .iter()
                .filter(|id| self.identities.contains_key(*id))
                .cloned(),
        );
        for id in eligible {
            let next = ParticipantId(self.identities.len() as u64 + 1);
            self.identities.entry(id).or_insert(next);
        }
        for row in roster {
            row.participant = None;
            row.issues.remove("participant");
            if row
                .riot_id
                .as_ref()
                .is_some_and(|id| conflicts.contains(id) || self.ambiguous.contains(id))
            {
                row.issues
                    .insert("participant".into(), FieldIssue::Conflict);
                continue;
            }
            row.participant = row
                .riot_id
                .as_ref()
                .filter(|id| !self.ambiguous.contains(*id))
                .and_then(|id| self.identities.get(id))
                .copied();
            if self.local.is_none()
                && active.is_some_and(|active| row.riot_id.as_deref() == Some(active))
            {
                self.local = row.participant;
            }
        }
        Ok(())
    }

    pub fn resolve_actor(&self, full_identity: &str) -> Option<ParticipantId> {
        (!self.ambiguous.contains(full_identity))
            .then(|| self.identities.get(full_identity).copied())
            .flatten()
    }

    pub fn actor_reference(&self, full_identity: &str) -> events::ActorReference {
        let participant = self.resolve_actor(full_identity);
        let status = if self.ambiguous.contains(full_identity) {
            events::ActorStatus::Conflict
        } else if participant.is_some() {
            events::ActorStatus::Resolved
        } else {
            events::ActorStatus::Unresolved
        };
        events::ActorReference {
            participant,
            status,
        }
    }
}

/// One fresh AP attempt, paired only with independently observed fresh GS data.
/// The existing two-round LCU association reducer remains the identity authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityRound {
    pub sequence: u64,
    pub active_player: Evidence,
    pub game_stats: Option<Evidence>,
    pub riot_id: Option<String>,
    pub map_number: Option<u64>,
    pub game_mode: Option<String>,
}

#[derive(Default)]
pub struct IdentityRounds {
    sequence: u64,
    latest_gs: Option<(Evidence, GameStats)>,
    pending: Option<(Evidence, Option<String>)>,
}

impl IdentityRounds {
    pub fn observe_gs(&mut self, evidence: Evidence, stats: GameStats) {
        self.latest_gs = Some((evidence, stats));
    }
    pub fn observe_ap(&mut self, evidence: Evidence, identity: Option<String>) {
        if self.pending.is_some() {
            self.missed_attempt();
        }
        self.pending = Some((evidence, identity));
    }
    pub fn clear_context(&mut self) {
        self.latest_gs = None;
        self.missed_attempt();
    }
    pub fn missed_attempt(&mut self) {
        self.sequence = self.sequence.saturating_add(1);
        self.pending = None;
    }

    /// A response may arrive before GS. Wait at most the collector's existing
    /// two-second admission freshness, without requesting another GS stream.
    pub fn take_ready(&mut self, now_us: u64) -> Option<IdentityRound> {
        let (ap, identity) = self.pending.as_ref()?;
        let gs = self.latest_gs.as_ref().filter(|(e, _)| {
            e.source == ap.source
                && now_us.saturating_sub(e.timing.start_us.min(ap.timing.start_us)) <= 2_000_000
                && e.timing.end_us <= now_us
                && ap.timing.end_us <= now_us
        });
        if identity.is_some()
            && gs.is_none()
            && now_us.saturating_sub(ap.timing.start_us) < 2_000_000
        {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        let round = IdentityRound {
            sequence: self.sequence,
            active_player: ap.clone(),
            game_stats: gs.map(|(e, _)| e.clone()),
            riot_id: gs.and(identity.clone()),
            map_number: gs.and_then(|(_, s)| s.map_number),
            game_mode: gs.and_then(|(_, s)| s.game_mode.clone()),
        };
        self.pending = None;
        Some(round)
    }
}
