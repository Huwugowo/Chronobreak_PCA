use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atomicwrites::{AllowOverwrite, AtomicFile};
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, value};

const APP_NAME: &str = "LeagueReplay";
const RETENTION_OPTIONS: [u32; 6] = [0, 7, 14, 30, 60, 90];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub recording: RecordingConfig,
    pub storage: StorageConfig,
    pub app: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordingConfig {
    pub profile: String,
    pub codec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    pub output_path: String,
    pub auto_delete_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub autostart: bool,
    pub hevc_playback_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub output_path: String,
    pub auto_delete_days: u32,
    pub hevc_playback_supported: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct SettingsUpdate {
    pub output_path: String,
    pub auto_delete_days: u32,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            profile: "auto".to_owned(),
            codec: "auto".to_owned(),
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            output_path: "~/LeagueReplays".to_owned(),
            auto_delete_days: 30,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            autostart: true,
            hevc_playback_supported: false,
        }
    }
}

impl Config {
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let text = fs::read_to_string(path)
                .with_context(|| format!("failed to read config {}", path.display()))?;
            let config: Self = toml::from_str(&text)
                .with_context(|| format!("failed to parse config {}", path.display()))?;
            config.validate()?;
            return Ok(config);
        }

        let config = Self::default();
        save(path, &config)?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.storage.output_path.trim().is_empty() {
            bail!("output folder cannot be empty");
        }
        if !RETENTION_OPTIONS.contains(&self.storage.auto_delete_days) {
            bail!("auto-delete must be Never, 7, 14, 30, 60, or 90 days");
        }
        Ok(())
    }

    pub fn settings(&self) -> Settings {
        Settings {
            output_path: self.storage.output_path.clone(),
            auto_delete_days: self.storage.auto_delete_days,
            hevc_playback_supported: self.app.hevc_playback_supported,
        }
    }

    pub fn apply_settings(&mut self, update: SettingsUpdate) -> Result<()> {
        self.storage.output_path = update.output_path.trim().to_owned();
        self.storage.auto_delete_days = update.auto_delete_days;
        self.validate()
    }

    pub fn resolved_output_path(&self) -> Result<PathBuf> {
        if let Some(path) = env::var_os("LEAGUE_REPLAY_OUTPUT_PATH") {
            let path = PathBuf::from(path);
            if path.as_os_str().is_empty() {
                bail!("LEAGUE_REPLAY_OUTPUT_PATH cannot be empty");
            }
            return Ok(path);
        }
        expand_user_path(&self.storage.output_path)
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LEAGUE_REPLAY_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let project = ProjectDirs::from("", "", APP_NAME)
        .context("could not determine the shared configuration directory")?;
    Ok(project.config_dir().join("config.toml"))
}

pub fn save(path: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let mut document = if path.exists() {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse config {}", path.display()))?
    } else {
        DocumentMut::new()
    };

    let recording = table_mut(&mut document, "recording");
    recording["profile"] = value(&config.recording.profile);
    recording["codec"] = value(&config.recording.codec);

    let storage = table_mut(&mut document, "storage");
    storage["output_path"] = value(&config.storage.output_path);
    storage["auto_delete_days"] = value(i64::from(config.storage.auto_delete_days));

    let app = table_mut(&mut document, "app");
    app["autostart"] = value(config.app.autostart);
    app["hevc_playback_supported"] = value(config.app.hevc_playback_supported);

    write_atomic(path, document.to_string().as_bytes())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(|error| anyhow::anyhow!("failed to atomically write {}: {error}", path.display()))
}

fn table_mut<'a>(document: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    if !document.as_table().contains_key(key) {
        document[key] = Item::Table(Table::new());
    }
    document[key]
        .as_table_mut()
        .expect("config section must be a table")
}

fn expand_user_path(value: &str) -> Result<PathBuf> {
    if value == "~" {
        return Ok(BaseDirs::new()
            .context("could not determine the user home directory")?
            .home_dir()
            .to_path_buf());
    }
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        return Ok(BaseDirs::new()
            .context("could not determine the user home directory")?
            .home_dir()
            .join(rest));
    }
    Ok(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unowned_sections_when_saving() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            r#"[recording]
profile = "auto"
codec = "auto"

[storage]
output_path = "~/LeagueReplays"
auto_delete_days = 30

[app]
autostart = true
hevc_playback_supported = false

[future_feature]
enabled = true
"#,
        )
        .unwrap();

        let mut config = Config::load_or_create(&path).unwrap();
        config.storage.auto_delete_days = 14;
        save(&path, &config).unwrap();

        let saved = fs::read_to_string(path).unwrap();
        assert!(saved.contains("auto_delete_days = 14"));
        assert!(saved.contains("[future_feature]"));
        assert!(saved.contains("enabled = true"));
    }

    #[test]
    fn zero_retention_means_never() {
        let mut config = Config::default();
        config
            .apply_settings(SettingsUpdate {
                output_path: "~/LeagueReplays".to_owned(),
                auto_delete_days: 0,
            })
            .unwrap();
        assert_eq!(config.storage.auto_delete_days, 0);
    }

    #[test]
    fn rejects_unknown_retention_values() {
        let mut config = Config::default();
        assert!(
            config
                .apply_settings(SettingsUpdate {
                    output_path: "~/LeagueReplays".to_owned(),
                    auto_delete_days: 13,
                })
                .is_err()
        );
    }
}
