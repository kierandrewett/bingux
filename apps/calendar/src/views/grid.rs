//! Week and day: a time grid of one, three or seven days.

use bingux_calendar_core::recur::Scope;
use bingux_calendar_core::{Occurrence, Status, Zone, recur};
use bingux_ui::calendar::layout::Span;
use bingux_ui::calendar::{Banner, Block, BlockEdit, Date, EditKind, SlotRange, time_grid};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::{colour_of, open, shown, title_of, writable};
use crate::convert;
use crate::editor::{self, Seed};
use crate::model::{EventRef, Model, time_text};

/// How many days the grid shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Span3 {
    Day,
    ThreeDays,
    Week,
}

impl Span3 {
    pub fn count(self) -> i64 {
        match self {
            Span3::Day => 1,
            Span3::ThreeDays => 3,
            Span3::Week => 7,
        }
    }
}

/// The first day a grid of `span` shows when `anchor` is on show.
pub fn first_day(model: &Model, anchor: Date, span: Span3) -> Date {
    match span {
        Span3::Day | Span3::ThreeDays => anchor,
        Span3::Week => anchor.week_start(model.first_weekday()),
    }
}

/// The banners and blocks of `count` days from `first`.
pub fn layout(
    model: &Model,
    tokens: &Tokens,
    first: Date,
    count: usize,
) -> (Vec<Banner>, Vec<Block>) {
    let last = first.add_days(count as i64);
    let twelve = model.twelve_hour();
    let view = model.zone();
    let mut banners = Vec::new();
    let mut blocks = Vec::new();
    for occurrence in shown(model.occurrences(first, last)) {
        let id = EventRef::of(&occurrence).id();
        let colour = colour_of(model, &occurrence.calendar, tokens);
        let editable = writable(model, &occurrence.calendar);
        let title = title_of(&occurrence);
        if occurrence.is_banner() {
            let (from, to) = occurrence.days();
            let (from, to) = (convert::date_of(from), convert::date_of(to));
            let last_shown = last.add_days(-1);
            banners.push(Banner {
                id,
                first: (from.max(first).days() - first.days()) as usize,
                last: (to.min(last_shown).days() - first.days()) as usize,
                title,
                colour,
                before: from < first,
                after: to > last_shown,
            });
            continue;
        }
        let detail = detail_of(&occurrence, twelve, view);
        for column in 0..count {
            let day = first.add_days(column as i64);
            let day_start = convert::midnight(day);
            let next = convert::midnight(day.add_days(1));
            let zero = occurrence.end <= occurrence.start;
            if occurrence.start >= next || (occurrence.end <= day_start && !zero) {
                continue;
            }
            if zero && occurrence.start < day_start {
                continue;
            }
            let start = occurrence.start.max(day_start);
            let end = occurrence.end.min(next);
            let from = convert::minutes_of(start);
            let to = if end >= next {
                24 * 60
            } else {
                convert::minutes_of(end)
            };
            blocks.push(Block {
                id: id.clone(),
                column,
                span: Span {
                    start: from,
                    end: to.max(from + 15).min(24 * 60).max(from + 1),
                },
                title: title.clone(),
                detail: detail.clone(),
                colour,
                tentative: occurrence.status == Status::Tentative,
                editable: editable && start == occurrence.start && end == occurrence.end,
                clipped_start: start > occurrence.start,
                clipped_end: end < occurrence.end,
            });
        }
    }
    (banners, blocks)
}

/// "9:00 to 10:00 · Nando's".
fn detail_of(occurrence: &Occurrence, twelve: bool, view: Zone) -> String {
    let mut text = time_text(
        occurrence,
        convert::date_of(occurrence.start.date()),
        twelve,
    );
    if !occurrence.location.is_empty() {
        text.push_str(" · ");
        text.push_str(&occurrence.location);
    }
    if !occurrence.all_day && occurrence.zone != view && occurrence.zone != Zone::Floating {
        text.push_str(&format!(" · {}", occurrence.zone.label()));
    }
    text
}

/// A week, a few days or one day.
#[derive(PartialEq)]
pub struct GridView {
    pub model: Model,
    pub span: Span3,
}

impl Component for GridView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let clock = *model.clock.read();
        let anchor = *model.anchor.read();
        let first = first_day(&model, anchor, self.span);
        let count = self.span.count() as usize;
        let days = (0..count as i64)
            .map(|offset| first.add_days(offset))
            .collect::<Vec<_>>();
        let (banners, blocks) = layout(&model, &tokens, first, count);
        let twelve = model.twelve_hour();
        let prefs = model.prefs.read().clone();
        let nonce = *model.scroll_nonce.read();
        let showing_today = days.contains(&clock.today);
        // Today scrolls to the time now; another day to the start of the
        // working day, an hour early so its first hour has room above it.
        let target = if showing_today {
            clock.minutes.saturating_sub(120)
        } else {
            (prefs.working_hours.0 * 60).saturating_sub(60)
        };
        let selected = model.selected.read().clone();
        let default_length = prefs.default_length.max(15) as u32;
        let first_for_create = first;

        let on_open = move |id: String| open(model, window, overlays, &id);
        let on_create = move |range: SlotRange| {
            let day = first_for_create.add_days(range.column as i64);
            editor::open(
                model,
                window,
                overlays,
                Seed::new_at(
                    convert::at_minutes(day, range.span.start),
                    convert::at_minutes(day, range.span.end),
                ),
            );
        };
        let on_edit = move |edit: BlockEdit| {
            let day = first_for_create.add_days(edit.column as i64);
            moved(model, window, overlays, &edit, day);
        };
        let on_day = move |day: Date| {
            model.go(day);
            model.show(bingux_calendar_core::ViewKind::Day);
        };
        let on_select = move |id: String| {
            let mut selected = model.selected;
            selected.set(Some(id));
        };

        let mut grid = time_grid(days, clock.today)
            .blocks(blocks)
            .banners(banners)
            .selected(selected)
            .twelve_hour(twelve)
            .default_length(default_length)
            .scroll_to(target, nonce)
            .on_open(on_open)
            .on_create(on_create)
            .on_edit(on_edit)
            .on_day(on_day)
            .on_select(on_select);
        if showing_today {
            grid = grid.now(clock.minutes);
        }
        rect().tw(&tokens, &[w_full(), h_full()]).child(grid)
    }
}

/// A block was moved or resized: change its event, asking which of a
/// repeating series first.
fn moved(model: Model, window: Window, overlays: Overlays, edit: &BlockEdit, day: Date) {
    let reference = EventRef::parse(&edit.id);
    let Some(item) = model.item(&reference.uid) else {
        return;
    };
    let view = model.zone();
    let mut event = recur::event_at(&item, reference.key);
    // The new times are on the clock the grid shows; the event keeps its own.
    let duration = event.duration();
    let start_view = convert::at_minutes(day, edit.span.start);
    let end_view = convert::at_minutes(day, edit.span.end);
    match edit.kind {
        EditKind::Move => {
            event.start = view.convert(start_view, event.zone);
            event.end = if event.all_day {
                event.start + duration
            } else {
                event
                    .zone
                    .wall(event.zone.instant(event.start, Zone::Utc) + duration)
            };
        }
        EditKind::Resize => {
            event.start = view.convert(start_view, event.zone);
            event.end = view.convert(end_view, event.zone);
        }
    }
    if item.series.is_none() {
        model.edit(&item, None, Scope::All, event, None);
        return;
    }
    editor::ask_scope(
        overlays,
        &tr!("calendar-change-this-title"),
        &tr!("calendar-repeats-note"),
        &tr!("calendar-change"),
        false,
        move |scope| {
            let _ = window;
            model.edit(&item, reference.key, scope, event.clone(), None);
        },
    );
}
