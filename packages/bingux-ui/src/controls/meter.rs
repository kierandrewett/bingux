//! The usage meter: one large figure with its unit, the total at the right,
//! and a thin bar beneath. Use it for a quantity that has a limit, such as
//! disk space used.

use freya::prelude::*;

use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, HERO, Ink};

/// `value` and `unit` are the large figure and its unit ("35", "GB used");
/// `total` is the muted text at the right ("256 GB total"). `fraction` is
/// the filled part of the bar, from 0.0 to 1.0.
#[derive(PartialEq)]
pub struct UsageMeter {
    value: String,
    unit: String,
    total: String,
    fraction: f64,
}

pub fn usage_meter(
    value: impl Into<String>,
    unit: impl Into<String>,
    total: impl Into<String>,
    fraction: f64,
) -> UsageMeter {
    UsageMeter {
        value: value.into(),
        unit: unit.into(),
        total: total.into(),
        fraction: fraction.clamp(0.0, 1.0),
    }
}

impl Component for UsageMeter {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let fraction = self.fraction;
        rect()
            .width(Size::fill())
            .tw(&tokens, &[flex_col(), px(GUTTER), py(S2), gap(S2)])
            .a11y_role(AccessibilityRole::ProgressIndicator)
            .a11y_builder(move |builder| {
                builder.set_numeric_value(fraction);
                builder.set_min_numeric_value(0.0);
                builder.set_max_numeric_value(1.0);
            })
            .child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[flex_row(), grow_children(), items(Align::End), gap(S2)],
                    )
                    .child(HERO.label(&tokens, self.value.clone()).max_lines(1))
                    .child(
                        rect()
                            .tw(&tokens, &[pb(S3)])
                            .child(CAPTION.in_ink(Ink::Text).label(&tokens, self.unit.clone())),
                    )
                    .child(rect().tw(&tokens, &[w_flex()]))
                    .child(
                        rect()
                            .tw(&tokens, &[pb(S3)])
                            .child(CAPTION.label(&tokens, self.total.clone())),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .tw(
                        &tokens,
                        &[
                            h(Dim::Track),
                            rounded(Radius::Mark),
                            bg(Paint::Control),
                            clip(),
                        ],
                    )
                    .child(
                        rect()
                            // The filled share is a run-time percentage, so a raw width.
                            .width(Size::percent(fraction as f32 * 100.0))
                            .tw(
                                &tokens,
                                &[h_full(), rounded(Radius::Mark), bg(Paint::Accent)],
                            ),
                    ),
            )
    }
}
