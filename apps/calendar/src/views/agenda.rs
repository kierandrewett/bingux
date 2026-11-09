//! Lists of events by day: the agenda, the results of a search, and the
//! chosen day under the small month on a phone. These are also how the
//! calendar is used with a screen reader and the keyboard: every event is a
//! stop of its own.

use bingux_calendar_core::Occurrence;
use bingux_calendar_core::chrono::Duration;
use bingux_ui::calendar::Date;
use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, CAPTION, EMPHASIS};
use freya::prelude::*;

use super::{colour_of, open, shown, title_of};
use crate::convert;
use crate::editor::{self, Seed};
use crate::model::{EventRef, Model, time_text};

/// "Today", "Tomorrow", or the weekday.
fn heading(day: Date, today: Date) -> String {
    match day.days() - today.days() {
        -1 => tr!("calendar-yesterday"),
        0 => tr!("calendar-today"),
        1 => tr!("calendar-tomorrow"),
        _ => day.weekday().name().to_string(),
    }
}

/// "12 March 2025".
fn date_line(day: Date, today: Date) -> String {
    if day.year == today.year {
        format!(
            "{} {}",
            day.day,
            bingux_ui::calendar::date::month_name(day.month)
        )
    } else {
        day.long()
    }
}

/// The occurrences of each day from `from` to `to` (not included) that has
/// any, in order. An event that lasts several days is under each.
fn by_day(occurrences: &[Occurrence], from: Date, to: Date) -> Vec<(Date, Vec<Occurrence>)> {
    let mut days: Vec<(Date, Vec<Occurrence>)> = Vec::new();
    let mut day = from;
    while day < to {
        let here: Vec<Occurrence> = occurrences
            .iter()
            .filter(|occurrence| {
                let (first, last) = occurrence.days();
                convert::date_of(first) <= day && day <= convert::date_of(last)
            })
            .cloned()
            .collect();
        if !here.is_empty() {
            days.push((day, here));
        }
        day = day.add_days(1);
    }
    days
}

/// The agenda: what is coming up from the day on show.
#[derive(PartialEq)]
pub struct AgendaView {
    pub model: Model,
}

impl Component for AgendaView {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let mut span = use_state(|| 45i64);
        let anchor = *model.anchor.read();
        let clock = *model.clock.read();
        let to = anchor.add_days(*span.read());
        let occurrences = shown(model.occurrences(anchor, to));
        let days = by_day(&occurrences, anchor, to);
        if days.is_empty() {
            return status_page(Icon::XOfficeCalendar, tr!("calendar-nothing-coming-up"))
                .description(tr!("calendar-no-events-in-days", days = *span.read()))
                .action(tr!("calendar-add-event"), move |_| {
                    editor::open(model, window, overlays, Seed::at_day(&model, anchor));
                })
                .into_element();
        }
        let sections = days.into_iter().map(|(day, here)| {
            DaySection {
                model,
                day,
                today: clock.today,
                occurrences: here,
            }
            .into_element()
        });
        ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_col(), gap(S3), p(S3)])
                    .children(sections)
                    .child(
                        rect()
                            .tw(&tokens, &[w_full(), flex_row(), justify(Align::Center)])
                            .child(action_button("Show 45 more days", move |_| {
                                let next = *span.peek() + 45;
                                span.set(next);
                            })),
                    ),
            )
            .into_element()
    }
}

/// One day under the small month, with a line to say when it is empty.
#[derive(PartialEq)]
pub struct DayList {
    pub model: Model,
    pub day: Date,
}

impl Component for DayList {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let day = self.day;
        let clock = *model.clock.read();
        let occurrences = shown(model.occurrences(day, day.add_days(1)));
        let body: Element = if occurrences.is_empty() {
            rect()
                .tw(
                    &tokens,
                    &[w_full(), flex_col(), items(Align::Center), gap(S2), p(S4)],
                )
                .child(CAPTION.label(&tokens, tr!("calendar-no-events")))
                .child(action_button(tr!("calendar-add-event"), move |_| {
                    editor::open(model, window, overlays, Seed::at_day(&model, day));
                }))
                .into_element()
        } else {
            DaySection {
                model,
                day,
                today: clock.today,
                occurrences,
            }
            .into_element()
        };
        ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(rect().tw(&tokens, &[w_full(), p(S2)]).child(body))
    }
}

/// What a search found.
#[derive(PartialEq)]
pub struct SearchResults {
    pub model: Model,
    pub query: String,
}

impl Component for SearchResults {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let model = self.model;
        let clock = *model.clock.read();
        let now = convert::at_minutes(clock.today, clock.minutes);
        let data = model.data();
        let found = data.search(
            &self.query,
            now - Duration::days(365 * 2),
            now + Duration::days(365 * 3),
            now,
            clock.zone,
            &model.hidden(),
            200,
        );
        let found = shown(found);
        if found.is_empty() {
            return status_page(Icon::Search, tr!("calendar-no-results"))
                .description(tr!("calendar-nothing-matches", query = self.query.trim()))
                .into_element();
        }
        let first = found
            .first()
            .map(|o| convert::date_of(o.start.date()))
            .unwrap_or(clock.today);
        let last = found
            .last()
            .map(|o| convert::date_of(o.start.date()))
            .unwrap_or(clock.today);
        let sections = by_day(&found, first, last.add_days(1))
            .into_iter()
            .map(|(day, here)| {
                DaySection {
                    model,
                    day,
                    today: clock.today,
                    occurrences: here,
                }
                .into_element()
            });
        ScrollView::new()
            .width(Size::fill())
            .height(Size::fill())
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_col(), gap(S3), p(S3)])
                    .children(sections),
            )
            .into_element()
    }
}

/// A day's name and its events.
#[derive(PartialEq)]
struct DaySection {
    model: Model,
    day: Date,
    today: Date,
    occurrences: Vec<Occurrence>,
}

impl Component for DaySection {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let day = self.day;
        let twelve = model.twelve_hour();
        let rows = self.occurrences.iter().map(|occurrence| {
            let id = EventRef::of(occurrence).id();
            AgendaRow {
                id: id.clone(),
                title: title_of(occurrence),
                time: time_text(occurrence, day, twelve),
                place: occurrence.location.clone(),
                colour: colour_of(&model, &occurrence.calendar, &tokens),
                selected: model.selected.read().as_deref() == Some(id.as_str()),
                on_open: EventHandler::new(move |id: String| open(model, window, overlays, &id)),
            }
            .into_element()
        });
        rect()
            .tw(&tokens, &[w_full(), flex_col(), gap(S1)])
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[w_full(), flex_row(), items(Align::Center), gap(S2), px(S3)],
                    )
                    .child(
                        EMPHASIS
                            .label(&tokens, heading(day, self.today))
                            .max_lines(1),
                    )
                    .child(
                        CAPTION
                            .label(&tokens, date_line(day, self.today))
                            .max_lines(1),
                    ),
            )
            .children(rows)
    }
}

/// One event in a list.
#[derive(PartialEq)]
struct AgendaRow {
    id: String,
    title: String,
    time: String,
    place: String,
    colour: Color,
    selected: bool,
    on_open: EventHandler<String>,
}

impl Component for AgendaRow {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.id, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let pressing = use_state(|| false);
        let id = self.id.clone();
        let on_open = self.on_open.clone();
        let label = if self.place.is_empty() {
            format!("{}, {}", self.title, self.time)
        } else {
            format!("{}, {}, {}", self.title, self.time, self.place)
        };
        let content = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S3),
                    px(S3),
                    py(S2),
                    surface(Paint::Transparent, hovering(), pressing(), self.selected),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .child(rect().tw(
                &tokens,
                &[
                    w(Dim::CalendarBar),
                    h(Dim::Box9),
                    rounded(Radius::Pill),
                    fill(self.colour),
                ],
            ))
            .child(
                rect()
                    .tw(&tokens, &[w_flex(), flex_col()])
                    .child(
                        BODY.label(&tokens, self.title.clone())
                            .max_lines(2)
                            .text_overflow(TextOverflow::Ellipsis),
                    )
                    .child(
                        CAPTION
                            .label(
                                &tokens,
                                if self.place.is_empty() {
                                    self.time.clone()
                                } else {
                                    format!("{} · {}", self.time, self.place)
                                },
                            )
                            .max_lines(1)
                            .text_overflow(TextOverflow::Ellipsis),
                    ),
            );
        focus_stop_press(
            content,
            label,
            AccessibilityRole::Button,
            true,
            Radius::Control,
            move |_| on_open.call(id.clone()),
            pressing,
        )
    }
}
