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
use crate::status::{BurstWatch, TrayState, hotkey_missing};
use crate::win::autostart::StartAtLogin;
use crate::win::fly_to_tray;
use crate::win::instance::{listen_for_show_settings, show_error};
use crate::win::settings_window::{self, Closed};
use crate::win::tray::{MenuAction, Tray};

/// What the clicker's threads, and a second launch, tell the main thread.
enum Event {
    Status(Status),
    BurstStarted,
    ShowSettings,
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
        // The send fails only once quit has dropped the receiver. A lost
        // wake-up, as in a modal loop, only delays handling to the next
        // message the main thread gets.
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

/// Whether the app leaves anything behind: the settings file, the Start at
/// login value, and the signal a second launch sends. The self-test enables
/// even when the settings file says not to, and touches none of them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Persist {
    Yes,
    No,
}

pub struct App {
    pub settings: Settings,
    pub enable_on_startup: bool,
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
    /// Starts everything, enabling on the first run and otherwise only when
    /// the settings file says to enable on startup; the state the app was left
    /// in last time doesn't count. The first run writes the file, so the
    /// next start is not a first run. Runs on the main thread, which must
    /// then pump messages: [`App::run`] does.
    pub fn start(persist: Persist) -> Result<App, String> {
        let config_path = config::default_path();
        let loaded = match &config_path {
            Some(path) => config::load(path),
            None => config::Loaded::defaults(Some(
                "APPDATA is not set, so settings can't be saved".into(),
            )),
        };
        let (events_tx, events) = mpsc::channel();
        let to_main = ToMain {
            events: events_tx,
            thread_id: unsafe { GetCurrentThreadId() },
        };
        if persist == Persist::Yes {
            let show_to_main = to_main.clone();
            // Without it a second launch shows a message instead of this
            // window, so the app carries on.
            let _ = listen_for_show_settings(move || show_to_main.send(Event::ShowSettings));
        }
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
        // On failure an older value keeps opening the settings window at
        // sign-in, which is all it costs.
        if let (Persist::Yes, Ok(exe)) = (persist, std::env::current_exe()) {
            let _ = start_at_login.update(&exe);
        }
        let tray_state = TrayState {
            enabled: false,
            elevated_warning: false,
            hotkey_missing: hotkey_missing(&applied.settings, applied.active_hotkey),
        };
        let tray = match Tray::new(tray_state, start_at_login_checked(&start_at_login)) {
            Ok(tray) => tray,
            Err(e) => {
                // Still disabled, so nothing is held up; stop in quit's order.
                drop(hotkeys);
                let _ = clicker.shutdown();
                return Err(e);
            }
        };
        let mut app = App {
            settings: applied.settings,
            enable_on_startup: loaded.enable_on_startup,
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
        if loaded.first_run {
            app.save();
        }
        if loaded.first_run || loaded.enable_on_startup || persist == Persist::No {
            app.clicker().controller().set_enabled(true);
        }
        Ok(app)
    }

    fn clicker(&self) -> &Clicker {
        self.clicker.as_ref().expect("present until quit")
    }

    pub fn enabled(&self) -> bool {
        self.tray_state.enabled
    }

    /// The main thread's loop: tray clicks, the clicker's reports, and the
    /// settings window when asked for, until Quit. `first` is what to do
    /// before anything has arrived.
    pub fn run(mut self, first: Next) -> Result<(), String> {
        let mut next = first;
        loop {
            match next {
                Next::Wait => {}
                Next::OpenSettings => {
                    match settings_window::show(&mut self, None) {
                        Ok(Closed {
                            next: Next::Quit, ..
                        }) => break,
                        Ok(Closed {
                            rect: Some(window),
                            scale,
                            ..
                        }) => fly_to_tray::fly(window, self.tray.rect(), scale, self.enabled()),
                        Ok(_) => {}
                        // The tray and the clicker carry on without the window.
                        Err(e) => show_error(&e),
                    }
                    next = self.handle_arrivals();
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
            next = self.handle_arrivals();
        }
        self.quit()
    }

    /// Handles everything that has arrived without waiting, and says what
    /// the caller's loop should do next.
    pub fn handle_arrivals(&mut self) -> Next {
        let shown = self.tray_state;
        let mut enabled_reported = None;
        let mut next = Next::Wait;
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Status(Status::Enabled) => enabled_reported = Some(true),
                Event::Status(Status::Disabled) => enabled_reported = Some(false),
                Event::Status(Status::TargetNotInjectable(_)) => {
                    self.tray_state.elevated_warning = true;
                }
                Event::BurstStarted => self.tray_state.elevated_warning = false,
                Event::ShowSettings => next = Next::OpenSettings,
            }
        }
        if let Some(enabled) = enabled_reported {
            self.tray_state.enabled = enabled;
        }
        if self.tray_state != shown {
            self.tray.show(self.tray_state);
        }
        // Only the menu matters; icon clicks are drained so they don't pile up.
        while TrayIconEvent::receiver().try_recv().is_ok() {}
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match self.tray.action(&event) {
                Some(MenuAction::ToggleEnabled) => self.clicker().controller().toggle_enabled(),
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
            .set_start_at_login(start_at_login_checked(&self.start_at_login));
    }

    /// Puts the settings window's settings into effect and saves them with
    /// whether to enable on startup (R14, R15). They apply from the next press.
    pub fn apply_settings(&mut self, requested: Settings, enable_on_startup: bool) {
        let hotkeys = self.hotkeys.as_mut().expect("present until quit");
        let applied = apply(&self.settings, self.active_hotkey, requested, hotkeys);
        self.clicker()
            .controller()
            .set_settings(applied.settings.clone());
        self.settings = applied.settings;
        self.active_hotkey = applied.active_hotkey;
        self.hotkey_problem = applied.hotkey_problem;
        self.enable_on_startup = enable_on_startup;
        self.tray_state.hotkey_missing = hotkey_missing(&self.settings, self.active_hotkey);
        self.tray.show(self.tray_state);
        self.save();
    }

    /// Writes the current settings to the settings file, unless this run
    /// doesn't persist, and records why when it can't. A successful save
    /// clears the warning about the file as loaded, since every value it
    /// named has been replaced.
    fn save(&mut self) {
        self.save_problem = match (&self.config_path, self.persist) {
            (_, Persist::No) => None,
            (None, Persist::Yes) => Some("APPDATA is not set, so settings can't be saved".into()),
            (Some(path), Persist::Yes) => {
                match config::save(path, &self.settings, self.enable_on_startup) {
                    Ok(()) => {
                        self.config_problem = None;
                        None
                    }
                    Err(e) => Some(format!(
                        "Settings could not be saved to {}: {e}",
                        path.display()
                    )),
                }
            }
        };
    }

    /// Pumps messages until the clicker reports enabled, for the self-test.
    pub fn wait_until_enabled(&mut self, deadline: Duration) -> Result<(), String> {
        let until = Instant::now() + deadline;
        while !self.enabled() {
            if Instant::now() > until {
                return Err(format!("not enabled within {deadline:?}"));
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

/// Checked only when the `Run` value starts this executable, so a value left
/// by a copy elsewhere reads as off and a click rewrites it. An unreadable
/// value reads as off too, and a click replaces it.
fn start_at_login_checked(entry: &StartAtLogin) -> bool {
    std::env::current_exe()
        .and_then(|exe| entry.starts(&exe))
        .unwrap_or(false)
}

/// Dispatches every message waiting for this thread, without waiting for
/// more.
pub fn pump_waiting_messages() {
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
