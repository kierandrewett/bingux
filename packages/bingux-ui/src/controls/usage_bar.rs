//! A thin bar for how full something is: a drive, a quota, a battery of
//! storage. It has no text. Put the figures beside it.

use freya::prelude::*;

use crate::component::ComponentName;
use crate::style::*;
use crate::surface::use_tokens;

/// The fraction at which a usage bar turns to the warning colour.
pub const USAGE_WARN: f32 = 0.9;

/// A 6 px track with a filled share. The share is accent coloured, and
/// turns to the warning colour once `fraction` reaches [`USAGE_WARN`].
/// `usage_bar(None)` draws the empty track, to hold the room for a figure
/// that is still being read.
#[derive(PartialEq)]
pub struct UsageBar {
    fraction: Option<f32>,
}

pub fn usage_bar(fraction: impl Into<Option<f32>>) -> UsageBar {
    UsageBar {
        fraction: fraction.into().map(|value| value.clamp(0.0, 1.0)),
    }
}

impl Component for UsageBar {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fraction = self.fraction;
        let fill = if fraction.is_some_and(|value| value >= USAGE_WARN) {
            Paint::Warning
        } else {
            Paint::Accent
        };
        rect()
            .component("UsageBar")
            .tw(
                &tokens,
                &[
                    w_full(),
                    h(Dim::Track),
                    rounded(Radius::Mark),
                    bg(Paint::Track),
                    clip(),
                ],
            )
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .a11y_builder(move |builder| {
                if let Some(value) = fraction {
                    builder.set_numeric_value(f64::from(value));
                    builder.set_min_numeric_value(0.0);
                    builder.set_max_numeric_value(1.0);
                }
            })
            .maybe_child(fraction.map(|value| {
                rect()
                    // The filled share is a run-time percentage, so a raw width.
                    .width(Size::percent(value * 100.0))
                    .tw(&tokens, &[h_full(), rounded(Radius::Mark), bg(fill)])
            }))
    }
}
