//! Wi-Fi signal strength as four rising bars (IconSize::Row square).
//!
//! Filled bars use the full text colour; empty bars use the muted colour at
//! about 30% opacity, so one to four bars read apart at a glance.

use freya::prelude::*;

use crate::style::*;
use crate::surface::use_tokens;

const BARS: u8 = 4;
// Each bar is one step taller than the last.
const BAR_STEP: f32 = 4.0;

/// Four signal bars. `level` is clamped to `0..=4`; `secured` adds a small
/// lock beside the bars is left to the caller, so this draws bars only.
pub fn signal_bars(level: u8, secured: bool) -> impl IntoElement {
    signal_bars_labelled(level, secured, None)
}

/// [`signal_bars`] with an accessible `label`, such as "Signal: strong", so a
/// screen reader announces the strength. Without one the bars are decoration.
pub fn signal_bars_labelled(level: u8, secured: bool, label: Option<String>) -> impl IntoElement {
    let tokens = use_tokens();
    let level = level.min(BARS);
    let _ = secured;
    let mut row = rect().tw(
        &tokens,
        &[
            w(Dim::IconBox),
            h(Dim::IconBox),
            flex_row(),
            items(Align::End),
            gap(S0_5),
        ],
    );
    for index in 0..BARS {
        let height = BAR_STEP * f32::from(index + 1);
        let bar = rect().height(Size::px(height)).tw(
            &tokens,
            &[
                w(Dim::SignalBar),
                rounded(Radius::Pill),
                bg(if index < level {
                    Paint::Text
                } else {
                    Paint::MutedFaint
                }),
            ],
        );
        row = row.child(bar);
    }
    match label {
        Some(text) => row.a11y_role(AccessibilityRole::Image).a11y_alt(text),
        None => row,
    }
}
