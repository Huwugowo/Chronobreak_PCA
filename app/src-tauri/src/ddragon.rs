use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::config;

const BASE_URL: &str = "https://ddragon.leagueoflegends.com";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DdragonStatus {
    pub state: String,
    pub version: Option<String>,
    pub item_count: u64,
    pub champion_count: u64,
    pub cache_directory: String,
    pub error: Option<String>,
}

impl DdragonStatus {
    pub fn loading(cache_directory: &Path) -> Self {
        Self {
            state: "loading".to_owned(),
            version: None,
            item_count: 0,
            champion_count: 0,
            cache_directory: cache_directory.to_string_lossy().into_owned(),
            error: None,
        }
    }
}

pub async fn initialize(cache_directory: PathBuf, status: Arc<RwLock<DdragonStatus>>) {
    let result = refresh(&cache_directory).await;
    let next = match result {
        Ok(status) => status,
        Err(error) => match newest_cached_status(&cache_directory) {
            Ok(Some(mut cached)) => {
                cached.error = Some(format!("using cached assets: {error}"));
                cached
            }
            _ => DdragonStatus {
                state: "offline".to_owned(),
                version: None,
                item_count: 0,
                champion_count: 0,
                cache_directory: cache_directory.to_string_lossy().into_owned(),
                error: Some(error.to_string()),
            },
        },
    };
    *status
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
}

pub fn resolve_item_name(
    cache_directory: &Path,
    version: &str,
    item_id: &str,
) -> Result<Option<String>> {
    let path = cache_directory.join(version).join("item.json");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value
        .get("data")
        .and_then(|data| data.get(item_id))
        .and_then(|item| item.get("name"))
        .and_then(|name| name.as_str())
        .map(str::to_owned))
}

async fn refresh(cache_directory: &Path) -> Result<DdragonStatus> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .context("failed to build Data Dragon client")?;
    let versions = client
        .get(format!("{BASE_URL}/api/versions.json"))
        .send()
        .await
        .context("failed to check the Data Dragon version")?
        .error_for_status()
        .context("Data Dragon version check returned an error")?
        .json::<Vec<String>>()
        .await
        .context("failed to parse Data Dragon versions")?;
    let version = versions
        .first()
        .context("Data Dragon returned no versions")?;
    let version_directory = cache_directory.join(version);
    let item_path = version_directory.join("item.json");
    let champion_path = version_directory.join("champion.json");

    if item_path.is_file() && champion_path.is_file() {
        return summarize_cache(cache_directory, version);
    }

    let item_request = client
        .get(format!("{BASE_URL}/cdn/{version}/data/en_US/item.json"))
        .send();
    let champion_request = client
        .get(format!("{BASE_URL}/cdn/{version}/data/en_US/champion.json"))
        .send();
    let (item_response, champion_response) = tokio::try_join!(item_request, champion_request)
        .context("failed to download Data Dragon manifests")?;
    let item_bytes = item_response
        .error_for_status()
        .context("item manifest returned an error")?
        .bytes()
        .await
        .context("failed to read item manifest")?;
    let champion_bytes = champion_response
        .error_for_status()
        .context("champion manifest returned an error")?
        .bytes()
        .await
        .context("failed to read champion manifest")?;

    validate_manifest(&item_bytes, "item")?;
    validate_manifest(&champion_bytes, "champion")?;
    fs::create_dir_all(&version_directory).with_context(|| {
        format!(
            "failed to create Data Dragon cache {}",
            version_directory.display()
        )
    })?;
    config::write_atomic(&item_path, &item_bytes)?;
    config::write_atomic(&champion_path, &champion_bytes)?;
    summarize_cache(cache_directory, version)
}

fn validate_manifest(bytes: &[u8], label: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .with_context(|| format!("failed to parse {label} manifest"))?;
    if value
        .get("data")
        .and_then(|data| data.as_object())
        .is_none()
    {
        bail!("{label} manifest has no data map");
    }
    Ok(())
}

fn summarize_cache(cache_directory: &Path, version: &str) -> Result<DdragonStatus> {
    let version_directory = cache_directory.join(version);
    let item_count = manifest_count(&version_directory.join("item.json"))?;
    let champion_count = manifest_count(&version_directory.join("champion.json"))?;
    Ok(DdragonStatus {
        state: "ready".to_owned(),
        version: Some(version.to_owned()),
        item_count,
        champion_count,
        cache_directory: cache_directory.to_string_lossy().into_owned(),
        error: None,
    })
}

fn manifest_count(path: &Path) -> Result<u64> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(value
        .get("data")
        .and_then(|data| data.as_object())
        .map(|data| data.len() as u64)
        .unwrap_or(0))
}

fn newest_cached_status(cache_directory: &Path) -> Result<Option<DdragonStatus>> {
    if !cache_directory.exists() {
        return Ok(None);
    }
    let mut versions = fs::read_dir(cache_directory)
        .with_context(|| format!("failed to read {}", cache_directory.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|version| {
            let directory = cache_directory.join(version);
            directory.join("item.json").is_file() && directory.join("champion.json").is_file()
        })
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| version_key(version));
    versions
        .pop()
        .map(|version| summarize_cache(cache_directory, &version))
        .transpose()
}

fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_manifest(path: &Path, names: &[&str]) {
        let data = names
            .iter()
            .map(|name| ((*name).to_owned(), serde_json::json!({ "name": name })))
            .collect::<serde_json::Map<_, _>>();
        fs::write(
            path,
            serde_json::to_vec(&serde_json::json!({ "data": data })).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn uses_the_newest_complete_cached_patch() {
        let root = tempfile::tempdir().unwrap();
        for version in ["15.9.1", "15.10.1"] {
            let directory = root.path().join(version);
            fs::create_dir(&directory).unwrap();
            write_manifest(&directory.join("item.json"), &["Boots", "Sword"]);
            write_manifest(&directory.join("champion.json"), &["Syndra"]);
        }

        let status = newest_cached_status(root.path()).unwrap().unwrap();
        assert_eq!(status.version.as_deref(), Some("15.10.1"));
        assert_eq!(status.item_count, 2);
        assert_eq!(status.champion_count, 1);
    }

    #[test]
    fn rejects_a_manifest_without_a_data_map() {
        assert!(validate_manifest(br#"{"version":"1"}"#, "item").is_err());
    }

    #[test]
    fn resolves_an_item_name_from_the_cached_manifest() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("15.10.1");
        fs::create_dir(&version).unwrap();
        fs::write(
            version.join("item.json"),
            br#"{"data":{"1001":{"name":"Boots"}}}"#,
        )
        .unwrap();

        assert_eq!(
            resolve_item_name(root.path(), "15.10.1", "1001").unwrap(),
            Some("Boots".to_owned())
        );
        assert_eq!(
            resolve_item_name(root.path(), "15.10.1", "missing").unwrap(),
            None
        );
    }
}
