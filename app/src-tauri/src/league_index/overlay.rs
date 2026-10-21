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
}

#[derive(Debug, Serialize)]
pub(crate) struct OverlayPlayer {
    pub participant: obs::ParticipantId,
    pub level: Option<u64>,
    pub is_dead: Option<bool>,
    pub respawn_timer: Option<f64>,
    pub spells: [Option<String>; 2],
}

#[derive(Debug, Serialize)]
pub(crate) struct OverlayScores {
    pub kills: Option<u64>,
    pub deaths: Option<u64>,
    pub assists: Option<u64>,
    pub creep_score: Option<u64>,
}

#[derive(Debug, Serialize)]
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
    pub(crate) fn overlay_page(
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
        // Project only overlay fields in SQLite, avoiding raw descriptions and
        // item payload parsing for every member of every sample.
        let mut members = self.db.prepare("SELECT m.participant_id,
            json_extract(p.data,'$.level'),json_extract(p.data,'$.is_dead'),json_extract(p.data,'$.respawn_timer'),
            COALESCE(CAST(NULLIF(json_extract(p.data,'$.summoner_spells.one.id'),0) AS TEXT),
                NULLIF(json_extract(p.data,'$.summoner_spells.one.display_name'),''),NULLIF(json_extract(p.data,'$.summoner_spells.one.raw_display_name'),'')),
            COALESCE(CAST(NULLIF(json_extract(p.data,'$.summoner_spells.two.id'),0) AS TEXT),
                NULLIF(json_extract(p.data,'$.summoner_spells.two.display_name'),''),NULLIF(json_extract(p.data,'$.summoner_spells.two.raw_display_name'),'')),
            json_type(p.data,'$.scores') IS NOT NULL,json_extract(p.data,'$.scores.kills'),json_extract(p.data,'$.scores.deaths'),
            json_extract(p.data,'$.scores.assists'),json_extract(p.data,'$.scores.creep_score')
            FROM roster_members m JOIN roster_payloads p
            ON p.media_id=m.media_id AND p.run_id=m.run_id AND p.epoch=m.epoch AND p.payload_id=m.payload_id
            WHERE m.media_id=?1 AND m.run_id=?2 AND m.epoch=?3 AND m.observation_id=?4
            ORDER BY m.ordinal LIMIT 65")?;
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
                sql_u64(id)?
            ])?;
            let mut count = 0;
            while let Some(player_row) = player_rows.next()? {
                count += 1;
                ensure!(count <= obs::MAX_ROSTER, "overlay roster capacity");
                let Some(participant) = optional_unsigned(player_row, 0)? else {
                    continue;
                };
                let participant = obs::ParticipantId(participant);
                if Some(participant) == local && player_row.get::<_, bool>(6)? {
                    sample.local_scores = Some(OverlayScores {
                        kills: optional_unsigned(player_row, 7)?,
                        deaths: optional_unsigned(player_row, 8)?,
                        assists: optional_unsigned(player_row, 9)?,
                        creep_score: optional_unsigned(player_row, 10)?,
                    });
                }
                sample.players.push(OverlayPlayer {
                    participant,
                    level: optional_unsigned(player_row, 1)?,
                    is_dead: player_row.get(2)?,
                    respawn_timer: player_row.get(3)?,
                    spells: [player_row.get(4)?, player_row.get(5)?],
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
