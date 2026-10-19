//! Small source-of-truth EOG ledger. SQLite is never involved in acquisition.
use super::{
    result::Binding,
    sidecar::{DirectoryGuard, DirectoryLocator},
};
use chronobreak_league_data::{ResultFile, canonical_game_id};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
    sync::Arc,
};

pub(super) const FILE: &str = "league_eog_jobs.json";
pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;
pub(super) const EXPIRY_MS: i64 = 180_000;
pub(super) const CADENCE_MS: i64 = 10_000;
pub(super) const MAX_PENDING: usize = 4;
pub(super) const MAX_ATTEMPTS: u32 = 18;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Terminal {
    Completed,
    Cancelled,
    Rejected,
    Expired,
    Deleted,
    PublicationFailed,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedJob {
    pub binding: Binding,
    pub directory: DirectoryLocator,
    pub closed_ms: i64,
    pub expires_ms: i64,
    pub next_ms: i64,
    pub attempts: u32,
    pub published: bool,
    pub result: Option<ResultFile>,
    pub terminal: Option<Terminal>,
}
impl SavedJob {
    pub fn new(
        binding: Binding,
        directory: DirectoryLocator,
        closed_ms: i64,
    ) -> Result<Self, &'static str> {
        let expires_ms = closed_ms.checked_add(EXPIRY_MS).ok_or("ledger_time")?;
        let job = Self {
            binding,
            directory,
            closed_ms,
            expires_ms,
            next_ms: closed_ms,
            attempts: 0,
            published: false,
            result: None,
            terminal: None,
        };
        job.validate()?;
        Ok(job)
    }
    fn validate(&self) -> Result<(), &'static str> {
        let b = &self.binding;
        if !canonical_game_id(&b.game_id)
            || !super::bounded(&b.puuid)
            || !super::full_identity(&b.riot_id)
            || !super::bounded(&b.mode)
            || b.map_id == 0
            || self.directory.media_id != b.media_id
            || !self.directory.path.is_absolute()
            || self
                .directory
                .path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || self.closed_ms <= 0
            || self.closed_ms.checked_add(EXPIRY_MS) != Some(self.expires_ms)
            || self.next_ms < self.closed_ms
            || self.next_ms > self.expires_ms.saturating_add(CADENCE_MS)
            || self.attempts > MAX_ATTEMPTS
            || self
                .result
                .as_ref()
                .is_some_and(|r| !r.valid_for(&b.media_id, &b.game_id))
        {
            return Err("ledger_job_invalid");
        }
        if let Some(result) = &self.result {
            // Result validity includes field/array bounds; retain the sidecar's
            // independent serialized-size bound before admitting it to a job.
            let bytes = encode(result, chronobreak_league_data::MAX_RESULT_BYTES)?;
            drop(bytes);
        }
        Ok(())
    }
    pub fn reserve(&mut self, now_ms: i64) -> bool {
        if self.terminal.is_some()
            || self.result.is_some()
            || now_ms < self.next_ms
            || now_ms < self.closed_ms
            || now_ms >= self.expires_ms
            || self.attempts == MAX_ATTEMPTS
        {
            return false;
        }
        self.attempts += 1;
        self.next_ms = now_ms.saturating_add(CADENCE_MS);
        true
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Ledger {
    schema_version: u32,
    pub last_wall_ms: i64,
    pub jobs: Vec<SavedJob>,
}
impl Ledger {
    pub fn empty(now_ms: i64) -> Self {
        Self {
            schema_version: 1,
            last_wall_ms: now_ms,
            jobs: Vec::new(),
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 || self.last_wall_ms <= 0 || self.jobs.len() > MAX_PENDING {
            return Err("ledger_invalid");
        }
        let mut ids = std::collections::HashSet::new();
        for job in &self.jobs {
            job.validate()?;
            if job.closed_ms > self.last_wall_ms || !ids.insert(&job.binding.media_id) {
                return Err("ledger_job_invalid");
            }
        }
        Ok(())
    }
    /// Clock rollback closes all retained work; neither expiry nor a reserved
    /// attempt is renewed on restart. This transition must be saved before work.
    pub fn reconcile_time(&mut self, now_ms: i64) -> Result<(), &'static str> {
        if now_ms <= 0 {
            return Err("ledger_clock");
        }
        let rollback = now_ms < self.last_wall_ms;
        for job in &mut self.jobs {
            if job.terminal.is_none() && (rollback || now_ms >= job.expires_ms) {
                job.terminal = Some(Terminal::Expired);
            }
        }
        // Preserve the high-water wall value: subsequent restarts during rollback
        // cannot revive work or admit a fresh job against an uncertain clock.
        self.last_wall_ms = self.last_wall_ms.max(now_ms);
        Ok(())
    }
}

fn encode(value: &impl Serialize, cap: usize) -> Result<Vec<u8>, &'static str> {
    struct Capped {
        bytes: Vec<u8>,
        cap: usize,
    }
    impl std::io::Write for Capped {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.cap.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other("ledger_size"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Capped {
        bytes: Vec::new(),
        cap,
    };
    serde_json::to_writer(&mut output, value).map_err(|_| "ledger_encode")?;
    Ok(output.bytes)
}

pub(super) fn wall_ms() -> Result<i64, &'static str> {
    let value = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "ledger_clock")?;
    i64::try_from(value.as_millis()).map_err(|_| "ledger_clock")
}

pub(super) struct Store {
    root: DirectoryGuard,
    _lock: File,
}
impl Store {
    /// Called by the owned coordinator's blocking task. A lock survives until
    /// that coordinator and every pending save have joined; no game is created.
    pub fn open(path: &Path, now_ms: i64) -> Result<(Arc<Self>, Ledger), &'static str> {
        let root = DirectoryGuard::capture(path, MediaId::new_v4())?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            options
                .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
        }
        let lock = options
            .open(path.join(".league_eog_jobs.lock"))
            .map_err(|_| "ledger_lock_open")?;
        let lock_info = lock.metadata().map_err(|_| "ledger_lock_metadata")?;
        if !plain_file(&lock_info) {
            return Err("ledger_lock_invalid");
        }
        lock.try_lock().map_err(|_| "ledger_already_owned")?;
        let ledger_path = path.join(FILE);
        let mut ledger = match File::open(&ledger_path) {
            Ok(file) => {
                let info =
                    std::fs::symlink_metadata(&ledger_path).map_err(|_| "ledger_metadata")?;
                if !plain_file(&info) || info.len() > MAX_BYTES as u64 {
                    return Err("ledger_size");
                }
                let mut bytes = Vec::new();
                file.take(MAX_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "ledger_read")?;
                if bytes.len() > MAX_BYTES {
                    return Err("ledger_size");
                }
                serde_json::from_slice::<Ledger>(&bytes).map_err(|_| "ledger_decode")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ledger::empty(now_ms),
            Err(_) => return Err("ledger_open"),
        };
        if !root.available() {
            return Err("ledger_root_changed");
        }
        ledger.validate()?;
        ledger.reconcile_time(now_ms)?;
        let store = Arc::new(Self { root, _lock: lock });
        store.save(&ledger)?;
        Ok((store, ledger))
    }
    pub fn save(&self, ledger: &Ledger) -> Result<(), &'static str> {
        ledger.validate()?;
        let games = self.root.locator()?.path.join("games");
        if ledger
            .jobs
            .iter()
            .any(|j| j.directory.path.parent() != Some(games.as_path()))
        {
            return Err("ledger_outside_library");
        }
        self.root.replace_ledger(&encode(ledger, MAX_BYTES)?)
    }
    pub async fn persist(self: &Arc<Self>, ledger: Ledger) -> Result<(), &'static str> {
        let owner = Arc::clone(self);
        // work() is joined by Coordinator even when its stop future is dropped.
        tokio::task::spawn_blocking(move || owner.save(&ledger))
            .await
            .map_err(|_| "ledger_worker")?
    }
}

fn plain_file(info: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if info.file_attributes()
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
            != 0
        {
            return false;
        }
    }
    info.is_file()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    const NOW: i64 = 1_800_000_000_000;
    fn job(root: &Path) -> SavedJob {
        let media = MediaId::new_v4();
        let games = root.join("games");
        std::fs::create_dir_all(&games).unwrap();
        let path = games.join(media.as_str());
        std::fs::create_dir(&path).unwrap();
        let directory = DirectoryGuard::capture(&path, media.clone()).unwrap();
        SavedJob::new(
            super::super::association::fixture_candidate(media).result_binding(),
            directory.locator().unwrap(),
            NOW,
        )
        .unwrap()
    }

    #[test]
    fn repeated_restart_preserves_reserved_attempts_cadence_and_original_expiry() {
        let root = tempfile::tempdir().unwrap();
        let (store, mut state) = Store::open(root.path(), NOW).unwrap();
        state.jobs.push(job(root.path()));
        store.save(&state).unwrap();
        drop(store);
        for attempt in 0..MAX_ATTEMPTS {
            let now = NOW + i64::from(attempt) * CADENCE_MS;
            let (store, mut state) = Store::open(root.path(), now).unwrap();
            assert_eq!(state.jobs[0].attempts, attempt);
            assert_eq!(state.jobs[0].expires_ms, NOW + EXPIRY_MS);
            assert!(state.jobs[0].reserve(now));
            store.save(&state).unwrap();
            drop(store); // Simulate interruption after reservation, before response.
            let (store, mut recovered) = Store::open(root.path(), now + 1).unwrap();
            assert_eq!(recovered.jobs[0].attempts, attempt + 1);
            assert!(!recovered.jobs[0].reserve(now + 1));
            drop(store);
        }
        let (_, recovered) = Store::open(root.path(), NOW + EXPIRY_MS).unwrap();
        assert!(matches!(
            recovered.jobs[0].terminal,
            Some(Terminal::Expired)
        ));
        assert_eq!(recovered.jobs[0].attempts, MAX_ATTEMPTS);
    }

    #[test]
    fn rollback_is_terminal_and_does_not_renew_the_clock_high_water() {
        let root = tempfile::tempdir().unwrap();
        let (store, mut state) = Store::open(root.path(), NOW).unwrap();
        state.jobs.push(job(root.path()));
        state.last_wall_ms = NOW + 50_000;
        store.save(&state).unwrap();
        drop(store);
        let (store, recovered) = Store::open(root.path(), NOW + 40_000).unwrap();
        assert!(matches!(
            recovered.jobs[0].terminal,
            Some(Terminal::Expired)
        ));
        assert_eq!(recovered.last_wall_ms, NOW + 50_000);
        drop(store);
        let (_, recovered) = Store::open(root.path(), NOW + 60_000).unwrap();
        assert!(matches!(
            recovered.jobs[0].terminal,
            Some(Terminal::Expired)
        ));
    }

    #[test]
    fn locator_survives_restart_but_rejects_deleted_or_replaced_directory() {
        let root = tempfile::tempdir().unwrap();
        let job = job(root.path());
        let directory = DirectoryGuard::recover(&job.directory).unwrap();
        drop(directory);
        std::fs::rename(&job.directory.path, root.path().join("old")).unwrap();
        std::fs::create_dir(&job.directory.path).unwrap();
        std::fs::write(job.directory.path.join("sentinel"), b"replacement").unwrap();
        assert!(DirectoryGuard::recover(&job.directory).is_err());
        assert_eq!(
            std::fs::read(job.directory.path.join("sentinel")).unwrap(),
            b"replacement"
        );
        std::fs::remove_file(job.directory.path.join("sentinel")).unwrap();
        std::fs::remove_dir(&job.directory.path).unwrap();
        assert!(DirectoryGuard::recover(&job.directory).is_err());
        assert!(!job.directory.path.exists());
    }

    #[test]
    fn store_has_one_owner_and_preserves_invalid_ledger_instead_of_resetting_budgets() {
        let root = tempfile::tempdir().unwrap();
        let (store, _) = Store::open(root.path(), NOW).unwrap();
        assert!(Store::open(root.path(), NOW).is_err());
        drop(store);
        let invalid = b"{\"schema_version\":1,\"jobs\":";
        std::fs::write(root.path().join(FILE), invalid).unwrap();
        assert!(Store::open(root.path(), NOW).is_err());
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), invalid);
    }

    #[test]
    fn pending_confirmed_result_and_terminal_state_round_trip_without_credentials() {
        let root = tempfile::tempdir().unwrap();
        let (store, mut state) = Store::open(root.path(), NOW).unwrap();
        let mut job = job(root.path());
        let eog: super::super::result::Eog = serde_json::from_value(
            serde_json::json!({"gameId": job.binding.game_id.parse::<u64>().unwrap()}),
        )
        .unwrap();
        job.result = Some(eog.confirm(&job.binding).unwrap());
        state.jobs.push(job);
        store.save(&state).unwrap();
        drop(store);
        let (store, mut state) = Store::open(root.path(), NOW + 1).unwrap();
        assert!(state.jobs[0].result.is_some());
        assert!(!state.jobs[0].published);
        assert!(!state.jobs[0].reserve(NOW + 1));
        state.jobs[0].published = true;
        state.jobs[0].terminal = Some(Terminal::Completed);
        store.save(&state).unwrap();
        drop(store);
        let (_, state) = Store::open(root.path(), NOW + 2).unwrap();
        assert!(state.jobs[0].published);
        assert!(matches!(state.jobs[0].terminal, Some(Terminal::Completed)));
        let wire = String::from_utf8(std::fs::read(root.path().join(FILE)).unwrap()).unwrap();
        for forbidden in ["password", "authorization", "credential", "access_token"] {
            assert!(!wire.contains(forbidden));
        }
    }
}
