//! `saw-measure`: a development tool for the parent issue's A1 and A3 runs.
//! It runs the core on the Windows backend with fixed settings and logs every
//! click applications received; `summary` reads a log back on any host.

mod args;
mod log;
#[cfg(windows)]
mod run;
mod summary;

use std::fs;
use std::process::ExitCode;

use args::Command;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let result = match args::parse(&argv) {
        Ok(command) => execute(command),
        Err(e) => Err(format!("{e}\n\n{}", args::USAGE)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("saw-measure: {e}");
            ExitCode::FAILURE
        }
    }
}

fn execute(command: Command) -> Result<(), String> {
    match command {
        Command::Summary { log } => print_summary(&log),
        #[cfg(windows)]
        Command::Run { info, log } => {
            run::run(info, &log)?;
            print_summary(&log)
        }
        #[cfg(not(windows))]
        Command::Run { .. } => Err("run needs Windows".into()),
    }
}

fn print_summary(path: &std::path::Path) -> Result<(), String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let log: log::Log = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    print!("{}", summary::Report(&log));
    Ok(())
}
