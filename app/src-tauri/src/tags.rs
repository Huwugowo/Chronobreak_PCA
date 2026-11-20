//! Tag definitions and assignment rules. Event evidence stays with its source.
use std::{collections::HashSet, fmt};

use anyhow::{Result, ensure};
use chronobreak_league_data::observation::events::ActorRole;
use chronobreak_replay_time::MediaId;
use serde::{Deserialize, Deserializer, Serialize, de};

pub(crate) const MAX_ASSIGNMENTS: usize = 40;
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

    fn suppression_key(&self) -> Self {
        if self.tier().is_some() {
            MULTIKILLS[0].id()
        } else {
            self.clone()
        }
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
const FIRST_BLOOD: Builtin = Builtin::new("first_blood", "First Blood", None);
const DRAGON_STEAL: Builtin = Builtin::new("dragon_steal", "Dragon Steal", None);
const BARON_STEAL: Builtin = Builtin::new("baron_steal", "Baron Steal", None);
const HERALD_STEAL: Builtin = Builtin::new("herald_steal", "Herald Steal", None);
const OTHER_BUILTINS: [Builtin; 4] = [FIRST_BLOOD, DRAGON_STEAL, BARON_STEAL, HERALD_STEAL];

fn builtin_entries() -> impl Iterator<Item = Builtin> {
    MULTIKILLS.into_iter().chain(OTHER_BUILTINS)
}

fn builtin(id: &str) -> Option<Builtin> {
    let key = id.strip_prefix("builtin:")?;
    builtin_entries().find(|entry| entry.key == key)
}

pub(crate) fn builtin_definition(id: &TagId) -> Option<TagDefinition> {
    builtin(id.as_str()).map(Builtin::definition)
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

pub(crate) fn event_role(kind: &str) -> Option<ActorRole> {
    match kind {
        "FirstBlood" => Some(ActorRole::Recipient),
        "Multikill" | "DragonKill" | "BaronKill" | "HeraldKill" => Some(ActorRole::Killer),
        _ => None,
    }
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
        "FirstBlood" => FIRST_BLOOD,
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

pub(crate) fn normalize_assignments(mut tags: Vec<TagDefinition>) -> Result<Vec<TagDefinition>> {
    ensure!(tags.len() <= MAX_ASSIGNMENTS, "too many tags");
    for tag in &tags {
        tag.validate()?;
    }
    let actual: HashSet<_> = canonical_ids(tags.iter().map(|tag| tag.id.clone()))
        .into_iter()
        .collect();
    let mut seen = HashSet::new();
    tags.retain(|tag| actual.contains(&tag.id) && seen.insert(tag.id.clone()));
    Ok(tags)
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MatchOverrides {
    pub added: Vec<TagId>,
    pub suppressed: Vec<TagId>,
}

impl MatchOverrides {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.added.len() <= MAX_ASSIGNMENTS
                && canonical_ids(self.added.iter().cloned()) == self.added
                && self.suppressed.len() <= OTHER_BUILTINS.len() + 1
                && self
                    .suppressed
                    .iter()
                    .all(|id| id.is_builtin() && *id == id.suppression_key())
                && self.suppressed.windows(2).all(|pair| pair[0] < pair[1]),
            "invalid match tag overrides"
        );
        Ok(())
    }

    pub(crate) fn apply(&mut self, previous: Vec<TagId>, selected: Vec<TagId>) -> Result<()> {
        ensure!(
            previous.len() <= MAX_ASSIGNMENTS + OTHER_BUILTINS.len() + 1
                && selected.len() <= MAX_ASSIGNMENTS + OTHER_BUILTINS.len() + 1,
            "too many match tags"
        );
        let previous = canonical_ids(previous);
        let selected = canonical_ids(selected);
        for removed in previous.iter().filter(|id| !selected.contains(id)) {
            self.added.retain(|id| id != removed);
            if removed.is_builtin() {
                self.suppressed.push(removed.suppression_key());
            }
        }
        for added in selected.into_iter().filter(|id| !previous.contains(id)) {
            if added.tier().is_some() {
                self.added.retain(|id| id.tier().is_none());
                self.suppressed.push(added.suppression_key());
            }
            if !self.added.contains(&added) {
                self.added.push(added);
            }
        }
        self.suppressed.sort_unstable();
        self.suppressed.dedup();
        self.validate()
    }
}

pub(crate) fn effective_ids(
    detected: impl IntoIterator<Item = TagId>,
    overrides: &MatchOverrides,
) -> Vec<TagId> {
    canonical_ids(
        overrides.added.iter().cloned().chain(
            detected
                .into_iter()
                .filter(|id| !overrides.suppressed.contains(&id.suppression_key())),
        ),
    )
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
            normalize_assignments(vec![quadra, penta.clone()]).unwrap(),
            [penta]
        );
    }

    #[test]
    fn a_downgrade_and_removal_override_future_evidence() {
        let penta = MULTIKILLS[3].id();
        let triple = MULTIKILLS[1].id();
        let quadra = MULTIKILLS[2].id();
        let mut edits = MatchOverrides::default();
        assert_eq!(effective_ids([penta.clone()], &edits), [penta.clone()]);
        edits
            .apply(vec![penta.clone()], vec![triple.clone()])
            .unwrap();
        let mut edits: MatchOverrides =
            serde_json::from_slice(&serde_json::to_vec(&edits).unwrap()).unwrap();
        assert_eq!(
            effective_ids([penta, quadra.clone()], &edits),
            [triple.clone()]
        );
        edits.apply(vec![triple], vec![]).unwrap();
        assert!(effective_ids([quadra], &edits).is_empty());
    }

    #[test]
    fn unrelated_late_events_remain_visible_when_saving_open_edit() {
        let dragon = DRAGON_STEAL.id();
        let baron = BARON_STEAL.id();
        let mut edits = MatchOverrides::default();
        edits.apply(vec![dragon.clone()], vec![]).unwrap();
        assert_eq!(effective_ids([dragon, baron.clone()], &edits), [baron]);
    }
}
