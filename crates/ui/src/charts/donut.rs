//! Donut layout, without drawing: how wide the ring is in the room it has,
//! where the legend goes, how the centre text fits the hole, and which
//! segment a point of the ring is on.
//!
//! The ring is always a square. Beside the legend it is [`DONUT_SIZE`] (more
//! at a large text scale). Above the legend, in a narrow card or at the
//! compact density, it is as wide as the card up to [`DONUT_MAX`], and
//! centred. The ring's thickness is [`DONUT_STROKE_RATIO`] of its size, so
//! the proportions hold at every size.

use std::f32::consts::TAU;

use super::geometry::*;

/// How a donut is laid out in the width it was given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DonutLayout {
    /// The legend sits beside the ring; otherwise below it, full width.
    pub beside: bool,
    /// The side of the ring's square.
    pub size: f32,
    /// The ring's thickness.
    pub stroke: f32,
}

impl DonutLayout {
    /// The layout in `width` px. `compact` is the fingertip density, `gap`
    /// the space between the ring and the legend, and `text` the text scale
    /// (1 to 2), which grows the ring so its centre text keeps its size.
    pub(crate) fn new(width: f32, compact: bool, gap: f32, text: f32) -> Self {
        let beside_size = DONUT_SIZE * text;
        let beside = !compact && width >= beside_size + gap + DONUT_LEGEND_MIN * text;
        let size = if beside {
            beside_size
        } else {
            width.min(DONUT_MAX * text).max(0.0)
        };
        Self {
            beside,
            size,
            stroke: (size * DONUT_STROKE_RATIO)
                .round()
                .clamp(DONUT_STROKE_MIN, DONUT_STROKE_MAX),
        }
    }

    /// The radius of the stroke's centre line. The outer edge is a pixel in.
    pub(crate) fn radius(&self) -> f32 {
        self.size / 2.0 - self.stroke / 2.0 - 1.0
    }

    /// The diameter of the circle free inside the ring for text.
    pub(crate) fn hole(&self) -> f32 {
        (self.size - (self.stroke + DONUT_HOLE_MARGIN) * 2.0).max(0.0)
    }
}

/// One line of text in the hole, as the style gives it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fit<'a> {
    pub text: &'a str,
    /// The font size in px, with the text scale in it.
    pub px: f32,
    /// Line height as a multiple of the size.
    pub leading: f32,
}

/// Where one line of the centre text goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    /// The share of its size the text is set at, 1 or less.
    pub scale: f32,
    /// The top of the line's box, in the ring's pixels.
    pub top: f32,
    /// The width of the line's box: the hole's width where the line sits.
    pub width: f32,
}

/// The centre text of a donut or ring: the value and, if it fits, a caption.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Centre {
    pub value: Slot,
    pub caption: Option<Slot>,
}

/// Fits `value` and `caption` in the hole of a ring `size` px square, whose
/// free circle is `hole` px across. Both lines shrink together, down to
/// [`CENTRE_MIN_SCALE`], until each is narrower than the hole is at its
/// height; if they still do not fit, the caption goes and the value alone
/// shrinks. A line's width is estimated, a little over, so what fits here
/// does not wrap or clip when drawn.
pub(crate) fn centre_fit(size: f32, hole: f32, value: Fit<'_>, caption: Option<Fit<'_>>) -> Centre {
    let attempt = |caption: Option<Fit<'_>>| {
        let mut scale = 1.0_f32;
        loop {
            if let Some(centre) = place(size, hole, value, caption, scale) {
                return Some(centre);
            }
            if scale <= CENTRE_MIN_SCALE {
                return None;
            }
            scale = (scale - CENTRE_STEP).max(CENTRE_MIN_SCALE);
        }
    };
    caption
        .and_then(|caption| attempt(Some(caption)))
        .or_else(|| attempt(None))
        // Nothing fits at the smallest scale: the value at that scale, cut
        // to the hole's width by the label.
        .unwrap_or_else(|| {
            let height = value.px * value.leading * CENTRE_MIN_SCALE;
            Centre {
                value: Slot {
                    scale: CENTRE_MIN_SCALE,
                    top: (size - height) / 2.0,
                    width: hole.max(0.0),
                },
                caption: None,
            }
        })
}

/// The lines at `scale`, or `None` when one of them is wider than the hole at
/// its height.
fn place(
    size: f32,
    hole: f32,
    value: Fit<'_>,
    caption: Option<Fit<'_>>,
    scale: f32,
) -> Option<Centre> {
    let value_height = value.px * value.leading * scale;
    let caption_height = caption.map_or(0.0, |caption| caption.px * caption.leading * scale);
    let space = if caption.is_some() { CENTRE_GAP } else { 0.0 };
    let block = value_height + space + caption_height;
    let radius = hole / 2.0;
    if block > hole {
        return None;
    }
    let centre = size / 2.0;
    let top = centre - block / 2.0;
    let line = |top: f32, height: f32, fit: Fit<'_>| {
        // The chord at the line's edge that is farthest from the centre.
        let reach = (top - centre).abs().max((top + height - centre).abs());
        let chord = 2.0 * (radius * radius - reach * reach).max(0.0).sqrt();
        (text_width(fit.text, fit.px * scale) <= chord).then_some(Slot {
            scale,
            top,
            width: chord,
        })
    };
    let value_slot = line(top, value_height, value)?;
    let caption_slot = match caption {
        Some(caption) => Some(line(top + value_height + space, caption_height, caption)?),
        None => None,
    };
    Some(Centre {
        value: value_slot,
        caption: caption_slot,
    })
}

/// What a legend row is judged by, in px: the text sizes of its numbers and
/// name, the gap between a row's parts and the swatch's width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowSizes {
    pub value_px: f32,
    pub share_px: f32,
    pub name_px: f32,
    pub gap: f32,
    pub swatch: f32,
}

/// Whether the legend's rows stack their numbers under the name. A row has
/// the width of the legend; its numbers take what they need, and the name is
/// left the rest, which must hold [`DONUT_NAME_EMS`] of its own text. Large
/// text and a narrow card are what leave it less, and a name cut to two
/// letters says nothing. One row stacking stacks them all, so the legend
/// does not mix two layouts.
pub(crate) fn rows_stack<'a>(
    legend: f32,
    rows: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    sizes: RowSizes,
) -> bool {
    rows.into_iter().any(|(value, share)| {
        let numbers = text_width(value, sizes.value_px)
            + share.map_or(0.0, |share| sizes.gap + text_width(share, sizes.share_px));
        legend - numbers - sizes.gap - sizes.swatch < sizes.name_px * DONUT_NAME_EMS
    })
}

/// The index of the segment under `(x, y)`, in the ring's pixels, or `None`
/// when the point is in the hole's middle or off the disc. A finger is not
/// as exact as an arrow, so the whole band from the middle of the hole out
/// to the edge counts, not only the stroke. `values` are the segments'
/// non-negative values and `total` their sum.
pub(crate) fn segment_at(values: &[f64], total: f64, size: f32, x: f32, y: f32) -> Option<usize> {
    if total <= 0.0 {
        return None;
    }
    let centre = size / 2.0;
    let (dx, dy) = (x - centre, y - centre);
    let distance = dx.hypot(dy);
    if distance > centre || distance < centre * DONUT_HIT_INNER {
        return None;
    }
    // Clockwise from twelve o'clock, as the arcs are drawn.
    let angle = f64::from(dx.atan2(-dy).rem_euclid(TAU));
    let target = angle / f64::from(TAU) * total;
    let mut cumulative = 0.0;
    let mut last = None;
    for (index, &value) in values.iter().enumerate() {
        if value <= 0.0 {
            continue;
        }
        cumulative += value;
        last = Some(index);
        if target < cumulative {
            return Some(index);
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(text: &str, px: f32) -> Fit<'_> {
        Fit {
            text,
            px,
            leading: 1.3,
        }
    }

    #[test]
    fn beside_the_legend_on_a_wide_card_and_below_on_a_narrow_one() {
        let wide = DonutLayout::new(640.0, false, 16.0, 1.0);
        assert!(wide.beside);
        assert_eq!(wide.size, DONUT_SIZE);
        assert_eq!(wide.stroke, 14.0);
        // Not enough room for a legend of DONUT_LEGEND_MIN beside it.
        let narrow = DonutLayout::new(300.0, false, 16.0, 1.0);
        assert!(!narrow.beside);
        assert_eq!(narrow.size, 300.0_f32.min(DONUT_MAX));
        // The fingertip density always stacks, whatever the width.
        assert!(!DonutLayout::new(640.0, true, 18.0, 1.0).beside);
    }

    #[test]
    fn the_ring_never_outgrows_its_room_and_its_stroke_scales_with_it() {
        for width in [60.0, 120.0, 248.0, 296.0, 318.0, 1000.0] {
            let layout = DonutLayout::new(width, true, 18.0, 1.0);
            assert!(layout.size <= width.min(DONUT_MAX) + f32::EPSILON, "{width}: {layout:?}");
            assert!(layout.stroke >= DONUT_STROKE_MIN && layout.stroke <= DONUT_STROKE_MAX);
            assert!(layout.hole() > 0.0 || layout.size < 40.0, "{width}: {layout:?}");
        }
        let small = DonutLayout::new(120.0, true, 18.0, 1.0);
        let large = DonutLayout::new(200.0, true, 18.0, 1.0);
        assert!(small.stroke < large.stroke);
    }

    #[test]
    fn a_large_text_scale_grows_the_ring_and_the_room_it_needs_beside_the_legend() {
        let scaled = DonutLayout::new(500.0, false, 16.0, 2.0);
        assert!(!scaled.beside, "256 + 16 + 400 does not fit in 500");
        assert_eq!(scaled.size, 400.0_f32.min(500.0));
        assert!(DonutLayout::new(700.0, false, 16.0, 2.0).beside);
    }

    #[test]
    fn short_centre_text_keeps_its_size() {
        let layout = DonutLayout::new(640.0, false, 16.0, 1.0);
        let centre = centre_fit(layout.size, layout.hole(), fit("62%", 18.0), Some(fit("Used", 13.0)));
        assert_eq!(centre.value.scale, 1.0);
        assert_eq!(centre.caption.map(|slot| slot.scale), Some(1.0));
    }

    #[test]
    fn long_centre_text_shrinks_into_the_hole_and_never_past_it() {
        let layout = DonutLayout::new(120.0, true, 18.0, 1.0);
        let centre = centre_fit(
            layout.size,
            layout.hole(),
            fit("1,204.5 GiB", 19.0),
            Some(fit("Used of two terabytes", 14.0)),
        );
        assert!(centre.value.scale < 1.0, "{centre:?}");
        let value = fit("1,204.5 GiB", 19.0);
        let caption = fit("Used of two terabytes", 14.0);
        for (slot, line) in [Some((centre.value, value)), centre.caption.map(|slot| (slot, caption))]
            .into_iter()
            .flatten()
        {
            assert!(slot.width <= layout.hole(), "{slot:?} is no wider than the hole");
            assert!(
                text_width(line.text, line.px * slot.scale) <= slot.width,
                "{:?} fits the {} px it is given",
                line.text,
                slot.width
            );
        }
    }

    #[test]
    fn the_caption_goes_before_the_value_is_shrunk_to_nothing() {
        // A hole of 30 px has no room for two lines, even at the smallest scale.
        let centre = centre_fit(60.0, 30.0, fit("62%", 18.0), Some(fit("of capacity", 13.0)));
        assert!(centre.caption.is_none());
        assert!(centre.value.scale >= CENTRE_MIN_SCALE);
    }

    #[test]
    fn a_point_on_the_ring_is_on_the_segment_drawn_there() {
        let values = [50.0, 25.0, 0.0, 25.0];
        let size = 128.0;
        let at = |angle: f32| {
            let r = 50.0;
            segment_at(&values, 100.0, size, 64.0 + r * angle.sin(), 64.0 - r * angle.cos())
        };
        // Clockwise from twelve o'clock: the first half, then a quarter, then
        // the last quarter (the empty segment is skipped).
        assert_eq!(at(0.1), Some(0));
        assert_eq!(at(TAU * 0.49), Some(0));
        assert_eq!(at(TAU * 0.6), Some(1));
        assert_eq!(at(TAU * 0.9), Some(3));
        // The middle of the hole and the corners are no segment.
        assert_eq!(segment_at(&values, 100.0, size, 64.0, 64.0), None);
        assert_eq!(segment_at(&values, 100.0, size, 2.0, 2.0), None);
        assert_eq!(segment_at(&[], 0.0, size, 64.0, 10.0), None);
    }

    fn sizes(scale: f32) -> RowSizes {
        RowSizes {
            value_px: 15.0 * scale,
            share_px: 14.0 * scale,
            name_px: 14.0 * scale,
            gap: 14.0,
            swatch: 10.0,
        }
    }

    #[test]
    fn rows_stack_when_the_name_would_be_left_too_little_room() {
        let rows = [("210 GiB", Some("60.9%")), ("1.2 TiB", Some("57.0%"))];
        // A phone card at the regular text size leaves the name room.
        assert!(!rows_stack(248.0, rows, sizes(1.0)));
        // The same card at twice the text size does not: the numbers alone are
        // wider than half of it.
        assert!(rows_stack(248.0, rows, sizes(2.0)));
        // A wide legend holds even the large text on one line.
        assert!(!rows_stack(560.0, rows, sizes(2.0)));
    }
}
