//! Custom-drawn fills: the one place a box takes a background that is a
//! drawing and not a colour.
//!
//! A fill is a token colour, a gradient, or (here) a function that paints the
//! box's background on a canvas. The canvas is in logical pixels with its
//! origin at the box's top left, and drawing is clipped to the box, so a
//! shape that must reach past its parent's edge (the flare of a selected tab)
//! is given a box that holds all of it. Colours are read from the tokens by
//! the caller and captured by `draw`; nothing here names a colour.

use freya::prelude::*;

/// `rect` with its background drawn by `draw`, once per paint.
pub(crate) fn painted(rect: Rect, draw: impl FnMut(&mut FillRenderContext) + 'static) -> Rect {
    rect.background(Fill::Callback(RenderCallback::new(draw)))
}
