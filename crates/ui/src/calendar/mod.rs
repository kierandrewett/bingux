//! Calendar views: a month of days, a month the size of a window, the time
//! grid of a day or a week, and pickers for a date and a time of day.
//!
//! These are drawing and gestures, not a calendar: they know civil dates and
//! minutes after midnight (`date`), draw the events they are given, and report
//! what a person did. What an event is, where it is kept and how it repeats
//! belongs to the app. Settings' Date and time uses the pickers; the shell's
//! calendar popout can use `MonthGrid`.

pub mod date;
pub mod layout;
mod month_grid;
mod month_view;
pub mod palette;
mod pickers;
mod time_grid;

use std::hash::{Hash, Hasher};

use freya::prelude::DiffKey;

/// The diff key of a list item: what makes two renders of it the same item.
pub(crate) fn key_of(value: impl Hash) -> DiffKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    DiffKey::U64(hasher.finish())
}

pub use date::{Date, Weekday, format_clock, format_hour, parse_clock};
pub use month_grid::{MonthGrid, month_grid};
pub use month_view::{MonthItem, MonthView, month_view};
pub use pickers::{DatePicker, TimePicker, date_picker, format_length, time_picker};
pub use time_grid::{Banner, Block, BlockEdit, EditKind, SlotRange, TimeGrid, time_grid};
