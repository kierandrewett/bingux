//! The lengths of a strip of tabs.
//!
//! A tab is a rounded rectangle of its own, not part of the content under it:
//! it stands [`FIELD_INSET`] from the strip's top and bottom, so it is as tall
//! as an end button and is the same distance from both edges. [`Geometry`]
//! holds every length, in logical pixels, from the tokens for one density. The
//! strip and the tests both read it, so what is tested is what is drawn.

use crate::style::*;

/// The lengths a strip of tabs is laid out with, for one density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// The strip's height ([`Dim::TabBar`]): 40 px, and 44 px at compact
    /// density. The strip is the window's title bar, so the window carries no
    /// second row above the tabs.
    pub strip: f32,
    /// A tab's height, and the side of an end button ([`Dim::TabButton`]): the
    /// strip less [`FIELD_INSET`] above and below, so a tab is the same
    /// distance from the strip's top and bottom. At compact density there is
    /// no inset and a tab is the whole 44 px touch target.
    pub tab: f32,
    /// What is left above a tab: [`FIELD_INSET`] (4 px, none at compact
    /// density). It is the part of the bar that still drags the window.
    pub top: f32,
    /// [`FIELD_INSET`]: the space between a tab's edge and what sits in it.
    pub inset: f32,
    /// The narrowest and widest a tab is.
    pub min: f32,
    pub max: f32,
    /// The space between two tabs ([`S1`]), and between the last tab and the
    /// end buttons.
    pub spacing: f32,
}

impl Geometry {
    /// The lengths at the current density.
    pub(crate) fn current() -> Self {
        let inset = FIELD_INSET.px();
        let strip = Dim::TabBar.px();
        Self {
            strip,
            tab: strip - 2.0 * inset,
            top: inset,
            inset,
            min: Dim::TabMin.px(),
            max: Dim::Field.px(),
            spacing: S1.px(),
        }
    }

    #[cfg(test)]
    pub(crate) fn in_class(class: SizeClass) -> Self {
        let inset = FIELD_INSET.px_in(class);
        let strip = Dim::TabBar.px_in(class);
        Self {
            strip,
            tab: strip - 2.0 * inset,
            top: inset,
            inset,
            min: Dim::TabMin.px_in(class),
            max: Dim::Field.px_in(class),
            spacing: S1.px_in(class),
        }
    }

    /// The distance between one tab and the next.
    pub(crate) fn gap(&self) -> f32 {
        self.spacing
    }

    /// The side of an end button (New tab, the tab list): a tab's height.
    pub(crate) fn button(&self) -> f32 {
        self.tab
    }

    /// The width of the cluster of `buttons` end buttons (New tab, the tab
    /// list) after the tabs: each is [`Geometry::button`] square, with the
    /// inset before the first and after the last. So a button is the same
    /// distance from the strip's top, its bottom and its trailing edge. At
    /// compact density there is no inset and each is the whole touch target.
    pub(crate) fn cluster(&self, buttons: usize) -> f32 {
        if buttons == 0 { 0.0 } else { buttons as f32 * self.button() + 2.0 * self.inset }
    }

    /// The width of the strip's content for `count` tabs of `tab`: a gap
    /// between each.
    pub(crate) fn content(&self, count: f32, tab: f32) -> f32 {
        count * (tab + self.gap()) - self.gap()
    }

    /// The tab width that makes `count` tabs fill `room` pixels: the inverse
    /// of [`Geometry::content`], not clamped.
    pub(crate) fn tab_in(&self, room: f32, count: f32) -> f32 {
        (room + self.gap()) / count - self.gap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: [SizeClass; 2] = [SizeClass::Regular, SizeClass::Compact];

    #[test]
    fn a_tab_is_as_far_from_the_strips_bottom_as_from_its_top() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.strip, Dim::TabBar.px_in(class), "{class:?}: the strip is a tab bar tall");
            assert_eq!(g.top, FIELD_INSET.px_in(class), "{class:?}: the band above the tabs is the field inset");
            assert_eq!(g.top + g.tab + g.top, g.strip, "{class:?}: the same band below");
            assert_eq!(g.tab, g.button(), "{class:?}: a tab is as tall as an end button, which is square");
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!((regular.strip, regular.tab, regular.top), (40.0, 32.0, 4.0));
        let compact = Geometry::in_class(SizeClass::Compact);
        assert_eq!((compact.strip, compact.tab, compact.top), (44.0, 44.0, 0.0), "a finger's tab is the whole strip");
        assert!(compact.tab >= TOUCH_MIN);
    }

    #[test]
    fn the_strip_is_taller_than_a_field_and_still_shorter_than_the_header_it_replaced() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert!(g.strip >= Dim::Control.px_in(class), "{class:?}: room for the tabs and the window's buttons");
            assert!(g.strip <= Dim::Header.px_in(class), "{class:?}: no second row above the tabs");
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!(Dim::Header.px_in(SizeClass::Regular) - regular.strip, 8.0, "8 px back for the window");
    }

    #[test]
    fn the_end_buttons_share_their_insets() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.cluster(0), 0.0);
            assert_eq!(g.cluster(1), g.button() + 2.0 * g.inset, "{class:?}: the inset before and after");
            assert_eq!(g.cluster(2), 2.0 * g.button() + 2.0 * g.inset, "{class:?}: two buttons share their insets");
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!((regular.tab, regular.cluster(1)), (32.0, 40.0));
        assert_eq!(Geometry::in_class(SizeClass::Compact).cluster(1), TOUCH_MIN, "flush on a phone");
    }

    #[test]
    fn tabs_are_a_gap_apart_and_the_width_inverts_the_content() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.gap(), S1.px_in(class));
            assert_eq!(g.content(1.0, 100.0), 100.0, "a lone tab has no gap");
            assert_eq!(g.content(3.0, 100.0), 300.0 + 2.0 * g.gap());
            for count in [1.0, 2.0, 7.0] {
                let tab = g.tab_in(500.0, count);
                assert!((g.content(count, tab) - 500.0).abs() < 0.001, "{class:?}: {count} tabs fill the room");
            }
        }
    }
}
