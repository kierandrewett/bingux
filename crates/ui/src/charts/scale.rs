//! Scales: a linear map from data to pixels, and tick domains.
//!
//! The nice-number rule picks a step of 1, 2, 5 or 10 times a power of ten,
//! so ticks read 20, 40, 60 rather than 17.3, 34.6, 51.9. Time ticks land on
//! whole local hours and days.

/// Maps the data interval `lo..=hi` onto the pixels `from..=to`. `from` may
/// be larger than `to`, which flips the axis: a y axis runs upward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Linear {
    pub lo: f64,
    pub hi: f64,
    pub from: f32,
    pub to: f32,
}

impl Linear {
    pub fn new(domain: (f64, f64), range: (f32, f32)) -> Self {
        Self {
            lo: domain.0,
            hi: domain.1,
            from: range.0,
            to: range.1,
        }
    }

    /// The pixel for `value`. A degenerate domain maps to the middle.
    pub fn map(self, value: f64) -> f32 {
        let span = self.hi - self.lo;
        if !(span > 0.0 && span.is_finite()) {
            return (self.from + self.to) / 2.0;
        }
        let t = ((value - self.lo) / span) as f32;
        self.from + t * (self.to - self.from)
    }
}

/// A domain and its ticks. `step` is the spacing between ticks, or zero when
/// the domain is too degenerate to tick.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Domain {
    pub lo: f64,
    pub hi: f64,
    pub ticks: Vec<f64>,
    pub step: f64,
}

/// Most ticks a domain carries. A denser grid means a degenerate step.
const MAX_TICKS: f64 = 64.0;
/// Beyond this tick index, `f64` to `i64` would stop being exact.
const MAX_INDEX: f64 = 9.0e15;
/// Seconds in an hour, a day and a week.
const HOUR: f64 = 3600.0;
const DAY: f64 = 86_400.0;
/// Time steps, from a minute to a week, that a time axis may use.
const TIME_STEPS: [f64; 14] = [
    60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, HOUR, 7200.0, 10_800.0, 21_600.0, 43_200.0, DAY,
    172_800.0, 604_800.0,
];

/// The smallest 1, 2, 5 or 10 times a power of ten that is at least `raw`.
pub(crate) fn nice_step(raw: f64) -> f64 {
    let base = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|multiple| multiple * base)
        .find(|step| *step >= raw)
        .unwrap_or(10.0 * base)
}

/// A domain covering `min..=max` with about `target` nice ticks.
/// `include_zero` extends the domain to zero, for bars and areas, which rest
/// on zero. Constant data gets a window around its value.
pub(crate) fn nice(min: f64, max: f64, target: usize, include_zero: bool) -> Domain {
    let mut lo = min.min(max);
    let mut hi = min.max(max);
    if include_zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    if hi - lo <= 0.0 {
        // One value gets a window of one nice step either side: the value
        // sits in the middle and the ticks are whole steps around it.
        let window = lone(lo);
        return grid(window.lo, window.hi, window.step, 0.0);
    }
    let step = nice_step((hi - lo) / target.max(1) as f64);
    grid(lo, hi, step, 0.0)
}

/// The x axis for a single x value: a window of one unit either side, with
/// one tick, at the value itself, so the axis does not read as -1, 0, 1.
pub(crate) fn lone(value: f64) -> Domain {
    let unit = if value == 0.0 {
        1.0
    } else {
        nice_step(value.abs() * 0.2)
    };
    Domain {
        lo: value - unit,
        hi: value + unit,
        ticks: vec![value],
        step: unit,
    }
}

/// Time ticks for Unix seconds, on whole local hours and days. `offset` is
/// the local offset from UTC in seconds. A single instant gets an hour
/// either side.
pub(crate) fn time(min: f64, max: f64, target: usize, offset: f64) -> Domain {
    let mut lo = min.min(max);
    let mut hi = min.max(max);
    if hi - lo <= 0.0 {
        lo -= HOUR / 2.0;
        hi += HOUR / 2.0;
    }
    let span = hi - lo;
    let target = target.max(1) as f64;
    let step = TIME_STEPS
        .into_iter()
        .find(|step| span / step <= target)
        .unwrap_or_else(|| nice_step(span / target));
    // The plot starts and ends where the data does, so ticks outside the
    // data are dropped. A span with no tick inside keeps its two ends.
    let domain = grid(lo, hi, step, offset);
    let inside: Vec<f64> = domain
        .ticks
        .iter()
        .copied()
        .filter(|tick| (lo..=hi).contains(tick))
        .collect();
    Domain {
        lo,
        hi,
        ticks: if inside.is_empty() {
            vec![lo, hi]
        } else {
            inside
        },
        step: domain.step,
    }
}

/// Ticks on multiples of `step`, shifted by `offset`, that cover `lo..=hi`.
/// Falls back to the two ends when the step is degenerate.
fn grid(lo: f64, hi: f64, step: f64, offset: f64) -> Domain {
    let first = ((lo + offset) / step).floor();
    let last = ((hi + offset) / step).ceil();
    let sane = step.is_finite()
        && step > 0.0
        && first.abs() < MAX_INDEX
        && last.abs() < MAX_INDEX
        && last - first <= MAX_TICKS;
    if !sane {
        return Domain {
            lo,
            hi,
            ticks: vec![lo, hi],
            step: 0.0,
        };
    }
    let ticks = (first as i64..=last as i64)
        .map(|k| k as f64 * step - offset)
        .collect();
    Domain {
        lo: first * step - offset,
        hi: last * step - offset,
        ticks,
        step,
    }
}

/// Whether `step` is a whole number of days, for choosing date labels.
pub(crate) fn is_day_or_more(step: f64) -> bool {
    step >= DAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks_use_one_two_five_steps() {
        let domain = nice(0.0, 97.0, 5, false);
        assert_eq!(domain.step, 20.0);
        assert_eq!(domain.lo, 0.0);
        assert_eq!(domain.hi, 100.0);
        assert_eq!(domain.ticks.len(), 6);
    }

    #[test]
    fn constant_data_gets_a_window() {
        let domain = nice(5.0, 5.0, 4, false);
        assert!(domain.lo < 5.0 && domain.hi > 5.0);
        assert!(domain.ticks.len() >= 2);
    }

    #[test]
    fn huge_ranges_stay_bounded() {
        let domain = nice(0.0, 1e300, 5, false);
        assert!(domain.ticks.len() <= 64);
        let degenerate = nice(-1e308, 1e308, 5, false);
        assert!(degenerate.ticks.len() <= 64);
    }

    #[test]
    fn time_ticks_land_on_local_hours() {
        // Offset of +01:00: whole local hours are whole UTC hours minus one.
        let domain = time(0.0, 86_400.0, 4, 3600.0);
        for tick in &domain.ticks {
            assert_eq!((tick + 3600.0).rem_euclid(HOUR), 0.0);
        }
    }
}
