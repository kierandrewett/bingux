//! The words for what `bingux-calendar-core` reports as values: a reminder
//! and the things a file read forgivingly. The core never writes a sentence;
//! each kind is a message of this app's catalogue.

use crate::convert::date_of;
use bingux_calendar_core::alarm::{Notice, When};
use bingux_calendar_core::chrono::Timelike;
use bingux_calendar_core::ics::Warning;
use bingux_ui::calendar::format_clock;
use bingux_ui::prelude::tr;

/// The title and the body of the notification for `notice`, with clocks
/// written the way the person reads them.
pub fn reminder(notice: &Notice, twelve_hour: bool) -> (String, String) {
    let clock = |time: bingux_calendar_core::chrono::NaiveTime| {
        format_clock(time.hour() * 60 + time.minute(), twelve_hour)
    };
    let when = match notice.when {
        When::Today => tr!("calendar-reminder-today"),
        When::Tomorrow => tr!("calendar-reminder-tomorrow"),
        When::InDays(count) => tr!("calendar-reminder-in-days", count = count),
        When::OnDate(date) => date_of(date).long(),
        When::Now => tr!("calendar-reminder-now"),
        When::InMinutes(count) => tr!("calendar-reminder-in-minutes", count = count),
        When::StartedMinutesAgo(count) => tr!("calendar-reminder-started-ago", count = count),
        When::TodayAt(time) => tr!("calendar-reminder-today-at", time = clock(time)),
        When::DayAt(date, time) => tr!(
            "calendar-reminder-day-at",
            day = date_of(date).short(),
            time = clock(time)
        ),
    };
    let body = if notice.location.is_empty() {
        when
    } else {
        tr!(
            "calendar-reminder-with-place",
            when = when,
            place = notice.location.as_str()
        )
    };
    let title = if notice.summary.is_empty() {
        tr!("calendar-reminder-untitled")
    } else {
        notice.summary.clone()
    };
    (title, body)
}

/// The sentence for something read in a forgiving way.
pub fn warning(warning: &Warning) -> String {
    match warning {
        Warning::NoStart { summary } => tr!(
            "calendar-warning-no-start",
            named = named(summary),
            title = summary.as_str()
        ),
        Warning::UnreadableStart { summary } => tr!(
            "calendar-warning-unreadable-start",
            named = named(summary),
            title = summary.as_str()
        ),
        Warning::StrayException { summary } => tr!(
            "calendar-warning-stray-exception",
            named = named(summary),
            title = summary.as_str()
        ),
        Warning::UnknownZone { tzid } => tr!("calendar-warning-unknown-zone", zone = tzid.as_str()),
        Warning::FileUnreadable { file } => {
            tr!("calendar-warning-file-unreadable", file = file.as_str())
        }
        Warning::FileNotCalendar { file, detail } => tr!(
            "calendar-warning-file-not-calendar",
            file = file.as_str(),
            detail = detail.as_str()
        ),
        Warning::FileRepeats { file, first } => tr!(
            "calendar-warning-file-repeats",
            file = file.as_str(),
            first = first.as_str()
        ),
        Warning::InFile {
            file,
            warning: inner,
        } => tr!(
            "calendar-warning-in-file",
            file = file.as_str(),
            warning = self::warning(inner)
        ),
    }
}

/// The selector the messages above use: an event with no title is "an event".
fn named(summary: &str) -> &'static str {
    if summary.is_empty() { "no" } else { "yes" }
}
