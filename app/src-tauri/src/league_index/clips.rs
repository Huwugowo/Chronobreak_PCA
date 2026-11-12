//! Disposable projection of accepted clip headers. Event payloads stay on disk.
use super::*;
use crate::clip_bundle::{ClipDescriptor, ClipHeader, ClipId, HighlightKind};
use crate::library::LibrarySnapshot;
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
        Ok(ClipPage { groups, next })
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
                custom_tags: vec!["Évasion".into()],
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
        let mut index = Index::open(cache.path(), root.path()).unwrap();
        index.reconcile(&before.games, &before.token).unwrap();
        assert!(!index.reconcile_clips(&before, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&before, &|| Ok(())).unwrap());
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
        let page = index
            .clips(&before, &scoped, "http://fixture", "generation")
            .unwrap();
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
        let renamed = scan("renamed");
        assert!(!index.clip_ready(&renamed.managed_clip_stamp));
        assert!(!index.reconcile_clips(&renamed, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&renamed, &|| Ok(())).unwrap());
        fs::rename(
            selected_dir.join(&renamed_leaf),
            selected_dir.join(&original_leaf),
        )
        .unwrap();
        fs::remove_dir_all(root.path().join("clips").join(other.as_str())).unwrap();
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
        assert!(!index.reconcile_clips(&after, &|| Ok(())).unwrap());
        assert!(index.reconcile_clips(&after, &|| Ok(())).unwrap());
        let filter = ClipQuery {
            search: Some("triple kill".into()),
            actor: Some("you".into()),
            queue: Some("classic".into()),
            match_group: Some(match_group),
            ..Default::default()
        };
        let page = index
            .clips(&after, &filter, "http://fixture", "generation")
            .unwrap();
        assert_eq!(page.groups.len(), 1);
        assert_eq!(page.groups[0].clips.len(), 1);
        let clip = &page.groups[0].clips[0];
        assert_eq!(clip.header.id, selected);
        assert_eq!(clip.title, "The finish");
        assert_eq!(clip.header.game.final_kda, Some([9, 2, 7]));
        assert!(clip.video_url.contains(selected.as_str()));
    }
}
