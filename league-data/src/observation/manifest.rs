//! Small telemetry discovery/timing hint. Media authority remains metadata.json;
//! consumers validate the referenced journal prefix before using observations.
use super::*;

pub const FILE: &str = "game_log.json";
pub const MAX_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub telemetry_version: u32,
    pub media_id: MediaId,
    pub run_id: MediaId,
    pub source_epoch: u64,
    pub committed: journal::Position,
    pub calibration: Option<GameCalibrationV2>,
}

impl Manifest {
    pub fn from_checkpoint(state: &checkpoint::Checkpoint) -> Self {
        Self {
            schema_version: 2,
            telemetry_version: journal::FORMAT_VERSION,
            media_id: state.header.media_id.clone(),
            run_id: state.header.run_id.clone(),
            source_epoch: state.source.epoch,
            committed: state.position.clone(),
            calibration: state.calibration.as_ref().map(|(_, value)| value.clone()),
        }
    }

    pub fn decode(bytes: &[u8], media_id: &MediaId) -> Result<Self, journal::Error> {
        if bytes.len() > MAX_BYTES {
            return Err(journal::Error::Capacity);
        }
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|_| journal::Error::Invalid("telemetry manifest"))?;
        if value.schema_version != 2
            || value.telemetry_version != journal::FORMAT_VERSION
            || &value.media_id != media_id
        {
            return Err(journal::Error::Invalid(
                "telemetry format or media identity",
            ));
        }
        Ok(value)
    }

    pub fn encode(&self) -> Result<Vec<u8>, journal::Error> {
        journal::encode(self, MAX_BYTES)
    }
}
