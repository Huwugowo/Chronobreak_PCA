//! Rebuildable vocabulary projection, including definitions carried by detached clips.
use super::*;
use crate::tag_store::{self, LoadedStore};
use crate::tags::{self, MatchOverrides, TagDefinition, TagId};

const SUGGESTION_LIMIT: usize = 24;

impl Index {
    pub(crate) fn match_exists(&self, key: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM recordings WHERE match_group=?1)",
            [key],
            |row| row.get(0),
        )?)
    }

    fn has_tag_table(&self, table: &str) -> Result<bool> {
        Ok(self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )?)
    }

    fn tag_stamp(&self) -> Result<Option<String>> {
        if !self.has_tag_table("tag_catalog_state")? {
            return Ok(None);
        }
        Ok(self
            .db
            .query_row(
                "SELECT stamp FROM tag_catalog_state WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub(super) fn refresh_tags_from_root(&mut self, root: &Path) -> Result<()> {
        let stamp = tag_store::current_stamp(root)?;
        if self.tag_stamp()?.as_deref() == Some(&stamp) {
            return Ok(());
        }
        self.refresh_tag_store(&tag_store::load(root)?)
    }

    pub(crate) fn refresh_tag_store(&mut self, source: &LoadedStore) -> Result<()> {
        if self.tag_stamp()?.as_deref() == Some(&source.stamp) {
            return Ok(());
        }
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM tag_definition", [])?;
        for tag in source.store.definitions.values() {
            upsert_definition(&tx, tag)?;
        }
        Self::clear_match_overrides(&tx)?;
        for (key, overrides) in &source.store.matches {
            Self::replace_match_overrides(&tx, key, overrides)?;
        }
        set_tag_stamp(&tx, &source.stamp)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn update_tag_projection(
        &mut self,
        stamp: &str,
        definitions: &[TagDefinition],
        subject: Option<(&str, &MatchOverrides)>,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        for definition in definitions {
            upsert_definition(&tx, definition)?;
        }
        if let Some((key, overrides)) = subject {
            Self::replace_match_overrides(&tx, key, overrides)?;
        }
        set_tag_stamp(&tx, stamp)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn resolve_tags(&self, tags: Vec<TagDefinition>) -> Result<Vec<TagDefinition>> {
        self.resolve_tag_ids(tags.into_iter().map(|tag| (tag.id.clone(), Some(tag))))
    }

    pub(crate) fn tag_definitions(&self, ids: Vec<TagId>) -> Result<Vec<TagDefinition>> {
        self.resolve_tag_ids(ids.into_iter().map(|id| (id, None)))
    }

    fn resolve_tag_ids(
        &self,
        ids: impl Iterator<Item = (TagId, Option<TagDefinition>)>,
    ) -> Result<Vec<TagDefinition>> {
        let mut result = Vec::new();
        let mut saved = None;
        let mut recovery = None;
        let mut opened = false;
        for (id, carried) in ids {
            if let Some(tag) = tags::builtin_definition(&id) {
                result.push(tag);
                continue;
            }
            if !opened {
                saved = self
                    .has_tag_table("tag_definition")?
                    .then(|| {
                        self.db
                            .prepare_cached("SELECT label FROM tag_definition WHERE id=?1")
                    })
                    .transpose()?;
                recovery = self
                    .has_tag_table("clip_recovered_tag_definition")?
                    .then(|| {
                        self.db.prepare_cached(
                            "SELECT label FROM clip_recovered_tag_definition WHERE tag_id=?1",
                        )
                    })
                    .transpose()?;
                opened = true;
            }
            if let Some(label) = lookup_label(saved.as_mut(), &id)? {
                result.push(custom(id, label));
                continue;
            }
            let label =
                lookup_label(recovery.as_mut(), &id)?.or_else(|| carried.map(|tag| tag.label));
            if let Some(label) = label {
                let label =
                    self.recovered_label(&id, label, saved.is_some(), recovery.is_some())?;
                result.push(custom(id, label));
            }
        }
        Ok(result)
    }

    fn recovered_label(
        &self,
        id: &TagId,
        label: String,
        saved: bool,
        recovery: bool,
    ) -> Result<String> {
        let key = tags::label_key(&label);
        let authored_collision = saved
            && self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM tag_definition WHERE label_key=?1)",
                [&key],
                |row| row.get::<_, bool>(0),
            )?;
        let earlier_recovery = recovery
            && self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM clip_recovered_tag_definition
             WHERE label_key=?1 AND tag_id<?2)",
                params![key, id.as_str()],
                |row| row.get::<_, bool>(0),
            )?;
        if !authored_collision
            && !earlier_recovery
            && !tags::builtin_definitions().any(|tag| tags::label_key(&tag.label) == key)
        {
            return Ok(label);
        }
        // Retain the foreign ID and give equal names distinguishable choices.
        let suffix = format!(" · {}", &id.as_str()[7..15]);
        let mut length = suffix.encode_utf16().count();
        let mut distinct = String::new();
        for c in label.chars() {
            length += c.len_utf16();
            if length > tags::MAX_LABEL {
                break;
            }
            distinct.push(c);
        }
        distinct.push_str(&suffix);
        Ok(distinct)
    }

    pub(crate) fn tag_choices(&self, prefix: &str) -> Result<Vec<TagDefinition>> {
        let prefix = tags::label_key(prefix);
        let ceiling = format!("{prefix}\u{10ffff}");
        let mut choices: Vec<_> = tags::builtin_definitions()
            .filter(|tag| tags::label_key(&tag.label).starts_with(&prefix))
            .collect();
        let mut candidates = Vec::new();
        let saved = self.has_tag_table("tag_definition")?;
        if saved {
            candidates.extend(self.tag_choice_ids(
                "SELECT id FROM tag_definition WHERE label_key>=?1 AND label_key<?2
                 ORDER BY label_key LIMIT ?3",
                &prefix,
                &ceiling,
            )?);
        }
        if self.has_tag_table("clip_recovered_tag_definition")? {
            let sql = if saved {
                "SELECT tag_id FROM clip_recovered_tag_definition r WHERE label_key>=?1 AND label_key<?2
                 AND NOT EXISTS(SELECT 1 FROM tag_definition d WHERE d.id=r.tag_id)
                 ORDER BY label_key,tag_id LIMIT ?3"
            } else {
                "SELECT tag_id FROM clip_recovered_tag_definition WHERE label_key>=?1 AND label_key<?2
                 ORDER BY label_key,tag_id LIMIT ?3"
            };
            candidates.extend(self.tag_choice_ids(sql, &prefix, &ceiling)?);
        }
        let mut custom = self.tag_definitions(candidates)?;
        custom.sort_by_cached_key(|tag| (tags::label_key(&tag.label), tag.id.clone()));
        choices.extend(custom.into_iter().take(SUGGESTION_LIMIT - choices.len()));
        Ok(choices)
    }

    pub(crate) fn exact_tag_label(&self, label: &str) -> Result<Option<TagDefinition>> {
        let key = tags::label_key(label);
        if let Some(tag) =
            tags::builtin_definitions().find(|tag| tags::label_key(&tag.label) == key)
        {
            return Ok(Some(tag));
        }
        let choices = self.tag_choices(label)?;
        Ok(choices
            .into_iter()
            .find(|tag| tags::label_key(&tag.label) == key))
    }

    fn tag_choice_ids(&self, sql: &str, prefix: &str, ceiling: &str) -> Result<Vec<TagId>> {
        let mut statement = self.db.prepare_cached(sql)?;
        statement
            .query_map(params![prefix, ceiling, SUGGESTION_LIMIT as i64], |row| {
                row.get::<_, String>(0)
            })?
            .map(|row| TagId::parse(&row?))
            .collect()
    }
}

fn lookup_label(
    statement: Option<&mut rusqlite::CachedStatement<'_>>,
    id: &TagId,
) -> Result<Option<String>> {
    match statement {
        Some(statement) => Ok(statement
            .query_row([id.as_str()], |row| row.get(0))
            .optional()?),
        None => Ok(None),
    }
}

fn custom(id: TagId, label: String) -> TagDefinition {
    TagDefinition {
        id,
        label,
        tier: None,
        builtin: false,
    }
}

fn upsert_definition(db: &Connection, tag: &TagDefinition) -> Result<()> {
    db.execute(
        "INSERT INTO tag_definition VALUES(?1,?2,?3)
         ON CONFLICT(id) DO UPDATE SET label=excluded.label,label_key=excluded.label_key",
        params![tag.id.as_str(), tag.label, tags::label_key(&tag.label)],
    )?;
    Ok(())
}

fn set_tag_stamp(db: &Connection, stamp: &str) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO tag_catalog_state VALUES(1,?1)",
        [stamp],
    )?;
    Ok(())
}
