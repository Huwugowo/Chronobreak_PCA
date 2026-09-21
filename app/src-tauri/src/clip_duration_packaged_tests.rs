#[tokio::test]
#[ignore = "requires staged runtime via QUEUEBACK_TEST_MEDIA_RUNTIME"]
async fn packaged_probe_uses_protected_route_without_following_external_mov_reference() {
    use crate::library_coordinator::LibraryCoordinator;
    use crate::playback_policy::BoundServer;
    use crate::playback_server::{self, MediaRoots, PlaybackMetrics};
    use std::sync::Arc;
    let runtime =
        std::env::var_os("QUEUEBACK_TEST_MEDIA_RUNTIME").expect("set staged runtime root");
    let tools = queueback_media_runtime::resolve_root(Path::new(&runtime))
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("clips")).unwrap();
    let media = include_bytes!("../resources/hevc-probe.mp4");
    fs::write(directory.path().join("clips/100_200.mp4"), media).unwrap();
    let roots = Arc::new(MediaRoots::new(directory.path().into()).unwrap());
    let origin = playback_server::start(
        BoundServer::bind().unwrap(),
        Arc::clone(&roots),
        Arc::new(PlaybackMetrics::default()),
        directory.path().join("ddragon"),
        #[cfg(feature = "replay-benchmark")]
        None,
        false,
    )
    .await
    .unwrap();
    let coordinator = LibraryCoordinator::new();
    let snapshot = coordinator.refresh(&roots, &origin).await.unwrap();
    let result = coordinator
        .resolve_durations(
            &roots,
            &origin,
            &snapshot.token,
            vec!["100_200".into()],
            Some(tools.ffprobe().into()),
            false,
        )
        .await
        .unwrap();
    assert!(
        matches!(result.clips[0].duration, DurationState::Available { duration_ms } if duration_ms > 0)
    );
    assert_eq!(
        fs::read(directory.path().join("clips/100_200.mp4")).unwrap(),
        media
    );

    // The external HTTP sentinel must receive no connection, even though HTTP is
    // whitelisted for the app-owned input. Replace only a dedicated copied fixture.
    let sentinel = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    sentinel.set_nonblocking(true).unwrap();
    let external = format!("http://{}/outside", sentinel.local_addr().unwrap());
    let mut count = 0;
    let external_media = replace_reference(media, &external, &mut count);
    assert!(count > 0);
    fs::write(directory.path().join("clips/100_201.mp4"), &external_media).unwrap();
    let snapshot = coordinator.refresh(&roots, &origin).await.unwrap();
    // Duration can remain readable from headers; the guarantee is no external input.
    coordinator
        .resolve_durations(
            &roots,
            &origin,
            &snapshot.token,
            vec!["100_201".into()],
            Some(tools.ffprobe().into()),
            false,
        )
        .await
        .unwrap();
    assert_eq!(
        sentinel.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        fs::read(directory.path().join("clips/100_201.mp4")).unwrap(),
        external_media
    );
}

fn replace_reference(bytes: &[u8], url: &str, count: &mut usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        assert!(size >= 8 && offset + size <= bytes.len());
        let kind = &bytes[offset + 4..offset + 8];
        let payload = &bytes[offset + 8..offset + size];
        let replaced = match kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"dinf" => {
                replace_reference(payload, url, count)
            }
            b"dref" => {
                let mut data = payload[..8].to_vec();
                data.extend(replace_reference(&payload[8..], url, count));
                data
            }
            b"url " => {
                *count += 1;
                let mut data = vec![0; 4];
                data.extend(url.bytes());
                data.push(0);
                data
            }
            _ => payload.to_vec(),
        };
        result.extend(u32::try_from(8 + replaced.len()).unwrap().to_be_bytes());
        result.extend(kind);
        result.extend(replaced);
        offset += size;
    }
    result
}
