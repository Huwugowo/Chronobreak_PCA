//! Bounded, compact recorded state, independent of event viewport queries.
use super::{replay::ReplayIdentity, *};
use chronobreak_replay_time::{MediaTimelineV2, ReplayTick};

const PAGE_SAMPLES: usize = 128;
const PAGE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OverlayCursor {
    pub replay_tick: ReplayTick,
    pub observation_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OverlayQuery {
    pub scope: ScopeQuery,
    pub after: Option<OverlayCursor>,
    pub revision: Option<journal::Position>,
    pub limit: u16,
    #[serde(default)]
    pub include_scoreboard: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct OverlayPlayer {
    pub participant: obs::ParticipantId,
    pub level: Option<u64>,
    pub is_dead: Option<bool>,
    pub respawn_timer: Option<f64>,
    pub spells: [Option<String>; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoreboard: Option<ScoreboardFacts>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct OverlayScores {
    pub kills: Option<u64>,
    pub deaths: Option<u64>,
    pub assists: Option<u64>,
    pub creep_score: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ScoreboardItem {
    pub item_id: Option<u64>,
    pub slot: Option<u64>,
    pub count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ScoreboardFacts {
    pub payload_id: u64,
    pub scores: OverlayScores,
    pub ward_score: Option<f64>,
    pub items: Option<Vec<ScoreboardItem>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct OverlaySample {
    pub replay_tick: ReplayTick,
    pub observation_id: u64,
    pub game_tick: GameTick,
    pub players: Vec<OverlayPlayer>,
    pub local_scores: Option<OverlayScores>,
}

#[derive(Debug, Serialize)]
pub(crate) struct OverlayPage {
    pub source: obs::SourceEpoch,
    pub local_participant: Option<obs::ParticipantId>,
    pub revision: journal::Position,
    pub identities: Option<Vec<ReplayIdentity>>,
    pub samples: Vec<OverlaySample>,
    pub next: Option<OverlayCursor>,
}

impl Index {
    pub(crate) fn compute_overlay_page(
        &self,
        query: &OverlayQuery,
        timeline: &MediaTimelineV2,
    ) -> Result<OverlayPage> {
        ensure!(
            query.scope.media_id == timeline.media_id,
            "overlay media identity"
        );
        ensure!(
            query.limit > 0 && usize::from(query.limit) <= PAGE_SAMPLES,
            "overlay page limit"
        );
        ensure!(
            query.after.is_some() == query.revision.is_some(),
            "overlay continuation revision"
        );
        let coverage = self.whole_source_coverage(&query.scope)?;
        ensure!(
            coverage.index_status == "ready",
            "overlay source is not ready"
        );
        let revision = coverage.committed.context("overlay committed revision")?;
        if let Some(expected) = &query.revision {
            ensure!(*expected == revision, "overlay source revision changed");
        }
        let scope = &query.scope;
        let local = self.local(scope)?;
        let mut output = OverlayPage {
            source: scope.source(),
            local_participant: local,
            revision,
            identities: if query.after.is_none() {
                Some(self.replay_identities(scope)?)
            } else {
                None
            },
            samples: Vec::new(),
            next: None,
        };
        // Reserve space for the continuation and sample separators as well as
        // the first-page identity header; bound work before serializing output.
        let mut bytes = serde_json::to_vec(&output)?.len() + 256;
        let mut statement = self.db.prepare(
            "SELECT observation_id,game_tick,replay_tick FROM rosters
            WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND replay_tick BETWEEN 0 AND ?4
            AND (?5 IS NULL OR (replay_tick,observation_id)>(?5,?6))
            ORDER BY replay_tick,observation_id LIMIT ?7",
        )?;
        let mut rows = statement.query(params![
            scope.media_id.as_str(),
            scope.run_id.as_str(),
            sql_u64(scope.epoch)?,
            sql_u64(timeline.video.replay_end.get())?,
            query
                .after
                .as_ref()
                .map(|c| sql_u64(c.replay_tick.get()))
                .transpose()?,
            query
                .after
                .as_ref()
                .map(|c| sql_u64(c.observation_id))
                .transpose()?,
            u32::from(query.limit) + 1,
        ])?;
        // The ordinary overlay never extracts inventory. TAB opts into compact
        // item facts; descriptions/prices never cross this boundary.
        let mut members = self.db.prepare("SELECT m.participant_id,
            json_extract(p.data,'$.level'),json_extract(p.data,'$.is_dead'),json_extract(p.data,'$.respawn_timer'),
            COALESCE(CAST(NULLIF(json_extract(p.data,'$.summoner_spells.one.id'),0) AS TEXT),
                NULLIF(json_extract(p.data,'$.summoner_spells.one.display_name'),''),NULLIF(json_extract(p.data,'$.summoner_spells.one.raw_display_name'),'')),
            COALESCE(CAST(NULLIF(json_extract(p.data,'$.summoner_spells.two.id'),0) AS TEXT),
                NULLIF(json_extract(p.data,'$.summoner_spells.two.display_name'),''),NULLIF(json_extract(p.data,'$.summoner_spells.two.raw_display_name'),'')),
            json_type(p.data,'$.scores') IS NOT NULL,json_extract(p.data,'$.scores.kills'),json_extract(p.data,'$.scores.deaths'),
            json_extract(p.data,'$.scores.assists'),json_extract(p.data,'$.scores.creep_score'),
            m.payload_id,CASE WHEN ?5 THEN json_extract(p.data,'$.scores.ward_score') END,
            CASE WHEN ?5 THEN CASE WHEN json_type(p.data,'$.items')='array' THEN
                (SELECT json_group_array(json_object('item_id',json_extract(value,'$.item_id'),
                    'slot',json_extract(value,'$.slot'),'count',json_extract(value,'$.count')))
                 FROM json_each(p.data,'$.items')) END END
            FROM roster_members m JOIN roster_payloads p
            ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
            WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4
            ORDER BY m.ordinal LIMIT 65")?;
        // Intern repeated payloads within a page, avoiding repeated item parsing.
        let mut scoreboard_payloads = std::collections::HashMap::new();
        while let Some(row) = rows.next()? {
            if output.samples.len() == usize::from(query.limit) {
                output.next = output.samples.last().map(|s| OverlayCursor {
                    replay_tick: s.replay_tick,
                    observation_id: s.observation_id,
                });
                break;
            }
            let id = unsigned(row, 0)?;
            let mut sample = OverlaySample {
                observation_id: id,
                game_tick: GameTick::new(row.get(1)?)?,
                replay_tick: ReplayTick::new(unsigned(row, 2)?)?,
                players: Vec::new(),
                local_scores: None,
            };
            let mut player_rows = members.query(params![
                scope.media_id.as_str(),
                scope.run_id.as_str(),
                sql_u64(scope.epoch)?,
                sql_u64(id)?,
                query.include_scoreboard,
            ])?;
            let mut count = 0;
            while let Some(player_row) = player_rows.next()? {
                count += 1;
                ensure!(count <= obs::MAX_ROSTER, "overlay roster capacity");
                let Some(participant) = optional_unsigned(player_row, 0)? else {
                    continue;
                };
                let participant = obs::ParticipantId(participant);
                let scores = || -> Result<OverlayScores> {
                    Ok(OverlayScores {
                        kills: optional_unsigned(player_row, 7)?,
                        deaths: optional_unsigned(player_row, 8)?,
                        assists: optional_unsigned(player_row, 9)?,
                        creep_score: optional_unsigned(player_row, 10)?,
                    })
                };
                if Some(participant) == local && player_row.get::<_, bool>(6)? {
                    sample.local_scores = Some(scores()?);
                }
                let scoreboard = if query.include_scoreboard {
                    let payload_id = unsigned(player_row, 11)?;
                    if let std::collections::hash_map::Entry::Vacant(entry) =
                        scoreboard_payloads.entry(payload_id)
                    {
                        let items: Option<String> = player_row.get(13)?;
                        let items = items
                            .map(|json| serde_json::from_str::<Vec<ScoreboardItem>>(&json))
                            .transpose()?;
                        ensure!(
                            items.as_ref().is_none_or(|items| items.len() <= 32),
                            "scoreboard item capacity"
                        );
                        entry.insert(ScoreboardFacts {
                            payload_id,
                            scores: scores()?,
                            ward_score: player_row.get(12)?,
                            items,
                        });
                    }
                    Some(scoreboard_payloads[&payload_id].clone())
                } else {
                    None
                };
                sample.players.push(OverlayPlayer {
                    participant,
                    level: optional_unsigned(player_row, 1)?,
                    is_dead: player_row.get(2)?,
                    respawn_timer: player_row.get(3)?,
                    spells: [player_row.get(4)?, player_row.get(5)?],
                    scoreboard,
                });
            }
            let size = serde_json::to_vec(&sample)?.len() + 1;
            ensure!(
                bytes + size <= PAGE_BYTES || !output.samples.is_empty(),
                "overlay sample response capacity"
            );
            if bytes + size > PAGE_BYTES {
                output.next = output.samples.last().map(|s| OverlayCursor {
                    replay_tick: s.replay_tick,
                    observation_id: s.observation_id,
                });
                break;
            }
            bytes += size;
            output.samples.push(sample);
        }
        ensure!(
            serde_json::to_vec(&output)?.len() <= PAGE_BYTES,
            "overlay page response capacity"
        );
        Ok(output)
    }
}

impl Index {
    /// UI reads prepared rows; reconstruction belongs exclusively to preparation.
    pub(crate) fn overlay_page(
        &self,
        query: &OverlayQuery,
        timeline: &MediaTimelineV2,
    ) -> Result<OverlayPage> {
        ensure!(
            query.scope.media_id == timeline.media_id
                && query.limit > 0
                && usize::from(query.limit) <= PAGE_SAMPLES,
            "overlay request"
        );
        let coverage = self.whole_source_coverage(&query.scope)?;
        let revision = coverage.committed.context("overlay revision pending")?;
        ensure!(
            query.after.is_some() == query.revision.is_some()
                && query.revision.as_ref().is_none_or(|r| r == &revision),
            "overlay revision changed"
        );
        let scope = &query.scope;
        let saved: (String,bool)=self.db.query_row("SELECT revision,complete FROM prepared_overlay_sources WHERE media_id=?1 AND run_id=?2 AND epoch=?3",params![scope.media_id.as_str(),scope.run_id.as_str(),sql_u64(scope.epoch)?],|r|Ok((r.get(0)?,r.get(1)?)))?;
        ensure!(
            saved.1 && saved.0 == serde_json::to_string(&revision)?,
            "overlay preparation pending"
        );
        let mut out = OverlayPage {
            source: scope.source(),
            local_participant: self.local(scope)?,
            revision,
            identities: if query.after.is_none() {
                Some(self.replay_identities(scope)?)
            } else {
                None
            },
            samples: Vec::new(),
            next: None,
        };
        let mut statement=self.db.prepare("SELECT data FROM prepared_overlay_samples WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND (?4 IS NULL OR (replay_tick,observation_id)>(?4,?5)) ORDER BY replay_tick,observation_id LIMIT ?6")?;
        let mut rows = statement.query(params![
            scope.media_id.as_str(),
            scope.run_id.as_str(),
            sql_u64(scope.epoch)?,
            query
                .after
                .as_ref()
                .map(|c| sql_u64(c.replay_tick.get()))
                .transpose()?,
            query
                .after
                .as_ref()
                .map(|c| sql_u64(c.observation_id))
                .transpose()?,
            u32::from(query.limit) + 1
        ])?;
        let mut bytes = serde_json::to_vec(&out)?.len() + 256;
        while let Some(row) = rows.next()? {
            let data: String = row.get(0)?;
            let mut sample: OverlaySample = serde_json::from_str(&data)?;
            if !query.include_scoreboard {
                for player in &mut sample.players {
                    player.scoreboard = None;
                }
            }
            let size = serde_json::to_vec(&sample)?.len() + 1;
            if out.samples.len() == usize::from(query.limit) || bytes + size > PAGE_BYTES {
                ensure!(!out.samples.is_empty(), "overlay response capacity");
                out.next = out.samples.last().map(|s| OverlayCursor {
                    replay_tick: s.replay_tick,
                    observation_id: s.observation_id,
                });
                break;
            }
            bytes += size;
            out.samples.push(sample);
        }
        Ok(out)
    }
    /// Prepare one bounded page per call, with restart-safe progress.
    pub(crate) fn prepare_overlay(&mut self, timeline: &MediaTimelineV2) -> Result<bool> {
        let scopes = self.scopes(&timeline.media_id)?;
        for source in scopes {
            let scope = source.source;
            let query = ScopeQuery {
                media_id: scope.media_id,
                run_id: scope.run_id,
                epoch: scope.epoch,
            };
            let coverage = self.whole_source_coverage(&query)?;
            if coverage.index_status != "ready" {
                return Ok(false);
            }
            let revision = coverage.committed.context("prepare revision")?;
            let revision_json = serde_json::to_string(&revision)?;
            let prior:Option<(String,bool,Option<String>)>=self.db.query_row("SELECT revision,complete,cursor FROM prepared_overlay_sources WHERE media_id=?1 AND run_id=?2 AND epoch=?3",params![query.media_id.as_str(),query.run_id.as_str(),sql_u64(query.epoch)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if prior.as_ref().is_some_and(|p| p.0 == revision_json && p.1) {
                continue;
            }
            let current = prior.as_ref().is_some_and(|p| p.0 == revision_json);
            let after = if current {
                prior
                    .and_then(|p| p.2)
                    .map(|c| serde_json::from_str::<OverlayCursor>(&c))
                    .transpose()?
            } else {
                None
            };
            let page = self.compute_overlay_page(
                &OverlayQuery {
                    scope: query.clone(),
                    revision: after.as_ref().map(|_| revision.clone()),
                    after,
                    limit: 128,
                    include_scoreboard: true,
                },
                timeline,
            )?;
            let tx = self.db.transaction()?;
            if !current {
                tx.execute("DELETE FROM prepared_overlay_samples WHERE media_id=?1 AND run_id=?2 AND epoch=?3",params![query.media_id.as_str(),query.run_id.as_str(),sql_u64(query.epoch)?])?;
            }
            for sample in &page.samples {
                tx.execute(
                    "INSERT OR REPLACE INTO prepared_overlay_samples VALUES (?1,?2,?3,?4,?5,?6)",
                    params![
                        query.media_id.as_str(),
                        query.run_id.as_str(),
                        sql_u64(query.epoch)?,
                        sql_u64(sample.replay_tick.get())?,
                        sql_u64(sample.observation_id)?,
                        serde_json::to_string(sample)?
                    ],
                )?;
            }
            tx.execute(
                "INSERT OR REPLACE INTO prepared_overlay_sources VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    query.media_id.as_str(),
                    query.run_id.as_str(),
                    sql_u64(query.epoch)?,
                    revision_json,
                    page.next.is_none(),
                    page.next.map(|c| serde_json::to_string(&c)).transpose()?
                ],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        Ok(true)
    }
}
