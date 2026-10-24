//! Windows backend of saw-auto-clicker: a low-level mouse hook on its own
//! thread, tagged `SendInput` injection, a high-resolution waitable timer,
//! the toggle hotkey and the elevated-target check, wired to the core by
//! [`Clicker`]. Win32 code lives behind `cfg(windows)` so the workspace still
//! checks on other hosts.

// Portable so their tests run anywhere; only the Windows code calls them.
#[cfg_attr(not(windows), allow(dead_code))]
mod hotkey_slot;
#[cfg_attr(not(windows), allow(dead_code))]
mod tag;
#[cfg_attr(not(windows), allow(dead_code))]
mod target;

#[cfg(windows)]
mod clicker;
#[cfg(windows)]
mod clock;
#[cfg(windows)]
mod handle;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
mod throttling;

#[cfg(windows)]
pub use clicker::Clicker;
#[cfg(windows)]
pub use clock::WaitableTimerClock;
#[cfg(windows)]
pub use hotkey::HotkeyThread;

/// Which kinds of Windows power throttling the process turns off at start
/// (R12). Whether either is needed alongside the high-resolution timer is
/// unverified until A1 runs, so both default to on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThrottlingOptOuts {
    /// Execution-speed throttling (EcoQoS).
    pub execution_speed: bool,
    /// Ignoring timer-resolution requests while the process has no visible
    /// window.
    pub timer_resolution: bool,
}

impl Default for ThrottlingOptOuts {
    fn default() -> ThrottlingOptOuts {
        ThrottlingOptOuts {
            execution_speed: true,
            timer_resolution: true,
        }
    }
}
