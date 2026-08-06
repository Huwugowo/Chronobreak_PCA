use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context as TaskContext, Poll};

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
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

use crate::library::{valid_clip_asset, valid_timestamp};
use crate::music;

const HEVC_PROBE: &[u8] = include_bytes!("../resources/hevc-probe.mp4");

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

#[derive(Debug)]
pub struct MediaRoots {
    output_directory: RwLock<PathBuf>,
}

impl MediaRoots {
    pub fn new(output_directory: PathBuf) -> Self {
        Self {
            output_directory: RwLock::new(output_directory),
        }
    }

    pub fn output_directory(&self) -> PathBuf {
        self.output_directory
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_output_directory(&self, output_directory: PathBuf) {
        *self
            .output_directory
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = output_directory;
    }
}

#[derive(Clone)]
struct PlaybackState {
    roots: Arc<MediaRoots>,
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

pub async fn start(roots: Arc<MediaRoots>, metrics: Arc<PlaybackMetrics>) -> Result<String> {
    let state = PlaybackState { roots, metrics };
    let router = Router::new()
        .route("/games/{timestamp}/video.mp4", any(game_video))
        .route("/clips/{filename}", any(clip_asset))
        .route("/music/{filename}", any(built_in_music))
        .route("/probe/hevc.mp4", any(hevc_probe))
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

async fn game_video(
    State(state): State<PlaybackState>,
    AxumPath(timestamp): AxumPath<String>,
    request: Request<Body>,
) -> Response<Body> {
    if !valid_timestamp(&timestamp) {
        return empty_response(StatusCode::NOT_FOUND, "video/mp4");
    }
    let path = state
        .roots
        .output_directory()
        .join("games")
        .join(timestamp)
        .join("video.mp4");
    serve_file(state, request, &path, "video/mp4").await
}

async fn clip_asset(
    State(state): State<PlaybackState>,
    AxumPath(filename): AxumPath<String>,
    request: Request<Body>,
) -> Response<Body> {
    if !valid_clip_asset(&filename) {
        return empty_response(StatusCode::NOT_FOUND, "application/octet-stream");
    }
    let content_type = if filename.ends_with(".jpg") {
        "image/jpeg"
    } else {
        "video/mp4"
    };
    let path = state.roots.output_directory().join("clips").join(filename);
    serve_file(state, request, &path, content_type).await
}

async fn hevc_probe(State(state): State<PlaybackState>, request: Request<Body>) -> Response<Body> {
    serve_embedded(state, request, HEVC_PROBE, "video/mp4").await
}

async fn built_in_music(
    State(state): State<PlaybackState>,
    AxumPath(filename): AxumPath<String>,
    request: Request<Body>,
) -> Response<Body> {
    let Some(bytes) = music::bytes_for(&filename) else {
        return empty_response(StatusCode::NOT_FOUND, "audio/mpeg");
    };
    serve_embedded(state, request, bytes, "audio/mpeg").await
}

async fn serve_embedded(
    state: PlaybackState,
    request: Request<Body>,
    bytes: &'static [u8],
    content_type: &'static str,
) -> Response<Body> {
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    if !valid_method(request.method()) {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED, content_type);
    }
    let total_length = bytes.len() as u64;
    let Some((status, start, end)) = response_range(&state, &request, total_length) else {
        return range_not_satisfiable(total_length, content_type);
    };
    let response_length = end - start + 1;
    let body = if request.method() == Method::HEAD {
        Body::empty()
    } else {
        state
            .metrics
            .response_bytes
            .fetch_add(response_length, Ordering::Relaxed);
        Body::from(bytes[start as usize..=end as usize].to_vec())
    };
    build_response(body, status, start, end, total_length, content_type)
}

async fn serve_file(
    state: PlaybackState,
    request: Request<Body>,
    path: &Path,
    content_type: &'static str,
) -> Response<Body> {
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    if !valid_method(request.method()) {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED, content_type);
    }

    let Ok(mut file) = File::open(path).await else {
        return empty_response(StatusCode::NOT_FOUND, content_type);
    };
    let Ok(metadata) = file.metadata().await else {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR, content_type);
    };
    let total_length = metadata.len();
    if total_length == 0 {
        return build_empty_file_response(content_type);
    }
    let Some((status, start, end)) = response_range(&state, &request, total_length) else {
        return range_not_satisfiable(total_length, content_type);
    };
    let response_length = end - start + 1;

    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR, content_type);
    }

    let body = if request.method() == Method::HEAD {
        Body::empty()
    } else {
        let reader = CountingReader::new(file.take(response_length), Arc::clone(&state.metrics));
        Body::from_stream(ReaderStream::new(reader))
    };
    build_response(body, status, start, end, total_length, content_type)
}

fn response_range(
    state: &PlaybackState,
    request: &Request<Body>,
    total_length: u64,
) -> Option<(StatusCode, u64, u64)> {
    match request.headers().get(RANGE) {
        Some(value) => {
            state.metrics.range_requests.fetch_add(1, Ordering::Relaxed);
            let (start, end) = value
                .to_str()
                .ok()
                .and_then(|value| parse_range(value, total_length))?;
            Some((StatusCode::PARTIAL_CONTENT, start, end))
        }
        None => Some((StatusCode::OK, 0, total_length - 1)),
    }
}

fn build_response(
    body: Body,
    status: StatusCode,
    start: u64,
    end: u64,
    total_length: u64,
    content_type: &'static str,
) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    insert_common_headers(headers, content_type);
    insert_u64_header(headers, CONTENT_LENGTH, end - start + 1);
    if status == StatusCode::PARTIAL_CONTENT
        && let Ok(value) = HeaderValue::from_str(&format!("bytes {start}-{end}/{total_length}"))
    {
        headers.insert(CONTENT_RANGE, value);
    }
    response
}

fn build_empty_file_response(content_type: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    insert_common_headers(response.headers_mut(), content_type);
    insert_u64_header(response.headers_mut(), CONTENT_LENGTH, 0);
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

fn valid_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD)
}

fn insert_common_headers(headers: &mut HeaderMap, content_type: &'static str) {
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn insert_u64_header(headers: &mut HeaderMap, name: axum::http::header::HeaderName, value: u64) {
    if let Ok(value) = HeaderValue::from_str(&value.to_string()) {
        headers.insert(name, value);
    }
}

fn empty_response(status: StatusCode, content_type: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    insert_common_headers(response.headers_mut(), content_type);
    response
}

fn range_not_satisfiable(total_length: u64, content_type: &'static str) -> Response<Body> {
    let mut response = empty_response(StatusCode::RANGE_NOT_SATISFIABLE, content_type);
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
