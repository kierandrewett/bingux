//! The tab strip: flat labels with the selected one underlined in the accent,
//! as Chrome's panel tabs and an app's view tabs. Only the selected tab is in
//! the tab order. Left and Right arrows move the selection and focus to the
//! neighbouring tab, and Home and End go to the first and last.
//!
//! A narrow container scrolls the strip sideways with [`TabStrip::scroll`]
//! rather than clipping its last tabs.

use std::cell::RefCell;
use std::rc::Rc;

use freya::prelude::*;

use super::press::use_press;
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{BODY, Ink, TextStyle};

/// A tab's label: body text one step smaller, as Chrome's panel tabs.
const TAB: TextStyle = TextStyle {
    size: TextSize::Sm,
    ..BODY
};

/// A row of tabs. `options` pairs each tab's key with its label, in strip
/// order; `on_select` is called with the key of the tab the user picked.
#[derive(PartialEq)]
pub struct TabStrip<K: Clone + PartialEq + 'static> {
    options: Vec<(K, String)>,
    selected: K,
    on_select: EventHandler<K>,
    scroll: bool,
}

/// A tab strip of `options`, with `selected` underlined.
pub fn tab_strip<K: Clone + PartialEq + 'static>(
    options: Vec<(K, String)>,
    selected: K,
    on_select: impl Into<EventHandler<K>>,
) -> TabStrip<K> {
    TabStrip {
        options,
        selected,
        on_select: on_select.into(),
        scroll: false,
    }
}

impl<K: Clone + PartialEq + 'static> TabStrip<K> {
    /// Scroll the strip sideways when its container is too narrow for it.
    pub fn scroll(mut self) -> Self {
        self.scroll = true;
        self
    }
}

impl<K: Clone + PartialEq + 'static> Component for TabStrip<K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        // One accessibility id per tab, kept for the life of the strip, so the
        // arrow keys can move focus to a neighbouring tab.
        let ids = use_hook(|| Rc::new(RefCell::new(Vec::<AccessibilityId>::new())));
        let ids: Vec<AccessibilityId> = {
            let mut ids = ids.borrow_mut();
            while ids.len() < self.options.len() {
                ids.push(AccessibilityId::new_unique());
            }
            ids[..self.options.len()].to_vec()
        };
        let options = self.options.clone();
        let selected = self.selected.clone();
        let on_select = self.on_select.clone();
        let key_on_select = on_select.clone();
        let key_ids = ids.clone();
        let strip = rect()
            .component("TabStrip")
            .tw(&tokens, &[h(Dim::Box7), flex_row()])
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
                    key_on_select.call(options[next].0.clone());
                }
                key_ids[next].request_focus();
            })
            .children(self.options.iter().enumerate().map(|(index, (key, text))| {
                Element::from(TabLabel {
                    key: key.clone(),
                    text: text.clone(),
                    selected: *key == self.selected,
                    a11y_id: ids[index],
                    on_select: self.on_select.clone(),
                })
            }));
        if self.scroll {
            rect()
                .tw(&tokens, &[w_full(), h(Dim::Box7)])
                .child(
                    scroll_x()
                        .width(Size::fill())
                        .height(Size::fill())
                        .child(strip),
                )
                .into_element()
        } else {
            strip.into_element()
        }
    }
}

#[derive(PartialEq)]
struct TabLabel<K: Clone + PartialEq + 'static> {
    key: K,
    text: String,
    selected: bool,
    a11y_id: AccessibilityId,
    on_select: EventHandler<K>,
}

impl<K: Clone + PartialEq + 'static> Component for TabLabel<K> {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(true);
        let press = use_press();
        let key = self.key.clone();
        let selected = self.selected;
        let on_select = self.on_select.clone();
        // Only the selected tab is brighter; hover brightens the label without
        // drawing a second underline.
        let ink = if selected || hovering() {
            Ink::Text
        } else {
            Ink::Muted
        };
        let underline = if selected {
            Paint::Accent
        } else {
            Paint::Transparent
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && selected)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Tab)
            .a11y_builder(move |builder| builder.set_selected(selected))
            .tw(&tokens, &[h_full(), px(S3), center()])
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
                TAB.in_ink(ink)
                    .label(&tokens, self.text.clone())
                    .max_lines(1),
            )
            .child(
                rect()
                    .position(Position::new_absolute().bottom(0.0).left(0.0))
                    .tw(&tokens, &[w_full(), h(Dim::Indicator), bg(underline)]),
            )
            .focusable(focus(), Radius::Inner)
    }
}
