//! Disposable projection of accepted clip headers. Event payloads stay on disk.
use super::*;
use crate::clip_bundle::{ClipDescriptor, ClipGame, ClipHeader, ClipId};
use crate::library::LibrarySnapshot;
use crate::tags::{TagDefinition, TagId, implied_ids};
use chronobreak_replay_time::{
    MediaTimelineV2, ReplayTick, RoundingMode, frame_boundary_to_replay_tick,
};
use std::collections::{HashMap, HashSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const BATCH: usize = 64;
const GROUPS: usize = 8;
const INITIAL: usize = 4;
const MORE: usize = 24;
const GRID_PAGE_SIZE: usize = 24;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS clip_projection_state(
    singleton INTEGER PRIMARY KEY CHECK(singleton=1), token TEXT, phase INTEGER, after_id TEXT
);
CREATE TABLE IF NOT EXISTS clip_projection(
    id TEXT PRIMARY KEY, stamp TEXT NOT NULL, match_group TEXT, group_key TEXT NOT NULL,
    played INTEGER NOT NULL, part INTEGER NOT NULL, frame INTEGER NOT NULL,
    champion TEXT NOT NULL, day TEXT NOT NULL, queue TEXT NOT NULL, outcome TEXT NOT NULL,
    search TEXT NOT NULL, header TEXT,
    created INTEGER GENERATED ALWAYS AS (CAST(json_extract(header,'$.exported_at') AS INTEGER)) VIRTUAL
);
CREATE INDEX IF NOT EXISTS clip_projection_group ON clip_projection(group_key,part,frame,id);
CREATE INDEX IF NOT EXISTS clip_projection_match ON clip_projection(match_group) WHERE header IS NOT NULL;
CREATE INDEX IF NOT EXISTS clip_projection_created ON clip_projection(created DESC,id DESC)
    WHERE header IS NOT NULL;
CREATE INDEX IF NOT EXISTS clip_projection_grid_champion ON clip_projection(champion)
    WHERE header IS NOT NULL;
CREATE TABLE IF NOT EXISTS clip_tag_projection(
    clip_id TEXT NOT NULL REFERENCES clip_projection(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL, label TEXT NOT NULL, label_key TEXT NOT NULL,
    PRIMARY KEY(clip_id,tag_id)
);
CREATE INDEX IF NOT EXISTS clip_tag_id_label ON clip_tag_projection(tag_id,label_key,label);
CREATE INDEX IF NOT EXISTS clip_tag_label_prefix ON clip_tag_projection(label_key,tag_id,label);
CREATE TABLE IF NOT EXISTS clip_recovered_tag_definition(
    tag_id TEXT PRIMARY KEY, label TEXT NOT NULL, label_key TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS clip_recovered_tag_label_prefix
    ON clip_recovered_tag_definition(label_key,tag_id);
CREATE TRIGGER IF NOT EXISTS clip_tag_projection_recover_insert
AFTER INSERT ON clip_tag_projection BEGIN
    INSERT OR REPLACE INTO clip_recovered_tag_definition
    SELECT tag_id,label,label_key FROM clip_tag_projection WHERE tag_id=NEW.tag_id
    ORDER BY label_key,label LIMIT 1;
END;
CREATE TRIGGER IF NOT EXISTS clip_tag_projection_recover_delete
AFTER DELETE ON clip_tag_projection BEGIN
    DELETE FROM clip_recovered_tag_definition WHERE tag_id=OLD.tag_id;
    INSERT OR REPLACE INTO clip_recovered_tag_definition
    SELECT tag_id,label,label_key FROM clip_tag_projection WHERE tag_id=OLD.tag_id
    ORDER BY label_key,label LIMIT 1;
END;
CREATE TABLE IF NOT EXISTS clip_grid_tag_projection(
    tag_id TEXT NOT NULL,
    created INTEGER NOT NULL,
    clip_id TEXT NOT NULL REFERENCES clip_projection(id) ON DELETE CASCADE,
    PRIMARY KEY(tag_id,created DESC,clip_id DESC)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS clip_grid_tag_clip ON clip_grid_tag_projection(clip_id,tag_id);
";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipQuery {
    pub match_group: Option<String>,
    pub include_game_clock: Option<bool>,
    pub after: Option<(i64, String)>,
    pub group_key: Option<String>,
    pub clip_after: Option<(i64, i64, ClipId)>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClipGridQuery {
    pub search: Option<String>,
    #[serde(default)]
    pub champions: Vec<String>,
    pub queue: Option<ClipGridQueue>,
    pub outcome: Option<ClipGridOutcome>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagId>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClipGridQueue {
    Ranked,
    Aram,
    Arena,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClipGridOutcome {
    Win,
    Loss,
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
        Self {
            video_url: clip_url(&clip.id, "video", origin, capability),
            thumbnail_url: clip_url(&clip.id, "thumbnail", origin, capability),
            header,
            title: clip.title().into(),
            game_clock: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipGroup {
    pub key: String,
    pub clips: Vec<ClipCard>,
    pub next: Option<(i64, i64, ClipId)>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipGridCard {
    pub id: ClipId,
    pub title: String,
    pub champion: Option<String>,
    pub duration: ReplayTick,
    pub thumbnail_url: String,
    pub tags: Vec<TagDefinition>,
}

pub(crate) type ClipGridCursor = (i64, ClipId);

#[derive(Debug, Default, Serialize)]
pub(crate) struct ClipGridPage {
    pub clips: Vec<ClipGridCard>,
    pub next: Option<ClipGridCursor>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipPlayback {
    pub clip: ClipCard,
    pub file_size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClipPage {
    pub groups: Vec<ClipGroup>,
    pub next: Option<(i64, String)>,
}

fn clip_url(id: &ClipId, asset: &str, origin: &str, capability: &str) -> String {
    format!("{origin}/clips/{}/{asset}?g={capability}", id.as_str())
}

fn grid_page_sql(filter: &ClipGridFilter, after: bool) -> String {
    let seek = if after {
        "WHERE (created,id)<(?9,?10)"
    } else {
        ""
    };
    let source = if filter.has_tags {
        "clip_grid_tag_projection anchor JOIN clip_projection p ON p.id=anchor.clip_id
         WHERE anchor.tag_id=json_extract(?7,'$[0]')"
    } else {
        "clip_projection p WHERE 1=1"
    };
    let select = if filter.has_tags {
        "anchor.clip_id id,anchor.created created,p.header"
    } else {
        "p.id,p.created,p.header"
    };
    let custom_search = if filter.search.is_empty() {
        ""
    } else {
        "matching_custom AS MATERIALIZED (
            SELECT id FROM tag_definition WHERE instr(label_key,?1)>0
            UNION ALL
            SELECT r.tag_id FROM clip_recovered_tag_definition r
            WHERE instr(r.label_key,?1)>0 AND NOT EXISTS(SELECT 1 FROM tag_definition d WHERE d.id=r.tag_id)
        ),"
    };
    format!(
        "WITH {custom_search} filtered AS (
        SELECT {select} FROM {source} AND p.header IS NOT NULL{}
        )
        SELECT id,created,header FROM filtered {seek} ORDER BY created DESC,id DESC LIMIT ?8",
        filter.predicate
    )
}

fn date_value(date: Option<&str>) -> Option<i64> {
    OffsetDateTime::parse(date?, &Rfc3339)
        .ok()
        .map(|d| d.unix_timestamp())
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
        self.refresh_tags_from_root(&self.root_path()?)?;
        if self.clip_ready(&snapshot.managed_clip_stamp) {
            return Ok(true);
        }
        let installed: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='clip_projection_state')",
            [],
            |r| r.get(0),
        )?;
        if !installed {
            check()?;
            let tx = self.db.transaction()?;
            tx.execute_batch(SCHEMA)?;
            check()?;
            tx.commit()?;
        }
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

    pub(crate) fn clip_grid_page(
        &self,
        snapshot: &LibrarySnapshot,
        query: &ClipGridQuery,
        after: Option<&ClipGridCursor>,
        origin: &str,
        capability: &str,
    ) -> Result<ClipGridPage> {
        ensure!(
            self.clip_ready(&snapshot.managed_clip_stamp),
            "clip catalog is pending"
        );
        let filter = ClipGridFilter::new(query)?;
        let mut statement = self.db.prepare(&grid_page_sql(&filter, after.is_some()))?;
        let limit = (GRID_PAGE_SIZE + 1) as i64;
        let after_id = after.map(|(_, id)| id.as_str());
        let seek = after.map(|(created, _)| [created as &dyn rusqlite::ToSql, &after_id]);
        let args = filter
            .args()
            .chain([&limit as &dyn rusqlite::ToSql])
            .chain(seek.into_iter().flatten());
        let mut rows = statement.query(rusqlite::params_from_iter(args))?;
        let mut page = ClipGridPage::default();
        let mut last = None;
        while let Some(row) = rows.next()? {
            if page.clips.len() == GRID_PAGE_SIZE {
                page.next = last;
                break;
            }
            let id = ClipId::parse(&row.get::<_, String>(0)?)?;
            let descriptor = snapshot
                .managed_clips
                .get(&id)
                .context("clip catalog changed")?;
            let header: ClipHeader = serde_json::from_str(&row.get::<_, String>(2)?)?;
            last = Some((row.get(1)?, id.clone()));
            page.clips.push(ClipGridCard {
                thumbnail_url: clip_url(&id, "thumbnail", origin, capability),
                id,
                title: descriptor.title().into(),
                champion: header.game.champion,
                duration: header.duration,
                tags: self.resolve_tags(header.tags)?,
            });
        }
        Ok(page)
    }

    pub(crate) fn clip_grid_champions(&self, inventory_stamp: &str) -> Result<Vec<String>> {
        if !self.clip_ready(inventory_stamp) {
            return Ok(Vec::new());
        }
        let mut next = self.db.prepare_cached(
            "SELECT champion,json_extract(header,'$.game.champion') FROM clip_projection
             WHERE header IS NOT NULL AND champion<>'' AND champion>?1
             ORDER BY champion LIMIT 1",
        )?;
        let mut after = String::new();
        let mut champions = Vec::new();
        while let Some((champion, label)) = next
            .query_row([&after], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()?
        {
            champions.push(label);
            after = champion;
        }
        Ok(champions)
    }

    pub(crate) fn clip_playback(
        &self,
        snapshot: &LibrarySnapshot,
        id: &ClipId,
        origin: &str,
        capability: &str,
    ) -> Result<ClipPlayback> {
        ensure!(
            self.clip_ready(&snapshot.managed_clip_stamp),
            "clip catalog is pending"
        );
        let descriptor = snapshot.managed_clips.get(id).context("clip unavailable")?;
        let json: String = self.db.query_row(
            "SELECT header FROM clip_projection WHERE id=?1 AND header IS NOT NULL",
            [id.as_str()],
            |row| row.get(0),
        )?;
        let mut header: ClipHeader = serde_json::from_str(&json)?;
        header.tags = self.resolve_tags(header.tags)?;
        // A missing or rebuilding match index never blocks independent playback.
        let _ = self.enrich_clip_game(&snapshot.catalog_key, &mut header.game);
        Ok(ClipPlayback {
            clip: ClipCard::new(descriptor, header, origin, capability),
            file_size_bytes: descriptor.video.length(),
        })
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

    /// Rebuild one changed header without invalidating the accepted inventory projection.
    pub(crate) fn reproject_clip(&mut self, clip: &ClipDescriptor) -> Result<()> {
        let stamp = clip.stamp();
        let tx = self.db.transaction()?;
        project_clip(&tx, clip, &stamp)?;
        tx.commit()?;
        Ok(())
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
        if let Some(key) = &query.group_key {
            let group = self.clip_group(
                snapshot,
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
        let groups_sql = "SELECT group_key,max(played) recent FROM clip_projection
            WHERE header IS NOT NULL AND (?1 IS NULL OR match_group=?1) GROUP BY group_key
            HAVING ?2 IS NULL OR recent<?2 OR (recent=?2 AND group_key>?3)
            ORDER BY recent DESC,group_key LIMIT ?4";
        let mut statement = self.db.prepare(&groups_sql)?;
        let after_time = query.after.as_ref().map(|(time, _)| *time);
        let after_key = query.after.as_ref().map(|(_, key)| key);
        let cap = (GROUPS + 1) as i64;
        let extra: [&dyn rusqlite::ToSql; 4] = [&query.match_group, &after_time, &after_key, &cap];
        let keys = statement
            .query_map(extra, |row| {
                Ok((row.get::<_, i64>(1)?, row.get::<_, String>(0)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let next = keys.get(GROUPS).map(|_| keys[GROUPS - 1].clone());
        let mut groups = Vec::new();
        for (_, key) in keys.into_iter().take(GROUPS) {
            groups.push(self.clip_group(snapshot, &key, None, INITIAL, origin, capability)?);
        }
        Ok(ClipPage { groups, next })
    }

    pub(crate) fn enrich_clip_game(&self, catalog_key: &str, game: &mut ClipGame) -> Result<()> {
        let Some(group) = &game.match_group else {
            return Ok(());
        };
        self.require_catalog(catalog_key)?;
        // Use the same checked part priority as History. Parts without final
        // facts or a local roster have no extra stats to add to the clip header.
        let part = self.db.prepare_cached(
            "SELECT part.media_id,part.champion,part.result_json FROM recordings part
             WHERE part.match_group=?1 AND part.facts_current=1
               AND (part.result_json IS NOT NULL OR (part.telemetry_status='ready' AND EXISTS(
                   SELECT 1 FROM scopes s JOIN roster_members m ON m.media_id=s.media_id
                       AND m.run_id=s.run_id AND m.epoch=s.epoch AND m.participant_id=s.local_participant
                   WHERE s.media_id=part.media_id
               )))
             ORDER BY part.result_json IS NULL,part.recorded_at DESC,part.media_id LIMIT 1",
        )?.query_row([group], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
        }).optional()?;
        if let Some((media, champion, result)) = part {
            if game
                .champion
                .as_deref()
                .is_none_or(|name| name == "Unknown")
                && champion != "Unknown"
            {
                game.champion = crate::clip_bundle::display_label(Some(&champion));
            }
            if let Some(result) = result {
                game.apply_final_result(&serde_json::from_str::<ResultFile>(&result)?);
            } else if let Some(spells) = self
                .history_stats(&MediaId::parse(&media)?, None)?
                .summoner_spells
            {
                // A recorded roster can supply spells, but its items, KDA and
                // game clock are snapshots rather than final match statistics.
                game.fill_spells(spells);
            }
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
        key: &str,
        after: Option<&(i64, i64, ClipId)>,
        limit: usize,
        origin: &str,
        capability: &str,
    ) -> Result<ClipGroup> {
        let sql = "SELECT id,part,frame,header FROM clip_projection
            WHERE header IS NOT NULL AND group_key=?1
            AND (?2 IS NULL OR (part,frame,id)>(?2,?3,?4))
            ORDER BY part,frame,id LIMIT ?5";
        let mut statement = self.db.prepare_cached(&sql)?;
        let part = after.map(|(part, _, _)| part);
        let frame = after.map(|(_, frame, _)| frame);
        let id = after.map(|(_, _, id)| id.as_str());
        let cap = (limit + 1) as i64;
        let extra: [&dyn rusqlite::ToSql; 5] = [&key, &part, &frame, &id, &cap];
        let mut rows = statement.query(extra)?;
        let mut clips = Vec::new();
        let mut last = None;
        while let Some(row) = rows.next()? {
            if clips.len() == limit {
                return Ok(ClipGroup {
                    key: key.into(),
                    clips,
                    next: last,
                });
            }
            let id = ClipId::parse(&row.get::<_, String>(0)?)?;
            let descriptor = snapshot
                .managed_clips
                .get(&id)
                .context("clip catalog changed")?;
            let mut header: ClipHeader = serde_json::from_str(&row.get::<_, String>(3)?)?;
            header.tags = self.resolve_tags(header.tags)?;
            clips.push(ClipCard::new(descriptor, header, origin, capability));
            last = Some((row.get(1)?, row.get(2)?, id));
        }
        Ok(ClipGroup {
            key: key.into(),
            clips,
            next: None,
        })
    }
}

fn project_clip(db: &Connection, clip: &ClipDescriptor, stamp: &str) -> Result<()> {
    db.execute(
        "DELETE FROM clip_grid_tag_projection WHERE clip_id=?1",
        [clip.id.as_str()],
    )?;
    db.execute(
        "DELETE FROM clip_tag_projection WHERE clip_id=?1",
        [clip.id.as_str()],
    )?;
    let Ok(header) = clip.read_header() else {
        db.execute("INSERT OR REPLACE INTO clip_projection VALUES(?1,?2,NULL,'',0,0,0,'','','','','',NULL)",
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
    let kda = game.final_kda.map_or_else(String::new, |kda| {
        kda.map(|value| value.map_or_else(String::new, |number| number.to_string()))
            .join("/")
    });
    let tag_labels = header
        .tags
        .iter()
        .filter(|tag| tag.builtin)
        .map(|tag| tag.label.as_str())
        .collect::<Vec<_>>();
    let search = format!(
        "{}\n{}\n{champion}\n{mode}\n{queue}\n{outcome}\n{day}\n{}\n{kda}",
        clip.title(),
        game.account.as_deref().unwrap_or_default(),
        tag_labels.join("\n")
    )
    .to_lowercase();
    db.execute(
        "INSERT OR REPLACE INTO clip_projection VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            clip.id.as_str(),
            stamp,
            game.match_group,
            key,
            played,
            part,
            sql_u64(header.range.start_frame.get())?,
            champion.to_lowercase(),
            day,
            queue.to_lowercase(),
            outcome,
            search,
            serde_json::to_string(&header)?
        ],
    )?;
    let mut insert = db.prepare_cached("INSERT INTO clip_tag_projection VALUES(?1,?2,?3,?4)")?;
    for tag in header.tags.iter().filter(|tag| !tag.builtin) {
        insert.execute(params![
            clip.id.as_str(),
            tag.id.as_str(),
            tag.label,
            tag.label.to_lowercase(),
        ])?;
    }
    let mut grid_insert =
        db.prepare_cached("INSERT OR IGNORE INTO clip_grid_tag_projection VALUES(?1,?2,?3)")?;
    let created = sql_u64(header.exported_at)?;
    for tag in &header.tags {
        for tag_id in implied_ids(&tag.id) {
            grid_insert.execute(params![tag_id.as_str(), created, clip.id.as_str()])?;
        }
    }
    Ok(())
}

struct ClipGridFilter {
    search: String,
    champions: Option<String>,
    from: Option<String>,
    to: Option<String>,
    queue: Option<String>,
    outcome: Option<String>,
    tags: Option<String>,
    has_tags: bool,
    predicate: String,
}

impl ClipGridFilter {
    fn new(query: &ClipGridQuery) -> Result<Self> {
        let search = query
            .search
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        ensure!(search.len() <= 256, "clip search is too long");
        ensure!(query.champions.len() <= 256, "too many clip champions");
        let mut seen_champions = HashSet::new();
        let champions = query
            .champions
            .iter()
            .filter_map(|champion| {
                let champion = champion.trim().to_lowercase();
                (!champion.is_empty() && seen_champions.insert(champion.clone()))
                    .then_some(champion)
            })
            .collect::<Vec<_>>();
        ensure!(
            champions
                .iter()
                .all(|champion| champion.chars().count() <= 128),
            "invalid clip champion"
        );
        let from = query.from.as_deref().map(grid_day).transpose()?;
        let to = query.to.as_deref().map(grid_day).transpose()?;
        ensure!(
            from.as_ref()
                .zip(to.as_ref())
                .is_none_or(|(from, to)| from <= to),
            "invalid clip date range"
        );
        ensure!(query.tags.len() <= 40, "too many clip tags");
        let mut seen_tags = HashSet::new();
        let tag_ids = query
            .tags
            .iter()
            .filter(|tag| seen_tags.insert(*tag))
            .map(|tag| tag.as_str())
            .collect::<Vec<_>>();
        let (queue, queue_predicate) = match query.queue {
            None => (None, String::new()),
            Some(queue) => {
                let ids = grid_queue_ids(queue);
                let predicate = if matches!(queue, ClipGridQueue::Other) {
                    " AND p.queue<>'' AND p.queue NOT GLOB '*[^0-9]*'
                      AND p.queue NOT IN (SELECT CAST(value AS TEXT) FROM json_each(?5))"
                } else {
                    " AND p.queue IN (SELECT CAST(value AS TEXT) FROM json_each(?5))"
                };
                (Some(serde_json::to_string(&ids)?), predicate.into())
            }
        };
        let outcome = query.outcome.map(|outcome| match outcome {
            ClipGridOutcome::Win => "win".to_owned(),
            ClipGridOutcome::Loss => "loss".to_owned(),
        });
        let search_predicate = if search.is_empty() {
            " AND ?1=''"
        } else {
            " AND (instr(p.search,?1)>0 OR (EXISTS(SELECT 1 FROM matching_custom)
               AND EXISTS(SELECT 1 FROM matching_custom named
                          CROSS JOIN clip_tag_projection assigned
                          WHERE assigned.clip_id=p.id AND assigned.tag_id=named.id)))"
        };
        let mut predicate = format!(
            " {search_predicate}
               AND (?2 IS NULL OR p.champion IN (SELECT value FROM json_each(?2)))
               AND (?3 IS NULL OR p.day>=?3 AND p.day<>'')
               AND (?4 IS NULL OR p.day<=?4 AND p.day<>'')
               {queue_predicate}
               AND (?6 IS NULL OR p.outcome=?6)"
        );
        for index in 1..tag_ids.len() {
            predicate.push_str(&format!(
                " AND EXISTS(SELECT 1 FROM clip_grid_tag_projection tag{index}
                    WHERE tag{index}.clip_id=p.id AND tag{index}.tag_id=json_extract(?7,'$[{index}]'))"
            ));
        }
        Ok(Self {
            search,
            champions: (!champions.is_empty())
                .then(|| serde_json::to_string(&champions))
                .transpose()?,
            from,
            to,
            queue,
            outcome,
            tags: (!tag_ids.is_empty())
                .then(|| serde_json::to_string(&tag_ids))
                .transpose()?,
            has_tags: !tag_ids.is_empty(),
            predicate,
        })
    }

    fn args(&self) -> std::array::IntoIter<&dyn rusqlite::ToSql, 7> {
        [
            &self.search as &dyn rusqlite::ToSql,
            &self.champions,
            &self.from,
            &self.to,
            &self.queue,
            &self.outcome,
            &self.tags,
        ]
        .into_iter()
    }
}

fn grid_queue_ids(queue: ClipGridQueue) -> Vec<u32> {
    use crate::league_index::query::HistoryQueue;
    let queues: &[HistoryQueue] = match queue {
        ClipGridQueue::Ranked => &[
            HistoryQueue::RankedSoloDuo,
            HistoryQueue::RankedFlex,
            HistoryQueue::Ranked5s,
        ],
        ClipGridQueue::Aram => &[
            HistoryQueue::AramMayhem,
            HistoryQueue::Aram,
            HistoryQueue::AramMayhemClassic,
        ],
        ClipGridQueue::Arena => &[HistoryQueue::Arena],
        // Match History's Other bucket intentionally excludes only its featured queues.
        ClipGridQueue::Other => &HistoryQueue::FEATURED,
    };
    queues
        .iter()
        .flat_map(|queue| queue.ids())
        .copied()
        .collect()
}

fn grid_day(value: &str) -> Result<String> {
    let timestamp = format!("{value}T00:00:00Z");
    OffsetDateTime::parse(&timestamp, &Rfc3339).context("invalid clip date")?;
    ensure!(value.len() == 10, "invalid clip date");
    Ok(value.to_owned())
}

#[cfg(test)]
#[path = "clip_grid_measurements.rs"]
mod measurements;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::clip_bundle::{self, ClipEvents, ClipGame, EventAvailability};
    use crate::clip_export::ClipExportPreset;
    use crate::tags::{TagDefinition, TagId, builtin_definition};
    use chronobreak_replay_time::{ClipRange, FrameBoundary, ReplayTick};

    fn write_clip_fixture(root: &Path, title: &str, header: &ClipHeader) {
        let directory = root.join("clips").join(header.id.as_str());
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(clip_bundle::media_leaf(title).unwrap()),
            b"fixture video",
        )
        .unwrap();
        fs::write(directory.join(clip_bundle::THUMBNAIL), b"fixture thumbnail").unwrap();
        clip_bundle::write_new_json(
            &directory.join(clip_bundle::HEADER),
            header,
            clip_bundle::HEADER_LIMIT,
        )
        .unwrap();
        clip_bundle::write_new_json(
            &directory.join(clip_bundle::EVENTS),
            &ClipEvents::unavailable(),
            clip_bundle::EVENTS_LIMIT,
        )
        .unwrap();
    }

    #[test]
    fn selected_tags_and_account_groups_survive_source_deletion_and_cache_rebuild() {
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
        let evasion = TagDefinition::custom("Évasion").unwrap();
        let montage = TagDefinition::custom("Montage").unwrap();
        let make_clip = |title: &str, game: &library::GameSummary, start: u64, selected: bool| {
            let id = ClipId::new();
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
                    final_kda: Some([Some(9), Some(2), Some(7)]),
                    ..Default::default()
                },
                exported_at: 1786000000,
                preset: ClipExportPreset::Horizontal,
                duration: ReplayTick::new(240_000_000).unwrap(),
                events: EventAvailability::Available,
                tags: selected
                    .then(|| {
                        builtin_definition(&TagId::parse("builtin:pentakill").unwrap()).unwrap()
                    })
                    .into_iter()
                    .chain(std::iter::once(if title == "Next part" {
                        montage.clone()
                    } else {
                        evasion.clone()
                    }))
                    .collect(),
            };
            write_clip_fixture(root.path(), title, &header);
            id
        };
        let selected = make_clip("The finish", &initial, 60, true);
        let older_match_part = make_clip("Next part", part("101"), 30, false);
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
        let playback = index
            .clip_playback(&before, &older_match_part, "http://fixture", "generation")
            .unwrap()
            .clip;
        let facts = &playback.header.game;
        assert_eq!(facts.champion.as_deref(), Some("Syndra"));
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
        let unrelated = index
            .clip_playback(&before, &other, "http://fixture", "generation")
            .unwrap();
        assert!(unrelated.clip.header.game.duration_seconds.is_none());
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
            index
                .tag_choices("ÉV")
                .unwrap()
                .into_iter()
                .map(|tag| tag.label)
                .collect::<Vec<_>>(),
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

        // The export authority handles exact-part results even without an index.
        let exact = library::recording_export_authority(&game_path, None)
            .unwrap()
            .game;
        assert_eq!(exact.summoner_spells, facts.summoner_spells);
        assert_eq!(exact.final_items, facts.final_items);
        assert_eq!(exact.duration_seconds, facts.duration_seconds);
        // A draft from an earlier part saves the checked account-scoped result.
        let mut durable = removed.managed_clips[&older_match_part]
            .read_header()
            .unwrap();
        durable.id = ClipId::new();
        durable.game = library::recording_export_authority(&root.path().join("games/101"), None)
            .unwrap()
            .game;
        index
            .enrich_clip_game(&before.catalog_key, &mut durable.game)
            .unwrap();
        assert_eq!(durable.game.duration_seconds, Some(1_804));
        write_clip_fixture(root.path(), "Durable match details", &durable);

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
            match_group: Some(match_group.clone()),
            ..Default::default()
        };
        let mut page = index
            .clips(&after, &filter, "http://fixture", "generation")
            .unwrap();
        index
            .add_clip_game_clocks(root.path(), &after, &mut page, &|| Ok(()))
            .unwrap();
        assert_eq!(page.groups.len(), 1);
        assert_eq!(page.groups[0].clips.len(), 4);
        let clip = page.groups[0]
            .clips
            .iter()
            .find(|clip| clip.header.id == selected)
            .unwrap();
        assert_eq!(clip.header.id, selected);
        assert_eq!(clip.title, "The finish");
        assert!(clip.game_clock.is_none());
        assert_eq!(
            clip.header.game.final_kda,
            Some([Some(9), Some(2), Some(7)])
        );
        assert!(clip.video_url.contains(selected.as_str()));
        let persisted = index
            .clip_playback(&after, &durable.id, "http://fixture", "generation")
            .unwrap();
        assert_eq!(persisted.clip.header.game, durable.game);
        assert_eq!(persisted.file_size_bytes, b"fixture video".len() as u64);
        let partial = index
            .clip_playback(&after, &selected, "http://fixture", "generation")
            .unwrap();
        assert_eq!(
            partial.clip.header.game.final_kda,
            Some([Some(9), Some(2), Some(7)])
        );
        assert!(partial.clip.header.game.final_items.is_none());
        assert!(partial.clip.header.game.duration_seconds.is_none());
    }

    #[test]
    fn clip_grid_seeks_by_creation_time_and_id_independently_of_match_date() {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let mut header = ClipHeader {
            version: clip_bundle::VERSION,
            id: ClipId::new(),
            range: ClipRange::new(
                MediaId::new_v4(),
                FrameBoundary::new(0).unwrap(),
                FrameBoundary::new(300).unwrap(),
                FrameBoundary::new(300).unwrap(),
            )
            .unwrap(),
            source_timestamp: "100".into(),
            game: ClipGame {
                champion: Some("Syndra".into()),
                ..Default::default()
            },
            exported_at: 0,
            preset: ClipExportPreset::Horizontal,
            duration: ReplayTick::new(240_000_000).unwrap(),
            events: EventAvailability::Unavailable,
            tags: vec![],
        };
        let mut expected = Vec::new();
        for position in 0..(GRID_PAGE_SIZE * 2 + 5) {
            let older_match = position % 2 == 0;
            header.id = ClipId::new();
            // The first continuation must cross a set of equal timestamps.
            header.exported_at = if position < 30 {
                200
            } else {
                140 - position as u64
            };
            header.game.played_at = Some(
                if older_match {
                    "2026-07-03T10:00:00Z"
                } else {
                    "2026-08-05T10:00:00Z"
                }
                .into(),
            );
            header.game.champion = Some(if older_match { "Syndra" } else { "Ahri" }.into());
            header.game.queue_id = Some(match position % 4 {
                0 => 420,
                1 => 450,
                2 => 1750,
                _ => 999,
            });
            header.game.outcome = (position % 3 == 0)
                .then_some(chronobreak_league_data::Outcome::Win)
                .or(Some(chronobreak_league_data::Outcome::Loss));
            header.tags = (position % 3 == 0)
                .then(|| builtin_definition(&TagId::parse("builtin:baron_steal").unwrap()).unwrap())
                .into_iter()
                .chain((position % 5 != 0).then(|| {
                    builtin_definition(&TagId::parse("builtin:pentakill").unwrap()).unwrap()
                }))
                .collect();
            write_clip_fixture(root.path(), &format!("Clip {position}"), &header);
            expected.push((
                header.exported_at,
                header.id.clone(),
                older_match && position % 3 == 0,
                older_match && position % 4 == 0 && position % 3 == 0 && position % 5 != 0,
                position % 5 != 0,
            ));
        }
        expected.sort_by(|a, b| (b.0, b.1.as_str()).cmp(&(a.0, a.1.as_str())));
        let snapshot =
            library::build_library_snapshot(root.path(), "", "grid".into(), || Ok(())).unwrap();
        let mut index = Index::open(cache.path(), root.path()).unwrap();
        while !index.reconcile_clips(&snapshot, &|| Ok(())).unwrap() {}

        assert!(index.clip_ready(&snapshot.managed_clip_stamp));
        drop(index);

        let index = Index::reader(cache.path(), root.path()).unwrap();
        assert_eq!(
            index
                .clip_grid_champions(&snapshot.managed_clip_stamp)
                .unwrap(),
            ["Ahri", "Syndra"]
        );
        let filters = ClipGridQuery::default();
        let page = |query, after| {
            index
                .clip_grid_page(&snapshot, query, after, "http://fixture", "generation")
                .unwrap()
        };
        let first = page(&filters, None);
        let second = page(&filters, first.next.as_ref());
        let last = page(&filters, second.next.as_ref());
        assert_eq!(
            [first.clips.len(), second.clips.len(), last.clips.len()],
            [24, 24, 5]
        );
        assert!(last.next.is_none());
        assert_eq!(
            first
                .clips
                .iter()
                .chain(&second.clips)
                .chain(&last.clips)
                .map(|clip| &clip.id)
                .collect::<Vec<_>>(),
            expected.iter().map(|entry| &entry.1).collect::<Vec<_>>(),
        );
        let filtered_query = ClipGridQuery {
            from: Some("2026-07-03".into()),
            to: Some("2026-07-03".into()),
            search: Some("BARON STEAL".into()),
            tags: vec![TagId::parse("builtin:baron_steal").unwrap()],
            ..Default::default()
        };
        let filtered = page(&filtered_query, None);
        assert_eq!(
            filtered
                .clips
                .iter()
                .map(|clip| &clip.id)
                .collect::<Vec<_>>(),
            expected
                .iter()
                .filter(|entry| entry.2)
                .map(|entry| &entry.1)
                .collect::<Vec<_>>(),
        );
        let exact_tag_query = ClipGridQuery {
            champions: vec!["syndra".into()],
            queue: Some(ClipGridQueue::Ranked),
            outcome: Some(ClipGridOutcome::Win),
            tags: vec![
                TagId::parse("builtin:pentakill").unwrap(),
                TagId::parse("builtin:baron_steal").unwrap(),
            ],
            ..Default::default()
        };
        let exact_tags = page(&exact_tag_query, None);
        assert_eq!(
            exact_tags
                .clips
                .iter()
                .map(|clip| &clip.id)
                .collect::<Vec<_>>(),
            expected
                .iter()
                .filter(|entry| entry.3)
                .map(|entry| &entry.1)
                .collect::<Vec<_>>(),
        );
        assert!(exact_tags.clips.iter().all(|clip| {
            clip.tags
                .iter()
                .any(|tag| tag.id.as_str() == "builtin:pentakill")
        }));
        let implied_quadra_query = ClipGridQuery {
            // A Pentakill carries the lower multikill IDs in the seek projection.
            tags: vec![TagId::parse("builtin:quadra_kill").unwrap()],
            ..Default::default()
        };
        let anchor_first = page(&implied_quadra_query, None);
        assert!(anchor_first.next.is_some());
        let anchor_second = page(&implied_quadra_query, anchor_first.next.as_ref());
        assert!(anchor_second.next.is_none());
        assert_eq!(
            anchor_first
                .clips
                .iter()
                .chain(&anchor_second.clips)
                .map(|clip| &clip.id)
                .collect::<Vec<_>>(),
            expected
                .iter()
                .filter(|entry| entry.4)
                .map(|entry| &entry.1)
                .collect::<Vec<_>>(),
        );

        let filter = ClipGridFilter::new(&ClipGridQuery::default()).unwrap();
        let plan = format!("EXPLAIN QUERY PLAN {}", grid_page_sql(&filter, true));
        let details = index
            .db
            .prepare(&plan)
            .unwrap()
            .query_map(
                rusqlite::params_from_iter(filter.args().chain([
                    &0_i64 as &dyn rusqlite::ToSql,
                    &None::<i64>,
                    &None::<String>,
                ])),
                |row| row.get::<_, String>(3),
            )
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(
            details
                .iter()
                .any(|line| line.contains("SEARCH") && line.contains("clip_projection_created")),
            "{details:?}"
        );
        assert!(
            !details.iter().any(|line| line.contains("TEMP B-TREE")),
            "{details:?}"
        );
    }
}
