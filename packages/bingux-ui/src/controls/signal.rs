//! Wi-Fi signal strength as four rising bars (ICON_LG square).
//!
//! Filled bars use the full text colour; empty bars use the muted colour at
//! about 30% opacity, so one to four bars read apart at a glance.

use freya::prelude::*;

use crate::scale::*;
use crate::surface::use_tokens;

const BARS: u8 = 4;
const BAR_WIDTH: f32 = 3.0;
const BAR_GAP: f32 = 2.0;
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
    let mut row = rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::End)
        .width(Size::px(ICON_LG))
        .height(Size::px(ICON_LG));
    for index in 0..BARS {
        let height = BAR_STEP * f32::from(index + 1);
        let filled = index < level;
        let colour = if filled {
            tokens.text
        } else {
            tokens.muted.with_a(77)
        };
        row = row.child(
            rect()
                .width(Size::px(BAR_WIDTH))
                .height(Size::px(height))
                .margin(Gaps::new(
                    0.0,
                    if index == 0 { 0.0 } else { BAR_GAP },
                    0.0,
                    0.0,
                ))
                .corner_radius(1.0)
                .background(colour),
        );
    }
    match label {
        Some(text) => row.a11y_role(AccessibilityRole::Image).a11y_alt(text),
        None => row,
    }
}
