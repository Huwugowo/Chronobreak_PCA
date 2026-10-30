//! One owner-only, bounded startup pass over interrupted recording publications.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chronobreak_replay_time::FinalizationExpectationsV2;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::publication::{self, DirectoryPins, Receipt};
use super::{METADATA_PENDING_JSON, validate_media};
use crate::poller::RecordingMetadata;
use crate::service::ServiceCommand;
use crate::storage::{METADATA_JSON, VIDEO_MP4, VIDEO_PARTIAL_MP4};

const MAX_ENTRIES: usize = 512;
const MAX_PROBES: usize = 4;
const ADMISSION_BUDGET: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
enum Disposition {
    CanonicalUntouched,
    Recovered,
    MissingEvidence,
    Ambiguous,
    InvalidMedia,
    Deferred,
    Inaccessible,
}

#[derive(Deserialize)]
struct Pending {
    #[serde(flatten)]
    metadata: RecordingMetadata,
    publication: Receipt,
}

struct Prepared {
    pins: DirectoryPins,
    media: File,
    pending: File,
    bytes: Vec<u8>,
    value: Pending,
    path: PathBuf,
    private: bool,
}

enum Inspection {
    Untouched(Disposition, &'static str),
    Candidate(Box<Prepared>),
}

fn inspect(directory: &Path) -> Result<Inspection> {
    let pins = DirectoryPins::open(directory)?;
    let has = |leaf| publication::exists(&directory.join(leaf));
    if has(METADATA_JSON)? {
        return Ok(Inspection::Untouched(
            Disposition::CanonicalUntouched,
            "canonical metadata exists; all artifacts retained",
        ));
    }
    let private = has(VIDEO_PARTIAL_MP4)?;
    let canonical = has(VIDEO_MP4)?;
    if private == canonical {
        return Ok(Inspection::Untouched(
            Disposition::Ambiguous,
            "missing media or multiple media names; all bytes retained",
        ));
    }
    if !has(METADATA_PENDING_JSON)? {
        return Ok(Inspection::Untouched(
            Disposition::MissingEvidence,
            "no staged terminal metadata; footage retained without promotion",
        ));
    }
    let mut pending = publication::open_file(&directory.join(METADATA_PENDING_JSON), false)?;
    if pending.metadata()?.len() > publication::MAX_METADATA_BYTES as u64 {
        return Ok(Inspection::Untouched(
            Disposition::Ambiguous,
            "pending metadata exceeds 64 KiB; all bytes retained",
        ));
    }
    let bytes = publication::read_metadata(&mut pending)?;
    let value: Pending = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return Ok(Inspection::Untouched(
                Disposition::Ambiguous,
                "invalid or legacy pending metadata/receipt; all bytes retained",
            ));
        }
    };
    let metadata = &value.metadata;
    // The library's canonical reader rejects unknown fields. Do not complete a
    // foreign/extended pending document that it would subsequently refuse.
    let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
    let known = serde_json::to_value(metadata)?;
    let foreign_top_level = raw.as_object().is_none_or(|object| {
        object
            .keys()
            .any(|key| key != "publication" && key != "capture" && known.get(key).is_none())
    });
    let foreign_capture = raw
        .get("capture")
        .and_then(|capture| capture.as_object())
        .is_some_and(|object| object.keys().any(|key| known["capture"].get(key).is_none()));
    if foreign_top_level || foreign_capture {
        return Ok(Inspection::Untouched(
            Disposition::Ambiguous,
            "pending metadata has foreign fields; all bytes retained",
        ));
    }
    if metadata.media_timeline.validate().is_err()
        || metadata.schema_version != 2
        || metadata.media_id != metadata.media_timeline.media_id
        || metadata.recording_codec != metadata.media_timeline.video.codec
        || metadata.media_runtime_id != metadata.media_timeline.producer.media_runtime_id
        || metadata.capture_backend != metadata.media_timeline.producer.backend
        || metadata
            .media_timeline
            .capture
            .as_ref()
            .is_some_and(|anchor| anchor.recorded_at != metadata.recorded_at)
    {
        return Ok(Inspection::Untouched(
            Disposition::Ambiguous,
            "pending metadata identity, timeline or producer facts disagree; all bytes retained",
        ));
    }
    let path = directory.join(if private {
        VIDEO_PARTIAL_MP4
    } else {
        VIDEO_MP4
    });
    let media = publication::open_file(&path, false)?;
    if value
        .publication
        .verify(&pins, &media, &metadata.media_id)
        .is_err()
    {
        return Ok(Inspection::Untouched(
            Disposition::Ambiguous,
            "receipt does not match media/directory generation or media ID; all bytes retained",
        ));
    }
    Ok(Inspection::Candidate(Box::new(Prepared {
        pins,
        media,
        pending,
        bytes,
        value,
        path,
        private,
    })))
}

async fn reconcile(
    directory: &Path,
    ffprobe: &Path,
    probes: &mut usize,
    deadline: Instant,
) -> Result<(Disposition, &'static str)> {
    let prepared = match inspect(directory)? {
        Inspection::Untouched(disposition, reason) => return Ok((disposition, reason)),
        Inspection::Candidate(prepared) => prepared,
    };
    if *probes >= MAX_PROBES || Instant::now() >= deadline {
        return Ok((
            Disposition::Deferred,
            "startup probe budget reached; all bytes retained",
        ));
    }
    *probes += 1;
    let metadata = &prepared.value.metadata;
    let expected = &metadata.media_timeline;
    let expectations = FinalizationExpectationsV2 {
        media_id: metadata.media_id.clone(),
        expected_video_codec: expected.video.codec.clone(),
        expected_audio_codec: expected.audio.codec.clone(),
        producer: expected.producer.clone(),
        capture: expected.capture.clone(),
        require_zero_video_start: true,
    };
    let observed = match validate_media(ffprobe, &prepared.path, expectations).await {
        Ok(observed) => observed.0,
        Err(error) => {
            warn!(directory = %directory.display(), error = %format!("{error:#}"),
                "interrupted recording validation failed; all bytes retained");
            return Ok((
                Disposition::InvalidMedia,
                "media failed existing integrity checks; all bytes retained",
            ));
        }
    };
    if &observed != expected {
        return Ok((
            Disposition::InvalidMedia,
            "observed timeline differs from staged metadata; all bytes retained",
        ));
    }
    // ffprobe is reaped. Release its compatible read pins before acquiring DELETE
    // access; then recheck generation and exact pending bytes across that gap.
    drop(prepared.media);
    drop(prepared.pending);
    let media = publication::open_file(&prepared.path, true)?;
    if prepared
        .value
        .publication
        .verify(&prepared.pins, &media, &metadata.media_id)
        .is_err()
    {
        return Ok((
            Disposition::Ambiguous,
            "media generation changed during validation; all bytes retained",
        ));
    }
    let mut pending = publication::open_file(&directory.join(METADATA_PENDING_JSON), true)?;
    if publication::read_metadata(&mut pending)? != prepared.bytes {
        return Ok((
            Disposition::Ambiguous,
            "pending metadata changed during validation; all bytes retained",
        ));
    }
    if publication::exists(&directory.join(METADATA_JSON))? {
        return Ok((
            Disposition::CanonicalUntouched,
            "canonical metadata appeared during validation; all bytes retained",
        ));
    }
    if prepared.private {
        prepared.pins.rename(&media, VIDEO_MP4)?;
    } else if publication::exists(&directory.join(VIDEO_PARTIAL_MP4))? {
        return Ok((
            Disposition::Ambiguous,
            "private media appeared during validation; all bytes retained",
        ));
    }
    // No rollback needed: an interruption/error here leaves the same recoverable
    // canonical-video/pending-metadata state. Neither rename replaces a target.
    prepared.pins.rename(&pending, METADATA_JSON)?;
    Ok((
        Disposition::Recovered,
        "validated recording publication completed in place",
    ))
}

fn bundle_name(name: &str) -> bool {
    let mut parts = name.split('-');
    let timestamp = parts.next().unwrap_or_default();
    if timestamp.parse::<u64>().is_err()
        || timestamp.is_empty()
        || (timestamp.len() > 1 && timestamp.starts_with('0'))
    {
        return false;
    }
    match parts.next() {
        None => true,
        Some(suffix) => suffix.parse::<u16>().is_ok_and(|n| {
            (1..=999).contains(&n) && suffix == n.to_string() && parts.next().is_none()
        }),
    }
}

/// Called once by the exclusively owned recorder service, before capture starts.
/// Returns true when shutdown was received; an admitted probe is always joined.
pub(crate) async fn run(
    output: &Path,
    ffprobe: &Path,
    commands: &mut mpsc::UnboundedReceiver<ServiceCommand>,
) -> Result<bool> {
    let started = Instant::now();
    let deadline = started + ADMISSION_BUDGET;
    let games = output.join("games");
    let _pins = DirectoryPins::open(&games)?;
    let entries = std::fs::read_dir(&games).context("cannot enumerate interrupted recordings")?;
    let mut probes = 0;
    let mut inspected = 0;
    let mut recovered = 0;
    let mut deferred = false;
    for entry in entries.take(MAX_ENTRIES + 1) {
        match commands.try_recv() {
            Ok(ServiceCommand::Shutdown) | Err(mpsc::error::TryRecvError::Disconnected) => {
                return Ok(true);
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
        }
        if inspected == MAX_ENTRIES || Instant::now() >= deadline {
            deferred = true;
            break;
        }
        inspected += 1;
        let entry = entry?;
        if !entry.file_name().to_str().is_some_and(bundle_name) {
            continue;
        }
        let directory = entry.path();
        match reconcile(&directory, ffprobe, &mut probes, deadline).await {
            Ok((Disposition::CanonicalUntouched, _)) => {}
            Ok((disposition, reason)) => {
                recovered += usize::from(disposition == Disposition::Recovered);
                deferred |= disposition == Disposition::Deferred;
                info!(directory = %directory.display(), ?disposition, reason,
                    "interrupted recording disposition");
            }
            Err(error) => {
                warn!(directory = %directory.display(), disposition = ?Disposition::Inaccessible,
                error = %format!("{error:#}"), "interrupted recording preserved")
            }
        }
    }
    info!(
        inspected,
        probes,
        recovered,
        deferred,
        elapsed_ms = started.elapsed().as_millis(),
        "interrupted recording recovery finished; deferred entries wait for a later recorder start"
    );
    Ok(false)
}

#[cfg(test)]
pub(super) mod tests;
