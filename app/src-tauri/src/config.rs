use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use atomicwrites::{AllowOverwrite, AtomicFile};
use chronobreak_recorder_control::config_lock::ConfigLock;
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
#[serde(default)]
pub struct RecordingConfig {
    pub profile: String,
    pub codec: String,
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
        let transaction = ConfigMutation::open(path)?;
        if !path.exists() {
            return transaction.commit();
        }
        Ok(transaction.config.clone())
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

/// Select an isolated native cache directory when explicitly requested.
pub fn app_data_path(default: PathBuf) -> Result<PathBuf> {
    app_data_path_with_override(default, env::var_os("LEAGUE_REPLAY_APP_DATA_DIR"))
}

fn app_data_path_with_override(
    default: PathBuf,
    override_path: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    let path = match override_path {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("LEAGUE_REPLAY_APP_DATA_DIR must be an absolute directory");
            }
            path
        }
        None => default,
    };
    fs::create_dir_all(&path)
        .with_context(|| format!("could not create app data directory {}", path.display()))?;
    Ok(path)
}

pub enum ConfigPatch {
    Storage(SettingsUpdate),
    Hevc(bool),
    Autostart(bool),
}

/// Owns the cross-process lock from the fresh read through atomic publication.
pub struct ConfigMutation {
    _lock: ConfigLock,
    path: PathBuf,
    document: DocumentMut,
    pub config: Config,
}

impl ConfigMutation {
    pub fn open(path: &Path) -> Result<Self> {
        let lock = ConfigLock::acquire(path)?;
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                toml::to_string_pretty(&Config::default())?
            }
            Err(error) => return Err(error).context("failed to read latest configuration"),
        };
        let config: Config = toml::from_str(&text).context("invalid persisted configuration")?;
        config.validate()?;
        let document: DocumentMut = text.parse()?;
        for section in ["recording", "storage", "app"] {
            if document
                .get(section)
                .is_some_and(|item| !item.is_table() && item.as_inline_table().is_none())
            {
                bail!("configuration section must be a table: {section}");
            }
        }
        Ok(Self {
            _lock: lock,
            path: path.into(),
            document,
            config,
        })
    }

    pub fn patch(&mut self, patch: ConfigPatch) -> Result<()> {
        match patch {
            ConfigPatch::Storage(update) => {
                self.config.apply_settings(update)?;
                let table = table_mut(&mut self.document, "storage");
                table["output_path"] = value(&self.config.storage.output_path);
                table["auto_delete_days"] = value(i64::from(self.config.storage.auto_delete_days));
            }
            ConfigPatch::Hevc(supported) => {
                self.config.app.hevc_playback_supported = supported;
                table_mut(&mut self.document, "app")["hevc_playback_supported"] = value(supported);
            }
            ConfigPatch::Autostart(enabled) => {
                self.config.app.autostart = enabled;
                table_mut(&mut self.document, "app")["autostart"] = value(enabled);
            }
        }
        self.config.validate()
    }

    pub fn commit(self) -> Result<Config> {
        let text = self.document.to_string();
        let merged: Config = toml::from_str(&text)?;
        merged.validate()?;
        write_atomic(&self.path, text.as_bytes())?;
        Ok(merged)
    }
}

pub fn mutate(path: &Path, patch: ConfigPatch) -> Result<Config> {
    let mut transaction = ConfigMutation::open(path)?;
    transaction.patch(patch)?;
    transaction.commit()
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(|error| anyhow::anyhow!("failed to atomically write {}: {error}", path.display()))
}

fn table_mut<'a>(document: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    if let Some(inline) = document.get(key).and_then(Item::as_inline_table) {
        let table = inline.clone().into_table();
        document[key] = Item::Table(table);
    }
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
    fn cross_process_patch_child() {
        let Some(root) = std::env::var_os("CHRONOBREAK_CONFIG_PATCH_FIXTURE") else {
            return;
        };
        let root = PathBuf::from(root);
        assert!(root.join("fixture-sentinel").is_file());
        let field = std::env::var("CHRONOBREAK_CONFIG_PATCH_FIELD").unwrap();
        let path = root.join("config.toml");
        let stale = Config::load_or_create(&path).unwrap();
        fs::write(root.join(format!("ready-{field}")), "ready").unwrap();
        let started = std::time::Instant::now();
        while !root.join("go").exists() {
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let patch = match field.as_str() {
            "storage" => ConfigPatch::Storage(SettingsUpdate {
                output_path: "fixture-output".into(),
                auto_delete_days: 14,
            }),
            "hevc" => ConfigPatch::Hevc(!stale.app.hevc_playback_supported),
            "autostart" => ConfigPatch::Autostart(!stale.app.autostart),
            _ => panic!("unexpected fixture field"),
        };
        mutate(&path, patch).unwrap();
        // A later creator must read the winner, never overwrite it with defaults.
        Config::load_or_create(&path).unwrap();
    }

    #[test]
    fn cross_process_latest_under_lock_preserves_disjoint_changes_and_unknown_fields() {
        use std::process::{Command, Stdio};
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("fixture-sentinel"), "fixture").unwrap();
        fs::write(
            root.path().join("config.toml"),
            "[future_feature]\nenabled = true\n[app]\nfuture_option = 42\n",
        )
        .unwrap();
        struct Children(Vec<std::process::Child>);
        impl Drop for Children {
            fn drop(&mut self) {
                for child in &mut self.0 {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
        let mut children = Children(Vec::new());
        for field in ["storage", "hevc", "autostart"] {
            children.0.push(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "config::tests::cross_process_patch_child",
                        "--nocapture",
                    ])
                    .env("CHRONOBREAK_CONFIG_PATCH_FIXTURE", root.path())
                    .env("CHRONOBREAK_CONFIG_PATCH_FIELD", field)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        let started = std::time::Instant::now();
        for field in ["storage", "hevc", "autostart"] {
            while !root.path().join(format!("ready-{field}")).exists() {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(10),
                    "child never read stale configuration"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        fs::write(root.path().join("go"), "go").unwrap();
        for child in &mut children.0 {
            assert!(child.wait().unwrap().success());
        }
        let path = root.path().join("config.toml");
        let merged = Config::load_or_create(&path).unwrap();
        assert_eq!(merged.storage.output_path, "fixture-output");
        assert_eq!(merged.storage.auto_delete_days, 14);
        assert!(merged.app.hevc_playback_supported);
        assert!(!merged.app.autostart);
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("enabled = true") && text.contains("future_option = 42"));
        let recorder = league_replay_recorder::config::Config::load_or_create(
            &root.path().join("config.toml"),
        )
        .unwrap();
        assert_eq!(recorder.storage.auto_delete_days, 14);
        assert!(recorder.app.hevc_playback_supported);
        assert!(!recorder.app.autostart);
    }

    #[test]
    fn app_data_override_is_explicit_and_never_falls_back_on_error() {
        let directory = tempfile::tempdir().unwrap();
        let default = directory.path().join("default");
        let isolated = directory.path().join("isolated");
        assert_eq!(
            app_data_path_with_override(default.clone(), Some(isolated.clone().into_os_string()))
                .unwrap(),
            isolated
        );
        assert!(!default.exists());
        for value in ["", "relative"] {
            assert!(app_data_path_with_override(default.clone(), Some(value.into())).is_err());
        }
        let file = directory.path().join("file");
        fs::write(&file, b"not a directory").unwrap();
        assert!(app_data_path_with_override(default.clone(), Some(file.into_os_string())).is_err());
        assert!(!default.exists());
        assert_eq!(
            app_data_path_with_override(default.clone(), None).unwrap(),
            default
        );
    }

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
        mutate(
            &path,
            ConfigPatch::Storage(SettingsUpdate {
                output_path: config.storage.output_path,
                auto_delete_days: config.storage.auto_delete_days,
            }),
        )
        .unwrap();

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
    fn inline_configuration_tables_keep_unowned_fields_when_patched() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "app = { autostart = false, future_option = 42 }\n").unwrap();
        let merged = mutate(&path, ConfigPatch::Hevc(true)).unwrap();
        assert!(!merged.app.autostart && merged.app.hevc_playback_supported);
        assert!(
            fs::read_to_string(path)
                .unwrap()
                .contains("future_option = 42")
        );
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
