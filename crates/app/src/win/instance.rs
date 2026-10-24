//! One instance per sign-in session (Q-a), and the message boxes the app
//! uses when it has no window of its own to speak through.

use std::io;

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MessageBoxW,
};
use windows::core::{HSTRING, w};

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
