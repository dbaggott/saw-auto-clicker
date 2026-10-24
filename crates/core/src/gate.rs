use std::sync::Arc;
use std::sync::mpsc::Sender;

use crate::clock::Clock;
use crate::scheduler::Input;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Right,
}

impl Button {
    pub const ALL: [Button; 2] = [Button::Left, Button::Right];

    pub(crate) fn index(self) -> usize {
        match self {
            Button::Left => 0,
            Button::Right => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Down,
    Up,
}

/// Where an observed button event came from, as the backend tells by the tag
/// on the app's own injections (D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// Not the app's own: the user's hand, or another program's injection
    /// such as a remapping driver's (R10).
    Physical,
    /// Injected by this app.
    Tagged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ButtonEvent {
    pub button: Button,
    pub direction: Direction,
    pub source: Source,
}

/// Whether the backend lets an observed event through to applications.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Pass,
    Suppress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Delivered {
    Unknown,
    Down,
    Up,
}

#[derive(Clone, Copy, Debug)]
struct GateState {
    /// The last down or up that passed, which is what applications hold.
    delivered: Delivered,
    /// Whether the physical button has been released since its last press.
    released: bool,
}

/// The button gate (D7): decides, for every button event the backend observes,
/// whether applications receive it, so that a burst always ends with exactly
/// one final up (R6).
///
/// The backend's observing thread owns the gate and calls
/// [`ButtonGate::observe`] synchronously inside its callback (D2). Taking
/// `&mut self` keeps every write to gate state on that thread.
pub struct ButtonGate {
    buttons: [GateState; 2],
    inbox: Inbox,
}

impl ButtonGate {
    pub(crate) fn new(inbox: Inbox) -> ButtonGate {
        ButtonGate {
            // Released, so a tagged event can never pass before the first
            // physical press.
            buttons: [GateState {
                delivered: Delivered::Unknown,
                released: true,
            }; 2],
            inbox,
        }
    }

    /// Applies D7's rules to one event, queues it for the scheduler thread
    /// without blocking, and returns the verdict for the backend to apply.
    pub fn observe(&mut self, event: ButtonEvent) -> Verdict {
        let state = &mut self.buttons[event.button.index()];
        let to_delivered = match event.direction {
            Direction::Down => Delivered::Down,
            Direction::Up => Delivered::Up,
        };
        let verdict = match (event.source, event.direction) {
            (Source::Physical, Direction::Down) => {
                state.released = false;
                state.delivered = Delivered::Down;
                Verdict::Pass
            }
            (Source::Physical, Direction::Up) => {
                state.released = true;
                if state.delivered == Delivered::Up {
                    Verdict::Suppress
                } else {
                    state.delivered = Delivered::Up;
                    Verdict::Pass
                }
            }
            (Source::Tagged, _) => {
                if state.released {
                    Verdict::Suppress
                } else {
                    state.delivered = to_delivered;
                    Verdict::Pass
                }
            }
        };
        self.inbox.send(Input::Observed(event));
        verdict
    }

    /// Forgets what applications hold for every button, so the next physical
    /// up passes. The backend calls this on the observing thread during
    /// [`Observe::reinstall`](crate::Observe::reinstall), after removing the
    /// old observation and before installing the new one (D8).
    pub fn reset_delivered(&mut self) {
        for state in &mut self.buttons {
            state.delivered = Delivered::Unknown;
        }
    }
}

/// The sending end of the scheduler thread's queue, shared by the gate and the
/// [`Controller`](crate::Controller).
#[derive(Clone)]
pub(crate) struct Inbox {
    tx: Sender<Input>,
    clock: Arc<dyn Clock>,
}

impl Inbox {
    pub(crate) fn new(tx: Sender<Input>, clock: Arc<dyn Clock>) -> Inbox {
        Inbox { tx, clock }
    }

    pub(crate) fn send(&self, input: Input) {
        // A send fails only once the scheduler thread has exited after
        // shutdown, when nothing is left to act on the input.
        if self.tx.send(input).is_ok() {
            self.clock.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};

    use super::*;
    use crate::clock::StdClock;

    fn gate() -> (ButtonGate, Receiver<Input>) {
        let (tx, rx) = mpsc::channel();
        (
            ButtonGate::new(Inbox::new(tx, Arc::new(StdClock::new()))),
            rx,
        )
    }

    fn event(direction: Direction, source: Source) -> ButtonEvent {
        ButtonEvent {
            button: Button::Left,
            direction,
            source,
        }
    }

    use Direction::{Down, Up};
    use Source::{Physical, Tagged};
    use Verdict::{Pass, Suppress};

    fn run(events: &[(Direction, Source)]) -> Vec<Verdict> {
        let (mut gate, _rx) = gate();
        events
            .iter()
            .map(|&(d, s)| gate.observe(event(d, s)))
            .collect()
    }

    #[test]
    fn plain_physical_click_passes() {
        assert_eq!(run(&[(Down, Physical), (Up, Physical)]), [Pass, Pass]);
    }

    #[test]
    fn tagged_events_pass_while_held() {
        assert_eq!(
            run(&[
                (Down, Physical),
                (Up, Tagged),
                (Down, Tagged),
                (Up, Physical)
            ]),
            [Pass, Pass, Pass, Pass]
        );
    }

    #[test]
    fn physical_up_after_delivered_up_is_suppressed_and_later_tags_too() {
        assert_eq!(
            run(&[
                (Down, Physical),
                (Up, Tagged),
                (Up, Physical),
                (Down, Tagged)
            ]),
            [Pass, Pass, Suppress, Suppress]
        );
    }

    #[test]
    fn tags_in_flight_after_physical_up_are_suppressed() {
        assert_eq!(
            run(&[
                (Down, Physical),
                (Up, Physical),
                (Up, Tagged),
                (Down, Tagged)
            ]),
            [Pass, Pass, Suppress, Suppress]
        );
    }

    #[test]
    fn tag_before_any_press_is_suppressed() {
        assert_eq!(run(&[(Up, Tagged)]), [Suppress]);
    }

    #[test]
    fn reset_lets_the_next_physical_up_pass() {
        let (mut gate, _rx) = gate();
        gate.observe(event(Down, Physical));
        gate.observe(event(Up, Tagged));
        gate.reset_delivered();
        assert_eq!(gate.observe(event(Up, Physical)), Pass);
    }

    #[test]
    fn buttons_are_independent() {
        let (mut gate, _rx) = gate();
        gate.observe(event(Down, Physical));
        let right_tag = ButtonEvent {
            button: Button::Right,
            direction: Up,
            source: Tagged,
        };
        assert_eq!(gate.observe(right_tag), Suppress);
    }

    #[test]
    fn every_event_is_queued_for_the_scheduler() {
        let (mut gate, rx) = gate();
        gate.observe(event(Down, Physical));
        gate.observe(event(Up, Tagged));
        let queued: Vec<_> = rx.try_iter().collect();
        assert!(matches!(
            queued.as_slice(),
            [Input::Observed(a), Input::Observed(b)]
                if *a == event(Down, Physical) && *b == event(Up, Tagged)
        ));
    }
}
