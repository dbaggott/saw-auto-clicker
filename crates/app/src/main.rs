//! saw-auto-clicker's tray app: arms the clicker at launch unless it was left
//! disarmed, shows its state in the notification area, and keeps its settings
//! and that state in a per-user file.

// No console window for a tray app.
#![cfg_attr(windows, windows_subsystem = "windows")]

// Portable so their tests run anywhere; only the Windows code calls them.
#[cfg_attr(not(windows), allow(dead_code))]
mod apply;
#[cfg_attr(not(windows), allow(dead_code))]
mod config;
#[cfg_attr(not(windows), allow(dead_code))]
mod form;
#[cfg_attr(not(windows), allow(dead_code))]
mod hotkey_text;
#[cfg_attr(not(windows), allow(dead_code))]
mod icon;
#[cfg_attr(not(windows), allow(dead_code))]
mod status;

#[cfg(windows)]
mod win;

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    win::main()
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("saw-app runs on Windows only");
    ExitCode::FAILURE
}
