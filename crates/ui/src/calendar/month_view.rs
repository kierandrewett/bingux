//! A month the size of the window: a cell for each day, events as bars that
//! run across the days they cover and as chips for events with a time.
//!
//! A cell shows as many events as fit and then "+3 more", which asks for the
//! day. A press on a cell chooses its day; two quick presses ask for a new
//! event on it. The grid is one Tab stop: the arrow keys move the chosen day,
//! Page Up and Page Down the month, Enter asks for the day.
//!
//! The grid draws what it is given and reports what a person did.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freya::prelude::*;

use super::date::{Date, Weekday};
use super::layout;
use super::palette::event_colours;
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, EVENT_TITLE, Ink, NUMERIC};

const DOUBLE_PRESS: Duration = Duration::from_millis(400);

/// An event, as the month draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct MonthItem {
    pub id: String,
    pub first: Date,
    pub last: Date,
    pub title: String,
    pub colour: Color,
    /// A filled bar (an event that lasts whole days) and not a line of text.
    pub bar: bool,
    /// The time shown before the title of a chip, such as "9:00".
    pub time: String,
    /// The minute of the day it starts, which orders the chips of one day.
    pub minutes: u32,
}

/// A month of day cells.
#[derive(PartialEq)]
pub struct MonthView {
    month: Date,
    today: Date,
    selected: Option<Date>,
    first_weekday: Weekday,
    week_numbers: bool,
    items: Vec<MonthItem>,
    selected_item: Option<String>,
    on_pick: EventHandler<Option<String>>,
    on_select: EventHandler<Date>,
    on_open: EventHandler<String>,
    on_create: EventHandler<Date>,
    on_activate: EventHandler<Date>,
}

/// The month `month` is in. `on_select` is told the day a press or a key
/// chooses.
pub fn month_view(month: Date, today: Date, on_select: impl Into<EventHandler<Date>>) -> MonthView {
    MonthView {
        month,
        today,
        selected: None,
        first_weekday: Weekday::Mon,
        week_numbers: false,
        items: Vec::new(),
        selected_item: None,
        on_pick: EventHandler::new(|_| {}),
        on_select: on_select.into(),
        on_open: EventHandler::new(|_| {}),
        on_create: EventHandler::new(|_| {}),
        on_activate: EventHandler::new(|_| {}),
    }
}

impl MonthView {
    pub fn selected(mut self, day: Date) -> Self {
        self.selected = Some(day);
        self
    }

    pub fn first_weekday(mut self, day: Weekday) -> Self {
        self.first_weekday = day;
        self
    }

    pub fn week_numbers(mut self, show: bool) -> Self {
        self.week_numbers = show;
        self
    }

    pub fn items(mut self, items: Vec<MonthItem>) -> Self {
        self.items = items;
        self
    }

    /// The event, by `MonthItem::id`, that the keyboard has chosen: drawn
    /// strong, with an outline on a bar.
    pub fn selected_item(mut self, id: Option<String>) -> Self {
        self.selected_item = id;
        self
    }

    /// The keyboard chose an event (Enter on a day with events, then Up and
    /// Down walk that day's events) or let go of it (`None`: Escape, or a
    /// move to another day).
    pub fn on_pick(mut self, handler: impl Into<EventHandler<Option<String>>>) -> Self {
        self.on_pick = handler.into();
        self
    }

    /// An event was pressed.
    pub fn on_open(mut self, handler: impl Into<EventHandler<String>>) -> Self {
        self.on_open = handler.into();
        self
    }

    /// A day was pressed twice: it asks for a new event.
    pub fn on_create(mut self, handler: impl Into<EventHandler<Date>>) -> Self {
        self.on_create = handler.into();
        self
    }

    /// A day was asked for: "+3 more", or Enter on the chosen day.
    pub fn on_activate(mut self, handler: impl Into<EventHandler<Date>>) -> Self {
        self.on_activate = handler.into();
        self
    }
}

/// The weeks a month needs: five or six rows.
fn weeks_of(first: Date, week_start: Weekday) -> i64 {
    let lead = first.days() - first.week_start(week_start).days();
    (lead + i64::from(first.days_in_month()) + 6) / 7
}

impl Component for MonthView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut size = use_state(|| (0.0f32, 0.0f32));
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let first = self.month.first_of_month();
        let start = first.week_start(self.first_weekday);
        let rows = weeks_of(first, self.first_weekday);
        let (width, height) = *size.read();
        let number_w = if self.week_numbers {
            Dim::CalendarWeekNumber.px()
        } else {
            0.0
        };
        let head_h = Dim::CalendarWeekNumber.px();
        let row_h = ((height - head_h) / rows as f32).max(0.0);
        let cell_w = ((width - number_w) / 7.0).max(0.0);
        let chip = Dim::CalendarChip.px();
        let gap = S0_5.px();
        let number_h = Dim::CalendarWeekNumber.px();

        // The bars of the month, as day ranges, in the order they are drawn.
        let mut order: Vec<usize> = (0..self.items.len()).collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (&self.items[a], &self.items[b]);
            x.first
                .cmp(&y.first)
                .then((y.last.days() - y.first.days()).cmp(&(x.last.days() - x.first.days())))
                .then(y.bar.cmp(&x.bar))
                .then(x.minutes.cmp(&y.minutes))
                .then(x.title.cmp(&y.title))
        });
        let ranges: Vec<(Date, Date)> = order
            .iter()
            .map(|&i| (self.items[i].first, self.items[i].last))
            .collect();

        let mut layers: Vec<Element> = Vec::new();
        for week in 0..rows {
            let week_start = start.add_days(week * 7);
            let top = head_h + week as f32 * row_h;
            let runs = layout::runs_in_week(week_start, &ranges);
            let placed = layout::lanes(&runs, 7);
            let lane_count = placed.iter().map(|p| p.lane + 1).max().unwrap_or(0);
            let room = (((row_h - number_h) / (chip + gap)).floor().max(0.0)) as usize;
            let (shown_lanes, overflow) = if lane_count > room && room > 0 {
                (room - 1, true)
            } else {
                (room, false)
            };
            // Events that do not fit, counted in each column they cover.
            let mut hidden = [0usize; 7];
            if overflow {
                for p in placed.iter().filter(|p| p.lane >= shown_lanes) {
                    for column in p.run.first..=p.run.last {
                        hidden[column] += 1;
                    }
                }
            }
            for p in placed.iter().filter(|p| p.lane < shown_lanes) {
                let item = &self.items[order[p.run.index]];
                layers.push(
                    MonthChip {
                        id: item.id.clone(),
                        title: item.title.clone(),
                        time: item.time.clone(),
                        colour: item.colour,
                        bar: item.bar,
                        selected: self.selected_item.as_deref() == Some(item.id.as_str()),
                        x: number_w + p.run.first as f32 * cell_w + gap,
                        y: top + number_h + p.lane as f32 * (chip + gap),
                        w: ((p.run.last - p.run.first + 1) as f32 * cell_w - gap * 2.0).max(0.0),
                        continues_before: p.run.continues_before,
                        continues_after: p.run.continues_after,
                        on_open: self.on_open.clone(),
                    }
                    .into_element(),
                );
            }
            if overflow {
                for (column, count) in hidden.iter().enumerate().filter(|(_, count)| **count > 0) {
                    let date = week_start.add_days(column as i64);
                    layers.push(
                        MoreChip {
                            date,
                            count: *count,
                            x: number_w + column as f32 * cell_w + gap,
                            y: top + number_h + shown_lanes as f32 * (chip + gap),
                            w: (cell_w - gap * 2.0).max(0.0),
                            on_activate: self.on_activate.clone(),
                        }
                        .into_element(),
                    );
                }
            }
        }

        let cells = (0..rows).flat_map(|week| {
            let week_start = start.add_days(week * 7);
            (0..7).map(move |column| (week, column, week_start.add_days(column)))
        });
        let cell_views = cells.map(|(week, column, date)| {
            MonthCell {
                date,
                in_month: date.month == first.month,
                today: date == self.today,
                selected: self.selected == Some(date),
                first_of_month: date.day == 1,
                x: number_w + column as f32 * cell_w,
                y: head_h + week as f32 * row_h,
                w: cell_w,
                h: row_h,
                on_select: self.on_select.clone(),
                on_create: self.on_create.clone(),
            }
            .into_element()
        });

        // Hairlines between the days.
        let lines_h = (0..=rows).map(|row| {
            rect()
                .key(format!("h{row}"))
                .position(
                    Position::new_absolute()
                        .left(number_w)
                        .top(head_h + row as f32 * row_h),
                )
                .width(Size::px((width - number_w).max(0.0)))
                .tw(&tokens, &[h(Dim::Hairline), bg(Paint::Separator)])
                .into_element()
        });
        let lines_v = (0..=7).map(|column| {
            rect()
                .key(format!("v{column}"))
                .position(
                    Position::new_absolute()
                        .left(number_w + column as f32 * cell_w)
                        .top(head_h),
                )
                .height(Size::px((height - head_h).max(0.0)))
                .tw(&tokens, &[w(Dim::Hairline), bg(Paint::Separator)])
                .into_element()
        });

        let heads = (0..7i64).map(|offset| {
            let day = self.first_weekday.plus(offset);
            rect()
                .key(offset)
                .position(
                    Position::new_absolute()
                        .left(number_w + offset as f32 * cell_w)
                        .top(0.0),
                )
                .width(Size::px(cell_w))
                .tw(&tokens, &[h(Dim::CalendarWeekNumber), center()])
                .child(
                    CAPTION
                        .in_ink(if day.is_weekend() {
                            Ink::Muted
                        } else {
                            Ink::Text
                        })
                        .label(&tokens, day.short())
                        .max_lines(1),
                )
                .into_element()
        });
        let week_labels = self.week_numbers.then(|| {
            (0..rows)
                .map(|week| {
                    rect()
                        .key(week)
                        .position(
                            Position::new_absolute()
                                .left(0.0)
                                .top(head_h + week as f32 * row_h),
                        )
                        .width(Size::px(number_w))
                        .tw(&tokens, &[h(Dim::CalendarWeekNumber), center()])
                        .child(
                            CAPTION
                                .label(&tokens, start.add_days(week * 7 + 3).iso_week().to_string())
                                .max_lines(1),
                        )
                        .into_element()
                })
                .collect::<Vec<_>>()
        });

        let current = self.selected.unwrap_or(self.today);
        let on_select = self.on_select.clone();
        let on_activate = self.on_activate.clone();
        let on_pick = self.on_pick.clone();
        let on_open_key = self.on_open.clone();
        let picked = self.selected_item.clone();
        // The events of the chosen day, in the order a cell lists them.
        let day_events: Vec<String> = {
            let mut found: Vec<&MonthItem> = self
                .items
                .iter()
                .filter(|item| item.first <= current && current <= item.last)
                .collect();
            found.sort_by(|a, b| {
                b.bar
                    .cmp(&a.bar)
                    .then(a.minutes.cmp(&b.minutes))
                    .then(a.title.cmp(&b.title))
            });
            found.into_iter().map(|item| item.id.clone()).collect()
        };
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let at = picked
                .as_ref()
                .and_then(|id| day_events.iter().position(|other| other == id));
            match &event.key {
                Key::Named(NamedKey::Escape) if picked.is_some() => {
                    event.stop_propagation();
                    on_pick.call(None);
                    return;
                }
                Key::Named(NamedKey::ArrowDown) if at.is_some() => {
                    event.stop_propagation();
                    event.prevent_default();
                    if let Some(next) = at.and_then(|at| day_events.get(at + 1)) {
                        on_pick.call(Some(next.clone()));
                    }
                    return;
                }
                Key::Named(NamedKey::ArrowUp) if at.is_some() => {
                    event.stop_propagation();
                    event.prevent_default();
                    match at {
                        Some(at) if at > 0 => on_pick.call(Some(day_events[at - 1].clone())),
                        _ => on_pick.call(None),
                    }
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    event.stop_propagation();
                    if let Some(id) = picked.as_ref().filter(|_| at.is_some()) {
                        on_open_key.call(id.clone());
                    } else if let Some(first) = day_events.first() {
                        on_pick.call(Some(first.clone()));
                    } else {
                        on_activate.call(current);
                    }
                    return;
                }
                _ => {}
            }
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
            if picked.is_some() {
                on_pick.call(None);
            }
            event.stop_propagation();
            event.prevent_default();
            on_select.call(target);
        };

        rect()
            .component("MonthView")
            .a11y_id(a11y_id)
            .a11y_focusable(true)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Grid)
            .a11y_alt(format!(
                "{}, {} events",
                self.month.month_year(),
                self.items.len()
            ))
            .tw(&tokens, &[w_full(), h_full()])
            .on_key_down(on_key_down)
            .on_sized(move |event: Event<SizedEventData>| {
                size.set_if_modified((event.area.width(), event.area.height()));
            })
            .children(heads)
            .children(week_labels.into_iter().flatten())
            .children(cell_views)
            .children(lines_h)
            .children(lines_v)
            .children(layers)
            .focusable_inside(focus())
            .tw(&tokens, &[w_full(), h_full()])
    }
}

/// One day of the month.
#[derive(PartialEq)]
struct MonthCell {
    date: Date,
    in_month: bool,
    today: bool,
    selected: bool,
    first_of_month: bool,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    on_select: EventHandler<Date>,
    on_create: EventHandler<Date>,
}

impl Component for MonthCell {
    fn render_key(&self) -> DiffKey {
        super::key_of(self.date.days())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let last = use_hook(|| Rc::new(Cell::new(None::<Instant>)));
        let date = self.date;
        let (on_select, on_create) = (self.on_select.clone(), self.on_create.clone());
        let label = if self.first_of_month {
            format!("{} {}", date.day, super::date::month_short(date.month))
        } else {
            date.day.to_string()
        };
        let number = rect()
            .tw(
                &tokens,
                &[
                    min_w(Dim::Box7),
                    h(Dim::Box7),
                    px(S1),
                    center(),
                    rounded(Radius::Pill),
                    number_fill(self.today),
                ],
            )
            .child(
                NUMERIC
                    .in_ink(number_ink(self.today, self.in_month))
                    .label(&tokens, label)
                    .max_lines(1),
            );
        rect()
            .component("MonthCell")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(date.long())
            .position(Position::new_absolute().left(self.x).top(self.y))
            .width(Size::px(self.w))
            .height(Size::px(self.h))
            .tw(
                &tokens,
                &[
                    flex_row(),
                    justify(Align::End),
                    p(S1),
                    surface(Paint::Transparent, hovering(), false, self.selected),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| {
                on_select.call(date);
                let now = Instant::now();
                if last
                    .get()
                    .is_some_and(|at| now.duration_since(at) < DOUBLE_PRESS)
                {
                    last.set(None);
                    on_create.call(date);
                } else {
                    last.set(Some(now));
                }
            })
            .child(number)
    }
}

fn number_ink(today: bool, in_month: bool) -> Ink {
    if today {
        Ink::OnAccent
    } else if in_month {
        Ink::Text
    } else {
        Ink::Muted
    }
}

/// An event in a cell: a bar, or a line of text.
#[derive(PartialEq)]
struct MonthChip {
    id: String,
    title: String,
    time: String,
    colour: Color,
    bar: bool,
    selected: bool,
    x: f32,
    y: f32,
    w: f32,
    continues_before: bool,
    continues_after: bool,
    on_open: EventHandler<String>,
}

impl Component for MonthChip {
    fn render_key(&self) -> DiffKey {
        super::key_of((&self.id, self.y as i64))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let colours = event_colours(&tokens, self.colour);
        let id = self.id.clone();
        let on_open = self.on_open.clone();
        let text = if self.time.is_empty() || self.bar {
            self.title.clone()
        } else {
            format!("{} {}", self.time, self.title)
        };
        let base = rect()
            .component("MonthChip")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{}, {}", self.title, self.time))
            .position(Position::new_absolute().left(self.x).top(self.y))
            .width(Size::px(self.w))
            .tw(&tokens, &[h(Dim::CalendarChip)])
            .tw(
                &tokens,
                &[flex_row(), items(Align::Center), clip(), px(S1), gap(S1)],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| on_open.call(id.clone()));
        if self.bar {
            return base
                .tw(
                    &tokens,
                    &[
                        rounded_on(
                            Radius::Mark,
                            bar_corners(self.continues_before, self.continues_after),
                            0.0,
                        ),
                        fill(colours.solid),
                    ],
                )
                .maybe(self.selected, |element| {
                    element.tw(&tokens, &[border(Line::Edge(Paint::Accent))])
                })
                .maybe(hovering(), |element| element.opacity(0.85))
                .child(
                    EVENT_TITLE
                        .label_filled(&tokens, text, colours.on_solid)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .into_element();
        }
        base.tw(
            &tokens,
            &[
                rounded(Radius::Mark),
                surface(Paint::Transparent, hovering(), false, self.selected),
            ],
        )
        .child(rect().tw(
            &tokens,
            &[
                w(Dim::CalendarDot),
                h(Dim::CalendarDot),
                rounded(Radius::Pill),
                fill(colours.solid),
            ],
        ))
        .child(
            CAPTION
                .in_ink(Ink::Text)
                .label(&tokens, text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
        .into_element()
    }
}

fn bar_corners(before: bool, after: bool) -> Corners {
    match (before, after) {
        (false, false) => Corners::All,
        (true, false) => Corners::Right,
        (false, true) => Corners::Left,
        (true, true) => Corners::None,
    }
}

/// "+3 more".
#[derive(PartialEq)]
struct MoreChip {
    date: Date,
    count: usize,
    x: f32,
    y: f32,
    w: f32,
    on_activate: EventHandler<Date>,
}

impl Component for MoreChip {
    fn render_key(&self) -> DiffKey {
        super::key_of(self.date.days())
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let date = self.date;
        let on_activate = self.on_activate.clone();
        rect()
            .component("MoreChip")
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(format!("{} more events on {}", self.count, date.long()))
            .position(Position::new_absolute().left(self.x).top(self.y))
            .width(Size::px(self.w))
            .tw(&tokens, &[h(Dim::CalendarChip)])
            .tw(
                &tokens,
                &[
                    flex_row(),
                    items(Align::Center),
                    px(S1),
                    rounded(Radius::Mark),
                    surface(Paint::Transparent, hovering(), false, false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .on_press(move |_: Event<PressEventData>| on_activate.call(date))
            .child(
                CAPTION
                    .in_ink(Ink::Muted)
                    .label(&tokens, format!("+{} more", self.count))
                    .max_lines(1),
            )
    }
}

/// The fill of a day number in a cell.
fn number_fill(today: bool) -> Util {
    if today {
        // ui-lint: allow(the number inside is Ink::OnAccent, set in number_ink)
        bg(Paint::Accent)
    } else {
        bg(Paint::Transparent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_needs_the_rows_it_spans() {
        let d = |y, m, day| Date::new(y, m, day).unwrap();
        // February 2026 starts on a Sunday and has 28 days: four Monday-first
        // rows would not hold it (Sunday is the last column), five do.
        assert_eq!(weeks_of(d(2026, 2, 1), Weekday::Mon), 5);
        assert_eq!(weeks_of(d(2026, 2, 1), Weekday::Sun), 4);
        // March 2025 starts on a Saturday: six Monday-first rows.
        assert_eq!(weeks_of(d(2025, 3, 1), Weekday::Mon), 6);
        assert_eq!(weeks_of(d(2025, 3, 1), Weekday::Sun), 6);
        assert_eq!(weeks_of(d(2025, 9, 1), Weekday::Mon), 5);
    }
}
