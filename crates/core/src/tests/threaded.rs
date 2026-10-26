//! The real threads and clock: `spawn`, a gate owned by another thread, and
//! shutdown. Timing is the OS's, so this checks wiring, not rates.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::backend::{Inject, Observe};
use crate::clock::StdClock;
use crate::gate::{Button, ButtonEvent, Direction, Source, Verdict};
use crate::runtime::spawn;
use crate::scheduler::Status;
use crate::settings::{ClickButtons, Settings};

/// Injections go to the observing thread, as an OS echoes them to its hook.
struct EchoingBackend {
    hook: Sender<ButtonEvent>,
}

impl Inject for EchoingBackend {
    fn inject(&mut self, button: Button, direction: Direction) {
        let event = ButtonEvent {
            button,
            direction,
            source: Source::Tagged,
        };
        // The observing thread outlives the scheduler in this test.
        self.hook.send(event).unwrap();
    }

    fn can_inject_at_cursor(&mut self) -> bool {
        true
    }
}

impl Observe for EchoingBackend {
    fn reinstall(&mut self) {}
}

#[test]
fn a_hold_clicks_and_shutdown_returns() {
    let (hook, hook_events) = mpsc::channel::<ButtonEvent>();
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let status_log = Arc::clone(&statuses);
    let settings = Settings::new(
        50,
        Duration::from_millis(50),
        false,
        ClickButtons::Both,
        None,
    )
    .unwrap();
    let (mut gate, handle) = spawn(
        EchoingBackend { hook: hook.clone() },
        Arc::new(StdClock::new()),
        settings,
        move |status| status_log.lock().unwrap().push(status),
    )
    .unwrap();
    let observer = thread::spawn(move || {
        let mut delivered = Vec::new();
        for event in hook_events {
            if gate.observe(event) == Verdict::Pass {
                delivered.push(event);
            }
        }
        delivered
    });

    handle.controller().set_enabled(true);
    let physical = |direction| ButtonEvent {
        button: Button::Left,
        direction,
        source: Source::Physical,
    };
    hook.send(physical(Direction::Down)).unwrap();
    thread::sleep(Duration::from_millis(400));
    hook.send(physical(Direction::Up)).unwrap();
    thread::sleep(Duration::from_millis(50));
    handle.shutdown().unwrap();
    drop(hook);
    let delivered = observer.join().unwrap();

    let tagged = delivered
        .iter()
        .filter(|e| e.source == Source::Tagged)
        .count();
    assert!(
        tagged >= 2,
        "only {tagged} injections passed: {delivered:?}"
    );
    let directions: Vec<Direction> = delivered.iter().map(|e| e.direction).collect();
    assert!(
        directions.windows(2).all(|w| w[0] != w[1]),
        "{directions:?}"
    );
    assert_eq!(directions.last(), Some(&Direction::Up));
    assert_eq!(*statuses.lock().unwrap(), [Status::Enabled]);
}
