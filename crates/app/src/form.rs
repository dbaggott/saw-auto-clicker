//! The settings in the units the settings window and the settings file use:
//! whole milliseconds, and Jitter as a whole percentage (R1).

use std::ops::RangeInclusive;
use std::time::Duration;

use saw_core::{ACTIVATION_DELAY_RANGE, Hotkey, JITTER_RANGE, Settings, SettingsError};

/// Q2's range in whole milliseconds.
pub fn delay_ms_range() -> RangeInclusive<u32> {
    ACTIVATION_DELAY_RANGE.start().as_millis() as u32
        ..=ACTIVATION_DELAY_RANGE.end().as_millis() as u32
}

/// D5's range as a whole percentage.
pub fn jitter_percent_range() -> RangeInclusive<u32> {
    percent(*JITTER_RANGE.start())..=percent(*JITTER_RANGE.end())
}

fn percent(fraction: f64) -> u32 {
    (fraction * 100.0).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draft {
    pub cps: u32,
    pub delay_ms: u32,
    pub jitter_percent: u32,
    pub hotkey: Option<Hotkey>,
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Draft {
        Draft {
            cps: settings.cps(),
            delay_ms: settings.activation_delay().as_millis() as u32,
            jitter_percent: percent(settings.jitter()),
            hotkey: settings.hotkey(),
        }
    }
}

impl Draft {
    pub fn to_settings(self) -> Result<Settings, SettingsError> {
        Settings::new(
            self.cps,
            Duration::from_millis(u64::from(self.delay_ms)),
            f64::from(self.jitter_percent) / 100.0,
            self.hotkey,
        )
    }
}

#[cfg(test)]
mod tests {
    use saw_core::{Key, Modifiers};

    use super::*;

    #[test]
    fn a_draft_round_trips_through_settings() {
        let draft = Draft {
            cps: 50,
            delay_ms: 2000,
            jitter_percent: 50,
            hotkey: Some(Hotkey {
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
                key: Key::Digit7,
            }),
        };
        assert_eq!(Draft::from(&draft.to_settings().unwrap()), draft);
        assert_eq!(
            Draft::from(&Settings::default()).to_settings(),
            Ok(Settings::default())
        );
    }

    #[test]
    fn jitter_percent_maps_to_a_fraction() {
        let mut draft = Draft::from(&Settings::default());
        draft.jitter_percent = 30;
        assert_eq!(draft.to_settings().unwrap().jitter(), 0.3);
    }

    #[test]
    fn the_whole_unit_ranges_match_q2_and_d5() {
        assert_eq!(delay_ms_range(), 50..=2000);
        assert_eq!(jitter_percent_range(), 0..=50);
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let mut draft = Draft::from(&Settings::default());
        draft.jitter_percent = 51;
        assert_eq!(draft.to_settings(), Err(SettingsError::Jitter(0.51)));
    }
}
