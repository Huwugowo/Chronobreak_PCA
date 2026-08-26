#[cfg(target_os = "windows")]
mod windows_probe {
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail};
    use league_replay_recorder::native::{NativeWgcSource, NvencDriverProbe};
    use league_replay_recorder::platform::capture_target_for_process;

    pub fn run() -> Result<()> {
        let mut args = std::env::args().skip(1);
        let mut pid = None;
        let mut duration = Duration::from_secs(10);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--pid" => {
                    pid = Some(
                        args.next()
                            .context("--pid requires a value")?
                            .parse::<u32>()
                            .context("--pid must be an integer")?,
                    );
                }
                "--duration-seconds" => {
                    duration = Duration::from_secs(
                        args.next()
                            .context("--duration-seconds requires a value")?
                            .parse::<u64>()
                            .context("--duration-seconds must be an integer")?,
                    );
                }
                other => bail!("unknown argument {other:?}"),
            }
        }
        let pid = pid.context("--pid is required")?;

        let target = capture_target_for_process(pid)
            .with_context(|| format!("could not resolve exact HWND for fixture PID {pid}"))?;
        let mut source = NativeWgcSource::start(&target)?;
        let nvenc = NvencDriverProbe::load()?;
        let h264 = nvenc.probe_h264_on_source(&source)?;
        let mut converter = source.create_nv12_converter(1920, 1080, 60)?;

        println!(
            "CHRONOBREAK_NATIVE_D3D11_READY adapter_index={} adapter_luid={:016x} adapter_name={:?} feature_level=0x{:x}",
            source.adapter_index(),
            source.adapter_luid(),
            source.adapter_name(),
            source.feature_level().0,
        );
        println!(
            "CHRONOBREAK_NATIVE_NVENC_DRIVER max_api={}.{}",
            nvenc.version().major,
            nvenc.version().minor
        );
        println!(
            "CHRONOBREAK_NATIVE_NVENC_H264_READY max_width={} max_height={} max_macroblocks_per_frame={} max_macroblocks_per_second={} async={}",
            h264.max_width,
            h264.max_height,
            h264.max_macroblocks_per_frame,
            h264.max_macroblocks_per_second,
            h264.async_encode_supported,
        );

        let deadline = Instant::now() + duration;
        let mut consumed = 0_u64;
        let mut first_qpc = None;
        let mut latest_qpc = None;
        while Instant::now() < deadline {
            if let Some(frame) = source.recv_timeout(Duration::from_millis(250))? {
                let (width, height) = frame.dimensions();
                if (width, height) != source.pool_dimensions() {
                    frame.close()?;
                    source.recreate_for_content_size(width, height)?;
                    converter.reconfigure_input(width, height)?;
                    continue;
                }
                let desc = frame.texture_desc()?;
                if desc.Width == 0 || desc.Height == 0 {
                    bail!("native WGC returned an empty D3D11 texture");
                }
                let qpc = frame.qpc_100ns();
                if latest_qpc.is_some_and(|last| qpc <= last) {
                    bail!("native WGC SystemRelativeTime/QPC did not advance monotonically");
                }
                first_qpc.get_or_insert(qpc);
                latest_qpc = Some(qpc);
                let converted = converter
                    .convert(&frame)?
                    .context("fixed native NV12 ring unexpectedly had no free slot")?;
                let nv12_desc = converted.texture_desc();
                if nv12_desc.Width != 1920 || nv12_desc.Height != 1080 {
                    bail!(
                        "native converter produced {}x{} instead of 1920x1080",
                        nv12_desc.Width,
                        nv12_desc.Height
                    );
                }
                if converted.qpc_100ns() != qpc {
                    bail!("native NV12 slot lost the WGC source timestamp");
                }
                converted.release();
                consumed = consumed.saturating_add(1);
                frame.close()?;
            }
            if source.telemetry().closed {
                break;
            }
        }
        let telemetry = source.telemetry();
        if consumed == 0 {
            bail!("native WGC source probe consumed no frames");
        }
        if telemetry.callback_errors != 0 {
            bail!(
                "native WGC callback recorded {} errors; first={:?}",
                telemetry.callback_errors,
                telemetry.first_callback_error,
            );
        }
        let conversion = converter.telemetry();
        if conversion.slot_texture_allocations != 4 || converter.free_slot_count() != 4 {
            bail!(
                "native NV12 ring violated fixed-slot contract: allocations={} free={}",
                conversion.slot_texture_allocations,
                converter.free_slot_count()
            );
        }
        if conversion.converted_frames != consumed
            || conversion.no_free_slot_admission_failures != 0
        {
            bail!(
                "native NV12 conversion accounting mismatch: converted={} consumed={} slot_drops={}",
                conversion.converted_frames,
                consumed,
                conversion.no_free_slot_admission_failures
            );
        }
        println!(
            "CHRONOBREAK_NATIVE_WGC_PASS consumed={} arrivals={} admitted={} handoff_drops={} recreations={} first_arrival_qpc={} latest_arrival_qpc={} first_accepted_qpc={} latest_accepted_qpc={} closed={}",
            consumed,
            telemetry.arrivals,
            telemetry.admitted,
            telemetry.handoff_drops,
            telemetry.recreations,
            telemetry.first_arrival_qpc_100ns.unwrap_or_default(),
            telemetry.latest_arrival_qpc_100ns.unwrap_or_default(),
            telemetry.first_accepted_qpc_100ns.unwrap_or_default(),
            telemetry.latest_accepted_qpc_100ns.unwrap_or_default(),
            telemetry.closed,
        );
        println!(
            "CHRONOBREAK_NATIVE_NV12_PASS converted={} slot_texture_allocations={} free_slots={} input_view_creations={} input_view_replacements={} input_view_cache_resets={} processor_recreations={} output_view_recreations={} no_free_slot_admission_failures={}",
            conversion.converted_frames,
            conversion.slot_texture_allocations,
            converter.free_slot_count(),
            conversion.input_view_creations,
            conversion.input_view_replacements,
            conversion.input_view_cache_resets,
            conversion.processor_recreations,
            conversion.output_view_recreations,
            conversion.no_free_slot_admission_failures,
        );
        source.close()?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_probe::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("native_wgc_source_probe is Windows-only");
}
