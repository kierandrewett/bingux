//! One rendered frame read back as pixels, and what changed between two.
//!
//! The state sweep asks pixel questions of a control's box: does it look
//! different under the pointer, different again while pressed, and is the
//! focus ring drawn on all four sides. The renderer's PNG is decoded once per
//! frame and every question is a count over a box of it.

use freya::engine::prelude::{SkData, SkImage, raster_n32_premul};
use torin::prelude::{Area, Point2D, Size2D};

use crate::snapshot::Snapshot;

/// A pixel counts as changed when one of its channels moved by at least this
/// much. A hover or pressed fill is several steps; the anti-aliasing of the
/// same drawing is none, because the renderer is deterministic.
pub(super) const VISIBLE_DELTA: u8 = 4;

/// The fewest changed pixels that can be a visible state change, and the
/// share of the box they must be, whichever is the larger need.
const MIN_PIXELS: usize = 8;
const MIN_SHARE: f32 = 0.005;

/// A frame as the renderer drew it.
pub(super) struct Frame {
    /// The PNG the renderer wrote, kept to show a frame a finding is about.
    png: Vec<u8>,
    bytes: Vec<u8>,
    row: usize,
    width: usize,
    height: usize,
    /// Physical pixels per logical pixel.
    scale: f32,
}

/// How much of a box differs between two frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Change {
    pub changed: usize,
    pub total: usize,
    /// The largest channel difference of any pixel.
    pub max_delta: u8,
}

impl Change {
    /// The share of the box that changed, 0 to 1.
    pub fn share(self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.changed as f32 / self.total as f32
        }
    }

    /// Whether a person would see the box change.
    pub fn visible(self) -> bool {
        let needed = (((self.total as f32) * MIN_SHARE).ceil() as usize).max(MIN_PIXELS);
        self.changed >= needed.min(self.total.max(1))
    }

    pub fn describe(self) -> String {
        format!(
            "{} of {} px changed ({:.1}%), at most by {}",
            self.changed,
            self.total,
            self.share() * 100.0,
            self.max_delta
        )
    }
}

impl Frame {
    /// Renders the window and decodes it. `window_width` (logical) tells the
    /// scale the frame was rendered at.
    pub fn of(shot: &mut Snapshot, window_width: f32) -> Self {
        let png = shot.runner().render();
        let image = SkImage::from_encoded(SkData::new_copy(&png)).expect("the frame decodes");
        let mut surface =
            raster_n32_premul((image.width(), image.height())).expect("a surface to read from");
        surface.canvas().draw_image(&image, (0, 0), None);
        let info = surface.canvas().image_info();
        let row = info.min_row_bytes();
        let mut bytes = vec![0u8; info.compute_byte_size(row)];
        assert!(surface.canvas().read_pixels(&info, &mut bytes, row, (0, 0)));
        Self {
            png: png.as_bytes().to_vec(),
            bytes,
            row,
            width: image.width() as usize,
            height: image.height() as usize,
            scale: if window_width > 0.0 { image.width() as f32 / window_width } else { 1.0 },
        }
    }

    /// Writes the frame to `BINGUX_UI_STATE_KEEP/NAME.png` when that is set,
    /// so a finding can be read against what was drawn.
    pub fn keep(&self, name: &str) {
        let Some(dir) = std::env::var_os("BINGUX_UI_STATE_KEEP") else { return };
        let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(80).collect();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), &self.png);
    }

    /// A frame of one flat colour, for the tests of this module.
    #[cfg(test)]
    pub fn flat(width: usize, height: usize, rgb: [u8; 3]) -> Self {
        let mut bytes = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            bytes.extend_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
        Self { png: Vec::new(), bytes, row: width * 4, width, height, scale: 1.0 }
    }

    /// Paints a logical box in one colour, for the tests of this module.
    #[cfg(test)]
    pub fn fill(&mut self, area: Area, rgb: [u8; 3]) {
        let (x0, x1, y0, y1) = self.span(area);
        for y in y0..y1 {
            for x in x0..x1 {
                let at = y * self.row + x * 4;
                self.bytes[at] = rgb[2];
                self.bytes[at + 1] = rgb[1];
                self.bytes[at + 2] = rgb[0];
            }
        }
    }

    /// The physical pixel span of a logical box, clamped to the frame.
    fn span(&self, area: Area) -> (usize, usize, usize, usize) {
        let clamp = |value: f32, max: usize| (value.round().max(0.0) as usize).min(max);
        (
            clamp(area.min_x() * self.scale, self.width),
            clamp(area.max_x() * self.scale, self.width),
            clamp(area.min_y() * self.scale, self.height),
            clamp(area.max_y() * self.scale, self.height),
        )
    }

    fn rgb(&self, x: usize, y: usize) -> [u8; 3] {
        // N32 on a little-endian machine is blue, green, red, alpha.
        let at = y * self.row + x * 4;
        [self.bytes[at + 2], self.bytes[at + 1], self.bytes[at]]
    }

    /// The pixel at a logical point.
    pub fn pixel(&self, x: f32, y: f32) -> [u8; 3] {
        let x = ((x * self.scale).round().max(0.0) as usize).min(self.width.saturating_sub(1));
        let y = ((y * self.scale).round().max(0.0) as usize).min(self.height.saturating_sub(1));
        self.rgb(x, y)
    }

    /// How much of `area` differs from the same area of `other`. Pixels
    /// outside the frame count as unchanged, so a box that is partly off the
    /// window is measured on its visible part and a box wholly outside has a
    /// total of 0.
    pub fn change(&self, other: &Frame, area: Area) -> Change {
        let (x0, x1, y0, y1) = self.span(area);
        let mut change = Change::default();
        for y in y0..y1 {
            for x in x0..x1 {
                change.total += 1;
                let (a, b) = (self.rgb(x, y), other.rgb(x, y));
                let delta = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
                if delta >= VISIBLE_DELTA {
                    change.changed += 1;
                }
                change.max_delta = change.max_delta.max(delta);
            }
        }
        change
    }

    /// The share of `area` whose pixels are the same in `other` and within
    /// `tolerance` of `rgb` on every channel in both: what a drawing would
    /// have changed, had it not been this colour already.
    pub fn unchanged_near(&self, other: &Frame, area: Area, rgb: [u8; 3], tolerance: u8) -> f32 {
        let (x0, x1, y0, y1) = self.span(area);
        let near = |pixel: [u8; 3]| pixel.iter().zip(&rgb).all(|(a, b)| a.abs_diff(*b) <= tolerance);
        let (mut same, mut total) = (0usize, 0usize);
        for y in y0..y1 {
            for x in x0..x1 {
                total += 1;
                let (a, b) = (self.rgb(x, y), other.rgb(x, y));
                if near(a) && near(b) {
                    same += 1;
                }
            }
        }
        if total == 0 { 0.0 } else { same as f32 / total as f32 }
    }
}

/// The four sides of a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

    pub fn name(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }
}

/// The strip of `side` of `face`, `from` to `to` pixels off its edge
/// (positive outside, negative inside), over the middle half of the side so
/// the corners' curves are not measured.
pub(super) fn strip(face: Area, side: Side, from: f32, to: f32) -> Area {
    let (w, h) = (face.width(), face.height());
    let (low, high) = (from.min(to), from.max(to));
    let (origin, size) = match side {
        Side::Top => (
            Point2D::new(face.min_x() + w * 0.25, face.min_y() - high),
            Size2D::new(w * 0.5, high - low),
        ),
        Side::Bottom => (
            Point2D::new(face.min_x() + w * 0.25, face.max_y() + low),
            Size2D::new(w * 0.5, high - low),
        ),
        Side::Left => (
            Point2D::new(face.min_x() - high, face.min_y() + h * 0.25),
            Size2D::new(high - low, h * 0.5),
        ),
        Side::Right => (
            Point2D::new(face.max_x() + low, face.min_y() + h * 0.25),
            Size2D::new(high - low, h * 0.5),
        ),
    };
    Area::new(origin, size)
}

/// The share of a ring's strip that must differ for the side to count as drawn.
pub(super) const SIDE_DRAWN: f32 = 0.5;

/// The strip of `side` of `face` a focus ring is drawn in: the band from the
/// gap to the gap and the width off the face for an outside ring, the band
/// just inside its edge for an inside one.
fn ring_band(face: Area, side: Side, inside: bool) -> Area {
    let gap = crate::motion::FOCUS_GAP;
    let width = crate::motion::FOCUS_WIDTH;
    if inside { strip(face, side, -width, 0.0) } else { strip(face, side, gap, gap + width) }
}

/// The sides of `face` on which a focus ring is drawn between `rest` and
/// `focused`: `(side, share of the strip that changed)`.
///
/// A ring drawn over a fill of its own colour (the accent ring over a
/// slider's accent fill) changes nothing there, yet is drawn: when `ring` is
/// the ring's colour, the pixels of a strip that already were that colour
/// count with the changed ones, once another side shows the ring.
pub(super) fn ring_sides(
    rest: &Frame,
    focused: &Frame,
    face: Area,
    inside: bool,
    ring: Option<[u8; 3]>,
) -> [(Side, f32); 4] {
    let mut sides = Side::ALL.map(|side| (side, rest.change(focused, ring_band(face, side, inside)).share()));
    if let Some(rgb) = ring
        && sides.iter().any(|(_, share)| *share >= SIDE_DRAWN)
    {
        for (side, share) in &mut sides {
            if *share < SIDE_DRAWN {
                *share += rest.unchanged_near(focused, ring_band(face, *side, inside), rgb, VISIBLE_DELTA * 2);
            }
        }
    }
    sides
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: f32, y: f32, w: f32, h: f32) -> Area {
        Area::new(Point2D::new(x, y), Size2D::new(w, h))
    }

    #[test]
    fn identical_frames_have_no_change() {
        let a = Frame::flat(40, 40, [200, 200, 200]);
        let b = Frame::flat(40, 40, [200, 200, 200]);
        let change = a.change(&b, area(0.0, 0.0, 40.0, 40.0));
        assert_eq!(change.changed, 0);
        assert!(!change.visible());
    }

    #[test]
    fn a_three_step_fill_is_not_visible_and_a_ten_step_one_is() {
        let a = Frame::flat(40, 40, [200, 200, 200]);
        let box_ = area(4.0, 4.0, 30.0, 30.0);
        let mut faint = Frame::flat(40, 40, [200, 200, 200]);
        faint.fill(box_, [197, 197, 197]);
        assert!(!a.change(&faint, box_).visible());
        let mut hover = Frame::flat(40, 40, [200, 200, 200]);
        hover.fill(box_, [190, 190, 190]);
        let change = a.change(&hover, box_);
        assert!(change.visible(), "{}", change.describe());
        assert_eq!(change.max_delta, 10);
    }

    #[test]
    fn a_ring_on_three_sides_is_missing_on_the_fourth() {
        let rest = Frame::flat(80, 80, [240, 240, 240]);
        let mut ring = Frame::flat(80, 80, [240, 240, 240]);
        let face = area(20.0, 20.0, 40.0, 40.0);
        // Top, bottom and left bands drawn; the right one clipped away.
        for side in [Side::Top, Side::Bottom, Side::Left] {
            ring.fill(strip(face, side, 2.0, 4.0), [20, 90, 220]);
        }
        let sides = ring_sides(&rest, &ring, face, false, None);
        for (side, share) in sides {
            if side == Side::Right {
                assert!(share < 0.1, "{side:?} {share}");
            } else {
                assert!(share > 0.9, "{side:?} {share}");
            }
        }
    }

    #[test]
    fn a_ring_over_a_fill_of_its_own_colour_is_drawn_there() {
        let blue = [20, 90, 220];
        let face = area(20.0, 20.0, 40.0, 40.0);
        // The page is grey and the fill to the left of the face is the ring's blue
        // already; the ring is then drawn on all four sides.
        let mut rest = Frame::flat(80, 80, [240, 240, 240]);
        rest.fill(strip(face, Side::Left, 2.0, 4.0), blue);
        let mut ring = Frame::flat(80, 80, [240, 240, 240]);
        for side in Side::ALL {
            ring.fill(strip(face, side, 2.0, 4.0), blue);
        }
        let plain = ring_sides(&rest, &ring, face, false, None);
        assert!(plain.iter().any(|(side, share)| *side == Side::Left && *share < 0.1), "{plain:?}");
        let merged = ring_sides(&rest, &ring, face, false, Some(blue));
        assert!(merged.iter().all(|(_, share)| *share > 0.9), "{merged:?}");
    }

    #[test]
    fn a_ring_the_colour_of_the_page_is_not_drawn_anywhere() {
        let blue = [20, 90, 220];
        let face = area(20.0, 20.0, 40.0, 40.0);
        let rest = Frame::flat(80, 80, [240, 240, 240]);
        let merged = ring_sides(&rest, &rest, face, false, Some(blue));
        assert!(merged.iter().all(|(_, share)| *share < 0.1), "{merged:?}");
    }
}
