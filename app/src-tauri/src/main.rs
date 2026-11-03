// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--prepare-library") {
        if let Err(error) = league_replay_app_lib::prepare_library_headless(&args) {
            eprintln!("Chronobreak preparation: {error}");
            std::process::exit(1);
        }
        return;
    }
    league_replay_app_lib::run()
}
