//! The places sidebar: Recent, Home, the XDG folders, devices and bookmarks.

use bingux_files::{Place, PlaceKind, Places};
use freya::prelude::*;

use super::directory::Where;
use super::glyphs::place_icon;
use crate::component::ComponentName;
use crate::controls::focus_stop;
use crate::focus::step_focus;
use crate::icons;
use crate::layout::divider;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::BODY;

/// A column of the system's places. The place that is exactly the current
/// location is selected. Each row is a Tab stop; Up and Down walk the rows.
///
/// Use it beside any file view. A second user besides the chooser is the
/// Files app's own sidebar.
#[derive(PartialEq)]
pub struct PlacesSidebar {
    places: Places,
    current: Where,
    recent: bool,
    on_open: EventHandler<Where>,
}

impl PlacesSidebar {
    /// `on_open` receives the place a person picks.
    pub fn new(places: Places, current: Where, on_open: impl Into<EventHandler<Where>>) -> Self {
        PlacesSidebar {
            places,
            current,
            recent: true,
            on_open: on_open.into(),
        }
    }

    /// Whether the Recent place is listed (default true). A chooser that
    /// saves has nothing to take from it.
    pub fn recent(mut self, recent: bool) -> Self {
        self.recent = recent;
        self
    }
}

impl Component for PlacesSidebar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let groups = [
            &self.places.standard,
            &self.places.devices,
            &self.places.bookmarks,
        ];
        let mut column = rect().tw(&tokens, &[flex_col(), w_full(), p(S2), gap(S0_5)]);
        let mut first = true;
        for places in groups {
            let rows: Vec<&Place> = places
                .iter()
                .filter(|place| self.recent || place.kind != PlaceKind::Recent)
                .collect();
            if rows.is_empty() {
                continue;
            }
            if !first {
                column = column.child(
                    rect()
                        .tw(&tokens, &[w_full(), py(S2), px(S2)])
                        .child(divider(&tokens)),
                );
            }
            first = false;
            for place in rows {
                let target = match &place.path {
                    Some(path) => Where::Folder(path.clone()),
                    None => Where::Recent,
                };
                column = column.child(Element::from(PlaceRow {
                    icon: place_icon(place.kind),
                    label: place.name.clone(),
                    selected: target == self.current,
                    on_press: {
                        let on_open = self.on_open.clone();
                        EventHandler::new(move |_: ()| on_open.call(target.clone()))
                    },
                }));
            }
        }
        rect()
            .component("PlacesSidebar")
            .tw(&tokens, &[w_full(), h_full()])
            .child(
                ScrollView::new()
                    .drag_scrolling(false)
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(column),
            )
    }
}

#[derive(PartialEq)]
struct PlaceRow {
    icon: crate::icons::Icon,
    label: String,
    selected: bool,
    on_press: EventHandler<()>,
}

impl Component for PlaceRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let ink = if self.selected {
            Paint::Text
        } else {
            Paint::Muted
        };
        let content = rect()
            .component("PlaceRow")
            .tw(&tokens, NAV_ITEM)
            .tw(
                &tokens,
                &[surface(
                    Paint::Transparent,
                    hovering(),
                    false,
                    self.selected,
                )],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| hovering.set(false))
            .a11y_builder({
                let selected = self.selected;
                move |builder| builder.set_selected(selected)
            })
            .child(rect().tw(&tokens, NAV_ICON).child(icons::icon(
                self.icon,
                IconSize::Row,
                ink.color(&tokens),
            )))
            .child(
                BODY.label(&tokens, self.label.clone())
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            );
        let on_press = self.on_press.clone();
        focus_stop(
            content,
            self.label.clone(),
            AccessibilityRole::Tab,
            true,
            Radius::Control,
            move |_: ()| on_press.call(()),
        )
        .tw(&tokens, &[w_full()])
        // The wrapper hears the keys of the focused row inside it.
        .on_key_down(|event: Event<KeyboardEventData>| match event.key {
            Key::Named(NamedKey::ArrowDown) => {
                event.stop_propagation();
                step_focus(false);
            }
            Key::Named(NamedKey::ArrowUp) => {
                event.stop_propagation();
                step_focus(true);
            }
            _ => {}
        })
    }
}
