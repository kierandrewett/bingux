//! Between the model's dates (chrono) and the views' (`bingux_ui::calendar`),
//! and the colours a calendar draws its events in.

use bingux_calendar_core::Calendar;
use bingux_calendar_core::chrono::{self, Datelike, NaiveDate, NaiveDateTime, Timelike};
use bingux_ui::calendar::Date;
use bingux_ui::calendar::Weekday;
use bingux_ui::calendar::palette::calendar_colour;
use bingux_ui::prelude::Tokens;
use freya::prelude::Color;

/// The view's date for a model date.
pub fn date_of(date: NaiveDate) -> Date {
    Date::new(date.year(), date.month() as u8, date.day() as u8).unwrap_or(Date {
        year: 1970,
        month: 1,
        day: 1,
    })
}

/// The model's date for a view date.
pub fn naive(date: Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year, u32::from(date.month), u32::from(date.day))
        .unwrap_or_default()
}

/// Midnight at the start of `date`.
pub fn midnight(date: Date) -> NaiveDateTime {
    naive(date).and_hms_opt(0, 0, 0).unwrap_or_default()
}

/// Minutes after midnight of a wall-clock time.
pub fn minutes_of(time: NaiveDateTime) -> u32 {
    time.hour() * 60 + time.minute()
}

/// The wall-clock time `minutes` after midnight of `date`; 1440 is the next
/// midnight.
pub fn at_minutes(date: Date, minutes: u32) -> NaiveDateTime {
    midnight(date) + chrono::Duration::minutes(i64::from(minutes))
}

pub fn weekday_of(day: chrono::Weekday) -> Weekday {
    Weekday::from_index(i64::from(day.num_days_from_monday()))
}

/// The colour a calendar draws its events in. `index` is its place in the
/// list, which picks an accent for a calendar that has no colour.
pub fn colour_of(calendar: Option<&Calendar>, index: usize, tokens: &Tokens) -> Color {
    calendar_colour(
        calendar.and_then(|calendar| calendar.colour.as_deref()),
        index,
        tokens.scheme.is_dark(),
    )
}
