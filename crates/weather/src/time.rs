//! Dates and times without a time zone library.
//!
//! A forecast carries every time as *wall-clock seconds at the place*: the
//! seconds from 1970-01-01 00:00 of the clock on the wall there, as if that
//! clock ran in UTC. Open-Meteo is asked for `timezone=auto`, so its local
//! times parse straight into that form, and nothing after the fetch needs the
//! zone's rules. The instant "now" at a place is `unix_now() + utc_offset`.

use std::time::{SystemTime, UNIX_EPOCH};

pub const DAY: i64 = 86_400;
pub const HOUR: i64 = 3_600;

/// Seconds since the Unix epoch, now.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The civil date `(year, month, day)` of a day count since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Parses `2026-11-09T14:00` or `2026-11-09` (seconds are optional) into
/// wall-clock seconds.
pub fn parse_local(text: &str) -> Option<i64> {
    let (date, clock) = match text.split_once('T') {
        Some((date, clock)) => (date, Some(clock)),
        None => (text, None),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut seconds = days_from_civil(year, month, day) * DAY;
    if let Some(clock) = clock {
        let mut parts = clock.split(':');
        let hour: i64 = parts.next()?.parse().ok()?;
        let minute: i64 = parts.next()?.parse().ok()?;
        let second: i64 = match parts.next() {
            Some(second) => second.trim_end_matches('Z').parse().ok()?,
            None => 0,
        };
        seconds += hour * HOUR + minute * 60 + second;
    }
    Some(seconds)
}

/// The day number (days since 1970-01-01) a wall-clock time falls on.
pub fn day_of(wall: i64) -> i64 {
    wall.div_euclid(DAY)
}

/// The hour of the day, 0 to 23.
pub fn hour_of(wall: i64) -> u32 {
    (wall.rem_euclid(DAY) / HOUR) as u32
}

/// The minute of the hour, 0 to 59.
pub fn minute_of(wall: i64) -> u32 {
    (wall.rem_euclid(HOUR) / 60) as u32
}

/// 0 for Monday to 6 for Sunday.
pub fn weekday(wall: i64) -> usize {
    (day_of(wall) + 3).rem_euclid(7) as usize
}

/// The month, 0 for January to 11 for December, and the day of the month.
pub fn month_day(wall: i64) -> (usize, u32) {
    let (_, month, day) = civil_from_days(day_of(wall));
    ((month - 1) as usize, day)
}

/// Whether times are written on a 12-hour clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Clock {
    #[default]
    TwentyFour,
    Twelve,
}

/// How long ago something was, in the largest whole unit: the app words it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ago {
    JustNow,
    Minutes(i64),
    Hours(i64),
    Days(i64),
}

/// How long ago a moment `seconds` back was.
pub fn ago(seconds: i64) -> Ago {
    let seconds = seconds.max(0);
    match seconds {
        0..=59 => Ago::JustNow,
        60..=3_599 => Ago::Minutes(seconds / 60),
        3_600..=86_399 => Ago::Hours(seconds / 3_600),
        _ => Ago::Days(seconds / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for days in [-1, 0, 1, 365, 10_000, 20_000, 20_401] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn parses_open_meteo_times() {
        let t = parse_local("2026-11-09T14:45").unwrap();
        assert_eq!(hour_of(t), 14);
        assert_eq!(minute_of(t), 45);
        assert_eq!(weekday(t), 0);
        assert_eq!(month_day(t), (10, 9));
        assert_eq!(parse_local("2026-11-09").unwrap(), t - 14 * HOUR - 45 * 60);
        assert!(parse_local("not a time").is_none());
    }

    #[test]
    fn ago_reads_plainly() {
        assert_eq!(ago(10), Ago::JustNow);
        assert_eq!(ago(300), Ago::Minutes(5));
        assert_eq!(ago(7_300), Ago::Hours(2));
        assert_eq!(ago(3 * DAY), Ago::Days(3));
    }
}
