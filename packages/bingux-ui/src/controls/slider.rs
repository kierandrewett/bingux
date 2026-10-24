//! The slider: `SeekSlider.qml`. A thin track with an accent fill and a
//! white knob. Track, fill and thumb share one centre line.

use freya::animation::*;
use freya::prelude::*;

use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{CAPTION, Ink};

/// Width of the 100% tick on the track, in px.
const TICK_WIDTH: f32 = 2.0;
/// Height of the 100% tick, as a fraction of the thumb: taller than the track.
const TICK_HEIGHT: f32 = 0.9;
/// Thumb scale while pressed and while dragging.
const SCALE_PRESSED: f32 = 1.1;
const SCALE_DRAGGING: f32 = 1.3;
const SCALE_MS: u64 = 120;
const PERCENT: f64 = 100.0;
/// Page Up and Page Down move the slider this many percent.
const PAGE_STEP: f64 = 10.0;

/// A thin track filled up to a thumb. The thumb stays inside the track, so
/// the ends of the travel are 0 and `max` (100 unless set with `.max()`).
#[derive(PartialEq)]
pub struct Slider {
    value: f64,
    /// The top of the travel, in percent. 100 unless the slider can go
    /// past full volume.
    max: f64,
    /// A tick on the track at this percent, such as the 100% clipping point.
    mark: Option<f64>,
    /// Where the travel turns to the danger colour, in percent, such as the
    /// clipping zone past 100%. The value still goes that high.
    danger_from: Option<f64>,
    enabled: bool,
    on_moved: EventHandler<f64>,
    width: Dim,
    fluid: bool,
    step: Option<f64>,
    show_value: bool,
}

pub fn slider(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Slider {
    Slider {
        value,
        max: PERCENT,
        mark: None,
        danger_from: None,
        enabled: true,
        on_moved: on_moved.into(),
        width: Dim::Field,
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

    /// The slider is this wide. A field-sized dim shrinks on a phone.
    pub fn width(mut self, width: Dim) -> Self {
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

    /// The top of the travel, in percent. Values past 100 are for an
    /// amplified output; the track and the keyboard respect this top.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max.max(1.0);
        self
    }

    /// Draw a tick on the track at `percent`, such as where the sound
    /// starts to clip. The tick does not limit the value.
    pub fn mark(mut self, percent: f64) -> Self {
        self.mark = Some(percent);
        self
    }

    /// Paint the travel past `percent` in the danger colour. It does not
    /// limit the value.
    pub fn danger_from(mut self, percent: f64) -> Self {
        self.danger_from = Some(percent);
        self
    }
}

impl Component for Slider {
    fn render(&self) -> impl IntoElement {
        // The pointer hit row, and the focus ring: a 1 px circle hugging the
        // thumb. Every part of the slider is placed from the centre of the hit
        // row, so the track, the fill and the thumb share one centre whatever
        // size the track and thumb are at that frame.
        let thumb_size = Dim::SliderThumb.px();
        let hit = Dim::Control.px();
        let centre = hit / 2.0;
        let focus_ring_size = thumb_size + 2.0;
        let tokens = use_tokens();
        let mut pressing = use_state(|| false);
        let mut dragging = use_state(|| false);
        let mut size = use_state(Area::default);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let on_moved = self.on_moved.clone();
        let max = self.max;
        let mark = self.mark;
        let value = self.value.clamp(0.0, max);
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
        let thumb = thumb_size * scale.get().value();

        let measured = size.read().width();
        let width = if self.fluid && measured > thumb_size {
            measured
        } else {
            self.width.px()
        };
        let travel = (width - thumb_size).max(1.0);
        let step = self.step;
        let snap = move |percent: f64| -> f64 {
            match step {
                Some(step) => ((percent / step).round() * step).clamp(0.0, max),
                None => percent,
            }
        };
        let percent_at = move |x: f64| -> f64 {
            let half = thumb_size as f64 / 2.0;
            snap(((x - half) / travel as f64 * max).clamp(0.0, max))
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
                    Key::Named(NamedKey::End) => max - value,
                    Key::Named(NamedKey::PageUp) => PAGE_STEP,
                    Key::Named(NamedKey::PageDown) => -PAGE_STEP,
                    _ => return,
                };
                on_moved.call(snap((value + delta).clamp(0.0, max)));
            }
        };

        let track_height = if interacting {
            Dim::SliderTrackActive.px()
        } else {
            Dim::SliderTrack.px()
        };
        let fill_paint = if enabled { Paint::Accent } else { Paint::Muted };
        let danger_from = self.danger_from;
        // Whole pixels keep the knob's edge crisp; a fractional left blurs it.
        let thumb_x = (travel * ((value / max) as f32)).round();
        let centre_x = thumb_size / 2.0 + thumb_x;
        let fill_width = centre_x;

        let row = rect()
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(max);
            })
            .maybe(self.fluid, |element| element.width(Size::fill()))
            .maybe(!self.fluid, |element| element.tw(&tokens, &[w(self.width)]))
            .tw(&tokens, &[h(Dim::Control)])
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
                            .top(centre - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(width))
                    .height(Size::px(track_height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Outline)]),
            )
            .child(
                rect()
                    .layer(Layer::Relative(1))
                    .position(
                        Position::new_absolute()
                            .top(centre - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(fill_width))
                    .height(Size::px(track_height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(fill_paint)]),
            )
            // The part of the fill past `danger_from`, drawn over the accent fill.
            .maybe_child(
                danger_from
                    .filter(|point| enabled && value > *point)
                    .map(|point| {
                        let left = thumb_size / 2.0 + (travel * ((point / max) as f32)).round();
                        rect()
                            .layer(Layer::Relative(1))
                            .position(
                                Position::new_absolute()
                                    .top(centre - track_height / 2.0)
                                    .left(left),
                            )
                            .width(Size::px((centre_x - left).max(0.0)))
                            .height(Size::px(track_height))
                            .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Danger)])
                    }),
            )
            .maybe_child(mark.filter(|point| *point < max).map(|point| {
                // Taller than the track and in the text colour, so the tick
                // stays visible over the accent fill.
                let tick = TICK_WIDTH;
                let tick_height = thumb_size * TICK_HEIGHT;
                rect()
                    .layer(Layer::Relative(2))
                    .position(
                        Position::new_absolute()
                            .top(centre - tick_height / 2.0)
                            .left(
                                thumb_size / 2.0 + (travel * ((point / max) as f32)).round()
                                    - tick / 2.0,
                            ),
                    )
                    .width(Size::px(tick))
                    .height(Size::px(tick_height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Text)])
            }))
            .maybe_child((enabled && focus() == Focus::Keyboard).then(|| {
                rect()
                    .position(
                        Position::new_absolute()
                            .top(centre - focus_ring_size / 2.0)
                            .left(centre_x - focus_ring_size / 2.0),
                    )
                    .width(Size::px(focus_ring_size))
                    .height(Size::px(focus_ring_size))
                    .tw(&tokens, &[rounded(Radius::Pill), focus_ring(focus())])
            }))
            .child(
                rect()
                    .layer(Layer::Relative(3))
                    .position(
                        Position::new_absolute()
                            .top((centre - thumb / 2.0).round())
                            .left((centre_x - thumb / 2.0).round()),
                    )
                    .width(Size::px(thumb))
                    .height(Size::px(thumb))
                    .tw(
                        &tokens,
                        &[
                            rounded(Radius::Pill),
                            bg(Paint::Knob),
                            shadow(Elevation::Knob),
                        ],
                    ),
            );
        if !self.show_value {
            return Element::from(row);
        }
        rect()
            .tw(&tokens, &[flex_row(), items(Align::Center), gap(S2)])
            .child(row)
            .child(
                rect().tw(&tokens, &[w(Dim::Box9)]).child(
                    CAPTION
                        .in_ink(Ink::Muted)
                        .label(&tokens, format!("{}%", value.round() as i64))
                        .text_align(TextAlign::End),
                ),
            )
            .into()
    }
}
