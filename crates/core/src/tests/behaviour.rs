//! What the user sees: short presses untouched (R3), holds auto-clicking per
//! button (R4, R5), the armed state (R2), elevated targets (R16), and when
//! changed settings apply (Q-c).

use super::sim::*;
use crate::clock::Time;
use crate::gate::{Button, Direction, Source};
use crate::scheduler::{Input, Status};

const LEFT: Button = Button::Left;
const RIGHT: Button = Button::Right;

fn physical(sim: &Sim, button: Button) -> Vec<(Time, Direction)> {
    sim.delivered(button)
        .into_iter()
        .inspect(|d| assert_eq!(d.source, Source::Physical))
        .map(|d| (d.at, d.direction))
        .collect()
}

/// Presses and releases `button` at each pair of times, in milliseconds.
fn presses(sim: &mut Sim, button: Button, times: &[(u64, u64)]) {
    for &(down, up) in times {
        sim.press(button, at_ms(down));
        sim.release(button, at_ms(up));
    }
}

fn expected(times: &[(u64, u64)]) -> Vec<(Time, Direction)> {
    times
        .iter()
        .flat_map(|&(down, up)| [(at_ms(down), Direction::Down), (at_ms(up), Direction::Up)])
        .collect()
}

#[test]
fn presses_shorter_than_the_delay_pass_untouched() {
    let cases: [&[(u64, u64)]; 3] = [
        // A click.
        &[(0, 90)],
        // A double-click.
        &[(0, 80), (150, 230)],
        // A short drag. Pointer moves never reach the gate, so to the core a
        // drag is a press held for most of the delay.
        &[(0, 249)],
    ];
    for times in cases {
        let mut sim = Sim::armed(settings(10, 250, 0.0));
        presses(&mut sim, LEFT, times);
        sim.run_until(at_ms(5_000));
        assert!(sim.injections(LEFT).is_empty(), "{times:?}");
        assert_eq!(physical(&sim, LEFT), expected(times));
    }
}

#[test]
fn holding_one_button_clicks_only_that_button() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.release(LEFT, at_ms(1_000));
    sim.run_until(at_ms(2_000));
    assert_eq!(sim.injections(LEFT).len(), 14);
    assert!(sim.injections(RIGHT).is_empty());
    assert!(sim.delivered(RIGHT).is_empty());
    sim.assert_ends_cleanly(LEFT, at_ms(1_000));
}

#[test]
fn both_buttons_run_independent_bursts() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.press(RIGHT, at_ms(30));
    sim.release(LEFT, at_ms(600));
    sim.release(RIGHT, at_ms(1_000));
    sim.run_until(at_ms(2_000));
    sim.assert_ends_cleanly(LEFT, at_ms(600));
    sim.assert_ends_cleanly(RIGHT, at_ms(1_000));
    let right_downs: Vec<Time> = sim
        .injections(RIGHT)
        .into_iter()
        .filter(|i| i.direction == Direction::Down)
        .map(|i| i.at)
        .collect();
    // Right keeps its own grid, from its own press, after left's release.
    assert_eq!(right_downs, [380, 480, 580, 680, 780, 880, 980].map(at_ms));
}

#[test]
fn disarmed_no_burst_starts_and_every_event_passes() {
    let mut sim = Sim::new(settings(10, 250, 0.0));
    let times = [(0, 2_000), (2_100, 2_150)];
    presses(&mut sim, LEFT, &times);
    presses(&mut sim, RIGHT, &times);
    sim.run_until(at_ms(5_000));
    for button in Button::ALL {
        assert!(sim.injections(button).is_empty());
        assert_eq!(physical(&sim, button), expected(&times));
    }
    assert!(sim.reinstalls().is_empty());
}

#[test]
fn the_hotkey_toggles_armed_and_reports_it() {
    let mut sim = Sim::new(settings(10, 250, 0.0));
    sim.control_at(at_ms(0), Input::ToggleArmed);
    sim.press(LEFT, at_ms(10));
    sim.release(LEFT, at_ms(500));
    sim.control_at(at_ms(1_000), Input::ToggleArmed);
    sim.press(LEFT, at_ms(1_010));
    sim.release(LEFT, at_ms(1_500));
    sim.run_until(at_ms(3_000));
    assert_eq!(
        sim.statuses(),
        [(at_ms(0), Status::Armed), (at_ms(1_000), Status::Disarmed)]
    );
    let injections = sim.injections(LEFT);
    assert!(!injections.is_empty());
    assert!(injections.iter().all(|i| i.at < at_ms(500)));
}

#[test]
fn a_target_not_injectable_at_the_delay_gets_no_burst() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.set_injectable_at(at_ms(0), false);
    let times = [(0, 1_000)];
    presses(&mut sim, LEFT, &times);
    sim.run_until(at_ms(5_000));
    assert!(sim.injections(LEFT).is_empty());
    assert_eq!(physical(&sim, LEFT), expected(&times));
    assert_eq!(
        sim.statuses(),
        [
            (at_ms(0), Status::Armed),
            (at_ms(250), Status::TargetNotInjectable(LEFT))
        ]
    );
}

#[test]
fn a_target_turning_non_injectable_ends_the_burst_logically_down() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.set_injectable_at(at_ms(400), false);
    sim.release(LEFT, at_ms(800));
    sim.run_until(at_ms(2_000));
    // Checked before the up due at 410 ms; the down at 350 ms was the last.
    assert!(sim.injections(LEFT).iter().all(|i| i.at <= at_ms(350)));
    let delivered = sim.delivered(LEFT);
    let before_release = delivered[delivered.len() - 2];
    assert_eq!(
        (before_release.direction, before_release.source),
        (Direction::Down, Source::Tagged)
    );
    sim.assert_ends_cleanly(LEFT, at_ms(400));
    assert!(
        sim.statuses()
            .contains(&(at_ms(410), Status::TargetNotInjectable(LEFT)))
    );
}

#[test]
fn changed_settings_apply_from_the_next_press() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.control_at(at_ms(400), Input::SetSettings(settings(20, 100, 0.0)));
    sim.release(LEFT, at_ms(700));
    sim.press(LEFT, at_ms(1_000));
    sim.release(LEFT, at_ms(1_300));
    sim.run_until(at_ms(2_000));
    let downs: Vec<Time> = sim
        .injections(LEFT)
        .into_iter()
        .filter(|i| i.direction == Direction::Down)
        .map(|i| i.at)
        .collect();
    // The second burst's down due at 1 300 ms meets the release there.
    assert_eq!(downs, [350, 450, 550, 650, 1_150, 1_200, 1_250].map(at_ms));
}

#[test]
fn settings_changed_during_the_delay_wait_for_the_next_press() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.control_at(at_ms(100), Input::SetSettings(settings(20, 100, 0.0)));
    sim.release(LEFT, at_ms(560));
    sim.run_until(at_ms(2_000));
    let downs: Vec<Time> = sim
        .injections(LEFT)
        .into_iter()
        .filter(|i| i.direction == Direction::Down)
        .map(|i| i.at)
        .collect();
    assert_eq!(downs, [350, 450, 550].map(at_ms));
}

#[test]
fn a_press_held_across_arming_starts_no_burst() {
    let mut sim = Sim::new(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim.control_at(at_ms(100), Input::SetArmed(true));
    sim.release(LEFT, at_ms(1_000));
    sim.run_until(at_ms(2_000));
    assert!(sim.injections(LEFT).is_empty());
    assert_eq!(
        physical(&sim, LEFT),
        [(at_ms(0), Direction::Down), (at_ms(1_000), Direction::Up)]
    );
}
