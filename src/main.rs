use std::process::ExitCode;

fn main() -> ExitCode {
    // Recognized anywhere in argv, matching the actions' pane-identity read: a process
    // invoked with this flag never counts as the review UI, so it must never run the
    // review UI either (`specs/herdr-host.md` Pane identity). This dispatch and the jq
    // exclusion in `herdr/pane.sh` (`is_reviewr_pane`) are the two halves of that
    // contract — a future non-UI flag must land in both, or the actions will count its
    // transient process as a live reviewr pane.
    if std::env::args_os().skip(1).any(|arg| arg == "--resolve-plugin-config") {
        if let Err(error) = herdr_reviewr::config::print_plugin_config() {
            eprintln!("reviewr: {error}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    // Agent-facing and pane subcommands are selected by the first positional token. Every
    // other invocation (including a bare repo-path argument) falls through to the review UI.
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("comment" | "skill-install" | "skill-path") => {
            return herdr_reviewr::cli::run(&args[1..]);
        }
        Some("sidebar") => return herdr_reviewr::sidebar::run(&args[2..]),
        _ => {}
    }

    match herdr_reviewr::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("reviewr: {error:?}");
            ExitCode::FAILURE
        }
    }
}
