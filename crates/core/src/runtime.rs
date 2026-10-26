use std::io;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use fastrand::Rng;

use crate::backend::{Inject, Observe};
use crate::clock::Clock;
use crate::gate::{ButtonGate, Inbox};
use crate::scheduler::{Input, Scheduler, Status};
use crate::settings::Settings;

/// Starts the scheduler thread, disabled, and returns the button gate for the
/// backend's observing thread to own, with the handle that controls the
/// scheduler and shuts it down.
///
/// `on_status` runs on the scheduler thread, so it must not block; an app
/// typically forwards each [`Status`] to its own event loop.
pub fn spawn<B>(
    backend: B,
    clock: Arc<dyn Clock>,
    settings: Settings,
    on_status: impl FnMut(Status) + Send + 'static,
) -> io::Result<(ButtonGate, SchedulerHandle)>
where
    B: Observe + Inject + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let inbox = Inbox::new(tx, Arc::clone(&clock));
    let scheduler = Scheduler::new(backend, settings, Rng::new(), Box::new(on_status));
    let thread = thread::Builder::new()
        .name("saw-scheduler".into())
        .spawn(move || scheduler.run(rx, &*clock))?;
    Ok((
        ButtonGate::new(inbox.clone()),
        SchedulerHandle {
            controller: Controller { inbox },
            thread,
        },
    ))
}

/// Enables, disables and reconfigures the scheduler from any thread. None of its
/// calls block.
#[derive(Clone)]
pub struct Controller {
    inbox: Inbox,
}

impl Controller {
    /// Enabling also replaces the observation (D8). Disabling stops injections
    /// at once; a running burst ends at its physical release (R2).
    pub fn set_enabled(&self, enabled: bool) {
        self.inbox.send(Input::SetEnabled(enabled));
    }

    /// What the backend calls on each press of the toggle hotkey.
    pub fn toggle_enabled(&self) {
        self.inbox.send(Input::ToggleEnabled);
    }

    /// Takes effect from the next press. A running burst keeps its interval
    /// and Jitter, so its timing stays on D5's grid.
    pub fn set_settings(&self, settings: Settings) {
        self.inbox.send(Input::SetSettings(settings));
    }
}

/// Owns the scheduler thread.
pub struct SchedulerHandle {
    controller: Controller,
    thread: JoinHandle<()>,
}

impl SchedulerHandle {
    pub fn controller(&self) -> Controller {
        self.controller.clone()
    }

    /// Stops the scheduler. A running burst whose button applications hold
    /// up gets one tagged down first, and this waits up to 100 ms for the gate
    /// to see it, so the physical up that follows still closes the burst (D7).
    /// Call it before removing the observation. Errs only if the scheduler
    /// thread panicked, with that panic's payload.
    pub fn shutdown(self) -> thread::Result<()> {
        self.controller.inbox.send(Input::Shutdown);
        self.thread.join()
    }
}
