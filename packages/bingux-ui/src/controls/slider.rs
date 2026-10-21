//! The slider: an accent-filled track and a round white handle.

use freya::animation::*;
use freya::prelude::*;

use crate::scale::*;
use crate::surface::{focus_border, use_tokens};

const TRACK: f32 = 6.0;
const HANDLE: f32 = 20.0;
const HANDLE_GROWN: f32 = 22.0;
const PERCENT: f32 = 100.0;

/// A 6 px track filled with the accent up to a 20 px white handle that
/// grows to 22 px while dragging. The handle stays inside the track, so
/// 0 and 100 are the ends of the travel.
#[derive(PartialEq)]
pub struct Slider {
    value: f64,
    enabled: bool,
    on_moved: EventHandler<f64>,
    width: f32,
    step: Option<f64>,
    show_value: bool,
}

pub fn slider(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Slider {
    Slider {
        value: value.clamp(0.0, 100.0),
        enabled: true,
        on_moved: on_moved.into(),
        width: 220.0,
        step: None,
        show_value: false,
    }
}

impl Slider {
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Snap reported values to multiples of `step` percent (keyboard arrows
    /// also move by this step).
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.max(0.1));
        self
    }

    /// Show the rounded percentage in a 12 px muted label after the track.
    pub fn show_value(mut self) -> Self {
        self.show_value = true;
        self
    }
}

impl Component for Slider {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut dragging = use_state(|| false);
        let mut size = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_moved = self.on_moved.clone();
        let value = self.value;
        let enabled = self.enabled;
        let motion = tokens.motion(120);
        let handle_anim = use_animation_transition(
            if dragging() { HANDLE + 2.0 } else { HANDLE },
            move |from, to| {
                AnimNum::new(from, to)
                    .time(motion)
                    .function(Function::Expo)
                    .ease(Ease::Out)
            },
        );
        let handle = handle_anim.get().value();
        let travel = (self.width - HANDLE).max(1.0);
        let step = self.step;
        let snap = move |percent: f64| -> f64 {
            match step {
                Some(step) => ((percent / step).round() * step).clamp(0.0, 100.0),
                None => percent,
            }
        };

        let percent_at = move |x: f64| -> f64 {
            let half = HANDLE as f64 / 2.0;
            let travel = travel as f64;
            snap(((x - half) / travel * 100.0).clamp(0.0, 100.0))
        };
        let on_pointer_down = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                a11y_id.request_focus();
                dragging.set(true);
                on_moved.call(percent_at(event.element_location().x));
            }
        };
        let on_move = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if *dragging.peek() {
                    event.prevent_default();
                    let x = event.global_location().x - size.read().min_x() as f64;
                    on_moved.call(percent_at(x));
                }
            }
        };
        let on_key_down = {
            let on_moved = on_moved.clone();
            move |event: Event<KeyboardEventData>| {
                let delta = step.unwrap_or(5.0);
                let delta = match event.key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => -delta,
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => delta,
                    Key::Named(NamedKey::Home) => -value,
                    Key::Named(NamedKey::End) => 100.0 - value,
                    _ => return,
                };
                on_moved.call(snap((value + delta).clamp(0.0, 100.0)));
            }
        };

        let track = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(100.0);
            })
            .width(Size::px(self.width))
            .height(Size::px(CONTROL_HEIGHT))
            .main_align(Alignment::Center)
            .corner_radius(RADIUS)
            .border(focus_border(&tokens, focus()))
            .maybe(!enabled, |element| element.opacity(0.45))
            .cursor(if enabled {
                CursorIcon::Pointer
            } else {
                CursorIcon::NotAllowed
            })
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .maybe(enabled, |element| {
                element
                    .on_pointer_down(on_pointer_down)
                    .on_capture_global_pointer_move(on_move)
                    .on_global_pointer_up(move |_| dragging.set_if_modified(false))
                    .on_key_down(on_key_down)
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(HANDLE_GROWN))
                    .main_align(Alignment::Center)
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(TRACK))
                            .corner_radius(TRACK / 2.0)
                            .background(tokens.outline),
                    )
                    .child(
                        rect()
                            .position(Position::new_absolute().top(1.0).left(0.0))
                            .width(Size::px(HANDLE / 2.0 + travel * (value as f32 / PERCENT)))
                            .height(Size::px(HANDLE))
                            .main_align(Alignment::Center)
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::px(TRACK))
                                    .corner_radius(TRACK / 2.0)
                                    .background(tokens.accent),
                            ),
                    )
                    .child(
                        rect()
                            .position(
                                Position::new_absolute()
                                    .top(1.0 + (HANDLE - handle) / 2.0)
                                    .left(
                                        travel * (value as f32 / 100.0) - (handle - HANDLE) / 2.0,
                                    ),
                            )
                            .width(Size::px(handle))
                            .height(Size::px(handle))
                            .corner_radius(handle / 2.0)
                            .background(Color::WHITE)
                            .shadow(
                                Shadow::new()
                                    .y(1.0)
                                    .blur(3.0)
                                    .color(Color::BLACK.with_af32(0.35)),
                            ),
                    ),
            );
        if !self.show_value {
            return Element::from(track);
        }
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .child(track)
            .child(
                label()
                    .text(format!("{}%", value.round() as i64))
                    .font_size(TEXT_SM)
                    .color(tokens.muted)
                    .min_width(Size::px(BOX_LG))
                    .text_align(TextAlign::End),
            )
            .into()
    }
}
