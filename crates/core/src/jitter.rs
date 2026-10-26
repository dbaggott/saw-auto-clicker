use std::time::Duration;

use fastrand::Rng;

use crate::clock::Time;

/// The Jitter a burst runs with when Jitter is on. Consecutive downs then
/// vary with a standard deviation of J/√6 ≈ 16% of the interval, roughly the
/// spread of a person clicking as fast as they can.
pub(crate) const HUMAN_JITTER: f64 = 0.4;

/// The shortest the button spends up, or down, in one click (D5).
pub(crate) const MIN_PHASE: Duration = Duration::from_millis(2);

/// When one click's injected up and down are due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cycle {
    pub(crate) up: Time,
    pub(crate) down: Time,
}

/// One burst's click deadlines under D5. Click k's down is at
/// `start + k·T + o_k`, with `o_k` uniform in ±(J/2)·T, so offsets never
/// accumulate. The up before it leads it by the up-time `g_k · u_k`, where
/// `g_k` is the interval since the previous down and `u_k` is uniform in
/// [0.4 − 0.2J, 0.4 + 0.2J]. The up-time and the down-hold are each at least
/// [`MIN_PHASE`].
///
/// `start` is the logical down the physical press created, which stands in for
/// click 0's down.
pub(crate) struct Grid {
    start: Time,
    interval: Duration,
    jitter: f64,
    rng: Rng,
    clicks: u32,
    prev_down: Time,
}

impl Grid {
    pub(crate) fn new(start: Time, interval: Duration, jitter: f64, rng: Rng) -> Grid {
        Grid {
            start,
            interval,
            jitter,
            rng,
            clicks: 0,
            prev_down: start,
        }
    }

    /// The next click's deadlines. Every click is drawn in turn, including
    /// those the scheduler then skips, so the grid never depends on timing.
    pub(crate) fn next_cycle(&mut self) -> Cycle {
        self.clicks += 1;
        let nominal = self.start + self.interval * self.clicks;
        let offset_ns = self.symmetric() * self.jitter / 2.0 * self.interval.as_nanos() as f64;
        let offset = Duration::from_nanos(offset_ns.abs().round() as u64);
        let down = if offset_ns < 0.0 {
            nominal - offset
        } else {
            nominal + offset
        };
        let gap = down - self.prev_down;
        let up_share = 0.4 + self.symmetric() * 0.2 * self.jitter;
        let up_time = gap
            .mul_f64(up_share)
            .max(MIN_PHASE)
            .min(gap.saturating_sub(MIN_PHASE));
        self.prev_down = down;
        Cycle {
            up: down - up_time,
            down,
        }
    }

    /// A draw uniform in [−1, 1].
    fn symmetric(&mut self) -> f64 {
        self.rng.f64_inclusive() * 2.0 - 1.0
    }
}
