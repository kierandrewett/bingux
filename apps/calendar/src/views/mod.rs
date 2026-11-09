//! The views: week and day (a time grid), month, year and the agenda list.
//! Each reads the model, draws with `bingux_ui::calendar`, and calls the
//! editor when a person asks to make or open an event.

pub mod agenda;
pub mod grid;
pub mod month;
pub mod year;

use bingux_calendar_core::{Occurrence, Status};
use bingux_ui::prelude::*;
use freya::prelude::Color;

use crate::convert;
use crate::editor::{self, Seed};
use crate::model::{EventRef, Model};

/// The colour of the calendar an occurrence is in.
pub fn colour_of(model: &Model, calendar: &str, tokens: &Tokens) -> Color {
    let data = model.data();
    let index = data
        .calendars
        .iter()
        .position(|c| c.id == calendar)
        .unwrap_or(0);
    convert::colour_of(data.calendar(calendar), index, tokens)
}

/// Whether events of a calendar can be changed.
pub fn writable(model: &Model, calendar: &str) -> bool {
    model
        .data()
        .calendar(calendar)
        .is_none_or(|calendar| !calendar.read_only)
}

/// Events that are not cancelled, which is what a view shows.
pub fn shown(occurrences: Vec<Occurrence>) -> Vec<Occurrence> {
    occurrences
        .into_iter()
        .filter(|occurrence| occurrence.status != Status::Cancelled)
        .collect()
}

/// What an empty title is called.
pub fn title_of(occurrence: &Occurrence) -> String {
    if occurrence.summary.trim().is_empty() {
        "New event".to_string()
    } else {
        occurrence.summary.clone()
    }
}

/// Opens the editor on the event a grid or a list named.
pub fn open(model: Model, window: Window, overlays: Overlays, id: &str) {
    let reference = EventRef::parse(id);
    editor::open(model, window, overlays, Seed::Existing(reference));
}
