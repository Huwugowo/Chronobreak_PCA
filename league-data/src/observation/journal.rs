//! Incremental telemetry framing. Only checksum-validated, flushed commits are
//! acknowledged. The caller owns directory/media authorization and async queues.
use super::{GapReason, Observation, Payload, Record, SourceEpoch, events::EventTracker};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Seek, SeekFrom, Write};
use std::path::Path;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_BATCH_RECORDS: usize = 256;
const MAX_LINE_BYTES: usize = MAX_BATCH_BYTES;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Invalid(&'static str),
    Capacity,
    Poisoned,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "journal I/O: {e}"),
            Self::Invalid(reason) => write!(f, "invalid journal: {reason}"),
            Self::Capacity => f.write_str("journal batch capacity exceeded"),
            Self::Poisoned => f.write_str("journal stopped after an uncertain write"),
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub format_version: u32,
    pub media_id: MediaId,
    pub run_id: MediaId,
    pub initial_epoch: u64,
}
impl Header {
    pub fn new(scope: &SourceEpoch) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            media_id: scope.media_id.clone(),
            run_id: scope.run_id.clone(),
            initial_epoch: scope.epoch,
        }
    }
    pub fn initial_position(&self) -> Result<Position, Error> {
        let bytes = encode(
            &Line::Header {
                header: self.clone(),
            },
            MAX_LINE_BYTES,
        )?;
        Ok(Position {
            end_offset: bytes.len() as u64,
            last_sequence: 0,
            checksum: Sha256::digest(&bytes).into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub sequence: u64,
    pub record: Record,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub first: u64,
    pub last: u64,
    pub count: u64,
    pub bytes: u64,
    /// SHA-256 over the previous digest followed by the exact entry lines.
    pub checksum: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case", deny_unknown_fields)]
enum Line {
    Header { header: Header },
    Entry { entry: Box<Entry> },
    Commit { commit: Commit },
}

#[derive(Serialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
enum BorrowedLine<'a> {
    Entry { entry: BorrowedEntry<'a> },
}

#[derive(Serialize)]
struct BorrowedEntry<'a> {
    sequence: u64,
    record: &'a Record,
}

/// A successful return means the OS accepted the data flush. This does not claim
/// arbitrary hardware power-loss durability. Test targets can inject failures.
pub trait DurableWrite: Write {
    fn sync_data(&mut self) -> io::Result<()>;
}
impl DurableWrite for File {
    fn sync_data(&mut self) -> io::Result<()> {
        File::sync_data(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub end_offset: u64,
    pub last_sequence: u64,
    pub checksum: [u8; 32],
}

pub struct Writer<W> {
    output: W,
    header: Header,
    position: Position,
    poisoned: bool,
}

impl Writer<File> {
    /// Never truncate an existing recording or silently append after an unknown tail.
    pub fn create_new(path: &Path, header: Header) -> Result<Self, Error> {
        Self::new(
            OpenOptions::new().create_new(true).write(true).open(path)?,
            header,
        )
    }
}

impl<W: DurableWrite> Writer<W> {
    /// `output` must be a new empty stream. Recovery is a separate read operation;
    /// restarting acquisition cannot establish continuity of the old source run.
    pub fn new(mut output: W, header: Header) -> Result<Self, Error> {
        if header.format_version != FORMAT_VERSION {
            return Err(Error::Invalid("format version"));
        }
        let bytes = encode(
            &Line::Header {
                header: header.clone(),
            },
            MAX_LINE_BYTES,
        )?;
        output.write_all(&bytes)?;
        output.flush()?;
        output.sync_data()?;
        let position = Position {
            end_offset: bytes.len() as u64,
            last_sequence: 0,
            checksum: Sha256::digest(&bytes).into(),
        };
        Ok(Self {
            output,
            header,
            position,
            poisoned: false,
        })
    }

    pub fn position(&self) -> &Position {
        &self.position
    }
    pub fn into_inner(self) -> W {
        self.output
    }

    /// A whole caller admission batch is committed together. The durable boundary
    /// advances only after both framing and data flush succeed. Failure poisons the
    /// writer; retrying into the same uncertain byte stream is forbidden.
    pub fn commit(&mut self, records: &[Record]) -> Result<Position, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        if records.is_empty() || records.len() > MAX_BATCH_RECORDS {
            return Err(Error::Capacity);
        }
        let first = self
            .position
            .last_sequence
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        let mut batch = BoundedBuffer {
            bytes: Vec::new(),
            limit: MAX_BATCH_BYTES,
        };
        let mut last = first;
        for (index, record) in records.iter().enumerate() {
            validate_scope(&self.header, record)?;
            last = first.checked_add(index as u64).ok_or(Error::Capacity)?;
            let line = BorrowedLine::Entry {
                entry: BorrowedEntry {
                    sequence: last,
                    record,
                },
            };
            serde_json::to_writer(&mut batch, &line).map_err(|_| Error::Capacity)?;
            batch.write_all(b"\n").map_err(|_| Error::Capacity)?;
        }
        let bytes = batch.bytes;
        let checksum = digest(self.position.checksum, &bytes);
        let commit = Commit {
            first,
            last,
            count: records.len() as u64,
            bytes: bytes.len() as u64,
            checksum,
        };
        let suffix = encode(&Line::Commit { commit }, MAX_LINE_BYTES)?;
        let end_offset = self
            .position
            .end_offset
            .checked_add(bytes.len() as u64)
            .and_then(|n| n.checked_add(suffix.len() as u64))
            .ok_or(Error::Capacity)?;
        if let Err(e) = self
            .output
            .write_all(&bytes)
            .and_then(|()| self.output.write_all(&suffix))
            .and_then(|()| self.output.flush())
            .and_then(|()| self.output.sync_data())
        {
            self.poisoned = true;
            return Err(Error::Io(e));
        }
        self.position = Position {
            end_offset,
            last_sequence: last,
            checksum,
        };
        Ok(self.position.clone())
    }
}

fn digest(previous: [u8; 32], bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(previous);
    hash.update(bytes);
    hash.finalize().into()
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit - self.bytes.len() {
            return Err(io::Error::other("batch capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, Error> {
    let mut out = BoundedBuffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut out, value).map_err(|_| Error::Capacity)?;
    out.write_all(b"\n").map_err(|_| Error::Capacity)?;
    Ok(out.bytes)
}

fn validate_scope(header: &Header, record: &Record) -> Result<(), Error> {
    let check = |e: &super::Evidence| {
        if e.source.media_id != header.media_id
            || e.source.run_id != header.run_id
            || e.timing.end_us < e.timing.start_us
        {
            Err(Error::Invalid("source or request interval"))
        } else {
            Ok(())
        }
    };
    match record {
        Record::Observation { observation } => match observation.as_ref() {
            Observation::Success { evidence, .. } | Observation::Gap { evidence, .. } => {
                check(evidence)
            }
        },
        Record::EventObservation { evidence, .. }
        | Record::RosterObservation { evidence, .. }
        | Record::Calibration { evidence, .. }
        | Record::CalibrationInvalidated { evidence }
        | Record::Boundary { evidence, .. }
        | Record::SourceReset { evidence, .. }
        | Record::LocalParticipant { evidence, .. } => check(evidence),
        Record::GapInterval {
            first,
            last,
            attempts,
            ..
        } => {
            if *attempts == 0 {
                return Err(Error::Invalid("empty gap interval"));
            }
            check(first)?;
            check(last)
        }
        Record::IdentityRound { round } => {
            check(&round.active_player)?;
            if let Some(gs) = &round.game_stats {
                check(gs)?;
            }
            Ok(())
        }
    }
}

/// Serial writer-side normalization. A batch can contain several observations.
/// Lookup changes are speculative until the same batch is flushed;
/// capacity exhaustion becomes explicit degraded coverage in that durable batch.
pub struct RetainingWriter<W> {
    writer: Writer<W>,
    scope: SourceEpoch,
    events: EventTracker,
    roster: super::roster::RosterStore,
    event_capacity: usize,
}
impl<W: DurableWrite> RetainingWriter<W> {
    pub fn new(writer: Writer<W>) -> Self {
        Self::with_event_capacity(writer, super::MAX_EVENTS)
    }
    pub fn with_event_capacity(writer: Writer<W>, capacity: usize) -> Self {
        let scope = SourceEpoch {
            media_id: writer.header.media_id.clone(),
            run_id: writer.header.run_id.clone(),
            epoch: writer.header.initial_epoch,
        };
        Self {
            writer,
            events: EventTracker::with_capacity(scope.clone(), capacity),
            roster: super::roster::RosterStore::new(scope.clone()),
            scope,
            event_capacity: capacity.min(super::MAX_EVENTS),
        }
    }
    pub fn position(&self) -> &Position {
        self.writer.position()
    }
    pub fn retained_event_ids(&self) -> usize {
        self.events.retained_ids()
    }
    pub fn into_inner(self) -> W {
        self.writer.into_inner()
    }

    pub fn commit_acquired(&mut self, records: &[Record]) -> Result<Position, Error> {
        self.commit_acquired_with(records, |_, _| {})
    }

    /// Invokes the observer only after the normalized batch is durable. The
    /// observer can update a small checkpoint/manifest without retaining history.
    pub fn commit_acquired_with(
        &mut self,
        records: &[Record],
        committed: impl FnOnce(&[Record], &Position),
    ) -> Result<Position, Error> {
        self.commit_owned_with(records.to_vec(), committed)
    }

    /// Consume admitted records so unchanged cumulative events and ordinary
    /// observations need no copies before the durable serialization pass.
    pub fn commit_owned_with(
        &mut self,
        records: Vec<Record>,
        committed: impl FnOnce(&[Record], &Position),
    ) -> Result<Position, Error> {
        if records.is_empty() || records.len() > MAX_BATCH_RECORDS {
            return Err(Error::Capacity);
        }
        let mut output = Vec::with_capacity(records.len());
        let mut replacement = None;
        let mut roster_replacement = None;
        let mut next_scope = self.scope.clone();
        let mut pending = Vec::new();
        let mut pending_roster = Vec::<super::roster::PreparedRoster>::new();
        for record in records {
            if let Record::SourceReset {
                evidence,
                previous_epoch,
            } = &record
            {
                if *previous_epoch != next_scope.epoch
                    || previous_epoch.checked_add(1) != Some(evidence.source.epoch)
                    || evidence.source.media_id != next_scope.media_id
                    || evidence.source.run_id != next_scope.run_id
                {
                    return Err(Error::Invalid("source epoch transition"));
                }
                next_scope = evidence.source.clone();
                pending.clear();
                pending_roster.clear();
                replacement = Some(EventTracker::with_capacity(
                    next_scope.clone(),
                    self.event_capacity,
                ));
                roster_replacement = Some(super::roster::RosterStore::new(next_scope.clone()));
            }
            if let Record::Observation { observation } = &record
                && let Observation::Success { evidence, .. } = observation.as_ref()
                && evidence.source != next_scope
            {
                return Err(Error::Invalid("successful data outside owned epoch"));
            }
            match record {
                Record::Observation { observation } => match *observation {
                    Observation::Success {
                        evidence,
                        payload: Payload::Events(events),
                    } => {
                        let tracker = replacement.as_ref().unwrap_or(&self.events);
                        match tracker.prepare_following(&evidence.source, events, &pending) {
                            Ok(mut prepared) => {
                                output.push(Record::EventObservation {
                                    evidence,
                                    revisions: prepared.take_revisions(),
                                });
                                pending.push(prepared);
                            }
                            Err(
                                reason @ (GapReason::Capacity
                                | GapReason::SourceReset
                                | GapReason::InvalidShape),
                            ) => {
                                output.push(Record::Observation {
                                    observation: Box::new(Observation::Gap { evidence, reason }),
                                });
                            }
                            Err(_) => {
                                return Err(Error::Invalid(
                                    "unexpected event normalization failure",
                                ));
                            }
                        }
                    }
                    Observation::Success {
                        evidence,
                        payload: Payload::PlayerList(players),
                    } => {
                        let store = pending_roster
                            .last()
                            .map(super::roster::PreparedRoster::state)
                            .or(roster_replacement.as_ref())
                            .unwrap_or(&self.roster);
                        match store.prepare(&evidence.source, evidence.timing.end_us, &players) {
                            Ok(mut prepared) => {
                                output.push(Record::RosterObservation {
                                    evidence,
                                    roster: prepared.take_observation(),
                                });
                                pending_roster.push(prepared);
                            }
                            Err(reason) => output.push(Record::Observation {
                                observation: Box::new(Observation::Gap { evidence, reason }),
                            }),
                        }
                    }
                    observation => output.push(Record::Observation {
                        observation: Box::new(observation),
                    }),
                },
                record => output.push(record),
            }
        }
        let position = self.writer.commit(&output)?;
        self.scope = next_scope;
        if let Some(new_state) = replacement {
            self.events = new_state;
        }
        for pending in pending {
            self.events
                .acknowledge(pending)
                .map_err(|_| Error::Invalid("event commit acknowledgement"))?;
        }
        if let Some(new_roster) = roster_replacement {
            self.roster = new_roster;
        }
        self.roster
            .acknowledge_batch(pending_roster)
            .map_err(|_| Error::Invalid("roster commit acknowledgement"))?;
        committed(&output, &position);
        Ok(position)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    pub header: Header,
    pub position: Position,
    /// Bytes after the last complete validated commit have unknown durability.
    pub tail_bytes: u64,
}

/// Streams bounded committed batches. A callback must not publish a partial batch.
/// Interior corruption is an error; only an incomplete/uncommitted suffix is a tail.
pub fn recover(
    mut input: impl BufRead,
    committed: impl FnMut(&[Entry], &Position) -> Result<(), Error>,
) -> Result<Recovery, Error> {
    let (header_bytes, complete) = read_line(&mut input)?;
    if !complete {
        return Err(Error::Invalid("incomplete header"));
    }
    let Line::Header { header } = decode(&header_bytes)? else {
        return Err(Error::Invalid("missing header"));
    };
    if header.format_version != FORMAT_VERSION {
        return Err(Error::Invalid("format version"));
    }
    let position = Position {
        end_offset: header_bytes.len() as u64,
        last_sequence: 0,
        checksum: Sha256::digest(&header_bytes).into(),
    };
    Ok(recover_commits(input, header, position, None, committed)?.recovery)
}

/// A bounded import page. An unfinished page has not examined the remaining
/// suffix, so its zero tail count makes no claim about that suffix's durability.
pub struct RecoveryPage {
    pub recovery: Recovery,
    pub complete: bool,
}

/// Continue from an already validated cached commit, verifying each new commit
/// against its checksum chain. The caller must discard this acceleration state
/// on source replacement/version invalidation; explicit rebuild starts at zero.
pub fn recover_after(
    mut input: impl BufRead + Seek,
    header: &Header,
    position: &Position,
    max_batches: usize,
    committed: impl FnMut(&[Entry], &Position) -> Result<(), Error>,
) -> Result<RecoveryPage, Error> {
    if !(1..=64).contains(&max_batches) {
        return Err(Error::Capacity);
    }
    input.seek(SeekFrom::Start(0))?;
    let (bytes, complete) = read_line(&mut input)?;
    if !complete {
        return Err(Error::Invalid("incomplete header"));
    }
    let Line::Header { header: actual } = decode(&bytes)? else {
        return Err(Error::Invalid("missing header"));
    };
    let initial = actual.initial_position()?;
    if &actual != header
        || actual.format_version != FORMAT_VERSION
        || position.end_offset < initial.end_offset
        || (position.last_sequence == 0 && position != &initial)
        || input.seek(SeekFrom::End(0))? < position.end_offset
    {
        return Err(Error::Invalid("cached source anchor"));
    }
    input.seek(SeekFrom::Start(position.end_offset))?;
    recover_commits(
        input,
        actual,
        position.clone(),
        Some(max_batches),
        committed,
    )
}

fn recover_commits(
    mut input: impl BufRead,
    header: Header,
    mut position: Position,
    max_batches: Option<usize>,
    mut committed: impl FnMut(&[Entry], &Position) -> Result<(), Error>,
) -> Result<RecoveryPage, Error> {
    let mut batches = 0;
    let mut total = position.end_offset;
    let mut batch_bytes = Vec::new();
    let mut entries = Vec::new();
    loop {
        if max_batches.is_some_and(|max| batches == max) {
            return Ok(RecoveryPage {
                recovery: Recovery {
                    header,
                    position,
                    tail_bytes: 0,
                },
                complete: false,
            });
        }
        let (bytes, complete) = read_line(&mut input)?;
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Capacity)?;
        if !complete {
            break;
        }
        match decode(&bytes)? {
            Line::Entry { entry } => {
                if entries.len() >= MAX_BATCH_RECORDS
                    || bytes.len() > MAX_BATCH_BYTES - batch_bytes.len()
                {
                    return Err(Error::Capacity);
                }
                let expected = position
                    .last_sequence
                    .checked_add(entries.len() as u64)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Error::Capacity)?;
                if entry.sequence != expected {
                    return Err(Error::Invalid("entry sequence"));
                }
                validate_scope(&header, &entry.record)?;
                entries.push(*entry);
                batch_bytes.extend_from_slice(&bytes);
            }
            Line::Commit { commit } => {
                let first = entries
                    .first()
                    .ok_or(Error::Invalid("empty commit"))?
                    .sequence;
                let last = entries
                    .last()
                    .ok_or(Error::Invalid("empty commit"))?
                    .sequence;
                if commit.first != first
                    || commit.last != last
                    || commit.count != entries.len() as u64
                    || commit.bytes != batch_bytes.len() as u64
                    || commit.checksum != digest(position.checksum, &batch_bytes)
                {
                    return Err(Error::Invalid("commit integrity"));
                }
                let next = Position {
                    end_offset: total,
                    last_sequence: last,
                    checksum: commit.checksum,
                };
                committed(&entries, &next)?;
                position = next;
                batches += 1;
                entries.clear();
                batch_bytes.clear();
            }
            Line::Header { .. } => return Err(Error::Invalid("interior header")),
        }
    }
    Ok(RecoveryPage {
        recovery: Recovery {
            header,
            tail_bytes: total - position.end_offset,
            position,
        },
        complete: true,
    })
}

fn decode(bytes: &[u8]) -> Result<Line, Error> {
    serde_json::from_slice(bytes).map_err(|_| Error::Invalid("frame encoding"))
}

fn read_line(input: &mut impl BufRead) -> Result<(Vec<u8>, bool), Error> {
    let mut line = Vec::new();
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok((line, false));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if count > MAX_LINE_BYTES - line.len() {
            return Err(Error::Capacity);
        }
        line.extend_from_slice(&available[..count]);
        input.consume(count);
        if newline.is_some() {
            return Ok((line, true));
        }
    }
}
