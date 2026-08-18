use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use league_replay_recorder::watcher::ProcessWatcher;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut refreshes = 500_u64;
    let mut target_name = "process-that-does-not-exist.exe".to_owned();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--refreshes" => {
                refreshes = args
                    .next()
                    .context("--refreshes requires a value")?
                    .parse::<u64>()
                    .context("--refreshes must be an integer")?;
            }
            "--target-name" => {
                target_name = args.next().context("--target-name requires a value")?;
            }
            other => bail!("unknown argument {other:?}"),
        }
    }
    ensure!(refreshes > 0, "refresh count must be positive");

    let mut watcher = ProcessWatcher::new(target_name);
    watcher.refresh();
    let started_at = Instant::now();
    let mut matches = 0_u64;
    for _ in 0..refreshes {
        matches = matches.saturating_add(u64::from(watcher.refresh().is_some()));
    }
    let elapsed = started_at.elapsed();
    let telemetry = watcher.telemetry();
    println!(
        "CHRONOBREAK_PROCESS_WATCHER_PASS measured_refreshes={refreshes} elapsed_100ns={} average_100ns={} matches={matches} known_processes={} maximum_known_processes={}",
        elapsed.as_nanos() / 100,
        (elapsed.as_nanos() / 100) / u128::from(refreshes),
        telemetry.known_processes,
        telemetry.maximum_known_processes,
    );
    Ok(())
}
