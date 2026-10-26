//! A discrete-event simulation of a backend and the applications behind it,
//! driving the real gate and scheduler on a fake clock (R9).
//!
//! The fake backend echoes each injection back through the gate after a
//! latency, as a hook sees the app's own `SendInput`. While the observation is
//! dead, every event, physical or injected, reaches applications unseen. What
//! applications receive is recorded, so tests assert on that rather than on
//! scheduler internals.

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use fastrand::Rng;

use crate::backend::{Inject, Observe};
use crate::clock::{Clock, Time};
use crate::gate::{Button, ButtonEvent, ButtonGate, Direction, Inbox, Source, Verdict};
use crate::scheduler::{Input, Scheduler, Status, Wake};
use crate::settings::{ClickButtons, Settings};

pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

pub fn us(n: u64) -> Duration {
    Duration::from_micros(n)
}

pub fn at_ms(n: u64) -> Time {
    Time::from_epoch(ms(n))
}

pub fn at_us(n: u64) -> Time {
    Time::from_epoch(us(n))
}

pub fn settings(cps: u32, delay_ms: u64, jitter: bool) -> Settings {
    Settings::new(cps, ms(delay_ms), jitter, ClickButtons::Both, None).unwrap()
}

/// An event applications received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub at: Time,
    pub direction: Direction,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Injection {
    pub at: Time,
    pub direction: Direction,
}

enum Happening {
    /// An event reaching the hook, or applications if the hook is dead.
    Event(ButtonEvent),
    Control(Input),
    KillObservation,
    Injectable(bool),
}

struct World {
    now: Time,
    observing: bool,
    echo_latency: Duration,
    slow_echoes: HashMap<usize, Duration>,
    agenda: BTreeMap<(Time, u64), Happening>,
    scheduled: u64,
    delivered: Vec<(Button, Delivery)>,
    injections: Vec<(Button, Injection)>,
    injectable: bool,
    reinstalls: Vec<Time>,
    reinstall_pending: bool,
    statuses: Vec<(Time, Status)>,
}

impl World {
    /// Happenings at the same time occur in the order they were scheduled.
    fn schedule(&mut self, at: Time, happening: Happening) {
        self.agenda.insert((at, self.scheduled), happening);
        self.scheduled += 1;
    }
}

struct FakeBackend(Arc<Mutex<World>>);

impl Inject for FakeBackend {
    fn inject(&mut self, button: Button, direction: Direction) {
        let mut world = self.0.lock().unwrap();
        let index = world.injections.len();
        let now = world.now;
        world
            .injections
            .push((button, Injection { at: now, direction }));
        let latency = world
            .slow_echoes
            .get(&index)
            .copied()
            .unwrap_or(world.echo_latency);
        world.schedule(
            now + latency,
            Happening::Event(ButtonEvent {
                button,
                direction,
                source: Source::Tagged,
            }),
        );
    }

    fn can_inject_at_cursor(&mut self) -> bool {
        self.0.lock().unwrap().injectable
    }
}

impl Observe for FakeBackend {
    fn reinstall(&mut self) {
        let mut world = self.0.lock().unwrap();
        let now = world.now;
        world.reinstalls.push(now);
        world.reinstall_pending = true;
    }
}

/// The simulation hands the scheduler its time directly, so only the gate's
/// wake-up is ever called.
struct NoClock;

impl Clock for NoClock {
    fn now(&self) -> Time {
        unreachable!("the simulation passes the time to the scheduler")
    }

    fn wait_until(&self, _deadline: Option<Time>) {
        unreachable!("the simulation never blocks")
    }

    fn wake(&self) {}
}

pub struct Sim {
    world: Arc<Mutex<World>>,
    gate: ButtonGate,
    scheduler: Scheduler<FakeBackend>,
    inbox: Receiver<Input>,
    control: Inbox,
    now: Time,
    exited_at: Option<Time>,
}

impl Sim {
    /// Starts disabled, as the real scheduler does, with a 1 ms echo latency.
    pub fn new(settings: Settings) -> Sim {
        Sim::with_seed(settings, 1)
    }

    pub fn with_seed(settings: Settings, seed: u64) -> Sim {
        let world = Arc::new(Mutex::new(World {
            now: Time::ZERO,
            observing: true,
            echo_latency: ms(1),
            slow_echoes: HashMap::new(),
            agenda: BTreeMap::new(),
            scheduled: 0,
            delivered: Vec::new(),
            injections: Vec::new(),
            injectable: true,
            reinstalls: Vec::new(),
            reinstall_pending: false,
            statuses: Vec::new(),
        }));
        let (tx, inbox) = mpsc::channel();
        let control = Inbox::new(tx, Arc::new(NoClock));
        let status_world = Arc::clone(&world);
        let on_status = Box::new(move |status| {
            let mut world = status_world.lock().unwrap();
            let now = world.now;
            world.statuses.push((now, status));
        });
        Sim {
            gate: ButtonGate::new(control.clone()),
            scheduler: Scheduler::new(
                FakeBackend(Arc::clone(&world)),
                settings,
                Rng::with_seed(seed),
                on_status,
            ),
            world,
            inbox,
            control,
            now: Time::ZERO,
            exited_at: None,
        }
    }

    /// Enabled at time zero.
    pub fn enabled(settings: Settings) -> Sim {
        let mut sim = Sim::new(settings);
        sim.control_at(Time::ZERO, Input::SetEnabled(true));
        sim
    }

    fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().unwrap()
    }

    fn schedule(&mut self, at: Time, happening: Happening) {
        self.world().schedule(at, happening);
    }

    fn physical(&mut self, button: Button, direction: Direction, at: Time) {
        let event = ButtonEvent {
            button,
            direction,
            source: Source::Physical,
        };
        self.schedule(at, Happening::Event(event));
    }

    pub fn press(&mut self, button: Button, at: Time) {
        self.physical(button, Direction::Down, at);
    }

    pub fn release(&mut self, button: Button, at: Time) {
        self.physical(button, Direction::Up, at);
    }

    pub fn control_at(&mut self, at: Time, input: Input) {
        self.schedule(at, Happening::Control(input));
    }

    pub fn kill_observation_at(&mut self, at: Time) {
        self.schedule(at, Happening::KillObservation);
    }

    pub fn set_injectable_at(&mut self, at: Time, injectable: bool) {
        self.schedule(at, Happening::Injectable(injectable));
    }

    pub fn set_echo_latency(&mut self, latency: Duration) {
        self.world().echo_latency = latency;
    }

    /// The `index`th injection, counting from zero across both buttons,
    /// echoes after `latency` instead.
    pub fn slow_echo(&mut self, index: usize, latency: Duration) {
        self.world().slow_echoes.insert(index, latency);
    }

    /// Runs the scheduler and the agenda until `end`. After the scheduler
    /// exits, the app has removed its observation, so later events reach
    /// applications unseen.
    pub fn run_until(&mut self, end: Time) {
        for _ in 0..10_000_000 {
            let wake = match self.exited_at {
                Some(_) => None,
                None => {
                    let wake = self.scheduler.turn(&self.inbox, self.now);
                    self.complete_reinstall();
                    match wake {
                        Wake::At(deadline) => {
                            assert!(deadline > self.now, "scheduler asked to wake in the past");
                            Some(deadline)
                        }
                        Wake::OnInput => None,
                        Wake::Exit => {
                            self.exited_at = Some(self.now);
                            self.world().observing = false;
                            None
                        }
                    }
                }
            };
            let next_happening = self.world().agenda.keys().next().map(|&(t, _)| t);
            let next = match (wake, next_happening) {
                (Some(a), Some(b)) => a.min(b),
                (a, b) => match a.or(b) {
                    Some(t) => t,
                    None => break,
                },
            };
            if next > end {
                break;
            }
            self.set_now(next.max(self.now));
            if next_happening.is_some_and(|t| t <= self.now) {
                self.occur_next();
            }
        }
        self.set_now(end.max(self.now));
    }

    fn set_now(&mut self, now: Time) {
        self.now = now;
        self.world().now = now;
    }

    fn complete_reinstall(&mut self) {
        let pending = std::mem::take(&mut self.world().reinstall_pending);
        if pending {
            self.gate.reset_delivered();
            self.world().observing = true;
        }
    }

    fn occur_next(&mut self) {
        let (_, happening) = self.world().agenda.pop_first().unwrap();
        match happening {
            Happening::Event(event) => {
                let observing = self.world().observing;
                let passes = !observing || self.gate.observe(event) == Verdict::Pass;
                if passes {
                    let now = self.now;
                    self.world().delivered.push((
                        event.button,
                        Delivery {
                            at: now,
                            direction: event.direction,
                            source: event.source,
                        },
                    ));
                }
            }
            Happening::Control(input) => self.control.send(input),
            Happening::KillObservation => self.world().observing = false,
            Happening::Injectable(injectable) => self.world().injectable = injectable,
        }
    }

    pub fn delivered(&self, button: Button) -> Vec<Delivery> {
        let world = self.world.lock().unwrap();
        world
            .delivered
            .iter()
            .filter(|(b, _)| *b == button)
            .map(|&(_, d)| d)
            .collect()
    }

    pub fn injections(&self, button: Button) -> Vec<Injection> {
        let world = self.world.lock().unwrap();
        world
            .injections
            .iter()
            .filter(|(b, _)| *b == button)
            .map(|&(_, i)| i)
            .collect()
    }

    pub fn reinstalls(&self) -> Vec<Time> {
        self.world.lock().unwrap().reinstalls.clone()
    }

    pub fn statuses(&self) -> Vec<(Time, Status)> {
        self.world.lock().unwrap().statuses.clone()
    }

    pub fn exited_at(&self) -> Option<Time> {
        self.exited_at
    }

    /// R6: applications received the button's downs and ups alternating,
    /// ending on an up, and nothing was injected for it after `released_at`.
    pub fn assert_ends_cleanly(&self, button: Button, released_at: Time) {
        self.assert_ends(button, released_at, 0);
    }

    /// R6 under D8: as [`Sim::assert_ends_cleanly`], except that one redundant
    /// up is permitted, which may be injected after the release.
    pub fn assert_ends_with_at_most_one_redundant_up(&self, button: Button, released_at: Time) {
        self.assert_ends(button, released_at, 1);
    }

    fn assert_ends(&self, button: Button, released_at: Time, redundant_ups: usize) {
        let delivered = self.delivered(button);
        let directions: Vec<Direction> = delivered.iter().map(|d| d.direction).collect();
        assert_eq!(
            directions.first(),
            Some(&Direction::Down),
            "applications never saw the press: {delivered:?}"
        );
        assert_eq!(
            directions.last(),
            Some(&Direction::Up),
            "button left down: {delivered:?}"
        );
        let mut repeated_ups = 0;
        for pair in directions.windows(2) {
            assert!(
                pair != [Direction::Down, Direction::Down],
                "two downs in a row: {delivered:?}"
            );
            if pair == [Direction::Up, Direction::Up] {
                repeated_ups += 1;
            }
        }
        assert!(
            repeated_ups <= redundant_ups,
            "{repeated_ups} redundant ups, {redundant_ups} allowed: {delivered:?}"
        );
        let late: Vec<Injection> = self
            .injections(button)
            .into_iter()
            .filter(|i| i.at > released_at)
            .collect();
        assert!(
            late.len() <= redundant_ups && late.iter().all(|i| i.direction == Direction::Up),
            "injected after the release at {released_at:?}: {late:?}"
        );
    }
}
