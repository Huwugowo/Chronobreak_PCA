//! Disposable projection of accepted clip headers. Event payloads stay on disk.
use super::*;
use crate::clip_bundle::{ClipDescriptor, ClipHeader, ClipId, HighlightKind};
use crate::library::LibrarySnapshot;
use chronobreak_replay_time::{MediaTimelineV2, RoundingMode, frame_boundary_to_replay_tick};
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const BATCH: usize = 64;
const GROUPS: usize = 8;
const INITIAL: usize = 4;
const MORE: usize = 24;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS clip_projection_state(
    singleton INTEGER PRIMARY KEY CHECK(singleton=1), token TEXT, phase INTEGER, after_id TEXT
);
CREATE TABLE IF NOT EXISTS clip_projection(
    id TEXT PRIMARY KEY, stamp TEXT NOT NULL, match_group TEXT, group_key TEXT NOT NULL,
    played INTEGER NOT NULL, part INTEGER NOT NULL, frame INTEGER NOT NULL,
    champion TEXT NOT NULL, day TEXT NOT NULL, queue TEXT NOT NULL, outcome TEXT NOT NULL,
    search TEXT NOT NULL, highlights TEXT NOT NULL, tags TEXT NOT NULL, header TEXT
);
CREATE INDEX IF NOT EXISTS clip_projection_group ON clip_projection(group_key,part,frame,id);
CREATE INDEX IF NOT EXISTS clip_projection_match ON clip_projection(match_group) WHERE header IS NOT NULL;
CREATE TABLE IF NOT EXISTS clip_tag_projection(
    clip_id TEXT NOT NULL REFERENCES clip_projection(id) ON DELETE CASCADE,
    tag_key TEXT NOT NULL, label TEXT NOT NULL, PRIMARY KEY(clip_id,tag_key)
);
CREATE INDEX IF NOT EXISTS clip_tag_prefix ON clip_tag_projection(tag_key,label);
";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipQuery {
    pub search: Option<String>,
    pub champion: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub queue: Option<String>,
    pub outcome: Option<String>,
    pub highlight: Option<String>,
    pub actor: Option<String>,
    pub match_group: Option<String>,
    pub include_game_clock: Option<bool>,
    pub after: Option<(i64, String)>,
    pub group_key: Option<String>,
    pub clip_after: Option<(i64, i64, ClipId)>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipCard {
    pub header: ClipHeader,
    pub title: String,
    pub video_url: String,
    pub thumbnail_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_clock: Option<GameTick>,
}

impl ClipCard {
    pub(crate) fn new(
        clip: &ClipDescriptor,
        header: ClipHeader,
        origin: &str,
        capability: &str,
    ) -> Self {
        let url = format!("{origin}/clips/{}", clip.id.as_str());
        Self {
            header,
            title: clip.title().into(),
            video_url: format!("{url}/video?g={capability}"),
            thumbnail_url: format!("{url}/thumbnail?g={capability}"),
            game_clock: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipGroup {
    pub key: String,
    pub clips: Vec<ClipCard>,
    pub next: Option<(i64, i64, ClipId)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_facts: Option<ClipMatchFacts>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipMatchFacts {
    pub champion: String,
    pub outcome: Option<chronobreak_league_data::Outcome>,
    pub summoner_spells: Option<[Option<String>; 2]>,
    pub final_kda: Option<[Option<u64>; 3]>,
    pub final_items: Option<Vec<Option<u64>>>,
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipPage {
    pub groups: Vec<ClipGroup>,
    pub next: Option<(i64, String)>,
}

fn date_value(date: Option<&str>) -> Option<i64> {
    OffsetDateTime::parse(date?, &Rfc3339)
        .ok()
        .map(|d| d.unix_timestamp())
}

impl HighlightKind {
    pub(crate) fn search_label(self) -> &'static str {
        match self {
            Self::DoubleKill => "double kill",
            Self::TripleKill => "triple kill",
            Self::QuadraKill => "quadra kill",
            Self::Pentakill => "pentakill",
            Self::FirstBlood => "first blood",
            Self::DragonSteal => "dragon steal",
            Self::BaronSteal => "baron steal",
            Self::HeraldSteal => "herald steal",
        }
    }

    pub(crate) fn tier(self) -> u8 {
        match self {
            Self::DoubleKill => 2,
            Self::TripleKill => 3,
            Self::QuadraKill => 4,
            Self::Pentakill => 5,
            _ => 0,
        }
    }
}

fn minimum_tier(value: &str) -> u8 {
    match value {
        "double kill" | "doublekill" => 2,
        "triple kill" | "triplekill" => 3,
        "quadra kill" | "quadrakill" => 4,
        "penta kill" | "pentakill" => 5,
        _ => 0,
    }
}

impl Index {
    pub(crate) fn clip_ready(&self, inventory_stamp: &str) -> bool {
        self.db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM clip_projection_state WHERE token=?1 AND phase=2)",
                [inventory_stamp],
                |r| r.get(0),
            )
            .unwrap_or(false)
    }

    /// Every invocation owns at most one batch of header reads or stale removals.
    pub(crate) fn reconcile_clips(
        &mut self,
        snapshot: &LibrarySnapshot,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<bool> {
        if self.clip_ready(&snapshot.managed_clip_stamp) {
            return Ok(true);
        }
        self.db.execute_batch(SCHEMA)?;
        let (phase, after): (i64, String) = self
            .db
            .query_row(
                "SELECT phase,after_id FROM clip_projection_state WHERE token=?1",
                [&snapshot.managed_clip_stamp],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .unwrap_or_default();
        let mut end = String::new();
        let next_phase;
        let tx = self.db.transaction()?;
        if phase == 0 {
            let ids = &snapshot.managed_clip_order;
            let start = ids.partition_point(|id| id.as_str() <= after.as_str());
            let batch = ids.iter().skip(start).take(BATCH);
            for id in batch {
                check()?;
                let descriptor = &snapshot.managed_clips[id];
                let stamp = descriptor.stamp();
                let unchanged: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM clip_projection WHERE id=?1 AND stamp=?2)",
                    params![id.as_str(), stamp],
                    |r| r.get(0),
                )?;
                if !unchanged {
                    project_clip(&tx, descriptor, &stamp)?;
                }
                end = id.as_str().to_owned();
            }
            next_phase = i64::from(start + BATCH >= ids.len());
        } else {
            let candidates = {
                let mut query =
                    tx.prepare("SELECT id FROM clip_projection WHERE id>?1 ORDER BY id LIMIT ?2")?;
                query
                    .query_map(params![after, BATCH as i64], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            for id in &candidates {
                if !ClipId::parse(id).is_ok_and(|id| snapshot.managed_clips.contains_key(&id)) {
                    tx.execute("DELETE FROM clip_projection WHERE id=?1", [id])?;
                }
                end.clone_from(id);
            }
            next_phase = if candidates.len() < BATCH { 2 } else { 1 };
        }
        if phase != next_phase {
            end.clear();
        }
        tx.execute(
            "INSERT OR REPLACE INTO clip_projection_state VALUES(1,?1,?2,?3)",
            params![snapshot.managed_clip_stamp, next_phase, end],
        )?;
        check()?;
        tx.commit()?;
        Ok(next_phase == 2)
    }

    pub(crate) fn attach_clip_counts(
        &self,
        snapshot: &LibrarySnapshot,
        page: &mut HistoryPage,
    ) -> Result<()> {
        if page.rows.is_empty() {
            return Ok(());
        }
        if snapshot.managed_clips.is_empty() {
            for row in &mut page.rows {
                row.clip_count = Some(0);
            }
            return Ok(());
        }
        if !self.clip_ready(&snapshot.managed_clip_stamp) {
            return Ok(());
        }
        let keys = page
            .rows
            .iter()
            .filter_map(|row| row.game.match_group.as_deref())
            .collect::<Vec<_>>();
        let mut query = self.db.prepare_cached(
            "SELECT g.value,(SELECT count(*) FROM clip_projection c
             WHERE c.match_group=g.value AND c.header IS NOT NULL) FROM json_each(?1) g",
        )?;
        let counts = query
            .query_map([serde_json::to_string(&keys)?], |row| {
                Ok((row.get::<_, String>(0)?, unsigned(row, 1)?))
            })?
            .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;
        for row in &mut page.rows {
            row.clip_count = row
                .game
                .match_group
                .as_deref()
                .map(|key| counts.get(key).copied().unwrap_or(0));
        }
        Ok(())
    }

    pub(crate) fn clip_tags(&self, inventory_stamp: &str, prefix: &str) -> Result<Vec<String>> {
        if !self.clip_ready(inventory_stamp) {
            return Ok(Vec::new());
        }
        let prefix = prefix.to_lowercase();
        let mut query = self.db.prepare(
            "SELECT min(label) FROM clip_tag_projection WHERE tag_key>=?1 AND tag_key<?1||X'FF'
             GROUP BY tag_key ORDER BY tag_key LIMIT 12",
        )?;
        Ok(query
            .query_map([prefix], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn clips(
        &self,
        snapshot: &LibrarySnapshot,
        query: &ClipQuery,
        origin: &str,
        capability: &str,
    ) -> Result<ClipPage> {
        ensure!(
            self.clip_ready(&snapshot.managed_clip_stamp),
            "clip catalog is pending"
        );
        let filter = ClipFilter::new(query)?;
        if let Some(key) = &query.group_key {
            let group = self.clip_group(
                snapshot,
                &filter,
                key,
                query.clip_after.as_ref(),
                MORE,
                origin,
                capability,
            )?;
            return Ok(ClipPage {
                groups: vec![group],
                next: None,
            });
        }
        let groups_sql = format!(
            "{CLIP_FILTER}
            SELECT group_key,max(played) recent FROM filtered GROUP BY group_key
            HAVING ?11 IS NULL OR recent<?11 OR (recent=?11 AND group_key>?12)
            ORDER BY recent DESC,group_key LIMIT ?13"
        );
        let mut statement = self.db.prepare(&groups_sql)?;
        let after_time = query.after.as_ref().map(|(time, _)| *time);
        let after_key = query.after.as_ref().map(|(_, key)| key);
        let cap = (GROUPS + 1) as i64;
        let extra: [&dyn rusqlite::ToSql; 3] = [&after_time, &after_key, &cap];
        let args = filter.args().chain(extra);
        let keys = statement
            .query_map(rusqlite::params_from_iter(args), |row| {
                Ok((row.get::<_, i64>(1)?, row.get::<_, String>(0)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let next = keys.get(GROUPS).map(|_| keys[GROUPS - 1].clone());
        let mut groups = Vec::new();
        for (_, key) in keys.into_iter().take(GROUPS) {
            groups
                .push(self.clip_group(snapshot, &filter, &key, None, INITIAL, origin, capability)?);
        }
        if query.match_group.is_none() && self.require_catalog(&snapshot.catalog_key).is_ok() {
            self.attach_clip_match_facts(&mut groups)?;
        }
        Ok(ClipPage { groups, next })
    }

    fn attach_clip_match_facts(&self, groups: &mut [ClipGroup]) -> Result<()> {
        let keys: Vec<_> = groups
            .iter()
            .map(|group| group.key.strip_prefix("game:"))
            .collect();
        if keys.iter().all(Option::is_none) {
            return Ok(());
        }
        // Use the same checked part priority as History. Parts without final
        // facts or a local roster have no extra stats to add to the clip header.
        let mut statement = self.db.prepare_cached(
            "SELECT g.key,r.media_id,r.champion,r.result_json
             FROM json_each(?1) g CROSS JOIN recordings r ON r.media_id=(
                 SELECT part.media_id FROM recordings part
                 WHERE part.match_group=g.value AND part.facts_current=1
                   AND (part.result_json IS NOT NULL OR (part.telemetry_status='ready' AND EXISTS(
                       SELECT 1 FROM scopes s JOIN roster_members m ON m.media_id=s.media_id
                           AND m.run_id=s.run_id AND m.epoch=s.epoch AND m.participant_id=s.local_participant
                       WHERE s.media_id=part.media_id
                   )))
                 ORDER BY part.result_json IS NULL,part.recorded_at DESC,part.media_id LIMIT 1
             )",
        )?;
        let mut rows = statement.query([serde_json::to_string(&keys)?])?;
        while let Some(row) = rows.next()? {
            let media = MediaId::parse(&row.get::<_, String>(1)?)?;
            let result = row
                .get::<_, Option<String>>(3)?
                .map(|json| serde_json::from_str::<ResultFile>(&json))
                .transpose()?;
            let stats = self.history_stats(&media, result.as_ref())?;
            let final_kda = [stats.kills, stats.deaths, stats.assists];
            let champion = result
                .as_ref()
                .and_then(|result| result.local_player.as_ref())
                .and_then(|player| player.champion_name.clone());
            let group = usize::try_from(row.get::<_, i64>(0)?)?;
            groups[group].match_facts = Some(ClipMatchFacts {
                champion: match champion {
                    Some(champion) => champion,
                    None => row.get(2)?,
                },
                outcome: result.as_ref().map(|result| result.outcome),
                summoner_spells: stats.summoner_spells,
                final_kda: (result.is_some() && final_kda.iter().any(Option::is_some))
                    .then_some(final_kda),
                final_items: if result.is_some() { stats.items } else { None },
                duration_seconds: stats.duration_seconds,
            });
        }
        Ok(())
    }

    pub(crate) fn add_clip_game_clocks(
        &self,
        root: &Path,
        snapshot: &LibrarySnapshot,
        page: &mut ClipPage,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let mut sources = HashMap::new();
        for clip in page.groups.iter_mut().flat_map(|group| &mut group.clips) {
            check()?;
            let header = &clip.header;
            let bundle = header.source_timestamp.as_str();
            let media = &header.range.media_id;
            let source = sources.entry((bundle, media)).or_insert_with(|| {
                snapshot
                    .games
                    .iter()
                    .any(|game| {
                        game.timestamp == bundle
                            && game.media_id.as_ref() == Some(media)
                            && game.video_available
                            && !game.incomplete
                    })
                    .then(|| self.clip_clock_source(root, bundle, media).ok())
                    .flatten()
            });
            if let Some((timeline, scope)) = source {
                clip.game_clock = self.clip_game_clock(header, timeline, scope).ok().flatten();
            }
        }
        Ok(())
    }

    fn clip_clock_source(
        &self,
        root: &Path,
        bundle: &str,
        media: &MediaId,
    ) -> Result<(MediaTimelineV2, SourceScope)> {
        self.require_source(root, bundle, media)?;
        let timeline_json: String = self.db.query_row(
            "SELECT timeline_json FROM recordings WHERE media_id=?1 AND bundle=?2",
            params![media.as_str(), bundle],
            |row| row.get(0),
        )?;
        let timeline = serde_json::from_str(&timeline_json)?;
        let scope = self
            .scopes(media)?
            .pop()
            .context("no recorded game clock")?;
        Ok((timeline, scope))
    }

    fn clip_game_clock(
        &self,
        header: &ClipHeader,
        timeline: &MediaTimelineV2,
        scope: &SourceScope,
    ) -> Result<Option<GameTick>> {
        header.range.validate_for(timeline)?;
        let start = frame_boundary_to_replay_tick(
            header.range.start_frame,
            timeline.video.frame_rate,
            RoundingMode::Exact,
        )?;
        let sample = self
            .db
            .prepare_cached(
                "SELECT game_tick FROM rosters
             WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND replay_tick BETWEEN 0 AND ?4
               AND game_tick IS NOT NULL
             ORDER BY replay_tick DESC,observation_id DESC LIMIT 1",
            )?
            .query_row(
                params![
                    timeline.media_id.as_str(),
                    scope.source.run_id.as_str(),
                    sql_u64(scope.source.epoch)?,
                    sql_u64(start.get())?,
                ],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        Ok(sample.map(GameTick::new).transpose()?)
    }

    #[allow(clippy::too_many_arguments)]
    fn clip_group(
        &self,
        snapshot: &LibrarySnapshot,
        filter: &ClipFilter<'_>,
        key: &str,
        after: Option<&(i64, i64, ClipId)>,
        limit: usize,
        origin: &str,
        capability: &str,
    ) -> Result<ClipGroup> {
        let sql = format!(
            "{CLIP_FILTER}
            SELECT id,part,frame,header FROM filtered WHERE group_key=?11
            AND (?12 IS NULL OR (part,frame,id)>(?12,?13,?14))
            ORDER BY part,frame,id LIMIT ?15"
        );
        let mut statement = self.db.prepare_cached(&sql)?;
        let part = after.map(|(part, _, _)| part);
        let frame = after.map(|(_, frame, _)| frame);
        let id = after.map(|(_, _, id)| id.as_str());
        let cap = (limit + 1) as i64;
        let extra: [&dyn rusqlite::ToSql; 5] = [&key, &part, &frame, &id, &cap];
        let args = filter.args().chain(extra);
        let mut rows = statement.query(rusqlite::params_from_iter(args))?;
        let mut clips = Vec::new();
        let mut last = None;
        while let Some(row) = rows.next()? {
            if clips.len() == limit {
                return Ok(ClipGroup {
                    key: key.into(),
                    clips,
                    next: last,
                    match_facts: None,
                });
            }
            let id = ClipId::parse(&row.get::<_, String>(0)?)?;
            let descriptor = snapshot
                .managed_clips
                .get(&id)
                .context("clip catalog changed")?;
            let header = serde_json::from_str(&row.get::<_, String>(3)?)?;
            clips.push(ClipCard::new(descriptor, header, origin, capability));
            last = Some((row.get(1)?, row.get(2)?, id));
        }
        Ok(ClipGroup {
            key: key.into(),
            clips,
            next: None,
            match_facts: None,
        })
    }
}

fn project_clip(db: &Connection, clip: &ClipDescriptor, stamp: &str) -> Result<()> {
    let Ok(header) = clip.read_header() else {
        db.execute("INSERT OR REPLACE INTO clip_projection VALUES(?1,?2,NULL,'',0,0,0,'','','','','','[]','[]',NULL)",
            params![clip.id.as_str(), stamp])?;
        return Ok(());
    };
    let game = &header.game;
    let key = game.match_group.as_ref().map_or_else(
        || format!("part:{}", header.range.media_id.as_str()),
        |id| format!("game:{id}"),
    );
    let day = game
        .played_at
        .as_deref()
        .or(game.part_recorded_at.as_deref())
        .and_then(|s| s.get(..10))
        .unwrap_or_default();
    let part = date_value(game.part_recorded_at.as_deref()).unwrap_or_default();
    let played = date_value(game.played_at.as_deref()).unwrap_or(part);
    let champion = game.champion.as_deref().unwrap_or_default();
    let mode = game.mode.as_deref().unwrap_or_default();
    let queue = game
        .queue_id
        .map_or_else(|| mode.to_owned(), |id| id.to_string());
    let outcome = game
        .outcome
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let kda = game
        .final_kda
        .map_or_else(String::new, |[k, d, a]| format!("{k}/{d}/{a}"));
    let search = format!(
        "{}\n{}\n{champion}\n{mode}\n{queue}\n{outcome}\n{day}\n{}\n{kda}",
        clip.title(),
        game.account.as_deref().unwrap_or_default(),
        header.custom_tags.join("\n")
    )
    .to_lowercase();
    let highlights = header
        .highlights
        .iter()
        .map(|tag| {
            serde_json::json!({
                "kind": tag.kind.search_label(), "tier": tag.kind.tier(), "actor": tag.attribution
            })
        })
        .collect::<Vec<_>>();
    db.execute(
        "INSERT OR REPLACE INTO clip_projection VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![clip.id.as_str(), stamp, game.match_group, key, played, part,
            sql_u64(header.range.start_frame.get())?, champion.to_lowercase(), day, queue.to_lowercase(), outcome,
            search, serde_json::to_string(&highlights)?, serde_json::to_string(&header.custom_tags)?, serde_json::to_string(&header)?],
    )?;
    let mut insert = db.prepare_cached("INSERT INTO clip_tag_projection VALUES(?1,?2,?3)")?;
    for label in &header.custom_tags {
        insert.execute(params![clip.id.as_str(), label.to_lowercase(), label])?;
    }
    Ok(())
}

const CLIP_FILTER: &str = "
WITH filtered AS (
 SELECT * FROM clip_projection WHERE header IS NOT NULL
 AND (?2 IS NULL OR champion=?2) AND (?3 IS NULL OR day>=?3 AND day<>'')
 AND (?4 IS NULL OR day<=?4 AND day<>'') AND (?5 IS NULL OR queue=?5 OR lower(json_extract(header,'$.game.mode'))=?5)
 AND (?6 IS NULL OR outcome=?6) AND (?9 IS NULL OR match_group=?9)
 AND (?1='' OR instr(search,?1)>0 OR EXISTS(
     SELECT 1 FROM json_each(highlights) h WHERE
     (?10>0 AND h.value->>'tier'>=?10 OR ?10=0 AND instr(h.value->>'kind',?1)>0)))
 AND (?7 IS NULL AND ?8 IS NULL OR EXISTS(
     SELECT 1 FROM json_each(highlights) h WHERE
     (?7 IS NULL OR CASE WHEN ?7 IN ('2','3','4','5')
        THEN h.value->>'tier'>=CAST(?7 AS INTEGER) ELSE h.value->>'kind'=?7 END)
     AND (?7 IS NOT NULL OR ?10=0 OR h.value->>'tier'>=?10)
     AND (?8 IS NULL OR h.value->>'actor'=?8)))
)";

struct ClipFilter<'a> {
    search: String,
    champion: Option<String>,
    queue: Option<String>,
    query: &'a ClipQuery,
    tier: u8,
}

impl<'a> ClipFilter<'a> {
    fn new(query: &'a ClipQuery) -> Result<Self> {
        let search = query
            .search
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        ensure!(search.len() <= 256, "clip search is too long");
        let tier = minimum_tier(&search);
        Ok(Self {
            search,
            champion: query.champion.as_ref().map(|v| v.to_lowercase()),
            queue: query.queue.as_ref().map(|v| v.to_lowercase()),
            query,
            tier,
        })
    }

    fn args(&self) -> std::array::IntoIter<&dyn rusqlite::ToSql, 10> {
        let q = self.query;
        let args: [&dyn rusqlite::ToSql; 10] = [
            &self.search,
            &self.champion,
            &q.from,
            &q.to,
            &self.queue,
            &q.outcome,
            &q.highlight,
            &q.actor,
            &q.match_group,
            &self.tier,
        ];
        args.into_iter()
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::clip_bundle::{
        self, Attribution, ClipEvents, ClipGame, EventAvailability, Highlight,
    };
    use crate::clip_export::ClipExportPreset;
    use chronobreak_replay_time::{ClipRange, FrameBoundary, ReplayTick};

    #[test]
    fn selected_highlights_and_account_groups_survive_source_deletion_and_cache_rebuild() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let game_path =
            library::tests::write_matched_game(root.path(), "100", "2026-08-05T10:00:00Z", false);
        let initial = library::list_games(root.path()).unwrap().remove(0);
        for (bundle, date, account) in [
            ("101", "2026-08-05T10:06:00Z", "synthetic-local-puuid"),
            ("102", "2026-08-05T10:07:00Z", "different-puuid"),
        ] {
            let part = library::tests::write_game(root.path(), bundle, date, false);
            let media = MediaId::new_v4();
            let metadata = part.join("metadata.json");
            let json = fs::read_to_string(&metadata).unwrap();
            fs::write(
                &metadata,
                json.replace(initial.media_id.as_ref().unwrap().as_str(), media.as_str()),
            )
            .unwrap();
            let mut candidate = crate::league_match::fixture(&media);
            candidate["local_puuid"] = serde_json::json!(account);
            fs::write(
                part.join("league_match.json"),
                serde_json::to_vec(&candidate).unwrap(),
            )
            .unwrap();
        }
        let games = library::list_games(root.path()).unwrap();
        let part = |id: &str| games.iter().find(|game| game.timestamp == id).unwrap();
        let match_group = initial.match_group.clone().unwrap();
        assert_eq!(part("101").match_group, initial.match_group);
        assert_ne!(part("102").match_group, initial.match_group);
        let make_clip = |title: &str, game: &library::GameSummary, start: u64, selected: bool| {
            let id = ClipId::new();
            let directory = root.path().join("clips").join(id.as_str());
            fs::create_dir_all(&directory).unwrap();
            let end = FrameBoundary::new(start + 300).unwrap();
            let start = FrameBoundary::new(start).unwrap();
            let header = ClipHeader {
                version: clip_bundle::VERSION,
                id: id.clone(),
                range: ClipRange::new(game.media_id.clone().unwrap(), start, end, end).unwrap(),
                source_timestamp: game.timestamp.clone(),
                game: ClipGame {
                    match_group: game.match_group.clone(),
                    played_at: Some(initial.recorded_at.clone()),
                    part_recorded_at: Some(game.recorded_at.clone()),
                    champion: Some("Syndra".into()),
                    account: Some("Fixture#EUW".into()),
                    queue_id: Some(420),
                    mode: Some("CLASSIC".into()),
                    outcome: Some(chronobreak_league_data::Outcome::Win),
                    final_kda: Some([9, 2, 7]),
                },
                exported_at: 1786000000,
                preset: ClipExportPreset::Horizontal,
                duration: ReplayTick::new(240_000_000).unwrap(),
                events: EventAvailability::Available,
                highlights: selected
                    .then_some(Highlight {
                        event_index: 0,
                        kind: HighlightKind::Pentakill,
                        attribution: Attribution::You,
                        actor: None,
                    })
                    .into_iter()
                    .collect(),
                custom_tags: if title == "Next part" {
                    vec!["Montage".into()]
                } else {
                    vec!["Évasion".into()]
                },
            };
            fs::write(
                directory.join(clip_bundle::media_leaf(title).unwrap()),
                b"fixture video",
            )
            .unwrap();
            fs::write(directory.join(clip_bundle::THUMBNAIL), b"fixture thumbnail").unwrap();
            clip_bundle::write_new_json(
                &directory.join(clip_bundle::HEADER),
                &header,
                clip_bundle::HEADER_LIMIT,
            )
            .unwrap();
            clip_bundle::write_new_json(
                &directory.join(clip_bundle::EVENTS),
                &ClipEvents::unavailable(),
                clip_bundle::EVENTS_LIMIT,
            )
            .unwrap();
            id
        };
        let selected = make_clip("The finish", &initial, 60, true);
        make_clip("Next part", part("101"), 30, false);
        make_clip("Later in the next part", part("101"), 90, false);
        make_clip("Opening moment", &initial, 0, false);
        make_clip("Third moment", &initial, 90, false);
        let other = make_clip("Other account", part("102"), 30, true);
        let scan = |token: &str| {
            library::build_library_snapshot(root.path(), "", token.into(), || Ok(())).unwrap()
        };
        let before = scan("before");
        assert_eq!(before.usage.clip_count, 6);
        let approved = crate::playback_file::ApprovedRoot::new(root.path()).unwrap();
        assert!(!library::clip_inventory_changed(&approved, &before).unwrap());
        let mut index = Index::open(cache.path(), root.path()).unwrap();
        index.reconcile(&before.games, &before.token).unwrap();
        let mut result = library::tests::result_fixture(initial.media_id.as_ref().unwrap());
        result.outcome = chronobreak_league_data::Outcome::Loss;
        result.duration_seconds = Some(1_804);
        let player = result.local_player.as_mut().unwrap();
        player.champion_name = Some("Syndra".into());
        player.spell1_id = Some(4);
        player.spell2_id = Some(14);
        result.source_facts.local_player = Some(obs::FinalPlayer {
            items: Some(vec![Some(3_085), None, Some(3_340)]),
            ..Default::default()
        });
        fs::write(
            game_path.join(chronobreak_league_data::RESULT_FILE),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        for bundle in ["100", "101", "102"] {
            index
                .history_recording(root.path(), part(bundle), false, &|| Ok(()))
                .unwrap();
        }
        let tagged = HistoryQuery {
            limit: 8,
            clip_tags: vec!["ÉVASION".into(), "montage".into()],
            ..Default::default()
        };
        let pending = index.history_for_snapshot(&tagged, 2, &before).unwrap();
        assert!(pending.clip_tags_pending && pending.rows.is_empty());
        assert!(!index.reconcile_clips(&before, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&before, &|| Ok(())).unwrap());
        let page = index
            .clips(
                &before,
                &ClipQuery::default(),
                "http://fixture",
                "generation",
            )
            .unwrap();
        assert_eq!(page.groups.len(), 2);
        let own_group = format!("game:{match_group}");
        let facts = page
            .groups
            .iter()
            .find(|group| group.key == own_group)
            .unwrap()
            .match_facts
            .as_ref()
            .unwrap();
        assert_eq!(facts.champion, "Syndra");
        assert_eq!(facts.outcome, Some(chronobreak_league_data::Outcome::Loss));
        assert_eq!(
            facts.summoner_spells,
            Some([Some("4".into()), Some("14".into())])
        );
        assert_eq!(facts.final_kda, Some([Some(6), Some(2), Some(9)]));
        assert_eq!(
            facts.final_items,
            Some(vec![Some(3_085), None, Some(3_340)])
        );
        assert_eq!(facts.duration_seconds, Some(1_804));
        assert!(
            page.groups
                .iter()
                .any(|group| group.key != own_group && group.match_facts.is_none())
        );
        let filtered = index
            .clips(
                &before,
                &ClipQuery {
                    search: Some("Next part".into()),
                    ..Default::default()
                },
                "http://fixture",
                "generation",
            )
            .unwrap();
        assert_eq!(
            filtered.groups[0]
                .match_facts
                .as_ref()
                .unwrap()
                .duration_seconds,
            Some(1_804)
        );
        let tagged_matches = index.history_for_snapshot(&tagged, 2, &before).unwrap();
        assert!(!tagged_matches.clip_tags_pending);
        assert_eq!(tagged_matches.rows.len(), 1);
        assert_eq!(
            tagged_matches.rows[0].game.match_group.as_ref(),
            Some(&match_group)
        );
        let unchanged = scan("fresh token");
        assert_ne!(before.token, unchanged.token);
        assert!(index.clip_ready(&unchanged.managed_clip_stamp));
        assert!(
            index
                .reconcile_clips(&unchanged, &|| panic!("reopened clip batch"))
                .unwrap()
        );
        let mut history = index
            .history(
                &HistoryQuery {
                    limit: 8,
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        index.attach_clip_counts(&before, &mut history).unwrap();
        assert_eq!(
            history
                .rows
                .iter()
                .find(|row| row.game.match_group.as_ref() == Some(&match_group))
                .unwrap()
                .clip_count,
            Some(5)
        );
        let scoped = ClipQuery {
            match_group: Some(match_group.clone()),
            ..Default::default()
        };
        let mut page = index
            .clips(&before, &scoped, "http://fixture", "generation")
            .unwrap();
        assert!(page.groups[0].match_facts.is_none());
        for (bundle, game_clock) in [("100", 1_122_000_000_i64), ("101", 1_370_000_000_i64)] {
            let media = part(bundle).media_id.as_ref().unwrap();
            let stamp =
                super::super::source_stamp(&root.path().join("games").join(bundle)).unwrap();
            let timeline = library::replay_descriptor(root.path(), "", bundle, String::new())
                .unwrap()
                .media_timeline;
            index.db.execute(
                "UPDATE recordings SET telemetry_status='ready',timeline_json=?2,source_stamp=?3 WHERE media_id=?1",
                params![media.as_str(), serde_json::to_string(&timeline).unwrap(), stamp],
            ).unwrap();
            let run = MediaId::new_v4();
            index
                .db
                .execute(
                    "INSERT INTO scopes(media_id,run_id,epoch) VALUES (?1,?2,0)",
                    params![media.as_str(), run.as_str()],
                )
                .unwrap();
            index.db.execute(
                "INSERT INTO rosters(media_id,run_id,epoch,observation_id,game_tick,replay_tick)
                 VALUES (?1,?2,0,1,?3,12000000),(?1,?2,0,2,?3+1000000,96000000)",
                params![media.as_str(), run.as_str(), game_clock],
            ).unwrap();
        }
        index
            .add_clip_game_clocks(root.path(), &before, &mut page, &|| Ok(()))
            .unwrap();
        assert_eq!(
            page.groups[0]
                .clips
                .iter()
                .map(|clip| clip.game_clock.map(GameTick::get))
                .collect::<Vec<_>>(),
            [
                None,
                Some(1_122_000_000),
                Some(1_122_000_000),
                Some(1_370_000_000)
            ]
        );
        let group = &page.groups[0];
        assert_eq!(
            group
                .clips
                .iter()
                .map(|clip| clip.title.as_str())
                .collect::<Vec<_>>(),
            ["Opening moment", "The finish", "Third moment", "Next part"]
        );
        let more = index
            .clips(
                &before,
                &ClipQuery {
                    group_key: Some(group.key.clone()),
                    clip_after: group.next.clone(),
                    ..scoped
                },
                "http://fixture",
                "generation",
            )
            .unwrap();
        assert_eq!(more.groups[0].clips[0].title, "Later in the next part");
        assert!(more.groups[0].next.is_none());
        assert_eq!(
            index.clip_tags(&before.managed_clip_stamp, "ÉV").unwrap(),
            ["Évasion"]
        );
        let selected_dir = root.path().join("clips").join(selected.as_str());
        let original_leaf = clip_bundle::media_leaf("The finish").unwrap();
        let renamed_leaf = clip_bundle::media_leaf("Renamed finish").unwrap();
        fs::rename(
            selected_dir.join(&original_leaf),
            selected_dir.join(&renamed_leaf),
        )
        .unwrap();
        assert!(library::clip_inventory_changed(&approved, &before).unwrap());
        let renamed = scan("renamed");
        assert!(!library::clip_inventory_changed(&approved, &renamed).unwrap());
        assert!(!index.clip_ready(&renamed.managed_clip_stamp));
        let pending = index.history_for_snapshot(&tagged, 2, &renamed).unwrap();
        assert!(pending.clip_tags_pending && pending.rows.is_empty());
        assert!(!index.reconcile_clips(&renamed, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&renamed, &|| Ok(())).unwrap());
        fs::rename(
            selected_dir.join(&renamed_leaf),
            selected_dir.join(&original_leaf),
        )
        .unwrap();
        fs::remove_dir_all(root.path().join("clips").join(other.as_str())).unwrap();
        assert!(library::clip_inventory_changed(&approved, &renamed).unwrap());
        let removed = scan("removed");
        assert!(!index.clip_ready(&removed.managed_clip_stamp));
        assert!(!index.reconcile_clips(&removed, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&removed, &|| Ok(())).unwrap());
        fs::remove_dir_all(game_path).unwrap();
        fs::remove_dir_all(root.path().join("games/101")).unwrap();
        fs::remove_dir_all(root.path().join("games/102")).unwrap();
        let after = scan("after");
        assert!(after.games.is_empty());
        let rebuilt = tempfile::tempdir().unwrap();
        let mut index = Index::open(rebuilt.path(), root.path()).unwrap();
        index.reconcile_snapshot(&after).unwrap();
        assert!(!index.reconcile_clips(&after, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&after, &|| Ok(())).unwrap());
        let filter = ClipQuery {
            search: Some("triple kill".into()),
            actor: Some("you".into()),
            queue: Some("classic".into()),
            ..Default::default()
        };
        let mut page = index
            .clips(&after, &filter, "http://fixture", "generation")
            .unwrap();
        index
            .add_clip_game_clocks(root.path(), &after, &mut page, &|| Ok(()))
            .unwrap();
        assert_eq!(page.groups.len(), 1);
        assert!(page.groups[0].match_facts.is_none());
        assert_eq!(page.groups[0].clips.len(), 1);
        let clip = &page.groups[0].clips[0];
        assert_eq!(clip.header.id, selected);
        assert_eq!(clip.title, "The finish");
        assert!(clip.game_clock.is_none());
        assert_eq!(clip.header.game.final_kda, Some([9, 2, 7]));
        assert!(clip.video_url.contains(selected.as_str()));
    }
}
