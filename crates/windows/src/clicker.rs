use std::io;
use std::mem::size_of;
use std::sync::Arc;

use saw_core::{
    Button, ButtonEvent, Controller, Direction, Inject, Observe, SchedulerHandle, Settings, Status,
    Verdict,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEINPUT, SendInput,
};

use crate::ThrottlingOptOuts;
use crate::clock::WaitableTimerClock;
use crate::hook::{HookThread, Reinstaller};
use crate::tag::OWN_EVENT_TAG;
use crate::target::TargetCheck;
use crate::throttling;

/// The core running on real Windows input: the hook thread observing, the
/// scheduler injecting on a high-resolution timer. Starts disabled.
pub struct Clicker {
    scheduler: SchedulerHandle,
    hook: HookThread,
    throttling: ThrottlingOptOuts,
}

impl Clicker {
    /// `on_status` runs on the scheduler thread and `on_observed` inside the
    /// hook callback, so neither may block. `on_observed` sees every left and
    /// right event with the gate's verdict.
    pub fn start(
        settings: Settings,
        throttling: ThrottlingOptOuts,
        on_status: impl FnMut(Status) + Send + 'static,
        on_observed: impl FnMut(ButtonEvent, Verdict) + Send + 'static,
    ) -> io::Result<Clicker> {
        let throttling = throttling::opt_out(throttling);
        let mut hook = HookThread::spawn()?;
        let backend = Backend {
            reinstaller: hook.reinstaller(),
            target: TargetCheck::new()?,
        };
        let (gate, scheduler) = saw_core::spawn(
            backend,
            Arc::new(WaitableTimerClock::new()?),
            settings,
            on_status,
        )?;
        if let Err(e) = hook.observe(gate, Box::new(on_observed)) {
            // Disabled with nothing observed, so the scheduler has no burst
            // to close and exits at once.
            let _ = scheduler.shutdown();
            return Err(e);
        }
        Ok(Clicker {
            scheduler,
            hook,
            throttling,
        })
    }

    /// The throttling opt-outs Windows accepted, which can be fewer than
    /// were asked for on a Windows version that lacks a mechanism.
    pub fn throttling_opt_outs(&self) -> ThrottlingOptOuts {
        self.throttling
    }

    pub fn controller(&self) -> Controller {
        self.scheduler.controller()
    }

    /// Stops the scheduler while the hook still observes, so a burst's final
    /// down can pass the gate (D7), then removes the hook.
    pub fn shutdown(self) -> std::thread::Result<()> {
        let result = self.scheduler.shutdown();
        drop(self.hook);
        result
    }
}

struct Backend {
    reinstaller: Reinstaller,
    target: TargetCheck,
}

impl Observe for Backend {
    fn reinstall(&mut self) {
        self.reinstaller.request();
    }
}

impl Inject for Backend {
    fn inject(&mut self, button: Button, direction: Direction) {
        let flags = match (button, direction) {
            (Button::Left, Direction::Down) => MOUSEEVENTF_LEFTDOWN,
            (Button::Left, Direction::Up) => MOUSEEVENTF_LEFTUP,
            (Button::Right, Direction::Down) => MOUSEEVENTF_RIGHTDOWN,
            (Button::Right, Direction::Up) => MOUSEEVENTF_RIGHTUP,
        };
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: flags,
                    dwExtraInfo: OWN_EVENT_TAG,
                    ..MOUSEINPUT::default()
                },
            },
        };
        // An injection that never reaches the hook, refused here or blocked
        // silently by UIPI (C5), never echoes, and the scheduler's liveness
        // check handles that the same way as a dead hook (D8).
        unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    }

    fn can_inject_at_cursor(&mut self) -> bool {
        self.target.target_injectable()
    }
}
