use super::*;
use crate::playback_file::{self, ApprovedRoot, FileScope};
use obs::{Evidence, Observation, Payload, Record};

impl Index {
    pub(super) fn import(
        &mut self,
        root: &Path,
        directory: &Path,
        timeline: &chronobreak_replay_time::MediaTimelineV2,
        stamp: &str,
        check: &(impl Fn() -> Result<()> + ?Sized),
    ) -> Result<ImportProgress> {
        let media = &timeline.media_id;
        let manifest = manifest::Manifest::decode(
            &bounded(&directory.join(manifest::FILE), manifest::MAX_BYTES)?,
            media,
        )?;
        let header = journal::Header::new(&obs::SourceEpoch {
            media_id: media.clone(),
            run_id: manifest.run_id.clone(),
            epoch: 0,
        });
        let path = directory.join(JOURNAL);
        let meta = fs::symlink_metadata(&path)?;
        ensure!(
            meta.is_file() && !meta.is_symlink() && meta.len() <= MAX_JOURNAL,
            "optional journal size/type"
        );
        let approved = ApprovedRoot::new(root)?;
        let file = playback_file::open_file(&path, FileScope::Root(&approved))?;
        let identity = playback_file::file_facts(&file)?.generation_key();
        let cached: (Option<String>,Option<String>,Option<String>,Option<String>) = self.db.prepare_cached("SELECT journal_header,source_identity,position_json,checkpoint_json FROM recordings WHERE media_id=?1")?.query_row(
            [media.as_str()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
        )?;
        let restored = (|| -> Result<checkpoint::State> {
            ensure!(
                cached.1.as_deref() == Some(&identity),
                "index source generation"
            );
            let prior: journal::Header =
                serde_json::from_str(cached.0.as_deref().context("cached header")?)?;
            let position: journal::Position =
                serde_json::from_str(cached.2.as_deref().context("cached position")?)?;
            ensure!(
                prior == header && position.end_offset <= file.metadata()?.len(),
                "index source replaced/truncated"
            );
            let cp: checkpoint::Checkpoint =
                serde_json::from_str(cached.3.as_deref().context("cached checkpoint")?)?;
            Ok(checkpoint::State::from_checkpoint(cp, &header, &position)?)
        })();
        let mut state = match restored {
            Ok(state) => state,
            Err(_) => {
                self.clear_telemetry(media)?;
                // Invalidating journal evidence does not invalidate the card
                // facts just checked by history_recording.
                self.refresh_history_summary(media)?;
                checkpoint::State::new(header.clone(), header.initial_position()?)
            }
        };
        let start = state.snapshot().position.clone();
        let mut batches = 0;
        let mut failure = None;
        // Publish one bounded unit atomically. Each source commit is still
        // checksum-validated and applied in order, but the current checkpoint
        // only needs to be persisted once at the unit's final committed prefix.
        let tx = self.db.transaction()?;
        let page = journal::recover_after(
            BufReader::new(file),
            &header,
            &start,
            IMPORT_BATCHES,
            |entries, position| {
                let imported = (|| -> Result<()> {
                    check()?;
                    let records: Vec<_> = entries.iter().map(|e| e.record.clone()).collect();
                    state.apply(&records, position)?;
                    let source_epoch = state.source().epoch;
                    for entry in entries {
                        import_record(&tx, entry, timeline)?;
                    }
                    tx.prepare_cached("INSERT INTO commits(media_id,sequence,position_json,epoch) VALUES (?1,?2,?3,?4)")?.execute(
                    params![media.as_str(),sql_u64(position.last_sequence)?,serde_json::to_string(position)?,sql_u64(source_epoch)?])?;
                    check()?;
                    batches += 1;
                    Ok(())
                })();
                if let Err(error) = imported {
                    failure = Some(error);
                    return Err(journal::Error::Invalid("index import"));
                }
                Ok(())
            },
        );
        if let Some(error) = failure {
            return Err(error);
        }
        let page = page?;
        check()?;
        // A valid stale manifest is allowed, but its exact commit and epoch must
        // have been validated. Importing beyond it does not invent a new anchor.
        let anchor = if manifest.committed.last_sequence == 0 {
            manifest.committed == header.initial_position()? && manifest.source_epoch == 0
        } else {
            let cached: Option<(String, u64)> = tx
                .query_row(
                    "SELECT position_json,epoch FROM commits WHERE media_id=?1 AND sequence=?2",
                    params![media.as_str(), sql_u64(manifest.committed.last_sequence)?],
                    |r| Ok((r.get(0)?, unsigned(r, 1)?)),
                )
                .optional()?;
            cached.is_some_and(|(json, epoch)| {
                epoch == manifest.source_epoch
                    && serde_json::from_str::<journal::Position>(&json)
                        .is_ok_and(|p| p == manifest.committed)
            })
        };
        if page.complete {
            ensure!(anchor, "optional telemetry manifest anchor");
        }
        let current = playback_file::open_file(&path, FileScope::Root(&approved))?;
        ensure!(
            playback_file::file_facts(&current)?.generation_key() == identity,
            "journal replaced during import"
        );
        let unchanged = source_stamp(directory)? == stamp;
        let ready = page.complete && anchor && unchanged;
        // Empty journals still need a resumable, validated initial checkpoint.
        let cp = state.snapshot();
        if ready {
            super::actors::refresh(&tx, media)?;
            // One summary per completed projection, not a whole-history rescan
            // after every bounded import batch. Readers of pending projections
            // retain the original live coverage calculation.
            super::coverage::refresh_recording(&tx, media)?;
        }
        tx.prepare_cached("UPDATE recordings SET journal_header=?2,source_identity=?3,position_json=?4,checkpoint_json=?5,telemetry_status=?6,tail_bytes=?7,source_stamp=?8 WHERE media_id=?1")?.execute(
            params![media.as_str(),serde_json::to_string(&header)?,identity,serde_json::to_string(&page.recovery.position)?,
                std::str::from_utf8(&cp.encode()?)?,if ready {"ready"} else {"pending"},sql_u64(page.recovery.tail_bytes)?,
                ready.then_some(stamp)])?;
        check()?;
        tx.commit()?;
        self.progress(media, batches)
    }

    pub(super) fn final_facts(
        &mut self,
        media: &MediaId,
        result: Option<&ResultFile>,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.prepare_cached("DELETE FROM final_players WHERE media_id=?1")?
            .execute([media.as_str()])?;
        tx.prepare_cached("DELETE FROM final_stats WHERE media_id=?1")?
            .execute([media.as_str()])?;
        if let Some(result) = result {
            if let Some(local) = &result.source_facts.local_player {
                final_player(&tx, media, "local", local, true)?;
            }
            for (team_index, team) in result.source_facts.teams.iter().flatten().enumerate() {
                let key = format!("team/{team_index}");
                if let Some(stats) = &team.stats {
                    final_stats(&tx, media, &key, stats)?;
                }
                for (index, player) in team.players.iter().flatten().enumerate() {
                    final_player(
                        &tx,
                        media,
                        &format!("{key}/player/{index}"),
                        player,
                        player.is_local_player == Some(true),
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

fn evidence(db: &Connection, e: &Evidence, tick: Option<GameTick>, observed: bool) -> Result<()> {
    let s = &e.source;
    // Observation IDs are run-local u64 values; acquisition starts at zero.
    ensure!(e.timing.start_us <= e.timing.end_us, "observation interval");
    db.prepare_cached("INSERT OR IGNORE INTO scopes(media_id,run_id,epoch) VALUES (?1,?2,?3)")?
        .execute(params![
            s.media_id.as_str(),
            s.run_id.as_str(),
            sql_u64(s.epoch)?
        ])?;
    let json = serde_json::to_string(e)?;
    let inserted = db
        .prepare_cached(
            "INSERT INTO evidence(media_id,run_id,epoch,observation_id,endpoint,start_us,end_us,
            game_tick,game_stats_id,calibration_id,data,observed)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(media_id,run_id,epoch,observation_id) DO NOTHING",
        )?
        .execute(params![
            s.media_id.as_str(),
            s.run_id.as_str(),
            sql_u64(s.epoch)?,
            sql_u64(e.id.0)?,
            name(&e.endpoint)?,
            sql_u64(e.timing.start_us)?,
            sql_u64(e.timing.end_us)?,
            tick.map(GameTick::get),
            e.game_stats.map(|id| sql_u64(id.0)).transpose()?,
            e.calibration.map(|id| sql_u64(id.0)).transpose()?,
            json,
            observed,
        ])?;
    if inserted == 0 {
        let (previous, was_observed): (String, bool) = db.prepare_cached(
            "SELECT data,observed FROM evidence WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND observation_id=?4",
        )?.query_row(params![s.media_id.as_str(), s.run_id.as_str(), sql_u64(s.epoch)?, sql_u64(e.id.0)?],
            |r| Ok((r.get(0)?, r.get(1)?)))?;
        ensure!(
            previous == json,
            "observation ID reused with different evidence"
        );
        if tick.is_some() || (observed && !was_observed) {
            db.prepare_cached(
                "UPDATE evidence SET game_tick=COALESCE(?5,game_tick),observed=?6
                 WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND observation_id=?4",
            )?
            .execute(params![
                s.media_id.as_str(),
                s.run_id.as_str(),
                sql_u64(s.epoch)?,
                sql_u64(e.id.0)?,
                tick.map(GameTick::get),
                observed || was_observed
            ])?;
        }
    }
    Ok(())
}

fn referenced_tick(db: &Connection, e: &Evidence) -> Result<Option<i64>> {
    let Some(id) = e.game_stats else {
        return Ok(None);
    };
    Ok(db.prepare_cached("SELECT game_tick FROM evidence WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND observation_id=?4 AND endpoint='game_stats'")?.query_row(
        params![e.source.media_id.as_str(),e.source.run_id.as_str(),sql_u64(e.source.epoch)?,sql_u64(id.0)?], |r| r.get(0)).optional()?.flatten())
}

fn projected_tick(
    db: &Connection,
    e: &Evidence,
    tick: Option<GameTick>,
    timeline: &chronobreak_replay_time::MediaTimelineV2,
) -> Result<Option<i64>> {
    let (Some(tick), Some(id)) = (tick, e.calibration) else {
        return Ok(None);
    };
    let json:Option<String>=db.prepare_cached("SELECT data FROM calibrations WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND observation_id=?4")?.query_row(
        params![e.source.media_id.as_str(),e.source.run_id.as_str(),sql_u64(e.source.epoch)?,sql_u64(id.0)?],|r|r.get(0)).optional()?;
    let calibration = json
        .map(|json| serde_json::from_str::<chronobreak_replay_time::GameCalibrationV2>(&json))
        .transpose()?;
    Ok(
        match library::map_game_tick(calibration.as_ref(), tick, timeline)? {
            chronobreak_replay_time::MappedReplayTime::InsideMedia { replay_tick, .. } => {
                Some(sql_u64(replay_tick.get())?)
            }
            _ => None,
        },
    )
}

fn import_record(
    db: &Connection,
    entry: &journal::Entry,
    timeline: &chronobreak_replay_time::MediaTimelineV2,
) -> Result<()> {
    let sequence = sql_u64(entry.sequence)?;
    let e = match &entry.record {
        Record::Observation { observation } => match observation.as_ref() {
            Observation::Success { evidence, .. } | Observation::Gap { evidence, .. } => evidence,
        },
        Record::EventObservation { evidence, .. }
        | Record::RosterObservation { evidence, .. }
        | Record::Calibration { evidence, .. }
        | Record::CalibrationInvalidated { evidence }
        | Record::Boundary { evidence, .. }
        | Record::SourceReset { evidence, .. }
        | Record::LocalParticipant { evidence, .. } => evidence,
        Record::GapInterval { last, .. } => last,
        Record::IdentityRound { round } => &round.active_player,
    };
    let s = &e.source;
    let tick = match &entry.record {
        Record::Observation { observation } => match observation.as_ref() {
            Observation::Success {
                payload: Payload::GameStats(gs),
                ..
            } => gs.game_tick,
            _ => None,
        },
        _ => None,
    };
    let observed = matches!(
        &entry.record,
        Record::EventObservation { .. } | Record::RosterObservation { .. }
    ) || matches!(&entry.record, Record::Observation { observation } if matches!(observation.as_ref(), Observation::Success { .. }));
    evidence(db, e, tick, observed)?;
    match &entry.record {
        Record::Observation { observation } => match observation.as_ref() {
            Observation::Gap { reason, .. } => gap(db, sequence, e, e, 1, reason, &entry.record)?,
            Observation::Success {
                payload: Payload::PlayerList(_) | Payload::Events(_),
                ..
            } => bail!("unshared telemetry input"),
            _ => {}
        },
        Record::Calibration { calibration, .. } => {
            db.prepare_cached("INSERT INTO calibrations VALUES (?1,?2,?3,?4,?5)")?
                .execute(params![
                    s.media_id.as_str(),
                    s.run_id.as_str(),
                    sql_u64(s.epoch)?,
                    sql_u64(e.id.0)?,
                    serde_json::to_string(calibration)?
                ])?;
        }
        Record::LocalParticipant { participant, .. } => {
            let prior: Option<u64> = db.prepare_cached("SELECT local_participant FROM scopes WHERE media_id=?1 AND run_id=?2 AND epoch=?3")?.query_row(
                params![s.media_id.as_str(), s.run_id.as_str(), sql_u64(s.epoch)?],
                |r| optional_unsigned(r, 0),
            )?;
            ensure!(
                prior.is_none_or(|p| p == participant.0),
                "local participant changed in epoch"
            );
            db.prepare_cached("UPDATE scopes SET local_participant=?4 WHERE media_id=?1 AND run_id=?2 AND epoch=?3")?.execute(
                params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sql_u64(participant.0)?])?;
        }
        Record::IdentityRound { round } => {
            if let Some(gs) = &round.game_stats {
                evidence(db, gs, None, false)?;
            }
        }
        Record::GapInterval {
            first,
            last,
            attempts,
            reason,
        } => {
            ensure!(
                first.source == last.source
                    && first.endpoint == last.endpoint
                    && first.timing.start_us <= last.timing.end_us
                    && *attempts > 0,
                "gap interval scope"
            );
            evidence(db, first, None, false)?;
            gap(db, sequence, first, last, *attempts, reason, &entry.record)?;
        }
        Record::RosterObservation { roster, .. } => {
            let tick = referenced_tick(db, e)?;
            for payload in &roster.payloads {
                let json = serde_json::to_string(&payload.player)?;
                let previous: Option<String> = db.prepare_cached("SELECT data FROM roster_payloads WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND payload_id=?4")?.query_row(
                    params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sql_u64(payload.id.0)?], |r| r.get(0)).optional()?;
                ensure!(
                    previous.as_ref().is_none_or(|p| p == &json),
                    "roster payload ID changed"
                );
                db.prepare_cached("INSERT OR IGNORE INTO roster_payloads VALUES (?1,?2,?3,?4,?5)")?
                    .execute(params![
                        s.media_id.as_str(),
                        s.run_id.as_str(),
                        sql_u64(s.epoch)?,
                        sql_u64(payload.id.0)?,
                        json
                    ])?;
                if let Some(id) = payload.player.participant {
                    let p = &payload.player;
                    // Identity is exact and immutable; champion/team are observed
                    // descriptors and may change without creating a participant.
                    let previous: Option<Option<String>> = db.prepare_cached("SELECT riot_id FROM participants WHERE media_id=?1 AND run_id=?2 AND epoch=?3 AND participant_id=?4")?.query_row(
                        params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sql_u64(id.0)?], |r| r.get(0)).optional()?;
                    ensure!(
                        previous
                            .flatten()
                            .as_ref()
                            .is_none_or(|prior| Some(prior) == p.riot_id.as_ref()),
                        "participant identity changed"
                    );
                    db.prepare_cached("INSERT INTO participants VALUES (?1,?2,?3,?4,?5,?6,?7)
                        ON CONFLICT(media_id,run_id,epoch,participant_id) DO UPDATE SET riot_id=COALESCE(excluded.riot_id,riot_id),champion=excluded.champion,team=excluded.team")?.execute(
                        params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sql_u64(id.0)?,p.riot_id,p.champion_name,p.team])?;
                }
            }
            let replay_tick =
                projected_tick(db, e, tick.map(GameTick::new).transpose()?, timeline)?;
            db.prepare_cached("INSERT INTO rosters VALUES (?1,?2,?3,?4,?5,?6)")?
                .execute(params![
                    s.media_id.as_str(),
                    s.run_id.as_str(),
                    sql_u64(s.epoch)?,
                    sql_u64(e.id.0)?,
                    tick,
                    replay_tick
                ])?;
            // Resolve the whole roster in one statement. Payloads have already
            // been validated and serialized when inserted into the dictionary.
            // Drive the join from the references, using one dictionary lookup
            // per ID instead of scanning the growing dictionary for each roster.
            let references = serde_json::to_string(&roster.references)?;
            let inserted = db
                .prepare_cached(
                    "INSERT INTO roster_members
                 SELECT ?1,?2,?3,?4,r.key,json_extract(p.data,'$.participant'),p.payload_id,?5
                 FROM json_each(?6) r CROSS JOIN roster_payloads p
                   ON p.media_id=?1 AND p.run_id=?2 AND p.epoch=?3 AND p.payload_id=r.value
                 WHERE r.type='integer' AND typeof(r.value)='integer'
                   AND (json_type(p.data,'$.participant') IS NULL
                     OR typeof(json_extract(p.data,'$.participant')) IN ('null','integer'))",
                )?
                .execute(params![
                    s.media_id.as_str(),
                    s.run_id.as_str(),
                    sql_u64(s.epoch)?,
                    sql_u64(e.id.0)?,
                    tick,
                    references
                ])?;
            ensure!(
                inserted == roster.references.len(),
                "missing or invalid roster payload"
            );
        }
        Record::EventObservation { revisions, .. } => {
            let mut lookup_error = None;
            obs::events::validate_committed_revisions(s, revisions, |id| {
              let value: Result<Option<Vec<u8>>> = (|| Ok(db.prepare_cached("SELECT r.response_fingerprint FROM events e JOIN event_revisions r ON r.media_id=e.media_id AND r.sequence=e.sequence AND r.ordinal=e.ordinal WHERE e.media_id=?1 AND e.run_id=?2 AND e.epoch=?3 AND e.source_id=?4")?.query_row(
                  params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sql_u64(id)?], |r| r.get(0)).optional()?))();
              match value {
                  Ok(Some(value)) => value.try_into().map(Some).map_err(|_| obs::GapReason::InvalidShape),
                  Ok(None) => Ok(None),
                  Err(error) => { lookup_error = Some(error); Err(obs::GapReason::InvalidShape) }
              }
          }).map_err(|reason| lookup_error.unwrap_or_else(|| anyhow::anyhow!("event revision contract: {reason:?}")))?;
            for (ordinal, revision) in revisions.iter().enumerate() {
                let ordinal = ordinal as i64;
                let event = &revision.event;
                let source_id = event.source_id.map(sql_u64).transpose()?;
                db.prepare_cached("INSERT INTO event_revisions VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)")?.execute(
                params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,sequence,ordinal,source_id,
                    &revision.fingerprint[..],&revision.response_fingerprint[..],sql_u64(e.id.0)?,event.game_tick.map(GameTick::get),event.name,event.stolen,
                    event.kill_streak.map(sql_u64).transpose()?,name(&revision.ingestion)?,serde_json::to_string(revision)?])?;
                if let Some(id) = source_id {
                    let replay_tick = projected_tick(db, e, event.game_tick, timeline)?;
                    db.prepare_cached("INSERT INTO events VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
                    ON CONFLICT(media_id,run_id,epoch,source_id) DO UPDATE SET sequence=excluded.sequence,ordinal=excluded.ordinal,game_tick=excluded.game_tick,
                    kind=excluded.kind,stolen=excluded.stolen,kill_streak=excluded.kill_streak,ingestion=excluded.ingestion,replay_tick=excluded.replay_tick")?.execute(
                    params![s.media_id.as_str(),s.run_id.as_str(),sql_u64(s.epoch)?,id,sequence,ordinal,event.game_tick.map(GameTick::get),event.name,event.stolen,
                        event.kill_streak.map(sql_u64).transpose()?,name(&revision.ingestion)?,replay_tick])?;
                }
                for (role, actors) in event.actors.iter().flat_map(|roles| roles.iter()) {
                    for (actor_ordinal, actor) in actors.iter().enumerate() {
                        db.prepare_cached(
                            "INSERT INTO event_actors VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                        )?
                        .execute(params![
                            s.media_id.as_str(),
                            s.run_id.as_str(),
                            sql_u64(s.epoch)?,
                            sequence,
                            ordinal,
                            name(role)?,
                            actor_ordinal as i64,
                            actor.participant.map(|p| sql_u64(p.0)).transpose()?,
                            name(&actor.status)?
                        ])?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn gap(
    db: &Connection,
    sequence: i64,
    first: &Evidence,
    last: &Evidence,
    attempts: u64,
    reason: &obs::GapReason,
    record: &Record,
) -> Result<()> {
    let s = &last.source;
    db.prepare_cached("INSERT INTO gaps VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?
        .execute(params![
            s.media_id.as_str(),
            s.run_id.as_str(),
            sql_u64(s.epoch)?,
            sequence,
            name(&last.endpoint)?,
            sql_u64(first.timing.start_us)?,
            sql_u64(last.timing.end_us)?,
            referenced_tick(db, last)?,
            sql_u64(attempts)?,
            name(reason)?,
            serde_json::to_string(record)?
        ])?;
    Ok(())
}

fn final_player(
    db: &Connection,
    media: &MediaId,
    key: &str,
    player: &obs::FinalPlayer,
    local: bool,
) -> Result<()> {
    let riot = player
        .riot_id_game_name
        .as_ref()
        .zip(player.riot_id_tag_line.as_ref())
        .map(|(name, tag)| format!("{name}#{tag}"))
        .filter(|s| obs::decode::full_riot_id(s));
    db.prepare_cached("INSERT INTO final_players VALUES (?1,?2,?3,?4,?5,?6,?7,?8)")?
        .execute(params![
            media.as_str(),
            key,
            riot,
            player.puuid,
            player.team_id.map(sql_u64).transpose()?,
            player.champion_id.map(sql_u64).transpose()?,
            local,
            serde_json::to_string(player)?
        ])?;
    if let Some(stats) = &player.stats {
        final_stats(db, media, key, stats)?;
    }
    Ok(())
}
fn final_stats(db: &Connection, media: &MediaId, key: &str, stats: &obs::FinalStats) -> Result<()> {
    for stat in Stats::KEYS
        .iter()
        .copied()
        .chain(stats.extras.keys().map(String::as_str))
    {
        let value = stats
            .stats
            .get(stat)
            .or_else(|| stats.extras.get(stat).copied());
        let issue = stats
            .issues
            .get(stat)
            .map(serde_json::to_string)
            .transpose()?;
        if value.is_none() && issue.is_none() {
            continue;
        }
        db.prepare_cached("INSERT INTO final_stats VALUES (?1,?2,?3,?4,?5,?6)")?
            .execute(params![
                media.as_str(),
                key,
                stat,
                value.map(sql_u64).transpose()?,
                issue,
                serde_json::to_string(&stats.aliases.get(stat))?
            ])?;
    }
    // Invalid special fields have no value and therefore no extras entry.
    for (stat, issue) in &stats.issues {
        if Stats::KEYS.contains(&stat.as_str()) {
            continue;
        }
        db.prepare_cached("INSERT INTO final_stats VALUES (?1,?2,?3,NULL,?4,?5)")?
            .execute(params![
                media.as_str(),
                key,
                stat,
                serde_json::to_string(issue)?,
                serde_json::to_string(&stats.aliases.get(stat))?
            ])?;
    }
    Ok(())
}
