//! The tray icon, its menu, and the notification it shows (R14).

use std::mem::size_of;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_GUID, NIF_INFO, NIF_TIP, NIIF_INFO, NIM_MODIFY, NOTIFY_ICON_DATA_FLAGS, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::core::GUID;

use crate::flight::ScreenRect;
use crate::icon;
use crate::status::TrayState;

/// What a click on a menu item asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    ToggleEnabled,
    OpenSettings,
    /// The Start at login item was clicked, and its check mark now reads
    /// this.
    StartAtLogin(bool),
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    /// The GUID the icon registered under, which is how the notification
    /// addresses it. `None` when registering under it failed, and the icon
    /// registered without one.
    guid: Option<u128>,
    enabled_icon: Icon,
    disabled_icon: Icon,
    toggle: MenuItem,
    settings: MenuItem,
    start_at_login: CheckMenuItem,
    quit: MenuItem,
}

impl Tray {
    /// Must run on a thread that pumps window messages, which then delivers
    /// the menu's clicks to [`MenuEvent::receiver`]. The icon registers under
    /// `guid` when it can, and otherwise without one, which leaves it no
    /// notification.
    pub fn new(state: TrayState, start_at_login: bool, guid: Option<u128>) -> Result<Tray, String> {
        let draw = |enabled| {
            Icon::from_rgba(icon::rgba(enabled), icon::SIZE, icon::SIZE)
                .map_err(|e| format!("cannot make the tray icon: {e}"))
        };
        let (enabled_icon, disabled_icon) = (draw(true)?, draw(false)?);
        let toggle = MenuItem::new(state.toggle_label(), true, None);
        let settings = MenuItem::new("Settings…", true, None);
        let start_at_login = CheckMenuItem::new("Start at login", true, start_at_login, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &toggle,
            &settings,
            &start_at_login,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| format!("cannot build the tray menu: {e}"))?;
        let build = |guid: Option<u128>| {
            let mut builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu.clone()))
                .with_tooltip(state.tooltip())
                .with_icon(if state.enabled {
                    enabled_icon.clone()
                } else {
                    disabled_icon.clone()
                });
            if let Some(guid) = guid {
                builder = builder.with_guid(guid);
            }
            builder
                .build()
                .map_err(|e| format!("cannot create the tray icon: {e}"))
        };
        // `build` succeeds even when Windows refuses the icon, and leaves it
        // to register when the taskbar next starts, so only a call addressed
        // by the GUID shows whether it registered.
        let registered = match guid {
            Some(guid) => {
                let icon = build(Some(guid))?;
                let tooltip = state.tooltip();
                let ok = modify(&icon, guid, NIF_TIP, |nid| {
                    copy_text(&mut nid.szTip, &tooltip)
                });
                ok.then_some((icon, Some(guid)))
            }
            None => None,
        };
        let (icon, guid) = match registered {
            Some(registered) => registered,
            None => (build(None)?, None),
        };
        Ok(Tray {
            icon,
            guid,
            enabled_icon,
            disabled_icon,
            toggle,
            settings,
            start_at_login,
            quit,
        })
    }

    /// Shows a Windows notification from the icon, if it has a GUID, and
    /// says whether Windows took it.
    pub fn notify(&self, title: &str, text: &str) -> bool {
        self.guid.is_some_and(|guid| {
            modify(&self.icon, guid, NIF_INFO, |nid| {
                copy_text(&mut nid.szInfoTitle, title);
                copy_text(&mut nid.szInfo, text);
                nid.dwInfoFlags = NIIF_INFO;
            })
        })
    }

    pub fn show(&self, state: TrayState) {
        let icon = if state.enabled {
            &self.enabled_icon
        } else {
            &self.disabled_icon
        };
        // A refused update leaves the previous look, and the next change
        // tries again.
        let _ = self.icon.set_icon(Some(icon.clone()));
        let _ = self.icon.set_tooltip(Some(state.tooltip()));
        self.toggle.set_text(state.toggle_label());
    }

    /// Where the icon is on screen, when Windows says. For an icon in the
    /// overflow area that is somewhere off the taskbar, or nowhere.
    pub fn rect(&self) -> Option<ScreenRect> {
        self.icon.rect().map(|rect| ScreenRect {
            left: rect.position.x,
            top: rect.position.y,
            width: rect.size.width.into(),
            height: rect.size.height.into(),
        })
    }

    pub fn set_start_at_login(&self, on: bool) {
        self.start_at_login.set_checked(on);
    }

    pub fn action(&self, event: &MenuEvent) -> Option<MenuAction> {
        if event.id == *self.toggle.id() {
            Some(MenuAction::ToggleEnabled)
        } else if event.id == *self.settings.id() {
            Some(MenuAction::OpenSettings)
        } else if event.id == *self.start_at_login.id() {
            // The menu flips the check mark itself before reporting the click.
            Some(MenuAction::StartAtLogin(self.start_at_login.is_checked()))
        } else if event.id == *self.quit.id() {
            Some(MenuAction::Quit)
        } else {
            None
        }
    }
}

/// Sends `NIM_MODIFY` for `flags` to the icon registered under `guid`, with
/// `fill` setting what they name, and says whether Windows took it.
fn modify(
    icon: &TrayIcon,
    guid: u128,
    flags: NOTIFY_ICON_DATA_FLAGS,
    fill: impl FnOnce(&mut NOTIFYICONDATAW),
) -> bool {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: HWND(icon.window_handle()),
        uFlags: flags | NIF_GUID,
        guidItem: GUID::from_u128(guid),
        ..NOTIFYICONDATAW::default()
    };
    fill(&mut nid);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool()
}

/// Copies `text` into a fixed-size field as UTF-16, cut short to leave room
/// for the terminating NUL.
fn copy_text(field: &mut [u16], text: &str) {
    let room = field.len() - 1;
    let mut len = 0;
    for (slot, unit) in field[..room].iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
        len += 1;
    }
    field[len] = 0;
}
