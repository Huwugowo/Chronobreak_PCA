//! Real process arbitration using only a sentinel-owned generated library.

use super::*;
use crate::library;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SENTINEL: &str = "QUEUEBACK_LIBRARY_MUTATION_FIXTURE";
const CHILD_TEST: &str = "library_mutation::process_tests::mutation_child";

fn wait_for(path: &Path) {
    let started = Instant::now();
    while !path.is_file() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "missing barrier {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn mutation_child() {
    let Some(fixture) = std::env::var_os("CHRONOBREAK_MUTATION_FIXTURE") else {
        return;
    };
    let fixture = std::path::PathBuf::from(fixture);
    assert!(fixture.join(SENTINEL).is_file());
    let name = std::env::var("CHRONOBREAK_MUTATION_LIBRARY").unwrap();
    assert!(matches!(name.as_str(), "library" | "alias" | "other"));
    let root = fixture.join(name);
    let label = std::env::var("CHRONOBREAK_MUTATION_LABEL").unwrap();
    assert!(label.chars().all(|c| c.is_ascii_alphanumeric()));
    let operation = std::env::var("CHRONOBREAK_MUTATION_OPERATION").unwrap();
    let phase = std::env::var("CHRONOBREAK_MUTATION_PHASE").unwrap();
    let ready = fixture.join(format!("{label}-ready"));
    let go = fixture.join(format!("{label}-go"));
    if phase == "observe" {
        assert!(!library::list_games(&root).unwrap()[0].saved);
        fs::write(&ready, b"observed unsaved").unwrap();
        wait_for(&go);
    }
    let approved = ApprovedRoot::new(&root).unwrap();
    let result = match LibraryMutation::acquire(&root, &approved) {
        Ok(owner) => {
            let result = match operation.as_str() {
                "save" => library::save_game(&owner, "100", true).map(|_| "ok".to_owned()),
                "delete" => library::delete_game(&owner, "100").map(|_| "ok".to_owned()),
                "retention" => {
                    library::run_auto_delete(&owner, 1).map(|r| format!("ok:{}", r.deleted_count))
                }
                "hold" => Ok("ok".to_owned()),
                _ => panic!("invalid fixture operation"),
            }
            .unwrap_or_else(|e| format!("err:{e:#}"));
            fs::write(fixture.join(format!("{label}-result")), &result).unwrap();
            if phase == "held" {
                fs::write(&ready, b"lease held after effects").unwrap();
                wait_for(&go);
            }
            result
        }
        Err(error) => format!("err:{error:#}"),
    };
    fs::write(fixture.join(format!("{label}-result")), result).unwrap();
}

struct FixtureChild(Child);
impl FixtureChild {
    fn finish(&mut self) {
        let started = Instant::now();
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "mutation test child failed: {status}");
                return;
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "mutation test child hung"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for FixtureChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(fixture: &Path, library: &str, label: &str, operation: &str, phase: &str) -> FixtureChild {
    FixtureChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env("CHRONOBREAK_MUTATION_FIXTURE", fixture)
            .env("CHRONOBREAK_MUTATION_LIBRARY", library)
            .env("CHRONOBREAK_MUTATION_LABEL", label)
            .env("CHRONOBREAK_MUTATION_OPERATION", operation)
            .env("CHRONOBREAK_MUTATION_PHASE", phase)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    )
}

fn fixture() -> tempfile::TempDir {
    let fixture = tempfile::tempdir().unwrap();
    fs::write(
        fixture.path().join(SENTINEL),
        b"generated test libraries only",
    )
    .unwrap();
    for name in ["library", "other"] {
        library::tests::write_game(
            &fixture.path().join(name),
            "100",
            "2020-01-01T00:00:00Z",
            false,
        );
    }
    super::tests::junction(
        &fixture.path().join("alias"),
        &fixture.path().join("library"),
    );
    fixture
}

fn result(fixture: &Path, label: &str) -> String {
    fs::read_to_string(fixture.join(format!("{label}-result"))).unwrap()
}

#[test]
fn actual_process_save_excludes_deletion_and_stale_retention_observations() {
    for operation in ["delete", "retention"] {
        let fixture = fixture();
        let path = fixture.path();
        let mut stale = spawn(path, "library", "stale", operation, "observe");
        wait_for(&path.join("stale-ready"));
        let mut saver = spawn(path, "library", "saver", "save", "held");
        wait_for(&path.join("saver-ready"));
        assert_eq!(result(path, "saver"), "ok");
        let mut contender = spawn(path, "alias", "contender", operation, "none");
        contender.finish();
        assert!(result(path, "contender").contains("busy"));
        fs::write(path.join("saver-go"), b"release").unwrap();
        saver.finish();
        fs::write(path.join("stale-go"), b"revalidate").unwrap();
        stale.finish();
        let outcome = result(path, "stale");
        if operation == "delete" {
            assert!(outcome.contains("saved"), "{outcome}");
        } else {
            assert_eq!(outcome, "ok:0");
        }
        assert!(library::list_games(&path.join("library")).unwrap()[0].saved);
        assert!(path.join("library/games/100/video.mp4").is_file());
    }
}

#[test]
fn actual_process_deletion_wins_before_save_with_truthful_results() {
    for operation in ["delete", "retention"] {
        let fixture = fixture();
        let path = fixture.path();
        let mut deleter = spawn(path, "library", "deleter", operation, "held");
        wait_for(&path.join("deleter-ready"));
        assert_eq!(
            result(path, "deleter"),
            if operation == "delete" { "ok" } else { "ok:1" }
        );
        let mut busy = spawn(path, "alias", "busy", "save", "none");
        busy.finish();
        assert!(result(path, "busy").contains("busy"));
        fs::write(path.join("deleter-go"), b"release").unwrap();
        deleter.finish();
        let mut late = spawn(path, "library", "late", "save", "none");
        late.finish();
        assert!(result(path, "late").starts_with("err:"));
        assert!(!path.join("library/games/100").exists());
    }
}

#[test]
fn killed_process_releases_lease_and_unrelated_library_progresses() {
    let fixture = fixture();
    let path = fixture.path();
    let mut holder = spawn(path, "library", "holder", "hold", "held");
    wait_for(&path.join("holder-ready"));
    assert!(fs::rename(path.join("library"), path.join("replaced-library")).is_err());
    let mut busy = spawn(path, "alias", "busy", "save", "none");
    busy.finish();
    assert!(result(path, "busy").contains("busy"));
    let mut other = spawn(path, "other", "other", "save", "none");
    other.finish();
    assert_eq!(result(path, "other"), "ok");
    holder.0.kill().unwrap();
    holder.0.wait().unwrap();
    let mut next = spawn(path, "library", "next", "save", "none");
    next.finish();
    assert_eq!(result(path, "next"), "ok");
    assert!(library::list_games(&path.join("library")).unwrap()[0].saved);
}
