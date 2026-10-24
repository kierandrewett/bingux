//! Tabs: a row of equal tabs on a quiet rail where the selected tab is raised,
//! as the segmented control and GNOME's view switcher. Unlike the segmented
//! control it fills its width and stays a rail on a phone, so a popup can
//! use it to switch between pages of one form. The row uses a roving tab stop: only the selected
//! tab takes focus from Tab and Shift+Tab. Left and Right arrows move the
//! selection and focus to the neighbouring tab, and Home and End go to the
//! first and last tab.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use freya::prelude::*;

use crate::motion::focus_ring;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{BODY, Ink};

/// A row of tabs. `options` holds `(key, label)` pairs.
#[derive(PartialEq)]
pub struct Tabs {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
}

/// Tabs titled by `options` with `selected` underlined. `on_select` receives the
/// key of the tab the user picks, by pointer or by arrow key.
pub fn tabs(
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Tabs {
    Tabs {
        options,
        selected: selected.to_string(),
        on_select: on_select.into(),
    }
}

impl Component for Tabs {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let options = self.options.clone();
        let selected = self.selected.clone();
        let on_select = self.on_select.clone();
        // One accessibility id per tab key, kept for the life of the row, so
        // the arrow keys can move focus to a neighbour that is not in the tab
        // order.
        let ids = use_hook(|| Rc::new(RefCell::new(HashMap::<String, AccessibilityId>::new())));
        let ids: Vec<AccessibilityId> = {
            let mut map = ids.borrow_mut();
            self.options
                .iter()
                .map(|(key, _)| {
                    *map.entry(key.clone())
                        .or_insert_with(AccessibilityId::new_unique)
                })
                .collect()
        };
        let key_ids = ids.clone();
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::Control),
                    flex_row(),
                    grow_children(),
                    items(Align::Center),
                    p(S1),
                    gap(S1),
                    rounded(Radius::Control),
                    bg(Paint::Control),
                ],
            )
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let Some(index) = options.iter().position(|(key, _)| *key == selected) else {
                    return;
                };
                let last = options.len().saturating_sub(1);
                let next = match &event.key {
                    Key::Named(NamedKey::ArrowLeft) => index.saturating_sub(1),
                    Key::Named(NamedKey::ArrowRight) => (index + 1).min(last),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => last,
                    _ => return,
                };
                event.stop_propagation();
                if next != index {
                    on_select.call(options[next].0.clone());
                }
                key_ids[next].request_focus();
            })
            .children(
                self.options
                    .iter()
                    .enumerate()
                    .map(|(index, (key, text))| Tab {
                        key: key.clone(),
                        text: text.clone(),
                        selected: *key == self.selected,
                        on_select: self.on_select.clone(),
                        a11y_id: ids[index],
                    }),
            )
    }
}

#[derive(PartialEq)]
struct Tab {
    key: String,
    text: String,
    selected: bool,
    on_select: EventHandler<String>,
    a11y_id: AccessibilityId,
}

impl Component for Tab {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(self.selected)
            .a11y_enabled(true)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .tw(
                &tokens,
                &[
                    w_flex(),
                    h_full(),
                    center(),
                    rounded(Radius::Inner),
                    surface(
                        if self.selected {
                            Paint::Card
                        } else {
                            Paint::Transparent
                        },
                        hovering(),
                        pressing(),
                        false,
                    ),
                ],
            )
            .maybe(self.selected, |element| {
                element.tw(&tokens, &[shadow(Elevation::Knob)])
            })
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .on_pointer_down(move |event: Event<PointerEventData>| {
                event.stop_propagation();
                pressing.set(true);
            })
            .on_global_pointer_up(move |_| pressing.set_if_modified(false))
            .on_press(move |event: Event<PressEventData>| {
                event.stop_propagation();
                on_select.call(key.clone());
                a11y_id.request_focus();
            })
            .child(
                BODY.in_ink(if self.selected { Ink::Text } else { Ink::Muted })
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .child(focus_ring(focus(), Radius::Inner))
    }
}
