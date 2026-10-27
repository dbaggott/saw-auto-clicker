//! The app on Windows.

mod app;
mod autostart;
mod butterfly;
mod fly_to_tray;
mod instance;
mod settings_window;
mod tray;

use std::process::ExitCode;
use std::time::Duration;

use app::{App, Next, Persist};
use autostart::AT_LOGIN;
use instance::{SingleInstance, ask_running_instance_to_show_settings, show_error, show_info};
use saw_core::{Hotkey, Key, Modifiers};

use crate::flight::{self, ScreenRect};

const USAGE: &str = "Usage: saw-app [--at-login | --self-test | --version]";

pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => run(Launch::ByHand),
        [AT_LOGIN] => run(Launch::AtLogin),
        ["--self-test"] => self_test(),
        // Before the single-instance check and the hook, so CI's installer
        // check can run it anywhere and read its exit code.
        // A GUI-subsystem process has no console, so the line is seen only
        // when stdout is redirected.
        ["--version"] => {
            println!("saw-app {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            show_error(USAGE);
            ExitCode::FAILURE
        }
    }
}

/// Who started the app. Someone who starts it by hand gets its settings
/// window, so they can see it started and where it went.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Launch {
    ByHand,
    /// By the Start at login entry, which starts it quietly in the tray.
    AtLogin,
}

fn run(launch: Launch) -> ExitCode {
    let _instance = match SingleInstance::claim() {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            if launch == Launch::ByHand && ask_running_instance_to_show_settings().is_err() {
                show_info(
                    "saw-auto-clicker is already running. Its saw-blade icon is in the \
                     system tray, by the clock.",
                );
            }
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            show_error(&format!("cannot check for another instance: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let first = match launch {
        Launch::ByHand => Next::OpenSettings,
        Launch::AtLogin => Next::Wait,
    };
    match App::start(Persist::Yes).and_then(|app| app.run(first)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            show_error(&e);
            ExitCode::FAILURE
        }
    }
}

/// `--self-test`: starts the whole app, waits for it to enable, opens and
/// closes the settings window twice with the tray running, playing the
/// flight to the tray after each close, applies the settings with
/// Ctrl+Alt+F8 and then as loaded, and quits through the same path as the
/// tray's Quit. Reports each step on stdout and exits non-zero
/// on the first failure. It never writes the settings file or the registry,
/// and refuses to run while another instance is running, which it would
/// otherwise disturb.
fn self_test() -> ExitCode {
    let result = (|| {
        let _instance = SingleInstance::claim()
            .map_err(|e| format!("cannot check for another instance: {e}"))?
            .ok_or("saw-auto-clicker is running; quit it before the self-test")?;
        let mut app = App::start(Persist::No)?;
        println!("started: tray icon created");
        app.wait_until_enabled(Duration::from_secs(5))?;
        println!("enabled");
        for round in 1..=2 {
            let closed = settings_window::show(&mut app, Some(30))?;
            println!("settings window {round}: drew 30 frames and closed");
            // Played directly, since `fly` skips it wherever animation
            // effects are off or there is no taskbar.
            let window = closed
                .rect
                .ok_or("the settings window's position was never reported")?;
            let corner = ScreenRect {
                width: 16.0,
                height: 16.0,
                ..window
            };
            fly_to_tray::play(window, corner, flight::START_SIZE * closed.scale, true)
                .map_err(|e| format!("the flight to the tray failed: {e}"))?;
            println!("flight {round}: played");
            app.handle_arrivals();
        }
        let loaded = app.settings.clone();
        // A hotkey of its own, since the loaded settings may have none and
        // registering one is what the hotkey path is for.
        let ctrl_alt_f8 = Hotkey {
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::default()
            },
            key: Key::F8,
        };
        let with_hotkey = loaded.clone().with_hotkey(Some(ctrl_alt_f8));
        for settings in [with_hotkey, loaded] {
            app.apply_settings(settings, app.enable_on_startup);
            if let Some(problem) = &app.hotkey_problem {
                // A hotkey already held elsewhere on this machine is not a
                // fault in the app.
                println!("note: {problem}");
            }
        }
        app.quit()?;
        println!("quit cleanly");
        Ok::<(), String>(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("saw-app self-test: {e}");
            ExitCode::FAILURE
        }
    }
}
