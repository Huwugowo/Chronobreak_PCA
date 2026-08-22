use std::collections::BTreeMap;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::watch;
use tracing::debug;

pub const CAPTURE_DIAGNOSTIC_ABI: u32 = 1;
const MAX_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingEvidence {
    pub capture_ready: bool,
    pub capture_terminal: bool,
    pub frame_pool_capacity: Option<u32>,
    pub output_pool_capacity: Option<u32>,
    pub source_frames_surfaced: u64,
    pub source_frames_superseded: u64,
    pub pool_recreations: u64,
    pub first_qpc: Option<i64>,
    pub latest_qpc: Option<i64>,
    pub encoded_frames: u64,
    pub muxed_bytes: u64,
    pub output_time_us: Option<i64>,
    pub cfr_duplicates: u64,
    pub cfr_discards: u64,
    pub progress_end: bool,
    pub protocol_error: Option<String>,
}

impl RecordingEvidence {
    pub fn startup_ready(&self) -> bool {
        self.protocol_error.is_none()
            && self.capture_ready
            && self.first_qpc.is_some()
            && self.source_frames_surfaced > 0
            && self.encoded_frames > 0
            && self.muxed_bytes > 0
            && self.output_time_us.is_some_and(|value| value > 0)
    }

    pub fn source_frames_received(&self) -> u64 {
        self.source_frames_surfaced
            .saturating_add(self.source_frames_superseded)
    }

    fn latch_error(&mut self, error: impl Into<String>) {
        if self.protocol_error.is_none() {
            self.protocol_error = Some(error.into());
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StreamKind {
    Stderr,
    Progress,
}

pub async fn drain_stream<R>(
    mut reader: R,
    kind: StreamKind,
    evidence: watch::Sender<RecordingEvidence>,
) where
    R: AsyncRead + Unpin,
{
    let mut chunk = [0_u8; 4096];
    let mut line = Vec::with_capacity(1024);
    let mut overflow = false;

    loop {
        let read = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                evidence.send_modify(|state| {
                    state.latch_error(format!("FFmpeg {kind:?} read failed: {error}"));
                });
                return;
            }
        };

        for byte in &chunk[..read] {
            if *byte == b'\n' {
                if overflow {
                    evidence.send_modify(|state| {
                        state.latch_error(format!(
                            "FFmpeg {kind:?} line exceeded {MAX_LINE_BYTES} bytes"
                        ));
                    });
                } else {
                    let text = String::from_utf8_lossy(&line);
                    ingest_line(kind, text.trim_end_matches('\r'), &evidence);
                }
                line.clear();
                overflow = false;
            } else if line.len() < MAX_LINE_BYTES {
                line.push(*byte);
            } else {
                overflow = true;
            }
        }
    }

    if overflow {
        evidence.send_modify(|state| {
            state.latch_error(format!(
                "FFmpeg {kind:?} line exceeded {MAX_LINE_BYTES} bytes"
            ));
        });
    } else if !line.is_empty() {
        let text = String::from_utf8_lossy(&line);
        ingest_line(kind, text.trim_end_matches('\r'), &evidence);
    }
}

fn ingest_line(kind: StreamKind, line: &str, evidence: &watch::Sender<RecordingEvidence>) {
    if line.is_empty() {
        return;
    }
    match kind {
        StreamKind::Stderr => {
            debug!(target: "ffmpeg", "{line}");
            if let Some(marker) = line.find("queueback_capture ") {
                let diagnostic = &line[marker + "queueback_capture ".len()..];
                evidence.send_modify(|state| ingest_capture_diagnostic(diagnostic, state));
            }
        }
        StreamKind::Progress => {
            evidence.send_modify(|state| ingest_progress_line(line, state));
        }
    }
}

fn parse_fields(line: &str) -> BTreeMap<&str, &str> {
    line.split_ascii_whitespace()
        .filter_map(|token| token.split_once('='))
        .collect()
}

fn ingest_capture_diagnostic(line: &str, evidence: &mut RecordingEvidence) {
    let fields = parse_fields(line);
    let abi = fields
        .get("abi")
        .and_then(|value| value.parse::<u32>().ok());
    if abi != Some(CAPTURE_DIAGNOSTIC_ABI) {
        evidence.latch_error(format!(
            "unsupported QueueBack capture diagnostics ABI {:?}",
            fields.get("abi")
        ));
        return;
    }
    let Some(event) = fields.get("event").copied() else {
        evidence.latch_error("QueueBack capture diagnostic is missing event");
        return;
    };

    match event {
        "ready" => {
            let Some(frame_pool) = parse_u32(&fields, "frame_pool_capacity", evidence) else {
                return;
            };
            let Some(output_pool) = parse_u32(&fields, "output_pool_capacity", evidence) else {
                return;
            };
            if frame_pool == 0 || output_pool == 0 {
                evidence.latch_error("QueueBack capture reported an unbounded/empty frame pool");
                return;
            }
            evidence.capture_ready = true;
            evidence.frame_pool_capacity = Some(frame_pool);
            evidence.output_pool_capacity = Some(output_pool);
        }
        "first_frame" | "progress" | "terminal" => {
            let Some(surfaced) = parse_u64(&fields, "source_frames_surfaced", evidence) else {
                return;
            };
            let Some(superseded) = parse_u64(&fields, "source_frames_superseded", evidence) else {
                return;
            };
            let first_qpc = parse_i64(&fields, "first_qpc", evidence);
            let latest_qpc = parse_i64(&fields, "latest_qpc", evidence);
            if first_qpc.is_none() || latest_qpc.is_none() {
                return;
            }
            if surfaced < evidence.source_frames_surfaced
                || superseded < evidence.source_frames_superseded
                || latest_qpc < evidence.latest_qpc
            {
                evidence.latch_error("QueueBack capture counters regressed");
                return;
            }
            if let Some(recreations) = fields
                .get("pool_recreations")
                .and_then(|value| value.parse::<u64>().ok())
            {
                if recreations < evidence.pool_recreations {
                    evidence.latch_error("QueueBack capture recreation counter regressed");
                    return;
                }
                evidence.pool_recreations = recreations;
            }
            evidence.source_frames_surfaced = surfaced;
            evidence.source_frames_superseded = superseded;
            evidence.first_qpc = first_qpc;
            evidence.latest_qpc = latest_qpc;
            evidence.capture_terminal |= event == "terminal";
        }
        unknown => evidence.latch_error(format!(
            "unknown QueueBack capture diagnostics event {unknown:?}"
        )),
    }
}

fn ingest_progress_line(line: &str, evidence: &mut RecordingEvidence) {
    let Some((key, value)) = line.split_once('=') else {
        evidence.latch_error("malformed FFmpeg progress line");
        return;
    };
    match key {
        "frame" => match monotonic_u64(value, "encoded frame", evidence.encoded_frames) {
            Ok(parsed) => evidence.encoded_frames = parsed,
            Err(error) => evidence.latch_error(error),
        },
        "total_size" => match monotonic_u64(value, "mux byte", evidence.muxed_bytes) {
            Ok(parsed) => evidence.muxed_bytes = parsed,
            Err(error) => evidence.latch_error(error),
        },
        "out_time_us" => {
            if value != "N/A" {
                match value.parse::<i64>() {
                    Ok(parsed) => {
                        evidence.output_time_us = Some(
                            evidence
                                .output_time_us
                                .map_or(parsed, |current| current.max(parsed)),
                        );
                    }
                    Err(_) => evidence.latch_error("invalid FFmpeg output time"),
                }
            }
        }
        "dup_frames" => match monotonic_u64(value, "CFR duplicate", evidence.cfr_duplicates) {
            Ok(parsed) => evidence.cfr_duplicates = parsed,
            Err(error) => evidence.latch_error(error),
        },
        "drop_frames" => match monotonic_u64(value, "CFR discard", evidence.cfr_discards) {
            Ok(parsed) => evidence.cfr_discards = parsed,
            Err(error) => evidence.latch_error(error),
        },
        "progress" => match value {
            "continue" => {}
            "end" => evidence.progress_end = true,
            _ => evidence.latch_error("invalid FFmpeg progress marker"),
        },
        _ => {}
    }
}

fn monotonic_u64(value: &str, label: &str, current: u64) -> Result<u64, String> {
    match value.parse::<u64>() {
        Ok(parsed) if parsed >= current => Ok(parsed),
        Ok(_) => Err(format!("FFmpeg {label} counter regressed")),
        Err(_) => Err(format!("invalid FFmpeg {label} counter")),
    }
}

fn parse_u64(
    fields: &BTreeMap<&str, &str>,
    key: &str,
    evidence: &mut RecordingEvidence,
) -> Option<u64> {
    match fields.get(key).and_then(|value| value.parse::<u64>().ok()) {
        Some(value) => Some(value),
        None => {
            evidence.latch_error(format!("QueueBack capture diagnostic has invalid {key}"));
            None
        }
    }
}

fn parse_u32(
    fields: &BTreeMap<&str, &str>,
    key: &str,
    evidence: &mut RecordingEvidence,
) -> Option<u32> {
    parse_u64(fields, key, evidence).and_then(|value| match u32::try_from(value) {
        Ok(value) => Some(value),
        Err(_) => {
            evidence.latch_error(format!("QueueBack capture diagnostic {key} is too large"));
            None
        }
    })
}

fn parse_i64(
    fields: &BTreeMap<&str, &str>,
    key: &str,
    evidence: &mut RecordingEvidence,
) -> Option<i64> {
    match fields.get(key).and_then(|value| value.parse::<i64>().ok()) {
        Some(value) if value >= 0 => Some(value),
        _ => {
            evidence.latch_error(format!("QueueBack capture diagnostic has invalid {key}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sender() -> (
        watch::Sender<RecordingEvidence>,
        watch::Receiver<RecordingEvidence>,
    ) {
        watch::channel(RecordingEvidence::default())
    }

    #[test]
    fn first_frame_and_mux_progress_are_both_required_for_readiness() {
        let (sender, receiver) = sender();
        ingest_line(
            StreamKind::Stderr,
            "[gfx] queueback_capture abi=1 event=ready frame_pool_capacity=2 output_pool_capacity=8",
            &sender,
        );
        ingest_line(
            StreamKind::Stderr,
            "[gfx] queueback_capture abi=1 event=first_frame source_frames_surfaced=1 source_frames_superseded=0 first_qpc=100 latest_qpc=100",
            &sender,
        );
        assert!(!receiver.borrow().startup_ready());
        for line in [
            "frame=2",
            "total_size=28",
            "out_time_us=16667",
            "progress=continue",
        ] {
            ingest_line(StreamKind::Progress, line, &sender);
        }
        assert!(receiver.borrow().startup_ready());
        assert_eq!(receiver.borrow().source_frames_received(), 1);
    }

    #[test]
    fn counter_regression_is_sticky() {
        let (sender, receiver) = sender();
        for line in ["frame=12", "frame=11", "frame=13"] {
            ingest_line(StreamKind::Progress, line, &sender);
        }
        assert_eq!(
            receiver.borrow().protocol_error.as_deref(),
            Some("FFmpeg encoded frame counter regressed")
        );
    }

    #[test]
    fn signed_output_time_is_tolerant_and_keeps_the_maximum() {
        let (sender, receiver) = sender();
        for line in [
            "out_time_us=-21333",
            "out_time_us=16667",
            "out_time_us=12000",
        ] {
            ingest_line(StreamKind::Progress, line, &sender);
        }
        assert_eq!(receiver.borrow().output_time_us, Some(16_667));
        assert!(receiver.borrow().protocol_error.is_none());
    }

    #[test]
    fn unknown_additive_fields_are_tolerated_but_unknown_events_are_not() {
        let (sender, receiver) = sender();
        ingest_line(
            StreamKind::Stderr,
            "queueback_capture abi=1 event=ready frame_pool_capacity=2 output_pool_capacity=8 future=ok",
            &sender,
        );
        assert!(receiver.borrow().protocol_error.is_none());
        ingest_line(
            StreamKind::Stderr,
            "queueback_capture abi=1 event=surprise",
            &sender,
        );
        assert!(receiver.borrow().protocol_error.is_some());
    }

    #[tokio::test]
    async fn oversized_line_is_discarded_while_the_stream_keeps_draining() {
        let (sender, receiver) = sender();
        let mut input = vec![b'x'; MAX_LINE_BYTES + 32];
        input.extend_from_slice(b"\nframe=7\n");
        drain_stream(input.as_slice(), StreamKind::Progress, sender).await;
        assert_eq!(receiver.borrow().encoded_frames, 7);
        assert!(receiver.borrow().protocol_error.is_some());
    }
}
