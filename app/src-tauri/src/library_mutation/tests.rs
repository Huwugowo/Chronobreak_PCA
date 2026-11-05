use super::*;
use std::ffi::OsStr;
use std::fs;

fn approve(root: &Path) -> ApprovedRoot {
    ApprovedRoot::new(root).unwrap()
}

pub(super) fn junction(link: &Path, target: &Path) {
    let output = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "temporary junction creation failed"
    );
}

#[test]
fn distinct_handles_contend_within_one_process_and_release_on_drop() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let approved = approve(a.path());
    let owner = LibraryMutation::acquire(a.path(), &approved).unwrap();
    let error = LibraryMutation::acquire(a.path(), &approved).err().unwrap();
    assert!(error.to_string().contains("busy"));
    let independent = LibraryMutation::acquire(b.path(), &approve(b.path())).unwrap();
    assert!(fs::rename(a.path(), a.path().with_extension("replaced")).is_err());
    drop(independent);
    drop(owner);
    LibraryMutation::acquire(a.path(), &approved).unwrap();
}

#[test]
fn guarded_chain_and_exact_removal_handle_allow_intended_final_deletion() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("games/100/nested")).unwrap();
    fs::write(root.path().join("games/100/nested/data"), b"fixture").unwrap();
    fs::create_dir(root.path().join("clips")).unwrap();
    fs::write(root.path().join("clips/100_200.mp4"), b"clip").unwrap();
    let owner = LibraryMutation::acquire(root.path(), &approve(root.path())).unwrap();
    let games = owner.games().unwrap();
    let target = games.removal_target(OsStr::new("100")).unwrap();
    assert!(fs::rename(root.path().join("games"), root.path().join("old-games")).is_err());
    assert!(fs::rename(target.path(), root.path().join("games/old-100")).is_err());
    assert!(target.is_directory().unwrap());
    target.remove_tree().unwrap();
    assert!(!root.path().join("games/100").exists());
    let clips = owner.clips().unwrap();
    let clip = clips.removal_target(OsStr::new("100_200.mp4")).unwrap();
    assert!(fs::rename(&clips.path, root.path().join("old-clips")).is_err());
    assert!(fs::rename(clip.path(), clips.path.join("replaced.mp4")).is_err());
    clip.remove_file().unwrap();
    assert!(!clips.path.join("100_200.mp4").exists());
}

#[test]
fn aliases_share_ownership_and_retargeting_cannot_redirect_guarded_work() {
    let fixture = tempfile::tempdir().unwrap();
    let a = fixture.path().join("a");
    let b = fixture.path().join("b");
    let alias = fixture.path().join("alias");
    for root in [&a, &b] {
        fs::create_dir_all(root.join("games")).unwrap();
    }
    junction(&alias, &a);
    let approved = approve(&alias);
    let owner = LibraryMutation::acquire(&alias, &approved).unwrap();
    assert!(
        LibraryMutation::acquire(&a, &approve(&a))
            .err()
            .unwrap()
            .to_string()
            .contains("busy")
    );
    fs::remove_dir(&alias).unwrap();
    junction(&alias, &b);
    assert_eq!(owner.games().unwrap().path, approved.path().join("games"));
    assert!(
        LibraryMutation::acquire(&alias, &approved)
            .err()
            .unwrap()
            .to_string()
            .contains("alias")
    );
    assert!(!b.join(".chronobreak-mutation.lease").exists());
}

#[test]
fn replaced_root_and_reparse_descendants_fail_before_media_effects() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("library");
    let old = fixture.path().join("old-library");
    let outside = fixture.path().join("outside");
    fs::create_dir_all(root.join("clips")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(root.join("clips/100_200.mp4"), b"original").unwrap();
    let approved = approve(&root);
    fs::rename(&root, &old).unwrap();
    fs::create_dir_all(root.join("clips")).unwrap();
    fs::write(root.join("clips/100_200.mp4"), b"replacement").unwrap();
    assert!(LibraryMutation::acquire(&root, &approved).is_err());
    assert_eq!(
        fs::read(old.join("clips/100_200.mp4")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(root.join("clips/100_200.mp4")).unwrap(),
        b"replacement"
    );
    assert!(!root.join(".chronobreak-mutation.lease").exists());
    junction(&root.join("games"), &outside);
    let owner = LibraryMutation::acquire(&root, &approve(&root)).unwrap();
    assert!(owner.games().is_err());
}

#[test]
fn retention_derives_current_eligibility_and_preserves_saved_incomplete_and_recent_games() {
    use crate::library;
    let root = tempfile::tempdir().unwrap();
    for (id, date, saved) in [
        ("100", "2020-01-01T00:00:00Z", false),
        ("101", "2020-01-01T00:00:00Z", false),
        ("102", "2099-01-01T00:00:00Z", false),
        ("103", "2020-01-01T00:00:00Z", false),
        ("104", "invalid", false),
        ("105", "2020-01-01T00:00:00Z", false),
    ] {
        library::tests::write_game(root.path(), id, date, saved);
    }
    let stale = library::list_games(root.path()).unwrap();
    assert!(
        !stale
            .iter()
            .find(|game| game.timestamp == "101")
            .unwrap()
            .saved
    );
    let approved = approve(root.path());
    {
        let owner = LibraryMutation::acquire(root.path(), &approved).unwrap();
        library::save_game(&owner, "101", true).unwrap();
    }
    fs::write(root.path().join("games/103/video.mp4"), b"incomplete").unwrap();
    fs::write(root.path().join("games/105/metadata.json"), b"invalid").unwrap();
    let owner = LibraryMutation::acquire(root.path(), &approved).unwrap();
    assert_eq!(
        library::run_auto_delete(&owner, 1).unwrap().deleted_count,
        1
    );
    assert!(!root.path().join("games/100").exists());
    for id in ["101", "102", "103", "104", "105"] {
        assert!(root.path().join("games").join(id).is_dir(), "lost {id}");
    }
}

#[test]
fn rejected_tree_preflight_preserves_all_recording_and_external_content() {
    use crate::library;
    use std::os::windows::fs::OpenOptionsExt;
    for reason in ["junction", "locked", "depth", "entries"] {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("library");
        let game = library::tests::write_game(&root, "100", "2020-01-01T00:00:00Z", false);
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("foreign"), b"outside").unwrap();
        fs::write(game.join("a-sibling"), b"preserve").unwrap();
        let locked = match reason {
            "junction" => {
                junction(&game.join("z-junction"), &outside);
                None
            }
            "locked" => {
                fs::write(game.join("z-locked"), b"locked").unwrap();
                Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(0)
                        .open(game.join("z-locked"))
                        .unwrap(),
                )
            }
            "depth" => {
                let mut nested = game.clone();
                for _ in 0..64 {
                    nested.push("d");
                    fs::create_dir(&nested).unwrap();
                }
                None
            }
            _ => {
                for n in 0..4096 {
                    fs::write(game.join(format!("entry-{n}")), b"fixture").unwrap();
                }
                None
            }
        };
        let metadata = fs::read(game.join("metadata.json")).unwrap();
        let video = fs::read(game.join("video.mp4")).unwrap();
        let before = fs::read_dir(&game).unwrap().count();
        let approved = approve(&root);
        let owner = LibraryMutation::acquire(&root, &approved).unwrap();
        for retention in [false, true] {
            let result = if retention {
                library::run_auto_delete(&owner, 1).map(|_| ())
            } else {
                library::delete_game(&owner, "100")
            };
            assert!(result.is_err(), "accepted {reason}");
            assert_eq!(fs::read(game.join("metadata.json")).unwrap(), metadata);
            assert_eq!(fs::read(game.join("video.mp4")).unwrap(), video);
            assert_eq!(fs::read(game.join("a-sibling")).unwrap(), b"preserve");
            assert_eq!(fs::read(outside.join("foreign")).unwrap(), b"outside");
            assert_eq!(fs::read_dir(&game).unwrap().count(), before);
        }
        drop(locked);
        // Failure releases every target handle, even though the library lease stays held.
        fs::rename(&game, root.join("games/101")).unwrap();
        fs::rename(root.join("games/101"), &game).unwrap();
    }
}
