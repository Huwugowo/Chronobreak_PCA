//! Completed-source, within-epoch aliases. Raw producer evidence stays intact.
use super::*;
use obs::events::{ActorRole, ActorStatus, EventActors, EventRevision};
use std::collections::BTreeMap;

type Aliases = BTreeMap<String, Option<obs::ParticipantId>>;

fn observe(aliases: &mut Aliases, player: &obs::RosterPlayer) {
    let Some(label) = player
        .riot_id_game_name
        .as_deref()
        .filter(|s| !s.is_empty())
    else {
        return;
    };
    let valid = player.riot_id.as_deref().is_some_and(|id| {
        obs::decode::full_riot_id(id)
            && id.split_once('#').is_some_and(|(name, tag)| {
                name == label && player.riot_id_tag_line.as_deref().is_none_or(|t| t == tag)
            })
    }) && [
        "riot_id",
        "riot_id_game_name",
        "riot_id_tag_line",
        "participant",
    ]
    .iter()
    .all(|key| !player.issues.contains_key(*key));
    let candidate = valid.then_some(player.participant).flatten();
    aliases
        .entry(label.to_ascii_lowercase())
        .and_modify(|prior| {
            if *prior != candidate {
                *prior = None;
            }
        })
        .or_insert(candidate);
}

fn label(event: &obs::Event, role: ActorRole, ordinal: usize) -> Option<&str> {
    if role == ActorRole::Assister {
        return event.assisters.as_ref()?.get(ordinal).map(String::as_str);
    }
    if ordinal != 0 {
        return None;
    }
    match role {
        ActorRole::Killer => event.killer.as_deref(),
        ActorRole::Victim => event.victim.as_deref(),
        ActorRole::Recipient => event.recipient.as_deref(),
        ActorRole::Acer => event.acer.as_deref(),
        ActorRole::Assister => None,
    }
}

pub(super) fn refresh(db: &Connection, media: &MediaId) -> Result<()> {
    db.execute(
        "DELETE FROM derived_actor_links WHERE media_id=?1",
        [media.as_str()],
    )?;
    // Stream payloads and retain only the distinct identity dictionary, not snapshots.
    let mut scopes: BTreeMap<(String, u64), Aliases> = BTreeMap::new();
    let mut roster =
        db.prepare("SELECT run_id,epoch,data FROM roster_payloads WHERE media_id=?1")?;
    let mut rows = roster.query([media.as_str()])?;
    while let Some(row) = rows.next()? {
        let key = (row.get(0)?, unsigned(row, 1)?);
        let player = serde_json::from_str::<obs::RosterPlayer>(&row.get::<_, String>(2)?)?;
        observe(scopes.entry(key).or_default(), &player);
    }
    let mut revisions = db.prepare(
        "SELECT run_id,epoch,sequence,ordinal,data FROM event_revisions WHERE media_id=?1",
    )?;
    let mut rows = revisions.query([media.as_str()])?;
    let mut insert = db.prepare(
        "INSERT INTO derived_actor_links VALUES (?1,?2,?3,?4,?5,?6,'validated_roster_game_name')",
    )?;
    while let Some(row) = rows.next()? {
        let key = (row.get(0)?, unsigned(row, 1)?);
        let Some(aliases) = scopes.get(&key) else {
            continue;
        };
        let revision: EventRevision = serde_json::from_str(&row.get::<_, String>(4)?)?;
        for (role, references) in revision.event.actors.iter().flat_map(|a| a.iter()) {
            for (ordinal, actor) in references.iter().enumerate() {
                if actor.status != ActorStatus::Unresolved || actor.participant.is_some() {
                    continue;
                }
                let Some(label) =
                    label(&revision.event, *role, ordinal).filter(|s| !s.contains('#'))
                else {
                    continue;
                };
                let Some(Some(participant)) = aliases.get(&label.to_ascii_lowercase()) else {
                    continue;
                };
                insert.execute(params![
                    media.as_str(),
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    name(role)?,
                    sql_u64(ordinal as u64)?,
                    sql_u64(participant.0)?
                ])?;
            }
        }
    }
    Ok(())
}

impl Index {
    pub(super) fn effective_actors(
        &self,
        media: &MediaId,
        sequence: u64,
        ordinal: u64,
        raw: &Option<EventActors>,
    ) -> Result<Option<EventActors>> {
        let mut actors = raw.clone();
        let mut statement = self.db.prepare("SELECT role,ordinal,participant_id FROM derived_actor_links WHERE media_id=?1 AND sequence=?2 AND revision_ordinal=?3")?;
        let mut rows = statement.query(params![
            media.as_str(),
            sql_u64(sequence)?,
            sql_u64(ordinal)?
        ])?;
        while let Some(row) = rows.next()? {
            let role: ActorRole = serde_json::from_value(serde_json::Value::String(row.get(0)?))?;
            let ordinal = usize::try_from(unsigned(row, 1)?)?;
            let actor = actors
                .as_mut()
                .and_then(|a| a.get_mut(&role))
                .and_then(|r| r.get_mut(ordinal))
                .context("derived actor missing raw reference")?;
            ensure!(
                actor.status == ActorStatus::Unresolved && actor.participant.is_none(),
                "derived actor raw state"
            );
            actor.status = ActorStatus::Resolved;
            actor.participant = Some(obs::ParticipantId(unsigned(row, 2)?));
        }
        Ok(actors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player(name: &str, tag: &str, participant: u64) -> obs::RosterPlayer {
        obs::RosterPlayer {
            riot_id: Some(format!("{name}#{tag}")),
            riot_id_game_name: Some(name.into()),
            riot_id_tag_line: Some(tag.into()),
            participant: Some(obs::ParticipantId(participant)),
            ..Default::default()
        }
    }
    #[test]
    fn aliases_require_explicit_consistent_identity_and_keep_all_collisions() {
        let mut aliases = Aliases::new();
        observe(&mut aliases, &player("Zyra Bot", "BOT", 1));
        assert_eq!(aliases["zyra bot"], Some(obs::ParticipantId(1)));
        observe(&mut aliases, &player("Zyra Bot", "BOT", 1));
        assert_eq!(aliases["zyra bot"], Some(obs::ParticipantId(1)));
        observe(&mut aliases, &player("Zyra bot", "OTHER", 2));
        observe(&mut aliases, &player("Zyra Bot", "BOT", 1));
        assert_eq!(aliases["zyra bot"], None);
        for (i, mut p) in [
            player("Split", "A", 3),
            player("Issue", "A", 4),
            player("Missing", "A", 5),
        ]
        .into_iter()
        .enumerate()
        {
            match i {
                0 => p.riot_id_tag_line = Some("B".into()),
                1 => {
                    p.issues
                        .insert("participant".into(), obs::FieldIssue::Conflict);
                }
                _ => p.participant = None,
            }
            observe(&mut aliases, &p);
            assert_eq!(
                aliases[&p.riot_id_game_name.unwrap().to_ascii_lowercase()],
                None
            );
        }
        let mut absent = player("Absent", "A", 6);
        absent.riot_id_game_name = None;
        observe(&mut aliases, &absent);
        assert!(!aliases.contains_key("absent"));
        observe(&mut aliases, &player("Écho", "A", 7));
        assert!(!aliases.contains_key("écho")); // No Unicode case folding.
    }

    #[test]
    fn derived_links_are_scoped_by_media_run_and_epoch_and_preserve_raw_states() {
        let index = Index {
            db: Connection::open_in_memory().unwrap(),
        };
        index.db.execute_batch(include_str!("schema.sql")).unwrap();
        let media = MediaId::parse("11111111-2222-4333-8444-555555555555").unwrap();
        let other = MediaId::parse("11111111-2222-4333-8444-555555555556").unwrap();
        for m in [&media, &other] {
            index.db.execute("INSERT INTO recordings(media_id,bundle,recorded_at,champion,mode,saved,outcome,summary_json) VALUES (?1,?1,'','','',0,'unknown','{}')",[m.as_str()]).unwrap();
        }
        let revision: EventRevision = serde_json::from_value(serde_json::json!({
            "event":{"source_id":1,"game_tick":"1000000","killer":"Shared","victim":"Shared","recipient":"Shared","assisters":["Baron"],
                "actors":{"killer":[{"participant":null,"status":"unresolved"}],"victim":[{"participant":null,"status":"conflict"}],
                    "recipient":[{"participant":9,"status":"resolved"}],"assister":[{"participant":null,"status":"unresolved"}]}},
            "fingerprint":vec![0u8;32],"response_fingerprint":vec![0u8;32],"ingestion":"accepted"
        })).unwrap();
        let cases = [
            (&media, "run-a", 0, 1),
            (&media, "run-a", 1, 2),
            (&media, "run-b", 0, 3),
            (&other, "run-a", 0, 4),
        ];
        for (m, run, epoch, participant) in cases {
            index
                .db
                .execute(
                    "INSERT INTO roster_payloads VALUES (?1,?2,?3,1,?4)",
                    params![
                        m.as_str(),
                        run,
                        epoch,
                        serde_json::to_string(&player("Shared", "TAG", participant)).unwrap()
                    ],
                )
                .unwrap();
            index.db.execute("INSERT INTO event_revisions VALUES (?1,?2,?3,?4,0,1,?5,?5,1,1000000,'ChampionKill',NULL,NULL,'accepted',?6)",params![m.as_str(),run,epoch,sql_u64(participant).unwrap(),&[0u8;32][..],serde_json::to_string(&revision).unwrap()]).unwrap();
        }
        refresh(&index.db, &media).unwrap();
        refresh(&index.db, &other).unwrap();
        for (m, _, _, participant) in cases {
            let effective = index
                .effective_actors(m, participant, 0, &revision.event.actors)
                .unwrap()
                .unwrap();
            assert_eq!(
                effective[&ActorRole::Killer][0].participant,
                Some(obs::ParticipantId(participant))
            );
            assert_eq!(
                effective[&ActorRole::Victim][0].status,
                ActorStatus::Conflict
            );
            assert_eq!(
                effective[&ActorRole::Recipient][0].participant,
                Some(obs::ParticipantId(9))
            );
            assert_eq!(
                effective[&ActorRole::Assister][0].status,
                ActorStatus::Unresolved
            );
            assert_eq!(
                revision.event.actors.as_ref().unwrap()[&ActorRole::Killer][0].status,
                ActorStatus::Unresolved
            );
        }
    }
}
