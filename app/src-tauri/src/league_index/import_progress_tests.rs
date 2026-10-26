//! Explicit, small-library cost characterization; never touches a user library.
use super::*;
#[cfg(windows)]
use std::time::Instant;
#[cfg(windows)]
use windows::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetCurrentProcess, GetProcessIoCounters, GetProcessTimes, IO_COUNTERS},
};

#[cfg(windows)]
struct Usage {
    cpu_100ns: u64,
    io: IO_COUNTERS,
}
#[cfg(windows)]
impl Usage {
    fn now() -> Self {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let mut io = IO_COUNTERS::default();
        // SAFETY: current-process pseudo-handle and live writable output values.
        unsafe {
            let process = GetCurrentProcess();
            GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user).unwrap();
            GetProcessIoCounters(process, &mut io).unwrap();
        }
        let ticks =
            |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
        Self {
            cpu_100ns: ticks(kernel) + ticks(user),
            io,
        }
    }
}

#[cfg(windows)]
fn measure(fixture: &Fixture, index: &mut Index, name: &str, calls: usize) -> Vec<ImportProgress> {
    let before = Usage::now();
    let start = Instant::now();
    let progress: Vec<_> = (0..calls)
        .map(|_| {
            index
                .recording(fixture.root.path(), &fixture.summary, false, &|| Ok(()))
                .unwrap()
        })
        .collect();
    let wall = start.elapsed();
    let after = Usage::now();
    println!(
        "{name}: calls={calls} wall_ms={:.3} cpu_ms={:.3} read_ops={} write_ops={} read_bytes={} write_bytes={} first={} last={}",
        wall.as_secs_f64() * 1000.0,
        (after.cpu_100ns - before.cpu_100ns) as f64 / 10_000.0,
        after.io.ReadOperationCount - before.io.ReadOperationCount,
        after.io.WriteOperationCount - before.io.WriteOperationCount,
        after.io.ReadTransferCount - before.io.ReadTransferCount,
        after.io.WriteTransferCount - before.io.WriteTransferCount,
        serde_json::to_string(progress.first().unwrap()).unwrap(),
        serde_json::to_string(progress.last().unwrap()).unwrap()
    );
    progress
}

fn append_uncommitted_entry(fixture: &mut Fixture) -> Vec<u8> {
    fixture.game_stats(999, 9_000_000, false);
    let path = fixture.directory.join(JOURNAL);
    let bytes = fs::read(&path).unwrap();
    // Preserve the valid Entry line, withholding only its final Commit line.
    let commit_start = bytes[..bytes.len() - 1]
        .iter()
        .rposition(|b| *b == b'\n')
        .unwrap()
        + 1;
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(commit_start as u64)
        .unwrap();
    bytes[commit_start..].to_vec()
}

#[test]
fn stable_uncommitted_tails_settle_without_retries_and_resume_after_commit() {
    for complete_entry in [false, true] {
        let mut fixture = Fixture::new(1);
        let mut index = fixture.index();
        fixture.ready(&mut index);
        let before = index.progress(&fixture.source.media_id, 0).unwrap();
        let path = fixture.directory.join(JOURNAL);
        let committed_len = fs::metadata(&path).unwrap().len();
        let commit = if complete_entry {
            append_uncommitted_entry(&mut fixture)
        } else {
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(b"{\"torn\":")
                .unwrap();
            Vec::new()
        };
        let tail_len = fs::metadata(&path).unwrap().len() - committed_len;
        assert!(tail_len > 0);
        for _ in 0..3 {
            let progress = index
                .recording(fixture.root.path(), &fixture.summary, false, &|| Ok(()))
                .unwrap();
            assert_eq!((progress.status.as_str(), progress.batches), ("ready", 0));
            assert_eq!(progress.position, before.position);
            assert_eq!(progress.tail_bytes, tail_len);
        }
        if complete_entry {
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(&commit)
                .unwrap();
        } else {
            fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(committed_len)
                .unwrap();
            fixture.game_stats(999, 9_000_000, true);
        }
        let progress = index
            .recording(fixture.root.path(), &fixture.summary, false, &|| Ok(()))
            .unwrap();
        assert_eq!(
            (
                progress.status.as_str(),
                progress.batches,
                progress.tail_bytes
            ),
            ("ready", 1, 0)
        );
        assert!(
            progress.position.as_ref().unwrap().end_offset > before.position.unwrap().end_offset
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "explicit native import CPU/IO characterization on generated libraries"]
fn import_retry_measurement() {
    let fixture = Fixture::new(100);
    let mut index = fixture.index();
    // An exact four-commit boundary needs one final empty read to establish EOF.
    let pages = measure(&fixture, &mut index, "productive_100_commits", 26);
    assert!(pages[..25].iter().all(|p| p.batches == 4));
    assert!(
        pages[..25]
            .windows(2)
            .all(|p| p[1].position.as_ref().unwrap().end_offset
                > p[0].position.as_ref().unwrap().end_offset)
    );
    assert_eq!(pages.last().unwrap().batches, 0);
    assert_eq!(pages.last().unwrap().status, "ready");

    let mut fixture = Fixture::new(1);
    let mut index = fixture.index();
    fixture.ready(&mut index);
    let journal = fixture.directory.join(JOURNAL);
    let committed_len = fs::metadata(&journal).unwrap().len();
    fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"{\"torn\":")
        .unwrap();
    let stable = measure(&fixture, &mut index, "unchanged_tail", 100);
    // A torn final line is EOF-complete: it retains unknown tail coverage but
    // does not itself ask the frontend to continue importing.
    assert!(stable.iter().all(|p| p.status == "ready"
        && p.batches == 0
        && p.tail_bytes == 8
        && p.position == stable[0].position));
    fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b" ")
        .unwrap();
    let grown = measure(&fixture, &mut index, "grown_tail", 1);
    assert_eq!(grown[0].tail_bytes, 9);
    assert_eq!(grown[0].position, stable[0].position);
    fs::OpenOptions::new()
        .write(true)
        .open(&journal)
        .unwrap()
        .set_len(committed_len)
        .unwrap();
    fixture.game_stats(999, 9_000_000, true);
    let resumed = measure(&fixture, &mut index, "new_commit", 1);
    assert_eq!(resumed[0].status, "ready");
    assert_eq!(resumed[0].batches, 1);
    assert_eq!(resumed[0].tail_bytes, 0);
    assert!(
        resumed[0].position.as_ref().unwrap().end_offset
            > stable[0].position.as_ref().unwrap().end_offset
    );

    let mut fixture = Fixture::new(1);
    let mut index = fixture.index();
    fixture.ready(&mut index);
    let commit = append_uncommitted_entry(&mut fixture);
    let stable = measure(&fixture, &mut index, "complete_uncommitted_entry", 100);
    assert!(stable.iter().all(|p| p.status == "ready"
        && p.batches == 0
        && p.tail_bytes > 0
        && p.position == stable[0].position));
    fs::OpenOptions::new()
        .append(true)
        .open(fixture.directory.join(JOURNAL))
        .unwrap()
        .write_all(&commit)
        .unwrap();
    let committed = measure(&fixture, &mut index, "entry_commit_arrives", 1);
    assert_eq!(
        (
            committed[0].status.as_str(),
            committed[0].batches,
            committed[0].tail_bytes
        ),
        ("ready", 1, 0)
    );
}
