//! A small month of days: the navigator beside a calendar, and the body of
//! a date picker.
//!
//! Today is the only day with a fill (the accent). The chosen day has the
//! neutral selection fill and an outline, so the two are told apart without
//! colour. A dot under a day says it has something on it. The whole grid is
//! one Tab stop: the arrow keys move the chosen day by a day or a week, Page
//! Up and Page Down by a month, Home and End to the ends of the month, and
//! Enter chooses it again. The caller owns the chosen day and the month on
//! show, so the grid follows whatever it is told.

use freya::prelude::*;

use super::date::{Date, Weekday};
use crate::component::ComponentName;
use crate::controls::{focus_stop, icon_button};
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, EMPHASIS, Ink, NUMERIC};

/// How many weeks a month grid draws. Six always, so the grid does not
/// change height from one month to the next.
const WEEKS: i64 = 6;

/// The ink of a day's number.
fn day_ink(today: bool, in_month: bool) -> Ink {
    if today {
        Ink::OnAccent
    } else if in_month {
        Ink::Text
    } else {
        Ink::Muted
    }
}

/// A month as a grid of days.
#[derive(PartialEq)]
pub struct MonthGrid {
    month: Date,
    today: Date,
    selected: Option<Date>,
    marked: Vec<Date>,
    first_weekday: Weekday,
    week_numbers: bool,
    header: bool,
    outside_days: bool,
    on_select: EventHandler<Date>,
    on_choose: Option<EventHandler<Date>>,
    on_page: Option<EventHandler<i32>>,
}

/// A grid of the month `month` is in. `on_select` is told the day a click or
/// a key chooses.
pub fn month_grid(month: Date, today: Date, on_select: impl Into<EventHandler<Date>>) -> MonthGrid {
    MonthGrid {
        month,
        today,
        selected: None,
        marked: Vec::new(),
        first_weekday: Weekday::Mon,
        week_numbers: false,
        header: true,
        outside_days: true,
        on_select: on_select.into(),
        on_choose: None,
        on_page: None,
    }
}

impl MonthGrid {
    /// The chosen day.
    pub fn selected(mut self, day: Date) -> Self {
        self.selected = Some(day);
        self
    }

    /// Days that have something on them.
    pub fn marked(mut self, days: Vec<Date>) -> Self {
        self.marked = days;
        self
    }

    /// The day a week starts on. Monday by default.
    pub fn first_weekday(mut self, day: Weekday) -> Self {
        self.first_weekday = day;
        self
    }

    /// Show the ISO week number at the start of each row.
    pub fn week_numbers(mut self, show: bool) -> Self {
        self.week_numbers = show;
        self
    }

    /// Without the month's name and its two arrows, for a grid whose caller
    /// draws its own.
    pub fn bare(mut self) -> Self {
        self.header = false;
        self
    }

    /// Leave out the days of the months either side of this one.
    pub fn inside_only(mut self) -> Self {
        self.outside_days = false;
        self
    }

    /// Called when a day is clicked, or Enter chooses the day the keys are
    /// on, in place of `on_select`. A date picker uses it to tell choosing a
    /// day from moving over one.
    pub fn on_choose(mut self, on_choose: impl Into<EventHandler<Date>>) -> Self {
        self.on_choose = Some(on_choose.into());
        self
    }

    /// Called with -1 or 1 when an arrow asks for another month.
    pub fn on_page(mut self, on_page: impl Into<EventHandler<i32>>) -> Self {
        self.on_page = Some(on_page.into());
        self
    }
}

impl Component for MonthGrid {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let first = self.month.first_of_month();
        let start = first.week_start(self.first_weekday);
        let on_select = self.on_select.clone();
        let on_pick = self
            .on_choose
            .clone()
            .unwrap_or_else(|| self.on_select.clone());

        let header = self.header.then(|| {
            let on_page = self.on_page.clone();
            let back = on_page.clone();
            let forward = on_page.clone();
            rect()
                .tw(
                    &tokens,
                    &[
                        w_full(),
                        flex_row(),
                        grow_children(),
                        items(Align::Center),
                        gap(S1),
                    ],
                )
                .child(
                    rect()
                        .tw(&tokens, &[w_flex(), px(S2)])
                        .child(EMPHASIS.label(&tokens, first.month_year()).max_lines(1)),
                )
                .child(
                    icon_button(Icon::GoPrevious, move |_| {
                        if let Some(handler) = &back {
                            handler.call(-1);
                        }
                    })
                    .tooltip("Previous month"),
                )
                .child(
                    icon_button(Icon::GoNext, move |_| {
                        if let Some(handler) = &forward {
                            handler.call(1);
                        }
                    })
                    .tooltip("Next month"),
                )
        });

        let weekdays = rect()
            .tw(&tokens, &[w_full(), flex_row(), grow_children()])
            .maybe_child(
                self.week_numbers
                    .then(|| rect().tw(&tokens, &[w(Dim::CalendarWeekNumber)])),
            )
            .children((0..7).map(|offset| {
                let day = self.first_weekday.plus(offset);
                rect()
                    .key(offset)
                    .tw(&tokens, &[w_flex(), center(), h(Dim::CalendarWeekNumber)])
                    .child(CAPTION.label(&tokens, day.narrow()))
                    .into_element()
            }));

        let rows = (0..WEEKS).map(|week| {
            let row_start = start.add_days(week * 7);
            rect()
                .key(week)
                .tw(&tokens, &[w_full(), flex_row(), grow_children()])
                .maybe_child(self.week_numbers.then(|| {
                    rect()
                        .tw(
                            &tokens,
                            &[w(Dim::CalendarWeekNumber), h(Dim::Control), center()],
                        )
                        .child(
                            CAPTION
                                .label(&tokens, row_start.add_days(3).iso_week().to_string())
                                .max_lines(1),
                        )
                }))
                .children((0..7).map(|column| {
                    let date = row_start.add_days(column);
                    let in_month = date.month == first.month;
                    let shown = in_month || self.outside_days;
                    DayCell {
                        date,
                        shown,
                        in_month,
                        today: date == self.today,
                        selected: self.selected == Some(date),
                        marked: shown && self.marked.contains(&date),
                        on_select: on_pick.clone(),
                    }
                    .into_element()
                }))
                .into_element()
        });

        let current = self.selected.unwrap_or(self.today);
        let label = format!("{}, {}", self.month.month_year(), current.long());
        let grid = rect()
            .tw(&tokens, &[w_full(), flex_col(), gap(S0_5)])
            .child(weekdays)
            .children(rows);

        let stop = focus_stop(
            grid,
            label,
            AccessibilityRole::Grid,
            true,
            Radius::Control,
            {
                let on_pick = on_pick.clone();
                move |_| on_pick.call(current)
            },
        )
        .on_key_down({
            let on_select = on_select.clone();
            move |event: Event<KeyboardEventData>| {
                let target = match &event.key {
                    Key::Named(NamedKey::ArrowLeft) => current.add_days(-1),
                    Key::Named(NamedKey::ArrowRight) => current.add_days(1),
                    Key::Named(NamedKey::ArrowUp) => current.add_days(-7),
                    Key::Named(NamedKey::ArrowDown) => current.add_days(7),
                    Key::Named(NamedKey::PageUp) => current.add_months(-1),
                    Key::Named(NamedKey::PageDown) => current.add_months(1),
                    Key::Named(NamedKey::Home) => current.first_of_month(),
                    Key::Named(NamedKey::End) => current.last_of_month(),
                    _ => return,
                };
                event.stop_propagation();
                event.prevent_default();
                on_select.call(target);
            }
        });

        rect()
            .component("MonthGrid")
            .tw(&tokens, &[w_full(), flex_col(), gap(S1)])
            .maybe_child(header)
            .child(stop)
    }
}

/// One day of a month grid.
#[derive(PartialEq)]
struct DayCell {
    date: Date,
    shown: bool,
    in_month: bool,
    today: bool,
    selected: bool,
    marked: bool,
    on_select: EventHandler<Date>,
}

impl Component for DayCell {
    fn render_key(&self) -> DiffKey {
        super::key_of(self.date.days())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let cell = rect().tw(
            &tokens,
            &[
                w_flex(),
                h(Dim::Control),
                flex_col(),
                items(Align::Center),
                justify(Align::Center),
            ],
        );
        if !self.shown {
            return cell.into_element();
        }
        let date = self.date;
        let on_select = self.on_select.clone();
        let label = format!(
            "{}{}",
            date.long(),
            if self.marked { ", has events" } else { "" }
        );
        let (today, selected) = (self.today, self.selected);
        let number = NUMERIC
            .in_ink(day_ink(self.today, self.in_month))
            .label(&tokens, date.day.to_string())
            .max_lines(1);
        let face = rect()
            .tw(
                &tokens,
                &[
                    w(Dim::Box7),
                    h(Dim::Box7),
                    center(),
                    rounded(Radius::Pill),
                    face_fill(today, selected, hovering(), pressing()),
                ],
            )
            .maybe(selected && !today, |element| {
                element.tw(&tokens, &[border(Line::Outline)])
            })
            .child(number);
        // The dot's slot is always there, so a day that gains an event does
        // not move its number.
        let dot = rect().tw(
            &tokens,
            &[
                w(Dim::CalendarDot),
                h(Dim::CalendarDot),
                rounded(Radius::Pill),
                bg(if self.marked {
                    Paint::Muted
                } else {
                    Paint::Transparent
                }),
            ],
        );
        cell.component("DayCell")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(label)
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set_if_modified(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |_| pressing.set(true))
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| on_select.call(date))
            .child(face)
            .child(dot)
            .into_element()
    }
}

/// The fill of a day's face.
fn face_fill(today: bool, selected: bool, hovering: bool, pressing: bool) -> Util {
    if today {
        return bg(Paint::Accent);
    }
    surface(Paint::Transparent, hovering, pressing, selected)
}
