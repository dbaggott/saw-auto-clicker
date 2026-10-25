//! The app on Windows.

mod app;
mod autostart;
mod instance;
mod settings_window;
mod tray;

use std::process::ExitCode;
use std::time::Duration;

use app::{App, Persist};
use instance::{SingleInstance, show_error, show_info};
use saw_core::{Hotkey, Key, Modifiers, Settings};

const USAGE: &str = "Usage: saw-app [--self-test | --version]";

pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => run(),
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

fn run() -> ExitCode {
    let _instance = match SingleInstance::claim() {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            show_info("saw-auto-clicker is already running. Its icon is in the notification area.");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            show_error(&format!("cannot check for another instance: {e}"));
            return ExitCode::FAILURE;
        }
    };
    match App::start(Persist::Yes).and_then(App::run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            show_error(&e);
            ExitCode::FAILURE
        }
    }
}

/// `--self-test`: starts the whole app, waits for it to arm, opens and
/// closes the settings window twice with the tray running, applies the
/// settings with Ctrl+Alt+F8 and then as loaded, and quits through the same
/// path as the tray's Quit. Reports each step on stdout and exits non-zero
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
        app.wait_until_armed(Duration::from_secs(5))?;
        println!("armed");
        for round in 1..=2 {
            settings_window::show(&mut app, Some(30))?;
            println!("settings window {round}: drew 30 frames and closed");
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
        let with_hotkey = Settings::new(
            loaded.cps(),
            loaded.activation_delay(),
            loaded.jitter(),
            Some(ctrl_alt_f8),
        )
        .map_err(|e| e.to_string())?;
        for settings in [with_hotkey, loaded] {
            app.apply_settings(settings, app.arm_on_startup);
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
