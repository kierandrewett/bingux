//! Times told from now: "2 minutes ago", "yesterday", "in 3 weeks".

use std::fmt;

use fixed_decimal::Decimal;
use icu_experimental::relativetime::options::Numeric;
use icu_experimental::relativetime::{RelativeTimeFormatter, RelativeTimeFormatterOptions};

use super::Width;
use super::cache::cached;
use super::datetime::{Date, HourCycle, Time, YearDisplay};
use super::moment::{Moment, Zone};
use crate::locale::{Category, Locale};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// The unit and signed count that tell `then` from `now`: the largest unit
/// with a whole count. Counts are cut, not rounded (59 minutes and 59
/// seconds is 59 minutes). From a day on they are calendar days and months
/// on the wall clock of `zone`: 25 hours back is "yesterday" or "2 days ago"
/// depending on what time it is where the person is.
fn choose(then: Moment, now: Moment, zone: &Zone) -> (Unit, i64) {
    let delta = then.unix_seconds().saturating_sub(now.unix_seconds());
    let size = delta.unsigned_abs();
    if size < 5 {
        return (Unit::Second, 0);
    }
    if size < 60 {
        return (Unit::Second, delta);
    }
    if size < 3600 {
        return (Unit::Minute, delta / 60);
    }
    if size < 86_400 {
        return (Unit::Hour, delta / 3600);
    }
    let (then_civil, now_civil) = (zone.civil(then), zone.civil(now));
    let days = then_civil.day_number() - now_civil.day_number();
    if days == 0 {
        // A day with a clock change can hold more than 24 hours.
        return (Unit::Hour, delta / 3600);
    }
    if days.abs() < 7 {
        return (Unit::Day, days);
    }
    if days.abs() < 30 {
        return (Unit::Week, days / 7);
    }
    let mut months = (i64::from(then_civil.year) * 12 + i64::from(then_civil.month))
        - (i64::from(now_civil.year) * 12 + i64::from(now_civil.month));
    // A month is whole when the day of the month has come round again.
    if months > 0 && then_civil.day < now_civil.day {
        months -= 1;
    } else if months < 0 && then_civil.day > now_civil.day {
        months += 1;
    }
    if months == 0 {
        months = days.signum();
    }
    if months.abs() < 12 {
        (Unit::Month, months)
    } else {
        (Unit::Year, months / 12)
    }
}

/// A time told from now.
///
/// ```
/// use bingux_ui::format::{Moment, Relative};
/// use bingux_ui::locale::Locale;
///
/// let now = Moment::from_unix(1_762_700_000);
/// let en = Locale::parse("en-GB").unwrap();
/// let ago = Relative::new(now.plus_seconds(-150)).now(now).locale(&en);
/// assert_eq!(ago.to_string(), "2 minutes ago");
/// ```
#[derive(Clone, Debug)]
pub struct Relative {
    then: Moment,
    now: Option<Moment>,
    zone: Zone,
    width: Width,
    numeric: Numeric,
    locale: Option<Locale>,
}

impl Relative {
    /// `then`, told from now: "5 minutes ago", "in 2 hours".
    pub fn new(then: impl Into<Moment>) -> Relative {
        Relative {
            then: then.into(),
            now: None,
            zone: Zone::Local,
            width: Width::Long,
            numeric: Numeric::Auto,
            locale: None,
        }
    }

    /// The moment to tell it from (the time now by default). Pass the same
    /// one for every row of a list so they agree.
    pub fn now(mut self, now: Moment) -> Relative {
        self.now = Some(now);
        self
    }

    /// The wall clock that decides what a day is.
    pub fn zone(mut self, zone: Zone) -> Relative {
        self.zone = zone;
        self
    }

    /// `Long` is "2 minutes ago", `Short` "2 min. ago", `Narrow` "2m ago".
    pub fn width(mut self, width: Width) -> Relative {
        self.width = width;
        self
    }

    /// Numbers always: "1 day ago", never "yesterday".
    pub fn numeric(mut self) -> Relative {
        self.numeric = Numeric::Always;
        self
    }

    /// The locale to write it for, instead of the person's (`LC_TIME`).
    pub fn locale(mut self, locale: &Locale) -> Relative {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Time));
        let now = self.now.unwrap_or_else(Moment::now);
        let (unit, count) = choose(self.then, now, &self.zone);
        told(&locale, self.width, unit, self.numeric, count)
    }
}

impl fmt::Display for Relative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

fn told(locale: &Locale, width: Width, unit: Unit, numeric: Numeric, count: i64) -> String {
    let key = format!("relative|{}|{width:?}|{unit:?}|{numeric:?}", locale.tag());
    let formatter = cached(&key, || {
        let prefs = locale.icu().into();
        let options = RelativeTimeFormatterOptions::default().with_numeric(numeric);
        macro_rules! make {
            ($long:ident, $short:ident, $narrow:ident) => {
                match width {
                    Width::Long => RelativeTimeFormatter::$long(prefs, options),
                    Width::Short => RelativeTimeFormatter::$short(prefs, options),
                    Width::Narrow => RelativeTimeFormatter::$narrow(prefs, options),
                }
            };
        }
        match unit {
            Unit::Second => make!(
                try_new_long_second,
                try_new_short_second,
                try_new_narrow_second
            ),
            Unit::Minute => make!(
                try_new_long_minute,
                try_new_short_minute,
                try_new_narrow_minute
            ),
            Unit::Hour => make!(try_new_long_hour, try_new_short_hour, try_new_narrow_hour),
            Unit::Day => make!(try_new_long_day, try_new_short_day, try_new_narrow_day),
            Unit::Week => make!(try_new_long_week, try_new_short_week, try_new_narrow_week),
            Unit::Month => make!(
                try_new_long_month,
                try_new_short_month,
                try_new_narrow_month
            ),
            Unit::Year => make!(try_new_long_year, try_new_short_year, try_new_narrow_year),
        }
        .ok()
    });
    match formatter {
        Some(formatter) => formatter.format(Decimal::from(count)).to_string(),
        None => format!("{count}"),
    }
}

/// A time as a list column shows it: the time of day for today, "yesterday",
/// then the date without the current year.
///
/// | Age                      | `en-GB`                         |
/// | ------------------------ | ------------------------------- |
/// | under a minute           | `now`                           |
/// | earlier today            | `14:03`                         |
/// | the day before           | `Yesterday`                     |
/// | earlier this year        | `12 Oct`                        |
/// | another year             | `12 Oct 2024`                   |
///
/// The words are the locale's own and come out capitalised, as a cell of a
/// list reads best.
#[derive(Clone, Debug)]
pub struct Recent {
    moment: Moment,
    now: Option<Moment>,
    zone: Zone,
    hour_cycle: HourCycle,
    locale: Option<Locale>,
}

impl Recent {
    pub fn new(moment: impl Into<Moment>) -> Recent {
        Recent {
            moment: moment.into(),
            now: None,
            zone: Zone::Local,
            hour_cycle: HourCycle::Locale,
            locale: None,
        }
    }

    /// The moment to tell it from (the time now by default).
    pub fn now(mut self, now: Moment) -> Recent {
        self.now = Some(now);
        self
    }

    pub fn zone(mut self, zone: Zone) -> Recent {
        self.zone = zone;
        self
    }

    pub fn hour_cycle(mut self, hour_cycle: HourCycle) -> Recent {
        self.hour_cycle = hour_cycle;
        self
    }

    pub fn locale(mut self, locale: &Locale) -> Recent {
        self.locale = Some(locale.clone());
        self
    }

    /// The text.
    pub fn format(&self) -> String {
        let locale = self
            .locale
            .clone()
            .unwrap_or_else(|| Locale::for_category(Category::Time));
        let now = self.now.unwrap_or_else(Moment::now);
        let delta = self
            .moment
            .unix_seconds()
            .saturating_sub(now.unix_seconds());
        let days = self.zone.civil(self.moment).day_number() - self.zone.civil(now).day_number();
        if delta.abs() < 60 {
            return capitalised(&told(&locale, Width::Long, Unit::Second, Numeric::Auto, 0));
        }
        match days {
            0 => Time::new(self.moment)
                .zone(self.zone.clone())
                .hour_cycle(self.hour_cycle)
                .locale(&locale)
                .format(),
            1 | -1 => capitalised(&told(&locale, Width::Long, Unit::Day, Numeric::Auto, days)),
            _ => Date::new(self.moment)
                .zone(self.zone.clone())
                .year(YearDisplay::IfNotCurrent)
                .now(now)
                .locale(&locale)
                .format(),
        }
    }
}

impl fmt::Display for Recent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

/// `text` with its first letter in capitals (`yesterday` to `Yesterday`);
/// scripts with no case are left as they are.
pub(crate) fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_762_700_000; // 2025-11-09T14:53:20Z

    fn told_in(seconds_ago: i64) -> (Unit, i64) {
        choose(
            Moment::from_unix(NOW - seconds_ago),
            Moment::from_unix(NOW),
            &Zone::Utc,
        )
    }

    #[test]
    fn units_are_the_largest_whole_one() {
        assert_eq!(told_in(0), (Unit::Second, 0));
        assert_eq!(told_in(4), (Unit::Second, 0));
        assert_eq!(told_in(-4), (Unit::Second, 0));
        assert_eq!(told_in(5), (Unit::Second, -5));
        assert_eq!(told_in(59), (Unit::Second, -59));
        assert_eq!(told_in(60), (Unit::Minute, -1));
        assert_eq!(told_in(3599), (Unit::Minute, -59));
        assert_eq!(told_in(3600), (Unit::Hour, -1));
        assert_eq!(told_in(86_399), (Unit::Hour, -23));
        assert_eq!(told_in(-120), (Unit::Minute, 2));
    }

    #[test]
    fn days_are_calendar_days() {
        // 14:53 UTC on the 9th: 15 hours back is 23:53 on the 8th.
        assert_eq!(told_in(15 * 3600), (Unit::Hour, -15));
        // 25 hours back is the 8th at 13:53: one calendar day.
        assert_eq!(told_in(25 * 3600), (Unit::Day, -1));
        // 2 days and a minute back is the 7th.
        assert_eq!(told_in(2 * 86_400 + 60), (Unit::Day, -2));
        assert_eq!(told_in(-30 * 3600), (Unit::Day, 1));
    }

    #[test]
    fn weeks_months_and_years() {
        let day = 86_400;
        assert_eq!(told_in(7 * day), (Unit::Week, -1));
        assert_eq!(told_in(20 * day), (Unit::Week, -2));
        assert_eq!(told_in(29 * day), (Unit::Week, -4));
        // Not yet a whole month back: 30 days from 9 Nov is 10 Oct.
        assert_eq!(told_in(30 * day), (Unit::Month, -1));
        assert_eq!(told_in(200 * day), (Unit::Month, -6));
        assert_eq!(told_in(364 * day), (Unit::Month, -11));
        assert_eq!(told_in(366 * day), (Unit::Year, -1));
        assert_eq!(told_in(800 * day), (Unit::Year, -2));
        assert_eq!(told_in(-45 * day), (Unit::Month, 1));
    }

    #[test]
    fn a_wall_clock_decides_the_day() {
        // Now is 23:10 UTC on the 8th, which is 00:10 on the 9th in UTC+1.
        // 25 hours back is the 7th in UTC (one day) and the 7th at 23:10 in
        // UTC+1 (two days from the 9th).
        let now = Moment::from_unix(1_762_643_400);
        let then = Moment::from_unix(1_762_643_400 - 25 * 3600);
        assert_eq!(choose(then, now, &Zone::Utc), (Unit::Day, -1));
        assert_eq!(choose(then, now, &Zone::Offset(3600)), (Unit::Day, -2));
    }

    #[test]
    fn capitals_follow_unicode() {
        assert_eq!(capitalised("yesterday"), "Yesterday");
        assert_eq!(capitalised("ñandú"), "Ñandú");
        assert_eq!(capitalised("昨日"), "昨日");
        assert_eq!(capitalised(""), "");
    }
}
