//! Chart geometry: margins, tick spacing, text estimates and the plot box.
//! Every number a chart draws with lives here, named.

use crate::style::*;

/// The height a chart takes when none is given.
pub(crate) const DEFAULT_HEIGHT: f32 = 200.0;
/// Logical pixels per tick. The y axis gets a label every this much height;
/// the x axis every this much width. Labels that still collide are dropped.
pub(crate) const Y_TICK_SPACING: f32 = 40.0;
pub(crate) const X_TICK_SPACING: f32 = 96.0;
/// The margins round the plot, inside the chart's box. The card around a
/// chart owns the space between its edge and the chart; these keep the end
/// labels and the last dot inside the box.
pub(crate) const PLOT_TOP: f32 = S2.fixed_px();
pub(crate) const PLOT_RIGHT: f32 = S2.fixed_px();
/// The bottom holds the x labels: the gap above them, their line and a
/// little air below.
pub(crate) const PLOT_BOTTOM: f32 = LABEL_GAP + LABEL_HEIGHT + S1.fixed_px();
/// The left gutter for y labels is at least this wide and leaves this gap
/// between the labels and the plot.
pub(crate) const GUTTER_MIN: f32 = S8.fixed_px();
pub(crate) const GUTTER_GAP: f32 = S3.fixed_px();
/// One line of caption text, and the gap above x labels and value labels.
pub(crate) const LABEL_HEIGHT: f32 = 19.0;
pub(crate) const LABEL_GAP: f32 = S2.fixed_px();
/// Horizontal room a thinned x label needs beside its text.
pub(crate) const LABEL_PAD: f32 = S4.fixed_px();
/// Extra room on an estimated text width.
pub(crate) const GLYPH_SLACK: f32 = 1.08;
/// Extra room for semibold text over the same text measured as regular.
pub(crate) const SEMIBOLD_SLACK: f32 = 1.06;
/// Stroke of gridlines, axes and crosshairs. Half a pixel is added so they
/// land on pixel centres and stay sharp.
pub(crate) const HAIRLINE: f32 = 1.0;
/// Stroke of data lines.
pub(crate) const LINE_WIDTH: f32 = 2.0;
/// Resting and active dot radii.
pub(crate) const DOT_RADIUS: f32 = 2.5;
pub(crate) const ACTIVE_RADIUS: f32 = 4.0;
/// Up to this many points show a dot each.
pub(crate) const DOT_LIMIT: usize = 16;
/// Opacity of an area fill at its top edge (it fades to nothing at the base).
pub(crate) const FILL_TOP: f32 = 0.28;
pub(crate) const GRID_OPACITY: f32 = 0.3;
pub(crate) const BASELINE_OPACITY: f32 = 0.55;
pub(crate) const CROSSHAIR_OPACITY: f32 = 0.6;
/// How long a chart takes to draw in when it first appears.
pub(crate) const DRAW_MS: u64 = 700;
/// Bars: the share of a band a bar group fills, the widest bar, and the gap
/// between bars of one group.
pub(crate) const BAR_FILL: f32 = 0.72;
pub(crate) const BAR_MAX: f32 = 56.0;
pub(crate) const BAR_GAP: f32 = S1.fixed_px();
/// The shortest bar drawn. Anything smaller is zero and is skipped.
pub(crate) const MIN_BAR: f32 = 0.5;
/// Bars label their values when there are at most this many.
pub(crate) const VALUE_LABEL_MAX: usize = 8;
/// Sparklines: vertical room for their min and max labels.
pub(crate) const SPARK_PAD: f32 = S4.fixed_px();
pub(crate) const SPARK_HEIGHT: f32 = 72.0;
/// Rings and donuts.
pub(crate) const RING_SIZE: f32 = 120.0;
pub(crate) const RING_STROKE: f32 = 10.0;
pub(crate) const RING_TRACK_OPACITY: f32 = 0.18;
pub(crate) const RING_FROM_OPACITY: f32 = 0.55;
/// A donut beside its legend is this wide. Above its legend it is as wide as
/// the room it has, up to `DONUT_MAX` (both grow with the text scale).
pub(crate) const DONUT_SIZE: f32 = 128.0;
pub(crate) const DONUT_MAX: f32 = 200.0;
/// The legend beside a donut needs at least this much width.
pub(crate) const DONUT_LEGEND_MIN: f32 = 200.0;
/// The ring's thickness as a share of its size, and its limits.
pub(crate) const DONUT_STROKE_RATIO: f32 = 0.11;
pub(crate) const DONUT_STROKE_MIN: f32 = 6.0;
pub(crate) const DONUT_STROKE_MAX: f32 = 24.0;
/// The gap kept between the ring's inner edge and the centre text.
pub(crate) const DONUT_HOLE_MARGIN: f32 = S1.fixed_px();
/// The gap between two segments, in radians.
pub(crate) const DONUT_GAP: f32 = 0.05;
/// A legend row at the regular density. At the compact density a row is
/// `TOUCH_MIN` tall, a finger's target.
pub(crate) const DONUT_ROW: f32 = 36.0;
/// A press or hover this far from the middle, as a share of the radius,
/// or beyond, picks the segment it is on. See `donut::segment_at`.
pub(crate) const DONUT_HIT_INNER: f32 = 0.4;
/// The centre text shrinks by this step, down to this share of its size,
/// to fit the hole; the gap between its two lines.
pub(crate) const CENTRE_STEP: f32 = 0.05;
pub(crate) const CENTRE_MIN_SCALE: f32 = 0.55;
pub(crate) const CENTRE_GAP: f32 = S0_5.fixed_px();
/// A legend row keeps at least this many ems of its name's text size beside
/// the numbers. Short of that the row stacks: the name on one line and the
/// numbers under it.
pub(crate) const DONUT_NAME_EMS: f32 = 5.0;
/// Width of the box for a constant series' value label.
pub(crate) const POINT_LABEL_WIDTH: f32 = 72.0;
/// A gauge is a 270 degree arc opening at the bottom, starting at 225 degrees.
pub(crate) const GAUGE_START: f32 = 1.25 * PI_F32;
pub(crate) const GAUGE_SWEEP: f32 = 1.5 * PI_F32;
const PI_F32: f32 = std::f32::consts::PI;
/// Tooltips. The card is as wide as its text; these size it only to choose
/// the side of the stop it fits on.
pub(crate) const TIP_GAP: f32 = S3.fixed_px();
pub(crate) const TIP_ROW: f32 = 22.0;
pub(crate) const TIP_TITLE_ROW: f32 = 22.0;
pub(crate) const TIP_MIN_WIDTH: f32 = 120.0;
pub(crate) const TIP_LAYER: i16 = 1;
/// The colour marker in the legend and the tooltip.
pub(crate) const SWATCH: f32 = 10.0;
/// A zero minimum width for a flex child that must be allowed to shrink.
pub(crate) const NO_MIN: f32 = 0.0;
/// Space between the x labels and the legend.
pub(crate) const LEGEND_GAP: f32 = S4.fixed_px();

/// The plot rectangle, in the chart box's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plot {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Plot {
    /// The plot of a `width` by `height` box with a `gutter` on the left.
    pub fn new(width: f32, height: f32, gutter: f32) -> Self {
        Self {
            left: gutter,
            top: PLOT_TOP,
            right: (width - PLOT_RIGHT).max(gutter),
            bottom: (height - PLOT_BOTTOM).max(PLOT_TOP),
        }
    }

    pub fn width(self) -> f32 {
        self.right - self.left
    }

    pub fn height(self) -> f32 {
        self.bottom - self.top
    }
}

/// Estimated width of `text` at `size`, for layout before it is measured.
/// Each glyph is a class average in ems, a little over so a label never
/// wraps in a box sized from it.
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    let ems: f32 = text.chars().map(glyph_em).sum();
    ems * size * GLYPH_SLACK
}

/// The average advance of a glyph, in ems.
fn glyph_em(glyph: char) -> f32 {
    match glyph {
        '0'..='9' => 0.6,
        '%' => 0.95,
        ' ' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'i' | 'j' | 'l' | 'I' => 0.3,
        '-' | '(' | ')' | 'f' | 'r' | 't' => 0.4,
        'm' | 'w' | 'M' | 'W' => 0.9,
        'A'..='Z' => 0.7,
        _ => 0.57,
    }
}

/// How many ticks an axis of `length` can carry at `spacing` each.
pub(crate) fn ticks_for(length: f32, spacing: f32) -> usize {
    (length / spacing).floor().max(1.0) as usize
}

/// Pixel-snaps a 1 px line: centres it on a pixel.
pub(crate) fn crisp(value: f32) -> f32 {
    value.round() + 0.5
}
