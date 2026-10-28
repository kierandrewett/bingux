//! The slider: `SeekSlider.qml`. A thin track with an accent fill and a
//! white knob. Track, fill and thumb share one centre line.

use freya::animation::*;
use freya::prelude::*;

use super::capture::{CaptureEvent, Phase, use_capture};
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};
use crate::typography::{CAPTION, Ink};

/// Width of the 100% tick on the track, in px.
const TICK_WIDTH: f32 = 2.0;
/// How strongly the danger colour tints the empty part of the track.
const DANGER_TRACK_OPACITY: f32 = 0.45;
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
    /// Colour stops across the track, in percent and RGB. Replaces the accent fill.
    gradient: Option<Vec<(f64, (u8, u8, u8))>>,
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
        gradient: None,
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

    /// Paints the track as a colour gradient instead of the accent fill. Each
    /// stop is a percent of the track and an RGB colour: a colour temperature
    /// strip, a hue bar or a brightness ramp. The knob and keys work as before.
    pub fn gradient(mut self, stops: Vec<(f64, (u8, u8, u8))>) -> Self {
        self.gradient = Some(stops);
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

    /// Tint the track past `percent` in the danger colour, always, and paint
    /// the filled part of it solid once the value passes. It does not limit
    /// the value.
    pub fn danger_from(mut self, percent: f64) -> Self {
        self.danger_from = Some(percent);
        self
    }
}

/// The SVG for a gradient track: one rounded rectangle filled with a linear
/// gradient through the stops, left to right.
fn gradient_svg(stops: &[(f64, (u8, u8, u8))], width: f32, height: f32) -> String {
    use std::fmt::Write as _;
    let mut body = String::new();
    for (at, (red, green, blue)) in stops {
        let _ = write!(
            body,
            r##"<stop offset="{at}%" stop-color="#{red:02x}{green:02x}{blue:02x}"/>"##
        );
    }
    let radius = height / 2.0;
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs><linearGradient id="track" x1="0" y1="0" x2="1" y2="0">{body}</linearGradient></defs><rect width="{width}" height="{height}" rx="{radius}" fill="url(#track)"/></svg>"#
    )
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
        // The press, the drag wherever the pointer goes, and the release are
        // one capture (see `controls::capture`): the value follows the pointer
        // at most once a frame, and the drag ends with the button.
        let capture = use_capture({
            let on_moved = on_moved.clone();
            move |event: CaptureEvent| match event.phase {
                Phase::Start => {
                    a11y_id.request_focus();
                    pressing.set(true);
                    on_moved.call(percent_at(event.local.0 as f64));
                }
                Phase::Move => {
                    dragging.set_if_modified(true);
                    on_moved.call(percent_at(event.local.0 as f64));
                }
                Phase::End => {
                    on_moved.call(percent_at(event.local.0 as f64));
                    pressing.set_if_modified(false);
                    dragging.set_if_modified(false);
                }
                Phase::Cancel => {
                    pressing.set_if_modified(false);
                    dragging.set_if_modified(false);
                }
            }
        });
        let tracker = capture.clone();
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

        let track_height = if interacting || self.gradient.is_some() {
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
        let gradient = self
            .gradient
            .as_ref()
            .map(|stops| gradient_svg(stops, width, track_height));

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
            .on_sized(move |event: Event<SizedEventData>| {
                // Only a change of size draws the slider again: a slider that
                // is merely carried (scrolled, slid in) keeps its frame.
                if size.peek().size != event.area.size {
                    size.set(event.area);
                }
                tracker.sized(&event);
            })
            .maybe(enabled, |element| {
                capture.handlers(element).on_key_down(on_key_down)
            })
            .child(match &gradient {
                Some(svg) => rect()
                    .position(
                        Position::new_absolute()
                            .top(centre - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(width))
                    .height(Size::px(track_height))
                    .child(
                        SvgViewer::new((svg.clone(), Bytes::from(svg.clone())))
                            .width(Size::px(width))
                            .height(Size::px(track_height)),
                    ),
                None => rect()
                    .position(
                        Position::new_absolute()
                            .top(centre - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(width))
                    .height(Size::px(track_height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Outline)]),
            })
            .maybe_child(gradient.is_none().then(|| {
                rect()
                    .layer(Layer::Relative(1))
                    .position(
                        Position::new_absolute()
                            .top(centre - track_height / 2.0)
                            .left(0.0),
                    )
                    .width(Size::px(fill_width))
                    .height(Size::px(track_height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(fill_paint)])
            }))
            // The whole travel past `danger_from`, tinted on the empty track so
            // the danger zone shows before the value reaches it.
            .maybe_child(
                danger_from
                    .filter(|point| gradient.is_none() && *point < max)
                    .map(|point| {
                        let left = thumb_size / 2.0 + (travel * ((point / max) as f32)).round();
                        rect()
                            .position(
                                Position::new_absolute()
                                    .top(centre - track_height / 2.0)
                                    .left(left),
                            )
                            .width(Size::px((width - left).max(0.0)))
                            .height(Size::px(track_height))
                            .opacity(DANGER_TRACK_OPACITY)
                            .tw(
                                &tokens,
                                &[rounded_on(Radius::Pill, Corners::Right, 0.0), bg(Paint::Danger)],
                            )
                    }),
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
                            .tw(
                                &tokens,
                                &[rounded_on(Radius::Pill, Corners::Right, 0.0), bg(Paint::Danger)],
                            )
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
            .maybe_child(enabled.then(|| {
                rect()
                    .position(
                        Position::new_absolute()
                            .top(centre - thumb_size / 2.0)
                            .left(centre_x - thumb_size / 2.0),
                    )
                    .child(
                        // The knob that paints is a sibling above this box, which
                        // stands for it at rest: the same size and radius.
                        rect()
                            .width(Size::px(thumb_size))
                            .height(Size::px(thumb_size))
                            .tw(&tokens, &[rounded(Radius::Pill)])
                            .focusable(focus(), Radius::Pill),
                    )
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
            .component("Slider")
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
