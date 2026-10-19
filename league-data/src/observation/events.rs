//! Event revision tracking. Preparing a batch never advances committed progress.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorRole {
    Killer,
    Victim,
    Assister,
    Recipient,
    Acer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorStatus {
    Resolved,
    Unresolved,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorReference {
    pub participant: Option<ParticipantId>,
    pub status: ActorStatus,
}

/// Entries follow the order of the retained source strings for each role.
pub type EventActors = BTreeMap<ActorRole, Vec<ActorReference>>;

pub fn resolve_actors(event: &Event, participants: &identity::Participants) -> EventActors {
    let mut actors = BTreeMap::new();
    for (role, value) in [
        (ActorRole::Killer, &event.killer),
        (ActorRole::Victim, &event.victim),
        (ActorRole::Recipient, &event.recipient),
        (ActorRole::Acer, &event.acer),
    ] {
        if let Some(value) = value {
            actors.insert(role, vec![participants.actor_reference(value)]);
        }
    }
    if let Some(values) = &event.assisters {
        actors.insert(
            ActorRole::Assister,
            values
                .iter()
                .map(|v| participants.actor_reference(v))
                .collect(),
        );
    }
    actors
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ingestion {
    Accepted,
    UnknownKind,
    Invalid,
    Correction,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRevision {
    pub event: Event,
    pub fingerprint: [u8; 32],
    /// Digest of every distinct variant for this ID in the same response. It
    /// detects both a correction and a later reversion to an older value.
    pub response_fingerprint: [u8; 32],
    pub ingestion: Ingestion,
}

/// Validate an incremental projection against the current digest for just the
/// IDs touched by this response. A database importer need not replay or clone
/// the entire accumulated event history to use the acquisition contract.
pub fn validate_committed_revisions(
    epoch: &SourceEpoch,
    revisions: &[EventRevision],
    mut lookup: impl FnMut(u64) -> Result<Option<[u8; 32]>, GapReason>,
) -> Result<(), GapReason> {
    if revisions.len() > MAX_EVENTS {
        return Err(GapReason::Capacity);
    }
    let mut tracker = EventTracker::new(epoch.clone());
    for id in revisions
        .iter()
        .filter_map(|r| r.event.source_id)
        .collect::<BTreeSet<_>>()
    {
        if let Some(digest) = lookup(id)? {
            tracker.current.insert(id, digest);
        }
    }
    tracker.restore_committed(epoch, revisions)
}

/// Bounded revision lookup, reconstructed by streaming committed journal records.
/// Epoch changes require constructing a fresh tracker, not resetting a global ID.
pub struct EventTracker {
    epoch: SourceEpoch,
    current: BTreeMap<u64, [u8; 32]>,
    generation: u64,
    capacity: usize,
}

pub struct PendingEvents {
    epoch: SourceEpoch,
    generation: u64,
    revisions: Vec<EventRevision>,
    additions: Vec<(u64, [u8; 32])>,
}

impl PendingEvents {
    pub fn revisions(&self) -> &[EventRevision] {
        &self.revisions
    }
    pub(crate) fn take_revisions(&mut self) -> Vec<EventRevision> {
        std::mem::take(&mut self.revisions)
    }
}

impl EventTracker {
    pub fn new(epoch: SourceEpoch) -> Self {
        Self::with_capacity(epoch, MAX_EVENTS)
    }
    pub fn with_capacity(epoch: SourceEpoch, capacity: usize) -> Self {
        Self {
            epoch,
            current: BTreeMap::new(),
            generation: 0,
            capacity: capacity.min(MAX_EVENTS),
        }
    }
    pub fn retained_ids(&self) -> usize {
        self.current.len()
    }

    /// Rebuild only from checksum-validated committed observations, in order.
    /// Recompute source fingerprints/statuses instead of trusting cached digests.
    pub fn restore_committed(
        &mut self,
        epoch: &SourceEpoch,
        revisions: &[EventRevision],
    ) -> Result<(), GapReason> {
        let prepared = self.prepare(epoch, revisions.iter().map(|r| r.event.clone()).collect())?;
        if prepared.revisions() != revisions {
            return Err(GapReason::InvalidShape);
        }
        self.acknowledge(prepared)
    }

    /// No cursor is inferred from max EventID, numeric gaps or HTTP success.
    /// A capacity error requires a durable degraded-coverage record from the caller.
    pub fn prepare(
        &self,
        epoch: &SourceEpoch,
        events: Vec<Event>,
    ) -> Result<PendingEvents, GapReason> {
        self.prepare_following(epoch, events, &[])
    }

    /// A flush can contain multiple EV responses. Overlay only this bounded
    /// uncommitted batch, never clone the accumulated event lookup.
    pub fn prepare_following(
        &self,
        epoch: &SourceEpoch,
        events: Vec<Event>,
        prior: &[PendingEvents],
    ) -> Result<PendingEvents, GapReason> {
        let mut generation = self.generation;
        let mut overlay = BTreeMap::new();
        for pending in prior {
            if pending.epoch != self.epoch || pending.generation != generation {
                return Err(GapReason::SourceReset);
            }
            generation = generation.checked_add(1).ok_or(GapReason::Capacity)?;
            overlay.extend(pending.additions.iter().copied());
        }
        let current = |id: &u64| overlay.get(id).or_else(|| self.current.get(id));
        if epoch != &self.epoch {
            return Err(GapReason::SourceReset);
        }
        if events.len() > MAX_EVENTS {
            return Err(GapReason::Capacity);
        }
        let mut batch = BTreeMap::<u64, BTreeSet<[u8; 32]>>::new();
        let mut hashed = Vec::with_capacity(events.len());
        for mut event in events {
            // Derived actor availability must never manufacture a source correction.
            let actors = event.actors.take();
            let hash = super::fingerprint(&event).map_err(|_| GapReason::InvalidShape)?;
            event.actors = actors;
            if let Some(id) = event.source_id {
                batch.entry(id).or_default().insert(hash);
            }
            hashed.push((event, hash));
        }
        if self.current.len()
            + overlay
                .keys()
                .filter(|id| !self.current.contains_key(id))
                .count()
            + batch.keys().filter(|id| current(id).is_none()).count()
            > self.capacity
        {
            return Err(GapReason::Capacity);
        }
        let response_fingerprints: BTreeMap<_, [u8; 32]> = batch
            .iter()
            .map(|(id, hashes)| {
                let mut digest = Sha256::new();
                for hash in hashes {
                    digest.update(hash);
                }
                (*id, digest.finalize().into())
            })
            .collect();
        let mut emitted = BTreeSet::new();
        let mut additions = BTreeMap::new();
        let mut revisions = Vec::new();
        for (event, fingerprint) in hashed {
            let response_fingerprint = event
                .source_id
                .map_or(fingerprint, |id| response_fingerprints[&id]);
            let ingestion = if let Some(id) = event.source_id {
                let key = (id, fingerprint);
                if current(&id) == Some(&response_fingerprint) || !emitted.insert(key) {
                    continue;
                }
                additions.insert(id, response_fingerprint);
                if batch.get(&id).is_some_and(|v| v.len() > 1) {
                    Ingestion::Conflict
                } else if current(&id).is_some() {
                    Ingestion::Correction
                } else if !event.valid_envelope() {
                    Ingestion::Invalid
                } else if event.known_kind() {
                    Ingestion::Accepted
                } else {
                    Ingestion::UnknownKind
                }
            } else {
                Ingestion::Invalid
            };
            revisions.push(EventRevision {
                event,
                fingerprint,
                response_fingerprint,
                ingestion,
            });
        }
        Ok(PendingEvents {
            epoch: epoch.clone(),
            generation,
            revisions,
            additions: additions.into_iter().collect(),
        })
    }

    /// Call only after the writer's corresponding checksum-bearing commit is flushed.
    /// Rejected/stale batches cannot change the durable deduplication boundary.
    pub fn acknowledge(&mut self, pending: PendingEvents) -> Result<(), GapReason> {
        if pending.epoch != self.epoch || pending.generation != self.generation {
            return Err(GapReason::SourceReset);
        }
        let next = self.generation.checked_add(1).ok_or(GapReason::Capacity)?;
        if self.current.len()
            + pending
                .additions
                .iter()
                .filter(|(id, _)| !self.current.contains_key(id))
                .count()
            > self.capacity
        {
            return Err(GapReason::Capacity);
        }
        for (id, hash) in pending.additions {
            self.current.insert(id, hash);
        }
        self.generation = next;
        Ok(())
    }
}
