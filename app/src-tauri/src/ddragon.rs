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
    pub asset_base_url: Option<String>,
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
            asset_base_url: None,
            item_count: 0,
            champion_count: 0,
            cache_directory: cache_directory.to_string_lossy().into_owned(),
            error: None,
        }
    }

    #[cfg(feature = "replay-benchmark")]
    pub fn offline(cache_directory: &Path) -> Self {
        match newest_cached_status(cache_directory) {
            Ok(Some(mut cached)) => {
                cached.error = Some("benchmark mode: fixed offline cache".to_owned());
                cached
            }
            _ => Self {
                state: "offline".to_owned(),
                version: None,
                asset_base_url: None,
                item_count: 0,
                champion_count: 0,
                cache_directory: cache_directory.to_string_lossy().into_owned(),
                error: Some("benchmark mode: Data Dragon network access disabled".to_owned()),
            },
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
                asset_base_url: None,
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

pub async fn ensure_asset(
    client: &reqwest::Client,
    cache_directory: &Path,
    version: &str,
    kind: &str,
    asset: &str,
    allow_network: bool,
) -> Result<PathBuf> {
    if !valid_version(version) {
        bail!("invalid Data Dragon version");
    }
    let cache_key = asset_cache_key(kind, asset)?;
    let path = cache_directory
        .join(version)
        .join("icons")
        .join(kind)
        .join(format!("{cache_key}.png"));
    if path.is_file() {
        return Ok(path);
    }
    if !allow_network {
        bail!("Data Dragon network access is disabled");
    }

    let remote_url = asset_remote_url(cache_directory, version, kind, asset)?;
    let bytes = client
        .get(&remote_url)
        .send()
        .await
        .with_context(|| format!("failed to download Data Dragon {kind} icon"))?
        .error_for_status()
        .with_context(|| format!("Data Dragon {kind} icon returned an error"))?
        .bytes()
        .await
        .with_context(|| format!("failed to read Data Dragon {kind} icon"))?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        bail!("Data Dragon {kind} icon was not a PNG");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    config::write_atomic(&path, &bytes)?;
    Ok(path)
}

fn asset_cache_key(kind: &str, asset: &str) -> Result<String> {
    match kind {
        "champion" => {
            let key = normalized_asset_key(asset);
            if key.is_empty() {
                bail!("invalid champion asset");
            }
            Ok(key)
        }
        "spell" => {
            if asset.is_empty()
                || !asset
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                bail!("invalid spell asset");
            }
            Ok(asset.to_ascii_lowercase())
        }
        "item" | "rune" => asset
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .map(|value| value.to_string())
            .context("invalid numeric Data Dragon asset"),
        _ => bail!("unsupported Data Dragon asset kind"),
    }
}

fn asset_remote_url(
    cache_directory: &Path,
    version: &str,
    kind: &str,
    asset: &str,
) -> Result<String> {
    match kind {
        "champion" => {
            let filename = champion_icon_filename(cache_directory, version, asset)?;
            Ok(format!("{BASE_URL}/cdn/{version}/img/champion/{filename}"))
        }
        "spell" => Ok(format!("{BASE_URL}/cdn/{version}/img/spell/{asset}.png")),
        "item" => Ok(format!("{BASE_URL}/cdn/{version}/img/item/{asset}.png")),
        "rune" => {
            let icon = rune_icon_path(cache_directory, version, asset.parse::<u32>()?)?;
            Ok(format!("{BASE_URL}/cdn/img/{icon}"))
        }
        _ => bail!("unsupported Data Dragon asset kind"),
    }
}

fn champion_icon_filename(cache_directory: &Path, version: &str, champion: &str) -> Result<String> {
    let path = cache_directory.join(version).join("champion.json");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let wanted = normalized_asset_key(champion);
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_object)
        .context("champion manifest has no data map")?;
    let filename = data.iter().find_map(|(key, champion)| {
        let matches = [
            Some(key.as_str()),
            champion.get("id").and_then(serde_json::Value::as_str),
            champion.get("name").and_then(serde_json::Value::as_str),
        ]
        .into_iter()
        .flatten()
        .any(|candidate| normalized_asset_key(candidate) == wanted);
        matches
            .then(|| {
                champion
                    .get("image")
                    .and_then(|image| image.get("full"))
                    .and_then(serde_json::Value::as_str)
            })
            .flatten()
    });
    let filename = filename.context("champion icon is absent from the cached manifest")?;
    if !valid_icon_filename(filename) {
        bail!("champion manifest contains an invalid icon filename");
    }
    Ok(filename.to_owned())
}

fn rune_icon_path(cache_directory: &Path, version: &str, rune_id: u32) -> Result<String> {
    let path = cache_directory.join(version).join("runesReforged.json");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let styles: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let icon = styles.iter().find_map(|style| {
        if style.get("id").and_then(serde_json::Value::as_u64) == Some(u64::from(rune_id)) {
            return style.get("icon").and_then(serde_json::Value::as_str);
        }
        style
            .get("slots")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|slot| {
                slot.get("runes")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .find(|rune| {
                rune.get("id").and_then(serde_json::Value::as_u64) == Some(u64::from(rune_id))
            })
            .and_then(|rune| rune.get("icon"))
            .and_then(serde_json::Value::as_str)
    });
    let icon = icon.context("rune icon is absent from the cached manifest")?;
    if !valid_icon_path(icon) {
        bail!("rune manifest contains an invalid icon path");
    }
    Ok(icon.to_owned())
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.split('.').all(|part| {
            !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
        })
}

fn normalized_asset_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn valid_icon_filename(filename: &str) -> bool {
    filename.ends_with(".png")
        && filename.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn valid_icon_path(path: &str) -> bool {
    path.ends_with(".png")
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
                })
        })
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
    let runes_path = version_directory.join("runesReforged.json");

    if item_path.is_file() && champion_path.is_file() && runes_path.is_file() {
        return summarize_cache(cache_directory, version);
    }

    let item_request = client
        .get(format!("{BASE_URL}/cdn/{version}/data/en_US/item.json"))
        .send();
    let champion_request = client
        .get(format!("{BASE_URL}/cdn/{version}/data/en_US/champion.json"))
        .send();
    let runes_request = client
        .get(format!(
            "{BASE_URL}/cdn/{version}/data/en_US/runesReforged.json"
        ))
        .send();
    let (item_response, champion_response, runes_response) =
        tokio::try_join!(item_request, champion_request, runes_request)
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
    let runes_bytes = runes_response
        .error_for_status()
        .context("rune manifest returned an error")?
        .bytes()
        .await
        .context("failed to read rune manifest")?;

    validate_manifest(&item_bytes, "item")?;
    validate_manifest(&champion_bytes, "champion")?;
    validate_rune_manifest(&runes_bytes)?;
    fs::create_dir_all(&version_directory).with_context(|| {
        format!(
            "failed to create Data Dragon cache {}",
            version_directory.display()
        )
    })?;
    config::write_atomic(&item_path, &item_bytes)?;
    config::write_atomic(&champion_path, &champion_bytes)?;
    config::write_atomic(&runes_path, &runes_bytes)?;
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

fn validate_rune_manifest(bytes: &[u8]) -> Result<()> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).context("failed to parse rune manifest")?;
    if value.as_array().is_none_or(Vec::is_empty) {
        bail!("rune manifest has no style list");
    }
    Ok(())
}

fn summarize_cache(cache_directory: &Path, version: &str) -> Result<DdragonStatus> {
    let version_directory = cache_directory.join(version);
    let item_count = manifest_count(&version_directory.join("item.json"))?;
    let champion_count = manifest_count(&version_directory.join("champion.json"))?;
    let runes_path = version_directory.join("runesReforged.json");
    let rune_bytes = fs::read(&runes_path)
        .with_context(|| format!("failed to read {}", runes_path.display()))?;
    validate_rune_manifest(&rune_bytes)?;
    Ok(DdragonStatus {
        state: "ready".to_owned(),
        version: Some(version.to_owned()),
        asset_base_url: None,
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
            directory.join("item.json").is_file()
                && directory.join("champion.json").is_file()
                && directory.join("runesReforged.json").is_file()
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
            fs::write(
                directory.join("runesReforged.json"),
                br#"[{"id":8200,"icon":"perk-images/Styles/Sorcery/Sorcery.png","slots":[]}]"#,
            )
            .unwrap();
        }

        let status = newest_cached_status(root.path()).unwrap().unwrap();
        assert_eq!(status.version.as_deref(), Some("15.10.1"));
        assert_eq!(status.item_count, 2);
        assert_eq!(status.champion_count, 1);
    }

    #[test]
    fn rejects_a_manifest_without_a_data_map() {
        assert!(validate_manifest(br#"{"version":"1"}"#, "item").is_err());
        assert!(validate_rune_manifest(br#"[]"#).is_err());
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

    #[test]
    fn resolves_versioned_champion_and_rune_asset_urls() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("15.10.1");
        fs::create_dir(&version).unwrap();
        fs::write(
            version.join("champion.json"),
            br#"{"data":{"MonkeyKing":{"id":"MonkeyKing","name":"Wukong","image":{"full":"MonkeyKing.png"}}}}"#,
        )
        .unwrap();
        fs::write(
            version.join("runesReforged.json"),
            br#"[{"id":8200,"icon":"perk-images/Styles/Sorcery/Sorcery.png","slots":[{"runes":[{"id":8214,"icon":"perk-images/Styles/Sorcery/SummonAery/SummonAery.png"}]}]}]"#,
        )
        .unwrap();

        assert_eq!(
            asset_remote_url(root.path(), "15.10.1", "champion", "Wukong").unwrap(),
            "https://ddragon.leagueoflegends.com/cdn/15.10.1/img/champion/MonkeyKing.png"
        );
        assert_eq!(
            asset_remote_url(root.path(), "15.10.1", "rune", "8214").unwrap(),
            "https://ddragon.leagueoflegends.com/cdn/img/perk-images/Styles/Sorcery/SummonAery/SummonAery.png"
        );
    }

    #[test]
    fn rejects_unsafe_asset_routes() {
        assert!(asset_cache_key("item", "../1001").is_err());
        assert!(asset_cache_key("spell", "../../secret").is_err());
        assert!(asset_cache_key("other", "1001").is_err());
        assert!(!valid_version("15/10/1"));
    }
}
