//! The backend interface (D2): the three capabilities that differ by OS.
//!
//! - **Observe.** The backend's observing thread owns the
//!   [`ButtonGate`](crate::ButtonGate), calls
//!   [`ButtonGate::observe`](crate::ButtonGate::observe) synchronously for
//!   every left and right button event, and applies the verdict it returns.
//!   Other mouse events never reach the gate. [`Observe`] is the part the
//!   scheduler calls.
//! - **Inject.** [`Inject`], called by the scheduler.
//! - **Hotkey.** [`RegisterHotkey`], called by the app. The backend delivers
//!   each press by calling [`Controller::toggle_armed`](crate::Controller::toggle_armed).
//!
//! The core makes no OS calls itself: it reaches the OS only through these
//! traits and a [`Clock`](crate::Clock).

use std::fmt;

use crate::gate::{Button, Direction};
use crate::settings::Hotkey;

pub trait Observe {
    /// Replaces the observation (D8). On the observing thread, in this order:
    /// stop observing, call
    /// [`ButtonGate::reset_delivered`](crate::ButtonGate::reset_delivered),
    /// start observing again, so the reset lands before the new observation's
    /// first event. May return before that completes.
    fn reinstall(&mut self);
}

pub trait Inject {
    /// Injects a down or up for `button`, tagged as the app's own (D3) so the
    /// observing thread reports it as [`Source::Tagged`](crate::Source::Tagged).
    fn inject(&mut self, button: Button, direction: Direction);

    /// Whether an injection now would reach the window under the cursor (R16).
    fn can_inject_at_cursor(&mut self) -> bool;
}

pub trait RegisterHotkey {
    /// Registers `hotkey` as the toggle, replacing the current one, or with
    /// `None` leaves no toggle registered. On error the current one stays
    /// registered (R15).
    fn register(&mut self, hotkey: Option<Hotkey>) -> Result<(), HotkeyError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    /// Another program holds the combination (C4).
    Taken,
    /// Any other refusal, with the OS's reason.
    Failed(String),
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HotkeyError::Taken => f.write_str("another program is using that hotkey"),
            HotkeyError::Failed(reason) => {
                write!(f, "the hotkey could not be registered: {reason}")
            }
        }
    }
}

impl std::error::Error for HotkeyError {}
