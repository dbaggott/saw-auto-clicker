//! A1's numbers from a log: per hold, the mean click rate and how the
//! intervals between clicks spread.

use std::fmt;

use saw_core::Button;

use crate::log::{Kind, Log, on_off};

/// One press-to-release of one button.
#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    pub button: Button,
    pub pressed_us: u64,
    /// `None` when the log ends before the release.
    pub released_us: Option<u64>,
    pub clicks: Vec<u64>,
}

/// The spread of a hold's click-to-click intervals, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intervals {
    pub count: usize,
    pub mean_ms: f64,
    pub sd_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

impl Hold {
    /// None with fewer than two clicks, which give no interval.
    pub fn intervals(&self) -> Option<Intervals> {
        let gaps: Vec<f64> = self
            .clicks
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64 / 1000.0)
            .collect();
        if gaps.is_empty() {
            return None;
        }
        let n = gaps.len() as f64;
        let mean = gaps.iter().sum::<f64>() / n;
        let variance = gaps.iter().map(|g| (g - mean).powi(2)).sum::<f64>() / n;
        Some(Intervals {
            count: gaps.len(),
            mean_ms: mean,
            sd_ms: variance.sqrt(),
            min_ms: gaps.iter().copied().fold(f64::INFINITY, f64::min),
            max_ms: gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    /// Clicks per second from the first click to the last, so the activation
    /// delay before the burst doesn't dilute it.
    pub fn mean_rate(&self) -> Option<f64> {
        self.intervals().map(|i| 1000.0 / i.mean_ms)
    }
}

/// Splits the log into holds, in press order. A click outside any hold of its
/// button is not attributed to one.
pub fn holds(log: &Log) -> Vec<Hold> {
    let mut done = Vec::new();
    let mut open: [Option<Hold>; 2] = [None, None];
    let slot = |button| match button {
        Button::Left => 0,
        Button::Right => 1,
    };
    for record in &log.records {
        let current = &mut open[slot(record.button)];
        match record.kind {
            Kind::Press => {
                // A press with no release before it means the release went
                // unseen; the earlier hold ends where the log lost track.
                done.extend(current.take());
                *current = Some(Hold {
                    button: record.button,
                    pressed_us: record.at_us,
                    released_us: None,
                    clicks: Vec::new(),
                });
            }
            Kind::Release => {
                if let Some(mut hold) = current.take() {
                    hold.released_us = Some(record.at_us);
                    done.push(hold);
                }
            }
            Kind::Click => {
                if let Some(hold) = current {
                    hold.clicks.push(record.at_us);
                }
            }
        }
    }
    done.extend(open.into_iter().flatten());
    done.sort_by_key(|h| h.pressed_us);
    done
}

/// The report `saw-measure summary` prints.
pub struct Report<'a>(pub &'a Log);

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let info = &self.0.info;
        writeln!(
            f,
            "target {} CPS, Jitter {}%, delay {} ms, execution-speed opt-out {}, timer-resolution opt-out {}",
            info.cps,
            info.jitter_percent,
            info.delay_ms,
            on_off(info.execution_speed_opt_out),
            on_off(info.timer_resolution_opt_out),
        )?;
        let holds = holds(self.0);
        if holds.is_empty() {
            return writeln!(f, "no holds recorded");
        }
        for (n, hold) in holds.iter().enumerate() {
            let button = match hold.button {
                Button::Left => "left",
                Button::Right => "right",
            };
            let held = match hold.released_us {
                Some(released) => format!("{:.2} s", (released - hold.pressed_us) as f64 / 1e6),
                None => "unreleased".into(),
            };
            write!(f, "hold {} ({button}, {held}): ", n + 1)?;
            match (hold.mean_rate(), hold.intervals()) {
                (Some(rate), Some(i)) => {
                    let error = (rate / f64::from(info.cps) - 1.0) * 100.0;
                    writeln!(
                        f,
                        "{} clicks, mean rate {rate:.2} CPS ({error:+.1}% vs target); intervals mean {:.2} ms, sd {:.2} ms, min {:.2} ms, max {:.2} ms",
                        hold.clicks.len(),
                        i.mean_ms,
                        i.sd_ms,
                        i.min_ms,
                        i.max_ms
                    )?;
                }
                _ => writeln!(f, "{} clicks, too few for a rate", hold.clicks.len())?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::{Record, RunInfo};

    fn log(records: &[(u64, Button, Kind)]) -> Log {
        Log {
            info: RunInfo {
                cps: 10,
                jitter_percent: 0,
                delay_ms: 250,
                execution_speed_opt_out: true,
                timer_resolution_opt_out: true,
            },
            records: records
                .iter()
                .map(|&(at_us, button, kind)| Record {
                    at_us,
                    button,
                    kind,
                })
                .collect(),
        }
    }

    use Button::{Left, Right};
    use Kind::{Click, Press, Release};

    #[test]
    fn rate_is_measured_from_first_click_to_last() {
        let mut records = vec![(0, Left, Press)];
        // Four clicks 100 ms apart, starting after a 250 ms delay.
        records.extend((0..4).map(|k| (250_000 + k * 100_000, Left, Click)));
        records.push((700_000, Left, Release));
        let holds = holds(&log(&records));
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0].mean_rate(), Some(10.0));
        let i = holds[0].intervals().unwrap();
        assert_eq!((i.count, i.mean_ms, i.sd_ms), (3, 100.0, 0.0));
    }

    #[test]
    fn spread_reports_varied_intervals() {
        let records = [
            (0, Left, Press),
            (100_000, Left, Click),
            (180_000, Left, Click),
            (300_000, Left, Click),
            (400_000, Left, Release),
        ];
        let i = holds(&log(&records))[0].intervals().unwrap();
        assert_eq!((i.mean_ms, i.min_ms, i.max_ms), (100.0, 80.0, 120.0));
        assert_eq!(i.sd_ms, 20.0);
    }

    #[test]
    fn buttons_are_split_into_their_own_holds() {
        let records = [
            (0, Left, Press),
            (10, Right, Press),
            (20, Left, Click),
            (30, Right, Click),
            (40, Left, Click),
            (50, Right, Release),
            (60, Left, Release),
        ];
        let holds = holds(&log(&records));
        assert_eq!(holds.len(), 2);
        assert_eq!(
            (holds[0].button, holds[0].clicks.clone()),
            (Left, vec![20, 40])
        );
        assert_eq!(
            (holds[1].button, holds[1].clicks.clone()),
            (Right, vec![30])
        );
        assert_eq!(holds[1].mean_rate(), None);
    }

    #[test]
    fn a_hold_cut_off_by_the_log_or_a_new_press_still_counts() {
        let records = [
            (0, Left, Press),
            (10, Left, Click),
            (20, Left, Press),
            (30, Left, Click),
        ];
        let holds = holds(&log(&records));
        assert_eq!(holds.len(), 2);
        assert!(holds.iter().all(|h| h.released_us.is_none()));
    }

    #[test]
    fn stray_clicks_are_not_attributed() {
        let records = [(5, Left, Click), (10, Left, Press), (20, Left, Release)];
        assert!(holds(&log(&records))[0].clicks.is_empty());
    }

    #[test]
    fn the_report_shows_the_error_against_target() {
        let mut records = vec![(0, Left, Press)];
        records.extend((0..11).map(|k| (250_000 + k * 95_000, Left, Click)));
        records.push((1_400_000, Left, Release));
        let report = Report(&log(&records)).to_string();
        assert!(
            report.contains("11 clicks, mean rate 10.53 CPS (+5.3% vs target)"),
            "{report}"
        );
        assert!(report.starts_with("target 10 CPS, Jitter 0%"), "{report}");
    }
}
