use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::storage::{GAME_LOG_JSON, write_json_atomic};

const LIVE_CLIENT_BASE_URL: &str = "https://127.0.0.1:2999/liveclientdata";
const API_TIMEOUT: Duration = Duration::from_secs(2);
const CALIBRATION_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(10);
const CALIBRATION_SAMPLE_COUNT: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GameLog {
    pub game_start_video_offset_ms: Option<i64>,
    pub snapshots: Vec<Snapshot>,
    pub events: Vec<GameEvent>,
    pub snapshot_derived_changes: Vec<SnapshotChange>,
    pub matchv5: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub game_time_ms: i64,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerSnapshot {
    pub summoner_name: String,
    pub team: String,
    pub champion: String,
    pub gold: Option<i64>,
    pub hp: Option<i64>,
    pub hp_max: Option<i64>,
    pub cs: u32,
    pub level: u32,
    pub items: Vec<ItemSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summoner_spells: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystone_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rune_ids: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemSnapshot {
    pub item_id: u32,
    pub slot: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub game_time_ms: i64,
    pub video_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub victim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assisters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inhibitor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dragon_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stolen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kill_streak: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acing_team: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotChange {
    pub game_time_ms: i64,
    pub player: String,
    pub change_type: SnapshotChangeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_level: Option<u32>,
    pub video_time_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SnapshotChangeType {
    ItemPurchased,
    ItemSold,
    LevelUp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollerSummary {
    pub game_start_video_offset_ms: Option<i64>,
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordingMetadata {
    pub recorded_at: String,
    pub duration_ms: u64,
    pub game_mode: Option<String>,
    pub local_player_summoner_name: Option<String>,
    pub local_player_champion: Option<String>,
    pub local_player_team: Option<String>,
    pub video_offset_ms: Option<i64>,
    pub encoder_used: String,
    pub recording_resolution: String,
    pub recording_fps: u32,
    pub win: Option<bool>,
    pub win_method: WinMethod,
    pub matchv5_fetched: bool,
    pub saved: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WinMethod {
    Matchv5,
    Derived,
    Unknown,
}

impl RecordingMetadata {
    pub fn new(
        recorded_at: SystemTime,
        duration: Duration,
        summary: PollerSummary,
        encoder_used: String,
        recording_resolution: String,
        recording_fps: u32,
    ) -> Result<Self> {
        let recorded_at = OffsetDateTime::from(recorded_at)
            .format(&Rfc3339)
            .context("failed to format recording start time")?;
        let duration_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);

        Ok(Self {
            recorded_at,
            duration_ms,
            game_mode: summary.game_mode,
            local_player_summoner_name: summary.local_player_summoner_name,
            local_player_champion: summary.local_player_champion,
            local_player_team: summary.local_player_team,
            video_offset_ms: summary.game_start_video_offset_ms,
            encoder_used,
            recording_resolution,
            recording_fps,
            win: None,
            win_method: WinMethod::Unknown,
            matchv5_fetched: false,
            saved: false,
        })
    }
}

pub struct PollerSession {
    cancellation: watch::Sender<bool>,
    task: JoinHandle<Result<()>>,
    state: Arc<Mutex<PollerState>>,
    output: PathBuf,
}

impl PollerSession {
    pub async fn start(directory: &Path, video_started_at: Instant) -> Result<Self> {
        let output = directory.join(GAME_LOG_JSON);
        let state = Arc::new(Mutex::new(PollerState::default()));
        write_json_atomic(&output, &GameLog::default()).await?;

        let client = LiveClient::new(LIVE_CLIENT_BASE_URL)?;
        let (cancellation, receiver) = watch::channel(false);
        let task_state = Arc::clone(&state);
        let task_output = output.clone();
        let task = tokio::spawn(async move {
            run_poller(client, video_started_at, receiver, task_state, task_output).await
        });

        Ok(Self {
            cancellation,
            task,
            state,
            output,
        })
    }

    pub async fn stop(self) -> PollerSummary {
        let _ = self.cancellation.send(true);
        match self.task.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => error!(%error, "Live Client poller stopped with an error"),
            Err(error) => error!(%error, "Live Client poller task panicked"),
        }

        let state = self.state.lock().await;
        if let Err(error) = write_json_atomic(&self.output, &state.game_log).await {
            error!(%error, "could not perform final game log flush");
        }
        state.summary.clone()
    }
}

#[derive(Debug, Default)]
struct PollerState {
    game_log: GameLog,
    summary: PollerSummary,
}

#[derive(Clone)]
struct LiveClient {
    http: Client,
    base_url: String,
}

impl LiveClient {
    fn new(base_url: &str) -> Result<Self> {
        let http = Client::builder()
            .danger_accept_invalid_certs(true)
            .connect_timeout(API_TIMEOUT)
            .timeout(API_TIMEOUT)
            .no_proxy()
            .build()
            .context("failed to create Live Client HTTP client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    async fn all_game_data(&self) -> Result<Received<RawAllGameData>> {
        self.get("allgamedata").await
    }

    async fn event_data(&self) -> Result<Received<RawEventData>> {
        self.get("eventdata").await
    }

    async fn get<T>(&self, endpoint: &str) -> Result<Received<T>>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}/{endpoint}", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("request to {url} failed"))?
            .error_for_status()
            .with_context(|| format!("request to {url} returned an error status"))?;
        let body = response
            .bytes()
            .await
            .with_context(|| format!("failed to receive response body from {url}"))?;
        let received_at = Instant::now();
        let value = serde_json::from_slice(&body)
            .with_context(|| format!("invalid JSON response from {url}"))?;
        Ok(Received { value, received_at })
    }
}

struct Received<T> {
    value: T,
    received_at: Instant,
}

async fn run_poller(
    client: LiveClient,
    video_started_at: Instant,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    output: PathBuf,
) -> Result<()> {
    info!("waiting for the Live Client API");
    let Some(calibration) = calibrate(&client, video_started_at, &mut cancellation).await? else {
        return Ok(());
    };

    {
        let mut state = state.lock().await;
        state.game_log.game_start_video_offset_ms = Some(calibration.video_offset_ms);
        state.summary.game_start_video_offset_ms = Some(calibration.video_offset_ms);
        update_summary(&mut state.summary, &calibration.initial_data);
        append_snapshot(
            &mut state.game_log,
            &calibration.initial_data,
            calibration.video_offset_ms,
        )?;
        write_json_atomic(&output, &state.game_log).await?;
    }
    info!(
        video_offset_ms = calibration.video_offset_ms,
        "Live Client clock calibrated"
    );

    let event_loop = event_loop(
        client.clone(),
        calibration.video_offset_ms,
        cancellation.clone(),
        Arc::clone(&state),
        output.clone(),
    );
    let snapshot_loop = snapshot_loop(
        client,
        calibration.video_offset_ms,
        cancellation,
        state,
        output,
    );
    tokio::try_join!(event_loop, snapshot_loop)?;
    Ok(())
}

struct Calibration {
    video_offset_ms: i64,
    initial_data: RawAllGameData,
}

async fn calibrate(
    client: &LiveClient,
    video_started_at: Instant,
    cancellation: &mut watch::Receiver<bool>,
) -> Result<Option<Calibration>> {
    let mut candidates = Vec::with_capacity(CALIBRATION_SAMPLE_COUNT);
    let mut initial_data = None;

    while candidates.len() < CALIBRATION_SAMPLE_COUNT {
        let response = tokio::select! {
            _ = cancelled(cancellation) => return Ok(None),
            response = client.all_game_data() => response,
        };

        match response {
            Ok(received) => {
                let Some(game_time_seconds) = valid_game_time(&received.value) else {
                    sleep_or_cancel(CALIBRATION_RETRY_INTERVAL, cancellation).await;
                    continue;
                };
                let elapsed_ms = received
                    .received_at
                    .checked_duration_since(video_started_at)
                    .context("Live Client response predates video capture")?
                    .as_secs_f64()
                    * 1000.0;
                candidates.push((elapsed_ms - game_time_seconds * 1000.0).round() as i64);
                initial_data = Some(received.value);
            }
            Err(error) => {
                debug!(%error, "Live Client API is not ready");
                sleep_or_cancel(CALIBRATION_RETRY_INTERVAL, cancellation).await;
            }
        }
    }

    Ok(Some(Calibration {
        video_offset_ms: median(&mut candidates)?,
        initial_data: initial_data.context("calibration completed without game data")?,
    }))
}

async fn event_loop(
    client: LiveClient,
    video_offset_ms: i64,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    output: PathBuf,
) -> Result<()> {
    let mut seen_event_ids = HashSet::new();

    loop {
        let response = tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            response = client.event_data() => response,
        };
        let received = match response {
            Ok(received) => received,
            Err(error) => {
                info!(%error, "Live Client event polling ended; video remains process-controlled");
                return Ok(());
            }
        };

        let mut new_events = Vec::new();
        for event in received.value.events {
            if seen_event_ids.insert(event.event_id) {
                match normalize_event(event, video_offset_ms) {
                    Ok(event) => new_events.push(event),
                    Err(error) => warn!(%error, "ignoring invalid Live Client event"),
                }
            }
        }
        if !new_events.is_empty() {
            let count = new_events.len();
            let mut state = state.lock().await;
            state.game_log.events.extend(new_events);
            write_json_atomic(&output, &state.game_log).await?;
            debug!(count, "stored Live Client event batch");
        }
        tokio::task::yield_now().await;
    }
}

async fn snapshot_loop(
    client: LiveClient,
    video_offset_ms: i64,
    mut cancellation: watch::Receiver<bool>,
    state: Arc<Mutex<PollerState>>,
    output: PathBuf,
) -> Result<()> {
    let start = tokio::time::Instant::now() + SNAPSHOT_INTERVAL;
    let mut interval = tokio::time::interval_at(start, SNAPSHOT_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            _ = interval.tick() => {}
        }

        let response = tokio::select! {
            _ = cancelled(&mut cancellation) => return Ok(()),
            response = client.all_game_data() => response,
        };
        let received = match response {
            Ok(received) => received,
            Err(error) => {
                info!(%error, "Live Client snapshot polling ended; video remains process-controlled");
                return Ok(());
            }
        };
        if valid_game_time(&received.value).is_none() {
            warn!("ignoring Live Client snapshot without a valid game time");
            continue;
        }

        let mut state = state.lock().await;
        update_summary(&mut state.summary, &received.value);
        append_snapshot(&mut state.game_log, &received.value, video_offset_ms)?;
        write_json_atomic(&output, &state.game_log).await?;
    }
}

fn append_snapshot(
    game_log: &mut GameLog,
    raw: &RawAllGameData,
    video_offset_ms: i64,
) -> Result<()> {
    if raw.all_players.is_empty() {
        return Ok(());
    }
    let first_snapshot = game_log.snapshots.is_empty();
    let snapshot = make_snapshot(raw, first_snapshot)?;
    if let Some(previous) = game_log.snapshots.last() {
        game_log.snapshot_derived_changes.extend(diff_snapshots(
            previous,
            &snapshot,
            video_offset_ms,
        ));
    }
    game_log.snapshots.push(snapshot);
    Ok(())
}

fn make_snapshot(raw: &RawAllGameData, include_stable_fields: bool) -> Result<Snapshot> {
    let game_time_ms = seconds_to_ms(
        valid_game_time(raw).context("snapshot does not contain a valid game time")?,
    )?;
    let active_name = raw.active_player.summoner_name.as_deref();
    let active_riot_id = raw.active_player.riot_id.as_deref();
    let mut players = Vec::with_capacity(raw.all_players.len());

    for player in &raw.all_players {
        let is_active = active_name.is_some_and(|name| name == player.summoner_name)
            || active_riot_id
                .zip(player.riot_id.as_deref())
                .is_some_and(|(active, candidate)| active == candidate);
        let champion_stats = is_active
            .then_some(raw.active_player.champion_stats.as_ref())
            .flatten();
        let summoner_spells = include_stable_fields
            .then(|| {
                [
                    player.summoner_spells.spell_one.as_ref(),
                    player.summoner_spells.spell_two.as_ref(),
                ]
                .into_iter()
                .flatten()
                .filter_map(canonical_spell_id)
                .collect::<Vec<_>>()
            })
            .filter(|spells| !spells.is_empty());
        let keystone_id = include_stable_fields
            .then_some(player.runes.keystone.as_ref().map(|rune| rune.id))
            .flatten();
        let rune_ids = (include_stable_fields && is_active)
            .then(|| {
                raw.active_player
                    .full_runes
                    .general_runes
                    .iter()
                    .chain(&raw.active_player.full_runes.stat_runes)
                    .map(|rune| rune.id)
                    .collect::<Vec<_>>()
            })
            .filter(|runes| !runes.is_empty());

        players.push(PlayerSnapshot {
            summoner_name: player.summoner_name.clone(),
            team: player.team.clone(),
            champion: player.champion_name.clone(),
            gold: is_active
                .then_some(raw.active_player.current_gold.map(round_number))
                .flatten(),
            hp: champion_stats
                .and_then(|stats| stats.current_health)
                .map(round_number),
            hp_max: champion_stats
                .and_then(|stats| stats.max_health)
                .map(round_number),
            cs: player.scores.creep_score,
            level: player.level,
            items: player
                .items
                .iter()
                .map(|item| ItemSnapshot {
                    item_id: item.item_id,
                    slot: item.slot,
                    count: item.count,
                })
                .collect(),
            summoner_spells,
            keystone_id,
            rune_ids,
        });
    }

    Ok(Snapshot {
        game_time_ms,
        players,
    })
}

fn diff_snapshots(
    previous: &Snapshot,
    current: &Snapshot,
    video_offset_ms: i64,
) -> Vec<SnapshotChange> {
    let previous_players: HashMap<&str, &PlayerSnapshot> = previous
        .players
        .iter()
        .map(|player| (player.summoner_name.as_str(), player))
        .collect();
    let video_time_ms = video_offset_ms.saturating_add(current.game_time_ms);
    let mut changes = Vec::new();

    for player in &current.players {
        let Some(previous) = previous_players.get(player.summoner_name.as_str()) else {
            continue;
        };
        let old_items = item_quantities(&previous.items);
        let new_items = item_quantities(&player.items);

        for (item_id, count) in &new_items {
            if *count > old_items.get(item_id).copied().unwrap_or_default() {
                changes.push(SnapshotChange {
                    game_time_ms: current.game_time_ms,
                    player: player.summoner_name.clone(),
                    change_type: SnapshotChangeType::ItemPurchased,
                    item_id: Some(*item_id),
                    new_level: None,
                    video_time_ms,
                });
            }
        }
        for (item_id, count) in &old_items {
            if *count > new_items.get(item_id).copied().unwrap_or_default() {
                changes.push(SnapshotChange {
                    game_time_ms: current.game_time_ms,
                    player: player.summoner_name.clone(),
                    change_type: SnapshotChangeType::ItemSold,
                    item_id: Some(*item_id),
                    new_level: None,
                    video_time_ms,
                });
            }
        }
        if player.level > previous.level {
            changes.push(SnapshotChange {
                game_time_ms: current.game_time_ms,
                player: player.summoner_name.clone(),
                change_type: SnapshotChangeType::LevelUp,
                item_id: None,
                new_level: Some(player.level),
                video_time_ms,
            });
        }
    }
    changes
}

fn item_quantities(items: &[ItemSnapshot]) -> HashMap<u32, u32> {
    let mut quantities = HashMap::new();
    for item in items {
        *quantities.entry(item.item_id).or_default() += item.count.max(1);
    }
    quantities
}

fn normalize_event(raw: RawEvent, video_offset_ms: i64) -> Result<GameEvent> {
    let game_time_ms = seconds_to_ms(raw.event_time)?;
    Ok(GameEvent {
        event_type: raw.event_name,
        game_time_ms,
        video_time_ms: video_offset_ms.saturating_add(game_time_ms),
        killer: string_field(&raw.fields, "KillerName"),
        victim: string_field(&raw.fields, "VictimName"),
        assisters: string_list_field(&raw.fields, "Assisters"),
        turret: string_field(&raw.fields, "TurretKilled"),
        inhibitor: string_field(&raw.fields, "InhibKilled")
            .or_else(|| string_field(&raw.fields, "InhibRespawningSoon"))
            .or_else(|| string_field(&raw.fields, "InhibRespawned")),
        dragon_type: string_field(&raw.fields, "DragonType"),
        stolen: bool_field(&raw.fields, "Stolen"),
        kill_streak: u32_field(&raw.fields, "KillStreak"),
        acer: string_field(&raw.fields, "Acer"),
        acing_team: string_field(&raw.fields, "AcingTeam"),
        result: string_field(&raw.fields, "Result"),
    })
}

fn update_summary(summary: &mut PollerSummary, data: &RawAllGameData) {
    if summary.game_mode.is_none() {
        summary.game_mode = data.game_data.game_mode.clone();
    }
    if summary.local_player_summoner_name.is_none() {
        summary.local_player_summoner_name = data.active_player.summoner_name.clone();
    }
    let local_player = data.all_players.iter().find(|player| {
        data.active_player
            .summoner_name
            .as_deref()
            .is_some_and(|name| name == player.summoner_name)
            || data
                .active_player
                .riot_id
                .as_deref()
                .zip(player.riot_id.as_deref())
                .is_some_and(|(active, candidate)| active == candidate)
    });
    if let Some(player) = local_player {
        if summary.local_player_champion.is_none() {
            summary.local_player_champion = Some(player.champion_name.clone());
        }
        if summary.local_player_team.is_none() {
            summary.local_player_team = Some(player.team.clone());
        }
    }
}

fn canonical_spell_id(spell: &RawSpell) -> Option<String> {
    let marker = "SummonerSpell_";
    spell
        .raw_display_name
        .split_once(marker)
        .and_then(|(_, suffix)| suffix.split_once("_DisplayName"))
        .map(|(identifier, _)| identifier.to_owned())
        .or_else(|| (!spell.display_name.is_empty()).then(|| spell.display_name.clone()))
}

fn string_field(fields: &Map<String, Value>, name: &str) -> Option<String> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn string_list_field(fields: &Map<String, Value>, name: &str) -> Option<Vec<String>> {
    let values = fields.get(name)?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

fn bool_field(fields: &Map<String, Value>, name: &str) -> Option<bool> {
    let value = fields.get(name)?;
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|text| text.parse::<bool>().ok()))
}

fn u32_field(fields: &Map<String, Value>, name: &str) -> Option<u32> {
    fields
        .get(name)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
}

fn valid_game_time(data: &RawAllGameData) -> Option<f64> {
    data.game_data
        .game_time
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn seconds_to_ms(seconds: f64) -> Result<i64> {
    if !seconds.is_finite() || seconds < 0.0 || seconds > i64::MAX as f64 / 1000.0 {
        bail!("invalid game time {seconds}");
    }
    Ok((seconds * 1000.0).round() as i64)
}

fn median(values: &mut [i64]) -> Result<i64> {
    if values.is_empty() {
        bail!("cannot calculate a median from zero samples");
    }
    values.sort_unstable();
    Ok(values[values.len() / 2])
}

fn round_number(value: f64) -> i64 {
    value.round() as i64
}

async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    if *receiver.borrow() {
        return;
    }
    let _ = receiver.changed().await;
}

async fn sleep_or_cancel(duration: Duration, receiver: &mut watch::Receiver<bool>) {
    tokio::select! {
        _ = tokio::time::sleep(duration) => {}
        _ = cancelled(receiver) => {}
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawAllGameData {
    #[serde(rename = "activePlayer", default)]
    active_player: RawActivePlayer,
    #[serde(rename = "allPlayers", default)]
    all_players: Vec<RawPlayer>,
    #[serde(rename = "gameData", default)]
    game_data: RawGameData,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawActivePlayer {
    #[serde(rename = "currentGold")]
    current_gold: Option<f64>,
    #[serde(rename = "summonerName")]
    summoner_name: Option<String>,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championStats")]
    champion_stats: Option<RawChampionStats>,
    #[serde(rename = "fullRunes", default)]
    full_runes: RawFullRunes,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawChampionStats {
    #[serde(rename = "currentHealth")]
    current_health: Option<f64>,
    #[serde(rename = "maxHealth")]
    max_health: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawFullRunes {
    #[serde(rename = "generalRunes", default)]
    general_runes: Vec<RawRune>,
    #[serde(rename = "statRunes", default)]
    stat_runes: Vec<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPlayer {
    #[serde(rename = "summonerName", default)]
    summoner_name: String,
    #[serde(rename = "riotId")]
    riot_id: Option<String>,
    #[serde(rename = "championName", default)]
    champion_name: String,
    #[serde(default)]
    team: String,
    #[serde(default)]
    level: u32,
    #[serde(default, deserialize_with = "deserialize_items")]
    items: Vec<RawItem>,
    #[serde(default)]
    scores: RawScores,
    #[serde(default)]
    runes: RawRunes,
    #[serde(rename = "summonerSpells", default)]
    summoner_spells: RawSummonerSpells,
}

#[derive(Debug, Clone, Deserialize)]
struct RawItem {
    #[serde(rename = "itemID", default)]
    item_id: u32,
    #[serde(default)]
    slot: u32,
    #[serde(default)]
    count: u32,
}

fn deserialize_items<'de, D>(deserializer: D) -> std::result::Result<Vec<RawItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Array(items) => {
            serde_json::from_value(Value::Array(items)).map_err(serde::de::Error::custom)
        }
        Value::Object(error) if error.contains_key("error") => Ok(Vec::new()),
        other => Err(serde::de::Error::custom(format!(
            "expected an item array or API error object, got {other}"
        ))),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawScores {
    #[serde(rename = "creepScore", default)]
    creep_score: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawRunes {
    keystone: Option<RawRune>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRune {
    id: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawSummonerSpells {
    #[serde(rename = "summonerSpellOne")]
    spell_one: Option<RawSpell>,
    #[serde(rename = "summonerSpellTwo")]
    spell_two: Option<RawSpell>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSpell {
    #[serde(rename = "displayName", default)]
    display_name: String,
    #[serde(rename = "rawDisplayName", default)]
    raw_display_name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawGameData {
    #[serde(rename = "gameTime")]
    game_time: Option<f64>,
    #[serde(rename = "gameMode")]
    game_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawEventData {
    #[serde(rename = "Events", default)]
    events: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(rename = "EventID")]
    event_id: i64,
    #[serde(rename = "EventName")]
    event_name: String,
    #[serde(rename = "EventTime")]
    event_time: f64,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    {
      "activePlayer": {
        "currentGold": 578.24,
        "summonerName": "Local#EUW",
        "riotId": "Local#EUW",
        "championStats": { "currentHealth": 901.6, "maxHealth": 1089.6 },
        "fullRunes": {
          "generalRunes": [{"id": 8369}, {"id": 8321}],
          "statRunes": [{"id": 5005}, {"id": 5008}, {"id": 5011}]
        }
      },
      "allPlayers": [
        {
          "summonerName": "Local#EUW",
          "riotId": "Local#EUW",
          "championName": "LeBlanc",
          "team": "ORDER",
          "level": 6,
          "items": [{"itemID": 3340, "slot": 6, "count": 1}],
          "scores": {"creepScore": 42},
          "runes": {"keystone": {"id": 8369}},
          "summonerSpells": {
            "summonerSpellOne": {
              "displayName": "Flash",
              "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerFlash_DisplayName"
            },
            "summonerSpellTwo": {
              "displayName": "Ignite",
              "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerDot_DisplayName"
            }
          }
        },
        {
          "summonerName": "Other#EUW",
          "riotId": "Other#EUW",
          "championName": "Jinx",
          "team": "CHAOS",
          "level": 5,
          "items": [],
          "scores": {"creepScore": 38},
          "runes": {"keystone": {"id": 8005}},
          "summonerSpells": {}
        }
      ],
      "gameData": {"gameMode": "CLASSIC", "gameTime": 220.125}
    }
    "#;

    #[test]
    fn snapshot_uses_private_stats_only_for_the_active_player() {
        let raw: RawAllGameData = serde_json::from_str(SAMPLE).unwrap();
        let snapshot = make_snapshot(&raw, true).unwrap();

        assert_eq!(snapshot.game_time_ms, 220_125);
        assert_eq!(snapshot.players.len(), 2);
        assert_eq!(snapshot.players[0].gold, Some(578));
        assert_eq!(snapshot.players[0].hp, Some(902));
        assert_eq!(snapshot.players[0].hp_max, Some(1_090));
        assert_eq!(
            snapshot.players[0].rune_ids,
            Some(vec![8369, 8321, 5005, 5008, 5011])
        );
        assert_eq!(
            snapshot.players[0].summoner_spells,
            Some(vec!["SummonerFlash".to_owned(), "SummonerDot".to_owned()])
        );
        assert_eq!(snapshot.players[1].gold, None);
        assert_eq!(snapshot.players[1].hp, None);
        assert_eq!(snapshot.players[1].hp_max, None);
        assert_eq!(snapshot.players[1].rune_ids, None);
    }

    #[test]
    fn stable_fields_are_omitted_after_the_first_snapshot() {
        let raw: RawAllGameData = serde_json::from_str(SAMPLE).unwrap();
        let snapshot = make_snapshot(&raw, false).unwrap();
        assert!(snapshot.players.iter().all(|player| {
            player.summoner_spells.is_none()
                && player.keystone_id.is_none()
                && player.rune_ids.is_none()
        }));
    }

    #[test]
    fn snapshot_diff_detects_item_and_level_changes() {
        let raw: RawAllGameData = serde_json::from_str(SAMPLE).unwrap();
        let previous = make_snapshot(&raw, true).unwrap();
        let mut current = previous.clone();
        current.game_time_ms = 230_000;
        current.players[0].level = 7;
        current.players[0].items = vec![ItemSnapshot {
            item_id: 3006,
            slot: 0,
            count: 1,
        }];

        let changes = diff_snapshots(&previous, &current, 142_300);
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::ItemPurchased && change.item_id == Some(3006)
        }));
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::ItemSold && change.item_id == Some(3340)
        }));
        assert!(changes.iter().any(|change| {
            change.change_type == SnapshotChangeType::LevelUp && change.new_level == Some(7)
        }));
        assert!(changes.iter().all(|change| change.video_time_ms == 372_300));
    }

    #[test]
    fn event_is_normalized_and_time_synced() {
        let raw: RawEvent = serde_json::from_str(
            r#"{
              "EventID": 17,
              "EventName": "ChampionKill",
              "EventTime": 214.5,
              "KillerName": "Player3",
              "VictimName": "Player7",
              "Assisters": ["Player1"]
            }"#,
        )
        .unwrap();

        let event = normalize_event(raw, 142_300).unwrap();
        assert_eq!(event.event_type, "ChampionKill");
        assert_eq!(event.game_time_ms, 214_500);
        assert_eq!(event.video_time_ms, 356_800);
        assert_eq!(event.killer.as_deref(), Some("Player3"));
        assert_eq!(event.victim.as_deref(), Some("Player7"));
    }

    #[test]
    fn calibration_uses_the_middle_sample() {
        let mut candidates = [142_303, 142_301, 180_000, 142_302, 142_300];
        assert_eq!(median(&mut candidates).unwrap(), 142_302);
    }

    #[test]
    fn metadata_keeps_unavailable_live_client_fields_null() {
        let metadata = RecordingMetadata::new(
            SystemTime::UNIX_EPOCH,
            Duration::from_millis(12_345),
            PollerSummary::default(),
            "nvenc".to_owned(),
            "1920x1080".to_owned(),
            60,
        )
        .unwrap();
        let json = serde_json::to_value(metadata).unwrap();

        assert_eq!(json["recorded_at"], "1970-01-01T00:00:00Z");
        assert_eq!(json["duration_ms"], 12_345);
        assert!(json["game_mode"].is_null());
        assert!(json["video_offset_ms"].is_null());
        assert_eq!(json["win_method"], "unknown");
    }

    #[test]
    fn empirical_capture_remains_parseable() {
        #[derive(Deserialize)]
        struct Capture {
            samples: Vec<CaptureSample>,
        }

        #[derive(Deserialize)]
        struct CaptureSample {
            data: Value,
        }

        let capture: Capture = serde_json::from_str(include_str!(
            "../../live-client-capture-20260730-110603.json"
        ))
        .unwrap();
        assert_eq!(capture.samples.len(), 25);
        let mut event_count = 0;
        for sample in capture.samples {
            let all_game: RawAllGameData = serde_json::from_value(sample.data.clone()).unwrap();
            make_snapshot(&all_game, true).unwrap();

            let events: RawEventData =
                serde_json::from_value(sample.data["events"].clone()).unwrap();
            for event in events.events {
                normalize_event(event, 0).unwrap();
                event_count += 1;
            }
        }
        assert!(event_count > 0);
    }
}
