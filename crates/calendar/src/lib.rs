//! The Bingux Calendar model: events, iCalendar files, recurrence, the local
//! store, quick add, reminders and CalDAV. No UI; the app is `apps/calendar`.
//!
//! The pieces, from the file up:
//!
//! - [`zone`]: what clock a time is on, and which instant a wall-clock time is.
//! - [`event`]: events, how they repeat, and the occurrences a view draws.
//! - [`ics`]: reading and writing iCalendar.
//! - [`recur`]: which occurrences fall in a window, and edits and deletes
//!   of one, the following or all of them.
//! - [`store`]: calendars as folders of `.ics` files, and the in-memory
//!   [`store::Snapshot`] the views read.
//! - [`quickadd`]: "Lunch with Sam tomorrow 1pm" as an event.
//! - [`alarm`]: reminders, shown once, through the notification service.
//! - [`prefs`]: preferences and the locale's week and clock.

pub mod alarm;
pub mod event;
pub mod ics;
pub mod prefs;
pub mod quickadd;
pub mod recur;
pub mod store;
pub mod zone;

pub use chrono;
pub use event::{
    Availability, Event, Exception, Item, Key, Occurrence, RawProp, Reminder, Series, Status,
    Trigger,
};
pub use prefs::{HourFormat, Locale, Prefs, ViewKind};
pub use recur::{Change, Repeat, RepeatEnd, Scope};
pub use store::{Calendar, Snapshot, Store};
pub use zone::Zone;

#[cfg(test)]
mod recurrence_tests;
