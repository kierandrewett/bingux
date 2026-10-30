//! The lengths of a colour picker, from the tokens, for one density.
//!
//! The picker lays itself out from these and nothing else, and a popover or a
//! sheet sizes itself from [`Geometry::height`] before the picker is drawn, so
//! what is placed is what is laid out: nothing in the picker changes size when
//! a colour is chosen, an entry is wrong or a recent colour is added. The
//! tests measure the drawn picker against these numbers.

use crate::style::*;

/// Swatches in a row of the palette and the recent colours.
pub const COLUMNS: usize = 6;

/// What a picker shows, as far as its height goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    /// The opacity strip and its box.
    pub alpha: bool,
    /// Swatches in the palette: the system accents and the presets.
    pub swatches: usize,
    /// The row of recent colours. Its room is kept whether or not any has
    /// been chosen yet, so the first one does not move what is below.
    pub recents: bool,
}

/// The lengths of one picker at one density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    pub class: SizeClass,
    /// The saturation and value plane's height.
    pub plane: f32,
    /// The knob the plane draws, and the target around it.
    pub knob: f32,
    pub handle: f32,
    /// A palette swatch's side.
    pub swatch: f32,
    /// A control's height: a slider's row, the hex field, a number box.
    pub control: f32,
    /// A line of caption text kept free: the hex message and the letters.
    pub caption: f32,
    /// The picker's width on a desktop. A phone's fills its sheet.
    pub width: f32,
    /// The space between groups, within a group, and between swatches.
    pub group: f32,
    pub inner: f32,
    pub between: f32,
    /// A footer or header row of buttons.
    pub action: f32,
}

impl Geometry {
    pub(crate) fn new(class: SizeClass) -> Self {
        Self {
            class,
            plane: Dim::PickerPlane.px_in(class),
            knob: Dim::PickerKnob.px_in(class),
            handle: Dim::PickerHandle.px_in(class),
            swatch: Dim::PickerSwatch.px_in(class),
            control: Dim::Control.px_in(class),
            caption: Dim::CaptionLine.px_in(class),
            width: Dim::PickerWidth.px_in(class),
            group: S3.px_in(class),
            inner: S2.px_in(class),
            between: S1.px_in(class),
            action: Dim::Button.px_in(class),
        }
    }

    /// Whether the picker is at a finger's density.
    pub(crate) fn compact(&self) -> bool {
        self.class.is_compact()
    }

    /// Rows the palette's `swatches` make.
    pub(crate) fn rows(swatches: usize) -> usize {
        swatches.div_ceil(COLUMNS)
    }

    /// The height of `rows` rows of swatches.
    pub(crate) fn swatch_rows(&self, rows: usize) -> f32 {
        if rows == 0 {
            return 0.0;
        }
        rows as f32 * self.swatch + (rows - 1) as f32 * self.between
    }

    /// The height of each group, top to bottom: the plane and strips, the hex
    /// row, the numbers, the palette and the recent colours (when shown).
    pub(crate) fn groups(&self, shape: Shape) -> Vec<f32> {
        // The plane and each strip are a gap apart.
        let strips = if shape.alpha { 2.0 } else { 1.0 };
        let mut groups = vec![
            self.plane + strips * (self.between + self.control),
            self.control + self.caption,
            self.control + self.inner + self.control + self.caption,
        ];
        if shape.swatches > 0 {
            groups.push(self.swatch_rows(Self::rows(shape.swatches)));
        }
        if shape.recents {
            groups.push(self.caption + self.swatch);
        }
        groups
    }

    /// The picker's height: its groups and the space between them.
    pub(crate) fn total(&self, shape: Shape) -> f32 {
        let groups = self.groups(shape);
        groups.iter().sum::<f32>() + groups.len().saturating_sub(1) as f32 * self.group
    }

    /// The Cancel and Done row, and the space that sets it off.
    pub(crate) fn actions(&self) -> f32 {
        self.group + self.action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPE: Shape = Shape {
        alpha: true,
        swatches: 12,
        recents: true,
    };

    #[test]
    fn a_picker_is_taller_with_alpha_and_recents_and_never_shorter_than_its_parts() {
        for class in [SizeClass::Regular, SizeClass::Compact] {
            let g = Geometry::new(class);
            let plain = g.total(Shape { alpha: false, swatches: 12, recents: false });
            let alpha = g.total(Shape { alpha: true, swatches: 12, recents: false });
            let full = g.total(SHAPE);
            assert_eq!(
                alpha - plain,
                g.control + g.between,
                "{class:?}: the opacity strip is a control tall and a gap from the hue strip"
            );
            assert!(full > alpha);
            assert!(plain > g.plane);
            // The palette is one row of swatches more or less.
            let fewer = g.total(Shape { alpha: false, swatches: 6, recents: false });
            assert_eq!(plain - fewer, g.swatch + g.between);
            assert_eq!(
                g.total(Shape { alpha: false, swatches: 0, recents: false }),
                g.groups(Shape { alpha: false, swatches: 0, recents: false }).iter().sum::<f32>()
                    + 2.0 * g.group
            );
        }
    }

    #[test]
    fn every_target_is_big_enough_for_its_density() {
        let desktop = Geometry::new(SizeClass::Regular);
        let phone = Geometry::new(SizeClass::Compact);
        assert!(desktop.handle >= 32.0 && desktop.swatch >= 32.0, "{desktop:?}");
        assert!(phone.handle >= TOUCH_MIN && phone.swatch >= TOUCH_MIN, "{phone:?}");
        assert!(phone.control >= TOUCH_MIN && phone.action >= TOUCH_MIN);
        // The knob is smaller than the target around it.
        assert!(desktop.knob < desktop.handle && phone.knob < phone.handle);
    }

    #[test]
    fn swatches_fall_into_rows_of_six() {
        assert_eq!(Geometry::rows(0), 0);
        assert_eq!(Geometry::rows(1), 1);
        assert_eq!(Geometry::rows(6), 1);
        assert_eq!(Geometry::rows(7), 2);
        assert_eq!(Geometry::rows(12), 2);
        assert_eq!(Geometry::rows(13), 3);
        let g = Geometry::new(SizeClass::Regular);
        assert_eq!(g.swatch_rows(0), 0.0);
        assert_eq!(g.swatch_rows(2), 2.0 * g.swatch + g.between);
    }
}
