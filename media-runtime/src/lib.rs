use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::task::JoinSet;

pub const CONTRACT_VERSION: u32 = 3;
pub const RUNTIME_DIRECTORY_NAME: &str = "media-runtime";
pub const DEVELOPMENT_OVERRIDE_ENV: &str = "QUEUEBACK_MEDIA_RUNTIME_DIR";
pub const EMBEDDED_LOCK_JSON: &str = include_str!("../runtime-lock.json");

const MANIFEST_FILENAME: &str = "runtime-manifest.json";
const FFMPEG_PATH: &str = "bin/ffmpeg.exe";
const FFPROBE_PATH: &str = "bin/ffprobe.exe";
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_REAP_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_PROBE_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLock {
    pub contract_version: u32,
    pub runtime_id: String,
    pub platform: RuntimePlatform,
    pub ffmpeg: FfmpegIdentity,
    pub sources: Vec<SourceIdentity>,
    pub patches: Vec<PatchIdentity>,
    pub toolchain: Vec<ToolchainPackage>,
    pub capabilities: RuntimeCapabilities,
    pub files: Vec<LockedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePlatform {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FfmpegIdentity {
    pub version_banner: String,
    pub compiler: String,
    pub upstream_tag: String,
    pub source_commit: String,
    pub required_configure_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub name: String,
    pub url: String,
    pub tag: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PatchIdentity {
    pub path: String,
    pub applies_to: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolchainPackage {
    pub package: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCapabilities {
    pub filters: Vec<String>,
    pub hwaccels: Vec<String>,
    pub encoders: Vec<String>,
    pub diagnostic_abi: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LockedFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequirements {
    pub filters: BTreeSet<String>,
    pub hwaccels: BTreeSet<String>,
    pub encoders: BTreeSet<String>,
    pub diagnostic_abi: Option<String>,
}

impl RuntimeRequirements {
    pub fn distribution_baseline(lock: &RuntimeLock) -> Self {
        Self {
            filters: lock.capabilities.filters.iter().cloned().collect(),
            hwaccels: lock.capabilities.hwaccels.iter().cloned().collect(),
            encoders: lock.capabilities.encoders.iter().cloned().collect(),
            diagnostic_abi: Some(lock.capabilities.diagnostic_abi.clone()),
        }
    }

    pub fn with_diagnostic_abi(mut self, abi: impl Into<String>) -> Self {
        self.diagnostic_abi = Some(abi.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityReport {
    pub filters: BTreeSet<String>,
    pub hwaccels: BTreeSet<String>,
    pub encoders: BTreeSet<String>,
    pub diagnostic_abi: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTools {
    root: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    lock: RuntimeLock,
    capabilities: CapabilityReport,
}

impl MediaTools {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ffmpeg(&self) -> &Path {
        &self.ffmpeg
    }

    pub fn ffprobe(&self) -> &Path {
        &self.ffprobe
    }

    pub fn runtime_id(&self) -> &str {
        &self.lock.runtime_id
    }

    pub fn lock(&self) -> &RuntimeLock {
        &self.lock
    }

    pub fn capabilities(&self) -> &CapabilityReport {
        &self.capabilities
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Missing,
    Incomplete,
    InvalidManifest,
    UnsafePath,
    IntegrityMismatch,
    IncompatibleBuild,
    MissingCapability,
    ExecutionBlocked,
    ProbeTimeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    kind: RuntimeErrorKind,
    detail: String,
}

impl RuntimeError {
    fn new(kind: RuntimeErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn kind(&self) -> RuntimeErrorKind {
        self.kind
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let action = match self.kind {
            RuntimeErrorKind::ExecutionBlocked => {
                "Repair QueueBack or allow its media tools through antivirus/security software"
            }
            _ => "Repair or reinstall QueueBack's complete media runtime",
        };
        write!(
            formatter,
            "QueueBack media runtime {}: {}. {action}.",
            error_label(self.kind),
            self.detail
        )
    }
}

impl std::error::Error for RuntimeError {}

/// Failure category for a bounded media-tool child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundedProcessErrorKind {
    /// The child could not be created or configured.
    Spawn,
    /// Waiting for the child or reading one of its pipes failed.
    Execution,
    /// The child exceeded its absolute runtime deadline.
    Timeout,
    /// Combined stdout and stderr exceeded the caller's byte limit.
    OutputLimit,
}

/// Failure produced by [`run_bounded_process`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedProcessError {
    kind: BoundedProcessErrorKind,
    detail: String,
}

impl BoundedProcessError {
    fn new(kind: BoundedProcessErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub fn kind(&self) -> BoundedProcessErrorKind {
        self.kind
    }

    /// Returns the bounded operation detail.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for BoundedProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bounded media process failed: {}", self.detail)
    }
}

impl std::error::Error for BoundedProcessError {}

/// Raw output from a successfully supervised bounded child process.
#[derive(Debug)]
pub struct BoundedProcessOutput {
    /// Exit status reported after the child was reaped.
    pub status: std::process::ExitStatus,
    /// Captured stdout, bounded together with stderr.
    pub stdout: Vec<u8>,
    /// Captured stderr, bounded together with stdout.
    pub stderr: Vec<u8>,
}

fn error_label(kind: RuntimeErrorKind) -> &'static str {
    match kind {
        RuntimeErrorKind::Missing => "is missing",
        RuntimeErrorKind::Incomplete => "is incomplete",
        RuntimeErrorKind::InvalidManifest => "manifest is invalid",
        RuntimeErrorKind::UnsafePath => "contains an unsafe path",
        RuntimeErrorKind::IntegrityMismatch => "failed its integrity check",
        RuntimeErrorKind::IncompatibleBuild => "is incompatible",
        RuntimeErrorKind::MissingCapability => "is missing a required capability",
        RuntimeErrorKind::ExecutionBlocked => "could not be executed",
        RuntimeErrorKind::ProbeTimeout => "probe timed out",
    }
}

pub fn embedded_lock() -> Result<RuntimeLock, RuntimeError> {
    parse_lock(EMBEDDED_LOCK_JSON, "embedded lock")
}

pub fn production_root_for_executable(executable: &Path) -> Result<PathBuf, RuntimeError> {
    let directory = executable.parent().ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorKind::UnsafePath,
            format!(
                "executable {} has no parent directory",
                executable.display()
            ),
        )
    })?;
    Ok(directory.join("resources").join(RUNTIME_DIRECTORY_NAME))
}

pub fn development_override() -> Option<PathBuf> {
    env::var_os(DEVELOPMENT_OVERRIDE_ENV).map(PathBuf::from)
}

pub async fn resolve(production_root: &Path) -> Result<MediaTools, RuntimeError> {
    let root = development_override().unwrap_or_else(|| production_root.to_path_buf());
    resolve_root(&root).await
}

pub async fn resolve_root(root: &Path) -> Result<MediaTools, RuntimeError> {
    let lock = embedded_lock()?;
    let requirements = RuntimeRequirements::distribution_baseline(&lock);
    resolve_root_with(root, &lock, &requirements).await
}

pub async fn resolve_root_with(
    root: &Path,
    expected: &RuntimeLock,
    requirements: &RuntimeRequirements,
) -> Result<MediaTools, RuntimeError> {
    validate_lock(expected, "embedded lock")?;
    let metadata = tokio::fs::metadata(root).await.map_err(|error| {
        RuntimeError::new(
            if error.kind() == std::io::ErrorKind::NotFound {
                RuntimeErrorKind::Missing
            } else {
                RuntimeErrorKind::ExecutionBlocked
            },
            format!("cannot access {}: {error}", root.display()),
        )
    })?;
    if !metadata.is_dir() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::Incomplete,
            format!("{} is not a directory", root.display()),
        ));
    }
    let root = tokio::fs::canonicalize(root).await.map_err(|error| {
        RuntimeError::new(
            RuntimeErrorKind::ExecutionBlocked,
            format!("cannot canonicalize {}: {error}", root.display()),
        )
    })?;

    let manifest_path = root.join(MANIFEST_FILENAME);
    let manifest_bytes = tokio::fs::read(&manifest_path).await.map_err(|error| {
        RuntimeError::new(
            if error.kind() == std::io::ErrorKind::NotFound {
                RuntimeErrorKind::Incomplete
            } else {
                RuntimeErrorKind::ExecutionBlocked
            },
            format!("cannot read {}: {error}", manifest_path.display()),
        )
    })?;
    let manifest_text = std::str::from_utf8(&manifest_bytes).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{} is not UTF-8: {error}", manifest_path.display()),
        )
    })?;
    let manifest = parse_lock(manifest_text, "runtime manifest")?;
    if &manifest != expected {
        return Err(RuntimeError::new(
            RuntimeErrorKind::IntegrityMismatch,
            "runtime-manifest.json does not match the QueueBack-embedded lock",
        ));
    }

    let mut resolved_files = Vec::with_capacity(expected.files.len());
    for file in &expected.files {
        let relative = safe_relative_path(&file.path)?;
        let path = root.join(&relative);
        let metadata = tokio::fs::symlink_metadata(&path).await.map_err(|error| {
            RuntimeError::new(
                if error.kind() == std::io::ErrorKind::NotFound {
                    RuntimeErrorKind::Incomplete
                } else {
                    RuntimeErrorKind::ExecutionBlocked
                },
                format!("cannot inspect {}: {error}", path.display()),
            )
        })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsafePath,
                format!("{} is not a regular non-symlink file", path.display()),
            ));
        }
        let canonical = tokio::fs::canonicalize(&path).await.map_err(|error| {
            RuntimeError::new(
                RuntimeErrorKind::ExecutionBlocked,
                format!("cannot canonicalize {}: {error}", path.display()),
            )
        })?;
        if !canonical.starts_with(&root) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnsafePath,
                format!("{} escapes {}", canonical.display(), root.display()),
            ));
        }
        if metadata.len() != file.size {
            return Err(RuntimeError::new(
                RuntimeErrorKind::IntegrityMismatch,
                format!(
                    "{} size is {}, expected {}",
                    file.path,
                    metadata.len(),
                    file.size
                ),
            ));
        }
        let actual_hash = hash_file(&canonical).await?;
        if actual_hash != file.sha256 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::IntegrityMismatch,
                format!("{} SHA-256 does not match the embedded lock", file.path),
            ));
        }
        resolved_files.push((file.path.as_str(), canonical));
    }

    let ffmpeg = locked_path(&resolved_files, FFMPEG_PATH)?;
    let ffprobe = locked_path(&resolved_files, FFPROBE_PATH)?;
    let version = run_probe(&ffmpeg, &["-hide_banner", "-version"]).await?;
    let probe_version = run_probe(&ffprobe, &["-hide_banner", "-version"]).await?;
    validate_tool_identity(expected, &version, &probe_version)?;

    let filters_output = run_probe(&ffmpeg, &["-hide_banner", "-filters"]).await?;
    let hwaccels_output = run_probe(&ffmpeg, &["-hide_banner", "-hwaccels"]).await?;
    let encoders_output = run_probe(&ffmpeg, &["-hide_banner", "-encoders"]).await?;
    let report = CapabilityReport {
        filters: listed_names(&filters_output, &requirements.filters),
        hwaccels: listed_names(&hwaccels_output, &requirements.hwaccels),
        encoders: listed_names(&encoders_output, &requirements.encoders),
        diagnostic_abi: requirements
            .diagnostic_abi
            .as_ref()
            .filter(|abi| version.contains(abi.as_str()))
            .cloned(),
    };
    validate_capabilities(requirements, &report)?;

    Ok(MediaTools {
        root,
        ffmpeg,
        ffprobe,
        lock: expected.clone(),
        capabilities: report,
    })
}

fn parse_lock(contents: &str, label: &str) -> Result<RuntimeLock, RuntimeError> {
    let lock = serde_json::from_str::<RuntimeLock>(contents).map_err(|error| {
        RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} could not be parsed: {error}"),
        )
    })?;
    validate_lock(&lock, label)?;
    Ok(lock)
}

fn validate_lock(lock: &RuntimeLock, label: &str) -> Result<(), RuntimeError> {
    if lock.contract_version != CONTRACT_VERSION {
        return Err(RuntimeError::new(
            RuntimeErrorKind::IncompatibleBuild,
            format!(
                "{label} contract version {} is not supported",
                lock.contract_version
            ),
        ));
    }
    if lock.runtime_id.trim().is_empty()
        || lock.platform.os != "windows"
        || lock.platform.arch != "x86_64"
        || lock.ffmpeg.version_banner.trim().is_empty()
        || lock.capabilities.diagnostic_abi.trim().is_empty()
        || !lock
            .ffmpeg
            .version_banner
            .contains(&lock.capabilities.diagnostic_abi)
    {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} has an invalid identity"),
        ));
    }
    let mut source_names = BTreeSet::new();
    if lock.sources.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} does not identify its pinned sources"),
        ));
    }
    for source in &lock.sources {
        if source.name.trim().is_empty()
            || !source_names.insert(source.name.as_str())
            || !source.url.starts_with("https://github.com/")
            || !source.url.ends_with(".git")
            || source.url.contains("latest")
            || source.tag.trim().is_empty()
            || source.commit.len() != 40
            || !source.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidManifest,
                format!("{label} has invalid source provenance for {}", source.name),
            ));
        }
    }
    let mut patch_paths = BTreeSet::new();
    if lock.patches.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} does not identify its source patches"),
        ));
    }
    for patch in &lock.patches {
        safe_relative_path(&patch.path)?;
        validate_sha256(&patch.sha256, &format!("{} SHA-256", patch.path))?;
        if !patch_paths.insert(patch.path.as_str())
            || !source_names.contains(patch.applies_to.as_str())
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidManifest,
                format!("{label} has invalid patch provenance for {}", patch.path),
            ));
        }
    }
    let mut toolchain_packages = BTreeSet::new();
    if lock.toolchain.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} does not identify its pinned build toolchain"),
        ));
    }
    for dependency in &lock.toolchain {
        if dependency.package.trim().is_empty()
            || dependency.package != dependency.package.trim()
            || dependency.version.trim().is_empty()
            || dependency.version != dependency.version.trim()
            || dependency.version.eq_ignore_ascii_case("latest")
            || !toolchain_packages.insert(dependency.package.as_str())
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidManifest,
                format!(
                    "{label} has invalid toolchain provenance for {}",
                    dependency.package
                ),
            ));
        }
    }
    let mut paths = BTreeSet::new();
    for file in &lock.files {
        safe_relative_path(&file.path)?;
        validate_sha256(&file.sha256, &format!("{} SHA-256", file.path))?;
        if file.size == 0 || !paths.insert(file.path.as_str()) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidManifest,
                format!("{label} has an empty or duplicate file {}", file.path),
            ));
        }
    }
    for required in [FFMPEG_PATH, FFPROBE_PATH] {
        if !paths.contains(required) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::Incomplete,
                format!("{label} does not lock {required}"),
            ));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), RuntimeError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidManifest,
            format!("{label} is not a SHA-256 digest"),
        ));
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, RuntimeError> {
    if value.is_empty() || value.contains('\\') {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsafePath,
            format!("{value:?} is not a normalized relative path"),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeError::new(
            RuntimeErrorKind::UnsafePath,
            format!("{value:?} is not a safe relative path"),
        ));
    }
    Ok(path.to_path_buf())
}

async fn hash_file(path: &Path) -> Result<String, RuntimeError> {
    let mut file = tokio::fs::File::open(path).await.map_err(|error| {
        RuntimeError::new(
            RuntimeErrorKind::ExecutionBlocked,
            format!("cannot open {} for hashing: {error}", path.display()),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(|error| {
            RuntimeError::new(
                RuntimeErrorKind::ExecutionBlocked,
                format!("cannot hash {}: {error}", path.display()),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn locked_path(files: &[(&str, PathBuf)], wanted: &str) -> Result<PathBuf, RuntimeError> {
    files
        .iter()
        .find_map(|(name, path)| (*name == wanted).then(|| path.clone()))
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::Incomplete,
                format!("runtime does not contain {wanted}"),
            )
        })
}

async fn run_probe(path: &Path, arguments: &[&str]) -> Result<String, RuntimeError> {
    run_probe_with_limits(path, arguments, PROBE_TIMEOUT, MAX_PROBE_OUTPUT_BYTES).await
}

async fn run_probe_with_limits(
    path: &Path,
    arguments: &[&str],
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<String, RuntimeError> {
    let mut command = Command::new(path);
    command.args(arguments);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = run_bounded_process(command, timeout, max_output_bytes)
        .await
        .map_err(|error| {
            let kind = match error.kind() {
                BoundedProcessErrorKind::Timeout => RuntimeErrorKind::ProbeTimeout,
                BoundedProcessErrorKind::OutputLimit => RuntimeErrorKind::IncompatibleBuild,
                BoundedProcessErrorKind::Spawn | BoundedProcessErrorKind::Execution => {
                    RuntimeErrorKind::ExecutionBlocked
                }
            };
            RuntimeError::new(kind, format!("{}: {}", path.display(), error.detail()))
        })?;
    if !output.status.success() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::IncompatibleBuild,
            format!("{} probe exited with {}", path.display(), output.status),
        ));
    }
    let mut output_text = String::from_utf8_lossy(&output.stdout).into_owned();
    output_text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(output_text)
}

/// Runs a media-tool child with an absolute deadline and one combined output cap.
///
/// The child is killed and reaped if it times out, either output pipe fails, or
/// stdout plus stderr exceeds `max_combined_output_bytes`. The byte limit is
/// exact: output equal to the limit succeeds and the first additional byte
/// fails the operation.
pub async fn run_bounded_process(
    mut command: Command,
    timeout: Duration,
    max_combined_output_bytes: usize,
) -> Result<BoundedProcessOutput, BoundedProcessError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(|error| {
        BoundedProcessError::new(
            BoundedProcessErrorKind::Spawn,
            format!("child could not start: {error}"),
        )
    })?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            stop_bounded_process(&mut child, None).await;
            return Err(BoundedProcessError::new(
                BoundedProcessErrorKind::Spawn,
                "child stdout was not captured",
            ));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            stop_bounded_process(&mut child, None).await;
            return Err(BoundedProcessError::new(
                BoundedProcessErrorKind::Spawn,
                "child stderr was not captured",
            ));
        }
    };

    let total_bytes = Arc::new(AtomicUsize::new(0));
    let mut drains = JoinSet::new();
    let stdout_total = Arc::clone(&total_bytes);
    drains.spawn(async move {
        (
            CapturedPipe::Stdout,
            drain_bounded_combined(stdout, stdout_total, max_combined_output_bytes).await,
        )
    });
    let stderr_total = Arc::clone(&total_bytes);
    drains.spawn(async move {
        (
            CapturedPipe::Stderr,
            drain_bounded_combined(stderr, stderr_total, max_combined_output_bytes).await,
        )
    });

    let deadline = tokio::time::Instant::now() + timeout;
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    while status.is_none() || stdout.is_none() || stderr.is_none() {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                stop_bounded_process(&mut child, Some(&mut drains)).await;
                return Err(BoundedProcessError::new(
                    BoundedProcessErrorKind::Timeout,
                    format!("child exceeded {} seconds", timeout.as_secs_f64()),
                ));
            }
            wait_result = child.wait(), if status.is_none() => {
                match wait_result {
                    Ok(exit_status) => status = Some(exit_status),
                    Err(error) => {
                        stop_bounded_process(&mut child, Some(&mut drains)).await;
                        return Err(BoundedProcessError::new(
                            BoundedProcessErrorKind::Execution,
                            format!("child could not be waited: {error}"),
                        ));
                    }
                }
            }
            drain_result = drains.join_next(), if stdout.is_none() || stderr.is_none() => {
                match drain_result {
                    Some(Ok((pipe, Ok(bytes)))) => match pipe {
                        CapturedPipe::Stdout if stdout.is_none() => stdout = Some(bytes),
                        CapturedPipe::Stderr if stderr.is_none() => stderr = Some(bytes),
                        _ => {
                            stop_bounded_process(&mut child, Some(&mut drains)).await;
                            return Err(BoundedProcessError::new(
                                BoundedProcessErrorKind::Execution,
                                "child output pipe completed more than once",
                            ));
                        }
                    },
                    Some(Ok((_, Err(DrainError::OutputLimit)))) => {
                        stop_bounded_process(&mut child, Some(&mut drains)).await;
                        return Err(BoundedProcessError::new(
                            BoundedProcessErrorKind::OutputLimit,
                            format!(
                                "combined stdout and stderr exceeded {max_combined_output_bytes} bytes"
                            ),
                        ));
                    }
                    Some(Ok((pipe, Err(DrainError::Read(error))))) => {
                        stop_bounded_process(&mut child, Some(&mut drains)).await;
                        return Err(BoundedProcessError::new(
                            BoundedProcessErrorKind::Execution,
                            format!("child {} could not be read: {error}", pipe.label()),
                        ));
                    }
                    Some(Err(error)) => {
                        stop_bounded_process(&mut child, Some(&mut drains)).await;
                        return Err(BoundedProcessError::new(
                            BoundedProcessErrorKind::Execution,
                            format!("child output task failed: {error}"),
                        ));
                    }
                    None => {
                        stop_bounded_process(&mut child, Some(&mut drains)).await;
                        return Err(BoundedProcessError::new(
                            BoundedProcessErrorKind::Execution,
                            "child output tasks ended without both pipes",
                        ));
                    }
                }
            }
        }
    }

    Ok(BoundedProcessOutput {
        status: status.expect("loop requires a child status"),
        stdout: stdout.expect("loop requires stdout"),
        stderr: stderr.expect("loop requires stderr"),
    })
}

#[derive(Debug, Clone, Copy)]
enum CapturedPipe {
    Stdout,
    Stderr,
}

impl CapturedPipe {
    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
enum DrainError {
    OutputLimit,
    Read(std::io::Error),
}

type DrainResult = (CapturedPipe, Result<Vec<u8>, DrainError>);

async fn stop_bounded_process(
    child: &mut tokio::process::Child,
    drains: Option<&mut JoinSet<DrainResult>>,
) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(PROBE_REAP_TIMEOUT, child.wait()).await;
    if let Some(drains) = drains {
        drains.abort_all();
        while drains.join_next().await.is_some() {}
    }
}

async fn drain_bounded_combined<R>(
    mut reader: R,
    total_bytes: Arc<AtomicUsize>,
    max_combined_output_bytes: usize,
) -> Result<Vec<u8>, DrainError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk).await.map_err(DrainError::Read)?;
        if read == 0 {
            return Ok(bytes);
        }
        let retained =
            reserve_combined_output(total_bytes.as_ref(), max_combined_output_bytes, read);
        bytes.extend_from_slice(&chunk[..retained]);
        if retained < read {
            return Err(DrainError::OutputLimit);
        }
    }
}

fn reserve_combined_output(
    total_bytes: &AtomicUsize,
    max_combined_output_bytes: usize,
    requested: usize,
) -> usize {
    let mut current = total_bytes.load(Ordering::Acquire);
    loop {
        let retained = max_combined_output_bytes
            .saturating_sub(current)
            .min(requested);
        match total_bytes.compare_exchange_weak(
            current,
            current + retained,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return retained,
            Err(actual) => current = actual,
        }
    }
}

fn validate_tool_identity(
    lock: &RuntimeLock,
    ffmpeg: &str,
    ffprobe: &str,
) -> Result<(), RuntimeError> {
    let ffmpeg_banner = format!("ffmpeg version {}", lock.ffmpeg.version_banner);
    let ffprobe_banner = format!("ffprobe version {}", lock.ffmpeg.version_banner);
    for (label, output, banner) in [
        ("ffmpeg", ffmpeg, ffmpeg_banner.as_str()),
        ("ffprobe", ffprobe, ffprobe_banner.as_str()),
    ] {
        if !output.contains(banner) || !output.contains(&lock.ffmpeg.compiler) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::IncompatibleBuild,
                format!("{label} version/compiler identity does not match the lock"),
            ));
        }
        for flag in &lock.ffmpeg.required_configure_flags {
            if !output.contains(flag) {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::IncompatibleBuild,
                    format!("{label} is missing configure flag {flag}"),
                ));
            }
        }
    }
    Ok(())
}

fn listed_names(output: &str, required: &BTreeSet<String>) -> BTreeSet<String> {
    required
        .iter()
        .filter(|name| {
            output.lines().any(|line| {
                line.split_ascii_whitespace()
                    .any(|token| token == name.as_str())
            })
        })
        .cloned()
        .collect()
}

fn validate_capabilities(
    required: &RuntimeRequirements,
    actual: &CapabilityReport,
) -> Result<(), RuntimeError> {
    let mut missing = Vec::new();
    for (kind, required, actual) in [
        ("filter", &required.filters, &actual.filters),
        (
            "hardware acceleration",
            &required.hwaccels,
            &actual.hwaccels,
        ),
        ("encoder", &required.encoders, &actual.encoders),
    ] {
        for name in required.difference(actual) {
            missing.push(format!("{kind} {name}"));
        }
    }
    if let Some(abi) = &required.diagnostic_abi
        && actual.diagnostic_abi.as_ref() != Some(abi)
    {
        missing.push(format!("diagnostics ABI {abi}"));
    }
    if !missing.is_empty() {
        return Err(RuntimeError::new(
            RuntimeErrorKind::MissingCapability,
            missing.join(", "),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn write_script(directory: &Path, name: &str, body: &str) -> PathBuf {
        let path = directory.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn embedded_lock_is_exact_and_immutable() {
        let lock = embedded_lock().unwrap();
        assert_eq!(lock.contract_version, 3);
        assert_eq!(lock.runtime_id, "queueback-ffmpeg-8.1.2-windows-x86_64-r6");
        assert_eq!(lock.sources.len(), 3);
        assert!(lock.sources.iter().all(|source| source.commit.len() == 40));
        assert_eq!(lock.patches.len(), 1);
        assert_eq!(lock.patches[0].applies_to, "ffmpeg");
        assert_eq!(lock.toolchain.len(), 9);
        assert!(lock.files.iter().any(|file| file.path == FFMPEG_PATH));
        assert!(lock.files.iter().any(|file| file.path == FFPROBE_PATH));
        assert!(!lock.files.iter().any(|file| file.path.contains("ffplay")));
    }

    #[test]
    fn rejects_unsafe_and_duplicate_manifest_paths() {
        for path in ["", "../ffmpeg.exe", "/ffmpeg.exe", "bin\\ffmpeg.exe"] {
            let mut lock = embedded_lock().unwrap();
            lock.files[0].path = path.to_owned();
            assert_eq!(
                validate_lock(&lock, "test").unwrap_err().kind(),
                RuntimeErrorKind::UnsafePath
            );
        }
        let mut lock = embedded_lock().unwrap();
        lock.files[1].path = lock.files[0].path.clone();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );
    }

    #[test]
    fn rejects_floating_or_unpinned_source() {
        let mut lock = embedded_lock().unwrap();
        lock.sources[0].url = "https://github.com/example/latest.git".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );
        let mut lock = embedded_lock().unwrap();
        lock.sources[0].commit = "nope".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );
    }

    #[test]
    fn rejects_unsafe_unpinned_or_unknown_patch_provenance() {
        let mut lock = embedded_lock().unwrap();
        lock.patches[0].path = "../escape.patch".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::UnsafePath
        );

        let mut lock = embedded_lock().unwrap();
        lock.patches[0].sha256 = "latest".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );

        let mut lock = embedded_lock().unwrap();
        lock.patches[0].applies_to = "unknown".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );
    }

    #[test]
    fn rejects_unpinned_or_duplicate_toolchain_packages() {
        let mut lock = embedded_lock().unwrap();
        lock.toolchain[0].version = "latest".to_owned();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );

        let mut lock = embedded_lock().unwrap();
        lock.toolchain[1].package = lock.toolchain[0].package.clone();
        assert_eq!(
            validate_lock(&lock, "test").unwrap_err().kind(),
            RuntimeErrorKind::InvalidManifest
        );
    }

    #[test]
    fn capability_parser_matches_whole_tokens_and_future_abi() {
        let lock = embedded_lock().unwrap();
        let required = RuntimeRequirements::distribution_baseline(&lock)
            .with_diagnostic_abi("queueback_capture_abi=1");
        let report = CapabilityReport {
            filters: listed_names(
                " .. gfxcapture |->V\n .. scale_d3d11 V->V",
                &required.filters,
            ),
            hwaccels: listed_names("d3d11va", &required.hwaccels),
            encoders: listed_names(
                "libx264 h264_nvenc hevc_nvenc h264_amf hevc_amf h264_qsv hevc_qsv",
                &required.encoders,
            ),
            diagnostic_abi: Some("queueback_capture_abi=1".to_owned()),
        };
        validate_capabilities(&required, &report).unwrap();

        let mut missing = report;
        missing.encoders.remove("h264_amf");
        assert_eq!(
            validate_capabilities(&required, &missing)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::MissingCapability
        );
    }

    #[tokio::test]
    async fn missing_runtime_is_actionable() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");
        let error = resolve_root(&missing).await.unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::Missing);
        assert!(error.to_string().contains("Repair or reinstall"));
    }

    #[tokio::test]
    async fn manifest_must_equal_the_embedded_lock() {
        let directory = tempfile::tempdir().unwrap();
        let mut lock = embedded_lock().unwrap();
        lock.runtime_id.push_str("-tampered");
        std::fs::write(
            directory.path().join(MANIFEST_FILENAME),
            serde_json::to_vec_pretty(&lock).unwrap(),
        )
        .unwrap();
        let error = resolve_root(directory.path()).await.unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::IntegrityMismatch);
    }

    #[tokio::test]
    async fn combined_output_limit_is_exact_across_pipes() {
        let total = Arc::new(AtomicUsize::new(0));
        let stdout = drain_bounded_combined(b"123".as_slice(), Arc::clone(&total), 7)
            .await
            .unwrap();
        let stderr = drain_bounded_combined(b"4567".as_slice(), Arc::clone(&total), 7)
            .await
            .unwrap();
        assert_eq!(stdout, b"123");
        assert_eq!(stderr, b"4567");
        assert!(matches!(
            drain_bounded_combined(b"8".as_slice(), total, 7).await,
            Err(DrainError::OutputLimit)
        ));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn probes_are_time_and_output_bounded_and_execution_errors_are_typed() {
        let directory = tempfile::tempdir().unwrap();
        let flood = write_script(directory.path(), "flood.cmd", "@echo off\necho 123456789\n");
        let error = run_probe_with_limits(&flood, &[], Duration::from_secs(1), 3)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::IncompatibleBuild);

        let looped = write_script(
            directory.path(),
            "loop.cmd",
            "@echo off\n:loop\ngoto loop\n",
        );
        let error = run_probe_with_limits(&looped, &[], Duration::from_millis(50), 1024)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::ProbeTimeout);

        let error = run_probe_with_limits(
            &directory.path().join("missing.exe"),
            &[],
            Duration::from_secs(1),
            1024,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), RuntimeErrorKind::ExecutionBlocked);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn output_overflow_kills_and_reaps_the_child() {
        let directory = tempfile::tempdir().unwrap();
        let lock_path = directory.path().join("overflow.lock");
        let escaped_lock_path = lock_path.display().to_string().replace('\'', "''");
        let script = format!(
            "$stream=[System.IO.File]::Open('{escaped_lock_path}',\
             [System.IO.FileMode]::Create,[System.IO.FileAccess]::ReadWrite,\
             [System.IO.FileShare]::None);\
             [Console]::Out.Write('1234');\
             Start-Sleep -Seconds 30"
        );
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);

        let error = run_bounded_process(command, Duration::from_secs(5), 3)
            .await
            .unwrap_err();
        assert_eq!(error.kind(), BoundedProcessErrorKind::OutputLimit);
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)
            .expect("overflowing child must be reaped and release its file lock");
    }
}
