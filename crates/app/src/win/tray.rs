//! The tray icon and its menu (R14).

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::icon;
use crate::status::TrayState;

/// What a click on a menu item asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    ToggleArmed,
    OpenSettings,
    /// The Start at login item was clicked, and its check mark now reads
    /// this.
    StartAtLogin(bool),
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    armed_icon: Icon,
    disarmed_icon: Icon,
    toggle: MenuItem,
    settings: MenuItem,
    start_at_login: CheckMenuItem,
    quit: MenuItem,
}

impl Tray {
    /// Must run on a thread that pumps window messages, which then delivers
    /// the menu's clicks to [`MenuEvent::receiver`].
    pub fn new(state: TrayState, start_at_login: bool) -> Result<Tray, String> {
        let draw = |armed| {
            Icon::from_rgba(icon::rgba(armed), icon::SIZE, icon::SIZE)
                .map_err(|e| format!("cannot make the tray icon: {e}"))
        };
        let (armed_icon, disarmed_icon) = (draw(true)?, draw(false)?);
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
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(state.tooltip())
            .with_icon(if state.armed {
                armed_icon.clone()
            } else {
                disarmed_icon.clone()
            })
            .build()
            .map_err(|e| format!("cannot create the tray icon: {e}"))?;
        Ok(Tray {
            icon,
            armed_icon,
            disarmed_icon,
            toggle,
            settings,
            start_at_login,
            quit,
        })
    }

    pub fn show(&self, state: TrayState) {
        let icon = if state.armed {
            &self.armed_icon
        } else {
            &self.disarmed_icon
        };
        // A refused update leaves the previous look, and the next change
        // tries again.
        let _ = self.icon.set_icon(Some(icon.clone()));
        let _ = self.icon.set_tooltip(Some(state.tooltip()));
        self.toggle.set_text(state.toggle_label());
    }

    pub fn set_start_at_login(&self, on: bool) {
        self.start_at_login.set_checked(on);
    }

    pub fn action(&self, event: &MenuEvent) -> Option<MenuAction> {
        if event.id == *self.toggle.id() {
            Some(MenuAction::ToggleArmed)
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
