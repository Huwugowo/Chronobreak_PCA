// Included in the coordinator test module to exercise ownership without production hooks.

fn clip_fixture(root: &std::path::Path) {
    fs::create_dir_all(root.join("clips")).unwrap();
    for id in ["100_200", "100_201"] {
        fs::write(root.join(format!("clips/{id}.mp4")), b"clip").unwrap();
    }
}

fn unavailable() -> ObservedDuration {
    ObservedDuration {
        facts: None,
        state: DurationState::Unavailable,
    }
}

#[tokio::test]
async fn duration_admission_is_bounded_selected_and_cache_is_disposable() {
    let directory = tempdir().unwrap();
    clip_fixture(directory.path());
    let roots = MediaRoots::new(directory.path().into()).unwrap();
    let coordinator = LibraryCoordinator::new();
    let first = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    for ids in [
        vec![],
        vec!["100_200".into(); 2],
        (0..9).map(|n| format!("100_{n}")).collect(),
    ] {
        assert_eq!(
            coordinator
                .resolve_durations(&roots, ORIGIN, &first.token, ids, None, false)
                .await
                .unwrap_err(),
            LibraryRefreshError::InvalidBatch
        );
    }
    for id in ["../100_200", "100_999"] {
        assert_eq!(
            coordinator
                .resolve_durations(&roots, ORIGIN, &first.token, vec![id.into()], None, false)
                .await
                .unwrap_err(),
            LibraryRefreshError::InvalidSelection
        );
    }
    let result = coordinator
        .resolve_durations(
            &roots,
            ORIGIN,
            &first.token,
            vec!["100_200".into()],
            None,
            false,
        )
        .await
        .unwrap();
    assert_eq!(result.clips[0].duration, DurationState::Unavailable);
    assert_eq!(coordinator.publication.lock().unwrap().durations.len(), 1);
    for retry in [false, true] {
        coordinator
            .resolve_durations_with(
                &roots,
                &first.token,
                vec!["100_200".into()],
                retry,
                move |_, _, _, cached| {
                    assert_eq!(cached.is_some(), !retry);
                    unavailable()
                },
            )
            .await
            .unwrap();
    }
    let next = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    assert!(coordinator.publication.lock().unwrap().durations.is_empty());
    assert_ne!(first.token, next.token);
    assert_eq!(
        coordinator
            .resolve_durations(
                &roots,
                ORIGIN,
                &first.token,
                vec!["100_200".into()],
                None,
                false
            )
            .await
            .unwrap_err(),
        LibraryRefreshError::Superseded
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_duration_waiter_retains_global_slot_and_skips_remaining_ids() {
    let directory = tempdir().unwrap();
    clip_fixture(directory.path());
    let roots = Arc::new(MediaRoots::new(directory.path().into()).unwrap());
    let coordinator = Arc::new(LibraryCoordinator::new());
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let (started, ready) = oneshot::channel();
    let started = Mutex::new(Some(started));
    let (release, blocked) = std::sync::mpsc::channel();
    let token = snapshot.token.clone();
    let task = {
        let coordinator = Arc::clone(&coordinator);
        let roots = Arc::clone(&roots);
        tokio::spawn(async move {
            coordinator
                .resolve_durations_with(
                    &roots,
                    &token,
                    vec!["100_200".into(), "100_201".into()],
                    false,
                    move |_, _, _, _| {
                        started
                            .lock()
                            .unwrap()
                            .take()
                            .expect("remaining IDs must be skipped")
                            .send(())
                            .unwrap();
                        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                        unavailable()
                    },
                )
                .await
        })
    };
    signalled(ready).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        coordinator
            .resolve_durations(
                &roots,
                ORIGIN,
                &snapshot.token,
                vec!["100_200".into()],
                None,
                false
            )
            .await
            .unwrap_err(),
        LibraryRefreshError::Busy
    );
    // The core can progress independently while the cancelled probe still owns its slot.
    coordinator.refresh(&roots, ORIGIN).await.unwrap();
    assert_eq!(coordinator.duration_slot.available_permits(), 0);
    release.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.duration_slot).acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    assert!(coordinator.publication.lock().unwrap().durations.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn probe_invalidation_root_aba_and_shutdown_reject_results() {
    for mode in ["refresh", "root_aba", "shutdown"] {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        clip_fixture(a.path());
        let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
        let coordinator = Arc::new(LibraryCoordinator::new());
        let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        let (started, ready) = oneshot::channel();
        let started = Mutex::new(Some(started));
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let coordinator = Arc::clone(&coordinator);
            let roots = Arc::clone(&roots);
            tokio::spawn(async move {
                coordinator
                    .resolve_durations_with(
                        &roots,
                        &snapshot.token,
                        vec!["100_200".into(), "100_201".into()],
                        false,
                        move |_, _, _, _| {
                            started.lock().unwrap().take().unwrap().send(()).unwrap();
                            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                            unavailable()
                        },
                    )
                    .await
            })
        };
        signalled(ready).await;
        match mode {
            "refresh" => {
                coordinator.refresh(&roots, ORIGIN).await.unwrap();
            }
            "root_aba" => {
                coordinator.publish_output_directory(
                    &roots,
                    OutputDirectory::new(b.path().into()).unwrap(),
                );
                coordinator.publish_output_directory(
                    &roots,
                    OutputDirectory::new(a.path().into()).unwrap(),
                );
            }
            _ => coordinator.shutdown(),
        }
        release.send(()).unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            LibraryRefreshError::Superseded
        );
        assert!(coordinator.publication.lock().unwrap().durations.is_empty());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn mutation_cancellation_retains_slot_and_invalidates_after_partial_failure() {
    let directory = tempdir().unwrap();
    clip_fixture(directory.path());
    let roots = Arc::new(MediaRoots::new(directory.path().into()).unwrap());
    let coordinator = Arc::new(LibraryCoordinator::new());
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let token = snapshot.token.clone();
    let runtime_thread = std::thread::current().id();
    let task = {
        let coordinator = Arc::clone(&coordinator);
        let roots = Arc::clone(&roots);
        tokio::spawn(async move {
            coordinator
                .mutate(&roots, &token, Selection::Clip("100_200"), move |path| {
                    started.send(std::thread::current().id()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5))?;
                    fs::remove_file(path.join("clips/100_200.mp4"))?;
                    anyhow::bail!("injected partial failure")
                })
                .await as Result<(), String>
        })
    };
    assert_ne!(signalled(ready).await, runtime_thread);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(matches!(
        coordinator.mutation_permit(),
        Err(LibraryRefreshError::Busy)
    ));
    assert!(
        coordinator
            .settings(|| Ok(()))
            .await
            .unwrap_err()
            .contains("busy")
    );
    // A scan during the owned mutation cannot remain admissible after completion.
    coordinator.refresh(&roots, ORIGIN).await.unwrap();
    release.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.mutation_slot).acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    assert!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .is_none()
    );
    assert!(!directory.path().join("clips/100_200.mp4").exists());
}

#[tokio::test]
async fn actual_save_delete_clip_game_and_retention_invalidate_success_and_partial_error() {
    let directory = tempdir().unwrap();
    let roots = Arc::new(MediaRoots::new(directory.path().into()).unwrap());
    let coordinator = LibraryCoordinator::new();
    library::tests::write_game(directory.path(), "100", "2020-01-01T00:00:00Z", false);
    clip_fixture(directory.path());
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    coordinator
        .mutate(&roots, &snapshot.token, Selection::Game("100"), |path| {
            library::save_game(path, "100", true)
        })
        .await
        .unwrap();
    assert!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .is_none()
    );
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    assert!(snapshot.games[0].saved);
    assert!(
        coordinator
            .mutate(&roots, &snapshot.token, Selection::Game("100"), |path| {
                library::delete_game(path, "100")
            })
            .await
            .unwrap_err()
            .contains("saved")
    );
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    coordinator
        .mutate(&roots, &snapshot.token, Selection::Game("100"), |path| {
            library::save_game(path, "100", false)
        })
        .await
        .unwrap();
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    coordinator
        .mutate(&roots, &snapshot.token, Selection::Game("100"), |path| {
            library::delete_game(path, "100")
        })
        .await
        .unwrap();
    assert!(!directory.path().join("games/100").exists());
    library::tests::write_game(directory.path(), "101", "2020-01-01T00:00:00Z", false);
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let result = coordinator
        .mutate(&roots, &snapshot.token, Selection::Library, |path| {
            library::run_auto_delete(path, 1)
        })
        .await
        .unwrap();
    assert_eq!(result.deleted_count, 1);
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    coordinator
        .mutate(
            &roots,
            &snapshot.token,
            Selection::Clip("100_201"),
            |path| library::delete_clip(path, "100_201"),
        )
        .await
        .unwrap();
    fs::create_dir(directory.path().join("clips/100_200.jpg")).unwrap();
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let error = coordinator
        .mutate(
            &roots,
            &snapshot.token,
            Selection::Clip("100_200"),
            |path| library::delete_clip(path, "100_200"),
        )
        .await
        .unwrap_err();
    assert!(error.contains("thumbnail"));
    assert!(!directory.path().join("clips/100_200.mp4").exists());
    assert!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .is_none()
    );
    assert!(
        coordinator
            .refresh(&roots, ORIGIN)
            .await
            .unwrap()
            .clips
            .is_empty()
    );
}

#[tokio::test]
async fn selected_tokens_reject_overlapping_roots_and_export_completion_uses_original_epoch() {
    let a = tempdir().unwrap();
    let b = tempdir().unwrap();
    for root in [a.path(), b.path()] {
        fs::create_dir_all(root.join("games/100")).unwrap();
        clip_fixture(root);
    }
    let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
    let coordinator = LibraryCoordinator::new();
    let original = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let export = coordinator
        .admit_export(&roots, &original.token, "100")
        .unwrap();
    assert!(matches!(
        coordinator.mutation_permit(),
        Err(LibraryRefreshError::Busy)
    ));
    coordinator.publish_output_directory(&roots, OutputDirectory::new(b.path().into()).unwrap());
    let current = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    assert!(
        coordinator
            .mutate(
                &roots,
                &original.token,
                Selection::Game("100"),
                |_| -> anyhow::Result<()> { panic!("old selection entered worker") }
            )
            .await
            .unwrap_err()
            .contains("superseded")
    );
    assert!(matches!(
        coordinator.admit_export(&roots, &current.token, "999"),
        Err(LibraryRefreshError::InvalidSelection)
    ));
    // Simulated publication uses the already admitted destination, even after root change.
    fs::write(export.capture.path.join("clips/100_300.mp4"), b"published").unwrap();
    assert!(!export.selection_is_current());
    drop(export);
    assert_eq!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .as_deref(),
        Some(current.token.as_str())
    );
    assert!(!b.path().join("clips/100_300.mp4").exists());
    // Same-root refresh during export still requires invalidation at completion.
    let export = coordinator
        .admit_export(&roots, &current.token, "100")
        .unwrap();
    coordinator.refresh(&roots, ORIGIN).await.unwrap();
    drop(export);
    assert!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .is_none()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn settings_worker_survives_cancel_and_serializes_root_capture() {
    let a = tempdir().unwrap();
    let b = tempdir().unwrap();
    let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
    let coordinator = Arc::new(LibraryCoordinator::new());
    let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
    let target = OutputDirectory::new(b.path().into()).unwrap();
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = {
        let coordinator = Arc::clone(&coordinator);
        let roots = Arc::clone(&roots);
        tokio::spawn(async move {
            let worker = Arc::clone(&coordinator);
            coordinator
                .settings(move || {
                    started.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                    worker.publish_output_directory(&roots, target);
                    Ok(())
                })
                .await
        })
    };
    signalled(ready).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(
        coordinator
            .mutate(&roots, &snapshot.token, Selection::Library, |_| Ok(()))
            .await
            .unwrap_err()
            .contains("busy")
    );
    release.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.mutation_slot).acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    assert_eq!(roots.output_directory(), b.path());
    assert!(
        coordinator
            .publication
            .lock()
            .unwrap()
            .selected_token
            .is_none()
    );
}

#[tokio::test]
async fn late_export_invalidates_returned_root_but_never_admits_old_response() {
    for switch_away in [false, true] {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        fs::create_dir_all(a.path().join("games/100")).unwrap();
        let roots = Arc::new(MediaRoots::new(a.path().into()).unwrap());
        let coordinator = LibraryCoordinator::new();
        let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
        let export = coordinator
            .admit_export(&roots, &snapshot.token, "100")
            .unwrap();
        if switch_away {
            coordinator
                .publish_output_directory(&roots, OutputDirectory::new(b.path().into()).unwrap());
        }
        coordinator
            .publish_output_directory(&roots, OutputDirectory::new(a.path().into()).unwrap());
        coordinator.refresh(&roots, ORIGIN).await.unwrap();
        assert!(!export.selection_is_current());
        drop(export);
        assert!(
            coordinator
                .publication
                .lock()
                .unwrap()
                .selected_token
                .is_none(),
            "export files change the current matching root even after A/B/A"
        );
    }
}
