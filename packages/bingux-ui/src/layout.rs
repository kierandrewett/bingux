//! Layout helpers in SwiftUI's vocabulary: stacks, spacer, divider, card
//! and clamp. Each returns a plain `Rect` so callers keep chaining Freya's
//! builder methods.

use freya::prelude::*;

use crate::scale::*;
use crate::theme::Tokens;

/// A horizontal stack with vertically centred children and `spacing`.
pub fn hstack(spacing: f32) -> Rect {
    rect()
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .spacing(spacing)
}

/// A vertical stack with `spacing`.
pub fn vstack(spacing: f32) -> Rect {
    rect().spacing(spacing)
}

/// Fills the remaining space in a flex row or column. The parent needs
/// `.content(Content::Flex)`.
pub fn spacer() -> Rect {
    rect().width(Size::flex(1.0)).height(Size::flex(1.0))
}

/// A 1 px hairline in the text colour at 7% alpha, as between rows.
pub fn divider(tokens: &Tokens) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(1.0))
        .background(tokens.text.with_af32(0.07))
}

/// The group card surface with 16 px padding, for content that is not a
/// list of rows.
pub fn card(tokens: &Tokens) -> Rect {
    rect()
        .width(Size::fill())
        .corner_radius(RADIUS_LG)
        .background(tokens.card)
        .padding(S4)
}

/// Centres `child` and caps its width, like `AdwClamp`. Pages use 640 px.
pub fn clamp(max_width: f32, child: impl Into<Element>) -> Rect {
    rect()
        .width(Size::fill())
        .cross_align(Alignment::Center)
        .child(
            rect()
                .width(Size::fill())
                .max_width(Size::px(max_width))
                .child(child.into()),
        )
}

/// The root of a settings page: full width, with the standard 24 px between
/// groups. Groups and status rows carry no outer margin of their own.
pub fn page_column() -> Rect {
    rect().width(Size::fill()).spacing(S6)
}
