use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const MANIFEST: &str = include_str!("../resources/music/music.json");
const MOMENTUM: &[u8] = include_bytes!("../resources/music/momentum.mp3");

#[derive(Debug, Clone, Deserialize)]
struct ManifestTrack {
    filename: String,
    display_name: String,
    mood: String,
    duration_s: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BuiltInMusicTrack {
    pub filename: String,
    pub display_name: String,
    pub mood: String,
    pub duration_s: u32,
    pub preview_url: String,
}

fn manifest() -> Result<Vec<ManifestTrack>> {
    serde_json::from_str(MANIFEST).context("bundled music manifest is invalid")
}

pub fn install(cache_root: &Path) -> Result<PathBuf> {
    let directory = cache_root.join("music");
    fs::create_dir_all(&directory)
        .with_context(|| format!("failed to create music cache {}", directory.display()))?;
    let path = directory.join("momentum.mp3");
    let current = fs::read(&path).ok();
    if current.as_deref() != Some(MOMENTUM) {
        fs::write(&path, MOMENTUM)
            .with_context(|| format!("failed to install bundled music {}", path.display()))?;
    }
    Ok(directory)
}

pub fn tracks(origin: &str) -> Result<Vec<BuiltInMusicTrack>> {
    manifest()?
        .into_iter()
        .map(|track| {
            if bytes_for(&track.filename).is_none() {
                bail!("music manifest references an unknown file")
            }
            Ok(BuiltInMusicTrack {
                preview_url: format!("{origin}/music/{}", track.filename),
                filename: track.filename,
                display_name: track.display_name,
                mood: track.mood,
                duration_s: track.duration_s,
            })
        })
        .collect()
}

pub fn resolve(directory: &Path, filename: &str) -> Result<PathBuf> {
    if bytes_for(filename).is_none() {
        bail!("unknown built-in music track")
    }
    let path = directory.join(filename);
    if !path.is_file() {
        bail!("built-in music track is unavailable")
    }
    Ok(path)
}

pub fn bytes_for(filename: &str) -> Option<&'static [u8]> {
    match filename {
        "momentum.mp3" => Some(MOMENTUM),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_only_exposes_embedded_tracks() {
        let tracks = tracks("http://127.0.0.1:9000").unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].filename, "momentum.mp3");
        assert_eq!(
            tracks[0].preview_url,
            "http://127.0.0.1:9000/music/momentum.mp3"
        );
    }
}
