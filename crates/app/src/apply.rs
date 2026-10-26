//! Putting new settings into effect (R14, R15): the hotkey is registered
//! first, and a refused hotkey leaves the previous one, or none, in effect
//! and in the settings that get saved.

use saw_core::{Hotkey, HotkeyError, RegisterHotkey, Settings};

use crate::hotkey_text::HotkeyText;

#[derive(Debug, PartialEq)]
pub struct Applied {
    /// What is now in effect, and what to save.
    pub settings: Settings,
    /// The hotkey registered now, if any.
    pub active_hotkey: Option<Hotkey>,
    /// R15's message, when the requested hotkey was refused.
    pub hotkey_problem: Option<String>,
}

/// Registers `requested`'s hotkey in place of `active_hotkey` and returns the
/// settings now in effect. `current` is what was in effect before; on a
/// refusal its hotkey is kept. At startup `current` and `requested` are both
/// the loaded settings and `active_hotkey` is `None`. A `requested` with no
/// hotkey unregisters the active one.
pub fn apply(
    current: &Settings,
    active_hotkey: Option<Hotkey>,
    requested: Settings,
    hotkeys: &mut impl RegisterHotkey,
) -> Applied {
    match hotkeys.register(requested.hotkey()) {
        Ok(()) => Applied {
            active_hotkey: requested.hotkey(),
            settings: requested,
            hotkey_problem: None,
        },
        Err(e) => Applied {
            hotkey_problem: Some(refusal(requested.hotkey(), &e, active_hotkey)),
            settings: requested.with_hotkey(current.hotkey()),
            active_hotkey,
        },
    }
}

fn refusal(requested: Option<Hotkey>, error: &HotkeyError, active: Option<Hotkey>) -> String {
    let what = match (requested.map(HotkeyText), error) {
        (Some(requested), HotkeyError::Taken) => format!("{requested} is in use by another app"),
        (Some(requested), HotkeyError::Failed(reason)) => {
            format!("{requested} could not be registered: {reason}")
        }
        (None, HotkeyError::Failed(reason)) => {
            format!("The hotkey could not be turned off: {reason}")
        }
        (None, e @ HotkeyError::Taken) => format!("The hotkey could not be turned off: {e}"),
    };
    match active {
        Some(active) => format!("{what}. The hotkey is still {}.", HotkeyText(active)),
        None => format!("{what}. No hotkey is active."),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use saw_core::{ClickButtons, Key, Modifiers};

    use super::*;

    /// Refuses every hotkey in `taken`, as if other programs held them.
    #[derive(Default)]
    struct FakeHotkeys {
        taken: Vec<Hotkey>,
        registered: Vec<Option<Hotkey>>,
    }

    impl RegisterHotkey for FakeHotkeys {
        fn register(&mut self, hotkey: Option<Hotkey>) -> Result<(), HotkeyError> {
            if hotkey.is_some_and(|h| self.taken.contains(&h)) {
                return Err(HotkeyError::Taken);
            }
            self.registered.push(hotkey);
            Ok(())
        }
    }

    fn ctrl_alt(key: Key) -> Hotkey {
        Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
            key,
        }
    }

    fn settings(cps: u32, hotkey: Hotkey) -> Settings {
        Settings::new(
            cps,
            Duration::from_millis(250),
            false,
            ClickButtons::Both,
            Some(hotkey),
        )
        .unwrap()
    }

    #[test]
    fn an_accepted_change_takes_effect_in_full() {
        let mut hotkeys = FakeHotkeys::default();
        let current = settings(10, ctrl_alt(Key::F8));
        let requested = settings(20, ctrl_alt(Key::F9));
        let applied = apply(
            &current,
            Some(ctrl_alt(Key::F8)),
            requested.clone(),
            &mut hotkeys,
        );
        assert_eq!(
            applied,
            Applied {
                settings: requested,
                active_hotkey: Some(ctrl_alt(Key::F9)),
                hotkey_problem: None,
            }
        );
        assert_eq!(hotkeys.registered, [Some(ctrl_alt(Key::F9))]);
    }

    #[test]
    fn no_hotkey_unregisters_the_active_one() {
        let mut hotkeys = FakeHotkeys::default();
        let current = settings(10, ctrl_alt(Key::F8));
        let requested = Settings::default();
        let applied = apply(
            &current,
            Some(ctrl_alt(Key::F8)),
            requested.clone(),
            &mut hotkeys,
        );
        assert_eq!(
            applied,
            Applied {
                settings: requested,
                active_hotkey: None,
                hotkey_problem: None,
            }
        );
        assert_eq!(hotkeys.registered, [None]);
    }

    #[test]
    fn a_taken_hotkey_keeps_the_previous_one_and_says_so() {
        let mut hotkeys = FakeHotkeys {
            taken: vec![ctrl_alt(Key::F8)],
            ..FakeHotkeys::default()
        };
        let current = settings(10, ctrl_alt(Key::F9));
        let applied = apply(
            &current,
            Some(ctrl_alt(Key::F9)),
            settings(30, ctrl_alt(Key::F8)),
            &mut hotkeys,
        );
        assert_eq!(applied.settings, settings(30, ctrl_alt(Key::F9)));
        assert_eq!(applied.active_hotkey, Some(ctrl_alt(Key::F9)));
        assert_eq!(
            applied.hotkey_problem.as_deref(),
            Some("Ctrl+Alt+F8 is in use by another app. The hotkey is still Ctrl+Alt+F9.")
        );
    }

    #[test]
    fn a_refused_hotkey_at_startup_leaves_none_active() {
        let mut hotkeys = FakeHotkeys {
            taken: vec![ctrl_alt(Key::F8)],
            ..FakeHotkeys::default()
        };
        let loaded = settings(10, ctrl_alt(Key::F8));
        let applied = apply(&loaded, None, loaded.clone(), &mut hotkeys);
        assert_eq!(applied.settings, loaded);
        assert_eq!(applied.active_hotkey, None);
        assert_eq!(
            applied.hotkey_problem.as_deref(),
            Some("Ctrl+Alt+F8 is in use by another app. No hotkey is active.")
        );
    }

    #[test]
    fn a_refused_removal_keeps_the_active_hotkey() {
        struct Refuses;
        impl RegisterHotkey for Refuses {
            fn register(&mut self, _: Option<Hotkey>) -> Result<(), HotkeyError> {
                Err(HotkeyError::Failed("the hotkey thread has exited".into()))
            }
        }
        let current = settings(10, ctrl_alt(Key::F8));
        let applied = apply(
            &current,
            Some(ctrl_alt(Key::F8)),
            Settings::default(),
            &mut Refuses,
        );
        assert_eq!(applied.settings.hotkey(), Some(ctrl_alt(Key::F8)));
        assert_eq!(applied.active_hotkey, Some(ctrl_alt(Key::F8)));
        assert_eq!(
            applied.hotkey_problem.as_deref(),
            Some(
                "The hotkey could not be turned off: the hotkey thread has exited. \
                 The hotkey is still Ctrl+Alt+F8."
            )
        );
    }

    #[test]
    fn other_refusals_carry_the_reason() {
        struct Refuses;
        impl RegisterHotkey for Refuses {
            fn register(&mut self, _: Option<Hotkey>) -> Result<(), HotkeyError> {
                Err(HotkeyError::Failed("F12 is reserved".into()))
            }
        }
        let current = settings(10, ctrl_alt(Key::F8));
        let applied = apply(
            &current,
            Some(ctrl_alt(Key::F8)),
            settings(10, ctrl_alt(Key::F12)),
            &mut Refuses,
        );
        assert_eq!(
            applied.hotkey_problem.as_deref(),
            Some(
                "Ctrl+Alt+F12 could not be registered: F12 is reserved. \
                 The hotkey is still Ctrl+Alt+F8."
            )
        );
    }
}
