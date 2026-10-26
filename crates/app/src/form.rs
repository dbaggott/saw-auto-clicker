//! The settings in the units the settings window and the settings file use:
//! whole milliseconds for the activation delay (R1).

use std::ops::RangeInclusive;
use std::time::Duration;

use saw_core::{
    ACTIVATION_DELAY_RANGE, BUTTERFLY_CPS, ClickButtons, Hotkey, Settings, SettingsError,
};

/// Q2's range in whole milliseconds.
pub fn delay_ms_range() -> RangeInclusive<u32> {
    ACTIVATION_DELAY_RANGE.start().as_millis() as u32
        ..=ACTIVATION_DELAY_RANGE.end().as_millis() as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draft {
    pub cps: u32,
    pub delay_ms: u32,
    pub jitter: bool,
    pub buttons: ClickButtons,
    pub hotkey: Option<Hotkey>,
}

impl From<&Settings> for Draft {
    fn from(settings: &Settings) -> Draft {
        Draft {
            cps: settings.cps(),
            delay_ms: settings.activation_delay().as_millis() as u32,
            jitter: settings.jitter(),
            buttons: settings.buttons(),
            hotkey: settings.hotkey(),
        }
    }
}

impl Draft {
    pub fn to_settings(self) -> Result<Settings, SettingsError> {
        Settings::new(
            self.cps,
            Duration::from_millis(u64::from(self.delay_ms)),
            self.jitter,
            self.buttons,
            self.hotkey,
        )
    }

    /// Whether the clicking matches the butterfly preset. Not stored: any
    /// change to the CPS or Jitter takes it off, and setting them back puts
    /// it on again.
    pub fn butterfly(&self) -> bool {
        self.jitter && self.cps == BUTTERFLY_CPS
    }

    pub fn set_butterfly(&mut self) {
        self.jitter = true;
        self.cps = BUTTERFLY_CPS;
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
            jitter: true,
            buttons: ClickButtons::Right,
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
    fn the_whole_unit_range_matches_q2() {
        assert_eq!(delay_ms_range(), 50..=2000);
    }

    #[test]
    fn out_of_range_values_are_refused() {
        let mut draft = Draft::from(&Settings::default());
        draft.cps = 51;
        assert_eq!(draft.to_settings(), Err(SettingsError::Cps(51)));
    }

    #[test]
    fn butterfly_is_its_cps_with_jitter_and_nothing_else() {
        let mut draft = Draft::from(&Settings::default());
        assert!(!draft.butterfly());
        draft.set_butterfly();
        assert!(draft.butterfly());
        assert_eq!((draft.cps, draft.jitter), (BUTTERFLY_CPS, true));

        draft.cps -= 1;
        assert!(!draft.butterfly());
        draft.cps += 1;
        draft.jitter = false;
        assert!(!draft.butterfly());
    }
}
