//! Platform-independent core of saw-auto-clicker: the button gate, the click
//! scheduler and Jitter, driven through a backend interface so it runs and is
//! tested without an OS.
//!
//! A backend wires it up with [`spawn`]: the scheduler thread gets the
//! backend's [`Inject`] and [`Observe`] capabilities and a [`Clock`], and the
//! returned [`ButtonGate`] goes to the backend's observing thread.

mod backend;
mod clock;
mod gate;
mod jitter;
mod runtime;
mod scheduler;
mod settings;

#[cfg(test)]
mod tests;

pub use backend::{HotkeyError, Inject, Observe, RegisterHotkey};
pub use clock::{Clock, StdClock, Time};
pub use gate::{Button, ButtonEvent, ButtonGate, Direction, Source, Verdict};
pub use runtime::{Controller, SchedulerHandle, spawn};
pub use scheduler::Status;
pub use settings::{
    ACTIVATION_DELAY_RANGE, BUTTERFLY_CPS, CPS_RANGE, ClickButtons, Hotkey, Key, Modifiers,
    Settings, SettingsError,
};
