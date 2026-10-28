//! The geometry of an attached tab and the path it is painted with.
//!
//! A selected tab is not a box with rounded corners. It is one outline: it
//! starts on the strip's bottom edge, curves up into the tab's left wall (the
//! *flare*, a concave quarter circle), rises, rounds the top left corner,
//! crosses, rounds the top right corner, comes down and flares out onto the
//! strip's bottom edge again. It is filled with the content's colour, so the
//! tab and the content under it are one surface, and stroked with a hairline
//! that stops where the fill meets the content: nothing is drawn along the
//! bottom, so there is no line between the tab and the content.
//!
//! [`Geometry`] holds every length, in logical pixels, from the tokens for one
//! density; [`Shape`] is the outline of one tab of a given width. The strip
//! and the tests both read them, so what is tested is what is drawn.

use freya::engine::prelude::{Canvas, Paint as Pen, PaintStyle, Path, PathBuilder, Rect as Oval};
use freya::prelude::*;

use crate::style::*;

/// The lengths a strip of tabs is laid out with, for one density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// The strip's height: the header bar's ([`Dim::Header`]). A tab stands on
    /// its bottom edge, which is the top edge of the content.
    pub strip: f32,
    /// A tab's height ([`Dim::Control`]): the height of a header button, and
    /// at compact density the touch minimum.
    pub tab: f32,
    /// What is left above a tab: the strip less the tab. It is the part of
    /// the bar that still drags the window.
    pub top: f32,
    /// The radius of a tab's two top corners ([`Radius::Control`]).
    pub radius: f32,
    /// The radius of the flare at a tab's foot: the top radius less the
    /// [`FIELD_INSET`] ([`Radius::nested`]), so a close button inset by that
    /// much from the tab's edge nests inside the tab's corner the same way.
    /// It is also the width of the foot, and the gap between two tabs.
    pub foot: f32,
    /// [`FIELD_INSET`]: the space between a tab's edge and what sits in it.
    pub inset: f32,
    /// The narrowest and widest a tab's body is.
    pub min: f32,
    pub max: f32,
    /// The side of the tab list and New tab buttons ([`Dim::InlineButton`]).
    pub button: f32,
    /// The width of the outline and of the strip's bottom edge.
    pub hairline: f32,
}

impl Geometry {
    pub(crate) fn current() -> Self {
        let inset = FIELD_INSET.px();
        Self {
            strip: Dim::Header.px(),
            tab: Dim::Control.px(),
            top: Dim::Header.px() - Dim::Control.px(),
            radius: Radius::Control.px(),
            foot: Radius::Control.nested(inset).px(),
            inset,
            min: Dim::TabMin.px(),
            max: Dim::Field.px(),
            button: Dim::InlineButton.px(),
            hairline: Dim::Hairline.px(),
        }
    }

    #[cfg(test)]
    pub(crate) fn in_class(class: SizeClass) -> Self {
        let inset = FIELD_INSET.px_in(class);
        Self {
            strip: Dim::Header.px_in(class),
            tab: Dim::Control.px_in(class),
            top: Dim::Header.px_in(class) - Dim::Control.px_in(class),
            radius: Radius::Control.px(),
            foot: Radius::Control.nested(inset).px(),
            inset,
            min: Dim::TabMin.px_in(class),
            max: Dim::Field.px_in(class),
            button: Dim::InlineButton.px_in(class),
            hairline: Dim::Hairline.px_in(class),
        }
    }

    /// The distance from one tab's body to the next: the foot the two share.
    pub(crate) fn gap(&self) -> f32 {
        self.foot
    }

    /// The width of the strip's content for `count` bodies of `tab`: a foot
    /// before the first and one after the last, and a gap between.
    pub(crate) fn content(&self, count: f32, tab: f32) -> f32 {
        self.foot + count * (tab + self.gap())
    }
}

/// The outline of one selected tab, in the coordinates of the box that is
/// painted: its top left is the tab's top left foot, and the box is
/// [`Shape::width`] wide and [`Geometry::tab`] tall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shape {
    /// The body's width: between the feet.
    pub body: f32,
    /// The box's height: the tab's.
    pub height: f32,
    /// The top corner radius, less than half the body when the tab is
    /// narrower than two corners.
    pub radius: f32,
    /// The flare radius, which is also the width of each foot.
    pub foot: f32,
    /// The line's width.
    pub line: f32,
}

impl Shape {
    pub(crate) fn new(geometry: &Geometry, body: f32) -> Self {
        Self {
            body,
            height: geometry.tab,
            radius: geometry.radius.min(body / 2.0).max(0.0),
            foot: geometry.foot,
            line: geometry.hairline,
        }
    }

    /// The painted box's width: the body and a foot on each side.
    pub(crate) fn width(&self) -> f32 {
        self.body + 2.0 * self.foot
    }

    /// The x of the tab's left wall and its right wall, in the box.
    pub(crate) fn walls(&self) -> (f32, f32) {
        (self.foot, self.foot + self.body)
    }

    /// Whether there is anything to draw: a tab that is still growing in has
    /// no room for its corners.
    pub(crate) fn is_drawn(&self) -> bool {
        self.body >= 2.0 * self.line
    }

    /// The outline along the middle of the line: from the left foot's bottom
    /// point, up the flare and the wall, over the top and down the right
    /// wall into the right foot's bottom point. With `closed` the path goes on
    /// along the box's bottom edge and back, which is the fill; without it the
    /// path stops at the feet, which is the line, so none is drawn along the
    /// tab's bottom.
    ///
    /// The middle of the line is half a line inside the edge, so the line's
    /// outer half lies on the strip and the whole tab, line included, fits the
    /// box; the feet start on the middle of the strip's own hairline.
    pub(crate) fn path(&self, closed: bool) -> Path {
        let half = self.line / 2.0;
        let (left, right) = self.walls();
        let (xl, xr) = (left + half, right - half);
        let (top, base) = (half, self.height - half);
        // The flare circle is tangent to the base at its bottom and to the
        // wall at its side, so its radius is the foot, on the middle line.
        let flare = self.foot + half;
        let flare_y = base - flare;
        let corner = (self.radius - half).max(0.0);
        let width = self.width();

        let oval = |cx: f32, cy: f32, r: f32| Oval::from_ltrb(cx - r, cy - r, cx + r, cy + r);
        let mut path = PathBuilder::new();
        path.move_to((0.0, base));
        path.arc_to(oval(0.0, flare_y, flare), 90.0, -90.0, false);
        path.line_to((xl, top + corner));
        path.arc_to(oval(xl + corner, top + corner, corner), 180.0, 90.0, false);
        path.line_to((xr - corner, top));
        path.arc_to(oval(xr - corner, top + corner, corner), 270.0, 90.0, false);
        path.line_to((xr, flare_y));
        path.arc_to(oval(width, flare_y, flare), 180.0, -90.0, false);
        if closed {
            path.line_to((width, self.height));
            path.line_to((0.0, self.height));
            path.close();
        }
        path.detach()
    }

    /// Paint the tab: `fill` is the content's colour, `line` the hairline's.
    pub(crate) fn paint(&self, canvas: &Canvas, fill: Color, line: Color) {
        if !self.is_drawn() {
            return;
        }
        let mut pen = Pen::default();
        pen.set_anti_alias(true);
        pen.set_style(PaintStyle::Fill);
        pen.set_color(fill);
        canvas.draw_path(&self.path(true), &pen);
        pen.set_style(PaintStyle::Stroke);
        pen.set_stroke_width(self.line);
        pen.set_color(line);
        canvas.draw_path(&self.path(false), &pen);
    }
}

/// A box that paints `shape` as its background: the fill in `fill`, the
/// outline in `line`. The box is as wide as the shape and a tab tall.
pub(crate) fn shape_box(shape: Shape, fill: Paint, line: Paint, tokens: &crate::theme::Tokens) -> Rect {
    let (fill, line) = (fill.color(tokens), line.color(tokens));
    painted(
        rect().interactive(false).tw(tokens, &[h(Dim::Control)]).width(Size::px(shape.width())),
        move |context| shape.paint(context.canvas, fill, line),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: [SizeClass; 2] = [SizeClass::Regular, SizeClass::Compact];

    #[test]
    fn the_strip_is_the_header_and_a_tab_stands_on_its_bottom_edge() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.strip, Dim::Header.px_in(class));
            assert_eq!(g.tab, Dim::Control.px_in(class));
            assert_eq!(g.top + g.tab, g.strip, "{class:?}: the tab ends where the strip does");
            assert!(g.top > 0.0, "{class:?}: there is a gap above the tabs to drag the window by");
        }
    }

    #[test]
    fn the_flare_radius_is_the_top_radius_nested_by_the_field_inset() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.foot, Radius::Control.nested(FIELD_INSET.px_in(class)).px());
            assert!(g.foot <= g.radius, "{class:?}: a flare is no rounder than the tab's corner");
            assert_eq!(g.radius, Radius::Control.px());
        }
        // 12 - 4 at mouse density; no inset at compact density, so no nesting.
        assert_eq!(Geometry::in_class(SizeClass::Regular).foot, 8.0);
        assert_eq!(Geometry::in_class(SizeClass::Compact).foot, 12.0);
    }

    #[test]
    fn the_corners_and_the_flares_fit_the_wall() {
        // The wall between the top corner and the flare must not be negative.
        for class in CLASSES {
            let g = Geometry::in_class(class);
            let shape = Shape::new(&g, g.min);
            let rise = g.tab - g.hairline - g.radius - g.foot;
            assert!(rise >= 0.0, "{class:?}: the wall is {rise} tall");
            assert_eq!(shape.width(), g.min + 2.0 * g.foot);
        }
    }

    #[test]
    fn the_outline_stays_in_its_box_and_ends_on_the_bottom_edge() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            for body in [g.min, 150.0, g.max] {
                let shape = Shape::new(&g, body);
                let fill = shape.path(true).bounds().to_owned();
                assert_eq!((fill.left, fill.right), (0.0, shape.width()));
                assert_eq!(fill.bottom, g.tab, "{class:?}: the fill reaches the strip's bottom edge");
                assert!(fill.top >= 0.0 && fill.top <= g.hairline);
                // The line opens and closes on the middle of the strip's bottom
                // hairline: half a line above the bottom edge.
                let line = shape.path(false);
                let last = line.last_pt().expect("a line has an end");
                let first = line.points().first().copied().expect("and a start");
                assert_eq!(first.y, g.tab - g.hairline / 2.0, "{class:?}: starts on the hairline");
                assert!((last.y - first.y).abs() < 0.001, "{class:?}: ends on the hairline");
                assert!((first.x - 0.0).abs() < 0.001 && (last.x - shape.width()).abs() < 0.001);
            }
        }
    }

    #[test]
    fn a_tab_narrower_than_its_corners_rounds_less_and_a_sliver_is_not_drawn() {
        let g = Geometry::in_class(SizeClass::Regular);
        assert_eq!(Shape::new(&g, 10.0).radius, 5.0);
        assert!(!Shape::new(&g, 1.0).is_drawn());
        assert!(Shape::new(&g, g.min).is_drawn());
    }
}
