//! The settings window's editable copy of the settings, in the units it
//! shows: whole milliseconds and Jitter as a percentage (R1).

use std::time::Duration;

use saw_core::{Hotkey, Settings, SettingsError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draft {
    pub cps: u32,
    pub delay_ms: u32,
    pub jitter_percent: u32,
    pub hotkey: Hotkey,
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Draft {
        Draft {
            cps: settings.cps(),
            delay_ms: settings.activation_delay().as_millis() as u32,
            jitter_percent: (settings.jitter() * 100.0).round() as u32,
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
            hotkey: Hotkey {
                modifiers: Modifiers {
                    shift: true,
                    ..Modifiers::default()
                },
                key: Key::Digit7,
            },
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
    fn out_of_range_values_are_refused() {
        let mut draft = Draft::from(&Settings::default());
        draft.jitter_percent = 51;
        assert_eq!(draft.to_settings(), Err(SettingsError::Jitter(0.51)));
    }
}
