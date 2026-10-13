//! Optional final facts; invalid enrichment never invalidates replay media.
use chronobreak_league_data::{MAX_RESULT_BYTES, RESULT_FILE, ResultFile};
use chronobreak_replay_time::MediaId;
use std::{fs::File, io::Read, path::Path};
pub(crate) fn read(
    directory: &Path,
    media: &MediaId,
    candidate: Option<&crate::league_match::LeagueMatch>,
) -> Option<ResultFile> {
    let candidate = candidate?;
    let path = directory.join(RESULT_FILE);
    if !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    let file = File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_RESULT_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_RESULT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_RESULT_BYTES {
        return None;
    }
    let facts: ResultFile = serde_json::from_slice(&bytes).ok()?;
    facts.valid_for(media, &candidate.game_id).then_some(facts)
}
