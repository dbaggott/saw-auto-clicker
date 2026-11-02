use std::time::Duration;

use fastrand::Rng;

use crate::clock::Time;
use crate::settings::CPS_RANGE;

/// The Jitter a burst runs with when Jitter is on. Consecutive downs then
/// vary with a standard deviation of J/√6 ≈ 16% of the interval, roughly the
/// spread of a person clicking as fast as they can.
pub(crate) const HUMAN_JITTER: f64 = 0.4;

/// How far the click rate drifts either side of the set CPS when Jitter is on,
/// as a share of it (D5). It is ±2 CPS at the butterfly preset's 15 CPS, so
/// 2/15, and scales with the CPS: about ±1.3 at 10.
pub(crate) const RATE_SPREAD: f64 = 2.0 / 15.0;

/// How often the drifting rate draws a new target (D5).
pub(crate) const RATE_SEGMENT: Duration = Duration::from_secs(1);

/// The shortest the button spends up, or down, in one click (D5).
pub(crate) const MIN_PHASE: Duration = Duration::from_millis(2);

/// When one click's injected up and down are due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cycle {
    pub(crate) up: Time,
    pub(crate) down: Time,
}

/// One burst's click deadlines under D5, on a moving grid. Click k's nominal
/// down is the one before it plus `T_k = 1 / rate(n_{k−1})`, where the rate
/// is [`Drift`]'s, or the set CPS when there is no drift. Its down is at
/// `n_k + o_k`, with `o_k` uniform in ±(J/2)·T_k, so offsets never
/// accumulate. The up before it leads it by the up-time `g_k · u_k`, where
/// `g_k` is the interval since the previous down and `u_k` is uniform in
/// [0.4 − 0.2J, 0.4 + 0.2J]. The up-time and the down-hold are each at least
/// [`MIN_PHASE`].
///
/// `start` is the logical down the physical press created, which stands in for
/// click 0's nominal and actual down.
pub(crate) struct Grid {
    interval: Duration,
    jitter: f64,
    drift: Option<Drift>,
    rng: Rng,
    nominal: Time,
    prev_down: Time,
}

impl Grid {
    /// A grid at `cps` with Jitter `jitter` whose rate drifts by `spread` of
    /// the CPS either side, or holds at the CPS when `spread` is 0.
    pub(crate) fn new(start: Time, cps: u32, jitter: f64, spread: f64, mut rng: Rng) -> Grid {
        let drift = (spread > 0.0).then(|| Drift::new(start, cps, spread, &mut rng));
        Grid {
            interval: Duration::from_secs(1) / cps,
            jitter,
            drift,
            rng,
            nominal: start,
            prev_down: start,
        }
    }

    /// The next click's deadlines. Every click is drawn in turn, including
    /// those the scheduler then skips, so the grid never depends on timing.
    pub(crate) fn next_cycle(&mut self) -> Cycle {
        let interval = match &mut self.drift {
            Some(drift) => drift.interval_at(self.nominal, &mut self.rng),
            None => self.interval,
        };
        self.nominal = self.nominal + interval;
        let offset_ns = self.symmetric() * self.jitter / 2.0 * interval.as_nanos() as f64;
        let offset = Duration::from_nanos(offset_ns.abs().round() as u64);
        let down = if offset_ns < 0.0 {
            self.nominal - offset
        } else {
            self.nominal + offset
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

/// D5's drifting rate: piecewise linear in nominal time, ramping across each
/// [`RATE_SEGMENT`] from one target to the next, with every target drawn
/// uniformly from the band `cps · (1 ± spread)`, its top clamped to the top of
/// [`CPS_RANGE`]. The rate starts at a draw, not at the CPS.
struct Drift {
    slowest: f64,
    fastest: f64,
    segment_start: Time,
    from: f64,
    to: f64,
}

impl Drift {
    fn new(start: Time, cps: u32, spread: f64, rng: &mut Rng) -> Drift {
        let cps = f64::from(cps);
        let slowest = cps * (1.0 - spread);
        let fastest = (cps * (1.0 + spread)).min(f64::from(*CPS_RANGE.end()));
        let mut drift = Drift {
            slowest,
            fastest,
            segment_start: start,
            from: 0.0,
            to: 0.0,
        };
        drift.from = drift.target(rng);
        drift.to = drift.target(rng);
        drift
    }

    /// The nominal interval after a nominal down at `at`, one over the rate
    /// there. `at` never goes backwards.
    fn interval_at(&mut self, at: Time, rng: &mut Rng) -> Duration {
        while at - self.segment_start >= RATE_SEGMENT {
            self.segment_start = self.segment_start + RATE_SEGMENT;
            self.from = self.to;
            self.to = self.target(rng);
        }
        let along = (at - self.segment_start).as_secs_f64() / RATE_SEGMENT.as_secs_f64();
        let rate = self.from + (self.to - self.from) * along;
        Duration::from_secs_f64(1.0 / rate)
    }

    fn target(&self, rng: &mut Rng) -> f64 {
        self.slowest + rng.f64_inclusive() * (self.fastest - self.slowest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The band's (fastest, slowest) nominal intervals at `cps`.
    fn band(cps: u32) -> (Duration, Duration) {
        let fastest = (f64::from(cps) * (1.0 + RATE_SPREAD)).min(f64::from(*CPS_RANGE.end()));
        let slowest = f64::from(cps) * (1.0 - RATE_SPREAD);
        (
            Duration::from_secs_f64(1.0 / fastest),
            Duration::from_secs_f64(1.0 / slowest),
        )
    }

    /// Draws `clicks` clicks and checks D5's per-click bounds and R7's mean on
    /// the moving grid.
    fn check_grid(cps: u32, seed: u64, clicks: usize) {
        let jitter = HUMAN_JITTER;
        let start = Time::from_epoch(Duration::from_millis(250));
        let mut grid = Grid::new(start, cps, jitter, RATE_SPREAD, Rng::with_seed(seed));
        let (t_fast, t_slow) = band(cps);
        let tolerance = Duration::from_nanos(2);
        let mut prev_nominal = start;
        let mut prev_down = start;
        // T_{k−1}, zero before click 1, whose down follows the press's own.
        let mut prev_t = Duration::ZERO;
        let mut intervals = Vec::with_capacity(clicks);
        let mut nominals = Vec::with_capacity(clicks);
        for _ in 0..clicks {
            let Cycle { up, down } = grid.next_cycle();
            let t_k = grid.nominal - prev_nominal;
            assert!(
                t_k + tolerance >= t_fast && t_k <= t_slow + tolerance,
                "{cps} CPS: nominal interval {t_k:?} outside {t_fast:?}..={t_slow:?}"
            );
            let gap = down - prev_down;
            let reach = (t_k + prev_t).mul_f64(jitter / 2.0);
            assert!(
                gap + reach + tolerance >= t_k && gap <= t_k + reach + tolerance,
                "{cps} CPS: interval {gap:?} more than {reach:?} from T_k {t_k:?}"
            );
            let up_time = down - up;
            let share_lo = gap.mul_f64(0.4 - 0.2 * jitter).max(MIN_PHASE);
            let share_hi = gap.mul_f64(0.4 + 0.2 * jitter);
            assert!(
                up_time + tolerance >= share_lo && up_time <= share_hi + tolerance,
                "{cps} CPS: up-time {up_time:?} outside {share_lo:?}..={share_hi:?}"
            );
            assert!(up_time >= MIN_PHASE && gap - up_time >= MIN_PHASE);
            intervals.push(gap);
            nominals.push(t_k);
            prev_nominal = grid.nominal;
            prev_down = down;
            prev_t = t_k;
        }
        let bound = t_slow.mul_f64(jitter) / 25 + tolerance;
        for (actual, nominal) in intervals.windows(25).zip(nominals.windows(25)) {
            let actual = actual.iter().sum::<Duration>() / 25;
            let nominal = nominal.iter().sum::<Duration>() / 25;
            assert!(
                actual.abs_diff(nominal) <= bound,
                "{cps} CPS: 25-interval mean {actual:?} more than {bound:?} from {nominal:?}"
            );
        }
    }

    #[test]
    fn the_moving_grid_keeps_nominal_intervals_actual_intervals_and_the_mean_in_bounds() {
        for cps in [1, 10, 15, 50] {
            for seed in 1..=5 {
                check_grid(cps, seed, 3_000);
            }
        }
    }

    #[test]
    fn at_the_top_of_the_range_the_band_is_clamped() {
        let start = Time::ZERO;
        let mut grid = Grid::new(start, 50, HUMAN_JITTER, RATE_SPREAD, Rng::with_seed(3));
        let mut prev = start;
        let mut fastest = Duration::MAX;
        for _ in 0..3_000 {
            grid.next_cycle();
            fastest = fastest.min(grid.nominal - prev);
            prev = grid.nominal;
        }
        assert!(fastest + Duration::from_nanos(2) >= Duration::from_millis(20));
    }

    #[test]
    fn without_spread_the_grid_is_fixed() {
        let start = Time::from_epoch(Duration::from_millis(250));
        let mut grid = Grid::new(start, 15, HUMAN_JITTER, 0.0, Rng::with_seed(1));
        for k in 1..=100u32 {
            grid.next_cycle();
            assert_eq!(grid.nominal, start + Duration::from_secs(1) / 15 * k);
        }
    }
}
