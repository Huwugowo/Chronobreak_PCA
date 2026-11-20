//! Tag definitions and assignment rules. Event evidence stays with its source.
use std::{collections::HashSet, fmt};

use anyhow::{Result, ensure};
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Deserializer, Serialize, de};

pub(crate) const MAX_ASSIGNMENTS: usize = 40;
pub(crate) const MAX_DETECTED_ASSIGNMENTS: usize = 4;
pub(crate) const MAX_LABEL: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct TagId(String);

impl TagId {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        if builtin(value).is_none() {
            let custom = value.strip_prefix("custom:").unwrap_or_default();
            MediaId::parse(custom)?;
        }
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_builtin(&self) -> bool {
        self.0.starts_with("builtin:")
    }

    fn tier(&self) -> Option<u8> {
        builtin(self.as_str()).and_then(|tag| tag.tier)
    }
}

impl fmt::Display for TagId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TagId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TagDefinition {
    pub id: TagId,
    pub label: String,
    pub tier: Option<u8>,
    pub builtin: bool,
}

/// A tag as assigned to a match or clip. Definitions are shared vocabulary;
/// provenance belongs to the assignment and is never inferred from its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TagAssignmentSource {
    Detected,
    Manual,
}

impl Default for TagAssignmentSource {
    fn default() -> Self {
        // Headers written before assignment provenance existed were always
        // editor selections, so preserve them as authored data.
        Self::Manual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TagAssignment {
    #[serde(flatten)]
    pub definition: TagDefinition,
    #[serde(default)]
    pub source: TagAssignmentSource,
}

impl TagAssignment {
    pub(crate) fn manual(definition: TagDefinition) -> Self {
        Self {
            definition,
            source: TagAssignmentSource::Manual,
        }
    }

    pub(crate) fn detected(definition: TagDefinition) -> Self {
        Self {
            definition,
            source: TagAssignmentSource::Detected,
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        self.definition.validate()?;
        ensure!(
            self.source != TagAssignmentSource::Detected || self.definition.builtin,
            "detected tags must be built in"
        );
        Ok(())
    }
}

impl TagDefinition {
    pub(crate) fn custom(label: &str) -> Result<Self> {
        Ok(Self {
            id: TagId(format!("custom:{}", MediaId::new_v4())),
            label: clean_label(label)?.to_owned(),
            tier: None,
            builtin: false,
        })
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if let Some(entry) = builtin(self.id.as_str()) {
            ensure!(
                entry.definition() == *self,
                "invalid built-in tag definition"
            );
        } else {
            ensure!(
                !self.builtin && self.tier.is_none() && clean_label(&self.label)? == self.label,
                "invalid custom tag definition"
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Builtin {
    key: &'static str,
    label: &'static str,
    tier: Option<u8>,
}

impl Builtin {
    const fn new(key: &'static str, label: &'static str, tier: Option<u8>) -> Self {
        Self { key, label, tier }
    }

    fn id(self) -> TagId {
        TagId(format!("builtin:{}", self.key))
    }

    fn definition(self) -> TagDefinition {
        TagDefinition {
            id: self.id(),
            label: self.label.to_owned(),
            tier: self.tier,
            builtin: true,
        }
    }
}

const MULTIKILLS: [Builtin; 4] = [
    Builtin::new("double_kill", "Double Kill", Some(2)),
    Builtin::new("triple_kill", "Triple Kill", Some(3)),
    Builtin::new("quadra_kill", "Quadra Kill", Some(4)),
    Builtin::new("pentakill", "Pentakill", Some(5)),
];
const DRAGON_STEAL: Builtin = Builtin::new("dragon_steal", "Dragon Steal", None);
const BARON_STEAL: Builtin = Builtin::new("baron_steal", "Baron Steal", None);
const HERALD_STEAL: Builtin = Builtin::new("herald_steal", "Herald Steal", None);
const OTHER_BUILTINS: [Builtin; 3] = [DRAGON_STEAL, BARON_STEAL, HERALD_STEAL];

fn builtin_entries() -> impl Iterator<Item = Builtin> {
    MULTIKILLS.into_iter().chain(OTHER_BUILTINS)
}

fn builtin(id: &str) -> Option<Builtin> {
    let key = id.strip_prefix("builtin:")?;
    builtin_entries().find(|entry| entry.key == key)
}

pub(crate) fn builtin_definition(id: &TagId) -> Option<TagDefinition> {
    builtin_entries()
        .find(|entry| entry.id() == *id)
        .map(Builtin::definition)
}

pub(crate) fn builtin_definitions() -> impl Iterator<Item = TagDefinition> {
    builtin_entries().map(Builtin::definition)
}

pub(crate) fn clean_label(label: &str) -> Result<&str> {
    let label = label.trim();
    ensure!(
        !label.is_empty()
            && label.encode_utf16().count() <= MAX_LABEL
            && !label.chars().any(char::is_control),
        "invalid tag name"
    );
    Ok(label)
}

pub(crate) fn label_key(label: &str) -> String {
    label.trim().to_lowercase()
}

pub(crate) fn event_tag(
    kind: &str,
    tier: Option<u32>,
    stolen: Option<bool>,
) -> Option<TagDefinition> {
    let definition = match kind {
        "Multikill" => MULTIKILLS
            .iter()
            .copied()
            .find(|tag| tag.tier.map(u32::from) == tier)?,
        "DragonKill" if stolen == Some(true) => DRAGON_STEAL,
        "BaronKill" if stolen == Some(true) => BARON_STEAL,
        "HeraldKill" if stolen == Some(true) => HERALD_STEAL,
        _ => return None,
    };
    Some(definition.definition())
}

pub(crate) fn implied_ids(id: &TagId) -> Vec<TagId> {
    match id.tier() {
        Some(tier) => MULTIKILLS
            .into_iter()
            .take(usize::from(tier - 1))
            .map(Builtin::id)
            .collect(),
        None => vec![id.clone()],
    }
}

pub(crate) fn matching_ids(id: &TagId) -> Vec<TagId> {
    match id.tier() {
        Some(tier) => MULTIKILLS
            .into_iter()
            .skip(usize::from(tier - 2))
            .map(Builtin::id)
            .collect(),
        None => vec![id.clone()],
    }
}

fn canonical_ids(ids: impl IntoIterator<Item = TagId>) -> Vec<TagId> {
    let mut seen = HashSet::new();
    let mut ids = ids
        .into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect::<Vec<_>>();
    let best = ids.iter().filter_map(TagId::tier).max();
    ids.retain(|id| id.tier().is_none_or(|tier| Some(tier) == best));
    ids
}

pub(crate) fn normalize_assignments(mut tags: Vec<TagAssignment>) -> Result<Vec<TagAssignment>> {
    ensure!(
        tags.iter()
            .filter(|tag| tag.source == TagAssignmentSource::Manual)
            .count()
            <= MAX_ASSIGNMENTS,
        "too many manual tags"
    );
    for tag in &tags {
        tag.validate()?;
    }
    let detected: HashSet<_> = canonical_ids(
        tags.iter()
            .filter(|tag| tag.source == TagAssignmentSource::Detected)
            .map(|tag| tag.definition.id.clone()),
    )
    .into_iter()
    .collect();
    let manual: HashSet<_> = canonical_ids(
        tags.iter()
            .filter(|tag| tag.source == TagAssignmentSource::Manual)
            .map(|tag| tag.definition.id.clone()),
    )
    .into_iter()
    .collect();
    let mut seen = HashSet::new();
    tags.retain(|tag| {
        let accepted = match tag.source {
            TagAssignmentSource::Detected => detected.contains(&tag.definition.id),
            TagAssignmentSource::Manual => manual.contains(&tag.definition.id),
        };
        accepted && seen.insert((tag.source, tag.definition.id.clone()))
    });
    ensure!(
        tags.iter()
            .filter(|tag| tag.source == TagAssignmentSource::Detected)
            .count()
            <= MAX_DETECTED_ASSIGNMENTS,
        "too many detected tags"
    );
    Ok(tags)
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ManualMatchTags {
    #[serde(rename = "added")]
    pub ids: Vec<TagId>,
}

impl<'de> Deserialize<'de> for ManualMatchTags {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StoredMatchTags {
            added: Vec<String>,
            #[serde(default, rename = "suppressed")]
            _retired: Option<de::IgnoredAny>,
        }

        let stored = StoredMatchTags::deserialize(deserializer)?;
        let ids = stored
            .added
            .iter()
            .filter(|id| id.as_str() != "builtin:first_blood")
            .map(|id| TagId::parse(id).map_err(de::Error::custom))
            .collect::<std::result::Result<_, _>>()?;
        Ok(Self { ids })
    }
}

impl ManualMatchTags {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.ids.len() <= MAX_ASSIGNMENTS
                && canonical_ids(self.ids.iter().cloned()) == self.ids,
            "invalid manual match tags"
        );
        Ok(())
    }

    pub(crate) fn replace(&mut self, selected: Vec<TagId>) -> Result<()> {
        ensure!(selected.len() <= MAX_ASSIGNMENTS, "too many match tags");
        self.ids = canonical_ids(selected);
        self.validate()
    }
}

pub(crate) fn effective_assignments(
    detected: impl IntoIterator<Item = TagId>,
    manual: &ManualMatchTags,
) -> Vec<(TagId, TagAssignmentSource)> {
    canonical_ids(detected)
        .into_iter()
        .map(|id| (id, TagAssignmentSource::Detected))
        .chain(
            manual
                .ids
                .iter()
                .cloned()
                .map(|id| (id, TagAssignmentSource::Manual)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_and_detected_assignments_share_the_tier_rule() {
        let penta = MULTIKILLS[3].definition();
        let quadra = MULTIKILLS[2].definition();
        assert_eq!(implied_ids(&penta.id), MULTIKILLS.map(Builtin::id));
        assert_eq!(
            implied_ids(&quadra.id),
            MULTIKILLS[..3]
                .iter()
                .map(|tag| tag.id())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            normalize_assignments(vec![
                TagAssignment::manual(quadra),
                TagAssignment::manual(penta.clone())
            ])
            .unwrap(),
            [TagAssignment::manual(penta)]
        );
    }

    #[test]
    fn flattened_assignment_round_trips_with_strict_fields() {
        let assignment = TagAssignment::detected(MULTIKILLS[3].definition());
        let json = serde_json::to_value(&assignment).unwrap();
        assert_eq!(json["source"], "detected");
        assert_eq!(
            serde_json::from_value::<TagAssignment>(json).unwrap(),
            assignment
        );
        assert!(
            serde_json::from_value::<TagAssignment>(serde_json::json!({
                "id": "builtin:pentakill", "label": "Pentakill", "tier": 5,
                "builtin": true, "source": "detected", "unexpected": true
            }))
            .is_err()
        );
    }

    #[test]
    fn development_store_keeps_manual_tags_and_retires_first_blood_and_dismissals() {
        let tags: ManualMatchTags = serde_json::from_value(serde_json::json!({
            "added": ["builtin:first_blood", "builtin:pentakill"],
            "suppressed": ["builtin:first_blood", "builtin:baron_steal"]
        }))
        .unwrap();
        tags.validate().unwrap();
        assert_eq!(tags.ids, [MULTIKILLS[3].id()]);
        assert_eq!(
            serde_json::to_value(tags).unwrap(),
            serde_json::json!({
                "added": ["builtin:pentakill"]
            })
        );
    }

    #[test]
    fn manual_assignments_never_suppress_future_evidence() {
        let penta = MULTIKILLS[3].id();
        let triple = MULTIKILLS[1].id();
        let quadra = MULTIKILLS[2].id();
        let mut edits = ManualMatchTags::default();
        edits.replace(vec![triple.clone()]).unwrap();
        assert_eq!(
            effective_assignments([penta.clone(), quadra], &edits),
            vec![
                (penta, TagAssignmentSource::Detected),
                (triple, TagAssignmentSource::Manual),
            ]
        );
    }

    #[test]
    fn unrelated_late_events_remain_visible_when_saving_open_edit() {
        let dragon = DRAGON_STEAL.id();
        let baron = BARON_STEAL.id();
        let mut edits = ManualMatchTags::default();
        edits.replace(vec![]).unwrap();
        assert_eq!(
            effective_assignments([dragon.clone(), baron.clone()], &edits),
            vec![
                (dragon, TagAssignmentSource::Detected),
                (baron, TagAssignmentSource::Detected),
            ]
        );
    }
}
