//! The slider: `SeekSlider.qml`. A thin track with a text-coloured fill and
//! a round 16 px thumb. Track, fill and thumb share one centre line.

use freya::animation::*;
use freya::prelude::*;

use crate::scale::*;
use crate::surface::{use_enabled, use_tokens};

/// The track is 6 px, or 8 px while it is being dragged.
const TRACK: f32 = 6.0;
const TRACK_ACTIVE: f32 = 8.0;
/// The thumb is a 16 px circle.
const THUMB: f32 = 16.0;
/// Thumb scale while pressed and while dragging.
const SCALE_PRESSED: f32 = 1.1;
const SCALE_DRAGGING: f32 = 1.3;
/// The pointer hit row, and the focus ring: a 1 px circle hugging the thumb.
const HIT: f32 = CONTROL_HEIGHT;
const FOCUS_RING: f32 = THUMB + 2.0;
const DISABLED_OPACITY: f32 = 0.45;
const SCALE_MS: u64 = 120;
const PERCENT: f64 = 100.0;
/// Page Up and Page Down move the slider this many percent.
const PAGE_STEP: f64 = 10.0;

/// Every part of the slider is placed from this one line, the middle of the
/// hit row, so the track, the fill and the thumb share one centre whatever
/// size the track and thumb are at that frame.
const CENTRE: f32 = HIT / 2.0;

/// A thin track filled up to a 16 px thumb. The thumb stays inside the
/// track, so 0 and 100 are the ends of the travel.
#[derive(PartialEq)]
pub struct Slider {
    value: f64,
    enabled: bool,
    on_moved: EventHandler<f64>,
    width: f32,
    fluid: bool,
    step: Option<f64>,
    show_value: bool,
}

pub fn slider(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Slider {
    Slider {
        value: value.clamp(0.0, 100.0),
        enabled: true,
        on_moved: on_moved.into(),
        width: FIELD_WIDTH,
        fluid: false,
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

    /// Fill the width the parent gives, instead of a fixed width. The track
    /// follows the measured width, so the first frame uses the fixed width.
    pub fn fill(mut self) -> Self {
        self.fluid = true;
        self
    }

    /// Snap reported values to multiples of `step` percent (keyboard arrows
    /// also move by this step).
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.max(0.1));
        self
    }

    /// Show the rounded percentage in a muted label after the track.
    pub fn show_value(mut self) -> Self {
        self.show_value = true;
        self
    }
}

impl Component for Slider {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut size = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_moved = self.on_moved.clone();
        let value = self.value;
        let enabled = use_enabled(self.enabled);
        let interacting = pressing() && dragging();

        let scale_target = if interacting {
            SCALE_DRAGGING
        } else if pressing() {
            SCALE_PRESSED
        } else {
            1.0
        };
        let motion = tokens.motion(SCALE_MS);
        let scale = use_animation_transition(scale_target, move |from, to| {
            AnimNum::new(from, to)
                .time(motion)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let thumb = THUMB * scale.get().value();

        let measured = size.read().width();
        let width = if self.fluid && measured > THUMB {
            measured
        } else {
            self.width
        };
        let travel = (width - THUMB).max(1.0);
        let step = self.step;
        let snap = move |percent: f64| -> f64 {
            match step {
                Some(step) => ((percent / step).round() * step).clamp(0.0, 100.0),
                None => percent,
            }
        };
        let percent_at = move |x: f64| -> f64 {
            let half = THUMB as f64 / 2.0;
            snap(((x - half) / travel as f64 * PERCENT).clamp(0.0, PERCENT))
        };
        let on_pointer_down = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if !event.data().is_primary() {
                    return;
                }
                event.stop_propagation();
                a11y_id.request_focus();
                pressing.set(true);
                on_moved.call(percent_at(event.element_location().x));
            }
        };
        let on_move = {
            let on_moved = on_moved.clone();
            move |event: Event<PointerEventData>| {
                if *pressing.peek() {
                    event.prevent_default();
                    dragging.set_if_modified(true);
                    let x = event.global_location().x - size.read().min_x() as f64;
                    on_moved.call(percent_at(x));
                }
            }
        };
        let on_release = move |_| {
            pressing.set_if_modified(false);
            dragging.set_if_modified(false);
        };
        let on_key_down = {
            let on_moved = on_moved.clone();
            move |event: Event<KeyboardEventData>| {
                let delta = step.unwrap_or(5.0);
                let delta = match event.key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => -delta,
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => delta,
                    Key::Named(NamedKey::Home) => -value,
                    Key::Named(NamedKey::End) => PERCENT - value,
                    Key::Named(NamedKey::PageUp) => PAGE_STEP,
                    Key::Named(NamedKey::PageDown) => -PAGE_STEP,
                    _ => return,
                };
                on_moved.call(snap((value + delta).clamp(0.0, PERCENT)));
            }
        };

        let track_height = if interacting { TRACK_ACTIVE } else { TRACK };
        let fill_colour = if !enabled {
            tokens.muted
        } else if interacting {
            tokens.accent
        } else {
            tokens.text
        };
        let thumb_x = travel * (value as f32 / PERCENT as f32);
        let centre_x = THUMB / 2.0 + thumb_x;
        let fill_width = centre_x;

        let row = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(PERCENT);
            })
            .width(if self.fluid {
                Size::fill()
            } else {
                Size::px(width)
            })
            .height(Size::px(HIT))
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
            .on_sized(move |event: Event<SizedEventData>| size.set(event.area))
            .maybe(enabled, |element| {
                element
                    .on_pointer_down(on_pointer_down)
                    .on_capture_global_pointer_move(on_move)
                    .on_global_pointer_up(on_release)
                    .on_key_down(on_key_down)
            })
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .top(CENTRE - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(width))
                    .height(Size::px(track_height))
                    .corner_radius(track_height / 2.0)
                    .background(tokens.outline),
            )
            .child(
                rect()
                    .layer(Layer::Relative(1))
                    .position(
                        Position::new_absolute()
                            .top(CENTRE - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(fill_width))
                    .height(Size::px(track_height))
                    .corner_radius(track_height / 2.0)
                    .background(fill_colour),
            )
            .maybe_child((enabled && focus() == Focus::Keyboard).then(|| {
                rect()
                    .position(
                        Position::new_absolute()
                            .top(CENTRE - FOCUS_RING / 2.0)
                            .left(centre_x - FOCUS_RING / 2.0),
                    )
                    .width(Size::px(FOCUS_RING))
                    .height(Size::px(FOCUS_RING))
                    .corner_radius(FOCUS_RING / 2.0)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.accent),
                    )
            }))
            .child(
                rect()
                    .layer(Layer::Relative(3))
                    .position(
                        Position::new_absolute()
                            .top(CENTRE - thumb / 2.0)
                            .left(centre_x - thumb / 2.0),
                    )
                    .width(Size::px(thumb))
                    .height(Size::px(thumb))
                    .corner_radius(thumb / 2.0)
                    .background(tokens.text)
                    .border(
                        Border::new()
                            .width(1.0)
                            .alignment(BorderAlignment::Inner)
                            .fill(tokens.elevated),
                    ),
            );
        if !self.show_value {
            return Element::from(row);
        }
        rect()
            .direction(Direction::Horizontal)
            .cross_align(Alignment::Center)
            .spacing(S2)
            .child(row)
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
