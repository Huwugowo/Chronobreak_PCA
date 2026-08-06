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
pub struct Config {
    pub recording: RecordingConfig,
    pub storage: StorageConfig,
    pub app: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecordingConfig {
    pub profile: RecordingProfile,
    pub codec: CodecPreference,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingProfile {
    #[default]
    Auto,
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

impl RecordingProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::VeryLow => "very_low",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::VeryHigh => "very_high",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CodecPreference {
    #[default]
    Auto,
    H264,
    Hevc,
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

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            profile: RecordingProfile::Auto,
            codec: CodecPreference::Auto,
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
        config.validate()?;
        write_new_config(path, &config)?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
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
        assert_eq!(config.recording.profile, RecordingProfile::Auto);
        assert_eq!(config.recording.codec, CodecPreference::Auto);
        assert_eq!(config.storage.output_path, "~/LeagueReplays");
        assert_eq!(config.storage.auto_delete_days, 30);
        assert!(config.app.autostart);
        assert!(!config.app.hevc_playback_supported);
    }

    #[test]
    fn reads_profile_and_codec_names() {
        let parsed: RecordingConfig = toml::from_str(
            r#"
                profile = "very_high"
                codec = "hevc"
            "#,
        )
        .unwrap();
        assert_eq!(parsed.profile, RecordingProfile::VeryHigh);
        assert_eq!(parsed.codec, CodecPreference::Hevc);
    }

    #[test]
    fn preserves_unknown_sections_when_reading() {
        let parsed: Config = toml::from_str(
            r#"
                [recording]
                profile = "auto"
                codec = "auto"

                [storage]
                output_path = "~/LeagueReplays"
                auto_delete_days = 30

                [app]
                autostart = true
                hevc_playback_supported = false

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
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("profile = \"auto\""));
        assert!(text.contains("codec = \"auto\""));
        assert!(text.contains("hevc_playback_supported = false"));
    }
}
