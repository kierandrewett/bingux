//! The header bar: where you are in time (Today and the arrows), the view,
//! search, a new event, and the menu.

use bingux_calendar_core::{HourFormat, ViewKind};
use bingux_ui::calendar::Date;
use bingux_ui::calendar::date::{month_name, month_short};
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use freya::prelude::*;

use crate::calendars;
use crate::model::Model;
use crate::quick;
use crate::sidebar::Sidebar;

/// What the bar says about the time on show.
pub fn title_of(model: &Model) -> String {
    let anchor = *model.anchor.read();
    match *model.view.read() {
        ViewKind::Day => anchor.long(),
        ViewKind::Week => {
            let start = anchor.week_start(model.first_weekday());
            let end = start.add_days(6);
            if start.month == end.month {
                format!("{} \u{2013} {} {}", start.day, end.day, anchor_month(end))
            } else if start.year == end.year {
                format!(
                    "{} {} \u{2013} {} {} {}",
                    start.day,
                    month_short(start.month),
                    end.day,
                    month_short(end.month),
                    end.year
                )
            } else {
                format!(
                    "{} {} {} \u{2013} {} {} {}",
                    start.day,
                    month_short(start.month),
                    start.year,
                    end.day,
                    month_short(end.month),
                    end.year
                )
            }
        }
        ViewKind::Month => anchor.month_year(),
        ViewKind::Year => anchor.year.to_string(),
        ViewKind::Agenda => format!("From {}", anchor.long()),
    }
}

fn anchor_month(date: Date) -> String {
    format!("{} {}", month_name(date.month), date.year)
}

/// The label of each view, in the order of the bar.
pub const VIEWS: [(&str, &str, ViewKind); 5] = [
    ("day", "Day", ViewKind::Day),
    ("week", "Week", ViewKind::Week),
    ("month", "Month", ViewKind::Month),
    ("year", "Year", ViewKind::Year),
    ("agenda", "Agenda", ViewKind::Agenda),
];

fn view_key(kind: ViewKind) -> &'static str {
    VIEWS
        .iter()
        .find(|(_, _, view)| *view == kind)
        .map_or("week", |(key, _, _)| key)
}

#[derive(PartialEq)]
pub struct Header {
    pub model: Model,
    pub search_id: AccessibilityId,
}

impl Component for Header {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let compact = is_compact();
        let view = *model.view.read();
        let title = title_of(&model);

        let previous =
            move || icon_button(Icon::GoPrevious, move |_| model.step(-1)).tooltip("Previous");
        let next = move || icon_button(Icon::GoNext, move |_| model.step(1)).tooltip("Next");
        let today = move || action_button("Today", move |_| model.today());
        let views = move || {
            segmented(
                VIEWS
                    .iter()
                    .map(|(key, label, _)| (key.to_string(), label.to_string()))
                    .collect(),
                view_key(view),
                move |key: String| {
                    if let Some((_, _, kind)) =
                        VIEWS.iter().find(|(candidate, _, _)| *candidate == key)
                    {
                        model.show(*kind);
                    }
                },
            )
        };

        let mut bar = HeaderBar::new(title).title_start(true);
        if compact {
            bar = bar
                .leading(
                    icon_button(Icon::SidebarShow, move |_| {
                        window.open(
                            popup(
                                "Calendars",
                                Sidebar {
                                    model,
                                    embedded: true,
                                },
                            )
                            .width(Dim::PopupNarrow),
                        );
                    })
                    .tooltip("Calendars"),
                )
                .trailing(
                    icon_button(Icon::Search, move |_| {
                        let mut open = model.search_open;
                        let next = !*open.peek();
                        open.set(next);
                    })
                    .tooltip("Search"),
                );
        } else {
            bar = bar
                .leading(hstack(S1).child(today()).child(previous()).child(next()))
                .trailing(views())
                .trailing(
                    search_field(model.query, move |_: String| {})
                        .placeholder("Search")
                        .width(Dim::FieldMin)
                        .a11y_id(self.search_id),
                );
        }
        bar = bar
            .trailing(
                icon_button(Icon::ListAdd, move |_| quick::open(model, window, overlays))
                    .tooltip("New event")
                    .filled(),
            )
            .trailing(
                menu_button("Main menu", main_menu(model, window, overlays, tokens))
                    .icon(Icon::OpenMenu),
            )
            .trailing(WindowControls);

        let mut column = rect().tw(&tokens, &[w_full(), flex_col()]).child(bar);
        if compact {
            // A phone has no room for the views and the arrows in the bar:
            // they are a row of their own under it.
            column = column.child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            gap(S1),
                            px(S2),
                            py(S1),
                            bg(Paint::Surface),
                        ],
                    )
                    .child(previous())
                    .child(next())
                    .child(rect().tw(&tokens, &[w_flex()]).child(views()))
                    .child(today()),
            );
        }
        column
    }
}

/// The menu: what to do, how it looks, and About.
fn main_menu(model: Model, window: Window, overlays: Overlays, tokens: Tokens) -> Vec<MenuItem> {
    let prefs = model.prefs.read().clone();
    let first = prefs.first_day();
    let weekday_items: Vec<MenuItem> = [
        (None, "From the language"),
        (Some(0u8), "Monday"),
        (Some(5u8), "Saturday"),
        (Some(6u8), "Sunday"),
    ]
    .into_iter()
    .map(|(choice, label)| {
        menu_item(label, move |_| {
            model.set_prefs(|prefs| prefs.first_weekday = choice)
        })
        .checked(prefs.first_weekday == choice)
    })
    .collect();
    let _ = first;
    let hour_items: Vec<MenuItem> = [
        (HourFormat::Auto, "From the language"),
        (HourFormat::H12, "12-hour"),
        (HourFormat::H24, "24-hour"),
    ]
    .into_iter()
    .map(|(choice, label)| {
        menu_item(label, move |_| {
            model.set_prefs(|prefs| prefs.hour_format = choice)
        })
        .checked(prefs.hour_format == choice)
    })
    .collect();
    let data = model.data();
    let calendar_items: Vec<MenuItem> = data
        .calendars
        .iter()
        .filter(|calendar| !calendar.read_only)
        .map(|calendar| {
            let id = calendar.id.clone();
            let chosen = model.default_calendar().as_deref() == Some(calendar.id.as_str());
            menu_item(calendar.name.clone(), move |_| {
                let id = id.clone();
                model.set_prefs(move |prefs| prefs.default_calendar = Some(id))
            })
            .checked(chosen)
        })
        .collect();
    let reminder_items: Vec<MenuItem> = [
        (None, "None"),
        (Some(0i64), "At the time of the event"),
        (Some(5), "5 minutes before"),
        (Some(10), "10 minutes before"),
        (Some(15), "15 minutes before"),
        (Some(30), "30 minutes before"),
        (Some(60), "1 hour before"),
        (Some(1440), "1 day before"),
    ]
    .into_iter()
    .map(|(choice, label)| {
        menu_item(label, move |_| {
            model.set_prefs(|prefs| prefs.default_reminder = choice)
        })
        .checked(prefs.default_reminder == choice)
    })
    .collect();

    let mut items = vec![
        menu_item("New event", move |_| quick::open(model, window, overlays)).shortcut("N"),
        menu_item("New calendar\u{2026}", move |_| {
            calendars::add(model, overlays, &tokens)
        }),
        menu_item("Import events\u{2026}", move |_| {
            calendars::import(model, window, overlays)
        }),
        menu_separator(),
        menu_item("Week numbers", move |_| {
            model.set_prefs(|prefs| prefs.week_numbers = !prefs.week_numbers)
        })
        .checked(prefs.week_numbers),
        submenu("First day of the week", weekday_items),
        submenu("Time format", hour_items),
        submenu("New events go in", calendar_items),
        submenu("New events remind", reminder_items),
        menu_separator(),
        menu_item("Keyboard shortcuts", move |_| {
            overlays.show_dialog(dialog("Keyboard shortcuts", SHORTCUTS).cancel("Close"));
        }),
    ];
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}

/// What the shortcuts dialog says.
const SHORTCUTS: &str = "\
T  Go to today\n\
N  New event\n\
1 to 5  Day, Week, Month, Year, Agenda\n\
Left, Right  Previous, next\n\
Up, Down  A week back, ahead (Month)\n\
Page Up, Page Down  A month back, ahead\n\
Ctrl+F or /  Search\n\
Escape  Clear the search\n\
Delete  Delete the chosen event\n\
Ctrl+Shift+?  About Calendar\n\
\n\
In a week or a day, drag over empty time to make an event, drag an event to\n\
move it, and drag its top or bottom edge to change when it starts or ends.\n\
Press a day twice to make an event on it.";
