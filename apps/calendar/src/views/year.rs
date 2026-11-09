//! The year: twelve small months. A day with something on it has a dot;
//! a press on a day opens it, a press on a month's name opens the month.

use bingux_ui::calendar::date::month_name;
use bingux_ui::calendar::{Date, month_grid};
use bingux_ui::prelude::*;
use freya::prelude::*;

use super::shown;
use crate::convert;
use crate::model::Model;

#[derive(PartialEq)]
pub struct YearPage {
    pub model: Model,
}

impl Component for YearPage {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let model = self.model;
        let clock = *model.clock.read();
        let anchor = *model.anchor.read();
        let year = anchor.year;
        let first_weekday = model.first_weekday();
        let week_numbers = model.prefs.read().week_numbers;
        let start = Date::new(year, 1, 1).unwrap_or(anchor);
        let end = Date::new(year + 1, 1, 1).unwrap_or(anchor);
        let occurrences = shown(model.occurrences(start, end));
        let mut days_with_events: Vec<Date> = Vec::new();
        for occurrence in &occurrences {
            let (from, to) = occurrence.days();
            let (from, to) = (convert::date_of(from), convert::date_of(to));
            let mut day = from.max(start);
            while day <= to.min(end.add_days(-1)) {
                days_with_events.push(day);
                day = day.add_days(1);
            }
        }
        days_with_events.sort();
        days_with_events.dedup();

        let columns = if is_compact() { 2 } else { 4 };
        let months: Vec<Element> = (1..=12u8)
            .map(|month| {
                let first = Date::new(year, month, 1).unwrap_or(anchor);
                let marked: Vec<Date> = days_with_events
                    .iter()
                    .copied()
                    .filter(|day| day.month == month)
                    .collect();
                let selected_here = anchor.month == month;
                let cell = rect()
                    .tw(&tokens, &[w_flex(), flex_col(), gap(S1), p(S2)])
                    .child(
                        action_button(month_name(month), move |_| {
                            model.go(first);
                            model.show(bingux_calendar_core::ViewKind::Month);
                        })
                        .flat(),
                    )
                    .child({
                        let grid = month_grid(first, clock.today, move |day: Date| {
                            model.go(day);
                            model.show(bingux_calendar_core::ViewKind::Day);
                        })
                        .bare()
                        .inside_only()
                        .marked(marked)
                        .first_weekday(first_weekday)
                        .week_numbers(week_numbers);
                        if selected_here {
                            grid.selected(anchor)
                        } else {
                            grid
                        }
                    });
                cell.into_element()
            })
            .collect();

        let rows: Vec<Element> = (0..months.len().div_ceil(columns))
            .map(|row| {
                let cells: Vec<Element> = (0..columns)
                    .filter_map(|column| months.get(row * columns + column).cloned())
                    .collect();
                rect()
                    .tw(&tokens, &[w_full(), flex_row(), grow_children()])
                    .children(cells)
                    .into_element()
            })
            .collect();

        ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_col(), p(S2)])
                    .children(rows),
            )
    }
}
