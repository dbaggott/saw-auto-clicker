//! Plays the saw-blade icon's [flight](crate::flight) into the tray in a
//! see-through, topmost window that takes neither clicks nor the focus.

use std::sync::Once;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC,
    HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, FindWindowW, GetWindowRect,
    IsWindowVisible, RegisterClassW, SPI_GETCLIENTAREAANIMATION, SW_SHOWNOACTIVATE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ShowWindow, SystemParametersInfoW, ULW_ALPHA,
    UpdateLayeredWindow, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, Result, w};

use crate::flight::{self, Frame, ScreenRect};
use crate::icon;

const CLASS: PCWSTR = w!("saw-auto-clicker-fly-to-tray");

/// Flies the icon from `from`, where the settings window was, to the tray
/// icon at `tray_icon`, starting at a size for `scale` physical pixels per
/// point. Does nothing when Windows' animation effects are off or there is
/// nowhere to fly to, and returns once the icon is gone.
pub fn fly(from: ScreenRect, tray_icon: Option<ScreenRect>, scale: f64) {
    if !animation_effects_on() {
        return;
    }
    let taskbar = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.ok();
    let Some(to) = flight::destination(
        tray_icon,
        taskbar.and_then(window_rect),
        taskbar.and_then(overflow_button),
    ) else {
        return;
    };
    // A failure costs only the animation; the window has already closed.
    let _ = play(from, to, flight::START_SIZE * scale);
}

/// Windows' Settings > Accessibility > Visual effects > Animation effects.
/// Unreadable reads as off.
fn animation_effects_on() -> bool {
    let mut on = BOOL(0);
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_ok() && on.as_bool()
}

/// The ^ button that opens the hidden icons, where the taskbar has one as a
/// window of its own.
fn overflow_button(taskbar: HWND) -> Option<ScreenRect> {
    let area =
        unsafe { FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null()) }.ok()?;
    let button = unsafe { FindWindowExW(Some(area), None, w!("Button"), PCWSTR::null()) }.ok()?;
    window_rect(button)
}

/// Where a visible window is, or `None` for a hidden or empty one.
fn window_rect(window: HWND) -> Option<ScreenRect> {
    if !unsafe { IsWindowVisible(window) }.as_bool() {
        return None;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window, &mut rect) }.ok()?;
    (rect.right > rect.left && rect.bottom > rect.top).then(|| ScreenRect {
        left: rect.left.into(),
        top: rect.top.into(),
        width: (rect.right - rect.left).into(),
        height: (rect.bottom - rect.top).into(),
    })
}

/// Runs the flight on this thread, a frame per screen refresh, without
/// pumping messages: anything arriving meanwhile waits the flight's length.
fn play(from: ScreenRect, to: ScreenRect, start_size: f64) -> Result<()> {
    let largest = start_size.max(to.width.min(to.height)).ceil() as i32;
    let mut sprite = Sprite::new(largest)?;
    let start = Instant::now();
    loop {
        let t = start.elapsed().as_secs_f64() / flight::DURATION.as_secs_f64();
        if t >= 1.0 {
            return Ok(());
        }
        sprite.show(flight::frame(from, to, start_size, t))?;
        if unsafe { DwmFlush() }.is_err() {
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

/// The layered window and the bitmap it is drawn from, both freed on drop.
struct Sprite {
    window: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    /// What the bitmap replaced in `dc`, put back before the bitmap goes.
    replaced: HGDIOBJ,
    /// The bitmap's pixels, `canvas` square, top row first, as
    /// premultiplied BGRA.
    pixels: *mut u8,
    canvas: i32,
    shown: bool,
}

impl Sprite {
    /// A hidden window, and a bitmap `canvas` pixels square to draw each
    /// frame into, at its top left.
    fn new(canvas: i32) -> Result<Sprite> {
        let instance = unsafe { GetModuleHandleW(None) }?;
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..WNDCLASSW::default()
            };
            // A failure shows as CreateWindowExW's below.
            unsafe { RegisterClassW(&class) };
        });
        let mut sprite = Sprite {
            window: HWND::default(),
            dc: HDC::default(),
            bitmap: HBITMAP::default(),
            replaced: HGDIOBJ::default(),
            pixels: std::ptr::null_mut(),
            canvas,
            shown: false,
        };
        sprite.window = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                CLASS,
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }?;
        sprite.dc = unsafe { CreateCompatibleDC(None) };
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: canvas,
                // Negative for top row first.
                biHeight: -canvas,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits = std::ptr::null_mut();
        sprite.bitmap = unsafe {
            CreateDIBSection(Some(sprite.dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        }?;
        sprite.pixels = bits.cast();
        sprite.replaced = unsafe { SelectObject(sprite.dc, sprite.bitmap.into()) };
        Ok(sprite)
    }

    fn show(&mut self, frame: Frame) -> Result<()> {
        let size = (frame.size.round() as i32).clamp(1, self.canvas);
        let canvas = self.canvas as usize;
        let pixels = unsafe { std::slice::from_raw_parts_mut(self.pixels, canvas * canvas * 4) };
        let drawing = icon::rgba_at(size as u32, true);
        for (row, line) in drawing.chunks_exact(size as usize * 4).enumerate() {
            let out = &mut pixels[row * canvas * 4..][..line.len()];
            for (to, &[r, g, b, a]) in out
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(line.as_chunks::<4>().0)
            {
                let premultiply = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
                *to = [premultiply(b), premultiply(g), premultiply(r), a];
            }
        }
        let half = f64::from(size) / 2.0;
        let position = POINT {
            x: (frame.centre.0 - half).round() as i32,
            y: (frame.centre.1 - half).round() as i32,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (frame.opacity * 255.0).round() as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                self.window,
                None,
                Some(&position),
                Some(&SIZE { cx: size, cy: size }),
                Some(self.dc),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }?;
        if !self.shown {
            let _ = unsafe { ShowWindow(self.window, SW_SHOWNOACTIVATE) };
            self.shown = true;
        }
        Ok(())
    }
}

impl Drop for Sprite {
    fn drop(&mut self) {
        unsafe {
            if !self.window.is_invalid() {
                let _ = DestroyWindow(self.window);
            }
            if !self.dc.is_invalid() {
                if !self.replaced.is_invalid() {
                    SelectObject(self.dc, self.replaced);
                }
                let _ = DeleteDC(self.dc);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap.into());
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
