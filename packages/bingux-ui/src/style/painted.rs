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

/// A checkerboard of square cells `cell` px across, `light` and `dark`, for
/// the box to paint behind a translucent colour so its alpha shows against a
/// pattern. The first cell, at the top left, is `light`. Cells are drawn
/// whole, clipped by the box's own rounded corners, so the pattern is the
/// same at every size and a track and a swatch share one look.
pub(crate) fn checker(light: Color, dark: Color, cell: f32) -> impl FnMut(&mut FillRenderContext) + 'static {
    use freya::engine::prelude::{Paint as Pen, PaintStyle, Rect as Cell};
    move |context| {
        let (width, height) = (context.size.width, context.size.height);
        let mut pen = Pen::default();
        pen.set_style(PaintStyle::Fill);
        pen.set_color(dark);
        context.canvas.draw_rect(Cell::from_xywh(0.0, 0.0, width, height), &pen);
        pen.set_color(light);
        let cell = cell.max(1.0);
        let (columns, rows) = ((width / cell).ceil() as usize, (height / cell).ceil() as usize);
        for row in 0..rows {
            for column in 0..columns {
                if (row + column) % 2 != 0 {
                    continue;
                }
                let (x, y) = (column as f32 * cell, row as f32 * cell);
                let cell = Cell::from_xywh(x, y, cell.min(width - x), cell.min(height - y));
                context.canvas.draw_rect(cell, &pen);
            }
        }
    }
}
