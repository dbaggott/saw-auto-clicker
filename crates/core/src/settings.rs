use std::fmt;
use std::ops::RangeInclusive;
use std::time::Duration;

use crate::gate::Button;

/// Supported clicks per second (Q1).
pub const CPS_RANGE: RangeInclusive<u32> = 1..=50;
/// Supported activation delays (Q2).
pub const ACTIVATION_DELAY_RANGE: RangeInclusive<Duration> =
    Duration::from_millis(50)..=Duration::from_millis(2000);
/// The fastest clicking a person reaches by butterfly clicking, alternating
/// two fingers on one button. The settings window's butterfly preset sets it,
/// with Jitter on.
pub const BUTTERFLY_CPS: u32 = 25;

/// The user's settings (R1), shared by every button that auto-clicks.
/// Construction rejects values outside Q1–Q2's ranges, so a `Settings` is
/// always in range.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    cps: u32,
    activation_delay: Duration,
    jitter: bool,
    buttons: ClickButtons,
    hotkey: Option<Hotkey>,
}

impl Settings {
    pub fn new(
        cps: u32,
        activation_delay: Duration,
        jitter: bool,
        buttons: ClickButtons,
        hotkey: Option<Hotkey>,
    ) -> Result<Settings, SettingsError> {
        if !CPS_RANGE.contains(&cps) {
            return Err(SettingsError::Cps(cps));
        }
        if !ACTIVATION_DELAY_RANGE.contains(&activation_delay) {
            return Err(SettingsError::ActivationDelay(activation_delay));
        }
        Ok(Settings {
            cps,
            activation_delay,
            jitter,
            buttons,
            hotkey,
        })
    }

    /// These settings with `hotkey` in place of their own.
    pub fn with_hotkey(self, hotkey: Option<Hotkey>) -> Settings {
        Settings { hotkey, ..self }
    }

    pub fn cps(&self) -> u32 {
        self.cps
    }

    pub fn activation_delay(&self) -> Duration {
        self.activation_delay
    }

    /// Whether click timing varies as a person's does (D5).
    pub fn jitter(&self) -> bool {
        self.jitter
    }

    pub fn buttons(&self) -> ClickButtons {
        self.buttons
    }

    /// The toggle hotkey, or `None` when the user has not chosen one (Q3).
    pub fn hotkey(&self) -> Option<Hotkey> {
        self.hotkey
    }

    /// The nominal click interval T, one second over the CPS.
    pub fn interval(&self) -> Duration {
        Duration::from_secs(1) / self.cps
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            cps: 10,
            activation_delay: Duration::from_millis(250),
            jitter: false,
            buttons: ClickButtons::Both,
            hotkey: None,
        }
    }
}

/// Which mouse buttons auto-click when held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClickButtons {
    Left,
    Both,
    Right,
}

impl ClickButtons {
    pub fn includes(self, button: Button) -> bool {
        match self {
            ClickButtons::Both => true,
            ClickButtons::Left => button == Button::Left,
            ClickButtons::Right => button == Button::Right,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsError {
    Cps(u32),
    ActivationDelay(Duration),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Cps(cps) => write!(
                f,
                "clicks per second must be {}–{}, got {cps}",
                CPS_RANGE.start(),
                CPS_RANGE.end()
            ),
            SettingsError::ActivationDelay(delay) => write!(
                f,
                "activation delay must be {}–{} ms, got {} ms",
                ACTIVATION_DELAY_RANGE.start().as_millis(),
                ACTIVATION_DELAY_RANGE.end().as_millis(),
                delay.as_millis()
            ),
        }
    }
}

impl std::error::Error for SettingsError {}

/// The toggle hotkey as a platform-neutral description, which a backend
/// translates to its own key codes when it registers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub modifiers: Modifiers,
    pub key: Key,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The Windows key on Windows, Command on macOS, Super on Linux.
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn defaults_match_q1_to_q3() {
        let s = Settings::default();
        assert_eq!(s.cps(), 10);
        assert_eq!(s.activation_delay(), ms(250));
        assert!(!s.jitter());
        assert_eq!(s.buttons(), ClickButtons::Both);
        assert_eq!(s.hotkey(), None);
        assert_eq!(s.interval(), ms(100));
    }

    #[test]
    fn range_ends_are_accepted() {
        for (cps, delay) in [(1, 50), (50, 2000)] {
            let s = Settings::new(cps, ms(delay), true, ClickButtons::Left, None).unwrap();
            assert_eq!((s.cps(), s.activation_delay()), (cps, ms(delay)));
        }
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let new = |cps, delay| Settings::new(cps, ms(delay), false, ClickButtons::Both, None);
        assert_eq!(new(0, 250), Err(SettingsError::Cps(0)));
        assert_eq!(new(51, 250), Err(SettingsError::Cps(51)));
        assert_eq!(new(10, 49), Err(SettingsError::ActivationDelay(ms(49))));
        assert_eq!(new(10, 2001), Err(SettingsError::ActivationDelay(ms(2001))));
    }

    #[test]
    fn click_buttons_include_their_buttons() {
        use Button::{Left, Right};
        assert!(ClickButtons::Left.includes(Left) && !ClickButtons::Left.includes(Right));
        assert!(ClickButtons::Right.includes(Right) && !ClickButtons::Right.includes(Left));
        assert!(ClickButtons::Both.includes(Left) && ClickButtons::Both.includes(Right));
    }

    #[test]
    fn the_butterfly_cps_is_in_range() {
        assert!(CPS_RANGE.contains(&BUTTERFLY_CPS));
    }
}
