//! Whole participant payload sharing, scoped to an epoch. References preserve
//! source order without treating a slot/order as participant identity.
use super::*;

pub const FULL_STATE_INTERVAL_US: u64 = 60_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PayloadId(pub u64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedPlayer {
    pub id: PayloadId,
    pub player: RosterPlayer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterObservation {
    pub full: bool,
    pub references: Vec<PayloadId>,
    pub payloads: Vec<SharedPlayer>,
}

/// Contains only the most recent roster's distinct payloads, never its history.
pub struct RosterStore {
    scope: SourceEpoch,
    current: BTreeMap<[u8; 32], SharedPlayer>,
    next_id: u64,
    last_full_us: Option<u64>,
    generation: u64,
}
pub struct PreparedRoster {
    observation: RosterObservation,
    state: RosterStore,
    generation: u64,
}
impl PreparedRoster {
    pub fn observation(&self) -> &RosterObservation {
        &self.observation
    }
    pub(crate) fn take_observation(&mut self) -> RosterObservation {
        std::mem::replace(
            &mut self.observation,
            RosterObservation {
                full: false,
                references: Vec::new(),
                payloads: Vec::new(),
            },
        )
    }
    pub(crate) fn state(&self) -> &RosterStore {
        &self.state
    }
}
impl RosterStore {
    pub fn new(scope: SourceEpoch) -> Self {
        Self {
            scope,
            current: BTreeMap::new(),
            next_id: 1,
            last_full_us: None,
            generation: 0,
        }
    }
    pub fn scope(&self) -> &SourceEpoch {
        &self.scope
    }
    pub fn retained_payloads(&self) -> usize {
        self.current.len()
    }

    /// Preparing a payload dictionary cannot advance committed payload IDs.
    pub fn prepare(
        &self,
        scope: &SourceEpoch,
        end_us: u64,
        players: &[RosterPlayer],
    ) -> Result<PreparedRoster, GapReason> {
        if scope != &self.scope {
            return Err(GapReason::SourceReset);
        }
        if players.len() > MAX_ROSTER {
            return Err(GapReason::Capacity);
        }
        let full = self
            .last_full_us
            .is_none_or(|last| end_us.saturating_sub(last) >= FULL_STATE_INTERVAL_US);
        let mut next_id = self.next_id;
        let mut current = BTreeMap::new();
        let mut references = Vec::with_capacity(players.len());
        let mut payloads = Vec::new();
        for player in players {
            let hash = super::fingerprint(player).map_err(|_| GapReason::InvalidShape)?;
            let shared = if let Some(shared) = current.get(&hash) {
                shared
            } else {
                let shared = if let Some(previous) = self.current.get(&hash) {
                    // Hash equality alone never substitutes a different payload.
                    if &previous.player != player {
                        return Err(GapReason::InvalidShape);
                    }
                    if full {
                        payloads.push(previous.clone());
                    }
                    previous.clone()
                } else {
                    let id = PayloadId(next_id);
                    next_id = next_id.checked_add(1).ok_or(GapReason::Capacity)?;
                    let shared = SharedPlayer {
                        id,
                        player: player.clone(),
                    };
                    payloads.push(shared.clone());
                    shared
                };
                current.entry(hash).or_insert(shared)
            };
            references.push(shared.id);
        }
        Ok(PreparedRoster {
            generation: self.generation,
            observation: RosterObservation {
                full,
                references,
                payloads,
            },
            state: Self {
                scope: self.scope.clone(),
                current,
                next_id,
                last_full_us: if full {
                    Some(end_us)
                } else {
                    self.last_full_us
                },
                generation: self.generation.checked_add(1).ok_or(GapReason::Capacity)?,
            },
        })
    }
    pub fn acknowledge(&mut self, prepared: PreparedRoster) -> Result<(), GapReason> {
        // A stale dictionary cannot reuse IDs allocated by a later commit.
        if self.scope != prepared.state.scope || self.generation != prepared.generation {
            return Err(GapReason::SourceReset);
        }
        *self = prepared.state;
        Ok(())
    }

    pub(crate) fn acknowledge_batch(
        &mut self,
        prepared: Vec<PreparedRoster>,
    ) -> Result<(), GapReason> {
        let mut generation = self.generation;
        for item in &prepared {
            if self.scope != item.state.scope || generation != item.generation {
                return Err(GapReason::SourceReset);
            }
            generation = item.state.generation;
        }
        if let Some(last) = prepared.into_iter().last() {
            *self = last.state;
        }
        Ok(())
    }
}

/// Bounded reader dictionary. A full state can start decoding independently;
/// a missing reference makes the observation unavailable, never an empty roster.
#[derive(Clone, Default)]
pub struct RosterReader {
    scope: Option<SourceEpoch>,
    current: BTreeMap<PayloadId, RosterPlayer>,
    references: Vec<PayloadId>,
}
impl RosterReader {
    pub fn observe(
        &mut self,
        scope: &SourceEpoch,
        row: &RosterObservation,
    ) -> Result<Vec<RosterPlayer>, GapReason> {
        self.observe_state(scope, row)?;
        Ok(self
            .references
            .iter()
            .map(|id| self.current[id].clone())
            .collect())
    }

    /// Validate the complete observation before changing the dictionary. Callers
    /// retaining references need not allocate a second materialized roster.
    pub fn observe_state(
        &mut self,
        scope: &SourceEpoch,
        row: &RosterObservation,
    ) -> Result<(), GapReason> {
        if row.references.len() > MAX_ROSTER || row.payloads.len() > MAX_ROSTER {
            return Err(GapReason::Capacity);
        }
        if !row.full && self.scope.as_ref() != Some(scope) {
            return Err(GapReason::SourceReset);
        }
        let mut supplied = std::collections::BTreeSet::new();
        for payload in &row.payloads {
            if payload.id.0 == 0
                || !supplied.insert(payload.id)
                || !row.references.contains(&payload.id)
            {
                return Err(GapReason::InvalidShape);
            }
            if !row.full
                && self
                    .current
                    .get(&payload.id)
                    .is_some_and(|previous| previous != &payload.player)
            {
                return Err(GapReason::InvalidShape);
            }
        }
        for id in &row.references {
            if !supplied.contains(id) && (row.full || !self.current.contains_key(id)) {
                return Err(GapReason::InvalidShape);
            }
        }
        if row.full {
            self.current.clear();
        } else {
            self.current.retain(|id, _| row.references.contains(id));
        }
        for payload in &row.payloads {
            self.current
                .entry(payload.id)
                .or_insert_with(|| payload.player.clone());
        }
        self.scope = Some(scope.clone());
        self.references.clone_from(&row.references);
        Ok(())
    }
    pub fn retained_payloads(&self) -> usize {
        self.current.len()
    }
    pub fn full_state(&self) -> RosterObservation {
        RosterObservation {
            full: true,
            references: self.references.clone(),
            payloads: self
                .current
                .iter()
                .map(|(id, player)| SharedPlayer {
                    id: *id,
                    player: player.clone(),
                })
                .collect(),
        }
    }
}
