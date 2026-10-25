//! Checkbox controls (libadwaita `GtkCheckButton`, SwiftUI
//! `Toggle(.checkbox)`). Radio buttons live in `radio.rs`.

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::icons::{self, Icon};
use crate::motion::Focusable;
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
        // The border follows the state: the accent when filled (the same colour
        // as the fill), otherwise the outline. Keyboard focus draws its own ring.
        let edge = if filled {
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
                    )
                    .focusable(focus(), Radius::Mark),
            )
            .maybe_child(
                self.text
                    .clone()
                    .map(|text| BODY.label(&tokens, text).max_lines(1)),
            )
    }
}
