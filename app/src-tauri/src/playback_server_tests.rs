use super::*;
use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, HOST, ORIGIN};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

struct WireFixture {
    _directory: tempfile::TempDir,
    roots: Arc<MediaRoots>,
    metrics: Arc<PlaybackMetrics>,
    base: String,
    routes: Vec<(&'static str, String, &'static str)>,
}

impl WireFixture {
    async fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::create_dir_all(root.join("games/1786000000")).unwrap();
        std::fs::create_dir_all(root.join("games/1786000001")).unwrap();
        std::fs::create_dir_all(root.join("clips")).unwrap();
        std::fs::create_dir_all(root.join("ddragon/15.1.1/icons/item")).unwrap();
        for name in [
            "games/1786000000/video.mp4",
            "clips/1786000000_1786000010.mp4",
            "clips/1786000000_1786000010.jpg",
            "input.wav",
        ] {
            std::fs::write(root.join(name), b"0123456789abcdef").unwrap();
        }
        std::fs::write(root.join("games/1786000001/video.mp4"), b"").unwrap();
        std::fs::write(
            root.join("ddragon/15.1.1/icons/item/1001.png"),
            b"\x89PNG\r\n\x1a\nfixture",
        )
        .unwrap();
        let roots = Arc::new(MediaRoots::new(root.to_path_buf()).unwrap());
        let token = roots
            .register_imported_music_preview(root.join("input.wav"))
            .unwrap();
        let metrics = Arc::new(PlaybackMetrics::default());
        let base = start(
            BoundServer::bind().unwrap(),
            Arc::clone(&roots),
            Arc::clone(&metrics),
            root.join("ddragon"),
            #[cfg(feature = "replay-benchmark")]
            None,
            false,
        )
        .await
        .unwrap();
        let routes = vec![
            (
                "game",
                "/games/1786000000/video.mp4".to_owned(),
                "video/mp4",
            ),
            (
                "clip",
                "/clips/1786000000_1786000010.mp4".to_owned(),
                "video/mp4",
            ),
            (
                "thumbnail",
                "/clips/1786000000_1786000010.jpg".to_owned(),
                "image/jpeg",
            ),
            ("music", "/music/momentum.mp3".to_owned(), "audio/mpeg"),
            ("import", format!("/music-preview/{token}"), "audio/wav"),
            ("probe", "/probe/hevc.mp4".to_owned(), "video/mp4"),
            (
                "ddragon",
                "/ddragon/15.1.1/item/1001".to_owned(),
                "image/png",
            ),
        ];
        Self {
            _directory: directory,
            roots,
            metrics,
            base,
            routes,
        }
    }

    fn origin(&self) -> &str {
        self.base.split("/cap/").next().unwrap()
    }
    fn authority(&self) -> &str {
        self.origin().strip_prefix("http://").unwrap()
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(4))
        .build()
        .unwrap()
}

#[tokio::test]
async fn every_route_enforces_the_same_capability_and_http_contract() {
    let fixture = WireFixture::start().await;
    let client = client();
    for (label, route, content_type) in &fixture.routes {
        let url = format!("{}{route}", fixture.base);
        let response = client
            .get(&url)
            .header(ORIGIN, playback_policy::PRODUCTION_ORIGIN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{label}");
        assert_eq!(response.headers()[CONTENT_TYPE], *content_type);
        assert_eq!(
            response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN],
            playback_policy::PRODUCTION_ORIGIN
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let bytes = response.bytes().await.unwrap();
        let head = client.head(&url).send().await.unwrap();
        assert_eq!(head.status(), StatusCode::OK, "{label}");
        // reqwest::content_length reports the body size hint (zero for HEAD),
        // while the wire header must describe the corresponding GET resource.
        assert_eq!(head.headers()[CONTENT_LENGTH], bytes.len().to_string());
        assert!(head.bytes().await.unwrap().is_empty());
        for method in [Method::GET, Method::HEAD] {
            let range = client
                .request(method.clone(), format!("{url}?qb_playback_session=load-1"))
                .header(RANGE, "bytes=1-3")
                .send()
                .await
                .unwrap();
            assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT, "{label}");
            assert_eq!(
                range.headers()[CONTENT_RANGE],
                format!("bytes 1-3/{}", bytes.len())
            );
            let part = range.bytes().await.unwrap();
            if method == Method::GET {
                assert_eq!(&part[..], &bytes[1..4]);
            } else {
                assert!(part.is_empty());
            }
        }
        let response = client
            .get(&url)
            .header(RANGE, "bytes=999999999-")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{label}"
        );
        assert_eq!(
            response.headers()[CONTENT_RANGE],
            format!("bytes */{}", bytes.len())
        );
        for candidate in [
            format!("{}{route}", fixture.origin()),
            format!("{}/cap/wrong{route}", fixture.origin()),
        ] {
            let denied = client.get(candidate).send().await.unwrap();
            assert_eq!(denied.status(), StatusCode::NOT_FOUND, "{label}");
            assert!(denied.bytes().await.unwrap().is_empty());
        }
    }
    for range in ["bytes=0-1,4-5", "items=0-1", "bytes=4-3"] {
        assert_eq!(
            client
                .get(format!("{}/games/1786000000/video.mp4", fixture.base))
                .header(RANGE, range)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
    }
    let empty = client
        .get(format!("{}/games/1786000001/video.mp4", fixture.base))
        .header(RANGE, "bytes=0-")
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(empty.headers()[CONTENT_RANGE], "bytes */0");
    for path in [
        "/games/nope/video.mp4",
        "/clips/1786000000_1786000010.json",
        "/ddragon/15.1.1/html/1001",
        "/music/anything.wav",
        "/future/derived.bin",
        "/games/%2e%2e%5csecret/video.mp4",
    ] {
        assert_eq!(
            client
                .get(format!("{}{path}", fixture.base))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert!(fixture.metrics.snapshot().rejected_requests >= 14);
    assert!(fixture.metrics.snapshot().peak_streams <= MAX_CONNECTIONS as u64);
}

#[tokio::test]
async fn hostile_origins_hosts_bodies_and_old_capabilities_fail_before_delivery() {
    let fixture = WireFixture::start().await;
    let other = WireFixture::start().await;
    let client = client();
    let url = format!("{}/probe/hevc.mp4", fixture.base);
    for origin in [
        "https://attacker.example",
        "null",
        "http://127.0.0.1:1420",
        "http://localhost:9999",
        "http://tauri.localhost.attacker.example",
    ] {
        let response = client
            .get(&url)
            .header(ORIGIN, origin)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));
    }
    let allowed = client
        .get(&url)
        .header(ORIGIN, "http://localhost:1420")
        .send()
        .await
        .unwrap();
    assert_eq!(
        allowed.status(),
        if cfg!(debug_assertions) {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        }
    );
    let response = client
        .get(&url)
        .header(ORIGIN, playback_policy::PRODUCTION_ORIGIN)
        .header(ORIGIN, "null")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for host in ["localhost:1", "attacker.example", "127.0.0.1:1"] {
        assert_eq!(
            client
                .get(&url)
                .header(HOST, host)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        client.get(&url).body("x").send().await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client.post(&url).send().await.unwrap().status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    let preflight = client
        .request(Method::OPTIONS, &url)
        .header(ORIGIN, playback_policy::PRODUCTION_ORIGIN)
        .header("access-control-request-method", "GET")
        .header("access-control-request-headers", "range")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        preflight.headers()["access-control-allow-methods"],
        "GET, HEAD"
    );
    assert_eq!(
        client
            .request(Method::OPTIONS, &url)
            .header(ORIGIN, playback_policy::PRODUCTION_ORIGIN)
            .header("access-control-request-method", "POST")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let old_capability = fixture.base.replace(fixture.origin(), other.origin());
    assert_eq!(
        client
            .get(format!("{old_capability}/probe/hevc.mp4"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let imported = fixture
        .routes
        .iter()
        .find(|(label, _, _)| *label == "import")
        .unwrap();
    let token = imported.1.strip_prefix("/music-preview/").unwrap();
    fixture.roots.release_imported_music_preview(token);
    assert_eq!(
        client
            .get(format!("{}{}", fixture.base, imported.1))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
}

async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn connection_limit_rejects_excess_and_reclaims_disconnected_slots() {
    let fixture = WireFixture::start().await;
    let mut sockets = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        sockets.push(TcpStream::connect(fixture.authority()).await.unwrap());
    }
    until(|| fixture.metrics.snapshot().active_connections == MAX_CONNECTIONS as u64).await;
    let mut excess = TcpStream::connect(fixture.authority()).await.unwrap();
    let mut byte = [0];
    let result = tokio::time::timeout(Duration::from_secs(2), excess.read(&mut byte))
        .await
        .unwrap();
    assert!(matches!(result, Ok(0)) || result.is_err());
    assert_eq!(fixture.metrics.snapshot().rejected_connections, 1);
    assert_eq!(
        fixture.metrics.snapshot().peak_connections,
        MAX_CONNECTIONS as u64
    );
    drop(sockets);
    until(|| fixture.metrics.snapshot().active_connections == 0).await;
    assert_eq!(
        client()
            .get(format!("{}/probe/hevc.mp4", fixture.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn oversized_and_slow_headers_have_finite_ownership() {
    let fixture = WireFixture::start().await;
    let mut socket = TcpStream::connect(fixture.authority()).await.unwrap();
    socket
        .write_all(
            format!(
                "GET / HTTP/1.1\r\nHost: {}\r\nX-Large: {}\r\n\r\n",
                fixture.authority(),
                "a".repeat(20 * 1024)
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut reply = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(2), socket.read_to_end(&mut reply))
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&reply).starts_with("HTTP/1.1 431"));
    drop(socket);
    until(|| fixture.metrics.snapshot().active_connections == 0).await;
    let mut slow = TcpStream::connect(fixture.authority()).await.unwrap();
    slow.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
    let mut reply = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(12), slow.read_to_end(&mut reply))
        .await
        .unwrap();
    until(|| fixture.metrics.snapshot().active_connections == 0).await;
}
