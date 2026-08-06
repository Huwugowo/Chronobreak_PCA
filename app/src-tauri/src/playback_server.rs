use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context as TaskContext, Poll};

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{
    ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_TYPE, RANGE,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use axum::routing::any;
use serde::Serialize;
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, ReadBuf, SeekFrom};
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

#[derive(Debug, Default)]
pub struct PlaybackMetrics {
    requests: AtomicU64,
    range_requests: AtomicU64,
    response_bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ServerMetrics {
    pub requests: u64,
    pub range_requests: u64,
    pub response_bytes: u64,
}

impl PlaybackMetrics {
    pub fn snapshot(&self) -> ServerMetrics {
        ServerMetrics {
            requests: self.requests.load(Ordering::Relaxed),
            range_requests: self.range_requests.load(Ordering::Relaxed),
            response_bytes: self.response_bytes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
struct PlaybackState {
    games_directory: PathBuf,
    metrics: Arc<PlaybackMetrics>,
}

struct CountingReader<R> {
    inner: R,
    metrics: Arc<PlaybackMetrics>,
}

impl<R> CountingReader<R> {
    fn new(inner: R, metrics: Arc<PlaybackMetrics>) -> Self {
        Self { inner, metrics }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let filled_before = buffer.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(context, buffer);
        if let Poll::Ready(Ok(())) = &result {
            let bytes_read = buffer.filled().len().saturating_sub(filled_before) as u64;
            this.metrics
                .response_bytes
                .fetch_add(bytes_read, Ordering::Relaxed);
        }
        result
    }
}

pub async fn start(games_directory: PathBuf, metrics: Arc<PlaybackMetrics>) -> Result<String> {
    let state = PlaybackState {
        games_directory,
        metrics,
    };
    let router = Router::new()
        .route("/games/{timestamp}/video.mp4", any(video))
        .with_state(state);
    let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .context("failed to bind the local playback server")?;
    let address = listener
        .local_addr()
        .context("failed to read the local playback server address")?;

    tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            eprintln!("local playback server stopped: {error}");
        }
    });

    Ok(format!("http://{address}"))
}

async fn video(
    State(state): State<PlaybackState>,
    Path(timestamp): Path<String>,
    request: Request<Body>,
) -> Response<Body> {
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED);
    }
    if !valid_timestamp(&timestamp) {
        return empty_response(StatusCode::NOT_FOUND);
    }

    let path = state.games_directory.join(timestamp).join("video.mp4");
    let Ok(mut file) = File::open(path).await else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    let Ok(metadata) = file.metadata().await else {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let total_length = metadata.len();
    let requested_range = match request.headers().get(RANGE) {
        Some(value) => {
            state.metrics.range_requests.fetch_add(1, Ordering::Relaxed);
            match value
                .to_str()
                .ok()
                .and_then(|value| parse_range(value, total_length))
            {
                Some(range) => Some(range),
                None => return range_not_satisfiable(total_length),
            }
        }
        None => None,
    };

    let (status, start, end) = requested_range
        .map(|(start, end)| (StatusCode::PARTIAL_CONTENT, start, end))
        .unwrap_or_else(|| (StatusCode::OK, 0, total_length.saturating_sub(1)));
    let response_length = if total_length == 0 {
        0
    } else {
        end - start + 1
    };

    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let body = if request.method() == Method::HEAD || response_length == 0 {
        Body::empty()
    } else {
        let reader = CountingReader::new(file.take(response_length), Arc::clone(&state.metrics));
        let stream = ReaderStream::new(reader);
        Body::from_stream(stream)
    };

    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    insert_common_headers(headers);
    insert_u64_header(headers, CONTENT_LENGTH, response_length);
    if status == StatusCode::PARTIAL_CONTENT {
        let value = format!("bytes {start}-{end}/{total_length}");
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert(CONTENT_RANGE, value);
        }
    }
    response
}

fn parse_range(value: &str, total_length: u64) -> Option<(u64, u64)> {
    if total_length == 0 || value.contains(',') {
        return None;
    }
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix_length = end.parse::<u64>().ok()?.min(total_length);
        if suffix_length == 0 {
            return None;
        }
        return Some((total_length - suffix_length, total_length - 1));
    }

    let start = start.parse::<u64>().ok()?;
    if start >= total_length {
        return None;
    }
    let end = if end.is_empty() {
        total_length - 1
    } else {
        end.parse::<u64>().ok()?.min(total_length - 1)
    };
    (start <= end).then_some((start, end))
}

fn valid_timestamp(timestamp: &str) -> bool {
    !timestamp.is_empty() && timestamp.bytes().all(|byte| byte.is_ascii_digit())
}

fn insert_common_headers(headers: &mut HeaderMap) {
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn insert_u64_header(headers: &mut HeaderMap, name: axum::http::header::HeaderName, value: u64) {
    if let Ok(value) = HeaderValue::from_str(&value.to_string()) {
        headers.insert(name, value);
    }
}

fn empty_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    insert_common_headers(response.headers_mut());
    response
}

fn range_not_satisfiable(total_length: u64) -> Response<Body> {
    let mut response = empty_response(StatusCode::RANGE_NOT_SATISFIABLE);
    if let Ok(value) = HeaderValue::from_str(&format!("bytes */{total_length}")) {
        response.headers_mut().insert(CONTENT_RANGE, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_browser_byte_ranges() {
        assert_eq!(parse_range("bytes=0-99", 1_000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1_000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1_000), Some((900, 999)));
        assert_eq!(parse_range("bytes=900-2000", 1_000), Some((900, 999)));
    }

    #[test]
    fn rejects_invalid_or_multi_ranges() {
        assert_eq!(parse_range("items=0-10", 1_000), None);
        assert_eq!(parse_range("bytes=1000-", 1_000), None);
        assert_eq!(parse_range("bytes=100-10", 1_000), None);
        assert_eq!(parse_range("bytes=0-1,4-5", 1_000), None);
        assert_eq!(parse_range("bytes=-0", 1_000), None);
    }

    #[test]
    fn accepts_only_timestamp_path_segments() {
        assert!(valid_timestamp("1786000000"));
        assert!(!valid_timestamp("../1786000000"));
        assert!(!valid_timestamp(""));
    }

    #[tokio::test]
    async fn counts_only_bytes_consumed_from_a_response() {
        let metrics = Arc::new(PlaybackMetrics::default());
        let source = tokio::io::repeat(7).take(16);
        let mut reader = CountingReader::new(source, Arc::clone(&metrics));
        let mut consumed = [0_u8; 6];

        reader.read_exact(&mut consumed).await.unwrap();

        assert_eq!(metrics.snapshot().response_bytes, 6);
    }
}
