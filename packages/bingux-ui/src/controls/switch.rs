//! The switch: `ControlSwitch.qml`. A 36 x 20 pill, accent when on and
//! elevated with a 1 px outline when off, with a 16 px thumb inset 2 px.

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use crate::surface::{is_activation_key, use_enabled, use_tokens};

const SWITCH_WIDTH: f32 = 48.0;
const SWITCH_HEIGHT: f32 = 28.0;
/// Pill radius: half the track height.
const SWITCH_RADIUS: f32 = 14.0;
const THUMB: f32 = 20.0;
/// The thumb widens to this while the switch is pressed.
const THUMB_PRESSED: f32 = 24.0;
const THUMB_INSET: f32 = 4.0;
const HOVER_OVERLAY: f32 = 0.08;
const PRESSED_OVERLAY: f32 = 0.16;
const DISABLED_OPACITY: f32 = 0.45;
/// Thumb travel eases over 100 ms and its width over 80 ms.
const TRAVEL_MS: u64 = 100;
const WIDTH_MS: u64 = 80;

/// A 36 x 20 switch. The thumb is white when on and the text colour when
/// off.
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
        let enabled = use_enabled(self.enabled);

        let track = if on { tokens.text } else { tokens.control };
        let outline = tokens.outline;
        let thumb_color = if on { tokens.bg } else { tokens.muted };
        let overlay = if enabled && pressing() {
            Color::BLACK.with_af32(PRESSED_OVERLAY)
        } else if enabled && hovering() {
            Color::WHITE.with_af32(HOVER_OVERLAY)
        } else {
            Color::TRANSPARENT
        };
        let thumb_width = if enabled && pressing() {
            THUMB_PRESSED
        } else {
            THUMB
        };
        let travel_end = SWITCH_WIDTH - THUMB_INSET - thumb_width;
        let travel_start = THUMB_INSET;
        let travel = if on { travel_end } else { travel_start };

        let travel_motion = tokens.motion(TRAVEL_MS);
        let thumb_x = use_animation_transition(travel, move |from, to| {
            AnimNum::new(from, to)
                .time(travel_motion)
                .function(Function::Cubic)
                .ease(Ease::Out)
        });
        let width_motion = tokens.motion(WIDTH_MS);
        let thumb_w = use_animation_transition(thumb_width, move |from, to| {
            AnimNum::new(from, to)
                .time(width_motion)
                .function(Function::Cubic)
                .ease(Ease::Out)
        });

        let border = if focus() == Focus::Keyboard {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Outer)
                .fill(tokens.text)
        } else if on {
            Border::new()
        } else {
            Border::new()
                .width(1.0)
                .alignment(BorderAlignment::Inner)
                .fill(outline)
        };

        rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Switch)
            .a11y_builder(move |builder| builder.set_toggled(Toggled::from(on)))
            .width(Size::px(SWITCH_WIDTH))
            .height(Size::px(SWITCH_HEIGHT))
            .corner_radius(SWITCH_RADIUS)
            .background(track)
            .border(border)
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
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
                    .corner_radius(SWITCH_RADIUS)
                    .background(overlay),
            )
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .top((SWITCH_HEIGHT - THUMB) / 2.0)
                            .left(thumb_x.get().value()),
                    )
                    .width(Size::px(thumb_w.get().value()))
                    .height(Size::px(THUMB))
                    .corner_radius(THUMB / 2.0)
                    .background(thumb_color),
            )
    }
}
