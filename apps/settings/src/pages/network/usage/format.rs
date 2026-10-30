//! How the page writes sizes, dates and ages.
//!
//! Sizes are decimal (1 GB = 1000 MB), the way a mobile plan or an internet
//! provider counts them, and the page says so once. Numbers always use a
//! point, whatever the locale, so a screenshot reads the same everywhere.

use bingux_data_usage::{Date, Totals, Zone};

const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

/// `512 B`, `1.2 GB`, `80 MB`: one decimal below ten, none from ten up.
pub fn bytes(value: u64) -> String {
    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 1000.0 && unit < UNITS.len() - 1 {
        scaled /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{value} B");
    }
    let decimals = if scaled < 10.0 { 1 } else { 0 };
    // 999.96 KB rounds to "1000 KB"; that reads better as 1.0 MB.
    let rounded = format!("{scaled:.decimals$}");
    if rounded.parse::<f64>().is_ok_and(|n| n >= 1000.0) && unit < UNITS.len() - 1 {
        return format!("{:.1} {}", scaled / 1000.0, UNITS[unit + 1]);
    }
    format!("{rounded} {}", UNITS[unit])
}

/// `↓ 1.2 GB · ↑ 80 MB`: received, then sent.
pub fn pair(totals: Totals) -> String {
    format!("↓ {} · ↑ {}", bytes(totals.rx), bytes(totals.tx))
}

/// The unit a limit is typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitUnit {
    Gb,
    Mb,
}

impl LimitUnit {
    pub fn key(self) -> &'static str {
        match self {
            Self::Gb => "GB",
            Self::Mb => "MB",
        }
    }

    pub fn from_key(key: &str) -> Self {
        if key == "MB" { Self::Mb } else { Self::Gb }
    }

    fn factor(self) -> f64 {
        match self {
            Self::Gb => 1e9,
            Self::Mb => 1e6,
        }
    }
}

/// The most a limit can be: a petabyte is far past any plan, and keeps the
/// arithmetic far from overflow.
const MAX_LIMIT: f64 = 1e15;

/// The number of bytes in a typed size such as `10` or `2.5`. A comma is
/// accepted as the decimal separator.
pub fn parse_size(text: &str, unit: LimitUnit) -> Result<u64, String> {
    let text = text.trim().replace(',', ".");
    let number: f64 = text
        .parse()
        .map_err(|_| "Enter a number, such as 10 or 2.5.".to_string())?;
    if !number.is_finite() || number <= 0.0 {
        return Err("Enter a size greater than zero.".into());
    }
    let bytes = (number * unit.factor()).round();
    if bytes > MAX_LIMIT {
        return Err("That is more than any plan.".into());
    }
    Ok(bytes as u64)
}

/// The check a size field runs: the unit does not matter for validity.
pub fn check_size(text: &str) -> Result<(), String> {
    parse_size(text, LimitUnit::Gb).map(|_| ())
}

/// A saved limit as the field and unit that show it: whole gigabytes when it
/// is at least one, else megabytes. Up to three decimals, trailing zeros cut.
pub fn size_fields(bytes: u64) -> (String, LimitUnit) {
    let unit = if bytes >= 1_000_000_000 {
        LimitUnit::Gb
    } else {
        LimitUnit::Mb
    };
    let number = format!("{:.3}", bytes as f64 / unit.factor());
    let number = number.trim_end_matches('0').trim_end_matches('.');
    (number.to_string(), unit)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// `26 Oct`.
pub fn short_date(date: Date) -> String {
    format!("{} {}", date.day, MONTHS[usize::from(date.month - 1)])
}

/// `26 Oct 2026`.
pub fn long_date(date: Date) -> String {
    format!("{} {}", short_date(date), date.year)
}

/// `Mon 26 Oct`.
pub fn weekday_date(date: Date) -> String {
    format!(
        "{} {}",
        WEEKDAYS[usize::from(date.weekday())],
        short_date(date)
    )
}

/// `2026-10-26`, the form the custom range fields take.
pub fn iso_date(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The days of a half-open `[from, to)` span, such as `26 Oct to 1 Nov`, or
/// one date when the span is a single day.
pub fn range(zone: Zone, from: i64, to: i64) -> String {
    let first = zone.date_of(from);
    let last = zone.date_of((to - 1).max(from));
    if first == last {
        long_date(first)
    } else if first.year == last.year {
        format!("{} to {}", short_date(first), long_date(last))
    } else {
        format!("{} to {}", long_date(first), long_date(last))
    }
}

/// The date in `YYYY-MM-DD`.
pub fn parse_date(text: &str) -> Result<Date, String> {
    let invalid = || "Use the form 2026-10-26.".to_string();
    let mut parts = text.trim().split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(invalid());
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return Err(invalid());
    }
    let (Ok(year), Ok(month), Ok(day)) = (year.parse(), month.parse(), day.parse()) else {
        return Err(invalid());
    };
    Date::new(year, month, day).ok_or_else(|| "That date does not exist.".to_string())
}

/// The check the custom range fields run.
pub fn check_date(text: &str) -> Result<(), String> {
    parse_date(text).map(|_| ())
}

/// The hour of the local day (0 to 23) a timestamp falls in, from the zone
/// the data was recorded in. Counting hours from midnight would be an hour off
/// after a clock change; the zone knows.
pub fn local_hour(zone: Zone, timestamp: i64) -> u32 {
    match zone {
        Zone::Fixed(offset) => ((timestamp + i64::from(offset)).rem_euclid(86_400) / 3600) as u32,
        Zone::Local => {
            let seconds = timestamp as libc::time_t;
            // SAFETY: a zeroed `tm` is a valid out-parameter for localtime_r,
            // and both pointers refer to live locals.
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            let result = unsafe { libc::localtime_r(&seconds, &mut tm) };
            if result.is_null() {
                ((timestamp.rem_euclid(86_400)) / 3600) as u32
            } else {
                tm.tm_hour as u32
            }
        }
    }
}

/// `just now`, `5 min ago`, `3 h ago`, `2 days ago`.
pub fn ago(now: i64, then: i64) -> String {
    let seconds = (now - then).max(0);
    match seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => {
            let days = seconds / 86_400;
            format!("{days} {} ago", if days == 1 { "day" } else { "days" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_decimal_units_with_few_digits() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1000), "1.0 KB");
        assert_eq!(bytes(1_234_000_000), "1.2 GB");
        assert_eq!(bytes(80_000_000), "80 MB");
        assert_eq!(bytes(9_940_000), "9.9 MB");
        assert_eq!(bytes(123_456_789_000), "123 GB");
    }

    #[test]
    fn bytes_roll_over_instead_of_printing_1000() {
        assert_eq!(bytes(999_960), "1.0 MB");
        assert_eq!(bytes(999_999_999), "1.0 GB");
        assert_eq!(bytes(u64::MAX), "18447 PB");
    }

    #[test]
    fn pair_puts_received_first() {
        let totals = Totals {
            rx: 1_200_000_000,
            tx: 80_000_000,
        };
        assert_eq!(pair(totals), "↓ 1.2 GB · ↑ 80 MB");
    }

    #[test]
    fn sizes_parse_with_point_or_comma() {
        assert_eq!(parse_size("10", LimitUnit::Gb), Ok(10_000_000_000));
        assert_eq!(parse_size(" 2.5 ", LimitUnit::Gb), Ok(2_500_000_000));
        assert_eq!(parse_size("2,5", LimitUnit::Gb), Ok(2_500_000_000));
        assert_eq!(parse_size("500", LimitUnit::Mb), Ok(500_000_000));
    }

    #[test]
    fn bad_sizes_say_what_is_wrong() {
        assert!(parse_size("", LimitUnit::Gb).is_err());
        assert!(parse_size("ten", LimitUnit::Gb).is_err());
        assert!(parse_size("0", LimitUnit::Gb).is_err());
        assert!(parse_size("-3", LimitUnit::Gb).is_err());
        assert!(parse_size("NaN", LimitUnit::Gb).is_err());
        assert!(parse_size("inf", LimitUnit::Gb).is_err());
        assert!(parse_size("1e30", LimitUnit::Gb).is_err());
    }

    #[test]
    fn a_saved_limit_shows_in_its_natural_unit() {
        assert_eq!(size_fields(10_000_000_000), ("10".into(), LimitUnit::Gb));
        assert_eq!(size_fields(2_500_000_000), ("2.5".into(), LimitUnit::Gb));
        assert_eq!(size_fields(500_000_000), ("500".into(), LimitUnit::Mb));
        assert_eq!(size_fields(1_500_000), ("1.5".into(), LimitUnit::Mb));
    }

    #[test]
    fn dates_round_trip() {
        let date = Date::new(2026, 10, 6).expect("valid date");
        assert_eq!(iso_date(date), "2026-10-06");
        assert_eq!(parse_date("2026-10-06"), Ok(date));
        assert_eq!(short_date(date), "6 Oct");
        assert_eq!(long_date(date), "6 Oct 2026");
        assert_eq!(weekday_date(date), "Tue 6 Oct");
    }

    #[test]
    fn ranges_name_both_ends() {
        let zone = Zone::Fixed(0);
        let monday = 1_792_972_800; // 26 October 2026.
        assert_eq!(range(zone, monday, monday + 86_400), "26 Oct 2026");
        assert_eq!(
            range(zone, monday, monday + 7 * 86_400),
            "26 Oct to 1 Nov 2026"
        );
        assert_eq!(
            range(zone, monday, monday + 70 * 86_400),
            "26 Oct 2026 to 3 Jan 2027"
        );
    }

    #[test]
    fn bad_dates_are_refused() {
        for text in ["", "2026-10", "26-10-2026", "2026-1-6", "2026-02-30", "x"] {
            assert!(parse_date(text).is_err(), "{text:?} should not parse");
        }
    }

    #[test]
    fn hours_follow_the_zone() {
        // 2026-10-26 13:30 UTC.
        let timestamp = 1_793_021_400;
        assert_eq!(local_hour(Zone::Fixed(0), timestamp), 13);
        assert_eq!(local_hour(Zone::Fixed(3600), timestamp), 14);
        assert_eq!(local_hour(Zone::Fixed(-14 * 3600), timestamp), 23);
    }

    #[test]
    fn ages_read_naturally() {
        assert_eq!(ago(1000, 990), "just now");
        assert_eq!(ago(1000, 400), "10 min ago");
        assert_eq!(ago(100_000, 100_000 - 7300), "2 h ago");
        assert_eq!(ago(1_000_000, 1_000_000 - 86_400), "1 day ago");
        assert_eq!(ago(1_000_000, 1_000_000 - 3 * 86_400), "3 days ago");
    }
}
