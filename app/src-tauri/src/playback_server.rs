#[cfg(feature = "replay-benchmark")]
use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::pin::Pin;
#[cfg(feature = "replay-benchmark")]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;
#[cfg(feature = "replay-benchmark")]
use std::time::Instant;

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

use crate::ddragon;
use crate::library::{valid_clip_asset, valid_game_id};
use crate::music;

const HEVC_PROBE: &[u8] = include_bytes!("../resources/hevc-probe.mp4");
#[cfg(feature = "replay-benchmark")]
const BENCHMARK_REQUEST_CAPACITY: usize = 4096;

#[derive(Debug, Default)]
pub struct PlaybackMetrics {
    requests: AtomicU64,
    range_requests: AtomicU64,
    response_bytes: AtomicU64,
    completed_streams: AtomicU64,
    cancelled_streams: AtomicU64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ServerMetrics {
    pub requests: u64,
    pub range_requests: u64,
    pub response_bytes: u64,
    pub completed_streams: u64,
    pub cancelled_streams: u64,
}

impl PlaybackMetrics {
    pub fn snapshot(&self) -> ServerMetrics {
        ServerMetrics {
            requests: self.requests.load(Ordering::Relaxed),
            range_requests: self.range_requests.load(Ordering::Relaxed),
            response_bytes: self.response_bytes.load(Ordering::Relaxed),
            completed_streams: self.completed_streams.load(Ordering::Relaxed),
            cancelled_streams: self.cancelled_streams.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteClass {
    GameVideo,
    Clip,
    BuiltInMusic,
    ImportedMusic,
    HevcProbe,
    Ddragon,
}

#[cfg(feature = "replay-benchmark")]
#[derive(Debug, Clone, Serialize)]
pub struct RequestLifecycle {
    pub schema_version: u32,
    pub run_id: String,
    pub scenario_id: String,
    pub trial_id: String,
    pub monotonic_ms: f64,
    pub source: String,
    pub kind: String,
    pub request_id: u64,
    pub route_class: RouteClass,
    pub method: String,
    pub status: u16,
    pub range_start: Option<u64>,
    pub range_end: Option<u64>,
    pub declared_bytes: u64,
    pub delivered_bytes: u64,
    pub started_ms: f64,
    pub first_byte_ms: Option<f64>,
    pub completed_ms: f64,
    pub outcome: String,
}

#[cfg(feature = "replay-benchmark")]
#[derive(Debug, Clone, Serialize)]
pub struct RequestTelemetrySnapshot {
    pub capacity: u64,
    pub high_water_mark: u64,
    pub overwritten_records: u64,
    pub active_streams: u64,
    pub peak_active_streams: u64,
    pub pending_records: u64,
    pub requests: Vec<RequestLifecycle>,
}

#[cfg(feature = "replay-benchmark")]
#[derive(Debug, Default)]
struct RequestBuffer {
    records: VecDeque<RequestLifecycle>,
    high_water_mark: u64,
}

#[cfg(feature = "replay-benchmark")]
#[derive(Debug)]
pub struct BenchmarkRequestTelemetry {
    run_id: String,
    scenario_id: String,
    trial_id: String,
    monotonic_offset_ms: f64,
    started_at: Instant,
    next_request_id: AtomicU64,
    active_streams: AtomicU64,
    peak_active_streams: AtomicU64,
    overwritten_records: AtomicU64,
    buffer: Mutex<RequestBuffer>,
}

#[cfg(feature = "replay-benchmark")]
impl BenchmarkRequestTelemetry {
    pub fn new(
        run_id: String,
        scenario_id: String,
        trial_id: String,
        monotonic_offset_ms: f64,
    ) -> Self {
        Self {
            run_id,
            scenario_id,
            trial_id,
            monotonic_offset_ms,
            started_at: Instant::now(),
            next_request_id: AtomicU64::new(1),
            active_streams: AtomicU64::new(0),
            peak_active_streams: AtomicU64::new(0),
            overwritten_records: AtomicU64::new(0),
            buffer: Mutex::new(RequestBuffer::default()),
        }
    }

    fn elapsed_ms(&self) -> f64 {
        self.monotonic_offset_ms + self.started_at.elapsed().as_secs_f64() * 1_000.0
    }

    fn begin(self: &Arc<Self>, route_class: RouteClass, method: &Method) -> PendingRequest {
        let active = self.active_streams.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_active_streams
            .fetch_max(active, Ordering::Relaxed);
        PendingRequest {
            telemetry: Arc::clone(self),
            request_id: self.next_request_id.fetch_add(1, Ordering::Relaxed),
            route_class,
            method: method.as_str().to_owned(),
            started_ms: self.elapsed_ms(),
            finished: false,
        }
    }

    fn push(&self, record: RequestLifecycle) {
        let mut buffer = self
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if buffer.records.len() == BENCHMARK_REQUEST_CAPACITY {
            buffer.records.pop_front();
            self.overwritten_records.fetch_add(1, Ordering::Relaxed);
        }
        buffer.records.push_back(record);
        buffer.high_water_mark = buffer.high_water_mark.max(buffer.records.len() as u64);
    }

    pub fn take(&self, maximum: usize) -> RequestTelemetrySnapshot {
        let mut buffer = self
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let count = maximum.min(buffer.records.len());
        let requests = buffer.records.drain(..count).collect::<Vec<_>>();
        RequestTelemetrySnapshot {
            capacity: BENCHMARK_REQUEST_CAPACITY as u64,
            high_water_mark: buffer.high_water_mark,
            overwritten_records: self.overwritten_records.load(Ordering::Relaxed),
            active_streams: self.active_streams.load(Ordering::Relaxed),
            peak_active_streams: self.peak_active_streams.load(Ordering::Relaxed),
            pending_records: buffer.records.len() as u64,
            requests,
        }
    }
}

#[cfg(feature = "replay-benchmark")]
struct PendingRequest {
    telemetry: Arc<BenchmarkRequestTelemetry>,
    request_id: u64,
    route_class: RouteClass,
    method: String,
    started_ms: f64,
    finished: bool,
}

#[cfg(feature = "replay-benchmark")]
impl PendingRequest {
    fn finish(
        mut self,
        status: StatusCode,
        range: Option<(u64, u64)>,
        declared_bytes: u64,
        delivered_bytes: u64,
        first_byte: bool,
        outcome: &str,
    ) {
        let completed_ms = self.telemetry.elapsed_ms();
        self.telemetry.push(RequestLifecycle {
            schema_version: 1,
            run_id: self.telemetry.run_id.clone(),
            scenario_id: self.telemetry.scenario_id.clone(),
            trial_id: self.telemetry.trial_id.clone(),
            monotonic_ms: completed_ms,
            source: "server".to_owned(),
            kind: "request_lifecycle".to_owned(),
            request_id: self.request_id,
            route_class: self.route_class,
            method: self.method.clone(),
            status: status.as_u16(),
            range_start: range.map(|(start, _)| start),
            range_end: range.map(|(_, end)| end),
            declared_bytes,
            delivered_bytes,
            started_ms: self.started_ms,
            first_byte_ms: first_byte.then_some(completed_ms),
            completed_ms,
            outcome: outcome.to_owned(),
        });
        self.finished = true;
        self.telemetry
            .active_streams
            .fetch_sub(1, Ordering::Relaxed);
    }

    fn stream(
        mut self,
        status: StatusCode,
        range: (u64, u64),
        declared_bytes: u64,
    ) -> RequestTracker {
        self.finished = true;
        RequestTracker {
            telemetry: Arc::clone(&self.telemetry),
            request_id: self.request_id,
            route_class: self.route_class,
            method: self.method.clone(),
            status,
            range,
            declared_bytes,
            delivered_bytes: 0,
            started_ms: self.started_ms,
            first_byte_ms: None,
            finished: false,
        }
    }
}

#[cfg(feature = "replay-benchmark")]
impl Drop for PendingRequest {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let completed_ms = self.telemetry.elapsed_ms();
        self.telemetry.push(RequestLifecycle {
            schema_version: 1,
            run_id: self.telemetry.run_id.clone(),
            scenario_id: self.telemetry.scenario_id.clone(),
            trial_id: self.telemetry.trial_id.clone(),
            monotonic_ms: completed_ms,
            source: "server".to_owned(),
            kind: "request_lifecycle".to_owned(),
            request_id: self.request_id,
            route_class: self.route_class,
            method: self.method.clone(),
            status: 499,
            range_start: None,
            range_end: None,
            declared_bytes: 0,
            delivered_bytes: 0,
            started_ms: self.started_ms,
            first_byte_ms: None,
            completed_ms,
            outcome: "cancelled".to_owned(),
        });
        self.telemetry
            .active_streams
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(feature = "replay-benchmark")]
struct RequestTracker {
    telemetry: Arc<BenchmarkRequestTelemetry>,
    request_id: u64,
    route_class: RouteClass,
    method: String,
    status: StatusCode,
    range: (u64, u64),
    declared_bytes: u64,
    delivered_bytes: u64,
    started_ms: f64,
    first_byte_ms: Option<f64>,
    finished: bool,
}

#[cfg(feature = "replay-benchmark")]
impl RequestTracker {
    fn observe(&mut self, bytes: u64) {
        if bytes > 0 && self.first_byte_ms.is_none() {
            self.first_byte_ms = Some(self.telemetry.elapsed_ms());
        }
        self.delivered_bytes = self.delivered_bytes.saturating_add(bytes);
    }

    fn complete(&mut self) {
        self.finish("completed");
    }

    fn finish(&mut self, outcome: &str) {
        if self.finished {
            return;
        }
        self.finished = true;
        let completed_ms = self.telemetry.elapsed_ms();
        self.telemetry.push(RequestLifecycle {
            schema_version: 1,
            run_id: self.telemetry.run_id.clone(),
            scenario_id: self.telemetry.scenario_id.clone(),
            trial_id: self.telemetry.trial_id.clone(),
            monotonic_ms: completed_ms,
            source: "server".to_owned(),
            kind: "request_lifecycle".to_owned(),
            request_id: self.request_id,
            route_class: self.route_class,
            method: self.method.clone(),
            status: self.status.as_u16(),
            range_start: Some(self.range.0),
            range_end: Some(self.range.1),
            declared_bytes: self.declared_bytes,
            delivered_bytes: self.delivered_bytes,
            started_ms: self.started_ms,
            first_byte_ms: self.first_byte_ms,
            completed_ms,
            outcome: outcome.to_owned(),
        });
        self.telemetry
            .active_streams
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(feature = "replay-benchmark")]
impl Drop for RequestTracker {
    fn drop(&mut self) {
        self.finish("cancelled");
    }
}

#[derive(Debug)]
pub struct MediaRoots {
    output_directory: RwLock<PathBuf>,
    imported_music_preview: RwLock<Option<(String, PathBuf)>>,
    imported_music_token: AtomicU64,
}

impl MediaRoots {
    pub fn new(output_directory: PathBuf) -> Self {
        Self {
            output_directory: RwLock::new(output_directory),
            imported_music_preview: RwLock::new(None),
            imported_music_token: AtomicU64::new(0),
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

    pub fn register_imported_music_preview(&self, path: PathBuf) -> String {
        let token = format!(
            "{:016x}",
            self.imported_music_token.fetch_add(1, Ordering::Relaxed) + 1
        );
        *self
            .imported_music_preview
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((token.clone(), path));
        token
    }

    fn imported_music_preview(&self, token: &str) -> Option<PathBuf> {
        self.imported_music_preview
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(|(registered, path)| registered == token && path.is_file())
            .map(|(_, path)| path.clone())
    }
}

#[derive(Clone)]
struct PlaybackState {
    roots: Arc<MediaRoots>,
    metrics: Arc<PlaybackMetrics>,
    #[cfg(feature = "replay-benchmark")]
    benchmark_requests: Option<Arc<BenchmarkRequestTelemetry>>,
    ddragon_cache: PathBuf,
    ddragon_client: reqwest::Client,
    allow_ddragon_network: bool,
}

struct CountingReader<R> {
    inner: R,
    metrics: Arc<PlaybackMetrics>,
    expected_bytes: u64,
    consumed_bytes: u64,
    completed: bool,
    #[cfg(feature = "replay-benchmark")]
    request: Option<RequestTracker>,
}

impl<R> CountingReader<R> {
    #[cfg(not(feature = "replay-benchmark"))]
    fn new(inner: R, metrics: Arc<PlaybackMetrics>, expected_bytes: u64) -> Self {
        Self {
            inner,
            metrics,
            expected_bytes,
            consumed_bytes: 0,
            completed: false,
        }
    }

    #[cfg(feature = "replay-benchmark")]
    fn new(
        inner: R,
        metrics: Arc<PlaybackMetrics>,
        expected_bytes: u64,
        request: Option<RequestTracker>,
    ) -> Self {
        Self {
            inner,
            metrics,
            expected_bytes,
            consumed_bytes: 0,
            completed: false,
            request,
        }
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
        match &result {
            Poll::Ready(Ok(())) => {
                let bytes_read = buffer.filled().len().saturating_sub(filled_before) as u64;
                this.metrics
                    .response_bytes
                    .fetch_add(bytes_read, Ordering::Relaxed);
                this.consumed_bytes = this.consumed_bytes.saturating_add(bytes_read);
                #[cfg(feature = "replay-benchmark")]
                if let Some(request) = &mut this.request {
                    request.observe(bytes_read);
                }
                if !this.completed && this.consumed_bytes >= this.expected_bytes {
                    this.completed = true;
                    this.metrics
                        .completed_streams
                        .fetch_add(1, Ordering::Relaxed);
                    #[cfg(feature = "replay-benchmark")]
                    if let Some(request) = &mut this.request {
                        request.complete();
                    }
                }
            }
            Poll::Ready(Err(_)) =>
            {
                #[cfg(feature = "replay-benchmark")]
                if let Some(request) = &mut this.request {
                    request.finish("error");
                }
            }
            Poll::Pending => {}
        }
        result
    }
}

impl<R> Drop for CountingReader<R> {
    fn drop(&mut self) {
        if !self.completed && self.consumed_bytes < self.expected_bytes {
            self.metrics
                .cancelled_streams
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub async fn start(
    roots: Arc<MediaRoots>,
    metrics: Arc<PlaybackMetrics>,
    ddragon_cache: PathBuf,
    #[cfg(feature = "replay-benchmark")] benchmark_requests: Option<Arc<BenchmarkRequestTelemetry>>,
    allow_ddragon_network: bool,
) -> Result<String> {
    let ddragon_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .context("failed to build the Data Dragon asset client")?;
    let state = PlaybackState {
        roots,
        metrics,
        #[cfg(feature = "replay-benchmark")]
        benchmark_requests,
        ddragon_cache,
        ddragon_client,
        allow_ddragon_network,
    };
    let router = Router::new()
        .route("/games/{timestamp}/video.mp4", any(game_video))
        .route("/clips/{filename}", any(clip_asset))
        .route("/music/{filename}", any(built_in_music))
        .route("/music-preview/{token}", any(imported_music_preview))
        .route("/probe/hevc.mp4", any(hevc_probe))
        .route("/ddragon/{version}/{kind}/{asset}", any(ddragon_asset))
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
    if !valid_game_id(&timestamp) {
        return empty_response(StatusCode::NOT_FOUND, "video/mp4");
    }
    let path = state
        .roots
        .output_directory()
        .join("games")
        .join(timestamp)
        .join("video.mp4");
    serve_file(state, request, &path, "video/mp4", RouteClass::GameVideo).await
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
    serve_file(state, request, &path, content_type, RouteClass::Clip).await
}

async fn hevc_probe(State(state): State<PlaybackState>, request: Request<Body>) -> Response<Body> {
    serve_embedded(
        state,
        request,
        HEVC_PROBE,
        "video/mp4",
        RouteClass::HevcProbe,
    )
    .await
}

async fn built_in_music(
    State(state): State<PlaybackState>,
    AxumPath(filename): AxumPath<String>,
    request: Request<Body>,
) -> Response<Body> {
    let Some(bytes) = music::bytes_for(&filename) else {
        return empty_response(StatusCode::NOT_FOUND, "audio/mpeg");
    };
    serve_embedded(
        state,
        request,
        bytes,
        "audio/mpeg",
        RouteClass::BuiltInMusic,
    )
    .await
}

async fn imported_music_preview(
    State(state): State<PlaybackState>,
    AxumPath(token): AxumPath<String>,
    request: Request<Body>,
) -> Response<Body> {
    let Some(path) = state.roots.imported_music_preview(&token) else {
        return empty_response(StatusCode::NOT_FOUND, "application/octet-stream");
    };
    let content_type = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => return empty_response(StatusCode::NOT_FOUND, "application/octet-stream"),
    };
    serve_file(
        state,
        request,
        &path,
        content_type,
        RouteClass::ImportedMusic,
    )
    .await
}

async fn ddragon_asset(
    State(state): State<PlaybackState>,
    AxumPath((version, kind, asset)): AxumPath<(String, String, String)>,
    request: Request<Body>,
) -> Response<Body> {
    if !valid_method(request.method()) {
        return empty_response(StatusCode::METHOD_NOT_ALLOWED, "image/png");
    }
    let path = match ddragon::ensure_asset(
        &state.ddragon_client,
        &state.ddragon_cache,
        &version,
        &kind,
        &asset,
        state.allow_ddragon_network,
    )
    .await
    {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Data Dragon asset unavailable: {error}");
            return empty_response(StatusCode::NOT_FOUND, "image/png");
        }
    };
    let mut response = serve_file(state, request, &path, "image/png", RouteClass::Ddragon).await;
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
}

async fn serve_embedded(
    state: PlaybackState,
    request: Request<Body>,
    bytes: &'static [u8],
    content_type: &'static str,
    route_class: RouteClass,
) -> Response<Body> {
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    #[cfg(feature = "replay-benchmark")]
    let pending = state
        .benchmark_requests
        .as_ref()
        .map(|telemetry| telemetry.begin(route_class, request.method()));
    #[cfg(not(feature = "replay-benchmark"))]
    let _ = route_class;
    if !valid_method(request.method()) {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::METHOD_NOT_ALLOWED,
            None,
            0,
            0,
            false,
            "error",
        );
        return empty_response(StatusCode::METHOD_NOT_ALLOWED, content_type);
    }
    let total_length = bytes.len() as u64;
    let Some((status, start, end)) = response_range(&state, &request, total_length) else {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::RANGE_NOT_SATISFIABLE,
            None,
            0,
            0,
            false,
            "error",
        );
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
    #[cfg(feature = "replay-benchmark")]
    let delivered = if request.method() == Method::HEAD {
        0
    } else {
        response_length
    };
    #[cfg(feature = "replay-benchmark")]
    finish_immediate(
        pending,
        status,
        Some((start, end)),
        response_length,
        delivered,
        delivered > 0,
        "completed",
    );
    build_response(body, status, start, end, total_length, content_type)
}

async fn serve_file(
    state: PlaybackState,
    request: Request<Body>,
    path: &Path,
    content_type: &'static str,
    route_class: RouteClass,
) -> Response<Body> {
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    #[cfg(feature = "replay-benchmark")]
    let pending = state
        .benchmark_requests
        .as_ref()
        .map(|telemetry| telemetry.begin(route_class, request.method()));
    #[cfg(not(feature = "replay-benchmark"))]
    let _ = route_class;
    if !valid_method(request.method()) {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::METHOD_NOT_ALLOWED,
            None,
            0,
            0,
            false,
            "error",
        );
        return empty_response(StatusCode::METHOD_NOT_ALLOWED, content_type);
    }

    let Ok(mut file) = File::open(path).await else {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(pending, StatusCode::NOT_FOUND, None, 0, 0, false, "error");
        return empty_response(StatusCode::NOT_FOUND, content_type);
    };
    let Ok(metadata) = file.metadata().await else {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            0,
            0,
            false,
            "error",
        );
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR, content_type);
    };
    let total_length = metadata.len();
    if total_length == 0 {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(pending, StatusCode::OK, None, 0, 0, false, "completed");
        return build_empty_file_response(content_type);
    }
    let Some((status, start, end)) = response_range(&state, &request, total_length) else {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::RANGE_NOT_SATISFIABLE,
            None,
            0,
            0,
            false,
            "error",
        );
        return range_not_satisfiable(total_length, content_type);
    };
    let response_length = end - start + 1;

    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            StatusCode::INTERNAL_SERVER_ERROR,
            Some((start, end)),
            response_length,
            0,
            false,
            "error",
        );
        return empty_response(StatusCode::INTERNAL_SERVER_ERROR, content_type);
    }

    let body = if request.method() == Method::HEAD {
        #[cfg(feature = "replay-benchmark")]
        finish_immediate(
            pending,
            status,
            Some((start, end)),
            response_length,
            0,
            false,
            "completed",
        );
        Body::empty()
    } else {
        #[cfg(feature = "replay-benchmark")]
        let request = pending.map(|pending| pending.stream(status, (start, end), response_length));
        #[cfg(feature = "replay-benchmark")]
        let reader = CountingReader::new(
            file.take(response_length),
            Arc::clone(&state.metrics),
            response_length,
            request,
        );
        #[cfg(not(feature = "replay-benchmark"))]
        let reader = CountingReader::new(
            file.take(response_length),
            Arc::clone(&state.metrics),
            response_length,
        );
        Body::from_stream(ReaderStream::new(reader))
    };
    build_response(body, status, start, end, total_length, content_type)
}

#[cfg(feature = "replay-benchmark")]
fn finish_immediate(
    pending: Option<PendingRequest>,
    status: StatusCode,
    range: Option<(u64, u64)>,
    declared_bytes: u64,
    delivered_bytes: u64,
    first_byte: bool,
    outcome: &str,
) {
    if let Some(pending) = pending {
        pending.finish(
            status,
            range,
            declared_bytes,
            delivered_bytes,
            first_byte,
            outcome,
        );
    }
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

    #[test]
    fn newest_imported_music_preview_invalidates_the_previous_token() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.mp3");
        let second = directory.path().join("second.wav");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();
        let roots = MediaRoots::new(directory.path().to_path_buf());

        let first_token = roots.register_imported_music_preview(first.clone());
        assert_eq!(roots.imported_music_preview(&first_token), Some(first));

        let second_token = roots.register_imported_music_preview(second.clone());
        assert_ne!(first_token, second_token);
        assert_eq!(roots.imported_music_preview(&first_token), None);
        assert_eq!(roots.imported_music_preview(&second_token), Some(second));
    }

    #[tokio::test]
    async fn serves_collision_suffixed_game_ids_without_relaxing_path_validation() {
        let directory = tempfile::tempdir().unwrap();
        let game_directory = directory.path().join("games").join("1786000000-1");
        std::fs::create_dir_all(&game_directory).unwrap();
        std::fs::write(game_directory.join("video.mp4"), b"video").unwrap();
        let state = PlaybackState {
            roots: Arc::new(MediaRoots::new(directory.path().to_path_buf())),
            metrics: Arc::new(PlaybackMetrics::default()),
            #[cfg(feature = "replay-benchmark")]
            benchmark_requests: None,
            ddragon_cache: directory.path().join("ddragon"),
            ddragon_client: reqwest::Client::new(),
            allow_ddragon_network: true,
        };
        let request = || Request::builder().body(Body::empty()).unwrap();

        let response = game_video(
            State(state.clone()),
            AxumPath("1786000000-1".to_owned()),
            request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        for token in ["first-session", "second-session"] {
            let response = game_video(
                State(state.clone()),
                AxumPath("1786000000-1".to_owned()),
                Request::builder()
                    .uri(format!(
                        "/games/1786000000-1/video.mp4?qb_playback_session={token}"
                    ))
                    .header(RANGE, "bytes=1-3")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(response.headers()[CONTENT_RANGE], "bytes 1-3/5");
            assert_eq!(response.headers()[CONTENT_LENGTH], "3");
            assert_eq!(
                axum::body::to_bytes(response.into_body(), 5)
                    .await
                    .unwrap()
                    .as_ref(),
                b"ide"
            );
        }

        let response = game_video(
            State(state),
            AxumPath("../1786000000-1".to_owned()),
            request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "replay-benchmark")]
    #[tokio::test]
    async fn head_request_declares_the_file_without_claiming_body_delivery() {
        let directory = tempfile::tempdir().unwrap();
        let game_directory = directory.path().join("games").join("1786000000-1");
        std::fs::create_dir_all(&game_directory).unwrap();
        std::fs::write(game_directory.join("video.mp4"), b"video").unwrap();
        let telemetry = Arc::new(BenchmarkRequestTelemetry::new(
            "run-1".to_owned(),
            "cold-open".to_owned(),
            "trial-1".to_owned(),
            0.0,
        ));
        let state = PlaybackState {
            roots: Arc::new(MediaRoots::new(directory.path().to_path_buf())),
            metrics: Arc::new(PlaybackMetrics::default()),
            benchmark_requests: Some(Arc::clone(&telemetry)),
            ddragon_cache: directory.path().join("ddragon"),
            ddragon_client: reqwest::Client::new(),
            allow_ddragon_network: true,
        };
        let request = Request::builder()
            .method(Method::HEAD)
            .body(Body::empty())
            .unwrap();

        let response = game_video(State(state), AxumPath("1786000000-1".to_owned()), request).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CONTENT_LENGTH], "5");
        let snapshot = telemetry.take(1);
        assert_eq!(snapshot.active_streams, 0);
        assert_eq!(snapshot.requests.len(), 1);
        let request = &snapshot.requests[0];
        assert_eq!(request.method, "HEAD");
        assert_eq!(request.declared_bytes, 5);
        assert_eq!(request.delivered_bytes, 0);
        assert_eq!(request.outcome, "completed");
        assert!(request.first_byte_ms.is_none());
    }

    #[tokio::test]
    async fn counts_only_bytes_consumed_from_a_response() {
        let metrics = Arc::new(PlaybackMetrics::default());
        let source = tokio::io::repeat(7).take(16);
        #[cfg(not(feature = "replay-benchmark"))]
        let mut reader = CountingReader::new(source, Arc::clone(&metrics), 16);
        #[cfg(feature = "replay-benchmark")]
        let mut reader = CountingReader::new(source, Arc::clone(&metrics), 16, None);
        let mut consumed = [0_u8; 6];

        reader.read_exact(&mut consumed).await.unwrap();

        assert_eq!(metrics.snapshot().response_bytes, 6);
        drop(reader);
        assert_eq!(metrics.snapshot().cancelled_streams, 1);
    }

    #[tokio::test]
    async fn counts_completed_streams() {
        let metrics = Arc::new(PlaybackMetrics::default());
        let source = tokio::io::repeat(7).take(8);
        #[cfg(not(feature = "replay-benchmark"))]
        let mut reader = CountingReader::new(source, Arc::clone(&metrics), 8);
        #[cfg(feature = "replay-benchmark")]
        let mut reader = CountingReader::new(source, Arc::clone(&metrics), 8, None);
        let mut consumed = [0_u8; 8];

        reader.read_exact(&mut consumed).await.unwrap();

        assert_eq!(metrics.snapshot().completed_streams, 1);
        assert_eq!(metrics.snapshot().cancelled_streams, 0);
    }

    #[cfg(feature = "replay-benchmark")]
    #[tokio::test]
    async fn benchmark_request_telemetry_reconciles_a_completed_stream() {
        let telemetry = Arc::new(BenchmarkRequestTelemetry::new(
            "run-1".to_owned(),
            "seek".to_owned(),
            "trial-1".to_owned(),
            25.0,
        ));
        let pending = telemetry.begin(RouteClass::GameVideo, &Method::GET);
        let tracker = pending.stream(StatusCode::PARTIAL_CONTENT, (10, 13), 4);
        let metrics = Arc::new(PlaybackMetrics::default());
        let source = tokio::io::repeat(7).take(4);
        let mut reader = CountingReader::new(source, metrics, 4, Some(tracker));
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        drop(reader);

        let snapshot = telemetry.take(128);
        assert_eq!(snapshot.active_streams, 0);
        assert_eq!(snapshot.peak_active_streams, 1);
        assert_eq!(snapshot.overwritten_records, 0);
        assert_eq!(snapshot.requests.len(), 1);
        let request = &snapshot.requests[0];
        assert_eq!(request.run_id, "run-1");
        assert_eq!(request.scenario_id, "seek");
        assert_eq!(request.trial_id, "trial-1");
        assert_eq!(request.route_class, RouteClass::GameVideo);
        assert_eq!(request.delivered_bytes, 4);
        assert_eq!(request.outcome, "completed");
        assert!(request.first_byte_ms.is_some());
        assert!(request.completed_ms >= request.started_ms);
    }

    #[cfg(feature = "replay-benchmark")]
    #[test]
    fn benchmark_request_cancelled_before_response_is_not_reported_as_an_error() {
        let telemetry = Arc::new(BenchmarkRequestTelemetry::new(
            "run-1".to_owned(),
            "seek".to_owned(),
            "trial-1".to_owned(),
            0.0,
        ));

        drop(telemetry.begin(RouteClass::GameVideo, &Method::GET));

        let snapshot = telemetry.take(1);
        assert_eq!(snapshot.active_streams, 0);
        assert_eq!(snapshot.requests.len(), 1);
        assert_eq!(snapshot.requests[0].status, 499);
        assert_eq!(snapshot.requests[0].outcome, "cancelled");
    }
}
