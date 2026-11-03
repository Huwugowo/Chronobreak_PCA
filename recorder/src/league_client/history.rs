//! Recoverable match-history acquisition. EOG remains an independent comparison source.
use super::{
    result::{Binding, Eog},
    sidecar::{DirectoryGuard, DirectoryLocator},
    transport::{self, Metrics, Transport},
};
use chronobreak_league_data::{Outcome, ResultFile};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Match {
    #[serde(skip)]
    evidence: Option<chronobreak_league_data::ResultEvidence>,
    game_id: u64,
    #[serde(default)]
    participants: Vec<Participant>,
    #[serde(default)]
    participant_identities: Vec<Identity>,
    #[serde(default)]
    teams: Vec<MatchTeam>,
    game_duration: Option<u64>,
    queue_id: Option<u64>,
    map_id: Option<u64>,
    game_version: Option<String>,
    game_mode: Option<String>,
    game_type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Participant {
    participant_id: u64,
    team_id: u64,
    champion_id: u64,
    spell1_id: Option<u64>,
    spell2_id: Option<u64>,
    #[serde(default)]
    stats: serde_json::Map<String, Value>,
    #[serde(default)]
    timeline: serde_json::Map<String, Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    participant_id: u64,
    player: IdentityPlayer,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityPlayer {
    puuid: Option<String>,
    #[serde(alias = "riotIdGameName")]
    game_name: Option<String>,
    #[serde(alias = "riotIdTagLine")]
    tag_line: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchTeam {
    team_id: u64,
    win: Option<String>,
}
impl Match {
    pub(super) fn set_receipt(&mut self, start: std::time::Instant) -> Result<(), &'static str> {
        self.evidence = Some(chronobreak_league_data::ResultEvidence {
            run_id: chronobreak_replay_time::MediaId::new_v4(),
            timing: chronobreak_league_data::observation::RequestTiming {
                start_us: 0,
                end_us: start
                    .elapsed()
                    .as_micros()
                    .try_into()
                    .map_err(|_| "history_clock")?,
                received_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .map_err(|_| "history_clock")?,
            },
        });
        Ok(())
    }

    fn confirm(&self, binding: &Binding) -> Result<ResultFile, &'static str> {
        if self.game_id.to_string() != binding.game_id
            || self.participants.is_empty()
            || self.participants.len() > chronobreak_league_data::MAX_PLAYERS
            || self.participant_identities.len() != self.participants.len()
            || self.teams.len() > chronobreak_league_data::MAX_TEAMS
        {
            return Err("history_identity");
        }
        let identities = self
            .participant_identities
            .iter()
            .map(|i| (i.participant_id, &i.player))
            .collect::<BTreeMap<_, _>>();
        let ids = self
            .participants
            .iter()
            .map(|p| p.participant_id)
            .collect::<std::collections::BTreeSet<_>>();
        if identities.len() != self.participant_identities.len()
            || ids.len() != self.participants.len()
        {
            return Err("history_duplicate_participant");
        }
        let mut teams = BTreeMap::<u64, Vec<Value>>::new();
        let mut local = None;
        let mut outcome = Outcome::Unknown;
        for p in &self.participants {
            let id = identities
                .get(&p.participant_id)
                .ok_or("history_participant_join")?;
            let is_local = id.puuid.as_deref() == Some(&binding.puuid);
            let mut stats = p.stats.clone();
            let win = stats.remove("win").and_then(|v| v.as_bool());
            if let Some(win) = win {
                stats.insert("WIN".into(), u64::from(win).into());
            }
            for slot in 0..6 {
                for suffix in ["", "Var1", "Var2", "Var3"] {
                    let key = format!("perk{slot}{suffix}");
                    if let Some(v) = stats.remove(&key) {
                        let alias = if suffix.is_empty() {
                            format!("PERK{slot}")
                        } else {
                            format!("PERK{slot}_VAR{}", &suffix[3..])
                        };
                        stats.insert(alias, v);
                    }
                }
            }
            for (from, to) in [
                ("perkPrimaryStyle", "PERK_PRIMARY_STYLE"),
                ("perkSubStyle", "PERK_SUB_STYLE"),
            ] {
                if let Some(v) = stats.remove(from) {
                    stats.insert(to.into(), v);
                }
            }
            let items = (0..7)
                .map(|i| {
                    p.stats
                        .get(&format!("item{i}"))
                        .cloned()
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            let player = json!({"gameId":self.game_id,"puuid":id.puuid,"riotIdGameName":id.game_name,"riotIdTagLine":id.tag_line,"teamId":p.team_id,"championId":p.champion_id,"isLocalPlayer":is_local,"spell1Id":p.spell1_id,"spell2Id":p.spell2_id,"selectedPosition":p.timeline.get("lane"),"detectedTeamPosition":p.timeline.get("role"),"items":items,"stats":stats});
            if is_local {
                if local.is_some() {
                    return Err("history_local_ambiguous");
                }
                outcome = match win {
                    Some(true) => Outcome::Win,
                    Some(false) => Outcome::Loss,
                    None => Outcome::Unknown,
                };
                if let Some(team) = self
                    .teams
                    .iter()
                    .find(|t| t.team_id == p.team_id)
                    .and_then(|t| t.win.as_deref())
                    && matches!((win, team), (Some(true), "Fail") | (Some(false), "Win"))
                {
                    return Err("history_outcome_conflict");
                }
                local = Some(player.clone());
            }
            teams.entry(p.team_id).or_default().push(player);
        }
        let local = local.ok_or("history_local_missing")?;
        let mut eog: Eog = serde_json::from_value(json!({"gameId":self.game_id,"localPlayer":local,"teams":teams.into_iter().map(|(id,players)|json!({"teamId":id,"players":players})).collect::<Vec<_>>(),"gameLength":self.game_duration,"endOfGameTimestamp":null,"queueId":self.queue_id,"mapId":self.map_id,"gameVersion":self.game_version,"gameMode":self.game_mode,"gameType":self.game_type})).map_err(|_|"history_decode")?;
        eog.evidence = self.evidence.clone();
        let mut result = eog.confirm(binding).map_err(|_| "history_association")?;
        result.source = "lcu_match_history".into();
        result.outcome = outcome;
        if !result.valid_for(&binding.media_id, &binding.game_id) {
            return Err("history_facts");
        }
        if serde_json::to_vec(&result)
            .map_err(|_| "history_encode")?
            .len()
            > chronobreak_league_data::MAX_RESULT_BYTES
        {
            return Err("history_size");
        }
        Ok(result)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Job {
    binding: Binding,
    directory: DirectoryLocator,
    attempts: u32,
    next_ms: i64,
    result: Option<ResultFile>,
    done: bool,
}
#[derive(Clone)]
pub(super) struct Handle(mpsc::Sender<Job>);
impl Handle {
    pub(super) fn submit(&self, binding: Binding, directory: DirectoryLocator) {
        if self
            .0
            .try_send(Job {
                binding,
                directory,
                attempts: 0,
                next_ms: 0,
                result: None,
                done: false,
            })
            .is_err()
        {
            tracing::warn!("history job queue unavailable");
        }
    }
}
pub(super) struct Coordinator {
    pub handle: Handle,
    stop: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}
impl Coordinator {
    pub(super) fn start(root: &Path) -> Self {
        let (send, receive) = mpsc::channel(16);
        let (stop, cancel) = watch::channel(false);
        let root = root.to_owned();
        Self {
            handle: Handle(send),
            stop,
            task: Some(tokio::spawn(run(root, receive, cancel))),
        }
    }
    pub(super) async fn stop(mut self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for Coordinator {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}
fn admit(jobs: &mut Vec<Job>, job: Job) -> bool {
    if jobs
        .iter()
        .any(|j| j.binding.media_id == job.binding.media_id)
    {
        return false;
    }
    if jobs.len() == 24 {
        if let Some(i) = jobs.iter().position(|j| j.done) {
            jobs.remove(i);
        } else {
            tracing::warn!("history pending capacity reached");
            return false;
        }
    }
    jobs.push(job);
    true
}
fn wall() -> i64 {
    super::post_game_ledger::wall_ms().unwrap_or(0)
}
async fn save(root: Arc<DirectoryGuard>, jobs: &[Job]) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(jobs).map_err(|_| "history_jobs_encode")?;
    tokio::task::spawn_blocking(move || root.replace_history_jobs(&bytes))
        .await
        .map_err(|_| "history_jobs_worker")?
}
async fn run(path: PathBuf, mut receive: mpsc::Receiver<Job>, mut stop: watch::Receiver<bool>) {
    let loaded = tokio::task::spawn_blocking(move || {
        use std::{
            fs::{File, OpenOptions},
            io::Read,
        };
        let root = Arc::new(DirectoryGuard::capture(
            &path,
            chronobreak_replay_time::MediaId::new_v4(),
        )?);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.join(".league_history_jobs.lock"))
            .map_err(|_| "history_lock")?;
        if !lock.metadata().map_err(|_| "history_lock")?.is_file()
            || std::fs::symlink_metadata(path.join(".league_history_jobs.lock"))
                .map_err(|_| "history_lock")?
                .is_symlink()
        {
            return Err("history_lock_type");
        }
        lock.try_lock().map_err(|_| "history_owned")?;
        let mut bytes = Vec::new();
        if std::fs::symlink_metadata(path.join("league_history_jobs.json"))
            .is_ok_and(|m| !m.is_file() || m.is_symlink())
        {
            return Err("history_jobs_type");
        }
        let jobs = match File::open(path.join("league_history_jobs.json")) {
            Ok(file) => {
                file.take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "history_read")?;
                if bytes.len() > 8 * 1024 * 1024 {
                    return Err("history_size");
                }
                serde_json::from_slice::<Vec<Job>>(&bytes).map_err(|_| "history_json")?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err("history_read"),
        };
        if jobs.len() > 24
            || jobs.iter().any(|j| {
                j.directory.path.parent() != Some(path.join("games").as_path())
                    || j.binding.media_id != j.directory.media_id
                    || !chronobreak_league_data::canonical_game_id(&j.binding.game_id)
                    || !super::bounded(&j.binding.puuid)
                    || j.result
                        .as_ref()
                        .is_some_and(|r| !r.valid_for(&j.binding.media_id, &j.binding.game_id))
            })
        {
            return Err("history_jobs_invalid");
        }
        Ok::<_, &'static str>((root, lock, jobs))
    })
    .await;
    let Ok(Ok((root, _lock, mut jobs))) = loaded else {
        tracing::warn!("history recovery unavailable");
        return;
    };
    let Ok(transport) = Transport::new() else {
        return;
    };
    let mut metrics = Metrics::default();
    let mut dirty = false;
    loop {
        while let Ok(job) = receive.try_recv() {
            dirty |= admit(&mut jobs, job);
        }
        if dirty {
            if save(Arc::clone(&root), &jobs).await.is_err() {
                tracing::warn!("history persistence unavailable");
                break;
            }
            dirty = false;
        }
        if *stop.borrow() {
            break;
        }
        if let Some(i) = jobs.iter().position(|j| !j.done && j.next_ms <= wall()) {
            // Removed allocations cannot receive facts. Publication of an existing
            // allocation may still be pending, so only absence closes this job.
            let locator = jobs[i].directory.clone();
            let missing = tokio::task::spawn_blocking(move || {
                std::fs::symlink_metadata(locator.path)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            })
            .await
            .unwrap_or(false);
            if missing {
                jobs[i].done = true;
                dirty = true;
                continue;
            }
            // Reserve before HTTP. Delayed/unavailable history remains pending across restarts.
            jobs[i].attempts = jobs[i].attempts.saturating_add(1);
            let delay =
                [2, 5, 15, 30, 60, 120, 300][usize::try_from(jobs[i].attempts.saturating_sub(1))
                    .unwrap_or(6)
                    .min(6)];
            jobs[i].next_ms = wall().saturating_add(delay * 1000);
            if save(Arc::clone(&root), &jobs).await.is_err() {
                break;
            }
            let binding = jobs[i].binding.clone();
            if jobs[i].result.is_none() {
                if let Some(cached) = jobs
                    .iter()
                    .find(|j| {
                        j.binding.game_id == binding.game_id
                            && j.binding.puuid == binding.puuid
                            && j.result.is_some()
                    })
                    .and_then(|j| j.result.clone())
                {
                    let mut result = cached;
                    result.media_id = binding.media_id.clone();
                    jobs[i].result = Some(result);
                } else if let Ok(discovered) = transport::discover().await {
                    let id = binding.game_id.parse().unwrap_or(0);
                    let response = tokio::select! { _=stop.changed()=>None, r=transport.history(&discovered,&mut metrics,id)=>r.ok() };
                    if let Some(response) = response {
                        match response.confirm(&binding) {
                            Ok(result) => jobs[i].result = Some(result),
                            Err(code) => tracing::warn!(code, "history response not accepted"),
                        }
                    }
                }
            }
            // Persist fetched facts before installing into any recording allocation.
            if save(Arc::clone(&root), &jobs).await.is_err() {
                break;
            }
            if let Some(result) = jobs[i].result.clone() {
                let locator = jobs[i].directory.clone();
                let installed = tokio::task::spawn_blocking(move || {
                    DirectoryGuard::recover(&locator)?.install_history(&result)
                })
                .await;
                if matches!(installed, Ok(Ok(()))) {
                    jobs[i].done = true;
                    dirty = true;
                    tracing::info!(attempts = jobs[i].attempts, "match history prepared");
                }
            }
            continue;
        }
        tokio::select! {_=stop.changed()=>{}, job=receive.recv()=>{if let Some(job)=job {dirty |= admit(&mut jobs,job);}else{break;}},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        Binding {
            media_id: chronobreak_replay_time::MediaId::new_v4(),
            game_id: "42".into(),
            puuid: "local".into(),
            riot_id: "Player#EUW".into(),
            map_id: 11,
            mode: "CLASSIC".into(),
            queue_id: 400,
        }
    }
    fn response(win: bool) -> Match {
        serde_json::from_value(json!({"gameId":42,"participants":[{"participantId":1,"teamId":100,"championId":11,"stats":{"win":win,"kills":3,"item0":1001}}],"participantIdentities":[{"participantId":1,"player":{"puuid":"local","gameName":"Player","tagLine":"EUW"}}],"teams":[{"teamId":100,"win":if win {"Win"}else{"Fail"}}]})).unwrap()
    }
    #[test]
    fn history_uses_exact_participant_join_and_boolean_outcomes() {
        for win in [true, false] {
            let b = binding();
            let r = response(win).confirm(&b).unwrap();
            assert!(r.valid_for(&b.media_id, "42"));
            assert_eq!(r.outcome, if win { Outcome::Win } else { Outcome::Loss });
            assert_eq!(r.local_player.unwrap().stats.kills, Some(3));
        }
    }
    #[test]
    fn history_rejects_wrong_game_account_and_duplicate_participants() {
        let mut b = binding();
        b.game_id = "43".into();
        assert!(response(true).confirm(&b).is_err());
        b.game_id = "42".into();
        b.puuid = "other".into();
        assert!(response(true).confirm(&b).is_err());
        let mut r = response(true);
        r.participant_identities.push(Identity {
            participant_id: 1,
            player: IdentityPlayer {
                puuid: Some("local".into()),
                game_name: None,
                tag_line: None,
            },
        });
        assert!(r.confirm(&binding()).is_err());
    }
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recovered_history_installs_without_overwriting_eog_and_is_durable() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("games/100");
        std::fs::create_dir_all(&directory).unwrap();
        let media = chronobreak_replay_time::MediaId::new_v4();
        let mut result = super::super::sidecar::tests::result_fixture(&directory, &media);
        let original = b"independent eog comparison";
        std::fs::write(
            directory.join(chronobreak_league_data::RESULT_FILE),
            original,
        )
        .unwrap();
        result.source = "lcu_match_history".into();
        let binding = super::super::association::fixture_candidate(media.clone()).result_binding();
        let locator = DirectoryGuard::capture(&directory, media.clone())
            .unwrap()
            .locator()
            .unwrap();
        let job = Job {
            binding,
            directory: locator,
            attempts: 1,
            next_ms: 0,
            result: Some(result),
            done: false,
        };
        std::fs::write(
            root.path().join("league_history_jobs.json"),
            serde_json::to_vec(&vec![job]).unwrap(),
        )
        .unwrap();
        let coordinator = Coordinator::start(root.path());
        tokio::time::timeout(Duration::from_secs(5), async {
            while !directory
                .join(chronobreak_league_data::HISTORY_RESULT_FILE)
                .exists()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        coordinator.stop().await;
        assert_eq!(
            std::fs::read(directory.join(chronobreak_league_data::RESULT_FILE)).unwrap(),
            original
        );
        let saved: Vec<Job> = serde_json::from_slice(
            &std::fs::read(root.path().join("league_history_jobs.json")).unwrap(),
        )
        .unwrap();
        assert!(saved[0].done);
        let installed: ResultFile = serde_json::from_slice(
            &std::fs::read(directory.join(chronobreak_league_data::HISTORY_RESULT_FILE)).unwrap(),
        )
        .unwrap();
        assert!(installed.valid_for(&media, &saved[0].binding.game_id));
    }
}
