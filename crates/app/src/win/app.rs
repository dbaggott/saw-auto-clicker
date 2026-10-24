//! The running app: the clicker, the hotkey, the tray, and the main thread's
//! message loop that the tray needs.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use saw_core::{Hotkey, Settings, Status};
use saw_windows::{Clicker, HotkeyThread, ThrottlingOptOuts};
use tray_icon::TrayIconEvent;
use tray_icon::menu::MenuEvent;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_REMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP, WM_QUIT,
};

use crate::apply::apply;
use crate::config;
use crate::status::{BurstWatch, TrayState};
use crate::win::autostart::StartAtLogin;
use crate::win::instance::show_error;
use crate::win::tray::{MenuAction, Tray};

/// What the clicker's threads tell the main thread.
enum Event {
    Status(Status),
    BurstStarted,
}

/// Sends to the main thread and wakes its message loop, from any thread
/// without blocking.
#[derive(Clone)]
struct ToMain {
    events: Sender<Event>,
    thread_id: u32,
}

impl ToMain {
    fn send(&self, event: Event) {
        // Both fail only once the main thread has stopped listening, during
        // quit, when nothing is left to tell.
        if self.events.send(event).is_ok() {
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_APP, WPARAM(0), LPARAM(0)) };
        }
    }
}

/// What the main loop does after handling what arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Wait,
    OpenSettings,
    Quit,
}

/// Whether applied settings are written to the settings file. The self-test
/// applies settings without touching the user's file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    Yes,
    No,
}

pub struct App {
    pub settings: Settings,
    pub active_hotkey: Option<Hotkey>,
    pub hotkey_problem: Option<String>,
    pub config_problem: Option<String>,
    pub save_problem: Option<String>,
    config_path: Option<PathBuf>,
    persist: Persist,
    tray_state: TrayState,
    tray: Tray,
    start_at_login: StartAtLogin,
    events: Receiver<Event>,
    // Dropped before the clicker in `quit`, so no toggle arrives mid-shutdown.
    hotkeys: Option<HotkeyThread>,
    clicker: Option<Clicker>,
}

impl App {
    /// Starts everything and arms (Q-b). Runs on the main thread, which must
    /// then pump messages: [`App::run`] does.
    pub fn start(persist: Persist) -> Result<App, String> {
        let config_path = config::default_path();
        let loaded = match &config_path {
            Some(path) => config::load(path),
            None => config::Loaded {
                settings: Settings::default(),
                problem: Some("APPDATA is not set, so settings can't be saved".into()),
            },
        };
        let (events_tx, events) = mpsc::channel();
        let to_main = ToMain {
            events: events_tx,
            thread_id: unsafe { GetCurrentThreadId() },
        };
        let status_to_main = to_main.clone();
        let mut watch = BurstWatch::default();
        let clicker = Clicker::start(
            loaded.settings.clone(),
            ThrottlingOptOuts::default(),
            move |status| status_to_main.send(Event::Status(status)),
            move |event, verdict| {
                if watch.burst_started(event, verdict) {
                    to_main.send(Event::BurstStarted);
                }
            },
        )
        .map_err(|e| format!("cannot start clicking: {e}"))?;
        let mut hotkeys = HotkeyThread::spawn(clicker.controller())
            .map_err(|e| format!("cannot start the hotkey: {e}"))?;
        let applied = apply(
            &loaded.settings,
            None,
            loaded.settings.clone(),
            &mut hotkeys,
        );
        let start_at_login = StartAtLogin::for_this_user();
        let tray_state = TrayState {
            armed: false,
            elevated_warning: false,
            hotkey_missing: applied.active_hotkey.is_none(),
        };
        let tray = Tray::new(tray_state, start_at_login.is_enabled().unwrap_or(false))?;
        let app = App {
            settings: applied.settings,
            active_hotkey: applied.active_hotkey,
            hotkey_problem: applied.hotkey_problem,
            config_problem: loaded.problem,
            save_problem: None,
            config_path,
            persist,
            tray_state,
            tray,
            start_at_login,
            events,
            hotkeys: Some(hotkeys),
            clicker: Some(clicker),
        };
        app.clicker().controller().set_armed(true);
        Ok(app)
    }

    fn clicker(&self) -> &Clicker {
        self.clicker.as_ref().expect("present until quit")
    }

    pub fn armed(&self) -> bool {
        self.tray_state.armed
    }

    /// The main thread's loop: tray clicks, the clicker's reports, and the
    /// settings window when asked for, until Quit.
    pub fn run(mut self) -> Result<(), String> {
        loop {
            match self.handle_arrivals() {
                Next::Wait => {}
                Next::OpenSettings => {
                    if crate::win::settings_window::show(&mut self, None)? == Next::Quit {
                        break;
                    }
                    continue;
                }
                Next::Quit => break,
            }
            let mut msg = MSG::default();
            let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            // 0 is WM_QUIT and -1 an invalid call; either way the loop is
            // over.
            if got.0 <= 0 {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        self.quit()
    }

    /// Handles everything that has arrived without waiting, and says what
    /// the caller's loop should do next.
    pub fn handle_arrivals(&mut self) -> Next {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(Status::Armed) => self.tray_state.armed = true,
                Event::Status(Status::Disarmed) => self.tray_state.armed = false,
                Event::Status(Status::TargetNotInjectable(_)) => {
                    self.tray_state.elevated_warning = true;
                }
                Event::BurstStarted => self.tray_state.elevated_warning = false,
            }
            self.tray.show(self.tray_state);
        }
        // Only the menu matters; icon clicks are drained so they don't pile up.
        while TrayIconEvent::receiver().try_recv().is_ok() {}
        let mut next = Next::Wait;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match self.tray.action(&event) {
                Some(MenuAction::ToggleArmed) => self.clicker().controller().toggle_armed(),
                Some(MenuAction::OpenSettings) if next == Next::Wait => next = Next::OpenSettings,
                Some(MenuAction::StartAtLogin(on)) => self.set_start_at_login(on),
                Some(MenuAction::Quit) => next = Next::Quit,
                Some(MenuAction::OpenSettings) | None => {}
            }
        }
        next
    }

    fn set_start_at_login(&mut self, on: bool) {
        let result = if on {
            std::env::current_exe().and_then(|exe| self.start_at_login.enable(&exe))
        } else {
            self.start_at_login.disable()
        };
        if let Err(e) = result {
            show_error(&format!("Start at login could not be changed: {e}"));
        }
        // The check mark shows what the registry now holds, whatever was
        // asked for.
        self.tray
            .set_start_at_login(self.start_at_login.is_enabled().unwrap_or(false));
    }

    /// Puts the settings window's settings into effect and saves them (R14,
    /// R15). They apply from the next press.
    pub fn apply_settings(&mut self, requested: Settings) {
        let hotkeys = self.hotkeys.as_mut().expect("present until quit");
        let applied = apply(&self.settings, self.active_hotkey, requested, hotkeys);
        self.clicker()
            .controller()
            .set_settings(applied.settings.clone());
        self.settings = applied.settings;
        self.active_hotkey = applied.active_hotkey;
        self.hotkey_problem = applied.hotkey_problem;
        self.tray_state.hotkey_missing = self.active_hotkey.is_none();
        self.tray.show(self.tray_state);
        self.save_problem = match (&self.config_path, self.persist) {
            (_, Persist::No) => None,
            (None, Persist::Yes) => Some("APPDATA is not set, so settings can't be saved".into()),
            (Some(path), Persist::Yes) => config::save(path, &self.settings)
                .err()
                .map(|e| format!("Settings could not be saved to {}: {e}", path.display())),
        };
    }

    /// Pumps messages until the clicker reports armed, for the self-test.
    pub fn wait_until_armed(&mut self, deadline: Duration) -> Result<(), String> {
        let until = Instant::now() + deadline;
        while !self.armed() {
            if Instant::now() > until {
                return Err(format!("not armed within {deadline:?}"));
            }
            pump_waiting_messages();
            self.handle_arrivals();
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    /// Stops the clicker before its hook goes (D7), then removes the tray
    /// icon.
    pub fn quit(mut self) -> Result<(), String> {
        drop(self.hotkeys.take());
        let stopped = self.clicker.take().expect("present until quit").shutdown();
        stopped.map_err(|_| "the scheduler thread panicked".to_string())
    }
}

fn pump_waiting_messages() {
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
        if msg.message == WM_QUIT {
            return;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
