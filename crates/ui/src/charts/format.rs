//! Formatting for axes and tooltips: numbers, percentages, bytes, durations,
//! and Unix time in local time.

use std::fmt;
use std::rc::Rc;

use super::scale::is_day_or_more;

/// What a value means. It chooses how the value is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// A plain number.
    Number,
    /// A percentage, written `42%`. Values are already percentages.
    Percent,
    /// Bytes in binary units, written `3.2 GiB`.
    Bytes,
    /// Bytes in decimal units, written `3.2 GB`: 1 GB is 1000 MB, as storage
    /// vendors and network meters count. Axis ticks fall on round decimal
    /// numbers (10 GB, 20 GB).
    DecimalBytes,
    /// Seconds as a duration, written `2 h 15 min`.
    Duration,
    /// Unix seconds in local time. `offset` is the local offset from UTC in
    /// seconds. Axis labels show the time of day, or the date when the ticks
    /// are a day or more apart.
    Clock { offset: i64 },
    /// A number and a unit name, written `42 W`.
    Suffix(&'static str),
}

/// A value formatter the caller writes through `y_format`, for anything a
/// [`Unit`] does not cover (a rate such as `4.2 Mbit/s`, a temperature, a
/// currency). It gets the value and returns the text for an axis tick or a
/// tooltip row.
#[derive(Clone)]
pub(crate) struct Formatter(Rc<dyn Fn(f64) -> String>);

impl Formatter {
    pub fn new(format: impl Fn(f64) -> String + 'static) -> Self {
        Self(Rc::new(format))
    }

    /// The text for `value`.
    pub fn apply(&self, value: f64) -> String {
        (self.0)(value)
    }
}

impl PartialEq for Formatter {
    /// Two formatters are equal when they are the same closure.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Formatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Formatter(..)")
    }
}

/// How one axis writes its values: the caller's formatter when there is
/// one, else the unit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValueText {
    pub unit: Unit,
    pub custom: Option<Formatter>,
}

impl ValueText {
    pub fn new(unit: Unit) -> Self {
        Self { unit, custom: None }
    }

    /// The text of an axis tick. `step` is the spacing between ticks.
    pub fn tick(&self, value: f64, step: f64) -> String {
        match &self.custom {
            Some(custom) => (custom.0)(value),
            None => format_tick(value, self.unit, step),
        }
    }

    /// The text of a value in a tooltip or a label.
    pub fn value(&self, value: f64) -> String {
        match &self.custom {
            Some(custom) => (custom.0)(value),
            None => format_value(value, self.unit),
        }
    }
}

/// The text of an axis tick. `step` is the spacing between ticks.
pub(crate) fn format_tick(value: f64, unit: Unit, step: f64) -> String {
    match unit {
        Unit::Clock { offset } if is_day_or_more(step) => date(value, offset as f64),
        Unit::Clock { offset } => clock_time(value, offset as f64),
        Unit::Duration => duration(value),
        Unit::Bytes => bytes(value),
        Unit::DecimalBytes => decimal_bytes(value),
        Unit::Percent => format!("{}%", fixed(value, decimals_for(step))),
        Unit::Suffix(suffix) => format!("{} {suffix}", fixed(value, decimals_for(step))),
        Unit::Number => fixed(value, decimals_for(step)),
    }
}

/// The text of a value in a tooltip or summary: full precision and unit.
pub(crate) fn format_value(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Clock { offset } => {
            let offset = offset as f64;
            format!("{} {}", date(value, offset), clock_time(value, offset))
        }
        Unit::Percent => format!("{}%", fixed(value, precision(value))),
        Unit::Suffix(suffix) => format!("{} {suffix}", fixed(value, precision(value))),
        Unit::Number => fixed(value, precision(value)),
        Unit::Bytes => bytes(value),
        Unit::DecimalBytes => decimal_bytes(value),
        Unit::Duration => duration(value),
    }
}

/// Decimals an axis needs so that neighbouring ticks read apart.
pub(crate) fn decimals_for(step: f64) -> usize {
    if !(step.is_finite() && step > 0.0) || step >= 1.0 {
        return 0;
    }
    ((-step.log10()).ceil() as usize).min(6)
}

/// Decimals for a tooltip value: none for whole numbers, then one or two.
fn precision(value: f64) -> usize {
    if (value - value.round()).abs() < 1e-9 {
        0
    } else if value.abs() >= 10.0 {
        1
    } else {
        2
    }
}

fn fixed(value: f64, decimals: usize) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    format!("{value:.decimals$}")
}

/// Binary byte units: `512 B`, `1.5 KiB`, `3.2 GiB`.
pub(crate) fn bytes(value: f64) -> String {
    scaled_bytes(value, 1024.0, &["B", "KiB", "MiB", "GiB", "TiB", "PiB"])
}

/// Decimal byte units: `512 B`, `1.5 KB`, `3.2 GB`.
pub(crate) fn decimal_bytes(value: f64) -> String {
    scaled_bytes(value, 1000.0, &["B", "KB", "MB", "GB", "TB", "PB"])
}

/// `value` in the largest unit that keeps it at 1 or more, with a decimal
/// only while it helps (under 10 and not whole).
fn scaled_bytes(value: f64, base: f64, units: &[&str]) -> String {
    if !value.is_finite() {
        return "–".into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let mut scaled = value.abs();
    let mut unit = 0;
    while scaled >= base && unit < units.len() - 1 {
        scaled /= base;
        unit += 1;
    }
    let tenths = (scaled * 10.0).round() / 10.0;
    let decimals = if unit == 0 || tenths >= 10.0 || scaled.fract().abs() < 0.05 {
        0
    } else {
        1
    };
    format!("{sign}{scaled:.decimals$} {}", units[unit])
}

/// A duration in its two largest units: `45 s`, `12 min`, `2 h 15 min`,
/// `3 d 4 h`.
pub(crate) fn duration(seconds: f64) -> String {
    if !seconds.is_finite() {
        return "–".into();
    }
    let sign = if seconds < 0.0 { "-" } else { "" };
    let total = seconds.abs().round() as u64;
    let (days, hours, minutes, secs) = (
        total / 86_400,
        total % 86_400 / 3600,
        total % 3600 / 60,
        total % 60,
    );
    let text = if days > 0 {
        join(days, "d", hours, "h")
    } else if hours > 0 {
        join(hours, "h", minutes, "min")
    } else if minutes > 0 {
        format!("{minutes} min")
    } else {
        format!("{secs} s")
    };
    format!("{sign}{text}")
}

fn join(major: u64, major_unit: &str, minor: u64, minor_unit: &str) -> String {
    if minor > 0 {
        format!("{major} {major_unit} {minor} {minor_unit}")
    } else {
        format!("{major} {major_unit}")
    }
}

/// Local time as `HH:MM`.
fn clock_time(seconds: f64, offset: f64) -> String {
    let (_, _, _, hour, minute) = parts(seconds, offset);
    format!("{hour:02}:{minute:02}")
}

/// Local date as `23 Oct`.
fn date(seconds: f64, offset: f64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (_, month, day, _, _) = parts(seconds, offset);
    format!("{day} {}", MONTHS[(month - 1) as usize])
}

/// Year, month, day, hour and minute of a Unix time in local time.
fn parts(seconds: f64, offset: f64) -> (i64, u32, u32, u32, u32) {
    let local = (seconds + offset).floor() as i64;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (secs / 3600) as u32,
        (secs % 3600 / 60) as u32,
    )
}

/// The proleptic Gregorian date of a day count from 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units() {
        assert_eq!(bytes(512.0), "512 B");
        assert_eq!(bytes(1024.0), "1 KiB");
        assert_eq!(bytes(3.2 * 1024.0 * 1024.0 * 1024.0), "3.2 GiB");
    }

    #[test]
    fn decimal_bytes_use_decimal_units() {
        assert_eq!(decimal_bytes(0.0), "0 B");
        assert_eq!(decimal_bytes(144.0), "144 B");
        assert_eq!(decimal_bytes(158_000.0), "158 KB");
        assert_eq!(decimal_bytes(9.6e6), "9.6 MB");
        assert_eq!(decimal_bytes(3.8e9), "3.8 GB");
        assert_eq!(decimal_bytes(23.4e9), "23 GB");
        // 9.96 rounds up to ten, which needs no decimal.
        assert_eq!(decimal_bytes(9.96e9), "10 GB");
    }

    #[test]
    fn a_callers_formatter_wins_over_the_unit() {
        let plain = ValueText::new(Unit::Number);
        assert_eq!(plain.tick(20.0, 10.0), "20");
        let mut custom = plain.clone();
        custom.custom = Some(Formatter::new(|value| format!("{value:.1} Mbit/s")));
        assert_eq!(custom.tick(20.0, 10.0), "20.0 Mbit/s");
        assert_eq!(custom.value(4.26), "4.3 Mbit/s");
    }

    #[test]
    fn durations_use_two_units() {
        assert_eq!(duration(45.0), "45 s");
        assert_eq!(duration(720.0), "12 min");
        assert_eq!(duration(8100.0), "2 h 15 min");
        assert_eq!(duration(273_600.0), "3 d 4 h");
    }

    #[test]
    fn clock_uses_local_time_and_date() {
        // 2026-10-23 14:05 UTC, shown at +01:00.
        let seconds = 1_792_764_300.0;
        assert_eq!(clock_time(seconds, 3600.0), "15:05");
        assert_eq!(date(seconds, 3600.0), "23 Oct");
    }

    #[test]
    fn decimals_follow_the_step() {
        assert_eq!(decimals_for(20.0), 0);
        assert_eq!(decimals_for(0.5), 1);
        assert_eq!(decimals_for(0.02), 2);
    }
}
