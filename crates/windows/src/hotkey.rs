//! The toggle hotkey (R2, R15, C4), on a thread of its own: neither the hook
//! thread (R11) nor the app's UI thread, whose event loop may not see
//! thread-posted `WM_HOTKEY` messages.

use std::io;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::{self, JoinHandle};

use saw_core::{Controller, Hotkey, HotkeyError, Key, RegisterHotkey};
use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
};
use windows::core::HRESULT;

use crate::hotkey_slot::{HotkeySlot, HotkeyTable};

const WM_REQUEST: u32 = WM_APP;
const WM_STOP: u32 = WM_APP + 1;

type Reply = mpsc::SyncSender<Result<(), HotkeyError>>;

/// The request `register` hands the thread. One slot is enough, because
/// `register` takes `&mut self` and waits for its answer.
type Pending = Arc<Mutex<Option<(Option<Hotkey>, Reply)>>>;

/// Owns the hotkey thread, where each press toggles enabled. Registration runs
/// on that thread, because `RegisterHotKey` without a window posts to the
/// thread that called it. Dropping this unregisters the hotkey.
pub struct HotkeyThread {
    thread_id: u32,
    pending: Pending,
    thread: Option<JoinHandle<()>>,
}

impl HotkeyThread {
    /// Starts with no hotkey registered.
    pub fn spawn(controller: Controller) -> io::Result<HotkeyThread> {
        let (id_tx, id_rx) = mpsc::sync_channel(1);
        let pending = Pending::default();
        let incoming = Arc::clone(&pending);
        let thread = thread::Builder::new()
            .name("saw-hotkey".into())
            .spawn(move || {
                let mut msg = MSG::default();
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }
                if id_tx.send(unsafe { GetCurrentThreadId() }).is_err() {
                    return;
                }
                let mut slot = HotkeySlot::new(ThreadHotkeys);
                loop {
                    let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if got.0 <= 0 || msg.message == WM_STOP {
                        break;
                    }
                    match msg.message {
                        WM_HOTKEY if slot.is_current(msg.wParam.0 as i32) => {
                            controller.toggle_enabled();
                        }
                        WM_REQUEST => {
                            if let Some((hotkey, reply)) = take(&incoming) {
                                let _ = reply.send(slot.change(hotkey));
                            }
                        }
                        _ => {}
                    }
                }
                slot.clear();
            })?;
        let thread_id = id_rx
            .recv()
            .map_err(|_| io::Error::other("the hotkey thread exited before starting"))?;
        Ok(HotkeyThread {
            thread_id,
            pending,
            thread: Some(thread),
        })
    }
}

impl RegisterHotkey for HotkeyThread {
    fn register(&mut self, hotkey: Option<Hotkey>) -> Result<(), HotkeyError> {
        let gone = || HotkeyError::Failed("the hotkey thread has exited".into());
        let (reply, answer) = mpsc::sync_channel(1);
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some((hotkey, reply));
        if let Err(e) =
            unsafe { PostThreadMessageW(self.thread_id, WM_REQUEST, WPARAM(0), LPARAM(0)) }
        {
            // Not posted, so the thread never takes it; withdraw it so a later
            // request doesn't carry it out.
            take(&self.pending);
            return Err(HotkeyError::Failed(e.message()));
        }
        answer.recv().map_err(|_| gone())?
    }
}

impl Drop for HotkeyThread {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_STOP, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn take(pending: &Pending) -> Option<(Option<Hotkey>, Reply)> {
    // The slot is a plain Option, so a panic elsewhere can't leave it
    // inconsistent.
    pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// `RegisterHotKey` with no window, for the calling thread.
struct ThreadHotkeys;

impl HotkeyTable for ThreadHotkeys {
    fn register(&mut self, id: i32, hotkey: Hotkey) -> Result<(), HotkeyError> {
        let (modifiers, key) = win32_combination(hotkey);
        unsafe { RegisterHotKey(None, id, modifiers, u32::from(key.0)) }.map_err(|e| {
            if e.code() == HRESULT::from_win32(ERROR_HOTKEY_ALREADY_REGISTERED.0) {
                HotkeyError::Taken
            } else {
                HotkeyError::Failed(e.message())
            }
        })
    }

    fn unregister(&mut self, id: i32) {
        // Fails only for an id not registered on this thread, which the slot
        // never asks for.
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
}

/// The modifiers always carry `MOD_NOREPEAT`, so holding the combination
/// toggles once.
fn win32_combination(hotkey: Hotkey) -> (HOT_KEY_MODIFIERS, VIRTUAL_KEY) {
    let m = hotkey.modifiers;
    let mut modifiers = MOD_NOREPEAT;
    for (on, flag) in [
        (m.ctrl, MOD_CONTROL),
        (m.alt, MOD_ALT),
        (m.shift, MOD_SHIFT),
        (m.meta, MOD_WIN),
    ] {
        if on {
            modifiers |= flag;
        }
    }
    (modifiers, virtual_key(hotkey.key))
}

fn virtual_key(key: Key) -> VIRTUAL_KEY {
    match key {
        Key::A => VK_A,
        Key::B => VK_B,
        Key::C => VK_C,
        Key::D => VK_D,
        Key::E => VK_E,
        Key::F => VK_F,
        Key::G => VK_G,
        Key::H => VK_H,
        Key::I => VK_I,
        Key::J => VK_J,
        Key::K => VK_K,
        Key::L => VK_L,
        Key::M => VK_M,
        Key::N => VK_N,
        Key::O => VK_O,
        Key::P => VK_P,
        Key::Q => VK_Q,
        Key::R => VK_R,
        Key::S => VK_S,
        Key::T => VK_T,
        Key::U => VK_U,
        Key::V => VK_V,
        Key::W => VK_W,
        Key::X => VK_X,
        Key::Y => VK_Y,
        Key::Z => VK_Z,
        Key::Digit0 => VK_0,
        Key::Digit1 => VK_1,
        Key::Digit2 => VK_2,
        Key::Digit3 => VK_3,
        Key::Digit4 => VK_4,
        Key::Digit5 => VK_5,
        Key::Digit6 => VK_6,
        Key::Digit7 => VK_7,
        Key::Digit8 => VK_8,
        Key::Digit9 => VK_9,
        Key::F1 => VK_F1,
        Key::F2 => VK_F2,
        Key::F3 => VK_F3,
        Key::F4 => VK_F4,
        Key::F5 => VK_F5,
        Key::F6 => VK_F6,
        Key::F7 => VK_F7,
        Key::F8 => VK_F8,
        Key::F9 => VK_F9,
        Key::F10 => VK_F10,
        Key::F11 => VK_F11,
        Key::F12 => VK_F12,
        Key::F13 => VK_F13,
        Key::F14 => VK_F14,
        Key::F15 => VK_F15,
        Key::F16 => VK_F16,
        Key::F17 => VK_F17,
        Key::F18 => VK_F18,
        Key::F19 => VK_F19,
        Key::F20 => VK_F20,
        Key::F21 => VK_F21,
        Key::F22 => VK_F22,
        Key::F23 => VK_F23,
        Key::F24 => VK_F24,
    }
}

#[cfg(test)]
mod tests {
    use saw_core::Modifiers;

    use super::*;

    #[test]
    fn ctrl_alt_f8_maps_without_repeat() {
        let (modifiers, key) = win32_combination(Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
            key: Key::F8,
        });
        assert_eq!(modifiers, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT);
        assert_eq!(key, VK_F8);
    }

    #[test]
    fn every_modifier_maps() {
        let all = Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                shift: true,
                meta: true,
            },
            key: Key::Digit0,
        };
        let (modifiers, key) = win32_combination(all);
        assert_eq!(
            modifiers,
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_WIN | MOD_NOREPEAT
        );
        assert_eq!(key, VK_0);
    }
}
