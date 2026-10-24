//! The shell's switch (`ControlSwitch.qml`).

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::surface::{is_activation_key, use_tokens};

/// The shell's switch: a 36 x 20 track, accent when on, with a 16 px thumb.
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
        let track = if on { tokens.accent } else { tokens.elevated };
        let thumb = if on { Color::WHITE } else { tokens.text };
        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.text)
        } else if on {
            Border::new()
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(tokens.outline)
        };
        let overlay = if self.enabled && pressing() {
            Color::BLACK.with_af32(0.16)
        } else if self.enabled && hovering() {
            Color::WHITE.with_af32(0.08)
        } else {
            Color::TRANSPARENT
        };
        let thumb_width = if pressing() { 18.0 } else { 16.0 };
        // Thumb travel and track colour ease over 100 ms (Expo out), like the
        // shell's `ControlSwitch.qml` behaviours; zero under reduced motion.
        let motion = tokens.motion(100);
        let thumb_x = use_animation_transition(
            if on { 36.0 - thumb_width - 2.0 } else { 2.0 },
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
            .width(Size::px(36.0))
            .height(Size::px(20.0))
            .corner_radius(10.0)
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
            .child(rect().expanded().corner_radius(10.0).background(overlay))
            .child(
                rect()
                    .position(Position::new_absolute().top(2.0).left(thumb_left))
                    .width(Size::px(thumb_width))
                    .height(Size::px(16.0))
                    .corner_radius(8.0)
                    .background(thumb),
            )
    }
}
