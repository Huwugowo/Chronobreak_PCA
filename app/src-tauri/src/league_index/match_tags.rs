//! Match tags are a small derived projection over verified local event facts.
//! Manual tags are projected independently so re-importing a source preserves them.
use super::*;
use std::collections::{HashMap, HashSet};

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
                AND a.role='killer'
                AND e.kind IN ('Multikill','DragonKill','BaronKill','HeraldKill')",
        )?;
        let mut rows = statement.query([media.as_str()])?;
        let mut result = std::collections::HashSet::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            if name(&chronobreak_league_data::observation::events::ActorRole::Killer)?
                != row.get::<_, String>(3)?
            {
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
    let mut manual = crate::tags::ManualMatchTags::default();
    let mut override_statement = db.prepare_cached(
        "SELECT tag_id FROM match_tag_manual WHERE match_group=?1 ORDER BY tag_id",
    )?;
    let mut rows = override_statement.query([match_group])?;
    while let Some(row) = rows.next()? {
        let id = crate::tags::TagId::parse(&row.get::<_, String>(0)?)?;
        manual.ids.push(id);
    }
    manual.validate()?;
    db.execute(
        "DELETE FROM match_tag_effective WHERE match_group=?1",
        [match_group],
    )?;
    let mut insert = db.prepare_cached(
        "INSERT INTO match_tag_effective(match_group,tag_id,source) VALUES (?1,?2,?3)",
    )?;
    for (id, source) in crate::tags::effective_assignments(detected, &manual) {
        insert.execute(params![
            match_group,
            id.as_str(),
            serde_json::to_value(source)?.as_str()
        ])?;
    }
    Ok(())
}

impl Index {
    pub(crate) fn effective_match_tags_for_groups(
        &self,
        groups: impl IntoIterator<Item = String>,
    ) -> Result<HashMap<String, Vec<crate::tags::TagAssignment>>> {
        let groups: Vec<_> = groups
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if groups.is_empty() {
            return Ok(HashMap::new());
        }
        let encoded = serde_json::to_string(&groups)?;
        let mut statement = self.db.prepare_cached(
            "SELECT match_group,tag_id,source FROM match_tag_effective
             WHERE match_group IN (SELECT value FROM json_each(?1)) ORDER BY match_group,source,tag_id",
        )?;
        let mut rows = statement.query([encoded])?;
        let mut records = Vec::new();
        let mut ids = HashSet::new();
        while let Some(row) = rows.next()? {
            let group: String = row.get(0)?;
            let id = crate::tags::TagId::parse(&row.get::<_, String>(1)?)?;
            let source = serde_json::from_value(serde_json::Value::String(row.get(2)?))?;
            ids.insert(id.clone());
            records.push((group, id, source));
        }
        let definitions: HashMap<_, _> = self
            .tag_definitions(ids.into_iter().collect())?
            .into_iter()
            .map(|tag| (tag.id.clone(), tag))
            .collect();
        let mut result: HashMap<String, Vec<crate::tags::TagAssignment>> = HashMap::new();
        for (group, id, source) in records {
            let definition = definitions.get(&id).cloned().context("tag unavailable")?;
            result
                .entry(group)
                .or_default()
                .push(crate::tags::TagAssignment { definition, source });
        }
        Ok(result)
    }

    pub(crate) fn effective_match_tags(
        &self,
        match_group: &str,
    ) -> Result<Vec<crate::tags::TagAssignment>> {
        Ok(self
            .effective_match_tags_for_groups([match_group.to_owned()])?
            .remove(match_group)
            .unwrap_or_default())
    }

    pub(crate) fn clear_manual_match_tags(db: &Connection) -> Result<()> {
        let mut query = db.prepare("SELECT DISTINCT match_group FROM match_tag_manual")?;
        let groups = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        db.execute("DELETE FROM match_tag_manual", [])?;
        for group in groups {
            refresh_group(db, &group)?;
        }
        Ok(())
    }
}

impl Index {
    /// Project one match's manual assignments within the caller's transaction.
    pub(crate) fn replace_manual_match_tags(
        db: &Connection,
        match_group: &str,
        manual: &crate::tags::ManualMatchTags,
    ) -> Result<()> {
        manual.validate()?;
        ensure!(!match_group.is_empty(), "manual tag group");
        db.execute(
            "DELETE FROM match_tag_manual WHERE match_group=?1",
            [match_group],
        )?;
        let mut insert =
            db.prepare_cached("INSERT INTO match_tag_manual(match_group,tag_id) VALUES (?1,?2)")?;
        for id in &manual.ids {
            insert.execute(params![match_group, id.as_str()])?;
        }
        refresh_group(db, match_group)?;
        Ok(())
    }
}
