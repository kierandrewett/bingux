//! The sidebar: a small month to move about with, and the calendars, each of
//! which can be shown or hidden.

use bingux_ui::calendar::palette::event_colours;
use bingux_ui::calendar::{Date, month_grid};
use bingux_ui::component::ComponentName;
use bingux_ui::prelude::MenuItem;
use bingux_ui::prelude::*;
use bingux_ui::typography::{BODY, Ink};
use freya::prelude::*;

use crate::calendars;
use crate::convert;
use crate::model::Model;
use crate::views::shown;

#[derive(PartialEq)]
pub struct Sidebar {
    pub model: Model,
    /// In a popup, as on a phone: as wide as it is given, with no fill.
    pub embedded: bool,
}

impl Component for Sidebar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let model = self.model;
        let clock = *model.clock.read();
        let anchor = *model.anchor.read();
        let first_weekday = model.first_weekday();
        let week_numbers = model.prefs.read().week_numbers;
        let start = anchor.first_of_month().week_start(first_weekday);
        let marked: Vec<Date> = {
            let mut days: Vec<Date> = Vec::new();
            for occurrence in shown(model.occurrences(start, start.add_days(42))) {
                let (from, to) = occurrence.days();
                let (from, to) = (convert::date_of(from), convert::date_of(to));
                let mut day = from.max(start);
                while day <= to.min(start.add_days(41)) {
                    days.push(day);
                    day = day.add_days(1);
                }
            }
            days.sort();
            days.dedup();
            days
        };
        let data = model.data();
        let hidden = model.hidden();
        let rows = data.calendars.iter().enumerate().map(|(index, calendar)| {
            let count = data
                .items
                .iter()
                .filter(|item| item.calendar == calendar.id)
                .count();
            CalendarRow {
                model,
                id: calendar.id.clone(),
                name: calendar.name.clone(),
                colour: convert::colour_of(Some(calendar), index, &tokens),
                visible: !hidden.contains(&calendar.id),
                read_only: calendar.read_only,
                count,
            }
            .into_element()
        });

        let frame = if self.embedded {
            rect().tw(&tokens, &[w_full(), flex_col(), gap(S3), p(S3)])
        } else {
            rect().tw(
                &tokens,
                &[
                    w(Dim::Sidebar),
                    h_full(),
                    flex_col(),
                    gap(S3),
                    p(S3),
                    bg(Paint::Sidebar),
                ],
            )
        };
        frame
            .component("Sidebar")
            .child(
                month_grid(anchor, clock.today, move |day: Date| model.go(day))
                    .selected(anchor)
                    .marked(marked)
                    .first_weekday(first_weekday)
                    .week_numbers(week_numbers)
                    .on_page(move |delta: i32| model.go(anchor.add_months(delta))),
            )
            .child(divider(&tokens))
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            w_full(),
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            px(S2),
                        ],
                    )
                    .child(
                        rect()
                            .tw(&tokens, &[w_flex()])
                            .child(section_label("Calendars")),
                    )
                    .child(
                        icon_button(Icon::ListAdd, move |_| {
                            calendars::add(model, overlays, &tokens);
                        })
                        .tooltip("Add calendar"),
                    ),
            )
            .child(
                rect()
                    .tw(&tokens, &[w_full(), flex_col(), gap(S0_5)])
                    .children(rows),
            )
            .child(
                action_button("Import Events", move |_| {
                    calendars::import(model, window, overlays)
                })
                .icon(Icon::DocumentOpen)
                .flat(),
            )
    }
}

/// One calendar: a coloured mark that is filled while its events show.
#[derive(PartialEq)]
struct CalendarRow {
    model: Model,
    id: String,
    name: String,
    colour: Color,
    visible: bool,
    read_only: bool,
    count: usize,
}

impl Component for CalendarRow {
    fn render_key(&self) -> DiffKey {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&self.id, &mut hasher);
        DiffKey::U64(std::hash::Hasher::finish(&hasher))
    }

    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let window = use_window();
        let overlays = use_overlays();
        let mut hovering = use_state(|| false);
        let pressing = use_state(|| false);
        let model = self.model;
        let id = self.id.clone();
        let toggle_id = self.id.clone();

        let mut menu = vec![
            menu_item("Rename\u{2026}", {
                let (id, name) = (id.clone(), self.name.clone());
                move |_| calendars::rename(model, overlays, id.clone(), name.clone())
            }),
            submenu(
                "Colour",
                calendars::colours()
                    .into_iter()
                    .map(|(hex, label)| {
                        let id = id.clone();
                        menu_item(label, move |_| {
                            calendars::recolour(model, overlays, id.clone(), hex.clone())
                        })
                    })
                    .collect(),
            ),
            menu_item("Export\u{2026}", {
                let (id, name) = (id.clone(), self.name.clone());
                move |_| calendars::export(model, window, overlays, id.clone(), name.clone())
            }),
            menu_separator(),
        ];
        menu.push(
            menu_item("Delete\u{2026}", {
                let (id, name, count) = (id.clone(), self.name.clone(), self.count);
                move |_| calendars::remove(model, overlays, id.clone(), name.clone(), count)
            })
            .destructive(),
        );

        let swatch = rect().tw(
            &tokens,
            &[
                w(Dim::Box5),
                h(Dim::Box5),
                rounded(Radius::Pill),
                center(),
                fill(self.colour),
            ],
        );
        let swatch = if self.visible {
            swatch.child(icon(
                Icon::Check,
                IconSize::Control,
                event_colours(&tokens, self.colour).on_solid,
            ))
        } else {
            swatch
                .tw(&tokens, &[border(Line::Edge(Paint::Outline))])
                .child(rect().tw(
                    &tokens,
                    &[
                        w(Dim::Box5),
                        h(Dim::Box5),
                        rounded(Radius::Pill),
                        bg(Paint::Bg),
                    ],
                ))
        };
        let content = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    gap(S2),
                    px(S2),
                    h(Dim::NavItem),
                    surface(Paint::Transparent, hovering(), pressing(), false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set_if_modified(false))
            .child(swatch)
            .child(
                rect().tw(&tokens, &[w_flex()]).child(
                    BODY.in_ink(if self.visible { Ink::Text } else { Ink::Muted })
                        .label(&tokens, self.name.clone())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                ),
            );
        let label = format!(
            "{}, {}{}",
            self.name,
            if self.visible { "shown" } else { "hidden" },
            if self.read_only { ", read only" } else { "" }
        );
        focus_stop_menu_pressed(
            content,
            label,
            move || model.toggle_calendar(&toggle_id),
            menu,
            pressing,
        )
    }
}

/// A row that is a tab stop with a context menu, and shows when it is pressed.
fn focus_stop_menu_pressed(
    content: Rect,
    label: String,
    on_activate: impl Fn() + 'static,
    menu: Vec<MenuItem>,
    pressing: State<bool>,
) -> Rect {
    bingux_ui::controls::focus_stop_pressed(
        use_a11y(),
        content,
        label,
        AccessibilityRole::CheckBox,
        true,
        Radius::Control,
        move |_: ()| on_activate(),
        menu,
        bingux_ui::controls::StopRing::Outside,
        Some(pressing),
    )
}
