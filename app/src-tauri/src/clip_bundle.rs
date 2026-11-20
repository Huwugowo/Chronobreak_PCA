//! Durable metadata for one exported MP4.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chronobreak_league_data::{MAX_COUNTER, Outcome, ResultFile, observation::Event};
use chronobreak_replay_time::{ClipRange, MediaId, ReplayTick};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::clip_export::ClipExportPreset;
use crate::directory_guard::{DirectoryGuard, open_regular, reject_reparse};
use crate::playback_file::{FileFacts, file_facts};

pub(crate) const HEADER: &str = "clip.json";
pub(crate) const EVENTS: &str = "events.json";
pub(crate) const THUMBNAIL: &str = "thumbnail.jpg";
pub(crate) const HEADER_LIMIT: u64 = 64 * 1024;
pub(crate) const EVENTS_LIMIT: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_EVENTS: usize = 8_192;
const MAX_TITLE_UNITS: usize = 120;
const MAX_TAGS: usize = 32;
pub(crate) const VERSION: u8 = 2;

pub(crate) type Inventory = HashMap<ClipId, ClipDescriptor>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipDescriptor {
    pub id: ClipId,
    pub directory: PathBuf,
    pub generation: String,
    pub video_leaf: String,
    pub video: FileFacts,
    pub thumbnail: FileFacts,
    pub header: FileFacts,
    pub events: FileFacts,
}

impl ClipDescriptor {
    pub(crate) fn inspect(collection: &DirectoryGuard, id: ClipId) -> Result<Self> {
        let directory = collection.child(id.as_str(), false)?;
        let mut video_leaf = None;
        for entry in fs::read_dir(&directory.path)? {
            let entry = entry?;
            let leaf = entry.file_name();
            let leaf = leaf.to_str().context("invalid clip filename")?;
            let metadata = entry.metadata()?;
            reject_reparse(&metadata)?;
            ensure!(metadata.is_file(), "unexpected clip directory entry");
            if matches!(leaf, HEADER | EVENTS | THUMBNAIL) {
                continue;
            }
            ensure!(
                title_from_leaf(leaf).is_some() && video_leaf.is_none(),
                "unexpected clip file"
            );
            video_leaf = Some(leaf.to_owned());
        }
        let video_leaf = video_leaf.context("clip video missing")?;
        let observe = |name| file_facts(&open_regular(&directory.path.join(name), false, false)?);
        let video = observe(video_leaf.as_str())?;
        let thumbnail = observe(THUMBNAIL)?;
        let header = observe(HEADER)?;
        let events = observe(EVENTS)?;
        ensure!(
            video.length() > 0
                && thumbnail.length() > 0
                && (1..=HEADER_LIMIT).contains(&header.length())
                && (1..=EVENTS_LIMIT).contains(&events.length()),
            "invalid clip asset size"
        );
        Ok(Self {
            id,
            generation: directory.generation()?,
            directory: directory.path,
            video_leaf,
            video,
            thumbnail,
            header,
            events,
        })
    }

    pub(crate) fn title(&self) -> &str {
        self.video_leaf
            .strip_suffix(".mp4")
            .expect("validated clip filename")
    }

    pub(crate) fn stamp(&self) -> String {
        format!(
            "{}:{}:{}",
            self.generation,
            self.video_leaf,
            self.header.stamp()
        )
    }

    pub(crate) fn read_header(&self) -> Result<ClipHeader> {
        let header: ClipHeader = read_json(
            &self.directory.join(HEADER),
            HEADER_LIMIT,
            Some(&self.header),
        )?;
        header.validate(&self.id)?;
        Ok(header)
    }

    pub(crate) fn read_events(&self) -> Result<ClipEvents> {
        let events: ClipEvents = read_json(
            &self.directory.join(EVENTS),
            EVENTS_LIMIT,
            Some(&self.events),
        )?;
        ensure!(events.events.len() <= MAX_EVENTS, "too many clip events");
        Ok(events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ClipId(MediaId);

impl ClipId {
    pub(crate) fn new() -> Self {
        Self(MediaId::new_v4())
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        Ok(Self(MediaId::parse(value)?))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipGame {
    pub match_group: Option<String>,
    pub played_at: Option<String>,
    pub part_recorded_at: Option<String>,
    pub champion: Option<String>,
    pub account: Option<String>,
    pub queue_id: Option<u32>,
    pub mode: Option<String>,
    pub outcome: Option<Outcome>,
    pub final_kda: Option<[Option<u64>; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summoner_spells: Option<[Option<String>; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_items: Option<Vec<Option<u64>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
}

impl ClipGame {
    /// Adds the compact display facts from a validated final result.
    pub(crate) fn apply_final_result(&mut self, result: &ResultFile) {
        if result.outcome != Outcome::Unknown {
            self.outcome = Some(result.outcome);
        }
        if let Some(player) = &result.local_player {
            if let Some(champion) = display_label(player.champion_name.as_deref()) {
                self.champion = Some(champion);
            }
            let saved_kda = self.final_kda.unwrap_or_default();
            let kda = [
                player.stats.kills.or(saved_kda[0]),
                player.stats.deaths.or(saved_kda[1]),
                player.stats.assists.or(saved_kda[2]),
            ];
            self.final_kda = kda.iter().any(Option::is_some).then_some(kda);
            let spells = [player.spell1_id, player.spell2_id].map(|id| {
                id.filter(|id| *id > 0 && u32::try_from(*id).is_ok())
                    .map(|id| id.to_string())
            });
            self.fill_spells(spells);
        }
        if let Some(items) = result
            .source_facts
            .local_player
            .as_ref()
            .and_then(|player| player.items.as_ref())
        {
            self.final_items = Some(items.clone());
        }
        if let Some(seconds) = result.duration_seconds.filter(|seconds| *seconds > 0) {
            self.duration_seconds = Some(seconds);
        }
    }

    pub(crate) fn fill_spells(&mut self, [one, two]: [Option<String>; 2]) {
        let [saved_one, saved_two] = self.summoner_spells.take().unwrap_or_default();
        let spells = [
            one.as_deref()
                .and_then(|s| display_label(Some(s)))
                .or(saved_one),
            two.as_deref()
                .and_then(|s| display_label(Some(s)))
                .or(saved_two),
        ];
        self.summoner_spells = spells.iter().any(Option::is_some).then_some(spells);
    }
}

pub(crate) fn display_label(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Attribution {
    You,
    Ally,
    Enemy,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HighlightKind {
    DoubleKill,
    TripleKill,
    QuadraKill,
    Pentakill,
    FirstBlood,
    DragonSteal,
    BaronSteal,
    HeraldSteal,
}

impl HighlightKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DoubleKill => "Double Kill",
            Self::TripleKill => "Triple Kill",
            Self::QuadraKill => "Quadra Kill",
            Self::Pentakill => "Pentakill",
            Self::FirstBlood => "First Blood",
            Self::DragonSteal => "Dragon Steal",
            Self::BaronSteal => "Baron Steal",
            Self::HeraldSteal => "Herald Steal",
        }
    }
}

pub(crate) fn normalize_predefined_tags(
    tags: impl IntoIterator<Item = HighlightKind>,
) -> Vec<HighlightKind> {
    let mut seen = HashSet::new();
    tags.into_iter().filter(|tag| seen.insert(*tag)).collect()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventAvailability {
    Available,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipEvent {
    pub replay_tick: ReplayTick,
    pub event: Event,
    pub attribution: Attribution,
    pub actor: Option<String>,
}

impl ClipEvent {
    fn highlight_kind(&self) -> Option<HighlightKind> {
        use HighlightKind::*;
        Some(match self.event.name.as_deref()? {
            "Multikill" => match self.event.kill_streak? {
                2 => DoubleKill,
                3 => TripleKill,
                4 => QuadraKill,
                5 => Pentakill,
                _ => return None,
            },
            "FirstBlood" => FirstBlood,
            "DragonKill" if self.event.stolen == Some(true) => DragonSteal,
            "BaronKill" if self.event.stolen == Some(true) => BaronSteal,
            "HeraldKill" if self.event.stolen == Some(true) => HeraldSteal,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipEvents {
    pub availability: EventAvailability,
    pub events: Vec<ClipEvent>,
}

impl ClipEvents {
    pub(crate) fn unavailable() -> Self {
        Self {
            availability: EventAvailability::Unavailable,
            events: Vec::new(),
        }
    }

    pub(crate) fn detected_tags(&self) -> Vec<HighlightKind> {
        normalize_predefined_tags(
            self.events
                .iter()
                .filter(|event| event.attribution == Attribution::You)
                .filter_map(ClipEvent::highlight_kind),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipHeader {
    pub version: u8,
    pub id: ClipId,
    pub range: ClipRange,
    pub source_timestamp: String,
    pub game: ClipGame,
    pub exported_at: u64,
    pub preset: ClipExportPreset,
    pub duration: ReplayTick,
    pub events: EventAvailability,
    pub predefined_tags: Vec<HighlightKind>,
    pub custom_tags: Vec<String>,
}

impl ClipHeader {
    pub(crate) fn validate(&self, id: &ClipId) -> Result<()> {
        ensure!(
            self.version == VERSION
                && &self.id == id
                && crate::library::valid_game_id(&self.source_timestamp)
                && self.duration.get() > 0,
            "invalid clip identity or range"
        );
        for date in [&self.game.played_at, &self.game.part_recorded_at]
            .into_iter()
            .flatten()
        {
            OffsetDateTime::parse(date, &Rfc3339).context("invalid clip game date")?;
        }
        ensure!(
            self.game.match_group.as_ref().is_none_or(|value| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }) && normalize_predefined_tags(self.predefined_tags.iter().copied())
                == self.predefined_tags
                && normalize_tags(&self.custom_tags)? == self.custom_tags
                && self
                    .game
                    .final_kda
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|v| *v <= MAX_COUNTER)
                && self
                    .game
                    .summoner_spells
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|s| display_label(Some(s)).as_ref() == Some(s))
                && self.game.final_items.as_ref().is_none_or(|items| {
                    items.len() <= 8 && items.iter().flatten().all(|v| *v <= MAX_COUNTER)
                })
                && self
                    .game
                    .duration_seconds
                    .is_none_or(|seconds| (1..=MAX_COUNTER).contains(&seconds)),
            "invalid clip game or tag data"
        );
        Ok(())
    }
}

pub(crate) fn media_leaf(title: &str) -> Result<String> {
    ensure!(
        !title.is_empty()
            && title == title.trim()
            && !title.ends_with('.')
            && !title.to_ascii_lowercase().ends_with(".mp4")
            && title.encode_utf16().count() <= MAX_TITLE_UNITS
            && !title
                .chars()
                .any(|character| { character.is_control() || "<>:\"/\\|?*".contains(character) }),
        "choose a title without a file extension or Windows filename characters"
    );
    let first = title.split('.').next().unwrap_or(title).trim_end();
    let device = first.to_ascii_uppercase();
    ensure!(
        !matches!(
            device.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) && !["COM", "LPT"].into_iter().any(|prefix| {
            device.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        }),
        "choose a title that is not a reserved Windows filename"
    );
    Ok(format!("{title}.mp4"))
}

pub(crate) fn title_from_leaf(leaf: &str) -> Option<&str> {
    let title = leaf.strip_suffix(".mp4")?;
    media_leaf(title).ok()?;
    Some(title)
}

pub(crate) fn normalize_tags(tags: &[String]) -> Result<Vec<String>> {
    ensure!(tags.len() <= MAX_TAGS, "too many custom tags");
    let mut seen = HashSet::with_capacity(tags.len());
    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim();
        ensure!(
            !tag.is_empty() && tag.chars().count() <= 40 && !tag.chars().any(char::is_control),
            "custom tags must contain 1 to 40 printable characters"
        );
        if seen.insert(tag.to_lowercase()) {
            normalized.push(tag.to_owned());
        }
    }
    Ok(normalized)
}

fn read_json<T: DeserializeOwned>(
    path: &Path,
    limit: u64,
    expected: Option<&FileFacts>,
) -> Result<T> {
    let file = open_regular(path, false, false)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "clip metadata exceeds its limit"
    );
    if let Some(expected) = expected {
        ensure!(file_facts(&file)? == *expected, "clip metadata changed");
    }
    let mut bytes = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "clip metadata exceeds its limit"
    );
    if let Some(expected) = expected {
        ensure!(file_facts(&file)? == *expected, "clip metadata changed");
    }
    serde_json::from_slice(&bytes).context("invalid clip metadata")
}

pub(crate) fn write_new_json(path: &Path, value: &impl Serialize, limit: u64) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "clip metadata exceeds its limit"
    );
    let mut file = File::create_new(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronobreak_replay_time::FrameBoundary;

    #[test]
    fn clip_titles_are_single_windows_filename_stems() {
        assert_eq!(
            media_leaf("Jinx – a comeback").unwrap(),
            "Jinx – a comeback.mp4"
        );
        for invalid in [
            "",
            "NUL",
            "COM¹.txt",
            "AUX .txt",
            "trailing.",
            "trailing ",
            "../escape",
            "a:b",
            "x.mp4",
        ] {
            assert!(media_leaf(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn custom_tags_are_case_insensitive_with_first_display_spelling() {
        assert_eq!(
            normalize_tags(&[" Nice escape ".into(), "nice ESCAPE".into(), "Baron".into()])
                .unwrap(),
            ["Nice escape", "Baron"]
        );
    }

    #[test]
    fn saves_predefined_tags_without_an_event_file() {
        let mut header = ClipHeader {
            version: VERSION,
            id: ClipId::new(),
            range: ClipRange::new(
                MediaId::new_v4(),
                FrameBoundary::new(0).unwrap(),
                FrameBoundary::new(30).unwrap(),
                FrameBoundary::new(30).unwrap(),
            )
            .unwrap(),
            source_timestamp: "100".into(),
            game: ClipGame::default(),
            exported_at: 10,
            preset: ClipExportPreset::Horizontal,
            duration: ReplayTick::new(48_000_000).unwrap(),
            events: EventAvailability::Unavailable,
            predefined_tags: Vec::new(),
            custom_tags: vec!["River fight".into()],
        };
        for selection in [vec![HighlightKind::BaronSteal], Vec::new()] {
            header.predefined_tags = selection.clone();
            let restored: ClipHeader =
                serde_json::from_slice(&serde_json::to_vec(&header).unwrap()).unwrap();
            restored.validate(&restored.id).unwrap();
            assert_eq!(restored.predefined_tags, selection);
        }
    }
}
