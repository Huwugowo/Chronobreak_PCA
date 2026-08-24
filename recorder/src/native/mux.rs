use std::ffi::{OsStr, OsString};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::encoder::{
    AudioSource, FRAGMENTED_MP4_FLAGS, append_windows_audio_arguments, push_args,
};

/// FFmpeg is retained only as the audio encoder and fragmented-MP4 muxer. The
/// native encoder writes Annex-B H.264 packets to stdin and closes the pipe for
/// bounded end-of-stream; FFmpeg must perform no video capture, filtering or
/// encoding on this path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMuxPlan {
    arguments: Vec<OsString>,
}

impl NativeMuxPlan {
    pub fn h264(frames_per_second: u32, audio: &AudioSource, output: &Path) -> Result<Self> {
        if frames_per_second == 0 || frames_per_second > 240 {
            bail!("native mux FPS must be in 1..=240");
        }
        if output.as_os_str().is_empty() {
            bail!("native mux output path is empty");
        }

        let mut arguments = Vec::new();
        push_args(
            &mut arguments,
            &[
                "-hide_banner",
                "-loglevel",
                "info",
                "-nostats",
                "-stats_period",
                "0.25",
                "-progress",
                "pipe:1",
                // Raw Annex-B packets do not carry MP4 timestamps. As an input
                // option, -r explicitly generates the configured CFR timeline.
                "-r",
            ],
        );
        arguments.push(frames_per_second.to_string().into());
        push_args(
            &mut arguments,
            &["-fflags", "+genpts", "-f", "h264", "-i", "pipe:0"],
        );

        append_windows_audio_arguments(&mut arguments, audio);
        push_args(
            &mut arguments,
            &[
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-shortest",
                "-flush_packets",
                "1",
                "-movflags",
                FRAGMENTED_MP4_FLAGS,
                "-y",
            ],
        );
        arguments.push(output.as_os_str().to_owned());

        let plan = Self { arguments };
        plan.validate_mux_only()?;
        Ok(plan)
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    fn validate_mux_only(&self) -> Result<()> {
        require_pair(&self.arguments, "-c:v", "copy")?;
        require_pair(&self.arguments, "-f", "h264")?;
        require_pair(&self.arguments, "-i", "pipe:0")?;

        const FORBIDDEN_EXACT: &[&str] = &[
            "-vf",
            "-filter_complex",
            "-filter_hw_device",
            "-init_hw_device",
            "-pix_fmt",
            "-b:v",
            "-maxrate",
            "-bufsize",
            "-g",
            "-bf",
            "-surfaces",
            "gdigrab",
            "desktop",
        ];
        const FORBIDDEN_SUBSTRINGS: &[&str] = &[
            "gfxcapture",
            "scale=",
            "scale_d3d11",
            "hwmap=",
            "h264_nvenc",
            "hevc_nvenc",
            "h264_amf",
            "h264_qsv",
        ];
        for argument in &self.arguments {
            let value = argument.to_string_lossy();
            if FORBIDDEN_EXACT.iter().any(|forbidden| value == *forbidden)
                || FORBIDDEN_SUBSTRINGS
                    .iter()
                    .any(|forbidden| value.contains(forbidden))
            {
                bail!("native mux plan contains forbidden video work argument {value:?}");
            }
        }
        Ok(())
    }
}

fn require_pair(arguments: &[OsString], option: &str, value: &str) -> Result<()> {
    arguments
        .windows(2)
        .any(|pair| pair[0] == OsStr::new(option) && pair[1] == OsStr::new(value))
        .then_some(())
        .with_context(|| format!("native mux plan is missing {option} {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(plan: &NativeMuxPlan) -> Vec<String> {
        plan.arguments()
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn h264_mux_plan_declares_cfr_streamcopy_and_fragmented_mp4() {
        let plan = NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("native.mp4"))
            .expect("valid mux plan");
        let arguments = strings(&plan);

        assert!(arguments.windows(2).any(|pair| pair == ["-r", "60"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-f", "h264"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-i", "pipe:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:v", "copy"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
        assert!(arguments.windows(2).any(|pair| pair == ["-map", "1:a:0"]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["-movflags", FRAGMENTED_MP4_FLAGS])
        );
        assert!(arguments.iter().any(|argument| argument == "-shortest"));
        assert_eq!(arguments.last().map(String::as_str), Some("native.mp4"));
    }

    #[test]
    fn mux_plan_rejects_every_capture_filter_and_video_encode_token() {
        let forbidden = [
            "-vf",
            "-filter_complex",
            "-filter_hw_device",
            "-init_hw_device",
            "-pix_fmt",
            "-b:v",
            "-g",
            "-surfaces",
            "gfxcapture=hwnd=1",
            "scale_d3d11=width=1920",
            "h264_nvenc",
            "h264_amf",
            "h264_qsv",
        ];
        for token in forbidden {
            let mut plan = NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("native.mp4"))
                .expect("valid baseline mux plan");
            plan.arguments.push(token.into());
            assert!(
                plan.validate_mux_only().is_err(),
                "forbidden token was accepted: {token}"
            );
        }
    }

    #[test]
    fn mux_plan_rejects_invalid_rate_and_empty_output() {
        assert!(NativeMuxPlan::h264(0, &AudioSource::Silent, Path::new("native.mp4")).is_err());
        assert!(NativeMuxPlan::h264(241, &AudioSource::Silent, Path::new("native.mp4")).is_err());
        assert!(NativeMuxPlan::h264(60, &AudioSource::Silent, Path::new("")).is_err());
    }
}

