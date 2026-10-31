//! The fader: a slider that runs up and down. A thin track with an accent fill
//! from the bottom and a knob, for a bank of levels side by side, such as the
//! bands of an equalizer or the channels of a mixer.
//!
//! The value is a percent, 0 at the bottom. Up and Down move it by `step`
//! (Shift moves ten), Page Up and Page Down by ten, Home and End go to the ends.

use freya::prelude::*;

use super::capture::{CaptureEvent, Phase, use_capture};
use crate::component::ComponentName;
use crate::motion::Focusable;
use crate::style::*;
use crate::surface::{use_enabled, use_tokens};

const PAGE_STEP: f64 = 10.0;

/// A vertical slider. See the module docs.
#[derive(PartialEq)]
pub struct Fader {
    value: f64,
    on_moved: EventHandler<f64>,
    height: f32,
    step: f64,
    enabled: bool,
    label: Option<String>,
}

/// A fader at `value` percent. `on_moved` gets the new percent while the knob
/// is dragged or a key moves it.
pub fn fader(value: f64, on_moved: impl Into<EventHandler<f64>>) -> Fader {
    Fader {
        value,
        on_moved: on_moved.into(),
        height: 160.0,
        step: 1.0,
        enabled: true,
        label: None,
    }
}

impl Fader {
    /// The height of the whole control in px.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height.max(Dim::Control.px() * 2.0);
        self
    }

    /// The percent one arrow key press moves the knob.
    pub fn step(mut self, step: f64) -> Self {
        self.step = step.max(0.01);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// What assistive technology calls the fader.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Component for Fader {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let thumb = Dim::SliderThumb.px();
        let width = Dim::Control.px();
        let height = self.height;
        let travel = (height - thumb).max(1.0);
        let value = self.value.clamp(0.0, 100.0);
        let enabled = use_enabled(self.enabled);
        let a11y_id = use_a11y();
        let focus = use_focus(a11y_id);
        let mut pressing = use_state(|| false);
        let on_moved = self.on_moved.clone();
        let step = self.step;
        let percent_at = move |y: f64| -> f64 {
            let half = f64::from(thumb) / 2.0;
            (100.0 - ((y - half) / f64::from(travel) * 100.0)).clamp(0.0, 100.0)
        };
        let capture = use_capture({
            let on_moved = on_moved.clone();
            move |event: CaptureEvent| match event.phase {
                Phase::Start => {
                    a11y_id.request_focus();
                    pressing.set(true);
                    on_moved.call(percent_at(f64::from(event.local.1)));
                }
                Phase::Move => on_moved.call(percent_at(f64::from(event.local.1))),
                Phase::End => {
                    on_moved.call(percent_at(f64::from(event.local.1)));
                    pressing.set_if_modified(false);
                }
                Phase::Cancel => pressing.set_if_modified(false),
            }
        });
        let tracker = capture.clone();
        let on_key_down = move |event: Event<KeyboardEventData>| {
            let delta = step
                * if event.modifiers.contains(Modifiers::SHIFT) {
                    10.0
                } else {
                    1.0
                };
            let next = match event.key {
                Key::Named(NamedKey::ArrowUp) => value + delta,
                Key::Named(NamedKey::ArrowDown) => value - delta,
                Key::Named(NamedKey::PageUp) => value + PAGE_STEP,
                Key::Named(NamedKey::PageDown) => value - PAGE_STEP,
                Key::Named(NamedKey::Home) => 0.0,
                Key::Named(NamedKey::End) => 100.0,
                _ => return,
            };
            on_moved.call(next.clamp(0.0, 100.0));
        };
        let track_width = Dim::SliderTrack.px();
        let thumb_top = (travel * (1.0 - (value / 100.0) as f32)).round();
        let centre_y = thumb / 2.0 + thumb_top;
        let track_left = (width - track_width) / 2.0;
        let label = self.label.clone();
        let paint = if enabled { Paint::Accent } else { Paint::Muted };
        rect()
            .component("Fader")
            .a11y_id(a11y_id)
            .a11y_focusable(enabled)
            .a11y_enabled(enabled)
            .a11y_role(AccessibilityRole::Slider)
            .maybe(label.is_some(), |element| {
                element.a11y_alt(label.clone().unwrap_or_default())
            })
            .a11y_builder(move |builder| {
                builder.set_numeric_value(value);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(100.0);
            })
            .width(Size::px(width))
            .height(Size::px(height))
            .maybe(!enabled, |element| element.opacity(DISABLED_OPACITY))
            .on_sized(move |event: Event<SizedEventData>| tracker.sized(&event))
            .maybe(enabled, |element| {
                capture.handlers(element).on_key_down(on_key_down)
            })
            .child(
                rect()
                    .component("FaderTrack")
                    .position(Position::new_absolute().top(0.0).left(track_left))
                    .width(Size::px(track_width))
                    .height(Size::px(height))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(Paint::Outline)]),
            )
            .child(
                rect()
                    .component("FaderFill")
                    .layer(Layer::Relative(1))
                    .position(Position::new_absolute().top(centre_y).left(track_left))
                    .width(Size::px(track_width))
                    .height(Size::px((height - centre_y).max(0.0)))
                    .tw(&tokens, &[rounded(Radius::Pill), bg(paint)]),
            )
            .maybe_child(enabled.then(|| {
                rect()
                    .position(
                        Position::new_absolute()
                            .top(thumb_top)
                            .left((width - thumb) / 2.0),
                    )
                    .width(Size::px(thumb))
                    .height(Size::px(thumb))
                    .tw(&tokens, &[rounded(Radius::Pill)])
                    .focusable(focus())
            }))
            .child(
                rect()
                    .component("FaderKnob")
                    .layer(Layer::Relative(3))
                    .position(
                        Position::new_absolute()
                            .top(thumb_top)
                            .left((width - thumb) / 2.0),
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
            )
    }
}
