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
pub const BUTTERFLY_CPS: u32 = 15;

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
    apps: Apps,
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
            apps: Apps::default(),
        })
    }

    /// These settings with `hotkey` in place of their own.
    pub fn with_hotkey(self, hotkey: Option<Hotkey>) -> Settings {
        Settings { hotkey, ..self }
    }

    /// These settings with `apps` in place of their own.
    pub fn with_apps(self, apps: Apps) -> Settings {
        Settings { apps, ..self }
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

    /// The apps that auto-click (R21). Empty, the default, means none do.
    pub fn apps(&self) -> &Apps {
        &self.apps
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
            apps: Apps::default(),
        }
    }
}

/// The apps auto-clicking happens in (R21), each named by its executable's
/// file name, such as `game.exe`. Names match ignoring case, over all of
/// Unicode, and the list never holds two that match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Apps(Vec<String>);

impl Apps {
    /// Adds `name`, without surrounding whitespace, unless the list already
    /// holds it in any case, and says whether it did.
    pub fn add(&mut self, name: &str) -> Result<bool, AppNameError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppNameError::Empty);
        }
        if name.contains(['/', '\\']) {
            return Err(AppNameError::Path(name.to_string()));
        }
        if self.contains(name) {
            return Ok(false);
        }
        self.0.push(name.to_string());
        Ok(true)
    }

    /// Removes `name`, in any case.
    pub fn remove(&mut self, name: &str) {
        let name = name.to_lowercase();
        self.0.retain(|n| n.to_lowercase() != name);
    }

    pub fn contains(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        self.0.iter().any(|n| n.to_lowercase() == name)
    }

    /// The names in the order they were added, each as first written.
    pub fn names(&self) -> &[String] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Why a name can't be one of the [`Apps`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppNameError {
    Empty,
    /// A path, where only the file name is matched.
    Path(String),
}

impl fmt::Display for AppNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppNameError::Empty => f.write_str("an app's name can't be empty"),
            AppNameError::Path(name) => write!(
                f,
                "an app is named by its file name alone, such as \"app.exe\", got \"{name}\""
            ),
        }
    }
}

impl std::error::Error for AppNameError {}

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
        assert!(s.apps().is_empty());
        assert_eq!(s.interval(), ms(100));
    }

    #[test]
    fn apps_match_and_collapse_ignoring_case_beyond_ascii() {
        let mut apps = Apps::default();
        assert_eq!(apps.add("Spiel.exe"), Ok(true));
        assert_eq!(apps.add("ÉCLAIR.EXE"), Ok(true));
        assert_eq!(apps.add("spiel.EXE"), Ok(false));
        assert_eq!(apps.add("éclair.exe"), Ok(false));
        assert_eq!(apps.add(" spiel.exe\t"), Ok(false));
        assert_eq!(apps.add(" other.exe "), Ok(true));
        assert_eq!(apps.names(), ["Spiel.exe", "ÉCLAIR.EXE", "other.exe"]);
        apps.remove("other.exe");
        assert!(apps.contains("SPIEL.exe") && apps.contains("Éclair.exe"));
        assert!(!apps.contains("spiel") && !apps.contains("other.exe"));
        apps.remove("éclair.EXE");
        assert_eq!(apps.names(), ["Spiel.exe"]);
    }

    #[test]
    fn an_empty_name_or_a_path_is_not_an_app() {
        let mut apps = Apps::default();
        assert_eq!(apps.add(""), Err(AppNameError::Empty));
        assert_eq!(apps.add("  "), Err(AppNameError::Empty));
        for path in ["C:\\Games\\app.exe", "bin/app.exe"] {
            assert_eq!(apps.add(path), Err(AppNameError::Path(path.into())));
        }
        assert!(apps.is_empty());
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
