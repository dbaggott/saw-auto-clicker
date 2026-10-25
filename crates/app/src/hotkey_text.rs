//! The toggle hotkey as text, such as `Ctrl+Alt+F8`: how the config file
//! stores it and how the settings window and its messages show it.

use std::fmt;

use saw_core::{Hotkey, Key, Modifiers};

/// Every key the hotkey can use, in the order the settings window lists them.
pub const KEYS: [(Key, &str); 60] = [
    (Key::F1, "F1"),
    (Key::F2, "F2"),
    (Key::F3, "F3"),
    (Key::F4, "F4"),
    (Key::F5, "F5"),
    (Key::F6, "F6"),
    (Key::F7, "F7"),
    (Key::F8, "F8"),
    (Key::F9, "F9"),
    (Key::F10, "F10"),
    (Key::F11, "F11"),
    (Key::F12, "F12"),
    (Key::F13, "F13"),
    (Key::F14, "F14"),
    (Key::F15, "F15"),
    (Key::F16, "F16"),
    (Key::F17, "F17"),
    (Key::F18, "F18"),
    (Key::F19, "F19"),
    (Key::F20, "F20"),
    (Key::F21, "F21"),
    (Key::F22, "F22"),
    (Key::F23, "F23"),
    (Key::F24, "F24"),
    (Key::A, "A"),
    (Key::B, "B"),
    (Key::C, "C"),
    (Key::D, "D"),
    (Key::E, "E"),
    (Key::F, "F"),
    (Key::G, "G"),
    (Key::H, "H"),
    (Key::I, "I"),
    (Key::J, "J"),
    (Key::K, "K"),
    (Key::L, "L"),
    (Key::M, "M"),
    (Key::N, "N"),
    (Key::O, "O"),
    (Key::P, "P"),
    (Key::Q, "Q"),
    (Key::R, "R"),
    (Key::S, "S"),
    (Key::T, "T"),
    (Key::U, "U"),
    (Key::V, "V"),
    (Key::W, "W"),
    (Key::X, "X"),
    (Key::Y, "Y"),
    (Key::Z, "Z"),
    (Key::Digit0, "0"),
    (Key::Digit1, "1"),
    (Key::Digit2, "2"),
    (Key::Digit3, "3"),
    (Key::Digit4, "4"),
    (Key::Digit5, "5"),
    (Key::Digit6, "6"),
    (Key::Digit7, "7"),
    (Key::Digit8, "8"),
    (Key::Digit9, "9"),
];

pub fn key_name(key: Key) -> &'static str {
    KEYS.iter()
        .find(|(k, _)| *k == key)
        .map(|(_, name)| *name)
        .expect("KEYS lists every Key")
}

/// Displays a hotkey as `Ctrl+Alt+Shift+Win+<key>`, naming only the
/// modifiers it holds.
pub struct HotkeyText(pub Hotkey);

impl fmt::Display for HotkeyText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.0.modifiers;
        for (on, name) in [
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.meta, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(key_name(self.0.key))
    }
}

/// Reads what [`HotkeyText`] writes, ignoring case and spaces around each
/// part, in any modifier order. `Control` and `Meta` are accepted too.
pub fn parse(text: &str) -> Result<Hotkey, String> {
    let mut modifiers = Modifiers::default();
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(&mut modifiers.ctrl),
            "alt" => Some(&mut modifiers.alt),
            "shift" => Some(&mut modifiers.shift),
            "win" | "meta" => Some(&mut modifiers.meta),
            _ => None,
        };
        if let Some(flag) = modifier {
            if *flag {
                return Err(format!("{part} appears twice in {text:?}"));
            }
            *flag = true;
            continue;
        }
        let found = KEYS
            .iter()
            .find(|(_, name)| name.eq_ignore_ascii_case(part))
            .map(|(k, _)| *k)
            .ok_or_else(|| format!("{part:?} is not a key the hotkey can use"))?;
        if key.replace(found).is_some() {
            return Err(format!("{text:?} names more than one key"));
        }
    }
    let key = key.ok_or_else(|| format!("{text:?} names no key"))?;
    Ok(Hotkey { modifiers, key })
}

/// Refuses a letter or digit without Ctrl, Alt or Win: registered alone or
/// with only Shift, it would take that key away from typing in every app.
pub fn usable(hotkey: Hotkey) -> Result<Hotkey, String> {
    let m = hotkey.modifiers;
    let function_key = matches!(
        hotkey.key,
        Key::F1
            | Key::F2
            | Key::F3
            | Key::F4
            | Key::F5
            | Key::F6
            | Key::F7
            | Key::F8
            | Key::F9
            | Key::F10
            | Key::F11
            | Key::F12
            | Key::F13
            | Key::F14
            | Key::F15
            | Key::F16
            | Key::F17
            | Key::F18
            | Key::F19
            | Key::F20
            | Key::F21
            | Key::F22
            | Key::F23
            | Key::F24
    );
    if !function_key && !(m.ctrl || m.alt || m.meta) {
        return Err(format!(
            "{} needs Ctrl, Alt or Win, or it would stop that key typing",
            HotkeyText(hotkey)
        ));
    }
    Ok(hotkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_alt_f8() -> Hotkey {
        Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
            key: Key::F8,
        }
    }

    #[test]
    fn ctrl_alt_f8_reads_as_written() {
        assert_eq!(HotkeyText(ctrl_alt_f8()).to_string(), "Ctrl+Alt+F8");
        assert_eq!(parse("Ctrl+Alt+F8"), Ok(ctrl_alt_f8()));
    }

    #[test]
    fn every_key_and_modifier_round_trips() {
        for (key, _) in KEYS {
            for bits in 0..16u8 {
                let hotkey = Hotkey {
                    modifiers: Modifiers {
                        ctrl: bits & 1 != 0,
                        alt: bits & 2 != 0,
                        shift: bits & 4 != 0,
                        meta: bits & 8 != 0,
                    },
                    key,
                };
                assert_eq!(parse(&HotkeyText(hotkey).to_string()), Ok(hotkey));
            }
        }
    }

    #[test]
    fn keys_are_listed_once_each() {
        for (i, (key, name)) in KEYS.iter().enumerate() {
            assert!(
                KEYS[i + 1..].iter().all(|(k, n)| k != key && n != name),
                "{name} is listed twice"
            );
        }
    }

    #[test]
    fn case_spacing_and_order_are_ignored() {
        assert_eq!(parse(" alt + CONTROL+f8 "), Ok(ctrl_alt_f8()));
    }

    #[test]
    fn letters_and_digits_need_a_non_shift_modifier() {
        let with = |shift, ctrl, key| Hotkey {
            modifiers: Modifiers {
                shift,
                ctrl,
                ..Modifiers::default()
            },
            key,
        };
        assert!(usable(with(false, false, Key::A)).is_err());
        assert!(usable(with(true, false, Key::Digit1)).is_err());
        assert!(usable(with(false, true, Key::A)).is_ok());
        assert!(usable(with(false, false, Key::F)).is_err());
        assert!(usable(with(false, false, Key::F8)).is_ok());
        assert!(usable(with(true, false, Key::F24)).is_ok());
    }

    #[test]
    fn malformed_text_is_rejected() {
        for text in [
            "",
            "Ctrl+Alt",
            "Ctrl+Ctrl+F8",
            "Ctrl+F8+F9",
            "Ctrl+Alt+F25",
            "Ctrl++F8",
        ] {
            assert!(parse(text).is_err(), "{text:?} parsed");
        }
    }
}
