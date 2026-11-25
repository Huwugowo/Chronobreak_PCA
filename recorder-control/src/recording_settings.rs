use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RecordingConfig {
    pub profile: RecordingProfile,
    pub codec: CodecPreference,
}

impl RecordingConfig {
    pub fn validate_windows(&self) -> Result<()> {
        if self.codec == CodecPreference::Hevc {
            bail!("the native recorder currently supports H.264 only");
        }
        if !matches!(
            self.profile,
            RecordingProfile::Auto | RecordingProfile::High
        ) {
            bail!("the native recorder currently requires the auto or high 1080p60 profile");
        }
        Ok(())
    }
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

impl CodecPreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::H264 => "h264",
            Self::Hevc => "hevc",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct EffectiveSettings {
    pub output_path: String,
    pub preferences: RecordingConfig,
    pub profile: RecordingProfile,
    pub codec: CodecPreference,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyState {
    #[default]
    Idle,
    Waiting,
    InvalidConfiguration,
    DestinationUnavailable,
    LibraryRecoveryUnavailable,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RecorderSettings {
    pub effective: Option<EffectiveSettings>,
    pub apply_state: ApplyState,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsAction {
    Query,
    Apply,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsRequest {
    pub protocol_version: u32,
    pub instance: String,
    pub settings: SettingsAction,
}

#[derive(Clone)]
pub struct SettingsControl {
    requests: watch::Sender<u64>,
    state: watch::Sender<(u64, RecorderSettings)>,
}

impl Default for SettingsControl {
    fn default() -> Self {
        Self {
            requests: watch::Sender::new(0),
            state: watch::Sender::new((0, RecorderSettings::default())),
        }
    }
}

impl SettingsControl {
    pub fn request_apply(&self) {
        self.requests
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub fn requested(&self) -> u64 {
        *self.requests.borrow()
    }

    pub fn snapshot(&self) -> RecorderSettings {
        let (handled, mut state) = self.state.borrow().clone();
        if handled != self.requested() {
            state.apply_state = ApplyState::Waiting;
        }
        state
    }

    pub fn publish(&self, handled: u64, state: RecorderSettings) {
        self.state.send_replace((handled, state));
    }
}
