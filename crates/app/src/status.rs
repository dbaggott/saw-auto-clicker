//! What the tray shows (R14, R16, R21), worked out from the core's reports.

use saw_core::{Button, ButtonEvent, Direction, Hotkey, Settings, Source, Verdict};

/// R16's message.
pub const ELEVATED_MESSAGE: &str = "Clicking is unavailable over elevated (administrator) windows.";
const HOTKEY_MISSING_MESSAGE: &str = "No hotkey is active; see Settings.";
const NO_APPS_MESSAGE: &str = "No applications allowed; see Settings.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    pub enabled: bool,
    /// Set when the core refuses or ends a burst over a window it can't
    /// click into, and cleared when the next burst starts normally.
    pub elevated_warning: bool,
    /// No toggle hotkey is registered, because the one in the settings was
    /// refused at startup (R15).
    pub hotkey_missing: bool,
    /// No app is chosen to auto-click in, so none does (R21).
    pub no_apps: bool,
}

/// Whether the settings name a hotkey that isn't registered, because it was
/// refused (R15). Having no hotkey at all is not a problem.
pub fn hotkey_missing(settings: &Settings, active: Option<Hotkey>) -> bool {
    settings.hotkey().is_some() && active.is_none()
}

impl TrayState {
    pub fn tooltip(&self) -> String {
        let state = if self.enabled { "enabled" } else { "disabled" };
        let mut text = format!("saw-auto-clicker: {state}");
        // With no apps chosen nothing clicks, elevated or not, and leaving
        // R16's message out keeps the tooltip within the tray's limit.
        for (shown, message) in [
            (self.no_apps, NO_APPS_MESSAGE),
            (self.elevated_warning && !self.no_apps, ELEVATED_MESSAGE),
            (self.hotkey_missing, HOTKEY_MISSING_MESSAGE),
        ] {
            if shown {
                text.push('\n');
                text.push_str(message);
            }
        }
        text
    }

    /// The label of the menu item that flips the enabled state.
    pub fn toggle_label(&self) -> &'static str {
        if self.enabled { "Disable" } else { "Enable" }
    }
}

/// The notification shown when the app starts enabled, as its title and
/// text, for a user who forgot it starts that way (R14). `apps` is how many
/// apps are chosen to auto-click in (R21); none are named.
pub fn started_enabled_notice(apps: usize) -> (&'static str, String) {
    let text = match apps {
        0 => "No applications are allowed yet, so it won't auto-click anywhere. \
              Add them in Settings, from the tray icon."
            .to_string(),
        1 => "It auto-clicks in the 1 allowed application.".to_string(),
        n => format!("It auto-clicks in the {n} allowed applications."),
    };
    ("saw-auto-clicker is running and enabled", text)
}

/// Spots, from the events the gate sees, the moment a burst starts: the
/// first of the app's own injections to pass after a physical press. That
/// is what clears R16's tooltip, since the core reports a refused burst but
/// not a started one.
#[derive(Debug, Default)]
pub struct BurstWatch {
    pressed_without_burst: [bool; 2],
}

impl BurstWatch {
    /// Whether `event` is the first injection of a burst.
    pub fn burst_started(&mut self, event: ButtonEvent, verdict: Verdict) -> bool {
        let pending = &mut self.pressed_without_burst[match event.button {
            Button::Left => 0,
            Button::Right => 1,
        }];
        match (event.source, event.direction) {
            (Source::Physical, Direction::Down) => {
                *pending = true;
                false
            }
            (Source::Physical, Direction::Up) => {
                *pending = false;
                false
            }
            (Source::Tagged, _) => verdict == Verdict::Pass && std::mem::take(pending),
        }
    }
}

#[cfg(test)]
mod tests {
    use saw_core::{Key, Modifiers};

    use super::*;

    fn event(button: Button, direction: Direction, source: Source) -> ButtonEvent {
        ButtonEvent {
            button,
            direction,
            source,
        }
    }

    #[test]
    fn the_tooltip_names_the_state_and_r16s_message_while_it_applies() {
        let mut state = TrayState::default();
        assert_eq!(state.tooltip(), "saw-auto-clicker: disabled");
        state.enabled = true;
        assert_eq!(state.tooltip(), "saw-auto-clicker: enabled");
        state.elevated_warning = true;
        assert_eq!(
            state.tooltip(),
            format!("saw-auto-clicker: enabled\n{ELEVATED_MESSAGE}")
        );
        state.hotkey_missing = true;
        assert_eq!(
            state.tooltip(),
            format!("saw-auto-clicker: enabled\n{ELEVATED_MESSAGE}\n{HOTKEY_MISSING_MESSAGE}")
        );
    }

    #[test]
    fn with_no_apps_the_tooltip_says_so_in_place_of_r16s_message() {
        let state = TrayState {
            enabled: true,
            elevated_warning: true,
            hotkey_missing: true,
            no_apps: true,
        };
        assert_eq!(
            state.tooltip(),
            format!("saw-auto-clicker: enabled\n{NO_APPS_MESSAGE}\n{HOTKEY_MISSING_MESSAGE}")
        );
    }

    #[test]
    fn only_a_chosen_hotkey_that_isnt_registered_is_missing() {
        let hotkey = Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            key: Key::F8,
        };
        let none = Settings::default();
        let chosen = none.clone().with_hotkey(Some(hotkey));
        assert!(!hotkey_missing(&none, None));
        assert!(hotkey_missing(&chosen, None));
        assert!(!hotkey_missing(&chosen, Some(hotkey)));
    }

    #[test]
    fn the_tooltip_fits_the_tray_limit() {
        // A notification-area tooltip holds 127 characters plus the NUL.
        for no_apps in [false, true] {
            let state = TrayState {
                enabled: false,
                elevated_warning: true,
                hotkey_missing: true,
                no_apps,
            };
            assert!(state.tooltip().encode_utf16().count() <= 127, "{no_apps}");
        }
    }

    #[test]
    fn the_started_enabled_notice_counts_the_apps_or_says_none_are_chosen() {
        let (title, none) = started_enabled_notice(0);
        assert!(none.contains("No applications are allowed") && none.contains("Settings"));
        assert_eq!(
            started_enabled_notice(1).1,
            "It auto-clicks in the 1 allowed application."
        );
        assert_eq!(
            started_enabled_notice(3).1,
            "It auto-clicks in the 3 allowed applications."
        );
        // NOTIFYICONDATAW holds 63 characters of title and 255 of text.
        assert!(title.encode_utf16().count() <= 63);
        assert!(none.encode_utf16().count() <= 255);
    }

    #[test]
    fn the_first_passed_injection_after_a_press_starts_a_burst() {
        use Direction::*;
        use Source::*;
        let mut watch = BurstWatch::default();
        let b = Button::Left;
        assert!(!watch.burst_started(event(b, Down, Physical), Verdict::Pass));
        assert!(watch.burst_started(event(b, Up, Tagged), Verdict::Pass));
        assert!(!watch.burst_started(event(b, Down, Tagged), Verdict::Pass));
        assert!(!watch.burst_started(event(b, Up, Tagged), Verdict::Pass));
        assert!(!watch.burst_started(event(b, Up, Physical), Verdict::Pass));
    }

    #[test]
    fn a_hold_with_no_burst_starts_none() {
        use Direction::*;
        use Source::*;
        let mut watch = BurstWatch::default();
        let b = Button::Right;
        watch.burst_started(event(b, Down, Physical), Verdict::Pass);
        watch.burst_started(event(b, Up, Physical), Verdict::Pass);
        // A late tagged event after the release, which the gate suppresses.
        assert!(!watch.burst_started(event(b, Down, Tagged), Verdict::Suppress));
        assert!(!watch.burst_started(event(b, Down, Tagged), Verdict::Pass));
    }

    #[test]
    fn buttons_are_tracked_apart() {
        use Direction::*;
        use Source::*;
        let mut watch = BurstWatch::default();
        watch.burst_started(event(Button::Left, Down, Physical), Verdict::Pass);
        assert!(!watch.burst_started(event(Button::Right, Up, Tagged), Verdict::Pass));
        assert!(watch.burst_started(event(Button::Left, Up, Tagged), Verdict::Pass));
    }
}
