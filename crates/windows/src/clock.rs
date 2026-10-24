use std::io;
use std::time::{Duration, Instant};

use saw_core::{Clock, Time};
use windows::Win32::Foundation::{WAIT_EVENT, WAIT_FAILED};
use windows::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventW, CreateWaitableTimerExW, INFINITE,
    SetEvent, SetWaitableTimer, TIMER_ALL_ACCESS, WaitForMultipleObjects, WaitForSingleObject,
};

use crate::handle::OwnedHandle;

/// The core's [`Clock`] on a high-resolution waitable timer (R12, C3).
///
/// The scheduler hands it absolute deadlines on its own grid, so per-click
/// error never accumulates. Each wait sets the timer relative to now, because
/// the timer's own absolute due times are wall-clock time, which can jump.
pub struct WaitableTimerClock {
    epoch: Instant,
    timer: OwnedHandle,
    /// Auto-reset, so a wake that lands between waits ends the next one.
    wakeup: OwnedHandle,
}

impl WaitableTimerClock {
    /// Fails before Windows 10 1803, which lacks high-resolution timers.
    pub fn new() -> io::Result<WaitableTimerClock> {
        unsafe {
            let timer = CreateWaitableTimerExW(
                None,
                None,
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS.0,
            )?;
            let timer = OwnedHandle::new(timer);
            let wakeup = OwnedHandle::new(CreateEventW(None, false, false, None)?);
            Ok(WaitableTimerClock {
                epoch: Instant::now(),
                timer,
                wakeup,
            })
        }
    }

    fn wait_for(&self, span: Duration) {
        // Negative due times are relative, in 100 ns units. Rounding up keeps
        // the wait from ending just short of the deadline and spinning.
        let ticks = i64::try_from(span.as_nanos().div_ceil(100)).unwrap_or(i64::MAX);
        let due = -ticks.max(1);
        unsafe {
            // Setting and waiting on handles this clock owns fails only if they
            // are invalid, which would leave the scheduler unable to keep time.
            SetWaitableTimer(self.timer.raw(), &due, 0, None, None, false)
                .expect("setting the click timer failed");
            wait_checked(WaitForMultipleObjects(
                &[self.wakeup.raw(), self.timer.raw()],
                false,
                INFINITE,
            ));
        }
    }
}

fn wait_checked(result: WAIT_EVENT) {
    assert_ne!(
        result,
        WAIT_FAILED,
        "clock wait failed: {}",
        io::Error::last_os_error()
    );
}

impl Clock for WaitableTimerClock {
    fn now(&self) -> Time {
        Time::from_epoch(self.epoch.elapsed())
    }

    fn wait_until(&self, deadline: Option<Time>) {
        match deadline {
            None => unsafe { wait_checked(WaitForSingleObject(self.wakeup.raw(), INFINITE)) },
            Some(deadline) => {
                let now = self.now();
                if now < deadline {
                    self.wait_for(deadline - now);
                }
            }
        }
    }

    fn wake(&self) {
        // Setting an event this clock owns cannot fail while it is open.
        let _ = unsafe { SetEvent(self.wakeup.raw()) };
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn a_wait_ends_at_its_deadline() {
        let clock = WaitableTimerClock::new().unwrap();
        let deadline = clock.now() + Duration::from_millis(20);
        clock.wait_until(Some(deadline));
        let now = clock.now();
        assert!(
            now >= deadline - Duration::from_micros(100),
            "woke {:?} early",
            deadline - now
        );
    }

    #[test]
    fn a_wake_ends_an_unbounded_wait() {
        let clock = Arc::new(WaitableTimerClock::new().unwrap());
        let waker = Arc::clone(&clock);
        let thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            waker.wake();
        });
        clock.wait_until(None);
        thread.join().unwrap();
    }

    #[test]
    fn a_wake_between_waits_ends_the_next_one() {
        let clock = WaitableTimerClock::new().unwrap();
        clock.wake();
        let start = clock.now();
        clock.wait_until(Some(start + Duration::from_secs(10)));
        assert!(clock.now() - start < Duration::from_secs(5));
    }

    #[test]
    fn a_past_deadline_returns_at_once() {
        let clock = WaitableTimerClock::new().unwrap();
        clock.wait_until(Some(Time::ZERO));
    }
}
