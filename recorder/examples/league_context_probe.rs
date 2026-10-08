use std::process::ExitCode;

use league_replay_recorder::league_client::probe::{self, Options, Scenario};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("League context probe unavailable: {code}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), &'static str> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 || args[0] != "--seconds" || args[2] != "--scenario" {
        eprintln!(
            "Usage: league_context_probe --seconds <1..3600> --scenario <startup|mid_match|consecutive_games|recorder_restart|normal_closure|app_closed|practice|reconnect>"
        );
        return Err("arguments");
    }
    let options = Options {
        seconds: args[1].parse().map_err(|_| "duration")?,
        scenario: Scenario::parse(&args[3]).ok_or("scenario")?,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime")?;
    runtime.block_on(probe::run(options, &mut std::io::stdout().lock()))
}
