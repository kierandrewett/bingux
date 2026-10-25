//! The shell's segmented control (`SegmentedControl.qml`), also used as the
//! tab rail of a popup. Only the selected segment is in the tab order. Left
//! and Right arrows move the selection and focus to the neighbouring segment,
//! and Home and End go to the first and last.
//!
//! Only the selected segment is raised. An unselected segment has no fill of
//! its own, even on hover: its text brightens instead, so a hovered segment
//! never looks like a second selection.

use super::press::use_press;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use freya::prelude::*;

use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{BODY, Ink};

/// The shell's segmented control: a quiet `control` rail where only the
/// selected segment is raised.
#[derive(PartialEq)]
pub struct Segmented {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
    fill: bool,
}

pub fn segmented(
    options: Vec<(String, String)>,
    selected: &str,
    on_select: impl Into<EventHandler<String>>,
) -> Segmented {
    Segmented {
        options,
        selected: selected.to_string(),
        on_select: on_select.into(),
        fill: false,
    }
}

impl Segmented {
    /// Span the full width with equal segments, as a popup's tab rail does.
    /// A filled control stays a rail on a phone instead of turning into a
    /// dropdown, because it switches pages rather than picking a value.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }
}

impl Component for Segmented {
    fn render(&self) -> impl IntoElement {
        // On a phone a row has no room for a track of segments beside its
        // title, so the same choice is a value picker, as the dropdown is.
        // The density is read here, so a change swaps the child below.
        if is_compact() && !self.fill {
            return crate::controls::dropdown::dropdown(
                self.options.clone(),
                self.selected.clone(),
                self.on_select.clone(),
            )
            .into_element();
        }
        SegmentRail {
            options: self.options.clone(),
            selected: self.selected.clone(),
            on_select: self.on_select.clone(),
            fill: self.fill,
        }
        .into_element()
    }
}

impl Component for SegmentRail {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let enabled = use_enabled(true);
        let options = self.options.clone();
        let selected = self.selected.clone();
        let on_select = self.on_select.clone();
        // One accessibility id per key, kept for the life of the control, so
        // the arrow keys can move focus to a neighbouring segment.
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
        Element::from(
            rect()
                .tw(
                    &tokens,
                    &[
                        h(Dim::Control),
                        flex_row(),
                        items(Align::Center),
                        p(S1),
                        gap(S1),
                        rounded(Radius::Control),
                        bg(Paint::Control),
                    ],
                )
                .maybe(self.fill, |element| {
                    element.tw(&tokens, &[w_full(), grow_children()])
                })
                .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
                .on_key_down(move |event: Event<KeyboardEventData>| {
                    if !enabled {
                        return;
                    }
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
                .children(self.options.iter().enumerate().map(|(index, (key, text))| {
                    Element::from(Segment {
                        key: key.clone(),
                        text: text.clone(),
                        selected: *key == self.selected,
                        fill: self.fill,
                        on_select: self.on_select.clone(),
                        a11y_id: ids[index],
                    })
                })),
        )
    }
}

/// The segment rail. [`Segmented`] chooses between it and the dropdown, so a
/// density change swaps one child for the other and no component changes its
/// hooks while it is mounted.
#[derive(PartialEq)]
struct SegmentRail {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
    fill: bool,
}

#[derive(PartialEq)]
struct Segment {
    key: String,
    text: String,
    selected: bool,
    /// Share the rail's width equally with the other segments.
    fill: bool,
    on_select: EventHandler<String>,
    a11y_id: AccessibilityId,
}

impl Component for Segment {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let press = use_press();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && self.selected)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .tw(
                &tokens,
                &[
                    h_full(),
                    px(S2),
                    center(),
                    rounded(Radius::Inner),
                    bg(if self.selected {
                        Paint::Raised
                    } else {
                        Paint::Transparent
                    }),
                ],
            )
            .maybe(self.fill, |element| element.tw(&tokens, &[w_flex()]))
            .maybe(self.selected, |element| {
                element.tw(&tokens, &[shadow(Elevation::Knob)])
            })
            .maybe(enabled, |element| {
                element
                    .on_pointer_enter(move |_| hovering.set(true))
                    .on_pointer_leave(move |_| hovering.set(false))
                    .on_pointer_down(move |event: Event<PointerEventData>| {
                        event.stop_propagation();
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_press(move |event: Event<PressEventData>| {
                        event.stop_propagation();
                        if !press.tap(&event) {
                            return;
                        }
                        on_select.call(key.clone());
                        a11y_id.request_focus();
                    })
            })
            .child(
                // The selected segment is raised; a hovered one only brightens
                // its text, so it never reads as a second selection.
                BODY.in_ink(if self.selected || hovering() {
                    Ink::Text
                } else {
                    Ink::Muted
                })
                .label(&tokens, self.text.clone())
                .max_lines(1),
            )
            .focusable(focus(), Radius::Inner)
            .maybe(self.fill, |wrapper| wrapper.tw(&tokens, &[w_flex()]))
            .tw(&tokens, &[h_full()])
    }
}
