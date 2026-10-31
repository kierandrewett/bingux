//! The level meter: a bar that fills from the left to a level, for sound.
//!
//! The bar is the accent colour up to `LEVEL_WARN`, the share of the travel above
//! which a signal is close to clipping, and the warning colour above it, so a
//! hot signal is seen without reading a number. The caller measures the level
//! (a peak, from 0 to 1 along the dB scale it likes) and draws it with this.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The share of the bar above which the level is drawn in the warning colour.
pub const LEVEL_WARN: f32 = 0.9;

/// A bar at `level`, 0 to 1.
#[derive(PartialEq)]
pub struct LevelMeter {
    level: f32,
    label: Option<String>,
}

/// A level meter showing `level`, from 0 (silent) to 1 (full scale).
pub fn level_meter(level: f32) -> LevelMeter {
    LevelMeter {
        level: level.clamp(0.0, 1.0),
        label: None,
    }
}

impl LevelMeter {
    /// What assistive technology calls the meter.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Component for LevelMeter {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let level = self.level;
        let hot = (level - LEVEL_WARN).max(0.0);
        let cool = level - hot;
        rect()
            .component("LevelMeter")
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .maybe(self.label.is_some(), |element| {
                element.a11y_alt(self.label.clone().unwrap_or_default())
            })
            .a11y_builder(move |builder| {
                builder.set_numeric_value(f64::from(level));
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(1.0);
            })
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::SliderTrack),
                    rounded(Radius::Pill),
                    bg(Paint::Control),
                    clip(),
                    flex_row(),
                ],
            )
            .child(
                rect()
                    .width(Size::percent(cool * 100.0))
                    .tw(&tokens, &[h_full(), bg(Paint::Accent)]),
            )
            .child(
                rect()
                    .width(Size::percent(hot * 100.0))
                    .tw(&tokens, &[h_full(), bg(Paint::Warning)]),
            )
    }
}
