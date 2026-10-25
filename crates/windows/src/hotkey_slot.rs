//! The order a hotkey change runs in (R15), kept apart from `RegisterHotKey`
//! so it is tested without Windows.

use saw_core::{Hotkey, HotkeyError};

/// The OS's table of registered hotkeys, as the slot drives it.
pub(crate) trait HotkeyTable {
    fn register(&mut self, id: i32, hotkey: Hotkey) -> Result<(), HotkeyError>;
    fn unregister(&mut self, id: i32);
}

/// Holds at most one toggle hotkey. Changing it registers the new combination
/// first and unregisters the old one only once that succeeded, so a refused
/// change leaves the old one working.
pub(crate) struct HotkeySlot<T> {
    table: T,
    current: Option<(i32, Hotkey)>,
    last_id: i32,
}

/// The ids an application may give its hotkeys.
const MAX_ID: i32 = 0xBFFF;

impl<T: HotkeyTable> HotkeySlot<T> {
    pub(crate) fn new(table: T) -> HotkeySlot<T> {
        HotkeySlot {
            table,
            current: None,
            last_id: 0,
        }
    }

    /// Whether `id` belongs to the hotkey registered now.
    pub(crate) fn is_current(&self, id: i32) -> bool {
        self.current.is_some_and(|(current, _)| current == id)
    }

    /// Replaces the current hotkey with `hotkey`, or with `None` unregisters
    /// it.
    pub(crate) fn change(&mut self, hotkey: Option<Hotkey>) -> Result<(), HotkeyError> {
        let Some(hotkey) = hotkey else {
            self.clear();
            return Ok(());
        };
        // Registering a combination this slot already holds would fail as
        // taken, by itself.
        if self.current.is_some_and(|(_, current)| current == hotkey) {
            return Ok(());
        }
        // A fresh id each time, so a press of a replaced hotkey still queued
        // as `WM_HOTKEY` never matches the current one.
        let id = self.last_id % MAX_ID + 1;
        self.table.register(id, hotkey)?;
        self.last_id = id;
        if let Some((old, _)) = self.current.replace((id, hotkey)) {
            self.table.unregister(old);
        }
        Ok(())
    }

    pub(crate) fn clear(&mut self) {
        if let Some((id, _)) = self.current.take() {
            self.table.unregister(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use saw_core::{Key, Modifiers};

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Register(i32, Hotkey),
        Unregister(i32),
    }

    /// A system-wide table where `taken` is held by other programs.
    #[derive(Default)]
    struct FakeTable {
        taken: Vec<Hotkey>,
        ours: HashMap<i32, Hotkey>,
        calls: Vec<Call>,
    }

    impl HotkeyTable for FakeTable {
        fn register(&mut self, id: i32, hotkey: Hotkey) -> Result<(), HotkeyError> {
            self.calls.push(Call::Register(id, hotkey));
            if self.taken.contains(&hotkey) || self.ours.values().any(|&h| h == hotkey) {
                return Err(HotkeyError::Taken);
            }
            self.ours.insert(id, hotkey);
            Ok(())
        }

        fn unregister(&mut self, id: i32) {
            self.calls.push(Call::Unregister(id));
            self.ours.remove(&id);
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

    #[test]
    fn a_change_registers_the_new_hotkey_before_unregistering_the_old() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        slot.change(Some(ctrl_alt(Key::F9))).unwrap();
        assert_eq!(
            slot.table.calls,
            [
                Call::Register(1, ctrl_alt(Key::F8)),
                Call::Register(2, ctrl_alt(Key::F9)),
                Call::Unregister(1),
            ]
        );
        assert_eq!(slot.table.ours, HashMap::from([(2, ctrl_alt(Key::F9))]));
        assert!(slot.is_current(2) && !slot.is_current(1));
    }

    #[test]
    fn a_taken_hotkey_is_refused_and_the_previous_one_stays() {
        let mut slot = HotkeySlot::new(FakeTable {
            taken: vec![ctrl_alt(Key::F9)],
            ..FakeTable::default()
        });
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        assert_eq!(
            slot.change(Some(ctrl_alt(Key::F9))),
            Err(HotkeyError::Taken)
        );
        assert_eq!(slot.table.ours, HashMap::from([(1, ctrl_alt(Key::F8))]));
        assert!(slot.is_current(1));
        assert!(
            !slot.table.calls.contains(&Call::Unregister(1)),
            "{:?}",
            slot.table.calls
        );
        slot.change(Some(ctrl_alt(Key::F10))).unwrap();
        assert_eq!(slot.table.ours, HashMap::from([(2, ctrl_alt(Key::F10))]));
    }

    #[test]
    fn a_refused_first_registration_leaves_none() {
        let mut slot = HotkeySlot::new(FakeTable {
            taken: vec![ctrl_alt(Key::F8)],
            ..FakeTable::default()
        });
        assert_eq!(
            slot.change(Some(ctrl_alt(Key::F8))),
            Err(HotkeyError::Taken)
        );
        assert!(!slot.is_current(1) && !slot.is_current(2));
    }

    #[test]
    fn a_replaced_hotkeys_id_is_not_reused_by_the_next_change() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        for key in [Key::F8, Key::F9, Key::F10] {
            slot.change(Some(ctrl_alt(key))).unwrap();
        }
        assert!(slot.is_current(3));
        assert!(!slot.is_current(1) && !slot.is_current(2));
    }

    #[test]
    fn ids_wrap_within_the_application_range() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        slot.last_id = MAX_ID;
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        assert!(slot.is_current(1));
    }

    #[test]
    fn changing_to_the_current_hotkey_does_nothing() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        assert_eq!(slot.table.calls, [Call::Register(1, ctrl_alt(Key::F8))]);
    }

    #[test]
    fn changing_to_none_unregisters_the_current_hotkey() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        slot.change(None).unwrap();
        assert!(slot.table.ours.is_empty());
        assert!(!slot.is_current(1));
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        assert!(slot.is_current(2));
    }

    #[test]
    fn clear_unregisters_the_current_hotkey() {
        let mut slot = HotkeySlot::new(FakeTable::default());
        slot.change(Some(ctrl_alt(Key::F8))).unwrap();
        slot.clear();
        assert!(slot.table.ours.is_empty());
        slot.clear();
        assert_eq!(slot.table.calls.len(), 2);
    }
}
