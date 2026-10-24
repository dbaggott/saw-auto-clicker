//! R6: every way a burst can end leaves applications with exactly one final
//! up and nothing injected afterwards.
//!
//! Unless a test says otherwise the timeline is 10 CPS (T = 100 ms), a 250 ms
//! delay, no Jitter, and a press at 0, so the burst starts at 250 ms and click
//! k injects an up at 250 + 100k − 40 ms and a down at 250 + 100k ms.

use super::sim::*;
use crate::clock::Time;
use crate::gate::{Button, Direction, Source};
use crate::scheduler::{Input, Status};

const LEFT: Button = Button::Left;

fn holding_left() -> Sim {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    sim
}

#[test]
fn release_at_every_point_of_the_click_cycle() {
    // Two whole cycles in 0.5 ms steps, which lands releases on each
    // injection's deadline and inside each 1 ms echo window.
    for step in 0..=400 {
        let release = at_us(250_000 + step * 500);
        let mut sim = holding_left();
        sim.release(LEFT, release);
        sim.run_until(at_ms(2_000));
        sim.assert_ends_cleanly(LEFT, release);
    }
}

/// An up injected at 310 ms echoes at 314 ms; a down injected at 350 ms echoes
/// at 354 ms.
fn with_slow_echoes(release: Time) -> Sim {
    let mut sim = holding_left();
    sim.set_echo_latency(ms(4));
    sim.release(LEFT, release);
    sim.run_until(at_ms(2_000));
    sim
}

fn final_up_source(sim: &Sim) -> Source {
    sim.delivered(LEFT).last().unwrap().source
}

#[test]
fn release_while_an_up_is_in_flight_physical_up_first() {
    let release = at_ms(312);
    let sim = with_slow_echoes(release);
    sim.assert_ends_cleanly(LEFT, release);
    assert_eq!(final_up_source(&sim), Source::Physical);
}

#[test]
fn release_while_an_up_is_in_flight_tagged_up_first() {
    let release = at_us(314_001);
    let sim = with_slow_echoes(release);
    sim.assert_ends_cleanly(LEFT, release);
    assert_eq!(final_up_source(&sim), Source::Tagged);
}

#[test]
fn release_while_a_down_is_in_flight_physical_up_first() {
    let release = at_ms(352);
    let sim = with_slow_echoes(release);
    sim.assert_ends_cleanly(LEFT, release);
    // The up injected at 310 ms is the final one; the in-flight down and the
    // physical up are both suppressed.
    assert_eq!(final_up_source(&sim), Source::Tagged);
    assert_eq!(sim.delivered(LEFT).len(), 2);
}

#[test]
fn release_while_a_down_is_in_flight_tagged_down_first() {
    let release = at_us(354_001);
    let sim = with_slow_echoes(release);
    sim.assert_ends_cleanly(LEFT, release);
    assert_eq!(final_up_source(&sim), Source::Physical);
}

fn disarmed_mid_burst(disarm: Time) -> Sim {
    let mut sim = holding_left();
    sim.control_at(disarm, Input::SetArmed(false));
    sim.release(LEFT, at_ms(800));
    sim.run_until(at_ms(2_000));
    sim
}

#[test]
fn disarm_mid_burst_with_the_button_delivered_up() {
    let disarm = at_ms(330);
    let sim = disarmed_mid_burst(disarm);
    sim.assert_ends_cleanly(LEFT, disarm);
    assert_eq!(final_up_source(&sim), Source::Tagged);
    assert!(sim.statuses().contains(&(disarm, Status::Disarmed)));
}

#[test]
fn disarm_mid_burst_with_the_button_delivered_down() {
    let disarm = at_ms(370);
    let sim = disarmed_mid_burst(disarm);
    sim.assert_ends_cleanly(LEFT, disarm);
    assert_eq!(final_up_source(&sim), Source::Physical);
}

fn quit_mid_burst(quit: Time) -> Sim {
    let mut sim = holding_left();
    sim.control_at(quit, Input::Shutdown);
    sim.release(LEFT, at_ms(800));
    sim.run_until(at_ms(2_000));
    sim
}

#[test]
fn quit_mid_burst_with_the_button_delivered_up() {
    let quit = at_ms(330);
    let sim = quit_mid_burst(quit);
    let after_quit: Vec<_> = sim
        .injections(LEFT)
        .into_iter()
        .filter(|i| i.at >= quit)
        .collect();
    assert_eq!(after_quit.len(), 1);
    assert_eq!(after_quit[0].direction, Direction::Down);
    // The down echoed, so shutdown returned without waiting out its grace.
    assert_eq!(sim.exited_at(), Some(at_ms(331)));
    sim.assert_ends_cleanly(LEFT, at_ms(800));
    // The release came after the app unhooked, straight to applications.
    assert_eq!(final_up_source(&sim), Source::Physical);
}

#[test]
fn quit_mid_burst_with_the_button_delivered_down() {
    let quit = at_ms(370);
    let sim = quit_mid_burst(quit);
    sim.assert_ends_cleanly(LEFT, quit);
    assert_eq!(sim.exited_at(), Some(quit));
    assert_eq!(final_up_source(&sim), Source::Physical);
}

#[test]
fn quit_while_an_up_is_in_flight() {
    let mut sim = holding_left();
    sim.set_echo_latency(ms(4));
    sim.control_at(at_ms(312), Input::Shutdown);
    sim.release(LEFT, at_ms(800));
    sim.run_until(at_ms(2_000));
    sim.assert_ends_cleanly(LEFT, at_ms(312));
    assert_eq!(sim.exited_at(), Some(at_ms(316)));
}

#[test]
fn quit_gives_up_waiting_after_100_ms() {
    let mut sim = holding_left();
    sim.kill_observation_at(at_ms(320));
    sim.control_at(at_ms(330), Input::Shutdown);
    sim.release(LEFT, at_ms(800));
    sim.run_until(at_ms(2_000));
    assert_eq!(sim.exited_at(), Some(at_ms(430)));
    sim.assert_ends_cleanly(LEFT, at_ms(330));
}

/// The observation dies at `death`; the button is released at `release`.
fn hook_death(death: Time, release: Time) -> Sim {
    let mut sim = holding_left();
    sim.kill_observation_at(death);
    sim.release(LEFT, release);
    sim.run_until(at_ms(3_000));
    sim
}

#[test]
fn hook_death_with_a_down_unechoed_release_before_detection() {
    // The down injected at 350 ms never echoes; detection is 250 ms later.
    let sim = hook_death(at_ms(349), at_ms(500));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(600)]);
    sim.assert_ends_with_at_most_one_redundant_up(LEFT, at_ms(500));
    let last = sim.injections(LEFT).last().copied().unwrap();
    assert_eq!((last.at, last.direction), (at_ms(600), Direction::Up));
}

#[test]
fn hook_death_with_a_down_unechoed_release_after_detection() {
    let sim = hook_death(at_ms(349), at_ms(700));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(600)]);
    sim.assert_ends_with_at_most_one_redundant_up(LEFT, at_ms(700));
}

#[test]
fn hook_death_with_an_up_unechoed_release_before_detection() {
    let sim = hook_death(at_ms(309), at_ms(500));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(560)]);
    sim.assert_ends_with_at_most_one_redundant_up(LEFT, at_ms(500));
    assert!(sim.injections(LEFT).iter().all(|i| i.at <= at_ms(310)));
}

#[test]
fn hook_death_with_an_up_unechoed_release_after_detection() {
    let sim = hook_death(at_ms(309), at_ms(700));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(560)]);
    sim.assert_ends_with_at_most_one_redundant_up(LEFT, at_ms(700));
    assert!(sim.injections(LEFT).iter().all(|i| i.at <= at_ms(310)));
}

#[test]
fn hook_death_stops_both_buttons_and_leaves_neither_down() {
    let mut sim = Sim::armed(settings(10, 250, 0.0));
    sim.press(LEFT, at_ms(0));
    // Right runs 20 ms behind left. After the observation dies at 365 ms,
    // right's down at 370 ms and left's up at 410 ms never echo, and right's
    // times out first.
    sim.press(Button::Right, at_ms(20));
    sim.kill_observation_at(at_ms(365));
    sim.release(LEFT, at_ms(1_000));
    sim.release(Button::Right, at_ms(1_000));
    sim.run_until(at_ms(3_000));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(620)]);
    for button in Button::ALL {
        sim.assert_ends_with_at_most_one_redundant_up(button, at_ms(1_000));
        assert!(sim.injections(button).iter().all(|i| i.at <= at_ms(620)));
    }
}

#[test]
fn hook_death_with_the_unechoed_down_landing_after_the_release() {
    // With the observation dead, the down injected at 350 ms reaches
    // applications at 351 ms, after the release at 350.5 ms. Only D8's
    // corrective up keeps the button from staying down.
    let sim = hook_death(at_ms(340), at_us(350_500));
    let delivered = sim.delivered(LEFT);
    let last_two: Vec<_> = delivered[delivered.len() - 2..]
        .iter()
        .map(|d| (d.at, d.direction, d.source))
        .collect();
    assert_eq!(
        last_two,
        [
            (at_ms(351), Direction::Down, Source::Tagged),
            (at_ms(601), Direction::Up, Source::Tagged)
        ]
    );
    sim.assert_ends_with_at_most_one_redundant_up(LEFT, at_us(350_500));
}
