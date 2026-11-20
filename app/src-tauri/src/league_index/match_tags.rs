//! Match tags are a small derived projection over verified local event facts.
//! User-authored additions and suppressions live separately and are applied at
//! query time, so re-importing a source can never erase them.
use super::*;

pub(super) fn refresh_recording(db: &Connection, media: &MediaId) -> Result<()> {
    db.execute(
        "DELETE FROM match_tag_observed WHERE media_id=?1",
        [media.as_str()],
    )?;
    let candidates = {
        let mut statement = db.prepare_cached(
            "SELECT e.kind,e.kill_streak,e.stolen,a.role
             FROM events e
             JOIN effective_event_actors a ON a.media_id=e.media_id
                AND a.run_id=e.run_id AND a.epoch=e.epoch
                AND a.sequence=e.sequence AND a.revision_ordinal=e.ordinal
             JOIN scopes s ON s.media_id=e.media_id AND s.run_id=e.run_id AND s.epoch=e.epoch
                AND a.participant_id=s.local_participant
             WHERE e.media_id=?1 AND e.ingestion IN ('accepted','correction')
                AND a.status='resolved'
                AND NOT EXISTS(SELECT 1 FROM event_actors other
                    WHERE other.media_id=a.media_id AND other.sequence=a.sequence
                      AND other.revision_ordinal=a.revision_ordinal
                      AND other.role=a.role AND other.ordinal<>a.ordinal)
                AND e.kind IN ('Multikill','FirstBlood','DragonKill','BaronKill','HeraldKill')",
        )?;
        let mut rows = statement.query([media.as_str()])?;
        let mut result = std::collections::HashSet::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            let Some(role) = crate::tags::event_role(&kind) else {
                continue;
            };
            if name(&role)? != row.get::<_, String>(3)? {
                continue;
            }
            let streak = optional_unsigned(row, 1)?.and_then(|value| u32::try_from(value).ok());
            let stolen: Option<bool> = row.get(2)?;
            if let Some(tag) = crate::tags::event_tag(&kind, streak, stolen) {
                result.insert(tag.id);
            }
        }
        result
    };
    let mut insert =
        db.prepare_cached("INSERT INTO match_tag_observed(media_id,tag_id) VALUES (?1,?2)")?;
    for tag in candidates {
        insert.execute(params![media.as_str(), tag.as_str()])?;
    }
    Ok(())
}

pub(super) fn refresh_dirty(db: &Connection) -> Result<()> {
    let mut query = db.prepare_cached("SELECT match_group FROM match_tag_dirty")?;
    let groups = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for group in groups {
        refresh_group(db, &group)?;
    }
    Ok(())
}

pub(super) fn refresh_group_for_media(db: &Connection, media: &MediaId) -> Result<()> {
    let group: Option<String> = db.query_row(
        "SELECT match_group FROM recordings WHERE media_id=?1",
        [media.as_str()],
        |row| row.get(0),
    )?;
    if let Some(group) = group {
        refresh_group(db, &group)?;
    }
    Ok(())
}

pub(super) fn refresh_group(db: &Connection, match_group: &str) -> Result<()> {
    db.execute(
        "DELETE FROM match_tag_dirty WHERE match_group=?1",
        [match_group],
    )?;
    let mut detected = Vec::new();
    let mut detected_statement = db.prepare_cached(
        "SELECT DISTINCT o.tag_id FROM match_tag_observed o
         JOIN recordings r ON r.media_id=o.media_id
         WHERE r.match_group=?1 AND r.facts_current=1 AND r.telemetry_status='ready'",
    )?;
    let mut rows = detected_statement.query([match_group])?;
    while let Some(row) = rows.next()? {
        detected.push(crate::tags::TagId::parse(&row.get::<_, String>(0)?)?);
    }
    let mut overrides = crate::tags::MatchOverrides::default();
    let mut override_statement = db.prepare_cached(
        "SELECT tag_id,added,suppressed FROM match_tag_overrides WHERE match_group=?1 ORDER BY tag_id",
    )?;
    let mut rows = override_statement.query([match_group])?;
    while let Some(row) = rows.next()? {
        let id = crate::tags::TagId::parse(&row.get::<_, String>(0)?)?;
        if row.get(1)? {
            overrides.added.push(id.clone());
        }
        if row.get(2)? {
            overrides.suppressed.push(id);
        }
    }
    overrides.validate()?;
    db.execute(
        "DELETE FROM match_tag_effective WHERE match_group=?1",
        [match_group],
    )?;
    let mut insert =
        db.prepare_cached("INSERT INTO match_tag_effective(match_group,tag_id) VALUES (?1,?2)")?;
    for id in crate::tags::effective_ids(detected, &overrides) {
        insert.execute(params![match_group, id.as_str()])?;
    }
    Ok(())
}

impl Index {
    pub(crate) fn effective_match_tags(
        &self,
        match_group: &str,
    ) -> Result<Vec<crate::tags::TagDefinition>> {
        let mut statement = self.db.prepare_cached(
            "SELECT tag_id FROM match_tag_effective WHERE match_group=?1 ORDER BY tag_id",
        )?;
        let mut rows = statement.query([match_group])?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next()? {
            ids.push(crate::tags::TagId::parse(&row.get::<_, String>(0)?)?);
        }
        self.tag_definitions(ids)
    }

    pub(crate) fn clear_match_overrides(db: &Connection) -> Result<()> {
        let mut query = db.prepare("SELECT DISTINCT match_group FROM match_tag_overrides")?;
        let groups = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        db.execute("DELETE FROM match_tag_overrides", [])?;
        for group in groups {
            refresh_group(db, &group)?;
        }
        Ok(())
    }

    pub(super) fn match_tag_definitions(
        &self,
        match_group: &Option<String>,
    ) -> Result<Vec<crate::tags::TagDefinition>> {
        let Some(match_group) = match_group else {
            return Ok(Vec::new());
        };
        self.effective_match_tags(match_group)
    }
}

impl Index {
    /// Replace one match's authored override set inside the caller's durable
    /// store/index transaction. Observed evidence is intentionally untouched.
    pub(crate) fn replace_match_overrides(
        db: &Connection,
        match_group: &str,
        overrides: &crate::tags::MatchOverrides,
    ) -> Result<()> {
        overrides.validate()?;
        ensure!(!match_group.is_empty(), "match tag override group");
        db.execute(
            "DELETE FROM match_tag_overrides WHERE match_group=?1",
            [match_group],
        )?;
        let mut insert = db.prepare_cached(
            "INSERT INTO match_tag_overrides(match_group,tag_id,added,suppressed)
             VALUES (?1,?2,?3,?4) ON CONFLICT(match_group,tag_id)
             DO UPDATE SET added=added OR excluded.added,suppressed=suppressed OR excluded.suppressed",
        )?;
        for id in &overrides.added {
            insert.execute(params![match_group, id.as_str(), true, false])?;
        }
        for id in &overrides.suppressed {
            insert.execute(params![match_group, id.as_str(), false, true])?;
        }
        refresh_group(db, match_group)?;
        Ok(())
    }
}
