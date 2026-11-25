// Launch the desktop app without a console in every Windows build.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match chronobreak_recorder_control::release_identity::report(
        &args,
        option_env!("CHRONOBREAK_RELEASE_REVISION"),
        option_env!("CHRONOBREAK_RELEASE_VERSION"),
    ) {
        Ok(false) => {}
        Ok(true) => return,
        Err(_) => std::process::exit(1),
    }
    #[cfg(windows)]
    if let Err(error) =
        chronobreak_recorder_control::install_guard::Installation::protect_current_process()
    {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
    if args.first().is_some_and(|a| a == "--migrate-clip-tags") {
        if let Err(error) = league_replay_app_lib::migrate_clip_tags_headless(&args) {
            eprintln!("Chronobreak clip tag migration: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().is_some_and(|a| a == "--prepare-library") {
        if let Err(error) = league_replay_app_lib::prepare_library_headless(&args) {
            eprintln!("Chronobreak preparation: {error}");
            std::process::exit(1);
        }
        return;
    }
    league_replay_app_lib::run()
}
