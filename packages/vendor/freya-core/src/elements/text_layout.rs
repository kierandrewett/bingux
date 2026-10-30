//! How wide a paragraph is laid out, so that a line that reads right to left
//! is aligned to the right edge of the box the paragraph gets.

use freya_engine::prelude::SkParagraph;
use torin::{node::Node, size::Size};

/// Lay `paragraph` out `width` wide: `f32::MAX` for a single line that is not
/// to wrap.
///
/// Left to right it is just `layout(width)`: the text starts at the left of its
/// box, whatever the width. Right to left, Skia aligns a line to the right of
/// the *layout width*, so that has to be the width of the box the text is
/// drawn in or the line lands outside it:
///
/// * `room` is the width of the box when it is fixed (the node's width is a
///   size or fills its parent), `None` when the box is as wide as the text
///   (`inner_sized`), which is the widest line.
/// * An unwrapped line is laid out at its natural width first, then at the
///   wider of that and `room`, so it is aligned to the right of its box and,
///   when it is longer, starts at the left and overflows to the right, as it
///   does left to right.
/// * A wrapped paragraph in a box that is as wide as its text is laid out once
///   more at the width of its longest line, so its shorter lines are aligned
///   to the right edge of that line and not of the room it had.
pub(crate) fn layout_paragraph(
    paragraph: &mut SkParagraph,
    width: f32,
    room: Option<f32>,
    rtl: bool,
    inner_sized: bool,
) {
    if !rtl {
        paragraph.layout(width);
        return;
    }
    if width == f32::MAX {
        paragraph.layout(f32::MAX);
        let natural = paragraph.longest_line().ceil();
        let target = if inner_sized {
            natural
        } else {
            natural.max(room.unwrap_or(natural))
        };
        paragraph.layout(target);
        return;
    }
    paragraph.layout(width);
    if inner_sized {
        let lines = paragraph.line_number();
        let tight = paragraph.longest_line().ceil();
        if tight + 0.5 < width {
            paragraph.layout(tight);
            if paragraph.line_number() != lines {
                paragraph.layout(width);
            }
        }
    }
}

/// How the box a paragraph is drawn in relates to its text, as
/// [`layout_paragraph`] takes it: `(room, inner_sized)`.
///
/// `room` is the width of the box when it is not as wide as the text, `inner_sized`
/// whether the box is exactly as wide as the widest line. A box that is as wide as
/// its text but has a minimum width (a text field's text is at least as wide as the
/// field) is as wide as the room when the text is shorter, so it is not shrink-wrapped.
pub(crate) fn fit(node: &Node, content_width: f32) -> (Option<f32>, bool) {
    // `Inner` is the default: no minimum at all, like a minimum of zero pixels.
    let has_minimum = !matches!(node.minimum_width, Size::Inner)
        && !matches!(node.minimum_width, Size::Pixels(pixels) if pixels.get() == 0.0);
    let inner_sized = node.width.inner_sized() && !has_minimum;
    (
        (!inner_sized).then_some(content_width + 1.0),
        inner_sized,
    )
}
