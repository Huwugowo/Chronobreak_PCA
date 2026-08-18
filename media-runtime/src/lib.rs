use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

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
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command.spawn().map_err(|error| {
        RuntimeError::new(
            RuntimeErrorKind::ExecutionBlocked,
            format!("{} could not start: {error}", path.display()),
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorKind::ExecutionBlocked,
            format!("{} stdout was not captured", path.display()),
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        RuntimeError::new(
            RuntimeErrorKind::ExecutionBlocked,
            format!("{} stderr was not captured", path.display()),
        )
    })?;
    let mut stdout_task = tokio::spawn(drain_bounded(stdout, max_output_bytes));
    let mut stderr_task = tokio::spawn(drain_bounded(stderr, max_output_bytes));

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map_err(|error| {
            RuntimeError::new(
                RuntimeErrorKind::ExecutionBlocked,
                format!("{} could not be waited: {error}", path.display()),
            )
        })?,
        Err(_) => {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(PROBE_REAP_TIMEOUT, child.wait()).await;
            stdout_task.abort();
            stderr_task.abort();
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(RuntimeError::new(
                RuntimeErrorKind::ProbeTimeout,
                format!(
                    "{} exceeded {} seconds",
                    path.display(),
                    timeout.as_secs_f64()
                ),
            ));
        }
    };
    if !status.success() {
        stdout_task.abort();
        stderr_task.abort();
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        return Err(RuntimeError::new(
            RuntimeErrorKind::IncompatibleBuild,
            format!("{} probe exited with {status}", path.display()),
        ));
    }
    let drains = match tokio::time::timeout(PROBE_REAP_TIMEOUT, async {
        let stdout = (&mut stdout_task)
            .await
            .map_err(|error| error.to_string())?;
        let stderr = (&mut stderr_task)
            .await
            .map_err(|error| error.to_string())?;
        let stdout = stdout.map_err(|error| error.to_string())?;
        let stderr = stderr.map_err(|error| error.to_string())?;
        Ok::<_, String>((stdout, stderr))
    })
    .await
    {
        Ok(result) => result.map_err(|error| {
            RuntimeError::new(
                RuntimeErrorKind::ExecutionBlocked,
                format!("{} output could not be read: {error}", path.display()),
            )
        })?,
        Err(_) => {
            if !stdout_task.is_finished() {
                stdout_task.abort();
                let _ = stdout_task.await;
            }
            if !stderr_task.is_finished() {
                stderr_task.abort();
                let _ = stderr_task.await;
            }
            return Err(RuntimeError::new(
                RuntimeErrorKind::ExecutionBlocked,
                format!("{} output pipes did not close", path.display()),
            ));
        }
    };
    if drains.0.overflow || drains.1.overflow {
        return Err(RuntimeError::new(
            RuntimeErrorKind::IncompatibleBuild,
            format!("{} probe output exceeded its bound", path.display()),
        ));
    }
    let mut output_text = String::from_utf8_lossy(&drains.0.bytes).into_owned();
    output_text.push_str(&String::from_utf8_lossy(&drains.1.bytes));
    Ok(output_text)
}

struct BoundedOutput {
    bytes: Vec<u8>,
    overflow: bool,
}

async fn drain_bounded<R>(mut reader: R, max_output_bytes: usize) -> std::io::Result<BoundedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut overflow = false;
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = max_output_bytes.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&chunk[..retained]);
        overflow |= retained < read;
    }
    Ok(BoundedOutput { bytes, overflow })
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
    async fn output_drain_retains_a_fixed_bound_while_consuming_all_input() {
        let input = vec![b'x'; 64];
        let output = drain_bounded(input.as_slice(), 7).await.unwrap();
        assert_eq!(output.bytes, vec![b'x'; 7]);
        assert!(output.overflow);
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
}
