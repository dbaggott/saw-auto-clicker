use std::path::PathBuf;

use crate::log::RunInfo;

pub const USAGE: &str = "\
usage:
  saw-measure run --cps <1-50> [--jitter <0-50>] [--delay <50-2000>] [--log <file>]
                  [--no-execution-speed-opt-out] [--no-timer-resolution-opt-out]
      Arms at once and logs every click applications receive until Ctrl+C,
      then prints the summary. Jitter is a percentage, the delay is in ms.
      The log defaults to saw-measure.log in the current directory.
  saw-measure summary <file>
      Prints the mean rate and interval spread of each hold in a log.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run { info: RunInfo, log: PathBuf },
    Summary { log: PathBuf },
}

/// Parses everything after the program name. Ranges are left to
/// `saw_core::Settings`, which owns them.
pub fn parse(args: &[String]) -> Result<Command, String> {
    let (command, rest) = args.split_first().ok_or("no command given")?;
    match command.as_str() {
        "summary" => match rest {
            [log] => Ok(Command::Summary { log: log.into() }),
            _ => Err("summary takes exactly one log file".into()),
        },
        "run" => parse_run(rest),
        other => Err(format!("unknown command {other:?}")),
    }
}

fn parse_run(args: &[String]) -> Result<Command, String> {
    let mut cps = None;
    let mut info = RunInfo {
        cps: 0,
        jitter_percent: 0,
        delay_ms: 250,
        execution_speed_opt_out: true,
        timer_resolution_opt_out: true,
    };
    let mut log = PathBuf::from("saw-measure.log");
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| format!("{flag} needs a value"))
                .cloned()
        };
        fn number<T: std::str::FromStr>(flag: &str, value: String) -> Result<T, String> {
            value
                .parse()
                .map_err(|_| format!("{flag} takes a whole number, got {value:?}"))
        }
        match flag.as_str() {
            "--cps" => cps = Some(number(flag, value()?)?),
            "--jitter" => info.jitter_percent = number(flag, value()?)?,
            "--delay" => info.delay_ms = number(flag, value()?)?,
            "--log" => log = value()?.into(),
            "--no-execution-speed-opt-out" => info.execution_speed_opt_out = false,
            "--no-timer-resolution-opt-out" => info.timer_resolution_opt_out = false,
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    info.cps = cps.ok_or("run needs --cps")?;
    Ok(Command::Run { info, log })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn run_defaults_keep_both_opt_outs_on() {
        assert_eq!(
            parse(&args("run --cps 20")),
            Ok(Command::Run {
                info: RunInfo {
                    cps: 20,
                    jitter_percent: 0,
                    delay_ms: 250,
                    execution_speed_opt_out: true,
                    timer_resolution_opt_out: true,
                },
                log: "saw-measure.log".into(),
            })
        );
    }

    #[test]
    fn run_takes_every_option() {
        assert_eq!(
            parse(&args(
                "run --jitter 30 --cps 50 --delay 300 --log a.log --no-execution-speed-opt-out --no-timer-resolution-opt-out"
            )),
            Ok(Command::Run {
                info: RunInfo {
                    cps: 50,
                    jitter_percent: 30,
                    delay_ms: 300,
                    execution_speed_opt_out: false,
                    timer_resolution_opt_out: false,
                },
                log: "a.log".into(),
            })
        );
    }

    #[test]
    fn summary_takes_one_file() {
        assert_eq!(
            parse(&args("summary a.log")),
            Ok(Command::Summary {
                log: "a.log".into()
            })
        );
        assert!(parse(&args("summary")).is_err());
    }

    #[test]
    fn mistakes_are_reported() {
        assert_eq!(parse(&args("run")), Err("run needs --cps".into()));
        assert_eq!(parse(&args("run --cps")), Err("--cps needs a value".into()));
        assert_eq!(
            parse(&args("run --cps fast")),
            Err("--cps takes a whole number, got \"fast\"".into())
        );
        assert!(parse(&args("run --cps 10 --turbo")).is_err());
        assert!(parse(&args("measure")).is_err());
        assert!(parse(&[]).is_err());
    }
}
