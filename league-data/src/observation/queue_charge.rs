//! Conservative admission credits for owned records. No JSON encoding is needed
//! on the acquisition task. Credits bound retained capacities (including spare
//! Vec/String capacity and map nodes) and the eventual journal representation.
//! Fixed field/tag allowances include writer normalization and entry framing;
//! strings count escaping without constructing an encoded representation.
use super::*;
use std::mem::size_of;

pub(super) trait Charge {
    fn charge(&self) -> usize;
}
impl<T: Charge> Charge for Option<T> {
    fn charge(&self) -> usize {
        self.as_ref().map_or(4, Charge::charge)
    }
}
impl<T: Charge> Charge for Box<T> {
    fn charge(&self) -> usize {
        (**self).charge().saturating_add(32)
    }
}
impl<T: Charge> Charge for Vec<T> {
    fn charge(&self) -> usize {
        self.iter().fold(
            self.capacity()
                .saturating_mul(size_of::<T>())
                .saturating_add(32),
            |bytes, value| bytes.saturating_add(value.charge()),
        )
    }
}
impl<K: Charge, V: Charge> Charge for BTreeMap<K, V> {
    fn charge(&self) -> usize {
        self.iter()
            .fold(map_storage::<K, V>(self.len()), |bytes, (key, value)| {
                bytes
                    .saturating_add(key.charge())
                    .saturating_add(value.charge())
            })
    }
}
fn map_storage<K, V>(len: usize) -> usize {
    // Nodes hold eleven pairs and (except the root) at least five. Include
    // a possibly empty allocated root, links and allocation overhead.
    let node = size_of::<(K, V)>().saturating_mul(11).saturating_add(256);
    let nodes = 1usize.saturating_add(len.saturating_sub(1) / 5);
    node.saturating_mul(nodes)
}
impl Charge for String {
    fn charge(&self) -> usize {
        let escaped = self.as_bytes().iter().fold(2usize, |bytes, byte| {
            bytes.saturating_add(match byte {
                b'"' | b'\\' => 2,
                0..=31 => 6,
                _ => 1,
            })
        });
        self.capacity().saturating_add(32).saturating_add(escaped)
    }
}
macro_rules! fixed {
    ($($ty:ty),* $(,)?) => { $(impl Charge for $ty {
        fn charge(&self) -> usize { size_of::<Self>().saturating_add(64) }
    })* };
}
fixed!(
    u64,
    f64,
    bool,
    ObservationId,
    ParticipantId,
    Endpoint,
    GapReason,
    FieldIssue,
    GameTick,
    serde_json::Number,
    events::ActorRole,
    events::ActorReference,
    roster::PayloadId
);
impl Charge for MediaId {
    // MediaId constructors allocate a canonical 36-byte UUID; callers cannot
    // supply a String with hidden spare capacity.
    fn charge(&self) -> usize {
        256
    }
}
impl Charge for GameCalibrationV2 {
    fn charge(&self) -> usize {
        size_of::<Self>().saturating_add(1024)
    }
}
impl Charge for crate::Stats {
    fn charge(&self) -> usize {
        size_of::<Self>().saturating_add(Self::KEYS.len().saturating_mul(128))
    }
}
macro_rules! fields {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl Charge for $ty {
            fn charge(&self) -> usize {
                let Self { $($field),* } = self;
                let mut bytes = size_of::<Self>().saturating_add(128);
                $(bytes = bytes.saturating_add(64).saturating_add($field.charge());)*
                bytes
            }
        }
    };
}
fields!(SourceEpoch {
    media_id,
    run_id,
    epoch
});
fields!(RequestTiming {
    start_us,
    end_us,
    received_at
});
fields!(Evidence {
    id,
    source,
    endpoint,
    timing,
    game_stats,
    calibration
});
fields!(FinalStats {
    stats,
    extras,
    aliases,
    issues
});
fields!(identity::IdentityRound {
    sequence,
    active_player,
    game_stats,
    riot_id,
    map_number,
    game_mode
});
fields!(roster::SharedPlayer { id, player });
fields!(roster::RosterObservation {
    full,
    references,
    payloads
});
impl Charge for events::EventRevision {
    fn charge(&self) -> usize {
        self.event.charge().saturating_add(512)
    }
}
impl Charge for Payload {
    fn charge(&self) -> usize {
        128usize.saturating_add(match self {
            Self::GameStats(v) => v.charge(),
            Self::ActivePlayer(v) => v.charge(),
            // Preparing the shared dictionary temporarily retains the acquired
            // roster, emitted payloads and pending dictionary until commit.
            // Account for those copies as well as their map nodes and IDs.
            Self::PlayerList(v) => v
                .charge()
                .saturating_mul(3)
                .saturating_add(map_storage::<[u8; 32], roster::SharedPlayer>(v.len()))
                .saturating_add(v.len().saturating_mul(256)),
            Self::Events(v) => v.charge().saturating_add(v.len().saturating_mul(512)),
            Self::EndOfGame(v) => v.charge(),
        })
    }
}
impl Charge for Observation {
    fn charge(&self) -> usize {
        256usize.saturating_add(match self {
            Self::Success { evidence, payload } => {
                evidence.charge().saturating_add(payload.charge())
            }
            Self::Gap { evidence, reason } => evidence.charge().saturating_add(reason.charge()),
        })
    }
}
impl Charge for Record {
    fn charge(&self) -> usize {
        let content = match self {
            Self::Observation { observation } => observation.charge(),
            Self::EventObservation {
                evidence,
                revisions,
            } => evidence.charge().saturating_add(revisions.charge()),
            Self::RosterObservation { evidence, roster } => {
                evidence.charge().saturating_add(roster.charge())
            }
            Self::Calibration {
                evidence,
                calibration,
            } => evidence.charge().saturating_add(calibration.charge()),
            Self::GapInterval { first, last, .. } => first.charge().saturating_add(last.charge()),
            Self::IdentityRound { round } => round.charge(),
            Self::CalibrationInvalidated { evidence }
            | Self::Boundary { evidence, .. }
            | Self::SourceReset { evidence, .. }
            | Self::LocalParticipant { evidence, .. } => evidence.charge(),
        };
        size_of::<Self>()
            .saturating_add(512)
            .saturating_add(content)
    }
}

impl Record {
    /// Conservative queue credit, covering owned capacities and an upper bound
    /// on the normalized journal bytes. Saturation makes oversized input reject.
    /// This is an admission bound, not a measurement of actual heap consumption.
    pub fn queue_charge(&self) -> usize {
        self.charge()
    }
}
