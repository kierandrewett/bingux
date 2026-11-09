//! The month: a cell for each day on a desktop, and on a phone a small month
//! over the list of the chosen day.

use bingux_ui::calendar::{Date, MonthItem, format_clock, month_grid, month_view};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::agenda::DayList;
use super::{colour_of, open, shown, title_of};
use crate::convert;
use crate::editor::{self, Seed};
use crate::model::{EventRef, Model};

#[derive(PartialEq)]
pub struct MonthPage {
    pub model: Model,
}

impl Component for MonthPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let clock = *model.clock.read();
        let anchor = *model.anchor.read();
        let first_weekday = model.first_weekday();
        let week_numbers = model.prefs.read().week_numbers;
        let first = anchor.first_of_month();
        let start = first.week_start(first_weekday);
        let end = start.add_days(42);
        let twelve = model.twelve_hour();
        let occurrences = shown(model.occurrences(start, end));

        let on_select = move |day: Date| model.go(day);

        if is_compact() {
            let marked: Vec<Date> = (0..42)
                .map(|offset| start.add_days(offset))
                .filter(|day| {
                    occurrences.iter().any(|occurrence| {
                        let (from, to) = occurrence.days();
                        convert::date_of(from) <= *day && *day <= convert::date_of(to)
                    })
                })
                .collect();
            return rect()
                .tw(
                    &tokens,
                    &[w_full(), h_full(), flex_col(), grow_children(), gap(S2)],
                )
                .child(
                    rect().tw(&tokens, &[w_full(), px(S2)]).child(
                        month_grid(anchor, clock.today, on_select)
                            .selected(anchor)
                            .first_weekday(first_weekday)
                            .week_numbers(week_numbers)
                            .marked(marked)
                            .on_page(move |delta: i32| model.go(anchor.add_months(delta))),
                    ),
                )
                .child(divider(&tokens))
                .child(
                    rect()
                        .tw(&tokens, &[w_full(), h_flex()])
                        .child(DayList { model, day: anchor }),
                )
                .into_element();
        }

        let items: Vec<MonthItem> = occurrences
            .iter()
            .map(|occurrence| {
                let (from, to) = occurrence.days();
                MonthItem {
                    id: EventRef::of(occurrence).id(),
                    first: convert::date_of(from),
                    last: convert::date_of(to),
                    title: title_of(occurrence),
                    colour: colour_of(&model, &occurrence.calendar, &tokens),
                    bar: occurrence.is_banner(),
                    time: if occurrence.all_day {
                        String::new()
                    } else {
                        format_clock(convert::minutes_of(occurrence.start), twelve)
                    },
                }
            })
            .collect();
        let length = model.prefs.read().default_length.max(15);
        month_view(anchor, clock.today, on_select)
            .selected(anchor)
            .first_weekday(first_weekday)
            .week_numbers(week_numbers)
            .items(items)
            .on_open(move |id: String| open(model, window, overlays, &id))
            .on_create(move |day: Date| {
                // A day pressed twice: an event at nine, or at the next hour
                // today.
                let start = if day == clock.today {
                    ((clock.minutes / 60 + 1) * 60).min(23 * 60)
                } else {
                    9 * 60
                };
                editor::open(
                    model,
                    window,
                    overlays,
                    Seed::new_at(
                        convert::at_minutes(day, start),
                        convert::at_minutes(day, start + length as u32),
                    ),
                );
            })
            .on_activate(move |day: Date| {
                model.go(day);
                model.show(bingux_calendar_core::ViewKind::Day);
            })
            .into_element()
    }
}
