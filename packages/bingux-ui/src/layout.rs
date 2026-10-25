//! Layout helpers in SwiftUI's vocabulary: stacks, spacer, divider, box,
//! clamp, the page column and tile rows. Each returns plain `Rect`s styled
//! with tokens, so callers keep chaining Freya's builder methods.

use freya::prelude::*;

use crate::style::*;
use crate::theme::Tokens;

/// A horizontal stack with vertically centred children, `gap` apart.
pub fn hstack(space: Space) -> Rect {
    rect().tw_layout(&[flex_row(), items(Align::Center), gap(space)])
}

/// A vertical stack with children `gap` apart.
pub fn vstack(space: Space) -> Rect {
    rect().tw_layout(&[flex_col(), gap(space)])
}

/// Fills the remaining space in a flex row or column. The parent needs
/// `grow_children()`.
pub fn spacer() -> Rect {
    rect().tw_layout(&[w_flex(), h_flex()])
}

/// A 1 px hairline in the separator colour, as between panes.
pub fn divider(tokens: &Tokens) -> Rect {
    rect().tw(tokens, &[w_full(), h(Dim::Hairline), bg(Paint::Separator)])
}

/// A box holding free content: the shared box with 16 px padding.
pub fn boxed(tokens: &Tokens) -> Rect {
    rect().tw(tokens, BOX).tw(tokens, BOX_BODY)
}

/// Centres `child` and caps its width at `max`, like `AdwClamp`.
pub fn clamp(max: Dim, child: impl Into<Element>) -> Rect {
    rect().tw_layout(&[w_full(), items(Align::Center)]).child(
        rect()
            .tw_layout(&[w_full(), max_w(max)])
            .child(child.into()),
    )
}

/// The root of a popup's body: groups 24 px apart, closer than on a page so a
/// form stays in one view.
pub fn popup_column() -> Rect {
    rect().tw_layout(&[flex_col(), w_full(), gap(S6)])
}

/// The root of a settings page: groups 40 px apart. Groups and status
/// blocks carry no outer margin of their own.
pub fn page_column() -> Rect {
    rect().tw_layout(PAGE)
}

/// Lays `tiles` out in rows for a group, two tiles a row in the compact size
/// class (a phone) and four in the regular one, so an app gets both layouts
/// from one call. Every tile is `tile_height` tall, and the last row is
/// padded with empty cells so its tiles keep the width of the others.
pub fn tile_rows(tokens: &Tokens, tiles: Vec<Element>, tile_height: f32) -> Vec<Element> {
    let columns = if is_compact() { 2 } else { 4 };
    let mut tiles = tiles.into_iter().peekable();
    let mut rows = Vec::new();
    while tiles.peek().is_some() {
        let mut line = rect().tw(
            tokens,
            &[flex_row(), grow_children(), gap(S3), w_full(), p(S3)],
        );
        for _ in 0..columns {
            line = match tiles.next() {
                Some(tile) => line.child(
                    rect()
                        .width(Size::flex(1.))
                        .height(Size::px(tile_height))
                        .child(tile),
                ),
                None => line.child(rect().tw(tokens, &[w_flex()])),
            };
        }
        rows.push(line.into());
    }
    rows
}
