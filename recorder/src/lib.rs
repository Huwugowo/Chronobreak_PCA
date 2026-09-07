#![deny(clippy::undocumented_unsafe_blocks)]

pub mod config;
pub mod encoder;
pub(crate) mod finalizer;
#[cfg(feature = "replay-time-fixture")]
pub use finalizer::{
    FINALIZER_FIXTURE_PROBE_OUTPUT_LIMIT, FINALIZER_FIXTURE_PROBE_TIMEOUT,
    FinalizerFixtureValidation, validate_finalizer_fixture,
};
#[cfg(target_os = "windows")]
pub mod native;
pub mod platform;
pub mod poller;
pub mod service;
pub mod storage;
pub mod watcher;
