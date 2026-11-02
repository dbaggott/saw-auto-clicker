//! Click timing: D5's Jitter and rate drift (R7), D8's lockstep and skipped
//! deadlines, the liveness timeout, and when the observation is reinstalled.

use std::time::Duration;

use fastrand::Rng;

use super::sim::*;
use crate::clock::Time;
use crate::gate::{Button, Direction};
use crate::jitter::{Cycle, Grid, HUMAN_JITTER, MIN_PHASE, RATE_SPREAD, Rate, interval_band};
use crate::scheduler::Input;

const LEFT: Button = Button::Left;

fn injected(sim: &Sim) -> Vec<(Time, Direction)> {
    sim.injections(LEFT)
        .into_iter()
        .map(|i| (i.at, i.direction))
        .collect()
}

fn downs(sim: &Sim) -> Vec<Time> {
    sim.injections(LEFT)
        .into_iter()
        .filter(|i| i.direction == Direction::Down)
        .map(|i| i.at)
        .collect()
}

#[test]
fn without_jitter_downs_are_exactly_one_interval_apart() {
    for cps in [1, 7, 20, 50] {
        let s = settings(cps, 250, false);
        let interval = s.interval();
        let mut sim = Sim::enabled(s);
        sim.press(LEFT, at_ms(0));
        sim.release(LEFT, at_ms(30_000));
        sim.run_until(at_ms(31_000));
        let downs = downs(&sim);
        assert!(
            downs.len() as u32 >= 29 * cps,
            "{} downs at {cps} CPS",
            downs.len()
        );
        for (k, down) in downs.iter().enumerate() {
            assert_eq!(*down, at_ms(250) + interval * (k as u32 + 1), "{cps} CPS");
        }
    }
}

/// Holds for `hold` and checks that every injection lands on the deadline of
/// the burst's own grid, so none is skipped or shifted and `jitter`'s tests
/// of the grid hold for what is injected. Then checks, on every click, what
/// the injections show of D5: each interval within the envelope the band
/// allows, and the up-time share and floors.
fn check_jitter(cps: u32, seed: u64, hold: Duration) {
    let jitter = HUMAN_JITTER;
    let s = settings(cps, 250, true);
    let (t_fast, t_slow) = interval_band(cps);
    let mut sim = Sim::with_seed(s, seed);
    sim.control_at(Time::ZERO, Input::SetEnabled(true));
    sim.press(LEFT, at_ms(0));
    let release = at_ms(250) + hold;
    sim.release(LEFT, release);
    sim.run_until(release + Duration::from_secs(1));

    let injected = injected(&sim);
    // The scheduler forks the burst's Rng from its own, seeded with `seed`.
    let mut rng = Rng::with_seed(seed).fork();
    let start = at_ms(250);
    let rate = Rate::drifting(start, cps, RATE_SPREAD, &mut rng);
    let mut grid = Grid::new(start, rate, jitter, rng);
    let pairs = injected.as_chunks::<2>().0;
    assert!(pairs.len() > 100, "{} cycles at {cps} CPS", pairs.len());
    for (k, pair) in pairs.iter().enumerate() {
        let Cycle { up, down } = grid.next_cycle();
        assert_eq!(
            *pair,
            [(up, Direction::Up), (down, Direction::Down)],
            "{cps} CPS, cycle {k}"
        );
    }
    let tolerance = Duration::from_nanos(2);
    let lo = t_fast.mul_f64(1.0 - jitter / 2.0) - t_slow.mul_f64(jitter / 2.0);
    let hi = t_slow.mul_f64(1.0 + jitter);
    let mut prev_down = at_ms(250);
    for pair in injected.as_chunks::<2>().0 {
        let [(up, Direction::Up), (down, Direction::Down)] = *pair else {
            panic!("injections out of order: {pair:?}");
        };
        let interval = down - prev_down;
        let up_time = down - up;
        assert!(
            interval + tolerance >= lo && interval <= hi + tolerance,
            "{cps} CPS: interval {interval:?} outside {lo:?}..={hi:?}"
        );
        let share_lo = interval.mul_f64(0.4 - 0.2 * jitter).max(MIN_PHASE);
        let share_hi = interval.mul_f64(0.4 + 0.2 * jitter);
        assert!(
            up_time + tolerance >= share_lo && up_time <= share_hi + tolerance,
            "up-time {up_time:?} outside {share_lo:?}..={share_hi:?} for interval {interval:?}"
        );
        assert!(up_time >= MIN_PHASE, "up-time {up_time:?} under the floor");
        assert!(interval - up_time >= MIN_PHASE, "down-hold under the floor");
        prev_down = down;
    }
}

#[test]
fn jitter_keeps_intervals_and_up_times_in_the_bands_envelope() {
    for cps in [1, 10, 15, 50] {
        let hold = Duration::from_secs(1) / cps * 3_000;
        check_jitter(cps, 7, hold);
    }
}

#[test]
fn jitter_drifts_the_rate_during_a_hold() {
    for seed in 1..=10 {
        let mut sim = Sim::with_seed(settings(15, 250, true), seed);
        sim.control_at(Time::ZERO, Input::SetEnabled(true));
        sim.press(LEFT, at_ms(0));
        sim.release(LEFT, at_ms(250 + 60_000));
        sim.run_until(at_ms(62_000));
        let downs = downs(&sim);
        let counts: Vec<usize> = (0..60u64)
            .map(|w| {
                let from = at_ms(250 + 1_000 * w);
                let to = from + Duration::from_secs(1);
                downs.iter().filter(|&&d| from <= d && d < to).count()
            })
            .collect();
        let (min, max) = (counts.iter().min().unwrap(), counts.iter().max().unwrap());
        assert!(
            (12..=18).contains(min) && (12..=18).contains(max),
            "seed {seed}: {counts:?}"
        );
        assert!(max - min >= 2, "seed {seed}: {counts:?}");
    }
}

#[test]
fn jitter_varies_the_intervals() {
    let mut sim = Sim::enabled(settings(10, 250, true));
    sim.press(LEFT, at_ms(0));
    sim.release(LEFT, at_ms(5_000));
    sim.run_until(at_ms(6_000));
    let downs = downs(&sim);
    let distinct: std::collections::HashSet<Duration> =
        downs.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(distinct.len() > 10, "{} distinct intervals", distinct.len());
}

#[test]
fn an_up_echoing_after_its_downs_deadline_sends_the_down_on_the_echo() {
    let mut sim = Sim::enabled(settings(10, 250, false));
    sim.press(LEFT, at_ms(0));
    // Injection 2 is the up at 410 ms; it echoes at 460 ms, after its down's
    // 450 ms deadline.
    sim.slow_echo(2, ms(50));
    sim.release(LEFT, at_ms(580));
    sim.run_until(at_ms(2_000));
    use Direction::{Down, Up};
    assert_eq!(
        injected(&sim),
        [
            (at_ms(310), Up),
            (at_ms(350), Down),
            (at_ms(410), Up),
            (at_ms(460), Down),
            (at_ms(510), Up),
            (at_ms(550), Down),
        ]
    );
    sim.assert_ends_cleanly(LEFT, at_ms(580));
}

#[test]
fn a_down_echoing_after_the_next_ups_deadline_skips_that_whole_cycle() {
    let mut sim = Sim::enabled(settings(10, 250, false));
    sim.press(LEFT, at_ms(0));
    // Injection 1 is the down at 350 ms; it echoes at 420 ms, after the next
    // up's 410 ms deadline, so that up and its down are dropped.
    sim.slow_echo(1, ms(70));
    sim.release(LEFT, at_ms(580));
    sim.run_until(at_ms(2_000));
    use Direction::{Down, Up};
    assert_eq!(
        injected(&sim),
        [
            (at_ms(310), Up),
            (at_ms(350), Down),
            (at_ms(510), Up),
            (at_ms(550), Down),
        ]
    );
    sim.assert_ends_cleanly(LEFT, at_ms(580));
}

/// The observation dies just before the first up, at `delay + 0.6T`.
fn detection_delay(cps: u32) -> Duration {
    let s = settings(cps, 250, false);
    let first_up = at_ms(250) + s.interval().mul_f64(0.6);
    let mut sim = Sim::enabled(s);
    sim.press(LEFT, at_ms(0));
    sim.kill_observation_at(first_up - ms(1));
    sim.release(LEFT, at_ms(10_000));
    sim.run_until(at_ms(11_000));
    let reinstalls = sim.reinstalls();
    assert_eq!(reinstalls.len(), 2, "{reinstalls:?}");
    reinstalls[1] - first_up
}

#[test]
fn liveness_timeout_is_the_longer_of_250_ms_and_two_intervals() {
    assert_eq!(detection_delay(10), ms(250));
    assert_eq!(detection_delay(4), ms(500));
    assert_eq!(detection_delay(1), ms(2_000));
}

#[test]
fn reinstall_on_every_enable() {
    let mut sim = Sim::new(settings(10, 250, false));
    sim.control_at(at_ms(10), Input::SetEnabled(true));
    sim.control_at(at_ms(20), Input::SetEnabled(true));
    sim.control_at(at_ms(30), Input::ToggleEnabled);
    sim.control_at(at_ms(40), Input::ToggleEnabled);
    sim.run_until(at_ms(1_000));
    assert_eq!(sim.reinstalls(), [at_ms(10), at_ms(40)]);
}

#[test]
fn reinstall_every_60_s_while_enabled_and_idle() {
    let mut sim = Sim::enabled(settings(10, 250, false));
    // Held across the 60 s mark, which defers that reinstall to the release.
    sim.press(LEFT, at_ms(59_000));
    sim.release(LEFT, at_ms(61_000));
    sim.control_at(at_ms(150_000), Input::SetEnabled(false));
    sim.run_until(at_ms(300_000));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(61_000), at_ms(121_000)]);
}

#[test]
fn a_late_down_is_never_followed_by_a_down_hold_under_the_floor() {
    // At 50 CPS click k's up is at 250 + 20k − 8 ms and its down 8 ms later.
    let mut sim = Sim::enabled(settings(50, 250, false));
    sim.press(LEFT, at_ms(0));
    // Injection 2 is the up at 282 ms; it echoes at 301 ms, so its down goes
    // out then, 1 ms before the next up's 302 ms deadline. That cycle is
    // dropped and the next up is at 322 ms.
    sim.slow_echo(2, ms(19));
    sim.release(LEFT, at_ms(335));
    sim.run_until(at_ms(1_000));
    use Direction::{Down, Up};
    assert_eq!(
        injected(&sim),
        [
            (at_ms(262), Up),
            (at_ms(270), Down),
            (at_ms(282), Up),
            (at_ms(301), Down),
            (at_ms(322), Up),
            (at_ms(330), Down),
        ]
    );
}

#[test]
fn a_held_button_does_not_stop_the_idle_reinstall() {
    let mut sim = Sim::enabled(settings(10, 250, false));
    // Refused at the delay, so held with no burst, and the release is never
    // seen.
    sim.set_injectable_at(at_ms(0), false);
    sim.press(LEFT, at_ms(10));
    sim.run_until(at_ms(130_000));
    assert_eq!(sim.reinstalls(), [at_ms(0), at_ms(60_000), at_ms(120_000)]);
}

#[test]
fn enabling_during_shutdown_is_ignored() {
    let mut sim = Sim::new(settings(10, 250, false));
    sim.control_at(at_ms(0), Input::Shutdown);
    sim.control_at(at_ms(0), Input::ToggleEnabled);
    sim.run_until(at_ms(1_000));
    assert!(sim.reinstalls().is_empty());
    assert!(sim.statuses().is_empty());
}
