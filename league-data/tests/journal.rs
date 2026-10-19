use chronobreak_league_data::observation::{journal::*, *};
use chronobreak_replay_time::MediaId;
use std::cell::RefCell;
use std::io::{self, Cursor, Write};
use std::rc::Rc;

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    fail_after: Option<usize>,
    fail_flush: bool,
    fail_sync: bool,
    syncs: usize,
}
#[derive(Clone, Default)]
struct Memory(Rc<RefCell<State>>);
impl Write for Memory {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        let count = state.fail_after.map_or(bytes.len(), |n| {
            n.saturating_sub(state.bytes.len()).min(bytes.len())
        });
        if count == 0 {
            return Err(io::Error::other("injected disk full"));
        }
        state.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.0.borrow().fail_flush {
            Err(io::Error::other("injected flush failure"))
        } else {
            Ok(())
        }
    }
}
impl DurableWrite for Memory {
    fn sync_data(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.syncs += 1;
        if state.fail_sync {
            Err(io::Error::other("injected sync failure"))
        } else {
            Ok(())
        }
    }
}
fn scope() -> SourceEpoch {
    SourceEpoch {
        media_id: MediaId::parse("10000000-0000-4000-8000-000000000001").unwrap(),
        run_id: MediaId::parse("10000000-0000-4000-8000-000000000002").unwrap(),
        epoch: 0,
    }
}
fn evidence(id: u64, endpoint: Endpoint) -> Evidence {
    Evidence {
        id: ObservationId(id),
        source: scope(),
        endpoint,
        timing: RequestTiming {
            start_us: id * 100,
            end_us: id * 100 + 10,
            received_at: "2026-10-15T00:00:00Z".into(),
        },
        game_stats: None,
        calibration: None,
    }
}
fn fixture_record(id: u64, endpoint: Endpoint, key: &str) -> Record {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/live-endpoints.json")).unwrap();
    Record::Observation {
        observation: Box::new(Observation::Success {
            evidence: evidence(id, endpoint),
            payload: decode::live(endpoint, &serde_json::to_vec(&value[key]).unwrap()).unwrap(),
        }),
    }
}
fn replay(bytes: &[u8]) -> (Recovery, Vec<Record>) {
    let mut rows = Vec::new();
    let recovery = recover(Cursor::new(bytes), |batch, _| {
        rows.extend(batch.iter().map(|e| e.record.clone()));
        Ok(())
    })
    .unwrap();
    (recovery, rows)
}

#[test]
fn bounded_pages_resume_at_validated_commits_without_replaying_history() {
    let memory = Memory::default();
    let header = Header::new(&scope());
    let mut writer = Writer::new(memory.clone(), header.clone()).unwrap();
    let initial = writer.position().clone();
    for id in 1..=5 {
        writer
            .commit(&[fixture_record(id, Endpoint::GameStats, "gs")])
            .unwrap();
    }
    let mut bytes = memory.0.borrow().bytes.clone();
    bytes.extend_from_slice(b"{\"incomplete\":");
    let mut position = initial;
    let mut ids = Vec::new();
    let mut pages = 0;
    loop {
        let page = recover_after(Cursor::new(&bytes), &header, &position, 2, |batch, _| {
            ids.extend(batch.iter().map(|entry| entry.sequence));
            Ok(())
        })
        .unwrap();
        pages += 1;
        position = page.recovery.position;
        if page.complete {
            assert_eq!(page.recovery.tail_bytes, 14);
            break;
        }
        assert_eq!(page.recovery.tail_bytes, 0);
    }
    assert_eq!(pages, 3);
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    let mut wrong = header;
    wrong.run_id = MediaId::new_v4();
    assert!(recover_after(Cursor::new(&bytes), &wrong, &position, 2, |_, _| Ok(())).is_err());
}

#[test]
fn committed_batches_round_trip_all_source_fields_and_equal_observations() {
    let output = Memory::default();
    let mut writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    let records = vec![
        fixture_record(1, Endpoint::GameStats, "gs"),
        fixture_record(2, Endpoint::PlayerList, "pl"),
        fixture_record(3, Endpoint::Events, "ev"),
        fixture_record(4, Endpoint::ActivePlayer, "ap"),
    ];
    writer.commit(&records).unwrap();
    let equal = fixture_record(5, Endpoint::PlayerList, "pl");
    writer.commit(std::slice::from_ref(&equal)).unwrap();
    let (recovered, actual) = replay(&output.0.borrow().bytes);
    assert_eq!(&recovered.position, writer.position());
    assert_eq!(recovered.tail_bytes, 0);
    assert_eq!(&actual[..4], records.as_slice());
    assert_eq!(actual[4], equal);
    assert_eq!(output.0.borrow().syncs, 3);
}

#[test]
fn every_torn_suffix_recovers_only_whole_committed_batches() {
    let output = Memory::default();
    let mut writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    let header_end = writer.position().end_offset as usize;
    writer
        .commit(&[fixture_record(1, Endpoint::GameStats, "gs")])
        .unwrap();
    let first_end = writer.position().end_offset as usize;
    writer
        .commit(&[
            fixture_record(2, Endpoint::ActivePlayer, "ap"),
            fixture_record(3, Endpoint::GameStats, "gs"),
        ])
        .unwrap();
    let bytes = &output.0.borrow().bytes;
    for cut in header_end..=bytes.len() {
        let (recovery, rows) = replay(&bytes[..cut]);
        let expected_count = if cut == bytes.len() {
            3
        } else if cut >= first_end {
            1
        } else {
            0
        };
        let expected_end = if cut == bytes.len() {
            bytes.len()
        } else if cut >= first_end {
            first_end
        } else {
            header_end
        };
        assert_eq!(rows.len(), expected_count, "cut {cut}");
        assert_eq!(recovery.position.end_offset, expected_end as u64);
        assert_eq!(recovery.tail_bytes, (cut - expected_end) as u64);
    }
}

#[test]
fn interior_corruption_and_commit_reordering_are_rejected() {
    let output = Memory::default();
    let mut writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    writer
        .commit(&[fixture_record(1, Endpoint::ActivePlayer, "ap")])
        .unwrap();
    writer
        .commit(&[fixture_record(2, Endpoint::GameStats, "gs")])
        .unwrap();
    let bytes = output.0.borrow().bytes.clone();
    let mut corrupted = String::from_utf8(bytes.clone()).unwrap();
    assert!(corrupted.contains("Fixture#TEST"));
    corrupted = corrupted.replace("Fixture#TEST", "Corrupt#TEST");
    let mut accepted = 0;
    assert!(matches!(
        recover(Cursor::new(corrupted), |rows, _| {
            accepted += rows.len();
            Ok(())
        }),
        Err(Error::Invalid("commit integrity"))
    ));
    assert_eq!(accepted, 0);
    let text = String::from_utf8(bytes).unwrap();
    let mut lines: Vec<_> = text.split_inclusive('\n').collect();
    lines.swap(2, 4);
    assert!(recover(Cursor::new(lines.concat()), |_, _| Ok(())).is_err());
}

#[test]
fn write_flush_and_sync_failures_never_advance_acknowledged_position() {
    for failure in 0..3 {
        let output = Memory::default();
        let mut writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
        let before = writer.position().clone();
        {
            let mut state = output.0.borrow_mut();
            match failure {
                0 => state.fail_after = Some(state.bytes.len() + 35),
                1 => state.fail_flush = true,
                _ => state.fail_sync = true,
            }
        }
        assert!(matches!(
            writer.commit(&[fixture_record(1, Endpoint::PlayerList, "pl")]),
            Err(Error::Io(_))
        ));
        assert_eq!(writer.position(), &before);
        assert!(matches!(
            writer.commit(&[fixture_record(2, Endpoint::GameStats, "gs")]),
            Err(Error::Poisoned)
        ));
        if failure == 0 {
            assert_eq!(replay(&output.0.borrow().bytes).1.len(), 0);
        }
        // A failed flush may leave readable complete bytes, but they were never
        // acknowledged to the running acquisition owner.
    }
}

#[test]
fn scope_and_capacity_errors_write_nothing_and_do_not_poison_writer() {
    let output = Memory::default();
    let mut writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    let before = output.0.borrow().bytes.len();
    let mut bad = evidence(1, Endpoint::ActivePlayer);
    bad.source.run_id = bad.source.media_id.clone();
    let bad = Record::Boundary {
        evidence: bad,
        active: true,
    };
    assert!(matches!(writer.commit(&[bad]), Err(Error::Invalid(_))));
    let huge = Record::Observation {
        observation: Box::new(Observation::Success {
            evidence: evidence(2, Endpoint::ActivePlayer),
            payload: Payload::ActivePlayer(ActiveIdentity {
                riot_id: Some("x".repeat(MAX_BATCH_BYTES)),
                ..Default::default()
            }),
        }),
    };
    assert!(matches!(writer.commit(&[huge]), Err(Error::Capacity)));
    assert_eq!(output.0.borrow().bytes.len(), before);
    writer
        .commit(&[fixture_record(3, Endpoint::GameStats, "gs")])
        .unwrap();
    assert_eq!(writer.position().last_sequence, 1);
    let oversized = vec![b'x'; MAX_BATCH_BYTES + 1];
    assert!(matches!(
        recover(Cursor::new(oversized), |_, _| Ok(())),
        Err(Error::Capacity)
    ));
}

#[test]
fn event_revisions_commit_once_while_every_successful_poll_keeps_coverage() {
    let output = Memory::default();
    let writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    let mut writer = RetainingWriter::new(writer);
    writer
        .commit_acquired(&[fixture_record(1, Endpoint::Events, "ev")])
        .unwrap();
    let retained = writer.retained_event_ids();
    assert!(retained > 0);
    writer
        .commit_acquired(&[fixture_record(2, Endpoint::Events, "ev")])
        .unwrap();
    assert_eq!(writer.retained_event_ids(), retained);
    let (_, rows) = replay(&output.0.borrow().bytes);
    assert!(
        matches!(&rows[0], Record::EventObservation { revisions, .. } if !revisions.is_empty())
    );
    let Record::EventObservation {
        evidence,
        revisions,
    } = &rows[1]
    else {
        panic!()
    };
    assert_eq!(evidence.id, ObservationId(2));
    // Invalid envelopes without an ID cannot deduplicate by source identity.
    assert!(revisions.iter().all(|r| r.event.source_id.is_none()));
}

#[test]
fn event_capacity_is_durable_degraded_coverage_and_failed_commit_does_not_ack() {
    let output = Memory::default();
    let writer = Writer::new(output.clone(), Header::new(&scope())).unwrap();
    let mut writer = RetainingWriter::with_event_capacity(writer, 0);
    writer
        .commit_acquired(&[fixture_record(1, Endpoint::Events, "ev")])
        .unwrap();
    assert_eq!(writer.retained_event_ids(), 0);
    assert!(
        matches!(&replay(&output.0.borrow().bytes).1[0], Record::Observation { observation } if matches!(observation.as_ref(), Observation::Gap { reason: GapReason::Capacity, .. }))
    );
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    output.0.borrow_mut().fail_sync = true;
    assert!(
        writer
            .commit_acquired(&[fixture_record(1, Endpoint::Events, "ev")])
            .is_err()
    );
    assert_eq!(writer.retained_event_ids(), 0);
    assert_eq!(writer.position().last_sequence, 0);
}

#[test]
fn stale_resets_cannot_erase_committed_event_deduplication() {
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    writer
        .commit_acquired(&[fixture_record(1, Endpoint::Events, "ev")])
        .unwrap();
    let before = writer.position().clone();
    let retained = writer.retained_event_ids();
    let stale = Record::SourceReset {
        evidence: evidence(2, Endpoint::GameStats),
        previous_epoch: 0,
    };
    assert!(matches!(
        writer.commit_acquired(&[stale, fixture_record(3, Endpoint::Events, "ev")]),
        Err(Error::Invalid("source epoch transition"))
    ));
    assert_eq!(writer.position(), &before);
    assert_eq!(writer.retained_event_ids(), retained);
    writer
        .commit_acquired(&[fixture_record(4, Endpoint::Events, "ev")])
        .unwrap();
    assert!(
        matches!(&replay(&output.0.borrow().bytes).1[1], Record::EventObservation { revisions, .. } if revisions.iter().all(|r| r.event.source_id.is_none()))
    );
    let mut reset_evidence = evidence(5, Endpoint::GameStats);
    reset_evidence.source.epoch = 1;
    let mut new_events = fixture_record(6, Endpoint::Events, "ev");
    if let Record::Observation { observation } = &mut new_events
        && let Observation::Success { evidence, .. } = observation.as_mut()
    {
        evidence.source.epoch = 1;
    }
    writer
        .commit_acquired(&[
            Record::SourceReset {
                evidence: reset_evidence,
                previous_epoch: 0,
            },
            new_events,
        ])
        .unwrap();
    assert!(
        matches!(replay(&output.0.borrow().bytes).1.last().unwrap(), Record::EventObservation { revisions, .. } if revisions.iter().any(|r| r.event.source_id.is_some()))
    );
}

#[test]
fn committed_events_rebuild_the_same_lookup_and_reject_forged_revisions() {
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    writer
        .commit_acquired(&[fixture_record(1, Endpoint::Events, "ev")])
        .unwrap();
    writer
        .commit_acquired(&[fixture_record(2, Endpoint::Events, "ev")])
        .unwrap();
    let mut tracker = events::EventTracker::new(scope());
    let (_, rows) = replay(&output.0.borrow().bytes);
    for record in &rows {
        if let Record::EventObservation {
            evidence,
            revisions,
        } = record
        {
            tracker
                .restore_committed(&evidence.source, revisions)
                .unwrap();
        }
    }
    assert_eq!(tracker.retained_ids(), writer.retained_event_ids());
    let Record::Observation { observation } = fixture_record(3, Endpoint::Events, "ev") else {
        panic!()
    };
    let Observation::Success {
        payload: Payload::Events(events),
        ..
    } = *observation
    else {
        panic!()
    };
    assert!(
        tracker
            .prepare(&scope(), events)
            .unwrap()
            .revisions()
            .iter()
            .all(|r| r.event.source_id.is_none())
    );
    let Record::EventObservation { mut revisions, .. } = rows[0].clone() else {
        panic!()
    };
    revisions[0].fingerprint[0] ^= 1;
    let mut empty = events::EventTracker::new(scope());
    assert_eq!(
        empty.restore_committed(&scope(), &revisions),
        Err(GapReason::InvalidShape)
    );
    assert_eq!(empty.retained_ids(), 0);
}

#[test]
fn unchanged_rosters_share_payloads_and_reader_reconstructs_every_observation() {
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    let mut expected = Vec::new();
    for id in 1..=32 {
        let mut row = fixture_record(id, Endpoint::PlayerList, "pl");
        if let Record::Observation { observation } = &mut row
            && let Observation::Success {
                evidence,
                payload: Payload::PlayerList(players),
            } = observation.as_mut()
        {
            evidence.timing.start_us = (id - 1) * 2_000_000;
            evidence.timing.end_us = evidence.timing.start_us + 100;
            // A missing/error transition is a real changed payload, not equality.
            if id == 4 {
                players[0].scores = None;
                players[0]
                    .issues
                    .insert("scores".into(), FieldIssue::SourceError);
            }
            expected.push(players.clone());
        }
        writer.commit_acquired(&[row]).unwrap();
    }
    let (_, rows) = replay(&output.0.borrow().bytes);
    let mut reader = roster::RosterReader::default();
    let mut fulls = 0;
    for (i, row) in rows.iter().enumerate() {
        let Record::RosterObservation { evidence, roster } = row else {
            panic!()
        };
        assert_eq!(
            reader.observe(&evidence.source, roster).unwrap(),
            expected[i]
        );
        assert!(reader.retained_payloads() <= MAX_ROSTER);
        if roster.full {
            fulls += 1;
        }
        if i == 1 {
            assert!(roster.payloads.is_empty());
        }
        if i == 3 || i == 4 {
            assert_eq!(roster.payloads.len(), 1);
        }
        if i == 30 {
            assert!(roster.full);
            let mut fresh = roster::RosterReader::default();
            assert_eq!(
                fresh.observe(&evidence.source, roster).unwrap(),
                expected[i]
            );
        }
    }
    assert_eq!(fulls, 2);
}

#[test]
fn rejected_roster_preparations_and_missing_references_do_not_change_state() {
    let mut store = roster::RosterStore::new(scope());
    let roster = vec![RosterPlayer {
        riot_id: Some("P#T".into()),
        ..Default::default()
    }];
    let a = store.prepare(&scope(), 0, &roster).unwrap();
    let b = store.prepare(&scope(), 0, &roster).unwrap();
    let first = a.observation().clone();
    store.acknowledge(a).unwrap();
    assert_eq!(store.acknowledge(b), Err(GapReason::SourceReset));
    let equal = store.prepare(&scope(), 1, &roster).unwrap();
    assert!(equal.observation().payloads.is_empty());
    let mut reader = roster::RosterReader::default();
    assert_eq!(
        reader.observe(&scope(), equal.observation()),
        Err(GapReason::SourceReset)
    );
    assert_eq!(reader.retained_payloads(), 0);
    assert_eq!(reader.observe(&scope(), &first).unwrap(), roster);
    let mut missing = equal.observation().clone();
    missing.references[0] = roster::PayloadId(999);
    assert_eq!(
        reader.observe(&scope(), &missing),
        Err(GapReason::InvalidShape)
    );
    assert_eq!(
        reader.observe(&scope(), equal.observation()).unwrap(),
        roster
    );
}

struct TemporaryLibrary(std::path::PathBuf);
impl TemporaryLibrary {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "chronobreak-journal-fixture-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TemporaryLibrary {
    fn drop(&mut self) {
        // Remove only known files in this newly created fixture; never recurse or
        // touch a selected library/recording directory.
        for name in ["journal.jsonl", "ready"] {
            let _ = std::fs::remove_file(self.0.join(name));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn real_file_creation_never_truncates_an_existing_journal() {
    let temp = TemporaryLibrary::new();
    let path = temp.0.join("journal.jsonl");
    let mut writer = Writer::create_new(&path, Header::new(&scope())).unwrap();
    writer
        .commit(&[fixture_record(1, Endpoint::PlayerList, "pl")])
        .unwrap();
    drop(writer);
    let original = std::fs::read(&path).unwrap();
    assert!(
        matches!(Writer::create_new(&path, Header::new(&scope())), Err(Error::Io(e)) if e.kind() == io::ErrorKind::AlreadyExists)
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let recovered = recover(
        std::io::BufReader::new(std::fs::File::open(path).unwrap()),
        |_, _| Ok(()),
    )
    .unwrap();
    assert_eq!(recovered.position.last_sequence, 1);
    assert_eq!(recovered.tail_bytes, 0);
}

struct KillPointFile {
    file: std::fs::File,
    marker: std::path::PathBuf,
    phase: String,
    armed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    writes: usize,
}
impl KillPointFile {
    fn stop_here(&self) -> ! {
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.marker)
            .unwrap()
            .sync_all()
            .unwrap();
        loop {
            std::thread::park();
        }
    }
    fn armed(&self) -> bool {
        self.armed.load(std::sync::atomic::Ordering::SeqCst)
    }
}
impl Write for KillPointFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.armed() {
            self.writes += 1;
            if (self.phase == "entry" && self.writes == 1)
                || (self.phase == "commit" && self.writes == 2)
            {
                self.file.write_all(&bytes[..bytes.len() / 2])?;
                self.stop_here();
            }
        }
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.armed() && self.phase == "flush" {
            self.stop_here();
        }
        self.file.flush()
    }
}
impl DurableWrite for KillPointFile {
    fn sync_data(&mut self) -> io::Result<()> {
        if self.armed() && self.phase == "sync" {
            self.stop_here();
        }
        self.file.sync_data()
    }
}

// Executed only by the bounded parent test with its own new temporary directory.
#[test]
fn journal_process_child() {
    let Some(directory) = std::env::var_os("CHRONOBREAK_JOURNAL_KILL_FIXTURE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    assert!(directory.starts_with(std::env::temp_dir()));
    let phase = std::env::var("CHRONOBREAK_JOURNAL_KILL_PHASE").unwrap();
    assert!(["entry", "commit", "flush", "sync"].contains(&phase.as_str()));
    let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let file = KillPointFile {
        file: std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join("journal.jsonl"))
            .unwrap(),
        marker: directory.join("ready"),
        phase,
        armed: std::sync::Arc::clone(&armed),
        writes: 0,
    };
    let mut writer = Writer::new(file, Header::new(&scope())).unwrap();
    writer
        .commit(&[fixture_record(1, Endpoint::GameStats, "gs")])
        .unwrap();
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    writer
        .commit(&[fixture_record(2, Endpoint::PlayerList, "pl")])
        .unwrap();
    panic!("child must stop at its requested fault point");
}

#[test]
fn killing_writer_at_real_write_and_flush_boundaries_recovers_valid_prefix() {
    for phase in ["entry", "commit", "flush", "sync"] {
        let temp = TemporaryLibrary::new();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "journal_process_child", "--nocapture"])
            .env("CHRONOBREAK_JOURNAL_KILL_FIXTURE", &temp.0)
            .env("CHRONOBREAK_JOURNAL_KILL_PHASE", phase)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !temp.0.join("ready").exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = temp.0.join("ready").exists();
        let _ = child.kill();
        child.wait().unwrap();
        assert!(ready, "child did not reach {phase}");
        let input =
            std::io::BufReader::new(std::fs::File::open(temp.0.join("journal.jsonl")).unwrap());
        let mut count = 0;
        let recovered = recover(input, |rows, _| {
            count += rows.len();
            Ok(())
        })
        .unwrap();
        // Complete unsynced bytes may still be readable after process termination;
        // this tests process death, not arbitrary machine/hardware power loss.
        let expected = if phase == "entry" || phase == "commit" {
            1
        } else {
            2
        };
        assert_eq!(count, expected, "phase {phase}");
        assert_eq!(recovered.position.last_sequence, expected as u64);
        assert_eq!(recovered.tail_bytes > 0, expected == 1);
    }
}

#[test]
fn checkpoint_contains_only_committed_current_state_and_resumes_roster_references() {
    let output = Memory::default();
    let header = Header::new(&scope());
    let writer = Writer::new(output.clone(), header.clone()).unwrap();
    let initial_position = writer.position().clone();
    let mut state = checkpoint::State::new(header.clone(), initial_position.clone());
    let mut writer = RetainingWriter::new(writer);
    writer
        .commit_acquired_with(
            &[
                fixture_record(1, Endpoint::PlayerList, "pl"),
                fixture_record(2, Endpoint::GameStats, "gs"),
            ],
            |rows, position| state.apply(rows, position).unwrap(),
        )
        .unwrap();
    let snapshot = state.snapshot().clone();
    assert_eq!(&snapshot.position, writer.position());
    assert!(snapshot.roster.as_ref().unwrap().1.full);
    let mut resumed =
        checkpoint::State::from_checkpoint(snapshot.clone(), &header, writer.position()).unwrap();
    assert!(
        checkpoint::State::from_checkpoint(snapshot.clone(), &header, &initial_position).is_err()
    );
    let mut second_rows = Vec::new();
    writer
        .commit_acquired_with(
            &[fixture_record(3, Endpoint::PlayerList, "pl")],
            |rows, position| {
                second_rows = rows.to_vec();
                state.apply(rows, position).unwrap();
                resumed.apply(rows, position).unwrap();
            },
        )
        .unwrap();
    assert!(
        matches!(&second_rows[0], Record::RosterObservation { roster, .. } if roster.payloads.is_empty())
    );
    assert_eq!(resumed.snapshot(), state.snapshot());
    let original = resumed.snapshot().clone();
    let mut future = writer.position().clone();
    future.last_sequence += 2;
    future.end_offset += 1;
    let broken = Record::RosterObservation {
        evidence: evidence(4, Endpoint::PlayerList),
        roster: roster::RosterObservation {
            full: false,
            references: vec![roster::PayloadId(999)],
            payloads: vec![],
        },
    };
    // A valid state change followed by a bad roster must leave the entire
    // checkpoint untouched, including the private reference dictionary.
    assert!(
        resumed
            .apply(
                &[fixture_record(4, Endpoint::GameStats, "gs"), broken],
                &future
            )
            .is_err()
    );
    assert_eq!(resumed.snapshot(), &original);
    future.last_sequence -= 1;
    resumed.apply(&second_rows, &future).unwrap();

    // Replaying the validated source obtains the same checkpoint as incremental
    // updates; no event history or old roster payloads enter the checkpoint.
    let mut rebuilt = checkpoint::State::new(header, initial_position);
    recover(
        Cursor::new(&output.0.borrow().bytes),
        |entries, position| {
            rebuilt.apply(
                &entries.iter().map(|e| e.record.clone()).collect::<Vec<_>>(),
                position,
            )
        },
    )
    .unwrap();
    assert_eq!(rebuilt.snapshot(), state.snapshot());
}

#[test]
fn one_flush_deduplicates_multiple_responses_without_acknowledging_failed_writes() {
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    let rows = [
        fixture_record(1, Endpoint::Events, "ev"),
        fixture_record(2, Endpoint::PlayerList, "pl"),
        fixture_record(3, Endpoint::Events, "ev"),
        fixture_record(4, Endpoint::PlayerList, "pl"),
    ];
    let charge: usize = rows.iter().map(Record::queue_charge).sum();
    let initial_offset = writer.position().end_offset;
    writer.commit_owned_with(rows.to_vec(), |_, _| {}).unwrap();
    assert!(writer.position().end_offset - initial_offset <= charge as u64);
    assert_eq!(output.0.borrow().syncs, 2); // header + one grouped commit
    let (_, stored) = replay(&output.0.borrow().bytes);
    assert!(
        matches!(&stored[2], Record::EventObservation { revisions, .. } if revisions.iter().all(|r| r.event.source_id.is_none()))
    );
    assert!(
        matches!(&stored[3], Record::RosterObservation { roster, .. } if roster.payloads.is_empty())
    );
    let output = Memory::default();
    let mut writer =
        RetainingWriter::new(Writer::new(output.clone(), Header::new(&scope())).unwrap());
    output.0.borrow_mut().fail_sync = true;
    assert!(writer.commit_acquired(&rows).is_err());
    assert_eq!(writer.retained_event_ids(), 0);
    assert_eq!(writer.position().last_sequence, 0);
}

#[test]
fn checkpoint_serialization_caps_combined_current_state() {
    let output = Memory::default();
    let header = Header::new(&scope());
    let writer = Writer::new(output, header.clone()).unwrap();
    let mut state = checkpoint::State::new(header, writer.position().clone());
    let mut writer = RetainingWriter::new(writer);
    writer
        .commit_acquired_with(
            &[fixture_record(1, Endpoint::PlayerList, "pl")],
            |rows, position| state.apply(rows, position).unwrap(),
        )
        .unwrap();
    let mut snapshot = state.snapshot().clone();
    let baseline = snapshot.encode().unwrap();
    // Each source observation fits independently; their retained combination
    // must also fit the checkpoint's hard serialized-byte budget.
    let (_, roster) = snapshot.roster.as_mut().unwrap();
    roster.payloads[0].player.riot_id =
        Some("x".repeat(checkpoint::MAX_CHECKPOINT_BYTES - baseline.len()));
    assert!(snapshot.encode().is_ok());
    snapshot.game_stats = Some((
        evidence(2, Endpoint::GameStats),
        GameStats {
            map_name: Some("y".repeat(4096)),
            ..Default::default()
        },
    ));
    assert!(matches!(snapshot.encode(), Err(Error::Capacity)));
}
