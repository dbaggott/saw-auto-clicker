//! The measurement log: a header line naming the run's settings, then one line
//! per physical press, physical release and passed click, timed in
//! microseconds from the start of the run.
//!
//! ```text
//! # saw-measure cps=50 jitter=0 delay_ms=250 execution_speed_opt_out=on timer_resolution_opt_out=on
//! 1250331 left press
//! 1500412 left click
//! ```

use std::fmt;
use std::str::FromStr;

use saw_core::Button;

const HEADER_PREFIX: &str = "# saw-measure";

/// What the run was configured with, so a log is self-describing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunInfo {
    pub cps: u32,
    pub jitter_percent: u32,
    pub delay_ms: u64,
    pub execution_speed_opt_out: bool,
    pub timer_resolution_opt_out: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A physical down, which starts a hold.
    Press,
    /// A physical up, which ends it.
    Release,
    /// A tagged down the gate passed: a click applications received.
    Click,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub at_us: u64,
    pub button: Button,
    pub kind: Kind,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Log {
    pub info: RunInfo,
    pub records: Vec<Record>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ParseError {}

pub fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

impl fmt::Display for RunInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{HEADER_PREFIX} cps={} jitter={} delay_ms={} execution_speed_opt_out={} timer_resolution_opt_out={}",
            self.cps,
            self.jitter_percent,
            self.delay_ms,
            on_off(self.execution_speed_opt_out),
            on_off(self.timer_resolution_opt_out),
        )
    }
}

impl FromStr for RunInfo {
    type Err = String;

    fn from_str(line: &str) -> Result<RunInfo, String> {
        let fields = line
            .strip_prefix(HEADER_PREFIX)
            .ok_or_else(|| format!("expected a header starting {HEADER_PREFIX:?}"))?;
        let mut info = RunInfo {
            cps: 0,
            jitter_percent: 0,
            delay_ms: 0,
            execution_speed_opt_out: false,
            timer_resolution_opt_out: false,
        };
        let mut seen = Vec::new();
        for field in fields.split_whitespace() {
            let (key, value) = field
                .split_once('=')
                .ok_or_else(|| format!("expected key=value, got {field:?}"))?;
            let bad = || format!("bad value for {key}: {value:?}");
            let flag = || match value {
                "on" => Ok(true),
                "off" => Ok(false),
                _ => Err(bad()),
            };
            match key {
                "cps" => info.cps = value.parse().map_err(|_| bad())?,
                "jitter" => info.jitter_percent = value.parse().map_err(|_| bad())?,
                "delay_ms" => info.delay_ms = value.parse().map_err(|_| bad())?,
                "execution_speed_opt_out" => info.execution_speed_opt_out = flag()?,
                "timer_resolution_opt_out" => info.timer_resolution_opt_out = flag()?,
                _ => return Err(format!("unknown field {key:?}")),
            }
            seen.push(key);
        }
        for key in [
            "cps",
            "jitter",
            "delay_ms",
            "execution_speed_opt_out",
            "timer_resolution_opt_out",
        ] {
            if !seen.contains(&key) {
                return Err(format!("header is missing {key}"));
            }
        }
        Ok(info)
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let button = match self.button {
            Button::Left => "left",
            Button::Right => "right",
        };
        let kind = match self.kind {
            Kind::Press => "press",
            Kind::Release => "release",
            Kind::Click => "click",
        };
        write!(f, "{} {button} {kind}", self.at_us)
    }
}

impl FromStr for Record {
    type Err = String;

    fn from_str(line: &str) -> Result<Record, String> {
        let mut words = line.split_whitespace();
        let (Some(at), Some(button), Some(kind), None) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(format!("expected \"<µs> <button> <kind>\", got {line:?}"));
        };
        Ok(Record {
            at_us: at.parse().map_err(|_| format!("bad time {at:?}"))?,
            button: match button {
                "left" => Button::Left,
                "right" => Button::Right,
                _ => return Err(format!("bad button {button:?}")),
            },
            kind: match kind {
                "press" => Kind::Press,
                "release" => Kind::Release,
                "click" => Kind::Click,
                _ => return Err(format!("bad kind {kind:?}")),
            },
        })
    }
}

impl FromStr for Log {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Log, ParseError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(i, line)| (i + 1, line.trim()))
            .filter(|(_, line)| !line.is_empty());
        let (line, header) = lines.next().ok_or(ParseError {
            line: 1,
            reason: "the log is empty".into(),
        })?;
        let info = header
            .parse()
            .map_err(|reason| ParseError { line, reason })?;
        let records = lines
            .map(|(line, text)| text.parse().map_err(|reason| ParseError { line, reason }))
            .collect::<Result<_, _>>()?;
        Ok(Log { info, records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> RunInfo {
        RunInfo {
            cps: 50,
            jitter_percent: 30,
            delay_ms: 250,
            execution_speed_opt_out: true,
            timer_resolution_opt_out: false,
        }
    }

    #[test]
    fn a_written_log_parses_back() {
        let records = [
            Record {
                at_us: 10,
                button: Button::Left,
                kind: Kind::Press,
            },
            Record {
                at_us: 260_000,
                button: Button::Left,
                kind: Kind::Click,
            },
            Record {
                at_us: 300_000,
                button: Button::Right,
                kind: Kind::Release,
            },
        ];
        let mut text = format!("{}\n", info());
        for r in &records {
            text += &format!("{r}\n");
        }
        assert_eq!(
            text.parse::<Log>(),
            Ok(Log {
                info: info(),
                records: records.to_vec()
            })
        );
    }

    #[test]
    fn the_header_names_every_setting() {
        assert_eq!(
            info().to_string(),
            "# saw-measure cps=50 jitter=30 delay_ms=250 execution_speed_opt_out=on timer_resolution_opt_out=off"
        );
    }

    #[test]
    fn errors_name_the_line() {
        let text = format!("{}\n10 left press\n\n20 middle click\n", info());
        assert_eq!(
            text.parse::<Log>().unwrap_err(),
            ParseError {
                line: 4,
                reason: "bad button \"middle\"".into()
            }
        );
    }

    #[test]
    fn a_header_missing_a_field_is_rejected() {
        let err = "# saw-measure cps=10 jitter=0 delay_ms=250 execution_speed_opt_out=on"
            .parse::<RunInfo>()
            .unwrap_err();
        assert!(err.contains("timer_resolution_opt_out"), "{err}");
    }

    #[test]
    fn a_log_without_a_header_is_rejected() {
        assert_eq!("10 left press".parse::<Log>().unwrap_err().line, 1);
    }
}
