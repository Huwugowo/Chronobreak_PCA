async fn replay_fixture(root: &std::path::Path) -> (Arc<MediaRoots>, LibraryCoordinator, String) {
    library::tests::write_game(root, "1786000000", "2026-08-05T10:00:00Z", false);
    let roots = Arc::new(MediaRoots::new(root.into()).unwrap());
    let coordinator = LibraryCoordinator::new();
    let token = coordinator.refresh(&roots, ORIGIN).await.unwrap().token;
    (roots, coordinator, token)
}

#[tokio::test(flavor = "current_thread")]
async fn replay_read_bounds_owned_work_after_caller_drop_and_recovers_errors() {
    let root = tempdir().unwrap();
    let (roots, coordinator, token) = replay_fixture(root.path()).await;
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = {
        let (roots, coordinator, token) = (Arc::clone(&roots), coordinator.clone(), token.clone());
        tokio::spawn(async move {
            coordinator
                .read_replay(&roots, &token, "1786000000", move |_| {
                    started.send(std::thread::current().id()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    assert_ne!(signalled(ready).await, std::thread::current().id());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    for _ in 0..16 {
        assert_eq!(
            coordinator
                .read_replay(&roots, &token, "1786000000", |_| Ok(()))
                .await
                .unwrap_err(),
            LibraryRefreshError::Busy
        );
    }
    assert_eq!(coordinator.replay_slot.available_permits(), 0);
    release.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.replay_slot).acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
    let panic = coordinator
        .read_replay::<()>(&roots, &token, "1786000000", |_| {
            panic!("fixture worker failure")
        })
        .await;
    assert!(matches!(panic, Err(LibraryRefreshError::Failed { .. })));
    let failure = coordinator
        .read_replay::<()>(&roots, &token, "1786000000", |_| {
            anyhow::bail!("fixture read failure")
        })
        .await;
    assert!(matches!(failure, Err(LibraryRefreshError::Failed { .. })));
    assert_eq!(
        coordinator
            .read_replay(&roots, &token, "1786000000", |_| Ok(7))
            .await
            .unwrap(),
        7
    );
    assert_eq!(
        coordinator
            .read_replay(&roots, &token, "999", |_| Ok(()))
            .await
            .unwrap_err(),
        LibraryRefreshError::InvalidSelection
    );
}

#[tokio::test(flavor = "current_thread")]
async fn replay_read_rejects_refresh_mutation_root_aba_and_shutdown_completions() {
    for invalidation in ["refresh", "mutation", "aba", "shutdown"] {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let (roots, coordinator, token) = replay_fixture(a.path()).await;
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let (roots, coordinator, token) =
                (Arc::clone(&roots), coordinator.clone(), token.clone());
            tokio::spawn(async move {
                coordinator
                    .read_replay(&roots, &token, "1786000000", move |_| {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5))?;
                        Ok(())
                    })
                    .await
            })
        };
        signalled(ready).await;
        match invalidation {
            "refresh" => {
                coordinator.refresh(&roots, ORIGIN).await.unwrap();
            }
            "mutation" => {
                coordinator
                    .mutate(&roots, &token, Selection::Game("1786000000"), |_| Ok(()))
                    .await
                    .unwrap();
            }
            "aba" => {
                coordinator.publish_output_directory(
                    &roots,
                    OutputDirectory::new(b.path().into()).unwrap(),
                );
                coordinator.publish_output_directory(
                    &roots,
                    OutputDirectory::new(a.path().into()).unwrap(),
                );
                coordinator.refresh(&roots, ORIGIN).await.unwrap();
            }
            "shutdown" => coordinator.shutdown(),
            _ => unreachable!(),
        }
        release.send(()).unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            LibraryRefreshError::Superseded,
            "{invalidation}"
        );
        assert_eq!(
            coordinator
                .read_replay(&roots, &token, "1786000000", |_| Ok(()))
                .await
                .unwrap_err(),
            LibraryRefreshError::Superseded
        );
    }
}
