//! The settings window's icon, in the taskbar and Alt+Tab as well as its
//! title bar, showing the enabled state as the tray icon does (R14).
//!
//! winit sets only a window's small icon, which the taskbar does not use, so
//! both sizes are set here with `WM_SETICON`.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIcon, DestroyIcon, HICON, ICON_BIG, ICON_SMALL, SendMessageW, WM_SETICON,
};

use crate::icon;

/// Both drawings as icons, destroyed on drop, which must come after the
/// window that shows them has closed.
pub struct WindowIcons {
    enabled: HICON,
    disabled: HICON,
}

impl WindowIcons {
    pub fn new() -> windows::core::Result<WindowIcons> {
        let enabled = create(true)?;
        match create(false) {
            Ok(disabled) => Ok(WindowIcons { enabled, disabled }),
            Err(e) => {
                let _ = unsafe { DestroyIcon(enabled) };
                Err(e)
            }
        }
    }

    pub fn show(&self, window: HWND, enabled: bool) {
        let icon = if enabled { self.enabled } else { self.disabled };
        for size in [ICON_BIG, ICON_SMALL] {
            unsafe {
                SendMessageW(
                    window,
                    WM_SETICON,
                    Some(WPARAM(size as usize)),
                    Some(LPARAM(icon.0 as isize)),
                );
            }
        }
    }
}

impl Drop for WindowIcons {
    fn drop(&mut self) {
        // Fails only for an icon already destroyed, which nothing else does.
        unsafe {
            let _ = DestroyIcon(self.enabled);
            let _ = DestroyIcon(self.disabled);
        }
    }
}

/// A 32-bit icon of the drawing: its alpha does the masking, so the AND
/// mask is all zeros.
fn create(enabled: bool) -> windows::core::Result<HICON> {
    let rgba = icon::rgba(enabled);
    let bgra: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [b, g, r, a])
        .collect();
    let side = icon::SIZE as i32;
    let mask = vec![0u8; (icon::SIZE * icon::SIZE / 8) as usize];
    unsafe { CreateIcon(None, side, side, 1, 32, mask.as_ptr(), bgra.as_ptr()) }
}
