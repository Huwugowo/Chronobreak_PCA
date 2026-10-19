//! Durable authored inputs. SQLite can be discarded without losing this file.
use anyhow::{Context, Result, ensure};
use chronobreak_league_data::observation::{Evidence, ParticipantId, SourceEpoch};
use chronobreak_replay_time::{ClipRange, GameTick, MediaId, ReplayTimeReference};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub(crate) const FILE: &str = "league_authored_records.json";
pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORDS: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    pub schema_version: u32,
    pub revision: u64,
    pub records: Vec<AuthoredRecord>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            records: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthoredRecord {
    pub id: MediaId,
    pub kind: RecordKind,
    pub created_at: String,
    pub reason: String,
    pub derivation: Derivation,
    pub inputs: Vec<InputReference>,
    pub anchors: Vec<Anchor>,
    /// Ordering is authored intent. Never sort, merge or deduplicate ranges.
    pub ranges: Vec<ClipRange>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RecordKind {
    Bookmark,
    Recipe,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Derivation {
    pub rule: String,
    pub version: String,
    pub parameters: BTreeMap<String, Parameter>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum Parameter {
    Boolean(bool),
    Integer(i64),
    Decimal(f64),
    Text(String),
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum InputReference {
    Media {
        media_id: MediaId,
    },
    Event {
        source: SourceEpoch,
        source_id: u64,
        revision: Option<EventRevisionReference>,
    },
    Participant {
        source: SourceEpoch,
        participant: ParticipantId,
    },
}
impl InputReference {
    pub(crate) fn media_id(&self) -> &MediaId {
        match self {
            Self::Media { media_id } => media_id,
            Self::Event { source, .. } | Self::Participant { source, .. } => &source.media_id,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventRevisionReference {
    pub sequence: u64,
    pub ordinal: u64,
    pub fingerprint: [u8; 32],
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Anchor {
    pub media_id: MediaId,
    pub replay: Option<ReplayTimeReference>,
    pub game_tick: Option<GameTick>,
    pub evidence: Option<Evidence>,
    pub mapping: Mapping,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Mapping {
    Exact,
    Estimated { uncertainty_game_ticks: u64 },
    Unavailable { reason: String },
}

fn text(value: &str, cap: usize) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= cap && !value.chars().any(char::is_control),
        "authored text"
    );
    Ok(())
}
impl Store {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1 && self.records.len() <= MAX_RECORDS,
            "authored schema/capacity"
        );
        let mut ids = HashSet::new();
        for record in &self.records {
            ensure!(ids.insert(&record.id), "duplicate authored record identity");
            time::OffsetDateTime::parse(
                &record.created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .context("authored creation timestamp")?;
            text(&record.created_at, 64)?;
            text(&record.reason, 1024)?;
            text(&record.derivation.rule, 128)?;
            text(&record.derivation.version, 64)?;
            ensure!(
                record.derivation.parameters.len() <= 64
                    && record.inputs.len() <= 256
                    && record.anchors.len() <= 64
                    && record.ranges.len() <= 256,
                "authored record capacity"
            );
            for (key, value) in &record.derivation.parameters {
                text(key, 128)?;
                match value {
                    Parameter::Text(v) => text(v, 1024)?,
                    Parameter::Decimal(v) => {
                        ensure!(v.is_finite(), "authored parameter is not finite")
                    }
                    _ => {}
                }
            }
            for reference in &record.inputs {
                match reference {
                    InputReference::Event {
                        revision: Some(revision),
                        ..
                    } => ensure!(
                        revision.sequence > 0 && revision.ordinal < 65_536,
                        "authored event revision"
                    ),
                    InputReference::Participant { participant, .. } => {
                        ensure!(participant.0 > 0, "authored participant identity")
                    }
                    _ => {}
                }
            }
            for anchor in &record.anchors {
                ensure!(
                    anchor
                        .replay
                        .as_ref()
                        .is_none_or(|t| t.media_id == anchor.media_id)
                        && anchor
                            .evidence
                            .as_ref()
                            .is_none_or(|e| e.source.media_id == anchor.media_id
                                && e.id.0 > 0
                                && e.timing.start_us <= e.timing.end_us),
                    "authored anchor media/evidence"
                );
                if let Some(e) = &anchor.evidence {
                    text(&e.timing.received_at, 64)?;
                    time::OffsetDateTime::parse(
                        &e.timing.received_at,
                        &time::format_description::well_known::Rfc3339,
                    )
                    .context("authored observation receipt")?;
                }
                match &anchor.mapping {
                    Mapping::Exact | Mapping::Estimated { .. } => ensure!(
                        anchor.replay.is_some(),
                        "mapped authored anchor requires exact replay coordinate"
                    ),
                    Mapping::Unavailable { reason } => text(reason, 256)?,
                }
            }
            for range in &record.ranges {
                ensure!(
                    range.start_frame < range.end_frame_exclusive,
                    "authored frame interval"
                );
            }
        }
        Ok(())
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "authored store capacity");
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        struct Capped(Vec<u8>);
        impl Write for Capped {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
                    return Err(std::io::Error::other("authored store capacity"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut output = Capped(Vec::new());
        serde_json::to_writer(&mut output, self)?;
        Ok(output.0)
    }
}

pub(crate) fn load(root: &Path) -> Result<Store> {
    let path = root.join(FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && !metadata.is_symlink() && metadata.len() <= MAX_BYTES as u64,
        "authored store size/type"
    );
    let approved = crate::playback_file::ApprovedRoot::new(root)?;
    let file =
        crate::playback_file::open_file(&path, crate::playback_file::FileScope::Root(&approved))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    Store::decode(&bytes)
}

/// CAS plus an exclusive cross-process lock prevents lost authored updates.
/// Invalid existing data is preserved and cannot be overwritten by a fresh file.
pub(crate) fn put(
    root: &Path,
    expected_revision: u64,
    record: AuthoredRecord,
    check: &dyn Fn() -> Result<()>,
) -> Result<Store> {
    let _directory = pin_directory(root)?;
    let _lock = lock(root)?;
    check()?;
    let mut store = load(root)?;
    ensure!(
        store.revision == expected_revision,
        "authored store revision changed"
    );
    if let Some(existing) = store.records.iter_mut().find(|r| r.id == record.id) {
        *existing = record;
    } else {
        store.records.push(record);
    }
    store.revision = store
        .revision
        .checked_add(1)
        .context("authored revision capacity")?;
    let bytes = store.encode()?;
    check()?;
    atomicwrites::AtomicFile::new(root.join(FILE), atomicwrites::AllowOverwrite)
        .write(|file| {
            file.write_all(&bytes)?;
            file.sync_all()
        })
        .map_err(|error| anyhow::anyhow!("authored atomic publication: {error}"))?;
    Ok(store)
}

#[cfg(windows)]
fn pin_directory(root: &Path) -> Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let file = OpenOptions::new()
        .read(true)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
        .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
        .open(root)?;
    ensure!(
        file.metadata()?.is_dir()
            && file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
        "authored root type"
    );
    Ok(file)
}
#[cfg(not(windows))]
fn pin_directory(root: &Path) -> Result<File> {
    Ok(File::open(root)?)
}
fn lock(root: &Path) -> Result<File> {
    let path = root.join(".league-authored.lock");
    if let Ok(meta) = fs::symlink_metadata(&path) {
        ensure!(meta.is_file() && !meta.is_symlink(), "authored lock type");
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    }
    let file = options.open(path)?;
    ensure!(
        file.metadata()?.is_file() && !file.metadata()?.is_symlink(),
        "authored lock type"
    );
    file.try_lock().context("authored store busy")?;
    Ok(file)
}

#[cfg(test)]
mod tests;
