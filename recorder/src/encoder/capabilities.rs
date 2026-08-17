use std::fmt;

use super::{EncoderKind, VideoCodec};

pub const WGC_FRAME_POOL_CAPACITY: u32 = 2;
pub const FILTER_BUFFERED_FRAME_LIMIT: u32 = 32;
pub const NVENC_SURFACE_LIMIT: u32 = 4;
pub const AMF_ASYNC_DEPTH_LIMIT: u32 = 4;
pub const QSV_ASYNC_DEPTH_LIMIT: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectInterop {
    D3d11Nvenc,
    D3d11Amf,
    D3d11Qsv,
}

impl DirectInterop {
    pub fn label(self) -> &'static str {
        match self {
            Self::D3d11Nvenc => "d3d11-nvenc-direct",
            Self::D3d11Amf => "d3d11-amf-direct",
            Self::D3d11Qsv => "d3d11-qsv-direct-map",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderCapability {
    pub encoder: EncoderKind,
    pub codec: VideoCodec,
    pub adapter_luid: u64,
    pub interop: DirectInterop,
    pub bounded_depth: Option<u32>,
    pub direct_hardware_frames: bool,
}

impl EncoderCapability {
    pub fn compiled_candidate(encoder: EncoderKind, codec: VideoCodec, adapter_luid: u64) -> Self {
        let (interop, bounded_depth) = match encoder {
            EncoderKind::Nvenc => (DirectInterop::D3d11Nvenc, NVENC_SURFACE_LIMIT),
            EncoderKind::Amf => (DirectInterop::D3d11Amf, AMF_ASYNC_DEPTH_LIMIT),
            EncoderKind::Qsv => (DirectInterop::D3d11Qsv, QSV_ASYNC_DEPTH_LIMIT),
            EncoderKind::Videotoolbox => {
                unreachable!("VideoToolbox is not a Windows D3D11 encoder")
            }
        };
        Self {
            encoder,
            codec,
            adapter_luid,
            interop,
            bounded_depth: Some(bounded_depth),
            direct_hardware_frames: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureCandidate {
    pub encoder: EncoderKind,
    pub codec: VideoCodec,
    pub adapter_luid: u64,
    pub interop: DirectInterop,
    pub encoder_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    EncoderUnavailable,
    CrossAdapterUnvalidated,
    DirectInteropUnavailable,
    UnboundedEncoderQueue,
}

impl fmt::Display for UnsupportedReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EncoderUnavailable => "no compiled candidate supports the requested codec",
            Self::CrossAdapterUnvalidated => {
                "compatible encoders exist only on a different physical adapter"
            }
            Self::DirectInteropUnavailable => {
                "no candidate proves direct GPU hardware-frame interoperability"
            }
            Self::UnboundedEncoderQueue => {
                "no candidate exposes an enforceable finite encoder depth"
            }
        })
    }
}

impl std::error::Error for UnsupportedReason {}

pub fn plan_candidates(
    capture_adapter_luid: u64,
    codecs: &[VideoCodec],
    capabilities: &[EncoderCapability],
) -> Result<Vec<CaptureCandidate>, UnsupportedReason> {
    let mut candidates = Vec::new();
    let mut saw_codec = false;
    let mut saw_same_adapter = false;
    let mut saw_direct = false;

    for encoder in [EncoderKind::Nvenc, EncoderKind::Amf, EncoderKind::Qsv] {
        for codec in codecs {
            for capability in capabilities
                .iter()
                .filter(|item| item.encoder == encoder && item.codec == *codec)
            {
                saw_codec = true;
                if capability.adapter_luid != capture_adapter_luid {
                    continue;
                }
                saw_same_adapter = true;
                if !capability.direct_hardware_frames {
                    continue;
                }
                saw_direct = true;
                let Some(encoder_depth) = capability.bounded_depth.filter(|depth| *depth > 0)
                else {
                    continue;
                };
                candidates.push(CaptureCandidate {
                    encoder,
                    codec: *codec,
                    adapter_luid: capture_adapter_luid,
                    interop: capability.interop,
                    encoder_depth,
                });
            }
        }
    }

    if !candidates.is_empty() {
        return Ok(candidates);
    }
    if !saw_codec {
        Err(UnsupportedReason::EncoderUnavailable)
    } else if !saw_same_adapter {
        Err(UnsupportedReason::CrossAdapterUnvalidated)
    } else if !saw_direct {
        Err(UnsupportedReason::DirectInteropUnavailable)
    } else {
        Err(UnsupportedReason::UnboundedEncoderQueue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURE_LUID: u64 = 0x1234_5678;

    fn capability(encoder: EncoderKind) -> EncoderCapability {
        EncoderCapability::compiled_candidate(encoder, VideoCodec::H264, CAPTURE_LUID)
    }

    #[test]
    fn each_vendor_boundary_uses_the_same_vendor_neutral_planner() {
        for (encoder, interop, depth) in [
            (EncoderKind::Nvenc, DirectInterop::D3d11Nvenc, 4),
            (EncoderKind::Amf, DirectInterop::D3d11Amf, 4),
            (EncoderKind::Qsv, DirectInterop::D3d11Qsv, 4),
        ] {
            let planned =
                plan_candidates(CAPTURE_LUID, &[VideoCodec::H264], &[capability(encoder)]).unwrap();
            assert_eq!(planned.len(), 1);
            assert_eq!(planned[0].encoder, encoder);
            assert_eq!(planned[0].interop, interop);
            assert_eq!(planned[0].encoder_depth, depth);
        }
    }

    #[test]
    fn ranking_is_stable_without_inspecting_a_vendor_id_or_gpu_name() {
        let planned = plan_candidates(
            CAPTURE_LUID,
            &[VideoCodec::H264],
            &[
                capability(EncoderKind::Qsv),
                capability(EncoderKind::Amf),
                capability(EncoderKind::Nvenc),
            ],
        )
        .unwrap();
        assert_eq!(
            planned
                .iter()
                .map(|candidate| candidate.encoder)
                .collect::<Vec<_>>(),
            vec![EncoderKind::Nvenc, EncoderKind::Amf, EncoderKind::Qsv]
        );
    }

    #[test]
    fn cross_adapter_candidate_is_not_selected() {
        let mut candidate = capability(EncoderKind::Nvenc);
        candidate.adapter_luid = CAPTURE_LUID + 1;
        assert_eq!(
            plan_candidates(CAPTURE_LUID, &[VideoCodec::H264], &[candidate]),
            Err(UnsupportedReason::CrossAdapterUnvalidated)
        );
    }

    #[test]
    fn host_copy_or_unbounded_candidate_is_not_optimized() {
        let mut copied = capability(EncoderKind::Amf);
        copied.direct_hardware_frames = false;
        assert_eq!(
            plan_candidates(CAPTURE_LUID, &[VideoCodec::H264], &[copied]),
            Err(UnsupportedReason::DirectInteropUnavailable)
        );

        let mut unbounded = capability(EncoderKind::Qsv);
        unbounded.bounded_depth = None;
        assert_eq!(
            plan_candidates(CAPTURE_LUID, &[VideoCodec::H264], &[unbounded]),
            Err(UnsupportedReason::UnboundedEncoderQueue)
        );
    }

    #[test]
    fn requested_codec_is_part_of_the_capability_contract() {
        assert_eq!(
            plan_candidates(
                CAPTURE_LUID,
                &[VideoCodec::Hevc],
                &[capability(EncoderKind::Nvenc)]
            ),
            Err(UnsupportedReason::EncoderUnavailable)
        );
    }
}
