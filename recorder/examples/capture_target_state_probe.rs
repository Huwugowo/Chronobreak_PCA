#[cfg(target_os = "windows")]
mod windows_probe {
    use std::thread;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result, bail, ensure};
    use league_replay_recorder::platform::{
        CaptureTargetStateCache, CaptureTargetVisibility, capture_target_for_process,
        query_capture_target_state,
    };

    pub fn run() -> Result<()> {
        let mut args = std::env::args().skip(1);
        let mut pid = None;
        let mut duration_seconds = 10_u64;
        let mut interval_ms = 100_u64;
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--pid" => {
                    pid = Some(
                        args.next()
                            .context("--pid requires a value")?
                            .parse::<u32>()
                            .context("--pid must be an integer")?,
                    );
                }
                "--duration-seconds" => {
                    duration_seconds = args
                        .next()
                        .context("--duration-seconds requires a value")?
                        .parse::<u64>()
                        .context("--duration-seconds must be an integer")?;
                }
                "--interval-ms" => {
                    interval_ms = args
                        .next()
                        .context("--interval-ms requires a value")?
                        .parse::<u64>()
                        .context("--interval-ms must be an integer")?;
                }
                other => bail!("unknown argument {other:?}"),
            }
        }
        let pid = pid.context("--pid is required")?;
        ensure!(duration_seconds > 0, "duration must be positive");
        ensure!(interval_ms > 0, "interval must be positive");

        let target = capture_target_for_process(pid)
            .with_context(|| format!("could not resolve exact HWND for fixture PID {pid}"))?;
        println!("CHRONOBREAK_TARGET_STATE_STARTED {}", target.description());
        let mut cache = CaptureTargetStateCache::default();
        let mut queries = 0_u64;
        let mut adapter_validations = 0_u64;
        let mut visible = 0_u64;
        let mut paused = 0_u64;
        let deadline = Instant::now() + Duration::from_secs(duration_seconds);
        while Instant::now() < deadline {
            let state = query_capture_target_state(&target, &mut cache)
                .context("capture target state query failed")?;
            queries = queries.saturating_add(1);
            adapter_validations =
                adapter_validations.saturating_add(u64::from(state.adapter_validated));
            match state.visibility {
                CaptureTargetVisibility::Visible => visible = visible.saturating_add(1),
                CaptureTargetVisibility::PausedByWindowVisibility => {
                    paused = paused.saturating_add(1);
                }
            }
            thread::sleep(Duration::from_millis(interval_ms));
        }

        ensure!(queries > 0, "target-state probe issued no query");
        println!(
            "CHRONOBREAK_TARGET_STATE_PASS queries={queries} visible={visible} paused={paused} adapter_validations={adapter_validations}"
        );
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    windows_probe::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("capture_target_state_probe is Windows-only");
}
