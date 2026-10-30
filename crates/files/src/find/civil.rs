//! Calendar days, and what the clock says now: just enough for `modified:`.
//!
//! The date arithmetic is the proleptic Gregorian calendar by days since
//! 1970-01-01. "Today" is the person's own: [`Clock::system`] asks the C
//! library for the zone's offset at the moment in question, so a day that
//! has 23 or 25 hours is measured right. Tests use [`Clock::fixed`].

use std::fmt::{self, Display};
use std::time::{SystemTime, UNIX_EPOCH};

/// A day on the calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// A year, a month of a year, or a day: how much of the calendar a person
/// wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    pub year: i32,
    pub month: Option<u8>,
    pub day: Option<u8>,
}

fn leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap(year) => 29,
        _ => 28,
    }
}

/// Days from 1970-01-01 to `date`.
pub fn days_from_civil(date: Date) -> i64 {
    let year = i64::from(date.year) - i64::from(date.month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(date.month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(date.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The day `days` after 1970-01-01.
pub fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    Date {
        year: (year + i64::from(month <= 2)) as i32,
        month,
        day,
    }
}

impl Moment {
    /// `2024`, `2024-05` or `2024-05-17`; `None` for anything else or a day
    /// that is not on the calendar.
    pub fn parse(text: &str) -> Option<Moment> {
        let mut parts = text.trim().split('-');
        let year_text = parts.next()?;
        if year_text.len() != 4 || !year_text.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let year: i32 = year_text.parse().ok()?;
        if year < 1 {
            return None;
        }
        let number = |part: Option<&str>| -> Option<Option<u8>> {
            match part {
                None => Some(None),
                Some(part) if part.len() == 2 && part.chars().all(|c| c.is_ascii_digit()) => {
                    part.parse().ok().map(Some)
                }
                Some(_) => None,
            }
        };
        let month = number(parts.next())?;
        let day = number(parts.next())?;
        if parts.next().is_some() || (month.is_none() && day.is_some()) {
            return None;
        }
        if let Some(month) = month {
            if !(1..=12).contains(&month) {
                return None;
            }
            if let Some(day) = day
                && !(1..=days_in_month(year, month)).contains(&day)
            {
                return None;
            }
        }
        Some(Moment { year, month, day })
    }

    /// The first day it covers.
    pub fn start(self) -> Date {
        Date {
            year: self.year,
            month: self.month.unwrap_or(1),
            day: self.day.unwrap_or(1),
        }
    }

    /// The day after the last day it covers.
    pub fn end(self) -> Date {
        match (self.month, self.day) {
            (None, _) => Date {
                year: self.year + 1,
                month: 1,
                day: 1,
            },
            (Some(12), None) => Date {
                year: self.year + 1,
                month: 1,
                day: 1,
            },
            (Some(month), None) => Date {
                year: self.year,
                month: month + 1,
                day: 1,
            },
            (Some(_), Some(_)) => civil_from_days(days_from_civil(self.start()) + 1),
        }
    }
}

impl Display for Moment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(month) = self.month {
            write!(f, "-{month:02}")?;
            if let Some(day) = self.day {
                write!(f, "-{day:02}")?;
            }
        }
        Ok(())
    }
}

impl Moment {
    /// The day `date` as a moment.
    pub fn from_date(date: Date) -> Moment {
        Moment {
            year: date.year,
            month: Some(date.month),
            day: Some(date.day),
        }
    }

    /// The last day it covers, as a moment of one day.
    pub fn last_day(self) -> Moment {
        shift_days(Moment::from_date(self.end()), -1)
    }
}

/// `text` as one day (`YYYY-MM-DD`); a year or a month alone is not one.
pub fn parse_day(text: &str) -> Option<Moment> {
    Moment::parse(text).filter(|moment| moment.day.is_some())
}

/// The first day `moment` covers, as `YYYY-MM-DD`.
pub fn day_text(moment: Moment) -> String {
    Moment::from_date(moment.start()).to_string()
}

/// The day `days` after the first day `moment` covers, as a moment of one
/// day.
pub fn shift_days(moment: Moment, days: i64) -> Moment {
    Moment::from_date(civil_from_days(days_from_civil(moment.start()) + days))
}

/// The moment a search began and the zone it is in.
#[derive(Clone, Copy, Debug)]
pub struct Clock {
    /// Seconds since 1970-01-01 00:00 UTC.
    pub now: i64,
    zone: Zone,
}

#[derive(Clone, Copy, Debug)]
enum Zone {
    System,
    Fixed(i64),
}

impl Clock {
    /// Now, in the zone of the machine.
    pub fn system() -> Clock {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64);
        Clock {
            now,
            zone: Zone::System,
        }
    }

    /// `now` seconds after the epoch in a zone `offset` seconds east of UTC.
    pub fn fixed(now: i64, offset: i64) -> Clock {
        Clock {
            now,
            zone: Zone::Fixed(offset),
        }
    }

    /// Seconds east of UTC at `at`.
    fn offset(&self, at: i64) -> i64 {
        match self.zone {
            Zone::Fixed(offset) => offset,
            Zone::System => {
                let time = at as libc::time_t;
                let mut parts = std::mem::MaybeUninit::<libc::tm>::zeroed();
                // SAFETY: `localtime_r` writes a `tm` through the pointer and
                // reads `time` through another; both are valid for the call.
                let done = unsafe { libc::localtime_r(&time, parts.as_mut_ptr()) };
                if done.is_null() {
                    return 0;
                }
                // SAFETY: `localtime_r` returned non-null, so it filled `parts`.
                unsafe { parts.assume_init() }.tm_gmtoff
            }
        }
    }

    /// The day it is here.
    pub fn today(&self) -> Date {
        civil_from_days((self.now + self.offset(self.now)).div_euclid(86_400))
    }

    /// When `date` starts here, in seconds since the epoch.
    pub fn midnight(&self, date: Date) -> i64 {
        let utc = days_from_civil(date) * 86_400;
        let guess = utc - self.offset(utc);
        utc - self.offset(guess)
    }
}
