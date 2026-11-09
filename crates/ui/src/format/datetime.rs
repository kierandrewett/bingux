//! Dates and times of day.

use std::fmt;

use icu::datetime::fieldsets::{D, E, M, MD, MDE, MDET, MDT, T, Y, YM, YMD, YMDE, YMDET, YMDT};
use icu::datetime::input::{Date as IcuDate, DateTime as IcuDateTime, Time as IcuTime};
use icu::datetime::preferences::HourCycle as IcuHourCycle;
use icu::datetime::{DateTimeFormatter, DateTimeFormatterPreferences, NoCalendarFormatter};

use super::cache::cached;
use super::moment::{Civil, Moment, Zone};
use crate::locale::{Category, Locale};

/// How much of a date is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Length {
    /// Numbers only: `09/11/2026`.
    Short,
    /// Abbreviated month: `9 Nov 2026`.
    #[default]
    Medium,
    /// The month in full: `9 November 2026`.
    Long,
}

/// Whether times are written on a 12 or a 24 hour clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HourCycle {
    /// As the locale writes it: `14:05` in `en-GB`, `2:05 PM` in `en-US`.
    #[default]
    Locale,
    /// `2:05 PM`.
    Hours12,
    /// `14:05`.
    Hours24,
}

impl HourCycle {
    fn key(self) -> &'static str {
        match self {
            HourCycle::Locale => "l",
            HourCycle::Hours12 => "12",
            HourCycle::Hours24 => "24",
        }
    }
}

/// Whether a date shows its year.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum YearDisplay {
    /// Always: `9 Nov 2026`.
    #[default]
    Always,
    /// Never: `9 Nov`.
    Never,
    /// Only when it is not the year of now: `9 Nov` this year, `9 Nov 2025`
    /// before. List columns use it.
    IfNotCurrent,
}

/// How exact a time of day is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimePrecision {
    /// Hours and minutes: `14:05`.
    #[default]
    Minutes,
    /// Hours, minutes and seconds: `14:05:09`.
    Seconds,
}

/// Which parts of the date are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DatePart {
    /// Day, month and year: `9 Nov 2026`.
    #[default]
    DayMonthYear,
    /// Day and month: `9 Nov`.
    DayMonth,
    /// Month and year: `Nov 2026`.
    MonthYear,
    /// The year: `2026`.
    Year,
    /// The month: `November`.
    Month,
    /// The day of the month: `9`.
    Day,
    /// The day of the week: `Monday`.
    Weekday,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Moment(Moment),
    Civil(Civil),
}

/// A date, with a time of day when asked for ([`Date::with_time`]).
///
/// The order of day, month and year, the month and weekday names, the
/// calendar (Gregorian, Buddhist, Persian, ...) and the clock all come from
/// the locale.
///
/// ```
/// use bingux_ui::format::{Date, Length, Moment, Zone};
/// use bingux_ui::locale::Locale;
///
/// let moment = Moment::from_unix(1_762_700_000);
/// let text = Date::new(moment)
///     .zone(Zone::Utc)
///     .locale(&Locale::parse("en-GB").unwrap())
///     .length(Length::Long)
///     .to_string();
/// assert_eq!(text, "9 November 2025");
/// ```
#[derive(Clone, Debug)]
pub struct Date {
    source: Source,
    zone: Zone,
    length: Length,
    part: DatePart,
    weekday: bool,
    year: YearDisplay,
    time: Option<TimePrecision>,
    hour_cycle: HourCycle,
    now: Option<Moment>,
    locale: Option<Locale>,
}

impl Date {
    /// The date `moment` falls on, read on the person's wall clock
    /// ([`Zone::Local`]).
    pub fn new(moment: impl Into<Moment>) -> Date {
        Date::from_source(Source::Moment(moment.into()))
    }

    /// A calendar date with no time or zone: an all day event, a birthday.
    /// `month` is 1 to 12. A date that does not exist (31 February) is
    /// written as `Invalid date`.
    pub fn civil(year: i32, month: u8, day: u8) -> Date {
        Date::from_source(Source::Civil(Civil::date(year, month, day)))
    }

    fn from_source(source: Source) -> Date {
        Date {
            source,
            zone: Zone::Local,
            length: Length::Medium,
            part: DatePart::DayMonthYear,
            weekday: false,
            year: YearDisplay::Always,
            time: None,
            hour_cycle: HourCycle::Locale,
            now: None,
            locale: None,
        }
    }

    pub fn zone(mut self, zone: Zone) -> Date {
        self.zone = zone;
        self
    }

    pub fn length(mut self, length: Length) -> Date {
        self.length = length;
        self
    }

    pub fn short(self) -> Date {
        self.length(Length::Short)
    }

    pub fn medium(self) -> Date {
        self.length(Length::Medium)
    }

    pub fn long(self) -> Date {
        self.length(Length::Long)
    }

    /// Which parts are written (the day, month and year by default).
    pub fn part(mut self, part: DatePart) -> Date {
        self.part = part;
        self
    }

    /// Adds the day of the week to a day, month and year or a day and
    /// month: `Mon, 9 Nov 2026`.
    pub fn weekday(mut self) -> Date {
        self.weekday = true;
        self
    }

    /// Whether the year is written; see [`YearDisplay`]. It applies to the
    /// day, month and year, and replaces it with the day and month when it
    /// is dropped.
    pub fn year(mut self, year: YearDisplay) -> Date {
        self.year = year;
        self
    }

    /// The moment `YearDisplay::IfNotCurrent` compares with (now by
    /// default).
    pub fn now(mut self, now: Moment) -> Date {
        self.now = Some(now);
        self
    }

    /// Adds the time of day: `9 Nov 2026, 14:05`. It goes with the day and
    /// month and the day, month and year; the other parts have no time.
    pub fn with_time(mut self, precision: TimePrecision) -> Date {
        self.time = Some(precision);
        self
    }

    pub fn hour_cycle(mut self, hour_cycle: HourCycle) -> Date {
        self.hour_cycle = hour_cycle;
        self
    }

    /// The locale to write it for, instead of the person's (`LC_TIME`).
    pub fn locale(mut self, locale: &Locale) -> Date {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        super::isolate(self.compose())
    }

    /// The text without the isolate [`format`](Self::format) adds in a
    /// right-to-left interface: for a formatter that writes it into its own.
    pub(super) fn compose(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Time));
        let civil = match self.source {
            Source::Moment(moment) => self.zone.civil(moment),
            Source::Civil(civil) => civil,
        };
        let drops_year = match self.year {
            YearDisplay::Always => false,
            YearDisplay::Never => true,
            YearDisplay::IfNotCurrent => {
                let now = self.now.unwrap_or_else(Moment::now);
                self.zone.civil(now).year == civil.year
            }
        };
        let part = match (self.part, drops_year) {
            (DatePart::DayMonthYear, true) => DatePart::DayMonth,
            (part, _) => part,
        };
        render(
            &locale,
            civil,
            part,
            self.length,
            self.weekday,
            self.time,
            self.hour_cycle,
        )
        .unwrap_or_else(|| "Invalid date".to_string())
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// A time of day.
///
/// ```
/// use bingux_ui::format::{HourCycle, Time};
/// use bingux_ui::locale::Locale;
///
/// let us = Locale::parse("en-US").unwrap();
/// assert_eq!(Time::of_day(14, 5, 0).locale(&us).to_string(), "2:05\u{202f}PM");
/// assert_eq!(Time::of_day(14, 5, 0).locale(&us).hour_cycle(HourCycle::Hours24).to_string(), "14:05");
/// ```
#[derive(Clone, Debug)]
pub struct Time {
    source: TimeSource,
    zone: Zone,
    precision: TimePrecision,
    hour_cycle: HourCycle,
    locale: Option<Locale>,
}

#[derive(Clone, Copy, Debug)]
enum TimeSource {
    Moment(Moment),
    OfDay { hour: u8, minute: u8, second: u8 },
}

impl Time {
    /// The time of day of `moment` on the person's wall clock.
    pub fn new(moment: impl Into<Moment>) -> Time {
        Time::from_source(TimeSource::Moment(moment.into()))
    }

    /// A time of day with no date or zone: an alarm, a opening hour.
    pub fn of_day(hour: u8, minute: u8, second: u8) -> Time {
        Time::from_source(TimeSource::OfDay {
            hour,
            minute,
            second,
        })
    }

    fn from_source(source: TimeSource) -> Time {
        Time {
            source,
            zone: Zone::Local,
            precision: TimePrecision::Minutes,
            hour_cycle: HourCycle::Locale,
            locale: None,
        }
    }

    pub fn zone(mut self, zone: Zone) -> Time {
        self.zone = zone;
        self
    }

    pub fn precision(mut self, precision: TimePrecision) -> Time {
        self.precision = precision;
        self
    }

    /// With seconds: `14:05:09`.
    pub fn seconds(self) -> Time {
        self.precision(TimePrecision::Seconds)
    }

    pub fn hour_cycle(mut self, hour_cycle: HourCycle) -> Time {
        self.hour_cycle = hour_cycle;
        self
    }

    pub fn locale(mut self, locale: &Locale) -> Time {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        super::isolate(self.compose())
    }

    /// The text without the isolate [`format`](Self::format) adds in a
    /// right-to-left interface: for a formatter that writes it into its own.
    pub(super) fn compose(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Time));
        let (hour, minute, second, nanosecond) = match self.source {
            TimeSource::Moment(moment) => {
                let civil = self.zone.civil(moment);
                (civil.hour, civil.minute, civil.second, civil.nanosecond)
            }
            TimeSource::OfDay {
                hour,
                minute,
                second,
            } => (hour, minute, second, 0),
        };
        let Ok(time) = IcuTime::try_new(hour, minute, second, nanosecond) else {
            return "Invalid time".to_string();
        };
        let key = format!(
            "time|{}|{:?}|{}",
            locale.tag(),
            self.precision,
            self.hour_cycle.key()
        );
        let prefs = preferences(&locale, self.hour_cycle);
        let set = match self.precision {
            TimePrecision::Minutes => T::hm(),
            TimePrecision::Seconds => T::hms(),
        };
        let formatter = cached(&key, || NoCalendarFormatter::try_new(prefs, set).ok());
        match formatter {
            Some(formatter) => formatter.format(&time).to_string(),
            None => format!("{hour:02}:{minute:02}"),
        }
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

fn preferences(locale: &Locale, hour_cycle: HourCycle) -> DateTimeFormatterPreferences {
    let mut prefs = DateTimeFormatterPreferences::from(locale.icu());
    match hour_cycle {
        HourCycle::Locale => {}
        HourCycle::Hours12 => prefs.hour_cycle = Some(IcuHourCycle::H12),
        HourCycle::Hours24 => prefs.hour_cycle = Some(IcuHourCycle::H23),
    }
    prefs
}

/// A formatter of one field set, cached for the locale and options.
macro_rules! run {
    ($locale:expr, $prefs:expr, $label:literal, $options:expr, $ty:ty, $set:expr, $input:expr) => {{
        let key = format!("date|{}|{}|{}", $locale.tag(), $label, $options);
        let formatter = cached::<DateTimeFormatter<$ty>>(&key, || {
            DateTimeFormatter::try_new($prefs, $set).ok()
        })?;
        formatter.format($input).to_string()
    }};
}

/// The field set of `$ty` for a [`Length`].
macro_rules! sized {
    ($ty:ident, $length:expr) => {
        match $length {
            Length::Short => $ty::short(),
            Length::Medium => $ty::medium(),
            Length::Long => $ty::long(),
        }
    };
}

fn render(
    locale: &Locale,
    civil: Civil,
    part: DatePart,
    length: Length,
    weekday: bool,
    time: Option<TimePrecision>,
    hour_cycle: HourCycle,
) -> Option<String> {
    let input = IcuDateTime {
        date: IcuDate::try_new_iso(civil.year, civil.month, civil.day).ok()?,
        time: IcuTime::try_new(civil.hour, civil.minute, civil.second, civil.nanosecond).ok()?,
    };
    let prefs = preferences(locale, hour_cycle);
    let options = format!("{length:?}|{}|{time:?}", hour_cycle.key());
    let input = &input;
    Some(match (part, weekday, time) {
        (DatePart::DayMonthYear, false, None) => {
            run!(
                locale,
                prefs,
                "ymd",
                options,
                YMD,
                sized!(YMD, length),
                input
            )
        }
        (DatePart::DayMonthYear, true, None) => {
            run!(
                locale,
                prefs,
                "ymde",
                options,
                YMDE,
                sized!(YMDE, length),
                input
            )
        }
        (DatePart::DayMonthYear, false, Some(precision)) => match precision {
            TimePrecision::Minutes => run!(
                locale,
                prefs,
                "ymdt",
                options,
                YMDT,
                sized!(YMD, length).with_time_hm(),
                input
            ),
            TimePrecision::Seconds => run!(
                locale,
                prefs,
                "ymdts",
                options,
                YMDT,
                sized!(YMD, length).with_time_hms(),
                input
            ),
        },
        (DatePart::DayMonthYear, true, Some(precision)) => match precision {
            TimePrecision::Minutes => run!(
                locale,
                prefs,
                "ymdet",
                options,
                YMDET,
                sized!(YMDE, length).with_time_hm(),
                input
            ),
            TimePrecision::Seconds => run!(
                locale,
                prefs,
                "ymdets",
                options,
                YMDET,
                sized!(YMDE, length).with_time_hms(),
                input
            ),
        },
        (DatePart::DayMonth, false, None) => {
            run!(locale, prefs, "md", options, MD, sized!(MD, length), input)
        }
        (DatePart::DayMonth, true, None) => {
            run!(
                locale,
                prefs,
                "mde",
                options,
                MDE,
                sized!(MDE, length),
                input
            )
        }
        (DatePart::DayMonth, false, Some(precision)) => match precision {
            TimePrecision::Minutes => run!(
                locale,
                prefs,
                "mdt",
                options,
                MDT,
                sized!(MD, length).with_time_hm(),
                input
            ),
            TimePrecision::Seconds => run!(
                locale,
                prefs,
                "mdts",
                options,
                MDT,
                sized!(MD, length).with_time_hms(),
                input
            ),
        },
        (DatePart::DayMonth, true, Some(precision)) => match precision {
            TimePrecision::Minutes => run!(
                locale,
                prefs,
                "mdet",
                options,
                MDET,
                sized!(MDE, length).with_time_hm(),
                input
            ),
            TimePrecision::Seconds => run!(
                locale,
                prefs,
                "mdets",
                options,
                MDET,
                sized!(MDE, length).with_time_hms(),
                input
            ),
        },
        (DatePart::MonthYear, ..) => {
            run!(locale, prefs, "ym", options, YM, sized!(YM, length), input)
        }
        (DatePart::Year, ..) => run!(locale, prefs, "y", options, Y, sized!(Y, length), input),
        (DatePart::Month, ..) => run!(locale, prefs, "m", options, M, sized!(M, length), input),
        (DatePart::Day, ..) => run!(locale, prefs, "d", options, D, sized!(D, length), input),
        (DatePart::Weekday, ..) => run!(locale, prefs, "e", options, E, sized!(E, length), input),
    })
}
