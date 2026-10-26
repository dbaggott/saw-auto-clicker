//! One instance per sign-in session (Q-a), the signal a second launch sends
//! the running instance, and the message boxes the app uses when it has no
//! window of its own to speak through.

use std::io;
use std::thread;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
    WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE,
    MessageBoxW,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Set by a second launch to ask the running instance for its settings
/// window.
const SHOW_SETTINGS_EVENT: PCWSTR = w!(r"Local\saw-auto-clicker-show-settings");

/// Held for the life of the process. Two instances would each hook and
/// inject with the same tag (D3), so each would take the other's clicks
/// for its own.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    /// `None` when another instance in this session holds the claim.
    pub fn claim() -> io::Result<Option<SingleInstance>> {
        let handle = unsafe { CreateMutexW(None, false, w!(r"Local\saw-auto-clicker")) }?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            let _ = unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(SingleInstance(handle)))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Calls `on_request` on a thread of its own each time
/// [`ask_running_instance_to_show_settings`] runs, for the life of the
/// process.
pub fn listen_for_show_settings(on_request: impl Fn() + Send + 'static) -> io::Result<()> {
    // Auto-reset, so each request wakes the thread once.
    let event = unsafe { CreateEventW(None, false, false, SHOW_SETTINGS_EVENT) }?;
    // A HANDLE is not Send; the thread owns this one from here on.
    let raw = event.0 as usize;
    thread::Builder::new()
        .name("saw-show-settings".into())
        .spawn(move || {
            let event = HANDLE(raw as *mut _);
            while unsafe { WaitForSingleObject(event, INFINITE) } == WAIT_OBJECT_0 {
                on_request();
            }
        })?;
    Ok(())
}

/// Errs when the running instance is not listening, as in the moment
/// before it starts to.
pub fn ask_running_instance_to_show_settings() -> io::Result<()> {
    // The running instance is in the background; without this, Windows
    // flashes its window's taskbar button instead of bringing it forward.
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, SHOW_SETTINGS_EVENT) }?;
    let set = unsafe { SetEvent(event) };
    let _ = unsafe { CloseHandle(event) };
    Ok(set?)
}

pub fn show_info(text: &str) {
    message_box(text, MB_ICONINFORMATION);
}

pub fn show_error(text: &str) {
    message_box(text, MB_ICONERROR);
}

fn message_box(text: &str, icon: MESSAGEBOX_STYLE) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            w!("saw-auto-clicker"),
            MB_OK | icon,
        );
    }
}
