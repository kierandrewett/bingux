//! Chart geometry: margins, tick spacing, text estimates and the plot box.
//! Every number a chart draws with lives here, named.

use crate::scale::*;

/// The height a chart takes when none is given.
pub(crate) const DEFAULT_HEIGHT: f32 = 200.0;
/// Logical pixels per tick. The y axis gets a label every this much height;
/// the x axis every this much width. Labels that still collide are dropped.
pub(crate) const Y_TICK_SPACING: f32 = 40.0;
pub(crate) const X_TICK_SPACING: f32 = 96.0;
/// The margins round the plot. The bottom holds the x labels.
pub(crate) const PLOT_TOP: f32 = S2;
pub(crate) const PLOT_RIGHT: f32 = S4;
pub(crate) const PLOT_BOTTOM: f32 = S6;
/// The left gutter for y labels is at least this wide and leaves this gap.
pub(crate) const GUTTER_MIN: f32 = S6;
pub(crate) const GUTTER_GAP: f32 = S2;
/// One line of caption text, and the gap above x labels and value labels.
pub(crate) const LABEL_HEIGHT: f32 = 16.0;
pub(crate) const LABEL_GAP: f32 = S1;
/// Horizontal room a thinned x label needs beside its text.
pub(crate) const LABEL_PAD: f32 = S4;
/// Average advance of a glyph in ems, for estimating text width. Set high
/// enough that "100%" at caption size never wraps in its gutter.
pub(crate) const GLYPH_EM: f32 = 0.85;
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
pub(crate) const GRID_OPACITY: f32 = 0.35;
pub(crate) const BASELINE_OPACITY: f32 = 0.5;
pub(crate) const CROSSHAIR_OPACITY: f32 = 0.6;
/// How long a chart takes to draw in when it first appears.
pub(crate) const DRAW_MS: u64 = 700;
/// Bars: the share of a band a bar group fills, the widest bar, and the gap
/// between bars of one group.
pub(crate) const BAR_FILL: f32 = 0.72;
pub(crate) const BAR_MAX: f32 = 56.0;
pub(crate) const BAR_GAP: f32 = S1;
/// The shortest bar drawn. Anything smaller is zero and is skipped.
pub(crate) const MIN_BAR: f32 = 0.5;
/// Bars label their values when there are at most this many.
pub(crate) const VALUE_LABEL_MAX: usize = 8;
/// Sparklines: vertical room for their min and max labels.
pub(crate) const SPARK_PAD: f32 = S4;
pub(crate) const SPARK_HEIGHT: f32 = 72.0;
/// Rings and donuts.
pub(crate) const RING_SIZE: f32 = 120.0;
pub(crate) const RING_STROKE: f32 = 10.0;
pub(crate) const RING_TRACK_OPACITY: f32 = 0.18;
pub(crate) const RING_FROM_OPACITY: f32 = 0.55;
pub(crate) const DONUT_SIZE: f32 = 128.0;
pub(crate) const DONUT_STROKE: f32 = 14.0;
pub(crate) const DONUT_GAP: f32 = 0.05;
pub(crate) const DONUT_ROW: f32 = 36.0;
/// Line height of the centre value inside a donut or ring hole (HEADING style).
pub(crate) const CENTRE_LINE: f32 = 22.0;
/// Width of the box for a constant series' value label.
pub(crate) const POINT_LABEL_WIDTH: f32 = 72.0;
/// A gauge is a 270 degree arc opening at the bottom, starting at 225 degrees.
pub(crate) const GAUGE_START: f32 = 1.25 * PI_F32;
pub(crate) const GAUGE_SWEEP: f32 = 1.5 * PI_F32;
const PI_F32: f32 = std::f32::consts::PI;
/// Tooltips.
pub(crate) const TIP_WIDTH: f32 = 176.0;
pub(crate) const TIP_GAP: f32 = S3;
pub(crate) const TIP_PAD: f32 = S3;
pub(crate) const TIP_ROW: f32 = 20.0;
pub(crate) const TIP_LAYER: i16 = 1;
pub(crate) const SWATCH: f32 = 8.0;
pub(crate) const SHADOW_Y: f32 = 8.0;
pub(crate) const SHADOW_BLUR: f32 = 24.0;
pub(crate) const SHADOW_OPACITY: f32 = 0.35;
pub(crate) const BORDER_WIDTH: f32 = 1.0;
/// Space between a chart and its legend.
pub(crate) const LEGEND_GAP: f32 = S3;

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
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * GLYPH_EM
}

/// How many ticks an axis of `length` can carry at `spacing` each.
pub(crate) fn ticks_for(length: f32, spacing: f32) -> usize {
    (length / spacing).floor().max(1.0) as usize
}

/// Pixel-snaps a 1 px line: centres it on a pixel.
pub(crate) fn crisp(value: f32) -> f32 {
    value.round() + 0.5
}
