//! The switch: a 44 x 24 track, accent when on, with a 20 px white thumb.

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::scale::*;
use crate::surface::{is_activation_key, use_tokens};

const SWITCH_WIDTH: f32 = 44.0;
const SWITCH_HEIGHT: f32 = 24.0;
const THUMB_INSET: f32 = 2.0;

/// A 44 x 24 track that is the accent when on and a neutral grey when off.
/// The 20 px thumb is white in both states.
#[derive(PartialEq)]
pub struct Switch {
    on: bool,
    enabled: bool,
    on_toggle: EventHandler<bool>,
}

pub fn switch(on: bool, on_toggle: impl Into<EventHandler<bool>>) -> Switch {
    Switch {
        on,
        enabled: true,
        on_toggle: on_toggle.into(),
    }
}

impl Switch {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Component for Switch {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on = self.on;
        let on_toggle = self.on_toggle.clone();
        let track = if on { tokens.accent } else { tokens.outline };
        let thumb = Color::WHITE;
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(RING_WIDTH)
                .alignment(BorderAlignment::Outer)
                .fill(tokens.accent)
        } else {
            Border::new()
        };
        let overlay = if self.enabled && pressing() {
            Color::BLACK.with_af32(0.16)
        } else if self.enabled && hovering() {
            Color::WHITE.with_af32(0.08)
        } else {
            Color::TRANSPARENT
        };
        let thumb_width = if pressing() { 22.0 } else { 20.0 };
        // Thumb travel and track colour ease over 100 ms (Expo out), like the
        // shell's `ControlSwitch.qml` behaviours; zero under reduced motion.
        let motion = tokens.motion(100);
        let thumb_x = use_animation_transition(
            if on {
                SWITCH_WIDTH - thumb_width - THUMB_INSET
            } else {
                THUMB_INSET
            },
            move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            },
        );
        let track_color = use_animation_transition(track, move |from, to| {
            AnimColor::new(from, to)
                .time(motion)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let track = track_color.get().value();
        let thumb_left = thumb_x.get().value();
        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(self.enabled)
            .a11y_enabled(self.enabled)
            .a11y_role(AccessibilityRole::Switch)
            .a11y_builder(move |builder| builder.set_toggled(Toggled::from(on)))
            .width(Size::px(SWITCH_WIDTH))
            .height(Size::px(SWITCH_HEIGHT))
            .corner_radius(RADIUS_LG)
            .background(track)
            .maybe(!self.enabled, |element| element.opacity(0.45))
            .border(border)
            .cursor(if self.enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
            .maybe(self.enabled, |element| {
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
                    .on_press({
                        let on_toggle = on_toggle.clone();
                        move |event: Event<PressEventData>| {
                            event.stop_propagation();
                            on_toggle.call(!on);
                            a11y_id.request_focus();
                        }
                    })
                    .on_key_down(move |event: Event<KeyboardEventData>| {
                        if is_activation_key(&event.key) {
                            event.stop_propagation();
                            on_toggle.call(!on);
                        }
                    })
            })
            .child(
                rect()
                    .expanded()
                    .corner_radius(RADIUS_LG)
                    .background(overlay),
            )
            .child(
                rect()
                    .position(Position::new_absolute().top(THUMB_INSET).left(thumb_left))
                    .width(Size::px(thumb_width))
                    .height(Size::px(BOX_SM))
                    .corner_radius(RADIUS_LG)
                    .background(thumb),
            )
    }
}
