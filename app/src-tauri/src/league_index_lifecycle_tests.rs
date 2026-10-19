// Included in the coordinator test module to exercise index-job ownership.

async fn index_fixture(root: &std::path::Path) -> (Arc<MediaRoots>, LibraryCoordinator, String) {
    library::tests::write_game(root, "1786000000", "2026-08-05T10:00:00Z", false);
    let roots = Arc::new(MediaRoots::new(root.into()).unwrap());
    let coordinator = LibraryCoordinator::new();
    let token = coordinator.refresh(&roots, ORIGIN).await.unwrap().token;
    (roots, coordinator, token)
}

#[tokio::test(flavor = "current_thread")]
async fn index_jobs_reject_root_aba_after_blocking_read_and_write() {
    for write in [false, true] {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        let (roots, coordinator, token) = index_fixture(a.path()).await;
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = {
            let (roots, coordinator, token) =
                (Arc::clone(&roots), coordinator.clone(), token.clone());
            tokio::spawn(async move {
                coordinator
                    .index_job(roots, &token, None, write, move |_, _, _| {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(5))?;
                        Ok(())
                    })
                    .await
            })
        };
        signalled(ready).await;
        coordinator
            .publish_output_directory(&roots, OutputDirectory::new(b.path().into()).unwrap());
        coordinator
            .publish_output_directory(&roots, OutputDirectory::new(a.path().into()).unwrap());
        coordinator.refresh(&roots, ORIGIN).await.unwrap();
        release.send(()).unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            LibraryRefreshError::Superseded,
            "write={write}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn aborted_index_writer_keeps_permit_until_blocking_work_exits() {
    let root = tempdir().unwrap();
    let (roots, coordinator, token) = index_fixture(root.path()).await;
    let coordinator = Arc::new(coordinator);
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_job(roots, &token, None, true, move |_, _, _| {
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
                .index_job(Arc::clone(&roots), &token, None, true, |_, _, _| Ok(()))
                .await
                .unwrap_err(),
            LibraryRefreshError::Busy
        );
    }
    assert_eq!(coordinator.index_writer_slot.available_permits(), 0);
    release.send(()).unwrap();
    let permit = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.index_writer_slot).acquire_owned(),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permit);
}

#[tokio::test(flavor = "current_thread")]
async fn aborted_index_admin_keeps_writer_and_all_reader_permits_until_worker_exits() {
    let root = tempdir().unwrap();
    let (roots, coordinator, token) = index_fixture(root.path()).await;
    let coordinator = Arc::new(coordinator);
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_admin_job(roots, &token, move |_, _, _| {
                    started.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    signalled(ready).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        coordinator
            .index_job(Arc::clone(&roots), &token, None, true, |_, _, _| Ok(()))
            .await
            .unwrap_err(),
        LibraryRefreshError::Busy
    );
    assert_eq!(
        coordinator
            .index_job(Arc::clone(&roots), &token, None, false, |_, _, _| Ok(()))
            .await
            .unwrap_err(),
        LibraryRefreshError::Busy
    );
    assert_eq!(coordinator.index_writer_slot.available_permits(), 0);
    assert_eq!(coordinator.index_reader_slots.available_permits(), 0);
    release.send(()).unwrap();
    let readers = tokio::time::timeout(
        Duration::from_secs(5),
        Arc::clone(&coordinator.index_reader_slots).acquire_many_owned(2),
    )
    .await
    .unwrap()
    .unwrap();
    drop(readers);
    assert_eq!(coordinator.index_writer_slot.available_permits(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn index_reader_writer_and_descriptor_slots_have_independent_capacity() {
    let root = tempdir().unwrap();
    let (roots, coordinator, token) = index_fixture(root.path()).await;
    let coordinator = Arc::new(coordinator);

    let (writer_started, writer_ready) = oneshot::channel();
    let (writer_release, writer_blocked) = std::sync::mpsc::channel();
    let writer = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_job(roots, &token, None, true, move |_, _, _| {
                    writer_started.send(()).unwrap();
                    writer_blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    signalled(writer_ready).await;

    let (first_started, first_ready) = oneshot::channel();
    let (first_release, first_blocked) = std::sync::mpsc::channel();
    let first_reader = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_job(roots, &token, None, false, move |_, _, _| {
                    first_started.send(()).unwrap();
                    first_blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    signalled(first_ready).await;

    let (second_started, second_ready) = oneshot::channel();
    let (second_release, second_blocked) = std::sync::mpsc::channel();
    let second_reader = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_job(roots, &token, None, false, move |_, _, _| {
                    second_started.send(()).unwrap();
                    second_blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    signalled(second_ready).await;

    assert_eq!(
        coordinator
            .index_job(Arc::clone(&roots), &token, None, false, |_, _, _| Ok(()))
            .await
            .unwrap_err(),
        LibraryRefreshError::Busy
    );
    assert_eq!(coordinator.index_reader_slots.available_permits(), 0);
    assert_eq!(coordinator.index_writer_slot.available_permits(), 0);
    assert_eq!(
        coordinator
            .read_replay(&roots, &token, "1786000000", |_| Ok("descriptor"))
            .await
            .unwrap(),
        "descriptor"
    );

    writer_release.send(()).unwrap();
    first_release.send(()).unwrap();
    second_release.send(()).unwrap();
    assert!(writer.await.unwrap().is_ok());
    assert!(first_reader.await.unwrap().is_ok());
    assert!(second_reader.await.unwrap().is_ok());
    assert_eq!(coordinator.index_writer_slot.available_permits(), 1);
    assert_eq!(coordinator.index_reader_slots.available_permits(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_denies_new_index_work_and_rejects_owned_completion() {
    let root = tempdir().unwrap();
    let (roots, coordinator, token) = index_fixture(root.path()).await;
    let coordinator = Arc::new(coordinator);
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = {
        let (roots, coordinator, token) =
            (Arc::clone(&roots), Arc::clone(&coordinator), token.clone());
        tokio::spawn(async move {
            coordinator
                .index_job(roots, &token, None, true, move |_, _, _| {
                    started.send(()).unwrap();
                    blocked.recv_timeout(Duration::from_secs(5))?;
                    Ok(())
                })
                .await
        })
    };
    signalled(ready).await;
    coordinator.shutdown();
    for write in [false, true] {
        assert_eq!(
            coordinator
                .index_job(Arc::clone(&roots), &token, None, write, |_, _, _| Ok(()))
                .await
                .unwrap_err(),
            LibraryRefreshError::Superseded,
            "write={write}"
        );
    }
    release.send(()).unwrap();
    assert_eq!(
        task.await.unwrap().unwrap_err(),
        LibraryRefreshError::Superseded
    );
}
