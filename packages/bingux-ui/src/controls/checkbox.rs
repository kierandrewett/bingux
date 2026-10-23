//! Checkbox and radio controls (libadwaita `GtkCheckButton`, SwiftUI
//! `Toggle(.checkbox)` and `Picker(.radioGroup)`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};
use crate::typography::BODY;

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
        // The accent fades in over the control colour. The fade is an
        // opacity, so the transition survives without a raw colour.
        let fill = use_animation_transition(if filled { 1.0_f32 } else { 0.0 }, move |from, to| {
            AnimNum::new(from, to)
                .time(motion)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let fill = fill.get().value();
        // The border follows the state: keyboard focus ring, the accent
        // when filled (the same colour as the fill), otherwise the outline.
        let edge = if focus() == Focus::Keyboard {
            focus_ring(Focus::Keyboard)
        } else if filled {
            border(Line::Ring(Paint::Accent))
        } else {
            border(Line::Outline)
        };
        let toggle = move || on_toggle.call(state == CheckState::Off);
        let glyph = match state {
            CheckState::Off => None,
            CheckState::On => Some(Element::from(icons::icon(
                Icon::Check,
                IconSize::Mark,
                Paint::OnAccent.color(&tokens),
            ))),
            CheckState::Mixed => Some(Element::from(rect().tw(
                &tokens,
                &[
                    w(Dim::Dot),
                    h(Dim::DashHeight),
                    rounded(Radius::Mark),
                    bg(Paint::OnAccent),
                ],
            ))),
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
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
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
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Check),
                            h(Dim::Check),
                            rounded(Radius::Mark),
                            center(),
                            surface(Paint::Control, hovering(), pressing(), false),
                            edge,
                        ],
                    )
                    .child(
                        rect()
                            .expanded()
                            .tw(
                                &tokens,
                                &[rounded(Radius::Mark), center(), bg(Paint::Accent)],
                            )
                            .opacity(fill)
                            .maybe_child(glyph),
                    ),
            )
            .maybe_child(
                self.text
                    .clone()
                    .map(|text| BODY.label(&tokens, text).max_lines(1)),
            )
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
        // Layout only: the group paints nothing, so no tokens are needed.
        let mut container = rect().tw_layout(&[gap(if self.horizontal { S4 } else { S2 })]);
        if self.horizontal {
            container = container.tw_layout(&[flex_row()]);
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
        // The dot grows from nothing: a runtime size, so a raw `Size::px`.
        let dot = use_animation_transition(
            if selected { Dim::Dot.px() } else { 0.0 },
            move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            },
        );
        let dot = dot.get().value();
        let edge = if focus() == Focus::Keyboard {
            focus_ring(Focus::Keyboard)
        } else if selected {
            border(Line::Ring(Paint::Accent))
        } else {
            border(Line::Outline)
        };
        let fill = if selected {
            bg(Paint::Accent)
        } else {
            surface(Paint::Control, hovering(), pressing(), false)
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
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
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
                    .tw(
                        &tokens,
                        &[
                            w(Dim::Check),
                            h(Dim::Check),
                            rounded(Radius::Pill),
                            center(),
                            fill,
                            edge,
                        ],
                    )
                    .child(
                        rect()
                            .width(Size::px(dot))
                            .height(Size::px(dot))
                            .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::OnAccent)]),
                    ),
            )
            .child(BODY.label(&tokens, self.text.clone()).max_lines(1))
    }
}
