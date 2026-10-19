//! Small committed current state, never accumulated event or roster history.
//! A checkpoint is an acceleration hint; its position must match a validated
//! journal commit before a reader uses it. It does not authorize media access.
use super::*;

pub const MAX_CHECKPOINT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Active,
    Stopped,
    Observed,
    Gap(GapReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointState {
    pub evidence: Evidence,
    pub coverage: Coverage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub format_version: u32,
    pub header: journal::Header,
    pub position: journal::Position,
    pub source: SourceEpoch,
    pub endpoints: BTreeMap<Endpoint, EndpointState>,
    pub game_stats: Option<(Evidence, GameStats)>,
    pub active_identity: Option<(Evidence, ActiveIdentity)>,
    pub roster: Option<(Evidence, roster::RosterObservation)>,
    pub calibration: Option<(Evidence, GameCalibrationV2)>,
    pub local_participant: Option<(Evidence, ParticipantId)>,
}

impl Checkpoint {
    /// Enforce the complete serialized-state limit before touching a checkpoint
    /// file. Bounded source responses alone do not bound their combined state.
    pub fn encode(&self) -> Result<Vec<u8>, journal::Error> {
        journal::encode(self, MAX_CHECKPOINT_BYTES)
    }
}

pub struct State {
    checkpoint: Checkpoint,
    roster: roster::RosterReader,
}
impl State {
    pub fn new(header: journal::Header, position: journal::Position) -> Self {
        let source = SourceEpoch {
            media_id: header.media_id.clone(),
            run_id: header.run_id.clone(),
            epoch: header.initial_epoch,
        };
        Self {
            checkpoint: Checkpoint {
                format_version: 1,
                header,
                position,
                source,
                endpoints: BTreeMap::new(),
                game_stats: None,
                active_identity: None,
                roster: None,
                calibration: None,
                local_participant: None,
            },
            roster: roster::RosterReader::default(),
        }
    }
    pub fn snapshot(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// The caller must obtain this header/position from validated journal framing,
    /// not from a checkpoint's self-declared location. Malformed hints are ignored
    /// by callers in favour of replaying the committed source.
    pub fn from_checkpoint(
        checkpoint: Checkpoint,
        header: &journal::Header,
        position: &journal::Position,
    ) -> Result<Self, journal::Error> {
        use journal::Error;
        if checkpoint.format_version != 1
            || &checkpoint.header != header
            || &checkpoint.position != position
            || checkpoint.source.media_id != header.media_id
            || checkpoint.source.run_id != header.run_id
        {
            return Err(Error::Invalid("checkpoint anchor"));
        }
        let mut roster = roster::RosterReader::default();
        if let Some((evidence, payload)) = &checkpoint.roster {
            if evidence.source != checkpoint.source || !payload.full {
                return Err(Error::Invalid("checkpoint roster scope"));
            }
            roster
                .observe_state(&evidence.source, payload)
                .map_err(|_| Error::Invalid("checkpoint roster"))?;
        }
        for evidence in checkpoint
            .endpoints
            .values()
            .map(|value| &value.evidence)
            .chain(checkpoint.game_stats.iter().map(|(e, _)| e))
            .chain(checkpoint.active_identity.iter().map(|(e, _)| e))
            .chain(checkpoint.calibration.iter().map(|(e, _)| e))
            .chain(checkpoint.local_participant.iter().map(|(e, _)| e))
        {
            if evidence.source != checkpoint.source {
                return Err(Error::Invalid("checkpoint evidence scope"));
            }
        }
        Ok(Self { checkpoint, roster })
    }

    /// Apply only a complete checksum-validated committed batch. Errors preserve
    /// the previous checkpoint, so it can never claim a partially applied commit.
    pub fn apply(
        &mut self,
        records: &[Record],
        position: &journal::Position,
    ) -> Result<(), journal::Error> {
        use journal::Error;
        if self
            .checkpoint
            .position
            .last_sequence
            .checked_add(records.len() as u64)
            != Some(position.last_sequence)
            || position.end_offset <= self.checkpoint.position.end_offset
        {
            return Err(Error::Invalid("checkpoint commit order"));
        }
        // Stage only the fallible roster changes. Validate the entire batch
        // before mutating the checkpoint instead of cloning its full payloads.
        let roster_reader = self.prepare_roster(records)?;
        let next = &mut self.checkpoint;
        let mut roster_evidence = None;
        for record in records {
            let (e, coverage) = match record {
                Record::SourceReset {
                    evidence,
                    previous_epoch: _,
                } => {
                    next.source = evidence.source.clone();
                    next.game_stats = None;
                    next.active_identity = None;
                    next.roster = None;
                    next.calibration = None;
                    next.local_participant = None;
                    next.endpoints.clear();
                    roster_evidence = None;
                    (evidence, None)
                }
                Record::Observation { observation } => match observation.as_ref() {
                    Observation::Success { evidence, payload } => {
                        match payload {
                            Payload::GameStats(gs) => {
                                next.game_stats = Some((evidence.clone(), gs.clone()))
                            }
                            Payload::ActivePlayer(ap) => {
                                next.active_identity = Some((evidence.clone(), ap.clone()))
                            }
                            Payload::PlayerList(_) | Payload::Events(_) | Payload::EndOfGame(_) => {
                            }
                        }
                        (evidence, Some(Coverage::Observed))
                    }
                    Observation::Gap { evidence, reason } => {
                        (evidence, Some(Coverage::Gap(*reason)))
                    }
                },
                Record::EventObservation { evidence, .. } => (evidence, Some(Coverage::Observed)),
                Record::RosterObservation { evidence, .. } => {
                    roster_evidence = Some(evidence);
                    (evidence, Some(Coverage::Observed))
                }
                Record::Calibration {
                    evidence,
                    calibration,
                } => {
                    next.calibration = Some((evidence.clone(), calibration.clone()));
                    (evidence, None)
                }
                Record::CalibrationInvalidated { evidence } => {
                    next.calibration = None;
                    (evidence, None)
                }
                Record::LocalParticipant {
                    evidence,
                    participant,
                } => {
                    next.local_participant = Some((evidence.clone(), *participant));
                    (evidence, None)
                }
                Record::GapInterval { last, reason, .. } => (last, Some(Coverage::Gap(*reason))),
                Record::Boundary { evidence, active } => (
                    evidence,
                    Some(if *active {
                        Coverage::Active
                    } else {
                        Coverage::Stopped
                    }),
                ),
                Record::IdentityRound { round } => (&round.active_player, None),
            };
            // An older in-flight request may finish after a reset. Its gap remains
            // in the journal, but cannot replace the current epoch's latest state.
            if e.source == next.source
                && let Some(coverage) = coverage
            {
                next.endpoints.insert(
                    e.endpoint,
                    EndpointState {
                        evidence: e.clone(),
                        coverage,
                    },
                );
            }
        }
        if let Some(roster_reader) = roster_reader {
            if let Some(evidence) = roster_evidence {
                next.roster = Some((evidence.clone(), roster_reader.full_state()));
            }
            self.roster = roster_reader;
        }
        next.position = position.clone();
        Ok(())
    }

    fn prepare_roster(
        &self,
        records: &[Record],
    ) -> Result<Option<roster::RosterReader>, journal::Error> {
        use journal::Error;
        let mut source = &self.checkpoint.source;
        let mut reader = None;
        for record in records {
            let evidence = match record {
                Record::SourceReset {
                    evidence,
                    previous_epoch,
                } => {
                    if source.epoch != *previous_epoch
                        || previous_epoch.checked_add(1) != Some(evidence.source.epoch)
                    {
                        return Err(Error::Invalid("checkpoint epoch transition"));
                    }
                    source = &evidence.source;
                    reader = Some(roster::RosterReader::default());
                    evidence
                }
                Record::Observation { observation } => match observation.as_ref() {
                    Observation::Success { evidence, payload } => {
                        if &evidence.source != source {
                            return Err(Error::Invalid("checkpoint source"));
                        }
                        if matches!(payload, Payload::PlayerList(_)) {
                            return Err(Error::Invalid("unshared roster in checkpoint input"));
                        }
                        evidence
                    }
                    Observation::Gap { evidence, .. } => evidence,
                },
                Record::RosterObservation { evidence, roster } => {
                    if &evidence.source != source {
                        return Err(Error::Invalid("checkpoint roster source"));
                    }
                    reader
                        .get_or_insert_with(|| self.roster.clone())
                        .observe_state(&evidence.source, roster)
                        .map_err(|_| Error::Invalid("checkpoint roster reference"))?;
                    evidence
                }
                Record::Calibration { evidence, .. }
                | Record::CalibrationInvalidated { evidence }
                | Record::LocalParticipant { evidence, .. } => {
                    if &evidence.source != source {
                        return Err(Error::Invalid("checkpoint context scope"));
                    }
                    evidence
                }
                Record::EventObservation { evidence, .. } | Record::Boundary { evidence, .. } => {
                    evidence
                }
                Record::GapInterval { last, .. } => last,
                Record::IdentityRound { round } => &round.active_player,
            };
            if evidence.source.media_id != self.checkpoint.header.media_id
                || evidence.source.run_id != self.checkpoint.header.run_id
            {
                return Err(Error::Invalid("checkpoint ownership"));
            }
        }
        Ok(reader)
    }
}
