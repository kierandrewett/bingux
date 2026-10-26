//! The switch: `ControlSwitch.qml`. A 36 x 20 pill, accent when on and
//! elevated with a 1 px outline when off, with a 16 px thumb inset 2 px.

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use super::press::use_press;
use crate::component::{ComponentName, Prop, Published, prop_override};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};

const THUMB_INSET: f32 = 2.0;
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
        // The inspector can set the switch live; the app's own value is what
        // the published call shows.
        let on = prop_override(a11y_id, "on")
            .map(|value| value == "true")
            .unwrap_or(self.on);
        let on_toggle = self.on_toggle.clone();
        let enabled = use_enabled(self.enabled);
        let press = use_press();

        let track = if on { Paint::Accent } else { Paint::Control };
        let thumb = if on { Paint::OnAccent } else { Paint::Text };
        let pressed = enabled && pressing();
        let thumb_width = if pressed {
            Dim::SwitchThumbPressed.px()
        } else {
            Dim::SwitchThumb.px()
        };
        let travel_end = Dim::SwitchWidth.px() - THUMB_INSET - thumb_width;
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

        rect()
            .component("Switch")
            .published(|| Published::call("switch").arg(Prop::flag("on", self.on).live()))
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Switch)
            .a11y_builder(move |builder| builder.set_toggled(Toggled::from(on)))
            .tw(
                &tokens,
                &[
                    w(Dim::SwitchWidth),
                    h(Dim::SwitchHeight),
                    rounded(Radius::Pill),
                    bg(track),
                ],
            )
            .maybe(!on, |element| element.tw(&tokens, &[border(Line::Outline)]))
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
                        press.begin(&event, None);
                    })
                    .on_global_pointer_move(move |event: Event<PointerEventData>| {
                        press.moved(&event);
                    })
                    .on_global_pointer_up(move |_| {
                        press.release();
                        pressing.set_if_modified(false);
                    })
                    .on_press({
                        let on_toggle = on_toggle.clone();
                        move |event: Event<PressEventData>| {
                            event.stop_propagation();
                            if !press.tap(&event) {
                                return;
                            }
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
            .child(rect().expanded().tw(
                &tokens,
                &[
                    rounded(Radius::Pill),
                    surface(Paint::Transparent, enabled && hovering(), pressed, false),
                ],
            ))
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .top((Dim::SwitchHeight.px() - Dim::SwitchThumb.px()) / 2.0)
                            .left(thumb_x.get().value()),
                    )
                    .width(Size::px(thumb_w.get().value()))
                    .tw(
                        &tokens,
                        &[h(Dim::SwitchThumb), rounded(Radius::Pill), bg(thumb)],
                    ),
            )
            .focusable(focus(), Radius::Pill)
    }
}
