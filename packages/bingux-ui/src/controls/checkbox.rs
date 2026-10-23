//! Checkbox and radio controls (libadwaita `GtkCheckButton`, SwiftUI
//! `Toggle(.checkbox)` and `Picker(.radioGroup)`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::scale::*;
use crate::surface::{is_activation_key, surface_color, use_enabled, use_tokens};

/// Side of the 18 px check and radio boxes.
const BOX_SIZE: f32 = 18.0;
const DASH_WIDTH: f32 = 8.0;
const DASH_HEIGHT: f32 = 2.0;
const DOT_SIZE: f32 = 8.0;

/// Tri-state checkbox value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Off,
    On,
    /// A group with mixed children; drawn as a dash.
    Mixed,
}

impl From<bool> for CheckState {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// An 18 px check box with an optional label. Checked and mixed states
/// fill with the accent colour; the border is the outline colour.
#[derive(PartialEq)]
pub struct Checkbox {
    state: CheckState,
    text: Option<String>,
    enabled: bool,
    on_toggle: EventHandler<bool>,
}

/// A checkbox; `on_toggle` receives the next boolean value (a mixed box
/// reports `true`).
pub fn checkbox(
    state: impl Into<CheckState>,
    on_toggle: impl Into<EventHandler<bool>>,
) -> Checkbox {
    Checkbox {
        state: state.into(),
        text: None,
        enabled: true,
        on_toggle: on_toggle.into(),
    }
}

impl Checkbox {
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for Checkbox {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let state = self.state;
        let enabled = use_enabled(self.enabled);
        let on_toggle = self.on_toggle.clone();
        let filled = state != CheckState::Off;
        let motion = tokens.motion(100);
        let fill = use_animation_transition(
            if filled {
                tokens.accent
            } else {
                tokens.elevated
            },
            move |from, to| {
                AnimColor::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            },
        );
        let fill = fill.get().value();
        let border_color = if focus() == Focus::Keyboard {
            tokens.text
        } else if filled {
            tokens.accent
        } else if hovering() {
            tokens.text.with_af32(0.6)
        } else {
            tokens.outline
        };
        let overlay = if enabled && pressing() {
            Color::BLACK.with_af32(0.16)
        } else if enabled && hovering() {
            Color::WHITE.with_af32(0.08)
        } else {
            Color::TRANSPARENT
        };
        let toggle = move || on_toggle.call(state == CheckState::Off);
        let glyph = match state {
            CheckState::Off => None,
            CheckState::On => Some(Element::from(icons::icon(
                Icon::Check,
                ICON_SM,
                tokens.accent_foreground,
            ))),
            CheckState::Mixed => Some(Element::from(
                rect()
                    .width(Size::px(DASH_WIDTH))
                    .height(Size::px(DASH_HEIGHT))
                    .corner_radius(1.0)
                    .background(tokens.accent_foreground),
            )),
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::CheckBox)
            .a11y_builder(move |builder| {
                builder.set_toggled(match state {
                    CheckState::Off => Toggled::False,
                    CheckState::On => Toggled::True,
                    CheckState::Mixed => Toggled::Mixed,
                })
            })
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .maybe(!enabled, |element| element.opacity(0.45))
            .maybe(enabled, |element| {
                let toggle_press = toggle.clone();
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
                        toggle_press();
                        a11y_id.request_focus();
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            toggle();
                        }
                    })
            })
            .child(
                rect()
                    .width(Size::px(BOX_SIZE))
                    .height(Size::px(BOX_SIZE))
                    .corner_radius(RADIUS_SM)
                    .background(fill)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(border_color),
                    )
                    .child(
                        rect()
                            .expanded()
                            .corner_radius(RADIUS_SM)
                            .background(overlay)
                            .center()
                            .maybe_child(glyph),
                    ),
            )
            .maybe_child(self.text.clone().map(|text| {
                label()
                    .text(text)
                    .font_size(TEXT_MD)
                    .color(tokens.text)
                    .max_lines(1)
            }))
    }
}

/// A set of mutually exclusive 18 px radio buttons, one per option, laid
/// out vertically by default. Only the selected option is in the tab order.
/// Arrow keys move the selection and focus to the next or previous option.
#[derive(PartialEq)]
pub struct RadioGroup {
    options: Vec<(String, String)>,
    selected: String,
    horizontal: bool,
    enabled: bool,
    on_select: EventHandler<String>,
}

/// `options` are `(key, label)` pairs; `on_select` receives the chosen key.
pub fn radio_group(
    options: Vec<(String, String)>,
    selected: impl Into<String>,
    on_select: impl Into<EventHandler<String>>,
) -> RadioGroup {
    RadioGroup {
        options,
        selected: selected.into(),
        horizontal: false,
        enabled: true,
        on_select: on_select.into(),
    }
}

impl RadioGroup {
    pub fn horizontal(mut self) -> Self {
        self.horizontal = true;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for RadioGroup {
    fn render(&self) -> impl IntoElement {
        let enabled = use_enabled(self.enabled);
        let on_select = self.on_select.clone();
        let key_select = on_select.clone();
        let options = self.options.clone();
        let selected = self.selected.clone();
        // One accessibility id per option, kept for the life of the group, so
        // the arrow keys can move focus to a neighbouring option.
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
        // The selected option is the tab stop; with no selection, the first is.
        let tab_stop = self
            .options
            .iter()
            .position(|(key, _)| *key == selected)
            .unwrap_or(0);
        let mut container = rect().spacing(if self.horizontal { S4 } else { S2 });
        if self.horizontal {
            container = container.direction(Direction::Horizontal);
        }
        container
            .a11y_role(AccessibilityRole::RadioGroup)
            .on_key_down(move |event: Event<KeyboardEventData>| {
                let count = options.len();
                if !enabled || count == 0 {
                    return;
                }
                // Arrows wrap around, as in a native radio group.
                let next = match &event.key {
                    Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => {
                        (tab_stop + count - 1) % count
                    }
                    Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => {
                        (tab_stop + 1) % count
                    }
                    _ => return,
                };
                event.stop_propagation();
                if next != tab_stop {
                    key_select.call(options[next].0.clone());
                }
                key_ids[next].request_focus();
            })
            .children(self.options.iter().enumerate().map(|(index, (key, text))| {
                let on_select = on_select.clone();
                let key_for_press = key.clone();
                Element::from(RadioButton {
                    text: text.clone(),
                    selected: *key == selected,
                    focusable: index == tab_stop,
                    enabled,
                    on_select: EventHandler::new(move |_: ()| {
                        on_select.call(key_for_press.clone())
                    }),
                    a11y_id: ids[index],
                })
            }))
    }
}

#[derive(PartialEq)]
struct RadioButton {
    text: String,
    selected: bool,
    /// The group's tab stop: the only option Tab reaches.
    focusable: bool,
    enabled: bool,
    on_select: EventHandler<()>,
    a11y_id: AccessibilityId,
}

impl Component for RadioButton {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = self.a11y_id;
        let focus = use_focus(a11y_id);
        let enabled = use_enabled(self.enabled);
        let selected = self.selected;
        let on_select = self.on_select.clone();
        let motion = tokens.motion(100);
        let dot =
            use_animation_transition(if selected { DOT_SIZE } else { 0.0 }, move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            });
        let dot = dot.get().value();
        let ring = if focus() == Focus::Keyboard {
            tokens.text
        } else if selected {
            tokens.accent
        } else if hovering() {
            tokens.text.with_af32(0.6)
        } else {
            tokens.outline
        };
        let fill = if selected {
            tokens.accent
        } else {
            surface_color(&tokens, tokens.elevated, hovering(), pressing(), false)
        };
        let select = move || {
            if !selected {
                on_select.call(());
            }
        };
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled && self.focusable)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::RadioButton)
            .a11y_builder(move |builder| builder.set_toggled(Toggled::from(selected)))
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .maybe(!enabled, |element| element.opacity(0.45))
            .maybe(enabled, |element| {
                let select_press = select.clone();
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
                        select_press();
                        a11y_id.request_focus();
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            select();
                        }
                    })
            })
            .child(
                rect()
                    .width(Size::px(BOX_SIZE))
                    .height(Size::px(BOX_SIZE))
                    .corner_radius(BOX_SIZE / 2.0)
                    .background(fill)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(ring),
                    )
                    .center()
                    .child(
                        rect()
                            .width(Size::px(dot))
                            .height(Size::px(dot))
                            .corner_radius(dot / 2.0)
                            .background(tokens.accent_foreground),
                    ),
            )
            .child(
                label()
                    .text(self.text.clone())
                    .font_size(TEXT_MD)
                    .color(tokens.text)
                    .max_lines(1),
            )
    }
}
