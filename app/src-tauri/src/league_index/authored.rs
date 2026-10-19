use super::*;
use crate::league_authored;

#[derive(Debug, Serialize)]
pub(crate) struct AuthoredPage {
    pub status: String,
    pub revision: u64,
    pub records: Vec<league_authored::AuthoredRecord>,
    pub next: Option<MediaId>,
}

impl Index {
    pub(super) fn refresh_authored(&mut self, root: &Path) -> Result<()> {
        let path = root.join(league_authored::FILE);
        let stamp = match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && !meta.is_symlink() => {
                let file = fs::File::open(&path)?;
                format!(
                    "{}:{}:{:?}",
                    crate::playback_file::file_facts(&file)?.generation_key(),
                    meta.len(),
                    meta.modified()?
                )
            }
            Ok(_) => "invalid-file-type".into(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".into(),
            Err(error) => return Err(error.into()),
        };
        let previous: Option<String> =
            self.db
                .query_row("SELECT authored_stamp FROM cache_identity", [], |r| {
                    r.get(0)
                })?;
        if previous.as_deref() == Some(&stamp) {
            return Ok(());
        }
        let source = league_authored::load(root);
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM authored_records", [])?;
        if let Ok(store) = &source {
            for record in &store.records {
                tx.execute(
                    "INSERT INTO authored_records VALUES (?1,?2,?3,?4)",
                    params![
                        record.id.as_str(),
                        name(&record.kind)?,
                        record.created_at,
                        serde_json::to_string(record)?
                    ],
                )?;
                for (ordinal, input) in record.inputs.iter().enumerate() {
                    let kind = match input {
                        league_authored::InputReference::Media { .. } => "media",
                        league_authored::InputReference::Event { .. } => "event",
                        league_authored::InputReference::Participant { .. } => "participant",
                    };
                    tx.execute(
                        "INSERT INTO authored_inputs VALUES (?1,?2,?3,?4,?5)",
                        params![
                            record.id.as_str(),
                            ordinal as i64,
                            input.media_id().as_str(),
                            kind,
                            serde_json::to_string(input)?
                        ],
                    )?;
                }
                for (ordinal, range) in record.ranges.iter().enumerate() {
                    tx.execute(
                        "INSERT INTO authored_ranges VALUES (?1,?2,?3,?4,?5)",
                        params![
                            record.id.as_str(),
                            ordinal as i64,
                            range.media_id.as_str(),
                            sql_u64(range.start_frame.get())?,
                            sql_u64(range.end_frame_exclusive.get())?
                        ],
                    )?;
                }
            }
        }
        tx.execute(
            "UPDATE cache_identity SET authored_stamp=?1,authored_status=?2,authored_revision=?3",
            params![
                stamp,
                if source.is_ok() {
                    "ready"
                } else {
                    "unavailable"
                },
                source
                    .as_ref()
                    .ok()
                    .map(|s| sql_u64(s.revision))
                    .transpose()?
                    .unwrap_or(0)
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn authored(&self, after: Option<&MediaId>, limit: u8) -> Result<AuthoredPage> {
        ensure!((1..=32).contains(&limit), "authored page limit");
        let status = self
            .db
            .query_row("SELECT authored_status FROM cache_identity", [], |r| {
                r.get(0)
            })?;
        let revision =
            self.db
                .query_row("SELECT authored_revision FROM cache_identity", [], |r| {
                    unsigned(r, 0)
                })?;
        let mut statement=self.db.prepare("SELECT data FROM authored_records WHERE (?1 IS NULL OR record_id>?1) ORDER BY record_id LIMIT ?2")?;
        let mut rows =
            statement.query(params![after.map(MediaId::as_str), u16::from(limit) + 1])?;
        let mut records: Vec<league_authored::AuthoredRecord> = Vec::new();
        let mut more = false;
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            if records.len() == usize::from(limit) || bytes + json.len() > 2 * 1024 * 1024 {
                more = true;
                break;
            }
            bytes += json.len();
            records.push(serde_json::from_str(&json)?);
        }
        let next = if more {
            records.last().map(|r| r.id.clone())
        } else {
            None
        };
        Ok(AuthoredPage {
            status,
            revision,
            records,
            next,
        })
    }
}
