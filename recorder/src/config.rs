use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};

const APP_QUALIFIER: &str = "";
const APP_ORGANIZATION: &str = "";
const APP_NAME: &str = "LeagueReplay";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub recording: RecordingConfig,
    pub storage: StorageConfig,
    pub app: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RecordingConfig {
    pub resolution: String,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StorageConfig {
    pub output_path: String,
    pub auto_delete_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub autostart: bool,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            resolution: "source".to_owned(),
            fps: 60,
            bitrate_kbps: 20_000,
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
        Self { autostart: true }
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
        config.validate()?;
        write_new_config(path, &config)?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !matches!(self.recording.fps, 30 | 60) {
            bail!("recording.fps must be 30 or 60");
        }
        if self.recording.bitrate_kbps == 0 {
            bail!("recording.bitrate_kbps must be greater than zero");
        }
        if parse_resolution(&self.recording.resolution)?.is_some_and(|(width, height)| {
            width < 2 || height < 2 || width % 2 != 0 || height % 2 != 0
        }) {
            bail!("recording.resolution dimensions must be positive even numbers");
        }
        if self.storage.output_path.trim().is_empty() {
            bail!("storage.output_path cannot be empty");
        }
        Ok(())
    }

    pub fn output_path(&self) -> Result<PathBuf> {
        expand_user_path(&self.storage.output_path)
    }
}

pub fn default_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LEAGUE_REPLAY_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let project = ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
        .context("could not determine the platform configuration directory")?;
    Ok(project.config_dir().join("config.toml"))
}

pub fn log_directory() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LEAGUE_REPLAY_LOG_DIR") {
        return Ok(PathBuf::from(path));
    }
    let project = ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
        .context("could not determine the platform log directory")?;
    Ok(project.data_local_dir().join("logs"))
}

pub fn parse_resolution(value: &str) -> Result<Option<(u32, u32)>> {
    if value.eq_ignore_ascii_case("source") {
        return Ok(None);
    }
    let (width, height) = value.split_once('x').with_context(|| {
        format!("invalid resolution {value:?}; expected source or WIDTHxHEIGHT")
    })?;
    let width = width
        .parse::<u32>()
        .with_context(|| format!("invalid resolution width in {value:?}"))?;
    let height = height
        .parse::<u32>()
        .with_context(|| format!("invalid resolution height in {value:?}"))?;
    Ok(Some((width, height)))
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

fn write_new_config(path: &Path, config: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(config).context("failed to serialize default config")?;
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, text)
        .with_context(|| format!("failed to write temporary config {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to install default config {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let config = Config::default();
        assert_eq!(config.recording.resolution, "source");
        assert_eq!(config.recording.fps, 60);
        assert_eq!(config.recording.bitrate_kbps, 20_000);
        assert_eq!(config.storage.output_path, "~/LeagueReplays");
        assert_eq!(config.storage.auto_delete_days, 30);
        assert!(config.app.autostart);
    }

    #[test]
    fn parses_supported_resolutions() {
        assert_eq!(parse_resolution("source").unwrap(), None);
        assert_eq!(parse_resolution("1920x1080").unwrap(), Some((1920, 1080)));
        assert!(parse_resolution("1920").is_err());
        assert!(parse_resolution("1920xnope").is_err());
    }

    #[test]
    fn preserves_unknown_sections_when_reading() {
        let parsed: Config = toml::from_str(
            r#"
                [recording]
                resolution = "source"
                fps = 60
                bitrate_kbps = 20000

                [storage]
                output_path = "~/LeagueReplays"
                auto_delete_days = 30

                [app]
                autostart = true

                [riot_account]
                riot_id = "ignored"
            "#,
        )
        .unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn creates_default_config_once() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("config.toml");
        let loaded = Config::load_or_create(&path).unwrap();
        assert_eq!(loaded, Config::default());
        assert!(path.exists());
    }
}
