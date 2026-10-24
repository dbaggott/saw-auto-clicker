use std::ops::{Add, Sub};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// A point on a [`Clock`]'s monotonic timeline, held as the time since the
/// clock's epoch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time(Duration);

impl Time {
    pub const ZERO: Time = Time(Duration::ZERO);

    pub const fn from_epoch(since_epoch: Duration) -> Time {
        Time(since_epoch)
    }

    pub const fn since_epoch(self) -> Duration {
        self.0
    }
}

impl Add<Duration> for Time {
    type Output = Time;

    fn add(self, rhs: Duration) -> Time {
        Time(self.0 + rhs)
    }
}

impl Sub<Duration> for Time {
    type Output = Time;

    fn sub(self, rhs: Duration) -> Time {
        Time(self.0 - rhs)
    }
}

/// The span from `rhs` to `self`, or zero when `rhs` is later, as
/// [`Instant::duration_since`] does.
impl Sub<Time> for Time {
    type Output = Duration;

    fn sub(self, rhs: Time) -> Duration {
        self.0.saturating_sub(rhs.0)
    }
}

/// The time source the scheduler thread runs on (R9), and the way the
/// observing thread wakes it when the button gate queues an event.
///
/// A backend supplies one whose waits are precise enough for the click rate:
/// on Windows, a high-resolution waitable timer (R12).
pub trait Clock: Send + Sync {
    fn now(&self) -> Time;

    /// Blocks until `deadline`, or indefinitely when it is `None`, returning
    /// early once [`Clock::wake`] is called. A wake that arrives while no wait
    /// is in progress ends the next wait at once. Spurious returns are allowed.
    /// Only the scheduler thread calls this.
    fn wait_until(&self, deadline: Option<Time>);

    /// Ends the current or next [`Clock::wait_until`]. The observing thread
    /// calls this from inside its callback, so it must return promptly (C1).
    fn wake(&self);
}

/// A portable [`Clock`] on [`Instant`] and a condition variable. Its waits are
/// only as precise as the OS's default timer, which on Windows is too coarse
/// for high click rates (C3), so the Windows backend brings its own.
pub struct StdClock {
    epoch: Instant,
    woken: Mutex<bool>,
    wakeup: Condvar,
}

impl StdClock {
    pub fn new() -> StdClock {
        StdClock {
            epoch: Instant::now(),
            woken: Mutex::new(false),
            wakeup: Condvar::new(),
        }
    }
}

impl Default for StdClock {
    fn default() -> StdClock {
        StdClock::new()
    }
}

impl Clock for StdClock {
    fn now(&self) -> Time {
        Time(self.epoch.elapsed())
    }

    fn wait_until(&self, deadline: Option<Time>) {
        // The flag is a plain bool, so a panic elsewhere cannot leave it
        // inconsistent; a poisoned lock is safe to keep using.
        let mut woken = self.woken.lock().unwrap_or_else(PoisonError::into_inner);
        while !*woken {
            match deadline {
                None => {
                    woken = self
                        .wakeup
                        .wait(woken)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                Some(deadline) => {
                    let now = self.now();
                    if now >= deadline {
                        return;
                    }
                    woken = self
                        .wakeup
                        .wait_timeout(woken, deadline - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        }
        *woken = false;
    }

    fn wake(&self) {
        *self.woken.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.wakeup.notify_one();
    }
}
