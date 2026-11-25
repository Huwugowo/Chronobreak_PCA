use std::collections::{HashMap, hash_map::Entry};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
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
    initialize_with_refresh(&cache_directory, &status, refresh(&cache_directory)).await;
}

async fn initialize_with_refresh(
    cache_directory: &Path,
    status: &RwLock<DdragonStatus>,
    refresh: impl std::future::Future<Output = Result<DdragonStatus>>,
) {
    // Local assets are usable before the optional online patch check completes.
    if let Ok(Some(cached)) = newest_cached_status(cache_directory) {
        *status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = cached;
    }
    let result = refresh.await;
    let next = match result {
        Ok(status) => status,
        Err(error) => match newest_cached_status(cache_directory) {
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
    champions: &ChampionLookups,
    version: &str,
    kind: &str,
    asset: &str,
    allow_network: bool,
) -> Result<PathBuf> {
    if !valid_version(version) {
        bail!("invalid Data Dragon version");
    }
    let cache_key = asset_cache_key(kind, asset)?;
    let icons = cache_directory.join(version).join("icons").join(kind);
    let mut path = icons.join(format!("{cache_key}.png"));
    if path.is_file() {
        return Ok(path);
    }
    let champion_filename = if kind == "champion" {
        let ChampionIcon { filename, name_key } =
            champions.get(cache_directory, version, &cache_key)?;
        let id_key = normalized_asset_key(filename.trim_end_matches(".png"));
        path = icons.join(format!("{name_key}.png"));
        // Keep using portraits cached under either the display name or Riot's ID.
        let id_path = icons.join(format!("{id_key}.png"));
        for candidate in [&path, &id_path] {
            if candidate.is_file() {
                return Ok(candidate.clone());
            }
        }
        Some(filename)
    } else {
        None
    };
    if !allow_network {
        bail!("Data Dragon network access is disabled");
    }

    let remote_url = match champion_filename {
        Some(filename) => format!("{BASE_URL}/cdn/{version}/img/champion/{filename}"),
        None => asset_remote_url(cache_directory, version, kind, asset)?,
    };
    let mut response = client
        .get(&remote_url)
        .send()
        .await
        .with_context(|| format!("failed to download Data Dragon {kind} icon"))?
        .error_for_status()
        .with_context(|| format!("Data Dragon {kind} icon returned an error"))?;
    const MAX_ICON_BYTES: usize = 2 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ICON_BYTES as u64)
    {
        bail!("Data Dragon icon exceeds the download limit");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to read Data Dragon icon")?
    {
        if chunk.len() > MAX_ICON_BYTES.saturating_sub(bytes.len()) {
            bail!("Data Dragon icon exceeds the download limit");
        }
        bytes.extend_from_slice(&chunk);
    }
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
                || !asset.chars().all(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | ' ' | '\'')
                })
            {
                bail!("invalid spell asset");
            }
            Ok(normalized_asset_key(asset))
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
        "spell" => {
            let filename = spell_icon_filename(cache_directory, version, asset)?;
            Ok(format!("{BASE_URL}/cdn/{version}/img/spell/{filename}"))
        }
        "item" => Ok(format!("{BASE_URL}/cdn/{version}/img/item/{asset}.png")),
        "rune" => {
            let icon = rune_icon_path(cache_directory, version, asset.parse::<u32>()?)?;
            Ok(format!("{BASE_URL}/cdn/img/{icon}"))
        }
        _ => bail!("unsupported Data Dragon asset kind"),
    }
}

fn spell_icon_filename(cache_directory: &Path, version: &str, spell: &str) -> Result<String> {
    let path = cache_directory.join(version).join("summoner.json");
    // Older offline caches can still serve the original internal-name routes.
    if !path.exists()
        && spell.starts_with("Summoner")
        && spell.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Ok(format!("{spell}.png"));
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    let wanted = normalized_asset_key(spell);
    let filename = value
        .get("data")
        .and_then(serde_json::Value::as_object)
        .context("spell manifest has no data map")?
        .iter()
        .find_map(|(key, entry)| {
            [
                Some(key.as_str()),
                entry.get("key").and_then(serde_json::Value::as_str),
                entry.get("name").and_then(serde_json::Value::as_str),
            ]
            .into_iter()
            .flatten()
            .any(|name| normalized_asset_key(name) == wanted)
            .then(|| entry.get("image")?.get("full")?.as_str())
            .flatten()
        })
        .context("spell icon is absent from the cached manifest")?;
    if !valid_icon_filename(filename) {
        bail!("spell manifest contains an invalid icon filename");
    }
    Ok(filename.to_owned())
}

#[derive(Clone)]
struct ChampionIcon {
    filename: String,
    name_key: String,
}

#[derive(Default)]
pub(crate) struct ChampionLookups(Mutex<HashMap<String, HashMap<String, ChampionIcon>>>);

impl ChampionLookups {
    fn get(&self, cache_directory: &Path, version: &str, key: &str) -> Result<ChampionIcon> {
        let mut versions = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let icons = match versions.entry(version.to_owned()) {
            Entry::Occupied(entry) => entry.into_mut(),
            // Failed reads remain retryable after the manifest is cached.
            Entry::Vacant(entry) => entry.insert(read_champion_icons(cache_directory, version)?),
        };
        icons
            .get(key)
            .cloned()
            .context("champion icon is absent from the cached manifest")
    }
}

fn read_champion_icons(
    cache_directory: &Path,
    version: &str,
) -> Result<HashMap<String, ChampionIcon>> {
    let path = cache_directory.join(version).join("champion.json");
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let data = value
        .get("data")
        .and_then(serde_json::Value::as_object)
        .context("champion manifest has no data map")?;
    let mut icons = HashMap::new();
    for (key, champion) in data {
        let name = champion.get("name").and_then(serde_json::Value::as_str);
        let filename = champion
            .get("image")
            .and_then(|image| image.get("full"))
            .and_then(serde_json::Value::as_str);
        let Some(filename) = filename.filter(|name| valid_icon_filename(name)) else {
            continue;
        };
        let name_key = normalized_asset_key(name.unwrap_or(key));
        if name_key.is_empty() {
            continue;
        }
        let icon = ChampionIcon {
            filename: filename.to_owned(),
            name_key,
        };
        for alias in [
            Some(key.as_str()),
            champion.get("id").and_then(serde_json::Value::as_str),
            name,
            champion.get("key").and_then(serde_json::Value::as_str),
        ]
        .into_iter()
        .flatten()
        {
            let alias = normalized_asset_key(alias);
            if !alias.is_empty() {
                icons.entry(alias).or_insert_with(|| icon.clone());
            }
        }
    }
    ensure!(
        !icons.is_empty(),
        "champion manifest has no valid portraits"
    );
    Ok(icons)
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
    let spells_path = version_directory.join("summoner.json");

    if item_path.is_file()
        && champion_path.is_file()
        && runes_path.is_file()
        && spells_path.is_file()
    {
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
    let spells_request = client
        .get(format!("{BASE_URL}/cdn/{version}/data/en_US/summoner.json"))
        .send();
    let (item_response, champion_response, runes_response, spells_response) = tokio::try_join!(
        item_request,
        champion_request,
        runes_request,
        spells_request
    )
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
    let spells_bytes = spells_response.error_for_status()?.bytes().await?;
    validate_manifest(&spells_bytes, "spell")?;
    fs::create_dir_all(&version_directory).with_context(|| {
        format!(
            "failed to create Data Dragon cache {}",
            version_directory.display()
        )
    })?;
    config::write_atomic(&item_path, &item_bytes)?;
    config::write_atomic(&champion_path, &champion_bytes)?;
    config::write_atomic(&runes_path, &runes_bytes)?;
    config::write_atomic(&spells_path, &spells_bytes)?;
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

    #[tokio::test]
    async fn cached_assets_are_ready_while_the_online_check_is_pending_and_after_it_fails() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("15.10.1");
        fs::create_dir(&directory).unwrap();
        write_manifest(&directory.join("item.json"), &["Boots"]);
        write_manifest(&directory.join("champion.json"), &["Syndra"]);
        fs::write(
            directory.join("runesReforged.json"),
            br#"[{"id":8200,"icon":"perk-images/Styles/Sorcery/Sorcery.png","slots":[]}]"#,
        )
        .unwrap();
        let status = Arc::new(RwLock::new(DdragonStatus::loading(root.path())));
        let (checking, online_check_started) = tokio::sync::oneshot::channel();
        let (fail_check, failure) = tokio::sync::oneshot::channel();
        let task_status = status.clone();
        let task = tokio::spawn(async move {
            initialize_with_refresh(root.path(), &task_status, async {
                checking.send(()).unwrap();
                failure.await.unwrap();
                bail!("online check unavailable")
            })
            .await;
        });

        online_check_started.await.unwrap();
        let cached = status.read().unwrap().clone();
        assert_eq!(cached.state, "ready");
        assert_eq!(cached.version.as_deref(), Some("15.10.1"));
        assert_eq!(cached.item_count, 1);
        assert_eq!(cached.champion_count, 1);
        assert!(cached.error.is_none());
        fail_check.send(()).unwrap();
        task.await.unwrap();
        let fallback = status.read().unwrap();
        assert_eq!(fallback.state, "ready");
        assert_eq!(fallback.version, cached.version);
        assert!(
            fallback
                .error
                .as_ref()
                .unwrap()
                .contains("online check unavailable")
        );
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
            br#"{"data":{"MonkeyKing":{"id":"MonkeyKing","key":"62","name":"Wukong","image":{"full":"MonkeyKing.png"}}}}"#,
        )
        .unwrap();
        fs::write(
            version.join("runesReforged.json"),
            br#"[{"id":8200,"icon":"perk-images/Styles/Sorcery/Sorcery.png","slots":[{"runes":[{"id":8214,"icon":"perk-images/Styles/Sorcery/SummonAery/SummonAery.png"}]}]}]"#,
        )
        .unwrap();

        let lookups = ChampionLookups::default();
        for alias in ["62", "monkeyking", "wukong"] {
            assert_eq!(
                lookups.get(root.path(), "15.10.1", alias).unwrap().filename,
                "MonkeyKing.png"
            );
        }
        assert_eq!(
            asset_remote_url(root.path(), "15.10.1", "rune", "8214").unwrap(),
            "https://ddragon.leagueoflegends.com/cdn/img/perk-images/Styles/Sorcery/SummonAery/SummonAery.png"
        );
    }

    #[tokio::test]
    async fn numeric_champion_reuses_named_portrait_offline() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("15.10.1");
        let icons = version.join("icons/champion");
        fs::create_dir_all(&icons).unwrap();
        let client = reqwest::Client::new();
        let lookups = ChampionLookups::default();
        let manifest = version.join("champion.json");
        fs::write(&manifest, b"{").unwrap();
        assert!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.1",
                "champion",
                "62",
                false
            )
            .await
            .is_err()
        );
        fs::write(&manifest, br#"{"data":{"MonkeyKing":{"id":"MonkeyKing","key":"62","name":"Wukong","image":{"full":"MonkeyKing.png"}},"Syndra":{"key":"134","name":"Syndra","image":{"full":"Syndra.png"}}}}"#).unwrap();
        let cached = icons.join("wukong.png");
        fs::write(&cached, b"cached image").unwrap();
        assert_eq!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.1",
                "champion",
                "62",
                false
            )
            .await
            .unwrap(),
            cached
        );
        fs::remove_file(&manifest).unwrap();
        let syndra = icons.join("syndra.png");
        fs::write(&syndra, b"cached image").unwrap();
        assert_eq!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.1",
                "champion",
                "134",
                false
            )
            .await
            .unwrap(),
            syndra
        );
        assert_eq!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.1",
                "champion",
                "MonkeyKing",
                false
            )
            .await
            .unwrap(),
            cached
        );
        assert!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.2",
                "champion",
                "134",
                false
            )
            .await
            .is_err()
        );
        assert!(
            ensure_asset(
                &client,
                root.path(),
                &lookups,
                "15.10.1",
                "champion",
                "9999",
                false
            )
            .await
            .is_err()
        );
    }

    #[test]
    fn rejects_unsafe_asset_routes() {
        assert!(asset_cache_key("item", "../1001").is_err());
        assert!(asset_cache_key("spell", "../../secret").is_err());
        assert!(asset_cache_key("other", "1001").is_err());
        assert!(!valid_version("15/10/1"));
    }

    #[test]
    fn resolves_spell_ids_and_recorded_names_without_inventing_unknown_icons() {
        let root = tempfile::tempdir().unwrap();
        let version = root.path().join("16.20.1");
        fs::create_dir(&version).unwrap();
        fs::write(version.join("summoner.json"), br#"{"data":{"SummonerFlash":{"key":"4","name":"Flash","image":{"full":"SummonerFlash.png"}},"SummonerHaste":{"key":"6","name":"Ghost","image":{"full":"SummonerHaste.png"}}}}"#).unwrap();
        for name in ["4", "Flash", "SummonerFlash"] {
            assert!(
                asset_remote_url(root.path(), "16.20.1", "spell", name)
                    .unwrap()
                    .ends_with("/SummonerFlash.png")
            );
        }
        assert!(
            asset_remote_url(root.path(), "16.20.1", "spell", "6")
                .unwrap()
                .ends_with("/SummonerHaste.png")
        );
        assert!(asset_remote_url(root.path(), "16.20.1", "spell", "999999").is_err());
        fs::write(
            version.join("summoner.json"),
            br#"{"data":{"bad":{"key":"4","image":{"full":"../secret.png"}}}}"#,
        )
        .unwrap();
        assert!(asset_remote_url(root.path(), "16.20.1", "spell", "4").is_err());
    }
}
