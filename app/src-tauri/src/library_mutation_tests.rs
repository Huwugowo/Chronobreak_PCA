// Included in the coordinator tests: exercise real admission without production hooks.

#[test]
fn root_replacement_before_or_after_admission_cannot_authorize_any_mutation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all().max_blocking_threads(1).build().unwrap();
    runtime.block_on(async {
        for queued in [false, true] {
            for operation in ["save", "game", "clip", "retention"] {
                let fixture = tempdir().unwrap();
                let root = fixture.path().join("library");
                let previous = fixture.path().join("previous");
                library::tests::write_game(&root, "100", "2020-01-01T00:00:00Z", false);
                clip_fixture(&root);
                let roots = Arc::new(MediaRoots::new(root.clone()).unwrap());
                let coordinator = Arc::new(LibraryCoordinator::new());
                let snapshot = coordinator.refresh(&roots, ORIGIN).await.unwrap();
                let (release, blocked) = std::sync::mpsc::channel();
                let blocker = if queued {
                    let (started, ready) = oneshot::channel();
                    let blocker = tokio::task::spawn_blocking(move || {
                        started.send(()).unwrap();
                        blocked.recv_timeout(Duration::from_secs(10)).unwrap();
                    });
                    signalled(ready).await;
                    Some(blocker)
                } else { None };
                let dispatch = |coordinator: Arc<LibraryCoordinator>, roots: Arc<MediaRoots>, token: String| {
                    tokio::spawn(async move {
                        let selection = match operation {
                            "clip" => Selection::Clip("100_200"),
                            "retention" => Selection::Library,
                            _ => Selection::Game("100"),
                        };
                        coordinator.mutate(&roots, &token, selection, move |owner| {
                            match operation {
                                "save" => library::save_game(owner, "100", true),
                                "game" => library::delete_game(owner, "100"),
                                "clip" => library::delete_clip(owner, "100_200"),
                                _ => library::run_auto_delete(owner, 1).map(|_| ()),
                            }
                        }).await
                    })
                };
                let mut task = if queued {
                    let task = dispatch(Arc::clone(&coordinator), Arc::clone(&roots), snapshot.token.clone());
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while coordinator.mutation_slot.available_permits() != 0 {
                            tokio::task::yield_now().await;
                        }
                    }).await.unwrap();
                    Some(task)
                } else { None };
                fs::rename(&root, &previous).unwrap();
                library::tests::write_game(&root, "100", "2020-01-01T00:00:00Z", false);
                clip_fixture(&root);
                fs::write(root.join("clips/100_200.mp4"), b"replacement").unwrap();
                let original_metadata = fs::read(previous.join("games/100/metadata.json")).unwrap();
                let replacement_metadata = fs::read(root.join("games/100/metadata.json")).unwrap();
                if !queued {
                    task = Some(dispatch(Arc::clone(&coordinator), Arc::clone(&roots), snapshot.token));
                } else { release.send(()).unwrap(); }
                let error = task.unwrap().await.unwrap().unwrap_err();
                assert!(error.contains("replaced"), "{operation}: {error}");
                if let Some(blocker) = blocker { blocker.await.unwrap(); }
                assert_eq!(fs::read(previous.join("games/100/metadata.json")).unwrap(), original_metadata);
                assert_eq!(fs::read(root.join("games/100/metadata.json")).unwrap(), replacement_metadata);
                assert_eq!(fs::read(previous.join("clips/100_200.mp4")).unwrap(), b"clip");
                assert_eq!(fs::read(root.join("clips/100_200.mp4")).unwrap(), b"replacement");
                assert!(coordinator.publication.lock().unwrap().selected_token.is_none());
            }
        }
    });
}
