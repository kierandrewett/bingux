//! Civil dates and clock times, with no time zone: what a calendar grid draws.
//!
//! `Date` is a day of the proleptic Gregorian calendar; `minutes` in the rest
//! of the module are minutes after midnight. An app with a time zone database
//! converts to and from its own types at the edge; nothing here knows a zone.

use std::fmt;

/// A day of the week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// 0 for Monday to 6 for Sunday.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// The weekday for an index; the index wraps.
    pub fn from_index(index: i64) -> Weekday {
        match index.rem_euclid(7) {
            0 => Weekday::Mon,
            1 => Weekday::Tue,
            2 => Weekday::Wed,
            3 => Weekday::Thu,
            4 => Weekday::Fri,
            5 => Weekday::Sat,
            _ => Weekday::Sun,
        }
    }

    /// The weekday `days` after this one.
    pub fn plus(self, days: i64) -> Weekday {
        Weekday::from_index(i64::from(self.index()) + days)
    }

    pub fn name(self) -> &'static str {
        match self {
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
        }
    }

    /// "Mon".
    pub fn short(self) -> &'static str {
        &self.name()[..3]
    }

    /// "M": one letter, for a narrow column.
    pub fn narrow(self) -> &'static str {
        &self.name()[..1]
    }

    /// Whether this is a Saturday or a Sunday.
    pub fn is_weekend(self) -> bool {
        matches!(self, Weekday::Sat | Weekday::Sun)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "March" for month 3. Months outside 1 to 12 wrap.
pub fn month_name(month: u8) -> &'static str {
    MONTHS[usize::from(month.max(1) - 1) % 12]
}

/// "Mar" for month 3.
pub fn month_short(month: u8) -> &'static str {
    &month_name(month)[..3]
}

/// A day of the Gregorian calendar.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Whether `year` has a 29th of February.
pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The days in a month of a year.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
    }
}

impl Date {
    /// `None` when the day does not exist.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Date> {
        if (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month) {
            Some(Date { year, month, day })
        } else {
            None
        }
    }

    /// The first day of the month.
    pub fn first_of_month(self) -> Date {
        Date { day: 1, ..self }
    }

    /// The last day of the month.
    pub fn last_of_month(self) -> Date {
        Date {
            day: days_in_month(self.year, self.month),
            ..self
        }
    }

    pub fn days_in_month(self) -> u8 {
        days_in_month(self.year, self.month)
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    pub fn days(self) -> i64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01.
    pub fn from_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        Date {
            year: (y + i64::from(month <= 2)) as i32,
            month,
            day,
        }
    }

    pub fn add_days(self, days: i64) -> Date {
        Date::from_days(self.days() + days)
    }

    /// `months` later, or earlier; a day that does not exist in the target
    /// month becomes its last day.
    pub fn add_months(self, months: i32) -> Date {
        let total = self.year * 12 + i32::from(self.month) - 1 + months;
        let year = total.div_euclid(12);
        let month = (total.rem_euclid(12) + 1) as u8;
        Date {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }

    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday.
        Weekday::from_index(self.days() + 3)
    }

    /// The first day of the week this day is in.
    pub fn week_start(self, first: Weekday) -> Date {
        let back = (i64::from(self.weekday().index()) - i64::from(first.index())).rem_euclid(7);
        self.add_days(-back)
    }

    /// The ISO 8601 week number, 1 to 53.
    pub fn iso_week(self) -> u32 {
        // The Thursday of this week decides the year the week belongs to.
        let thursday = self.add_days(3 - i64::from(self.weekday().index()));
        let jan1 = Date {
            year: thursday.year,
            month: 1,
            day: 1,
        };
        ((thursday.days() - jan1.days()) / 7 + 1) as u32
    }

    /// "12 March 2025".
    pub fn long(self) -> String {
        format!("{} {} {}", self.day, month_name(self.month), self.year)
    }

    /// "Wed 12 Mar".
    pub fn short(self) -> String {
        format!(
            "{} {} {}",
            self.weekday().short(),
            self.day,
            month_short(self.month)
        )
    }

    /// "March 2025".
    pub fn month_year(self) -> String {
        format!("{} {}", month_name(self.month), self.year)
    }

    /// "2025-03-12", the form `parse` reads.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Reads `2025-03-12`, `12/03/2025` (day first unless `month_first`),
    /// `12 Mar 2025`, `12 March` and `March 12 2025`. A date with no year is
    /// in `reference`'s year.
    pub fn parse(text: &str, reference: Date, month_first: bool) -> Option<Date> {
        let text = text.trim().to_lowercase();
        let words: Vec<&str> = text
            .split(|c: char| c.is_whitespace() || c == ',' || c == '/' || c == '-' || c == '.')
            .filter(|word| !word.is_empty())
            .collect();
        let number = |word: &str| -> Option<i32> {
            word.trim_end_matches(|c: char| c.is_ascii_alphabetic())
                .parse()
                .ok()
        };
        let month_of = |word: &str| -> Option<u8> {
            (1..=12u8).find(|month| {
                let name = month_name(*month).to_lowercase();
                word.len() >= 3 && name.starts_with(word)
            })
        };
        let year_of = |value: i32| if value < 100 { 2000 + value } else { value };
        match words.as_slice() {
            [a, b, c] => {
                if let (Some(y), Some(m), Some(d)) = (number(a), number(b), number(c)) {
                    if a.len() == 4 {
                        return Date::new(y, m as u8, d as u8);
                    }
                    let (day, month) = if month_first { (m, y) } else { (y, m) };
                    return Date::new(year_of(d), month as u8, day as u8);
                }
                if let (Some(d), Some(m), Some(y)) = (number(a), month_of(b), number(c)) {
                    return Date::new(year_of(y), m, d as u8);
                }
                if let (Some(m), Some(d), Some(y)) = (month_of(a), number(b), number(c)) {
                    return Date::new(year_of(y), m, d as u8);
                }
                None
            }
            [a, b] => {
                if let (Some(d), Some(m)) = (number(a), month_of(b)) {
                    return Date::new(reference.year, m, d as u8);
                }
                if let (Some(m), Some(d)) = (month_of(a), number(b)) {
                    return Date::new(reference.year, m, d as u8);
                }
                if let (Some(x), Some(y)) = (number(a), number(b)) {
                    let (day, month) = if month_first { (y, x) } else { (x, y) };
                    return Date::new(reference.year, month as u8, day as u8);
                }
                None
            }
            _ => None,
        }
    }
}

/// How the time of day is written.
pub fn format_clock(minutes: u32, twelve_hour: bool) -> String {
    let (hour, minute) = (minutes / 60 % 24, minutes % 60);
    if twelve_hour {
        let suffix = if hour < 12 { "AM" } else { "PM" };
        let shown = match hour % 12 {
            0 => 12,
            other => other,
        };
        format!("{shown}:{minute:02} {suffix}")
    } else {
        format!("{hour:02}:{minute:02}")
    }
}

/// The hour label of a time grid: "9 AM", "13:00", "Noon" is not used.
pub fn format_hour(hour: u32, twelve_hour: bool) -> String {
    if twelve_hour {
        let suffix = if hour % 24 < 12 { "AM" } else { "PM" };
        let shown = match hour % 12 {
            0 => 12,
            other => other,
        };
        format!("{shown} {suffix}")
    } else {
        format!("{:02}:00", hour % 24)
    }
}

/// A time of day as a person types it: `9`, `9:30`, `930`, `9.30`, `9am`,
/// `9:30 pm`, `1530`, `noon`, `midnight`. A bare hour from 1 to 7 is the
/// afternoon only when `prefer_pm` is set.
pub fn parse_clock(text: &str, prefer_pm: bool) -> Option<u32> {
    let text = text.trim().to_lowercase().replace(' ', "");
    match text.as_str() {
        "noon" | "midday" => return Some(12 * 60),
        "midnight" => return Some(0),
        _ => {}
    }
    let (body, meridiem) = if let Some(body) = text.strip_suffix("pm") {
        (body, Some(true))
    } else if let Some(body) = text.strip_suffix("am") {
        (body, Some(false))
    } else if let Some(body) = text.strip_suffix('p') {
        (body, Some(true))
    } else if let Some(body) = text.strip_suffix('a') {
        (body, Some(false))
    } else {
        (text.as_str(), None)
    };
    let (hours, minutes): (u32, u32) = if let Some((h, m)) = body.split_once([':', '.']) {
        (h.parse().ok()?, m.parse().ok()?)
    } else if body.len() > 2 && body.len() <= 4 && body.chars().all(|c| c.is_ascii_digit()) {
        let split = body.len() - 2;
        (body[..split].parse().ok()?, body[split..].parse().ok()?)
    } else {
        (body.parse().ok()?, 0)
    };
    if minutes > 59 {
        return None;
    }
    let hours = match meridiem {
        Some(pm) => {
            if !(1..=12).contains(&hours) {
                return None;
            }
            hours % 12 + if pm { 12 } else { 0 }
        }
        None => {
            if hours > 24 || (hours == 24 && minutes > 0) {
                return None;
            }
            if prefer_pm && (1..=7).contains(&hours) && !body.contains([':', '.']) {
                hours + 12
            } else {
                hours % 24
            }
        }
    };
    Some(hours * 60 + minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn days_since_the_epoch_round_trip() {
        assert_eq!(d(1970, 1, 1).days(), 0);
        assert_eq!(d(2000, 3, 1).days(), 11_017);
        for days in [-800_000i64, -1, 0, 19_000, 60_000, 400_000] {
            assert_eq!(Date::from_days(days).days(), days);
        }
    }

    #[test]
    fn weekdays() {
        assert_eq!(d(1970, 1, 1).weekday(), Weekday::Thu);
        assert_eq!(d(2025, 3, 12).weekday(), Weekday::Wed);
        assert_eq!(d(2024, 2, 29).weekday(), Weekday::Thu);
    }

    #[test]
    fn months_clamp_the_day() {
        assert_eq!(d(2025, 1, 31).add_months(1), d(2025, 2, 28));
        assert_eq!(d(2024, 1, 31).add_months(1), d(2024, 2, 29));
        assert_eq!(d(2025, 3, 31).add_months(-1), d(2025, 2, 28));
        assert_eq!(d(2025, 12, 15).add_months(1), d(2026, 1, 15));
        assert_eq!(d(2025, 1, 15).add_months(-1), d(2024, 12, 15));
    }

    #[test]
    fn week_starts() {
        // Wednesday 12 March 2025.
        let day = d(2025, 3, 12);
        assert_eq!(day.week_start(Weekday::Mon), d(2025, 3, 10));
        assert_eq!(day.week_start(Weekday::Sun), d(2025, 3, 9));
        assert_eq!(day.week_start(Weekday::Sat), d(2025, 3, 8));
        assert_eq!(d(2025, 3, 10).week_start(Weekday::Mon), d(2025, 3, 10));
    }

    #[test]
    fn iso_week_numbers() {
        assert_eq!(d(2025, 1, 1).iso_week(), 1);
        assert_eq!(d(2024, 12, 30).iso_week(), 1);
        assert_eq!(d(2021, 1, 3).iso_week(), 53);
        assert_eq!(d(2025, 3, 12).iso_week(), 11);
        assert_eq!(d(2026, 12, 31).iso_week(), 53);
    }

    #[test]
    fn dates_are_read_as_people_write_them() {
        let now = d(2025, 3, 12);
        assert_eq!(Date::parse("2025-04-03", now, false), Some(d(2025, 4, 3)));
        assert_eq!(Date::parse("3/4/2025", now, false), Some(d(2025, 4, 3)));
        assert_eq!(Date::parse("3/4/2025", now, true), Some(d(2025, 3, 4)));
        assert_eq!(Date::parse("3 apr 2025", now, false), Some(d(2025, 4, 3)));
        assert_eq!(Date::parse("April 3", now, false), Some(d(2025, 4, 3)));
        assert_eq!(Date::parse("31/2", now, false), None);
        assert_eq!(Date::parse("soon", now, false), None);
    }

    #[test]
    fn clocks_are_written_and_read() {
        assert_eq!(format_clock(13 * 60 + 5, false), "13:05");
        assert_eq!(format_clock(13 * 60 + 5, true), "1:05 PM");
        assert_eq!(format_clock(0, true), "12:00 AM");
        assert_eq!(format_hour(15, true), "3 PM");
        assert_eq!(parse_clock("9", false), Some(9 * 60));
        assert_eq!(parse_clock("9:30pm", false), Some(21 * 60 + 30));
        assert_eq!(parse_clock("1530", false), Some(15 * 60 + 30));
        assert_eq!(parse_clock("12am", false), Some(0));
        assert_eq!(parse_clock("12pm", false), Some(12 * 60));
        assert_eq!(parse_clock("3", true), Some(15 * 60));
        assert_eq!(parse_clock("noon", false), Some(12 * 60));
        assert_eq!(parse_clock("25:00", false), None);
        assert_eq!(parse_clock("9:75", false), None);
    }
}
