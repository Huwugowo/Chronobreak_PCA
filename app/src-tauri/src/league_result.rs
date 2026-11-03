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
    for leaf in [chronobreak_league_data::HISTORY_RESULT_FILE, RESULT_FILE] {
        if let Some(result) = read_file(directory, media, &candidate.game_id, leaf) {
            return Some(result);
        }
    }
    None
}
fn read_file(directory: &Path, media: &MediaId, game: &str, leaf: &str) -> Option<ResultFile> {
    let path = directory.join(leaf);
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
    facts.valid_for(media, game).then_some(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefers_valid_history_and_falls_back_without_changing_eog_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let media = MediaId::new_v4();
        std::fs::write(
            directory.path().join("league_match.json"),
            serde_json::to_vec(&crate::league_match::fixture(&media)).unwrap(),
        )
        .unwrap();
        let candidate = crate::league_match::read(directory.path(), &media).unwrap();
        let eog = crate::library::tests::result_fixture(&media);
        let bytes = serde_json::to_vec(&eog).unwrap();
        std::fs::write(directory.path().join(RESULT_FILE), &bytes).unwrap();
        let history_path = directory
            .path()
            .join(chronobreak_league_data::HISTORY_RESULT_FILE);
        let mut history = eog.clone();
        history.source = "lcu_match_history".into();
        std::fs::write(&history_path, serde_json::to_vec(&history).unwrap()).unwrap();
        assert_eq!(
            read(directory.path(), &media, Some(&candidate))
                .unwrap()
                .source,
            "lcu_match_history"
        );
        history.game_id = "123".into();
        std::fs::write(&history_path, serde_json::to_vec(&history).unwrap()).unwrap();
        assert_eq!(
            read(directory.path(), &media, Some(&candidate))
                .unwrap()
                .source,
            "lcu_eog"
        );
        assert_eq!(
            std::fs::read(directory.path().join(RESULT_FILE)).unwrap(),
            bytes
        );
    }
}
