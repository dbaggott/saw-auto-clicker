//! The observing thread (R11, C1): a low-level mouse hook on a thread that
//! does nothing but pump messages and run the hook callback.

use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use saw_core::{Button, ButtonEvent, ButtonGate, Direction, Verdict};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

use crate::tag::source_of;

/// Runs on the hook thread with each left or right event and the verdict it
/// got, after the gate has decided. It runs inside the hook callback, so it
/// must return as promptly as the gate does (C1).
pub(crate) type ObservedTap = Box<dyn FnMut(ButtonEvent, Verdict) + Send>;

const WM_REINSTALL: u32 = WM_APP;
const WM_STOP: u32 = WM_APP + 1;

/// The left or right button event a hook message carries, if any. Every other
/// message, such as a move, the wheel, or the middle and side buttons, is not
/// the gate's business.
pub(crate) fn button_event(message: u32, extra_info: usize) -> Option<ButtonEvent> {
    let (button, direction) = match message {
        WM_LBUTTONDOWN => (Button::Left, Direction::Down),
        WM_LBUTTONUP => (Button::Left, Direction::Up),
        WM_RBUTTONDOWN => (Button::Right, Direction::Down),
        WM_RBUTTONUP => (Button::Right, Direction::Up),
        _ => return None,
    };
    Some(ButtonEvent {
        button,
        direction,
        source: source_of(extra_info),
    })
}

struct Observer {
    gate: ButtonGate,
    tap: ObservedTap,
    hook: Option<HHOOK>,
}

thread_local! {
    /// The hook callback is a plain function that Windows runs on the thread
    /// that installed it, so it finds that thread's gate here.
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

/// Owns the hook thread. Dropping it removes the hook and joins the thread.
pub(crate) struct HookThread {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    handover: Option<mpsc::SyncSender<(ButtonGate, ObservedTap)>>,
    installed: mpsc::Receiver<io::Result<()>>,
}

/// What the scheduler holds to replace the hook (D8).
#[derive(Clone, Copy)]
pub(crate) struct Reinstaller {
    thread_id: u32,
}

impl Reinstaller {
    /// Queues the reinstall on the hook thread and returns at once.
    pub(crate) fn request(self) {
        // Posting fails only once the hook thread has exited, when there is
        // no hook left to replace.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_REINSTALL, WPARAM(0), LPARAM(0)) };
    }
}

impl HookThread {
    /// Starts the thread, not yet observing: the gate comes later through
    /// [`HookThread::observe`], because the core creates it from a backend
    /// that needs this thread's id. Returns once the thread can receive
    /// messages.
    pub(crate) fn spawn() -> io::Result<HookThread> {
        let (id_tx, id_rx) = mpsc::sync_channel(1);
        let (handover, handed_over) = mpsc::sync_channel::<(ButtonGate, ObservedTap)>(1);
        let (installed_tx, installed) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("saw-hook".into())
            .spawn(move || {
                let mut msg = MSG::default();
                // Creates this thread's message queue, so posts to it from now
                // on are kept.
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }
                if id_tx.send(unsafe { GetCurrentThreadId() }).is_err() {
                    return;
                }
                let Ok((gate, tap)) = handed_over.recv() else {
                    return;
                };
                OBSERVER.set(Some(Observer {
                    gate,
                    tap,
                    hook: None,
                }));
                let result = install();
                let ok = result.is_ok();
                let _ = installed_tx.send(result);
                if ok {
                    pump();
                }
                if let Some(hook) = OBSERVER.take().and_then(|o| o.hook) {
                    unsafe {
                        let _ = UnhookWindowsHookEx(hook);
                    }
                }
            })?;
        let thread_id = id_rx
            .recv()
            .map_err(|_| io::Error::other("the hook thread exited before starting"))?;
        Ok(HookThread {
            thread_id,
            thread: Some(thread),
            handover: Some(handover),
            installed,
        })
    }

    pub(crate) fn reinstaller(&self) -> Reinstaller {
        Reinstaller {
            thread_id: self.thread_id,
        }
    }

    /// Hands the thread its gate and installs the hook, returning once it is
    /// in place.
    pub(crate) fn observe(&mut self, gate: ButtonGate, tap: ObservedTap) -> io::Result<()> {
        let gone = || io::Error::other("the hook thread exited");
        self.handover
            .take()
            .ok_or_else(|| io::Error::other("the hook thread is already observing"))?
            .send((gate, tap))
            .map_err(|_| gone())?;
        self.installed.recv().map_err(|_| gone())?
    }
}

impl Drop for HookThread {
    fn drop(&mut self) {
        // A thread still waiting for its gate exits when the sender goes.
        self.handover = None;
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_STOP, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn pump() {
    let mut msg = MSG::default();
    loop {
        // 0 is WM_QUIT and -1 an error; neither leaves anything to pump.
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            return;
        }
        match msg.message {
            WM_STOP => return,
            WM_REINSTALL => reinstall(),
            _ => {}
        }
    }
}

fn install() -> io::Result<()> {
    let module = unsafe { GetModuleHandleW(None) }?;
    let hook =
        unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(HINSTANCE(module.0)), 0) }?;
    OBSERVER.with_borrow_mut(|o| {
        if let Some(o) = o {
            o.hook = Some(hook);
        }
    });
    Ok(())
}

/// D8: unhook, forget what applications hold, install. The reset runs between
/// the two, on this thread, so it lands before the new hook's first event.
fn reinstall() {
    OBSERVER.with_borrow_mut(|o| {
        let Some(o) = o else { return };
        if let Some(old) = o.hook.take() {
            // Fails only for a hook Windows already removed (C1), which is
            // the case being recovered from.
            let _ = unsafe { UnhookWindowsHookEx(old) };
        }
        o.gate.reset_delivered();
    });
    // A failed install leaves no hook; the scheduler's idle reinstall retries
    // it, and no burst can start meanwhile because no press is seen.
    let _ = install();
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // For HC_ACTION, lparam points at the event's MSLLHOOKSTRUCT.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if let Some(event) = button_event(wparam.0 as u32, info.dwExtraInfo) {
            let verdict = OBSERVER.with(|o| {
                // The callback runs only while this thread waits in
                // GetMessageW, never inside another borrow, so this borrow
                // always succeeds; passing is the safe answer if it didn't.
                let mut o = o.try_borrow_mut().ok()?;
                let o = o.as_mut()?;
                let verdict = o.gate.observe(event);
                (o.tap)(event, verdict);
                Some(verdict)
            });
            if verdict == Some(Verdict::Suppress) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use saw_core::Source;
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_XBUTTONDOWN,
    };

    use super::*;
    use crate::tag::OWN_EVENT_TAG;

    #[test]
    fn left_and_right_messages_become_events() {
        let cases = [
            (WM_LBUTTONDOWN, Button::Left, Direction::Down),
            (WM_LBUTTONUP, Button::Left, Direction::Up),
            (WM_RBUTTONDOWN, Button::Right, Direction::Down),
            (WM_RBUTTONUP, Button::Right, Direction::Up),
        ];
        for (message, button, direction) in cases {
            assert_eq!(
                button_event(message, 0),
                Some(ButtonEvent {
                    button,
                    direction,
                    source: Source::Physical
                })
            );
            assert_eq!(
                button_event(message, OWN_EVENT_TAG).map(|e| e.source),
                Some(Source::Tagged)
            );
        }
    }

    #[test]
    fn other_messages_are_not_the_gates() {
        for message in [WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MBUTTONDOWN, WM_XBUTTONDOWN] {
            assert_eq!(button_event(message, OWN_EVENT_TAG), None);
        }
    }
}
