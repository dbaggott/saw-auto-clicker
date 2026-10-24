use std::fmt;
use std::ops::RangeInclusive;
use std::time::Duration;

/// Supported clicks per second (Q1).
pub const CPS_RANGE: RangeInclusive<u32> = 1..=50;
/// Supported activation delays (Q2).
pub const ACTIVATION_DELAY_RANGE: RangeInclusive<Duration> =
    Duration::from_millis(50)..=Duration::from_millis(2000);
/// Supported Jitter, as a fraction of the nominal interval (D5); the settings
/// window shows it as 0–50%.
pub const JITTER_RANGE: RangeInclusive<f64> = 0.0..=0.5;

/// The user's settings (R1), shared by both buttons. Construction rejects
/// values outside Q1–Q3's ranges, so a `Settings` is always in range.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    cps: u32,
    activation_delay: Duration,
    jitter: f64,
    hotkey: Hotkey,
}

impl Settings {
    pub fn new(
        cps: u32,
        activation_delay: Duration,
        jitter: f64,
        hotkey: Hotkey,
    ) -> Result<Settings, SettingsError> {
        if !CPS_RANGE.contains(&cps) {
            return Err(SettingsError::Cps(cps));
        }
        if !ACTIVATION_DELAY_RANGE.contains(&activation_delay) {
            return Err(SettingsError::ActivationDelay(activation_delay));
        }
        if !JITTER_RANGE.contains(&jitter) {
            return Err(SettingsError::Jitter(jitter));
        }
        Ok(Settings {
            cps,
            activation_delay,
            jitter,
            hotkey,
        })
    }

    pub fn cps(&self) -> u32 {
        self.cps
    }

    pub fn activation_delay(&self) -> Duration {
        self.activation_delay
    }

    pub fn jitter(&self) -> f64 {
        self.jitter
    }

    pub fn hotkey(&self) -> Hotkey {
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
            jitter: 0.0,
            hotkey: Hotkey::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsError {
    Cps(u32),
    ActivationDelay(Duration),
    Jitter(f64),
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
            SettingsError::Jitter(jitter) => write!(
                f,
                "Jitter must be {}–{}%, got {}%",
                JITTER_RANGE.start() * 100.0,
                JITTER_RANGE.end() * 100.0,
                jitter * 100.0
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

/// Ctrl+Alt+F8 (Q3).
impl Default for Hotkey {
    fn default() -> Hotkey {
        Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
            key: Key::F8,
        }
    }
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
        assert_eq!(s.jitter(), 0.0);
        assert_eq!(
            s.hotkey(),
            Hotkey {
                modifiers: Modifiers {
                    ctrl: true,
                    alt: true,
                    shift: false,
                    meta: false
                },
                key: Key::F8,
            }
        );
        assert_eq!(s.interval(), ms(100));
    }

    #[test]
    fn range_ends_are_accepted() {
        let hk = Hotkey::default();
        for (cps, delay, jitter) in [(1, 50, 0.0), (50, 2000, 0.5)] {
            let s = Settings::new(cps, ms(delay), jitter, hk).unwrap();
            assert_eq!(
                (s.cps(), s.activation_delay(), s.jitter()),
                (cps, ms(delay), jitter)
            );
        }
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let hk = Hotkey::default();
        assert_eq!(
            Settings::new(0, ms(250), 0.0, hk),
            Err(SettingsError::Cps(0))
        );
        assert_eq!(
            Settings::new(51, ms(250), 0.0, hk),
            Err(SettingsError::Cps(51))
        );
        assert_eq!(
            Settings::new(10, ms(49), 0.0, hk),
            Err(SettingsError::ActivationDelay(ms(49)))
        );
        assert_eq!(
            Settings::new(10, ms(2001), 0.0, hk),
            Err(SettingsError::ActivationDelay(ms(2001)))
        );
        assert_eq!(
            Settings::new(10, ms(250), -0.01, hk),
            Err(SettingsError::Jitter(-0.01))
        );
        assert_eq!(
            Settings::new(10, ms(250), 0.51, hk),
            Err(SettingsError::Jitter(0.51))
        );
        assert!(matches!(
            Settings::new(10, ms(250), f64::NAN, hk),
            Err(SettingsError::Jitter(_))
        ));
    }
}
