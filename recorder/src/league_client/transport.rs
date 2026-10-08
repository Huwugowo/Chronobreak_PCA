use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::{Client, redirect::Policy};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{Gameflow, LcuRound, Summoner};

const LOCKFILE_LIMIT: u64 = 4096;
const RESPONSE_LIMIT: usize = 256 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, PartialEq, Eq)]
struct Credentials {
    pid: u32,
    port: u16,
    secret: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}

fn parse_lockfile(bytes: &[u8]) -> Result<Credentials, &'static str> {
    if bytes.len() > LOCKFILE_LIMIT as usize {
        return Err("lockfile_size");
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "lockfile_format")?;
    let fields: Vec<_> = text.split(':').collect();
    if fields.len() != 5 || fields[0] != "LeagueClient" || fields[4] != "https" {
        return Err("lockfile_format");
    }
    let pid = fields[1].parse::<u32>().map_err(|_| "lockfile_format")?;
    let port = fields[2].parse::<u16>().map_err(|_| "lockfile_format")?;
    if pid == 0
        || port == 0
        || fields[3].is_empty()
        || fields[3].len() > 256
        || !fields[3].bytes().all(|b| b.is_ascii_graphic())
        || pid.to_string() != fields[1]
        || port.to_string() != fields[2]
    {
        return Err("lockfile_format");
    }
    Ok(Credentials {
        pid,
        port,
        secret: fields[3].to_owned(),
    })
}

pub(super) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, &'static str> {
    let file = std::fs::File::open(path).map_err(|_| "file_unavailable")?;
    if !file.metadata().map_err(|_| "file_unavailable")?.is_file() {
        return Err("file_unavailable");
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file_unavailable")?;
    if bytes.len() as u64 > limit {
        return Err("file_size");
    }
    Ok(bytes)
}

pub(super) struct Discovered {
    credentials: Credentials,
    lockfile: PathBuf,
    process_started: u64,
}

// Opaque in-memory equality token. Never serialized, logged or exposed to callers.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Epoch {
    credentials: Credentials,
    lockfile: PathBuf,
    process_started: u64,
}

impl Discovered {
    pub(super) fn epoch(&self) -> Epoch {
        Epoch {
            credentials: self.credentials.clone(),
            lockfile: self.lockfile.clone(),
            process_started: self.process_started,
        }
    }
}

#[cfg(test)]
pub(super) fn fixture_epoch(rotated: bool) -> Epoch {
    Epoch {
        credentials: Credentials {
            pid: 123,
            port: 12345,
            secret: if rotated {
                "synthetic-rotated"
            } else {
                "synthetic"
            }
            .into(),
        },
        lockfile: PathBuf::from("synthetic/lockfile"),
        process_started: 42,
    }
}

/// One owned blocking worker per discovery; callers always await it, even at stop.
pub(super) async fn discover() -> Result<Discovered, &'static str> {
    tokio::task::spawn_blocking(discover_blocking)
        .await
        .map_err(|_| "discovery_worker")?
}

#[cfg(not(target_os = "windows"))]
fn discover_blocking() -> Result<Discovered, &'static str> {
    Err("non_windows")
}

#[cfg(target_os = "windows")]
fn discover_blocking() -> Result<Discovered, &'static str> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    let pids: Vec<_> = system
        .processes()
        .iter()
        .filter(|(_, p)| crate::watcher::process_name_matches(p.name(), "LeagueClient.exe"))
        .map(|(pid, _)| *pid)
        .collect();
    if pids.is_empty() {
        return Err("client_absent");
    }
    // Only selected client executable paths; no command lines, environment or memory.
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing()
            .without_tasks()
            .with_exe(UpdateKind::Always),
    );
    let override_path = std::env::var_os("QUEUEBACK_LCU_LOCKFILE").map(PathBuf::from);
    if override_path.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err("override_path");
    }
    let mut found = None;
    for pid in pids {
        let process = system.process(pid).ok_or("client_changed")?;
        let exe = process
            .exe()
            .ok_or("client_path_denied")?
            .canonicalize()
            .map_err(|_| "client_path_denied")?;
        if !exe
            .file_name()
            .is_some_and(|name| crate::watcher::process_name_matches(name, "LeagueClient.exe"))
        {
            return Err("client_path_mismatch");
        }
        let lockfile = exe.parent().ok_or("client_path_denied")?.join("lockfile");
        if let Some(path) = &override_path
            && path.canonicalize().map_err(|_| "override_unavailable")?
                != lockfile
                    .canonicalize()
                    .map_err(|_| "lockfile_unavailable")?
        {
            return Err("override_path_mismatch");
        }
        let credentials = parse_lockfile(&read_bounded(&lockfile, LOCKFILE_LIMIT)?)?;
        if credentials.pid != pid.as_u32() {
            return Err("lockfile_pid_mismatch");
        }
        if found.is_some() {
            return Err("client_ambiguous");
        }
        found = Some(Discovered {
            credentials,
            lockfile,
            process_started: process.start_time(),
        });
    }
    found.ok_or("client_absent")
}

/// Recheck the same credentials/path/process after the round, with no new discovery.
pub(super) async fn unchanged(discovered: Discovered) -> Result<bool, &'static str> {
    tokio::task::spawn_blocking(move || {
        let credentials = parse_lockfile(&read_bounded(&discovered.lockfile, LOCKFILE_LIMIT)?)?;
        if credentials != discovered.credentials {
            return Ok(false);
        }
        #[cfg(target_os = "windows")]
        {
            use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
            let pid = Pid::from_u32(credentials.pid);
            let mut system = System::new();
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing()
                    .without_tasks()
                    .with_exe(UpdateKind::Always),
            );
            let Some(process) = system.process(pid) else {
                return Ok(false);
            };
            let Some(exe) = process.exe() else {
                return Ok(false);
            };
            let expected_exe = discovered
                .lockfile
                .parent()
                .ok_or("client_path_denied")?
                .join("LeagueClient.exe");
            Ok(process.start_time() == discovered.process_started
                && exe.canonicalize().ok()
                    == Some(
                        expected_exe
                            .canonicalize()
                            .map_err(|_| "client_path_denied")?,
                    )
                && crate::watcher::process_name_matches(process.name(), "LeagueClient.exe"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = discovered.process_started;
            Ok(false)
        }
    })
    .await
    .map_err(|_| "discovery_worker")?
}

pub(super) struct Transport {
    http: Client,
}

#[derive(Default, Serialize)]
pub(super) struct Metrics {
    pub requests: u64,
    pub response_bytes: u64,
    pub last_endpoint: Option<&'static str>,
    pub last_http_status: Option<u16>,
}

enum Endpoint {
    Summoner,
    Gameflow,
}
impl Endpoint {
    fn path(&self) -> &'static str {
        match self {
            Self::Summoner => "/lol-summoner/v1/current-summoner",
            Self::Gameflow => "/lol-gameflow/v1/session",
        }
    }
}

impl Transport {
    pub(super) fn new() -> Result<Self, &'static str> {
        Ok(Self {
            http: Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .danger_accept_invalid_certs(true)
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(REQUEST_TIMEOUT)
                .build()
                .map_err(|_| "http_client")?,
        })
    }

    pub(super) async fn round(
        &self,
        discovered: &Discovered,
        metrics: &mut Metrics,
    ) -> Result<LcuRound, &'static str> {
        let summoner: Summoner = self
            .get(&discovered.credentials, Endpoint::Summoner, metrics)
            .await?;
        let gameflow: Gameflow = self
            .get(&discovered.credentials, Endpoint::Gameflow, metrics)
            .await?;
        let round = LcuRound { summoner, gameflow };
        round.validate_strings()?;
        Ok(round)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        credentials: &Credentials,
        endpoint: Endpoint,
        metrics: &mut Metrics,
    ) -> Result<T, &'static str> {
        metrics.requests += 1;
        metrics.last_endpoint = Some(endpoint.path());
        metrics.last_http_status = None;
        // No public URL/method override and no reqwest/serde error strings escape.
        let mut response = self
            .http
            .get(format!(
                "https://127.0.0.1:{}{}",
                credentials.port,
                endpoint.path()
            ))
            .basic_auth("riot", Some(&credentials.secret))
            .send()
            .await
            .map_err(|_| "lcu_transport")?;
        metrics.last_http_status = Some(response.status().as_u16());
        if !response.status().is_success() {
            return Err("lcu_http_status");
        }
        if response
            .content_length()
            .is_some_and(|n| n > RESPONSE_LIMIT as u64)
        {
            return Err("lcu_body_size");
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "lcu_transport")? {
            metrics.response_bytes += chunk.len() as u64;
            if chunk.len() > RESPONSE_LIMIT - body.len() {
                return Err("lcu_body_size");
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body).map_err(|_| "lcu_json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockfile_is_strict_and_secret_debug_is_redacted() {
        let value = parse_lockfile(b"LeagueClient:123:456:synthetic-secret:https").unwrap();
        assert_eq!(value.pid, 123);
        assert_eq!(format!("{value:?}"), "Credentials([redacted])");
        for invalid in [
            "Other:123:456:secret:https",
            "LeagueClient:0:456:secret:https",
            "LeagueClient:123:0:secret:https",
            "LeagueClient:123:456::https",
            "LeagueClient:123:456:secret:http",
            "LeagueClient:123:456:secret:https:extra",
            "LeagueClient:123:456:secret\n:https",
            "LeagueClient:0123:456:secret:https",
        ] {
            assert!(parse_lockfile(invalid.as_bytes()).is_err());
        }
        assert!(parse_lockfile(&vec![b'x'; 4097]).is_err());
    }

    #[test]
    fn bounded_reads_do_not_truncate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture");
        std::fs::write(&path, [0; 16]).unwrap();
        assert_eq!(read_bounded(&path, 16).unwrap().len(), 16);
        assert_eq!(read_bounded(&path, 15), Err("file_size"));
    }
}
