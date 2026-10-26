use super::*;
use anyhow::bail;

fn fixture() -> (tempfile::TempDir, ApprovedRoot) {
    let temporary = tempfile::tempdir().unwrap();
    let root = ApprovedRoot::new(temporary.path()).unwrap();
    (temporary, root)
}

fn abandon(lease: &Lease) -> PathBuf {
    let mut batch = lease.begin(vec!["1786000000_1786000010".into()]).unwrap();
    let path = batch.directory.as_ref().unwrap().path.clone();
    fs::write(batch.staged(0, "mp4"), b"incomplete generated media").unwrap();
    drop(batch.directory.take()); // Model abrupt loss of the staging owner.
    path
}

#[test]
fn live_lease_and_read_only_recovery_leave_staging_untouched() {
    let (_temporary, root) = fixture();
    assert_eq!(
        recover(&root, RecoveryMode::ReadOnly, || Ok(()))
            .unwrap()
            .recovered,
        0
    );
    assert!(!root.path().join("clips").exists());
    let lease = Lease::acquire(&root, true).unwrap().unwrap();
    let path = abandon(&lease);
    let report = recover(&root, RecoveryMode::Normal, || Ok(())).unwrap();
    assert!(report.active);
    assert!(report.summary().unwrap().contains("active=true"));
    assert!(path.join("0.mp4").exists());
    drop(lease);
    assert_eq!(
        recover(&root, RecoveryMode::Normal, || Ok(()))
            .unwrap()
            .recovered,
        1
    );
    assert!(!path.exists());
    assert_eq!(
        recover(&root, RecoveryMode::Normal, || Ok(()))
            .unwrap()
            .recovered,
        0
    );
}

#[test]
fn publication_collisions_preserve_every_final_path_and_completed_member() {
    let (_temporary, root) = fixture();
    let lease = Lease::acquire(&root, true).unwrap().unwrap();
    let batch = lease
        .begin(vec![
            "1786000000_1786000010".into(),
            "1786000000_1786000011".into(),
        ])
        .unwrap();
    fs::write(batch.staged(0, "mp4"), b"validated first").unwrap();
    fs::write(batch.staged(0, "jpg"), b"first thumbnail").unwrap();
    fs::write(batch.staged(1, "mp4"), b"validated second").unwrap();
    let existing = lease.clips().join("1786000000_1786000011.mp4");
    fs::write(&existing, b"preexisting user media").unwrap();
    batch.publish(0, "mp4").unwrap();
    let thumbnail = lease.clips().join("1786000000_1786000010.jpg");
    fs::write(&thumbnail, b"preexisting user thumbnail").unwrap();
    assert!(batch.publish(0, "jpg").is_err());
    assert!(batch.publish(1, "mp4").is_err());
    drop(batch);
    assert_eq!(fs::read(existing).unwrap(), b"preexisting user media");
    assert_eq!(fs::read(thumbnail).unwrap(), b"preexisting user thumbnail");
    assert_eq!(
        fs::read(lease.clips().join("1786000000_1786000010.mp4")).unwrap(),
        b"validated first"
    );
    assert_eq!(fs::read_dir(&lease.staging.path).unwrap().count(), 1); // Persistent lease only.
}

#[test]
fn invalid_ownership_foreign_entries_and_locked_files_are_retained() {
    let (_temporary, root) = fixture();
    let lease = Lease::acquire(&root, true).unwrap().unwrap();
    let invalid = [
        b"invalid json".to_vec(),
        vec![b'x'; MANIFEST_LIMIT as usize + 1],
        br#"{"version":99,"batch_id":"00000000000000000000000000000000","destinations":["1786000000_1"]}"#.to_vec(),
        br#"{"version":1,"batch_id":"00000000000000000000000000000000","destinations":["../source"]}"#.to_vec(),
    ];
    for bytes in invalid {
        let path = abandon(&lease);
        fs::write(path.join(OWNER), &bytes).unwrap();
        assert!(lease.recover(|| Ok(())).unwrap().retained >= 1);
        assert_eq!(fs::read(path.join(OWNER)).unwrap(), bytes);
        assert!(path.join("0.mp4").exists());
    }
    let foreign = abandon(&lease);
    fs::write(foreign.join("user-file"), b"foreign sentinel").unwrap();
    let locked = abandon(&lease);
    let file = open_regular(&locked.join("0.mp4"), false, false).unwrap();
    let report = lease.recover(|| Ok(())).unwrap();
    assert!(report.retained >= 2);
    assert!(foreign.join("0.mp4").exists());
    assert_eq!(
        fs::read(foreign.join("user-file")).unwrap(),
        b"foreign sentinel"
    );
    assert!(locked.join(OWNER).exists());
    drop(file);
    assert!(lease.recover(|| Ok(())).unwrap().recovered >= 1);
    assert!(!locked.exists());
}

#[test]
fn directory_guards_prevent_root_replacement_and_budget_rotates_past_retained_batches() {
    let (temporary, root) = fixture();
    let lease = Lease::acquire(&root, true).unwrap().unwrap();
    assert!(fs::rename(temporary.path(), temporary.path().with_extension("moved")).is_err());
    for index in 0..BATCH_LIMIT + 2 {
        let path = lease.staging.path.join(format!("{index:032x}"));
        fs::create_dir(&path).unwrap();
        fs::write(path.join(OWNER), b"unknown owner").unwrap();
    }
    let abandoned = abandon(&lease);
    let first = lease.recover(|| Ok(())).unwrap();
    assert_eq!(first.retained + first.recovered, BATCH_LIMIT);
    assert_eq!(first.deferred, 3);
    let second = lease.recover(|| Ok(())).unwrap();
    assert!(second.retained + second.recovered <= BATCH_LIMIT);
    // The rotating cursor eventually reaches all IDs, including after wrap.
    for _ in 0..3 {
        lease.recover(|| Ok(())).unwrap();
    }
    assert!(!abandoned.exists());
    assert!(lease.recover(|| bail!("superseded")).is_err());
}

#[test]
fn junction_batch_and_junction_staging_never_authorize_cleanup() {
    let (temporary, root) = fixture();
    let lease = Lease::acquire(&root, true).unwrap().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    fs::write(foreign.path().join("sentinel"), b"foreign data").unwrap();
    let junction = lease.staging.path.join("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
    let status = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(foreign.path())
        .output()
        .unwrap();
    assert!(status.status.success(), "junction fixture creation failed");
    assert!(lease.recover(|| Ok(())).unwrap().retained >= 1);
    assert_eq!(
        fs::read(foreign.path().join("sentinel")).unwrap(),
        b"foreign data"
    );
    fs::remove_dir(junction).unwrap();
    drop(lease);
    let staging = temporary.path().join("clips").join(STAGING);
    fs::remove_file(staging.join(".lease")).unwrap();
    fs::remove_dir(&staging).unwrap();
    let status = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&staging)
        .arg(foreign.path())
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(recover(&root, RecoveryMode::Normal, || Ok(())).is_err());
    assert_eq!(
        fs::read(foreign.path().join("sentinel")).unwrap(),
        b"foreign data"
    );
    fs::remove_dir(staging).unwrap();
}

#[test]
fn replaced_captured_root_does_not_authorize_recovery_in_a_new_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let library = temporary.path().join("library");
    fs::create_dir(&library).unwrap();
    let captured = ApprovedRoot::new(&library).unwrap();
    fs::rename(&library, temporary.path().join("previous-library")).unwrap();
    fs::create_dir(&library).unwrap();
    let current = ApprovedRoot::new(&library).unwrap();
    let lease = Lease::acquire(&current, true).unwrap().unwrap();
    let batch = abandon(&lease);
    drop(lease);
    assert!(recover(&captured, RecoveryMode::Normal, || Ok(())).is_err());
    assert!(batch.join("0.mp4").exists());
    assert_eq!(
        recover(&current, RecoveryMode::Normal, || Ok(()))
            .unwrap()
            .recovered,
        1
    );
}
