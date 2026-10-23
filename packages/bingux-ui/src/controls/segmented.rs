//! The shell's segmented control (`SegmentedControl.qml`). Only the selected
//! segment is in the tab order. Left and Right arrows move the selection and
//! focus to the neighbouring segment, and Home and End go to the first and last.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use freya::prelude::*;

use crate::scale::*;
use crate::surface::{focus_border, over, surface_color, use_enabled, use_tokens};

/// The shell's segmented control: a quiet `control` rail where only the
/// selected segment is raised.
#[derive(PartialEq)]
pub struct Segmented {
    options: Vec<(String, String)>,
    selected: String,
    on_select: EventHandler<String>,
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
    }
}

impl Component for Segmented {
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
        rect()
            .height(Size::px(control_height()))
            .maybe(!enabled, |element| element.opacity(0.45))
            .corner_radius(RADIUS)
            .background(tokens.control)
            .padding(S1)
            .spacing(S1)
            .direction(Direction::Horizontal)
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
                    on_select: self.on_select.clone(),
                    a11y_id: ids[index],
                })
            }))
    }
}

#[derive(PartialEq)]
struct Segment {
    key: String,
    text: String,
    selected: bool,
    on_select: EventHandler<String>,
    a11y_id: AccessibilityId,
}

impl Component for Segment {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let key = self.key.clone();
        let on_select = self.on_select.clone();
        let base = if self.selected {
            over(tokens.pressed, tokens.control)
        } else {
            Color::TRANSPARENT
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && self.selected)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(|builder| builder.set_selected(self.selected))
            .height(Size::fill())
            .padding((0.0, S2))
            .center()
            .corner_radius(RADIUS - S1)
            .background(surface_color(&tokens, base, hovering(), pressing(), false))
            .border(focus_border(&tokens, focus()))
            .maybe(enabled, |element| {
                element
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
            })
            .child(
                label()
                    .text(self.text.clone())
                    .color(if self.selected {
                        tokens.text
                    } else {
                        tokens.muted
                    })
                    .font_size(TEXT_MD)
                    .max_lines(1),
            )
    }
}
