//! Local calendar arithmetic: days, weeks, months and billing cycles.
//!
//! Minute and hour buckets are plain UTC epoch seconds. Daily buckets are
//! **local calendar days**: `bucket_start` of a `daily` row is the UTC epoch
//! second of local midnight. A [`Zone`] turns timestamps into local dates
//! and back; `Zone::Local` asks the C library (`localtime_r`/`mktime`, so
//! `TZ` and DST are honoured), `Zone::Fixed` is a constant offset used by
//! tests and by callers that want deterministic days.

use std::time::{SystemTime, UNIX_EPOCH};

/// The seconds since the Unix epoch, now.
pub fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

/// A calendar date without a time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Date {
    /// `None` when the month or day does not exist in that year.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Days since 1970-01-01 (proleptic Gregorian).
    pub fn to_days(self) -> i64 {
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year.rem_euclid(400);
        let month = i64::from(self.month);
        let shifted = (month + 9) % 12;
        let day_of_year = (153 * shifted + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// The inverse of [`Date::to_days`].
    pub fn from_days(days: i64) -> Date {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u8;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        } as u8;
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Date {
            year: year as i32,
            month,
            day,
        }
    }

    pub fn add_days(self, days: i64) -> Date {
        Date::from_days(self.to_days() + days)
    }

    /// Monday is 0, Sunday is 6.
    pub fn weekday(self) -> u8 {
        // 1970-01-01 was a Thursday (3).
        (self.to_days() + 3).rem_euclid(7) as u8
    }

    /// The first day of this date's month.
    pub fn first_of_month(self) -> Date {
        Date { day: 1, ..self }
    }

    /// The first day of the following month.
    pub fn next_month(self) -> Date {
        if self.month == 12 {
            Date {
                year: self.year + 1,
                month: 1,
                day: 1,
            }
        } else {
            Date {
                month: self.month + 1,
                day: 1,
                ..self
            }
        }
    }

    /// The first day of the previous month.
    pub fn previous_month(self) -> Date {
        if self.month == 1 {
            Date {
                year: self.year - 1,
                month: 12,
                day: 1,
            }
        } else {
            Date {
                month: self.month - 1,
                day: 1,
                ..self
            }
        }
    }
}

/// Days in `month` (1 to 12) of `year`.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        _ => 28,
    }
}

/// The time zone that defines where a "day" starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// The process's local zone, DST included.
    Local,
    /// A constant offset east of UTC in seconds. Use whole minutes: bucket
    /// boundaries are minute-aligned.
    Fixed(i32),
}

impl Zone {
    /// The local date at UTC epoch second `timestamp`.
    pub fn date_of(self, timestamp: i64) -> Date {
        match self {
            Zone::Fixed(offset) => {
                Date::from_days((timestamp + i64::from(offset)).div_euclid(86_400))
            }
            Zone::Local => match local_tm(timestamp) {
                Some(tm) => Date {
                    year: tm.tm_year + 1900,
                    month: (tm.tm_mon + 1) as u8,
                    day: tm.tm_mday as u8,
                },
                // localtime_r fails only for years outside `time_t`'s calendar
                // range; UTC is the honest answer then.
                None => Zone::Fixed(0).date_of(timestamp),
            },
        }
    }

    /// The UTC epoch second of 00:00 local time on `date`.
    pub fn date_start(self, date: Date) -> i64 {
        match self {
            Zone::Fixed(offset) => date.to_days() * 86_400 - i64::from(offset),
            Zone::Local => {
                // SAFETY: `tm` is plain data; zeroed is a valid value and
                // every field mktime reads is set below.
                let mut tm: libc::tm = unsafe { std::mem::zeroed() };
                tm.tm_year = date.year - 1900;
                tm.tm_mon = i32::from(date.month) - 1;
                tm.tm_mday = i32::from(date.day);
                tm.tm_isdst = -1;
                // SAFETY: `tm` is a valid, exclusively borrowed struct.
                let start = unsafe { libc::mktime(&mut tm) };
                if start == -1 {
                    Zone::Fixed(0).date_start(date)
                } else {
                    start as i64
                }
            }
        }
    }

    /// The local midnight at or before `timestamp`.
    pub fn day_start(self, timestamp: i64) -> i64 {
        self.date_start(self.date_of(timestamp))
    }

    /// The local midnight after the day containing `timestamp`. A day is 23
    /// or 25 hours long across a DST change.
    pub fn next_day_start(self, timestamp: i64) -> i64 {
        self.date_start(self.date_of(timestamp).add_days(1))
    }

    /// Today's date in this zone.
    pub fn today(self, now: i64) -> Date {
        self.date_of(now)
    }
}

fn local_tm(timestamp: i64) -> Option<libc::tm> {
    let seconds = timestamp as libc::time_t;
    // SAFETY: zeroed `tm` is a valid out-parameter for localtime_r.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers refer to live, exclusively borrowed locals.
    let result = unsafe { libc::localtime_r(&seconds, &mut tm) };
    if result.is_null() { None } else { Some(tm) }
}

/// The billing cycle containing `today`, as `(first day, first day of the
/// next cycle)`. A cycle starts on `billing_day` of a month; a month too
/// short for it (day 31 in a 30-day month) starts on its last day.
/// `billing_day` is clamped to 1..=31.
pub fn billing_cycle(today: Date, billing_day: u8) -> (Date, Date) {
    let billing_day = billing_day.clamp(1, 31);
    let start_in = |month_first: Date| Date {
        day: billing_day.min(days_in_month(month_first.year, month_first.month)),
        ..month_first
    };
    let this_month = start_in(today.first_of_month());
    let start = if this_month <= today {
        this_month
    } else {
        start_in(today.first_of_month().previous_month())
    };
    let end = start_in(start.first_of_month().next_month());
    (start, end)
}

/// The Monday on or before `today`.
pub fn week_start(today: Date) -> Date {
    today.add_days(-i64::from(today.weekday()))
}

/// The first day of `today`'s month.
pub fn month_start(today: Date) -> Date {
    today.first_of_month()
}

/// Half-open `[from, to)` UTC epoch ranges for the usual "so far" periods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Periods {
    /// Local midnight to the next local midnight.
    pub today: (i64, i64),
    /// Monday 00:00 local to the next Monday 00:00.
    pub week: (i64, i64),
    /// The first of the month 00:00 local to the first of the next month.
    pub month: (i64, i64),
    /// The billing cycle containing today; `.1` is when the next one starts.
    pub cycle: (i64, i64),
}

impl Periods {
    pub fn at(zone: Zone, now: i64, billing_day: u8) -> Periods {
        let today = zone.today(now);
        let week = week_start(today);
        let month = month_start(today);
        let (cycle_start, cycle_end) = billing_cycle(today, billing_day);
        Periods {
            today: (zone.date_start(today), zone.date_start(today.add_days(1))),
            week: (zone.date_start(week), zone.date_start(week.add_days(7))),
            month: (zone.date_start(month), zone.date_start(month.next_month())),
            cycle: (zone.date_start(cycle_start), zone.date_start(cycle_end)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> Date {
        Date::new(year, month, day).expect("valid date")
    }

    #[test]
    fn days_round_trip_and_weekday() {
        assert_eq!(date(1970, 1, 1).to_days(), 0);
        assert_eq!(date(1970, 1, 1).weekday(), 3);
        assert_eq!(date(2026, 10, 26).weekday(), 0, "2026-10-26 is a Monday");
        for days in [-800_000, -1, 0, 59, 60, 19_000, 20_000, 800_000] {
            assert_eq!(Date::from_days(days).to_days(), days);
        }
        assert_eq!(Date::from_days(19_723), date(2024, 1, 1));
    }

    #[test]
    fn invalid_dates_are_rejected() {
        assert!(Date::new(2025, 2, 29).is_none());
        assert!(Date::new(2024, 2, 29).is_some());
        assert!(Date::new(2024, 13, 1).is_none());
        assert!(Date::new(2024, 4, 31).is_none());
    }

    #[test]
    fn billing_day_31_clamps_in_short_months() {
        // 30-day month: the cycle that contains 30 April starts on 30 April.
        assert_eq!(
            billing_cycle(date(2025, 4, 30), 31),
            (date(2025, 4, 30), date(2025, 5, 31))
        );
        // Early May still belongs to the cycle that began on 30 April.
        assert_eq!(
            billing_cycle(date(2025, 5, 5), 31),
            (date(2025, 4, 30), date(2025, 5, 31))
        );
        // February of a leap and a normal year.
        assert_eq!(
            billing_cycle(date(2024, 2, 29), 31),
            (date(2024, 2, 29), date(2024, 3, 31))
        );
        assert_eq!(
            billing_cycle(date(2025, 3, 1), 31),
            (date(2025, 2, 28), date(2025, 3, 31))
        );
    }

    #[test]
    fn billing_cycle_wraps_the_year() {
        assert_eq!(
            billing_cycle(date(2025, 1, 10), 15),
            (date(2024, 12, 15), date(2025, 1, 15))
        );
        assert_eq!(
            billing_cycle(date(2025, 12, 20), 15),
            (date(2025, 12, 15), date(2026, 1, 15))
        );
        assert_eq!(
            billing_cycle(date(2025, 6, 15), 15),
            (date(2025, 6, 15), date(2025, 7, 15)),
            "the billing day itself starts the new cycle"
        );
        assert_eq!(
            billing_cycle(date(2025, 6, 10), 1),
            (date(2025, 6, 1), date(2025, 7, 1))
        );
        assert_eq!(
            billing_cycle(date(2025, 6, 10), 0),
            (date(2025, 6, 1), date(2025, 7, 1)),
            "0 clamps to 1"
        );
    }

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(week_start(date(2026, 10, 26)), date(2026, 10, 26));
        assert_eq!(week_start(date(2026, 10, 25)), date(2026, 10, 19));
        assert_eq!(week_start(date(2026, 11, 1)), date(2026, 10, 26));
        assert_eq!(week_start(date(2026, 1, 1)), date(2025, 12, 29));
        assert_eq!(month_start(date(2026, 10, 26)), date(2026, 10, 1));
    }

    #[test]
    fn fixed_zone_days() {
        let zone = Zone::Fixed(3600);
        // 2025-06-01 23:30 UTC is already 2025-06-02 00:30 at UTC+1.
        let late = date(2025, 6, 1).to_days() * 86_400 + 23 * 3600 + 1800;
        assert_eq!(zone.date_of(late), date(2025, 6, 2));
        assert_eq!(
            zone.day_start(late),
            date(2025, 6, 2).to_days() * 86_400 - 3600
        );
        assert_eq!(zone.next_day_start(late) - zone.day_start(late), 86_400);
        assert_eq!(Zone::Fixed(-5 * 3600).date_of(0), date(1969, 12, 31));
    }

    #[test]
    fn periods_cover_the_expected_days() {
        let zone = Zone::Fixed(0);
        let now = zone.date_start(date(2026, 10, 28)) + 3600; // Wednesday
        let periods = Periods::at(zone, now, 15);
        assert_eq!(
            periods.today,
            (
                zone.date_start(date(2026, 10, 28)),
                zone.date_start(date(2026, 10, 29))
            )
        );
        assert_eq!(
            periods.week,
            (
                zone.date_start(date(2026, 10, 26)),
                zone.date_start(date(2026, 11, 2))
            )
        );
        assert_eq!(
            periods.month,
            (
                zone.date_start(date(2026, 10, 1)),
                zone.date_start(date(2026, 11, 1))
            )
        );
        assert_eq!(
            periods.cycle,
            (
                zone.date_start(date(2026, 10, 15)),
                zone.date_start(date(2026, 11, 15))
            )
        );
    }

    #[test]
    fn local_zone_days_are_consistent() {
        // Holds for whatever TZ the test runs under, DST changes included.
        let zone = Zone::Local;
        for now in [
            0,
            1_700_000_000,
            1_711_846_000,
            1_730_000_000,
            1_792_000_000,
        ] {
            let start = zone.day_start(now);
            let next = zone.next_day_start(now);
            assert!(start <= now && now < next, "{start} <= {now} < {next}");
            assert!((23 * 3600..=25 * 3600).contains(&(next - start)));
            assert_eq!(zone.day_start(start), start);
            assert_eq!(zone.date_of(next), zone.date_of(now).add_days(1));
        }
    }
}
