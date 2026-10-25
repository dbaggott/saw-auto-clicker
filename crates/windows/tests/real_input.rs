//! The backend on the real Windows desktop: the hook, `SendInput`, the timer
//! and `RegisterHotKey`. These move the cursor and click, so they belong on a
//! CI runner or a machine nobody is using.

#![cfg(windows)]

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use saw_core::{
    Button, ButtonEvent, Direction, Hotkey, HotkeyError, Inject, Key, Modifiers, Observe,
    RegisterHotkey, Settings, Source, StdClock, Verdict,
};
use saw_windows::{Clicker, HotkeyThread, ThrottlingOptOuts};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT, RegisterHotKey, SendInput,
    UnregisterHotKey, VK_F20, VK_F21,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, MSG,
    PostThreadMessageW, SetCursorPos, WM_QUIT, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

/// Another program's injection, untagged, which the backend must treat as a
/// physical press (R10).
fn send_untagged(flags: MOUSE_EVENT_FLAGS) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..MOUSEINPUT::default()
            },
        },
    };
    let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    assert_eq!(sent, 1, "SendInput: {}", std::io::Error::last_os_error());
}

/// A topmost window of this process under the cursor, so the elevated-target
/// check sees a target it can read and the clicks land somewhere harmless.
struct TargetWindow {
    thread_id: u32,
    thread: Option<thread::JoinHandle<()>>,
}

impl TargetWindow {
    fn under_cursor() -> TargetWindow {
        let (ready_tx, ready) = mpsc::channel();
        let thread = thread::spawn(move || unsafe {
            let mut cursor = Default::default();
            GetCursorPos(&mut cursor).unwrap();
            let window = CreateWindowExW(
                WS_EX_TOPMOST,
                w!("BUTTON"),
                w!("saw-windows test target"),
                WS_POPUP | WS_VISIBLE,
                cursor.x - 100,
                cursor.y - 100,
                200,
                200,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            SetCursorPos(cursor.x, cursor.y).unwrap();
            ready_tx.send(GetCurrentThreadId()).unwrap();
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                DispatchMessageW(&msg);
            }
            let _ = DestroyWindow(window);
        });
        TargetWindow {
            thread_id: ready.recv().unwrap(),
            thread: Some(thread),
        }
    }
}

impl Drop for TargetWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn a_held_button_auto_clicks_through_the_real_hook() {
    let _target = TargetWindow::under_cursor();
    let observed = Arc::new(Mutex::new(Vec::<(ButtonEvent, Verdict)>::new()));
    let log = Arc::clone(&observed);
    let settings = Settings::new(20, Duration::from_millis(100), 0.0, None).unwrap();
    let clicker = Clicker::start(
        settings,
        ThrottlingOptOuts::default(),
        |_| {},
        move |event, verdict| log.lock().unwrap().push((event, verdict)),
    )
    .unwrap();
    clicker.controller().set_armed(true);
    // Arming reinstalls the hook; let that land before pressing.
    thread::sleep(Duration::from_millis(200));

    send_untagged(MOUSEEVENTF_LEFTDOWN);
    thread::sleep(Duration::from_millis(700));
    send_untagged(MOUSEEVENTF_LEFTUP);
    thread::sleep(Duration::from_millis(200));
    clicker.shutdown().unwrap();

    let observed = observed.lock().unwrap();
    let left: Vec<_> = observed
        .iter()
        .filter(|(e, _)| e.button == Button::Left)
        .collect();
    let physical = |direction| ButtonEvent {
        button: Button::Left,
        direction,
        source: Source::Physical,
    };
    assert_eq!(
        left.first().map(|(e, v)| (*e, *v)),
        Some((physical(Direction::Down), Verdict::Pass)),
        "{observed:?}"
    );
    let clicks = left
        .iter()
        .filter(|(e, v)| {
            e.source == Source::Tagged && e.direction == Direction::Down && *v == Verdict::Pass
        })
        .count();
    // 600 ms of burst at 20 CPS is about 12 clicks; the bound only shows the
    // loop runs on real input, not its rate, which is A1's job.
    assert!(clicks >= 5, "{clicks} clicks: {observed:?}");
    let passed: Vec<Direction> = left
        .iter()
        .filter(|(_, v)| *v == Verdict::Pass)
        .map(|(e, _)| e.direction)
        .collect();
    assert!(passed.windows(2).all(|w| w[0] != w[1]), "{passed:?}");
    assert_eq!(passed.last(), Some(&Direction::Up), "{observed:?}");
}

struct Inert;

impl Observe for Inert {
    fn reinstall(&mut self) {}
}

impl Inject for Inert {
    fn inject(&mut self, _: Button, _: Direction) {}

    fn can_inject_at_cursor(&mut self) -> bool {
        false
    }
}

fn ctrl_alt_shift(key: Key) -> Hotkey {
    Hotkey {
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            shift: true,
            meta: false,
        },
        key,
    }
}

/// Whether some thread already holds Ctrl+Alt+Shift+`vk`, tested by trying to
/// take it from a fresh thread.
fn held_elsewhere(vk: u16) -> bool {
    thread::spawn(move || unsafe {
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
        let free = RegisterHotKey(None, 7, modifiers, u32::from(vk)).is_ok();
        if free {
            UnregisterHotKey(None, 7).unwrap();
        }
        !free
    })
    .join()
    .unwrap()
}

#[test]
fn a_taken_hotkey_is_refused_and_the_previous_one_keeps_working() {
    // Another holder of F20, on a thread that keeps it until told to stop.
    let (stop_tx, stop) = mpsc::channel::<()>();
    let (held_tx, held) = mpsc::channel();
    let holder = thread::spawn(move || unsafe {
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
        RegisterHotKey(None, 1, modifiers, u32::from(VK_F20.0)).unwrap();
        held_tx.send(()).unwrap();
        let _ = stop.recv();
        UnregisterHotKey(None, 1).unwrap();
    });
    held.recv().unwrap();

    // The hotkey thread only needs a controller to toggle; a clicker's own
    // would put the real mouse hook into this test too.
    let (_gate, scheduler) = saw_core::spawn(
        Inert,
        Arc::new(StdClock::new()),
        Settings::default(),
        |_| {},
    )
    .unwrap();
    let mut hotkeys = HotkeyThread::spawn(scheduler.controller()).unwrap();

    hotkeys.register(Some(ctrl_alt_shift(Key::F21))).unwrap();
    assert!(held_elsewhere(VK_F21.0));
    assert_eq!(
        hotkeys.register(Some(ctrl_alt_shift(Key::F20))),
        Err(HotkeyError::Taken)
    );
    assert!(held_elsewhere(VK_F21.0), "the previous hotkey was dropped");

    hotkeys.register(None).unwrap();
    assert!(!held_elsewhere(VK_F21.0), "None did not unregister");
    hotkeys.register(Some(ctrl_alt_shift(Key::F21))).unwrap();
    drop(hotkeys);
    assert!(!held_elsewhere(VK_F21.0), "dropping did not unregister");
    stop_tx.send(()).unwrap();
    holder.join().unwrap();
    scheduler.shutdown().unwrap();
}
