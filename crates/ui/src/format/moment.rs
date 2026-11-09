//! A point in time and the time zone it is read in.

use std::time::SystemTime;

use jiff::Timestamp;
use jiff::civil;
use jiff::tz::{Offset, TimeZone};

/// A point in time: seconds since 1970-01-01 00:00 UTC, with nanoseconds.
///
/// It names no zone. A [`Zone`] says which wall clock reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Moment(Timestamp);

impl Moment {
    /// The time now.
    pub fn now() -> Moment {
        Moment(Timestamp::now())
    }

    /// `seconds` after the epoch (before it when negative). A time outside
    /// the years -9999 to 9999 is clamped to the nearest edge.
    pub fn from_unix(seconds: i64) -> Moment {
        Moment(Timestamp::from_second(seconds).unwrap_or(if seconds < 0 {
            Timestamp::MIN
        } else {
            Timestamp::MAX
        }))
    }

    /// `millis` milliseconds after the epoch.
    pub fn from_unix_millis(millis: i64) -> Moment {
        Moment(
            Timestamp::from_millisecond(millis).unwrap_or(if millis < 0 {
                Timestamp::MIN
            } else {
                Timestamp::MAX
            }),
        )
    }

    /// Whole seconds after the epoch, rounded down.
    pub fn unix_seconds(self) -> i64 {
        self.0.as_second()
    }

    /// This moment moved `seconds` later (earlier when negative).
    pub fn plus_seconds(self, seconds: i64) -> Moment {
        Moment::from_unix(self.unix_seconds().saturating_add(seconds))
    }

    pub(crate) fn timestamp(self) -> Timestamp {
        self.0
    }
}

impl From<SystemTime> for Moment {
    fn from(time: SystemTime) -> Moment {
        Moment(Timestamp::try_from(time).unwrap_or(
            match time.duration_since(SystemTime::UNIX_EPOCH) {
                Ok(_) => Timestamp::MAX,
                Err(_) => Timestamp::MIN,
            },
        ))
    }
}

/// The wall clock a [`Moment`] is read on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Zone {
    /// The system's zone (`/etc/localtime`, or `TZ`), with its daylight
    /// saving rules.
    #[default]
    Local,
    /// Coordinated Universal Time.
    Utc,
    /// A fixed offset from UTC in seconds east (`3600` is UTC+01:00).
    Offset(i32),
    /// An IANA zone such as `Europe/Paris`; make one with [`Zone::named`].
    Named(String),
}

impl Zone {
    /// The IANA zone `id` (`America/New_York`), or `None` when there is no
    /// such zone. The zone database is part of the program, so this does not
    /// depend on the host.
    pub fn named(id: &str) -> Option<Zone> {
        TimeZone::get(id).ok().map(|_| Zone::Named(id.to_string()))
    }

    pub(crate) fn time_zone(&self) -> TimeZone {
        match self {
            Zone::Local => TimeZone::system(),
            Zone::Utc => TimeZone::UTC,
            Zone::Offset(seconds) => Offset::from_seconds(*seconds)
                .map(TimeZone::fixed)
                .unwrap_or(TimeZone::UTC),
            Zone::Named(id) => TimeZone::get(id).unwrap_or(TimeZone::UTC),
        }
    }

    /// Seconds east of UTC that `moment` is read at in this zone: `3600` in
    /// Paris in winter, `7200` in summer.
    pub fn offset_at(&self, moment: Moment) -> i32 {
        self.time_zone().to_offset(moment.0).seconds()
    }

    /// The wall clock reading of `moment` in this zone.
    pub fn civil(&self, moment: Moment) -> Civil {
        let wall = self.time_zone().to_datetime(moment.0);
        Civil {
            year: i32::from(wall.year()),
            month: wall.month() as u8,
            day: wall.day() as u8,
            hour: wall.hour() as u8,
            minute: wall.minute() as u8,
            second: wall.second() as u8,
            nanosecond: wall.subsec_nanosecond() as u32,
        }
    }

    /// The moment the wall clock reads `civil` in this zone. In the hour a
    /// clock change repeats it is the first reading; in the hour it skips it
    /// is the moment after the gap. `None` for a reading outside the years
    /// -9999 to 9999 or one that is not a date (31 February).
    pub fn moment_of(&self, civil: Civil) -> Option<Moment> {
        let wall = civil::DateTime::new(
            civil.year.try_into().ok()?,
            civil.month.try_into().ok()?,
            civil.day.try_into().ok()?,
            civil.hour.try_into().ok()?,
            civil.minute.try_into().ok()?,
            civil.second.try_into().ok()?,
            civil.nanosecond.try_into().ok()?,
        )
        .ok()?;
        self.time_zone()
            .to_ambiguous_timestamp(wall)
            .compatible()
            .ok()
            .map(Moment)
    }
}

/// A date and time of day as a wall clock shows them, with no zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Civil {
    pub year: i32,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59.
    pub second: u8,
    pub nanosecond: u32,
}

impl Civil {
    /// A date at midnight.
    pub const fn date(year: i32, month: u8, day: u8) -> Civil {
        Civil {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
            nanosecond: 0,
        }
    }

    /// Days since 1970-01-01 of this date: the difference of two is the
    /// number of calendar days between them, whatever the time of day.
    pub fn day_number(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let month_index = (i64::from(self.month) + 9) % 12;
        let day_of_year = (153 * month_index + 2) / 5 + i64::from(self.day) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
}
