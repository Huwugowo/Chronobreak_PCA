//! Joined post-teardown maintenance; never on the capture admission path.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use chronobreak_replay_time::FinalizationExpectationsV2;
use serde::Deserialize;
use tracing::{info, warn};

use super::publication::{self, DirectoryPins, Receipt};
use super::{METADATA_PENDING_JSON, validate_media};
use crate::poller::RecordingMetadata;
use crate::storage::{METADATA_JSON, VIDEO_MP4, VIDEO_PARTIAL_MP4};

mod discovery;

const MAX_ENTRIES: usize = 512;
const MAX_SEARCHES: usize = 512;
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

fn inspect_when_idle(
    directory: &Path,
    may_inspect: &mut impl FnMut() -> bool,
) -> Result<Inspection> {
    // This only declines recovery. It grants no authority to read or mutate media,
    // so a stale observation/reparse can at worst defer work to a later pass.
    if publication::exists(&directory.join(METADATA_JSON))? {
        return Ok(Inspection::Untouched(
            Disposition::CanonicalUntouched,
            "canonical metadata exists; all artifacts retained",
        ));
    }
    if !may_inspect() {
        return Ok(Inspection::Untouched(
            Disposition::Deferred,
            "League present; candidate inspection deferred without opening media",
        ));
    }
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

async fn reconcile_when_idle(
    directory: &Path,
    ffprobe: &Path,
    probes: &mut usize,
    deadline: Instant,
    may_probe: &mut impl FnMut() -> bool,
) -> Result<(Disposition, &'static str)> {
    let prepared = match inspect_when_idle(directory, may_probe)? {
        Inspection::Untouched(disposition, reason) => return Ok((disposition, reason)),
        Inspection::Candidate(prepared) => prepared,
    };
    if *probes >= MAX_PROBES || Instant::now() >= deadline || !may_probe() {
        return Ok((
            Disposition::Deferred,
            "maintenance budget reached or League present; all bytes retained",
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

#[derive(Debug, Default)]
pub(crate) struct Report {
    inspected: usize,
    searches: usize,
    probes: usize,
    recovered: usize,
    #[cfg(test)]
    attempted: Vec<PathBuf>,
}

// Finish enumeration synchronously and close its search handle before any await.
// There is no retained cursor/handle or blocking discovery worker during capture.
fn discover(
    games: &Path,
    prefix: &str,
    limit: usize,
    deadline: Instant,
) -> Result<(Vec<String>, bool)> {
    let Some(mut search) = discovery::Search::open(games, prefix)? else {
        return Ok((Vec::new(), true));
    };
    let mut names = Vec::new();
    while names.len() < limit && Instant::now() < deadline {
        match search.next()? {
            Some(name) => names.push(name),
            None => return Ok((names, true)),
        }
    }
    Ok((names, false))
}

async fn attempt(
    directory: &Path,
    ffprobe: &Path,
    report: &mut Report,
    deadline: Instant,
    may_probe: &mut impl FnMut() -> bool,
) -> bool {
    #[cfg(test)]
    let before = report.probes;
    let result =
        reconcile_when_idle(directory, ffprobe, &mut report.probes, deadline, may_probe).await;
    #[cfg(test)]
    if report.probes != before {
        report.attempted.push(directory.to_owned());
    }
    match result {
        Ok((Disposition::CanonicalUntouched, _)) => {}
        Ok((disposition, reason)) => {
            report.recovered += usize::from(disposition == Disposition::Recovered);
            info!(directory = %directory.display(), ?disposition, reason,
                "interrupted recording disposition");
            return disposition != Disposition::Deferred;
        }
        Err(error) => {
            warn!(directory = %directory.display(), disposition = ?Disposition::Inaccessible,
            error = %format!("{error:#}"), "interrupted recording preserved")
        }
    }
    true
}

/// Only after service teardown, while the production owner is still held.
/// Fresh process checks gate discovery regions, candidate pinning and probes.
/// Completed bundles need only a cheap metadata-existence check. The caller
/// joins this future before ShutdownComplete; no command receiver is consumed.
pub(crate) async fn run(
    output: &Path,
    ffprobe: &Path,
    mut idle: impl FnMut() -> bool,
) -> Result<Report> {
    // Once League is observed, never resume this maintenance pass, even if a
    // subsequent observation could already see it exit again.
    let mut remained_idle = true;
    let mut may_continue = || {
        remained_idle = remained_idle && idle();
        remained_idle
    };
    let mut report = Report::default();
    if !may_continue() {
        return Ok(report);
    }
    let started = Instant::now();
    let deadline = started + ADMISSION_BUDGET;
    let games = output.join("games");
    let pins = DirectoryPins::open(&games)?;
    let mut cursor = match discovery::Cursor::open(&games, &pins) {
        Ok(cursor) => Some(cursor),
        Err(error) => {
            warn!(error = %format!("{error:#}"),
                "recovery progress unavailable; cursor retained, bounded pass starts at root");
            None
        }
    };
    let mut prefix = cursor
        .as_ref()
        .map_or_else(String::new, |cursor| cursor.prefix.clone());
    let mut complete = false;
    'pass: while report.inspected < MAX_ENTRIES
        && report.searches < MAX_SEARCHES
        && report.probes < MAX_PROBES
        && Instant::now() < deadline
    {
        if !may_continue() {
            break;
        }
        // A bundle name can itself prefix other bundle names (1, 10, 1-1).
        // Inspect it before splitting; never lose it in the descendant search.
        if bundle_name(&prefix) {
            report.inspected += 1;
            let directory = games.join(&prefix);
            if publication::exists(&directory)?
                && !attempt(
                    &directory,
                    ffprobe,
                    &mut report,
                    deadline,
                    &mut may_continue,
                )
                .await
            {
                break;
            } // Not attempted: preserve this exact frontier.
        }
        if report.inspected == MAX_ENTRIES
            || report.probes == MAX_PROBES
            || Instant::now() >= deadline
        {
            prefix = discovery::descend(&prefix)
                .or_else(|| discovery::advance(&prefix))
                .unwrap_or_default();
            break;
        }
        // An exact-prefix candidate may just have spent its full probe deadline.
        // Reobserve League before beginning another filesystem search.
        if !may_continue() {
            break;
        }
        report.searches += 1;
        let (names, exhausted) =
            discover(&games, &prefix, MAX_ENTRIES - report.inspected, deadline)?;
        report.inspected += names.len();
        for name in names {
            if name == prefix || !name.starts_with(&prefix) || !bundle_name(&name) {
                continue;
            }
            if report.probes == MAX_PROBES
                || Instant::now() >= deadline
                || !attempt(
                    &games.join(name),
                    ffprobe,
                    &mut report,
                    deadline,
                    &mut may_continue,
                )
                .await
            {
                prefix = discovery::descend(&prefix)
                    .or_else(|| discovery::advance(&prefix))
                    .unwrap_or_default();
                break 'pass;
            }
        }
        let next = if exhausted {
            discovery::advance(&prefix)
        } else {
            discovery::descend(&prefix).or_else(|| discovery::advance(&prefix))
        };
        match next {
            Some(next) => prefix = next,
            None => {
                prefix.clear();
                complete = true;
                break;
            }
        }
    }
    // Capture wins even over persisting an advisory progress hint.
    if let Some(cursor) = &mut cursor
        && may_continue()
    {
        cursor.save(&prefix)?;
    }
    info!(
        inspected = report.inspected,
        searches = report.searches,
        probes = report.probes,
        recovered = report.recovered,
        complete,
        next_prefix = prefix,
        elapsed_ms = started.elapsed().as_millis(),
        "interrupted recording maintenance finished; deferred entries wait for an eligible orderly exit"
    );
    Ok(report)
}

#[cfg(test)]
pub(super) mod tests;
