//! What a calendar holds: events, how they repeat, and the occurrences that
//! a view draws.
//!
//! One `.ics` resource is one [`Item`]: the event as first written, the rule
//! that repeats it, and the occurrences that were changed or removed. An
//! [`Occurrence`] is what a person sees on one day.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, Utc};

use crate::zone::Zone;

/// Which occurrence of a repeating event: its start on the event's own clock.
/// All-day events use midnight.
pub type Key = NaiveDateTime;

/// A property this app does not edit, kept so saving a file does not lose it
/// (attendees, organiser, custom `X-` properties and the like).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawProp {
    pub name: String,
    pub params: Vec<(String, Vec<String>)>,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Confirmed,
    Tentative,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    #[default]
    Busy,
    Free,
}

/// When a reminder fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// This long before (negative) or after (positive) the start.
    FromStart(Duration),
    /// This long before (negative) or after (positive) the end.
    FromEnd(Duration),
    /// At one instant, for the event as first written.
    At(DateTime<Utc>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reminder {
    pub trigger: Trigger,
}

impl Reminder {
    /// A reminder `minutes` before the start.
    pub fn before(minutes: i64) -> Self {
        Self {
            trigger: Trigger::FromStart(Duration::minutes(-minutes)),
        }
    }

    /// The minutes before the start, when that is what this is.
    pub fn minutes_before(&self) -> Option<i64> {
        match self.trigger {
            Trigger::FromStart(offset) if offset <= Duration::zero() => Some(-offset.num_minutes()),
            _ => None,
        }
    }
}

/// What one occurrence says: everything an exception may change.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub summary: String,
    pub location: String,
    pub description: String,
    pub url: String,
    pub all_day: bool,
    /// The clock `start` and `end` are read on.
    pub zone: Zone,
    /// The start on the event's own clock. For an all-day event, midnight of
    /// its first day.
    pub start: NaiveDateTime,
    /// The end on the event's own clock, not included. For an all-day event,
    /// midnight of the day after its last.
    pub end: NaiveDateTime,
    pub status: Status,
    pub availability: Availability,
    pub reminders: Vec<Reminder>,
    pub extra: Vec<RawProp>,
}

impl Event {
    /// An event of one hour from `start`, on `zone`.
    pub fn new(summary: impl Into<String>, start: NaiveDateTime, zone: Zone) -> Self {
        Self {
            summary: summary.into(),
            location: String::new(),
            description: String::new(),
            url: String::new(),
            all_day: false,
            zone,
            start,
            end: start + Duration::hours(1),
            status: Status::Confirmed,
            availability: Availability::Busy,
            reminders: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// An all-day event on `first..=last`.
    pub fn all_day(summary: impl Into<String>, first: NaiveDate, last: NaiveDate) -> Self {
        let mut event = Self::new(
            summary,
            first.and_hms_opt(0, 0, 0).unwrap_or_default(),
            Zone::Floating,
        );
        event.all_day = true;
        event.end = (last.max(first) + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap_or_default();
        event
    }

    /// The length. For a timed event, the elapsed time between the instants,
    /// so an hour that crosses a clock change is still an hour.
    pub fn duration(&self) -> Duration {
        if self.all_day {
            return self.end - self.start;
        }
        self.zone.instant(self.end, Zone::Utc) - self.zone.instant(self.start, Zone::Utc)
    }

    /// The first day (all-day events) or the day the event starts on.
    pub fn first_day(&self) -> NaiveDate {
        self.start.date()
    }

    /// The last day an all-day event covers.
    pub fn last_day(&self) -> NaiveDate {
        let last = self.end - Duration::days(1);
        last.date().max(self.start.date())
    }
}

/// How a series repeats, besides the rule's own exclusions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    /// The `RRULE` value, such as `FREQ=WEEKLY;BYDAY=MO,WE`.
    pub rule: String,
    /// Occurrences removed, by key.
    pub exclusions: Vec<Key>,
    /// Extra occurrences, by start on the event's clock.
    pub additions: Vec<Key>,
}

/// An occurrence changed from what the rule says.
#[derive(Clone, Debug, PartialEq)]
pub struct Exception {
    /// The occurrence it replaces.
    pub key: Key,
    pub event: Event,
    /// It stands for every later occurrence too (`RANGE=THISANDFUTURE`).
    pub and_future: bool,
}

/// One `.ics` resource: an event with all that is said of how it repeats.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub uid: String,
    /// The id of the calendar it is in.
    pub calendar: String,
    pub event: Event,
    pub series: Option<Series>,
    pub exceptions: Vec<Exception>,
    pub sequence: u32,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
    /// The server's tag for this version, for a CalDAV calendar.
    pub etag: Option<String>,
}

impl Item {
    pub fn new(uid: impl Into<String>, calendar: impl Into<String>, event: Event) -> Self {
        Self {
            uid: uid.into(),
            calendar: calendar.into(),
            event,
            series: None,
            exceptions: Vec::new(),
            sequence: 0,
            created: None,
            modified: None,
            etag: None,
        }
    }

    pub fn is_recurring(&self) -> bool {
        self.series.is_some()
    }
}

/// An event on one occasion, with times on the clock it is shown on.
#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub calendar: String,
    /// Which occurrence of a repeating event this is.
    pub key: Option<Key>,
    pub recurring: bool,
    /// Changed from what the rule says.
    pub changed: bool,
    pub summary: String,
    pub location: String,
    pub all_day: bool,
    /// Start and end on the shown clock; the end is not included. For an
    /// all-day occurrence these are midnights, in dates of the event itself.
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    /// The event's own clock and times, when they differ from the shown ones.
    pub zone: Zone,
    pub origin_start: NaiveDateTime,
    pub origin_end: NaiveDateTime,
    pub status: Status,
    pub availability: Availability,
    pub has_reminder: bool,
}

impl Occurrence {
    /// The days it covers on the shown clock, first to last.
    pub fn days(&self) -> (NaiveDate, NaiveDate) {
        let first = self.start.date();
        let mut last = (self.end - Duration::seconds(1)).date();
        if self.end <= self.start {
            last = first;
        }
        (first, last.max(first))
    }

    /// Whether it touches `day`.
    pub fn on(&self, day: NaiveDate) -> bool {
        let (first, last) = self.days();
        first <= day && day <= last
    }

    /// Whether it is on a day other than the one it started on, or lasts a
    /// whole day or more: drawn as a bar, not a block of time.
    pub fn is_banner(&self) -> bool {
        let (first, last) = self.days();
        self.all_day || first != last && (self.end - self.start) >= Duration::hours(24)
    }

    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}
