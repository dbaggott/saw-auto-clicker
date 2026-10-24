use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use fastrand::Rng;

use crate::backend::{Inject, Observe};
use crate::clock::{Clock, Time};
use crate::gate::{Button, ButtonEvent, Direction, Source};
use crate::jitter::{Cycle, Grid, MIN_PHASE};
use crate::settings::Settings;

/// The floor on how long an injection may go unechoed before the observation
/// counts as dead (D8); the timeout is this or twice the interval.
const MIN_LIVENESS_TIMEOUT: Duration = Duration::from_millis(250);
/// How often the observation is replaced while armed and idle (D8).
const IDLE_REINSTALL_PERIOD: Duration = Duration::from_secs(60);
/// How long shutdown waits for a final tagged down to echo (D7).
const QUIT_GRACE: Duration = Duration::from_millis(100);

/// What the scheduler thread's queue carries.
#[derive(Debug)]
pub(crate) enum Input {
    Observed(ButtonEvent),
    SetArmed(bool),
    ToggleArmed,
    SetSettings(Settings),
    Shutdown,
}

/// Changes the app shows the user, reported to the owner's status callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Armed,
    Disarmed,
    /// A burst did not start, or ended, because the window under the cursor
    /// can't be injected into (R16).
    TargetNotInjectable(Button),
}

/// When the scheduler thread next needs to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wake {
    At(Time),
    OnInput,
    Exit,
}

enum Track {
    /// Not held, as far as the scheduler knows.
    Idle,
    /// Held, waiting out the activation delay, with the settings in force at
    /// the press.
    Pending {
        delay_ends: Time,
        settings: Settings,
    },
    /// Held with no burst running, until the physical up.
    Held,
    Burst(Burst),
    /// Shutting down. Waits for an up still in flight to echo before
    /// injecting the final down, then for that down to echo.
    Closing {
        awaiting: Direction,
    },
}

struct Burst {
    grid: Grid,
    cycle: Cycle,
    /// Which of `cycle`'s injections goes out next.
    next: Direction,
    /// The injection that has not yet echoed through the gate. While it is
    /// set, nothing else is injected for this button (D8's lockstep).
    in_flight: Option<InFlight>,
    liveness_timeout: Duration,
}

#[derive(Clone, Copy)]
struct InFlight {
    direction: Direction,
    sent: Time,
}

impl Burst {
    fn new(start: Time, settings: &Settings, rng: Rng) -> Burst {
        let interval = settings.interval();
        let mut grid = Grid::new(start, interval, settings.jitter(), rng);
        let cycle = grid.next_cycle();
        Burst {
            grid,
            cycle,
            next: Direction::Up,
            in_flight: None,
            liveness_timeout: MIN_LIVENESS_TIMEOUT.max(interval * 2),
        }
    }

    fn liveness_expiry(&self) -> Option<Time> {
        self.in_flight.map(|f| f.sent + self.liveness_timeout)
    }

    fn echoed(&mut self, direction: Direction, now: Time) {
        let Some(echoed) = self.in_flight.filter(|f| f.direction == direction) else {
            return;
        };
        self.in_flight = None;
        if direction == Direction::Down {
            // An up whose deadline passed while this down was unechoed is
            // dropped with its whole cycle, not caught up (D8). So is one that
            // would follow a late down by less than D5's floor.
            let earliest_up = now.max(echoed.sent + MIN_PHASE);
            self.cycle = self.grid.next_cycle();
            while self.cycle.up < earliest_up {
                self.cycle = self.grid.next_cycle();
            }
        }
    }
}

/// Owns click timing (D4, D5, D8) and calls the backend to inject. It runs on
/// its own thread, fed by the button gate and the [`Controller`](crate::Controller)
/// through one queue, and reads the time only through the `now` it is handed,
/// so tests drive it on a fake clock.
pub(crate) struct Scheduler<B> {
    backend: B,
    settings: Settings,
    armed: bool,
    tracks: [Track; 2],
    rng: Rng,
    last_reinstall: Time,
    quit_by: Option<Time>,
    on_status: Box<dyn FnMut(Status) + Send>,
}

impl<B: Observe + Inject> Scheduler<B> {
    pub(crate) fn new(
        backend: B,
        settings: Settings,
        rng: Rng,
        on_status: Box<dyn FnMut(Status) + Send>,
    ) -> Scheduler<B> {
        Scheduler {
            backend,
            settings,
            armed: false,
            tracks: [Track::Idle, Track::Idle],
            rng,
            last_reinstall: Time::ZERO,
            quit_by: None,
            on_status,
        }
    }

    pub(crate) fn run(mut self, inbox: Receiver<Input>, clock: &dyn Clock) {
        loop {
            match self.turn(&inbox, clock.now()) {
                Wake::At(deadline) => clock.wait_until(Some(deadline)),
                Wake::OnInput => clock.wait_until(None),
                Wake::Exit => return,
            }
        }
    }

    /// Handles everything queued, injects whatever is due at `now`, and says
    /// when to run next.
    pub(crate) fn turn(&mut self, inbox: &Receiver<Input>, now: Time) -> Wake {
        loop {
            match inbox.try_recv() {
                Ok(input) => self.handle(input, now),
                Err(TryRecvError::Empty) => break,
                // The gate and every controller are gone, so no release can
                // ever arrive to end a burst.
                Err(TryRecvError::Disconnected) => return Wake::Exit,
            }
        }
        self.poll(now)
    }

    fn handle(&mut self, input: Input, now: Time) {
        match input {
            Input::Observed(event) => self.observed(event, now),
            Input::SetArmed(armed) => self.set_armed(armed, now),
            Input::ToggleArmed => self.set_armed(!self.armed, now),
            Input::SetSettings(settings) => self.settings = settings,
            Input::Shutdown => self.begin_quit(now),
        }
    }

    fn observed(&mut self, event: ButtonEvent, now: Time) {
        let track = &mut self.tracks[event.button.index()];
        match (event.source, event.direction) {
            (Source::Physical, Direction::Down) => {
                if self.quit_by.is_none() {
                    *track = Track::Pending {
                        delay_ends: now + self.settings.activation_delay(),
                        settings: self.settings.clone(),
                    };
                }
            }
            (Source::Physical, Direction::Up) => *track = Track::Idle,
            (Source::Tagged, direction) => match track {
                Track::Burst(burst) => burst.echoed(direction, now),
                Track::Closing { awaiting } if *awaiting == direction => {
                    if direction == Direction::Up {
                        self.backend.inject(event.button, Direction::Down);
                        *awaiting = Direction::Down;
                    } else {
                        *track = Track::Idle;
                    }
                }
                _ => {}
            },
        }
    }

    fn set_armed(&mut self, armed: bool, now: Time) {
        if armed == self.armed || self.quit_by.is_some() {
            return;
        }
        self.armed = armed;
        // A press that spans the change starts no burst. Disarming stops
        // bursts at once (R2); arming leaves a press already held alone,
        // because the reinstall below could lose its release.
        for track in &mut self.tracks {
            if matches!(track, Track::Burst(_) | Track::Pending { .. }) {
                *track = Track::Held;
            }
        }
        if armed {
            self.reinstall(now);
            (self.on_status)(Status::Armed);
        } else {
            (self.on_status)(Status::Disarmed);
        }
    }

    /// Leaves every running burst logically down, so the physical up that
    /// follows the app's unhook still closes it (D7).
    fn begin_quit(&mut self, now: Time) {
        if self.quit_by.is_some() {
            return;
        }
        self.quit_by = Some(now + QUIT_GRACE);
        for button in Button::ALL {
            let track = &mut self.tracks[button.index()];
            let closing = match track {
                Track::Burst(burst) => match (burst.in_flight, burst.next) {
                    // Lockstep holds here too: the down waits for the up's
                    // echo, so a dead observation gets no down it can't close.
                    (Some(f), _) => Some(f.direction),
                    (None, Direction::Down) => {
                        self.backend.inject(button, Direction::Down);
                        Some(Direction::Down)
                    }
                    (None, Direction::Up) => None,
                },
                _ => None,
            };
            if let Some(awaiting) = closing {
                *track = Track::Closing { awaiting };
            } else if matches!(track, Track::Burst(_) | Track::Pending { .. }) {
                *track = Track::Held;
            }
        }
    }

    fn poll(&mut self, now: Time) -> Wake {
        if let Some(quit_by) = self.quit_by {
            let closing = self
                .tracks
                .iter()
                .any(|t| matches!(t, Track::Closing { .. }));
            return if closing && now < quit_by {
                Wake::At(quit_by)
            } else {
                Wake::Exit
            };
        }
        let expired = self.tracks.iter().any(|t| match t {
            Track::Burst(burst) => burst.liveness_expiry().is_some_and(|e| now >= e),
            _ => false,
        });
        if expired {
            self.observation_died(now);
        }
        let mut wake = None;
        for button in Button::ALL {
            wake = earliest(wake, self.poll_track(button, now));
        }
        // Held counts as idle: its release may have been lost to a dead
        // observation, and only a reinstall would recover from that.
        let idle = self
            .tracks
            .iter()
            .all(|t| matches!(t, Track::Idle | Track::Held));
        if self.armed && idle {
            if now >= self.last_reinstall + IDLE_REINSTALL_PERIOD {
                self.reinstall(now);
            }
            wake = earliest(wake, Some(self.last_reinstall + IDLE_REINSTALL_PERIOD));
        }
        wake.map_or(Wake::OnInput, Wake::At)
    }

    /// Advances one button's track to `now` and returns when it next needs
    /// attention.
    fn poll_track(&mut self, button: Button, now: Time) -> Option<Time> {
        let track = &mut self.tracks[button.index()];
        match track {
            Track::Idle | Track::Held | Track::Closing { .. } => None,
            Track::Pending {
                delay_ends,
                settings,
            } => {
                let start = *delay_ends;
                if now < start {
                    return Some(start);
                }
                if !self.armed {
                    *track = Track::Held;
                    return None;
                }
                if !self.backend.can_inject_at_cursor() {
                    *track = Track::Held;
                    (self.on_status)(Status::TargetNotInjectable(button));
                    return None;
                }
                *track = Track::Burst(Burst::new(start, settings, self.rng.fork()));
                self.poll_track(button, now)
            }
            Track::Burst(burst) => {
                if let Some(expiry) = burst.liveness_expiry() {
                    return Some(expiry);
                }
                let due = match burst.next {
                    Direction::Up => burst.cycle.up,
                    Direction::Down => burst.cycle.down,
                };
                if now < due {
                    return Some(due);
                }
                if burst.next == Direction::Up && !self.backend.can_inject_at_cursor() {
                    // The last injection was a down, so the physical up ends
                    // the burst cleanly.
                    *track = Track::Held;
                    (self.on_status)(Status::TargetNotInjectable(button));
                    return None;
                }
                let direction = burst.next;
                self.backend.inject(button, direction);
                burst.in_flight = Some(InFlight {
                    direction,
                    sent: now,
                });
                burst.next = match direction {
                    Direction::Up => Direction::Down,
                    Direction::Down => Direction::Up,
                };
                burst.liveness_expiry()
            }
        }
    }

    /// D8's response to an injection that never echoed: the observation is
    /// dead, so at most one injection per button reached applications unseen.
    /// Leave no button logically down, inject nothing further, and reinstall.
    fn observation_died(&mut self, now: Time) {
        for button in Button::ALL {
            let track = &mut self.tracks[button.index()];
            if let Track::Burst(Burst {
                in_flight:
                    Some(InFlight {
                        direction: Direction::Down,
                        ..
                    }),
                ..
            }) = track
            {
                self.backend.inject(button, Direction::Up);
            }
            // Releases during the outage were never seen, so a track left
            // pending could start a burst on a button nobody holds.
            *track = Track::Idle;
        }
        self.reinstall(now);
    }

    fn reinstall(&mut self, now: Time) {
        self.backend.reinstall();
        self.last_reinstall = now;
    }
}

fn earliest(a: Option<Time>, b: Option<Time>) -> Option<Time> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
