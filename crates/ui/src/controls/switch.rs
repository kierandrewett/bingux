//! The switch: `ControlSwitch.qml`. A 36 x 20 pill, accent when on and
//! elevated with a 1 px outline when off, with a 16 px thumb inset 2 px.

use accesskit::Toggled;
use freya::animation::*;
use freya::prelude::*;

use super::press::{hit_area, use_press};
use crate::component::{ComponentName, Prop, Published, prop_override};
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{is_activation_key, use_enabled, use_tokens};

const THUMB_INSET: f32 = 2.0;
/// Thumb travel eases over 100 ms and its width over 80 ms.
const TRAVEL_MS: u64 = 100;
const WIDTH_MS: u64 = 80;

/// A 36 x 20 switch. The thumb is white when on and the text colour when
/// off ([`thumb_paint`]).
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

/// The paint of the thumb. Off it is the text colour, the one thing on the neutral track that
/// reads against it in both schemes. On it is the knob's white, the slider's thumb on the same
/// accent fill and the one `ControlSwitch.qml` draws: not `OnAccent`, which is an ink for text and
/// is a different colour for every accent (a dark navy on the blue of the dark scheme, next to an
/// off thumb that is white), so the thumb looked like two objects as it travelled. A soft knob
/// shadow (below) keeps it apart from a mid-tone track the white only reaches 2.3:1 on.
pub(crate) fn thumb_paint(on: bool) -> Paint {
    if on { Paint::Knob } else { Paint::Text }
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
        let thumb = thumb_paint(on);
        let pressed = enabled && pressing();
        let thumb_width = if pressed {
            Dim::SwitchThumbPressed.px()
        } else {
            Dim::SwitchThumb.px()
        };
        let travel_end = Dim::SwitchWidth.px() - THUMB_INSET - thumb_width;
        let travel_start = THUMB_INSET;
        let travel = if on { travel_end } else { travel_start };

        // Tweens, not transitions: a switch mounts at rest, and a page of
        // them mounts without starting an animation for each thumb.
        let thumb_x = crate::motion::use_tween_with(
            travel,
            travel,
            tokens.motion(TRAVEL_MS),
            Function::Cubic,
        );
        let thumb_w = crate::motion::use_tween_with(
            thumb_width,
            thumb_width,
            tokens.motion(WIDTH_MS),
            Function::Cubic,
        );

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
            // At compact density the hit area reads the hover (see `hit_area`): the
            // switch's own box ends at its face, so a pointer in the ring never reaches it.
            .maybe(enabled && !is_compact(), |element| {
                element
                    .on_pointer_over(move |_| hovering.set(true))
                    .on_pointer_out(move |_| {
                        hovering.set(false);
                        pressing.set_if_modified(false);
                    })
            })
            .maybe(enabled, |element| {
                element
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
                    .component("SwitchThumb")
                    .position(
                        Position::new_absolute()
                            .top((Dim::SwitchHeight.px() - Dim::SwitchThumb.px()) / 2.0)
                            .left(thumb_x),
                    )
                    .width(Size::px(thumb_w))
                    .tw(
                        &tokens,
                        &[
                            h(Dim::SwitchThumb),
                            rounded(Radius::Pill),
                            bg(thumb),
                            shadow(Elevation::Knob),
                        ],
                    ),
            )
            // At compact density the painted switch is 28 px tall and a finger
            // needs 44: the hit area is widened, the drawing is not.
            .maybe_child(is_compact().then(|| {
                let face = (Dim::SwitchWidth.px(), Dim::SwitchHeight.px());
                hit_area(face, (face.0.max(TOUCH_MIN), TOUCH_MIN), hovering, pressing)
            }))
            .focusable(focus())
    }
}
