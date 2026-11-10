//! The lengths of a strip of tabs.
//!
//! A tab is a floating tile on the strip, not part of the content under it:
//! [`Dim::TabButton`] tall (30 px), centred in the [`Dim::TabBar`] strip (40
//! px), so it is as tall as an end button and the same distance, [`top`], from
//! both edges. [`Geometry`] holds every length, in logical pixels, from the
//! tokens for one density. The strip and the tests both read it, so what is
//! tested is what is drawn.
//!
//! [`top`]: Geometry::top

use crate::style::*;

/// How tall the hairline between two unchosen tabs is.
pub(crate) const SEPARATOR_HEIGHT: f32 = 16.0;

/// The lengths a strip of tabs is laid out with, for one density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    /// The strip's height ([`Dim::TabBar`]): 40 px, and 44 px at compact
    /// density. The strip is the window's title bar, so the window carries no
    /// second row above the tabs.
    pub strip: f32,
    /// A tab's height, and the side of an end button ([`Dim::TabButton`]):
    /// 30 px, and the whole 44 px touch target at compact density.
    pub tab: f32,
    /// What is left above a tab, and below it: half of what the tab leaves of
    /// the strip, 5 px (none at compact density). It is the part of the bar that
    /// still drags the window.
    pub top: f32,
    /// The narrowest and widest a tab is ([`Dim::TabMin`], [`Dim::TabMax`]).
    pub min: f32,
    pub max: f32,
    /// A tab narrower than this has no room for a title and shows its icon
    /// only ([`Dim::TabTitleMin`]).
    pub title_min: f32,
    /// The space between two tabs ([`S0_5`]), and between the last tab and
    /// the end buttons.
    pub spacing: f32,
    /// The space before a tab's icon.
    pub lead: f32,
    /// The space between the parts of a tab (its icon, title and close
    /// button).
    pub part_gap: f32,
    /// A tab's icon ([`IconSize::Control`]).
    pub icon: f32,
    /// The mark of a document with unsaved changes ([`Dim::Dot`]).
    pub dot: f32,
    /// The close circle ([`Dim::TabClose`]): 16 px, centred in the tile, and
    /// the whole 44 px target at compact density, where the tab has no band
    /// around it.
    pub close: f32,
}

impl Geometry {
    /// The lengths at the current density.
    pub(crate) fn current() -> Self {
        Self::of(
            Dim::TabBar.px(),
            Dim::TabButton.px(),
            Dim::TabMin.px(),
            Dim::TabMax.px(),
            Dim::TabTitleMin.px(),
            S0_5.px(),
            S2.px(),
            S1.px(),
            IconSize::Control.px(),
            Dim::Dot.px(),
            Dim::TabClose.px(),
        )
    }

    #[cfg(test)]
    pub(crate) fn in_class(class: SizeClass) -> Self {
        Self::of(
            Dim::TabBar.px_in(class),
            Dim::TabButton.px_in(class),
            Dim::TabMin.px_in(class),
            Dim::TabMax.px_in(class),
            Dim::TabTitleMin.px_in(class),
            S0_5.px_in(class),
            S2.px_in(class),
            S1.px_in(class),
            IconSize::Control.px_in(class),
            Dim::Dot.px_in(class),
            Dim::TabClose.px_in(class),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn of(
        strip: f32,
        tab: f32,
        min: f32,
        max: f32,
        title_min: f32,
        spacing: f32,
        lead: f32,
        part_gap: f32,
        icon: f32,
        dot: f32,
        close: f32,
    ) -> Self {
        let top = (strip - tab) / 2.0;
        Self {
            strip,
            tab,
            top,
            min,
            max,
            title_min,
            spacing,
            lead,
            part_gap,
            icon,
            dot,
            close,
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
    /// list) after the tabs: each is [`Geometry::button`] square, a gap after
    /// the last tab before the first and the band after the last, so a button
    /// is the same distance from the strip's top, its bottom and its trailing
    /// edge. At compact density there is no band and each is the whole touch
    /// target.
    pub(crate) fn cluster(&self, buttons: usize) -> f32 {
        if buttons == 0 {
            0.0
        } else {
            buttons as f32 * self.button() + self.gap() + self.top
        }
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

    /// The space around a tab's close circle inside the tile: 7 px, none at
    /// compact density, where the button is the whole tab.
    pub(crate) fn pad(&self) -> f32 {
        (self.tab - self.close) / 2.0
    }

    /// The width of a tab less its title: the space before the icon, the icon
    /// (if it has one), the dot of unsaved changes (if any), the gaps between
    /// the parts and the close button's slot with the band after it. A title
    /// shown in a tab of `width` has `width - fixed` to be drawn in.
    pub(crate) fn fixed(&self, icon: bool, edited: bool) -> f32 {
        let parts = 2 + usize::from(icon) + usize::from(edited);
        let marks = if icon { self.icon } else { 0.0 } + if edited { self.dot } else { 0.0 };
        self.lead + marks + (parts - 1) as f32 * self.part_gap + self.close + self.pad()
    }

    /// A tab's natural width: everything in it at its full length, `title`
    /// being the width of the whole title, and no more than [`Geometry::max`].
    pub(crate) fn natural(&self, title: f32, icon: bool, edited: bool) -> f32 {
        (self.fixed(icon, edited) + title.ceil() + 1.0).clamp(self.min, self.max)
    }

    /// Whether a tab of `width` shows only its icon. A tab narrower than
    /// [`Geometry::title_min`] has no room for a title, but one that is as wide
    /// as it needs (`natural`, the width its longest title asks for) shows it
    /// whole however short it is: the title is dropped only when the strip
    /// has cut it. A tab without an icon has nothing else to show and keeps its
    /// title.
    pub(crate) fn icon_only(&self, width: f32, icon: bool, natural: f32) -> bool {
        icon && width < self.title_min.min(natural - 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASSES: [SizeClass; 2] = [SizeClass::Regular, SizeClass::Compact];

    #[test]
    fn a_tab_floats_the_same_distance_from_the_strips_top_and_bottom() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(
                g.strip,
                Dim::TabBar.px_in(class),
                "{class:?}: the strip is a tab bar tall"
            );
            assert_eq!(g.top + g.tab + g.top, g.strip, "{class:?}: an even band");
            assert_eq!(
                (g.strip - g.tab) % 2.0,
                0.0,
                "{class:?}: the band is a whole number of pixels either side"
            );
            assert_eq!(
                g.tab,
                g.button(),
                "{class:?}: a tab is as tall as an end button, which is square"
            );
            assert!(g.close <= g.tab, "{class:?}: the close circle fits a tab");
            assert!(
                g.min < g.title_min && g.title_min < g.max,
                "{class:?}: icon only, then titled, then the widest"
            );
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!((regular.strip, regular.tab, regular.top), (40.0, 30.0, 5.0));
        assert_eq!(
            (regular.close, regular.min, regular.title_min, regular.max),
            (16.0, 36.0, 96.0, 200.0)
        );
        let compact = Geometry::in_class(SizeClass::Compact);
        assert_eq!(
            (compact.strip, compact.tab, compact.top, compact.close),
            (44.0, 44.0, 0.0, 44.0),
            "a finger's tab is the whole strip, and its close circle the whole tab"
        );
        assert!(compact.tab >= TOUCH_MIN);
        assert_eq!(Radius::Tab.px(), 10.0, "a tile of about 10 px");
        assert!(
            SEPARATOR_HEIGHT < regular.tab,
            "a separator is shorter than the tabs it stands between"
        );
    }

    #[test]
    fn the_strip_is_taller_than_a_field_and_still_shorter_than_the_header_it_replaced() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert!(
                g.strip >= Dim::Control.px_in(class),
                "{class:?}: room for the tabs and the window's buttons"
            );
            assert!(
                g.strip <= Dim::Header.px_in(class),
                "{class:?}: no second row above the tabs"
            );
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!(
            Dim::Header.px_in(SizeClass::Regular) - regular.strip,
            8.0,
            "8 px back for the window"
        );
    }

    #[test]
    fn the_end_buttons_follow_the_last_tab_and_keep_the_band_behind_them() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.cluster(0), 0.0);
            assert_eq!(
                g.cluster(1),
                g.gap() + g.button() + g.top,
                "{class:?}: a gap after the last tab and the band after the button"
            );
            assert_eq!(
                g.cluster(2) - g.cluster(1),
                g.button(),
                "{class:?}: a second button adds only itself"
            );
        }
        let regular = Geometry::in_class(SizeClass::Regular);
        assert_eq!((regular.tab, regular.cluster(1)), (30.0, 37.0));
    }

    #[test]
    fn tabs_are_a_gap_apart_and_the_width_inverts_the_content() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            assert_eq!(g.gap(), S0_5.px_in(class));
            assert_eq!(g.content(1.0, 100.0), 100.0, "a lone tab has no gap");
            assert_eq!(g.content(3.0, 100.0), 300.0 + 2.0 * g.gap());
            for count in [1.0, 2.0, 7.0] {
                let tab = g.tab_in(500.0, count);
                assert!(
                    (g.content(count, tab) - 500.0).abs() < 0.001,
                    "{class:?}: {count} tabs fill the room"
                );
            }
        }
    }

    #[test]
    fn a_tabs_natural_width_is_what_is_in_it_and_never_more_than_the_cap() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            let short = g.natural(40.0, true, false);
            assert_eq!(
                short,
                (g.fixed(true, false) + 41.0).max(g.min),
                "{class:?}: icon, gaps, title, close button and no more"
            );
            assert!(short < g.max, "{class:?}: a short title is a short tab");
            assert_eq!(
                g.natural(5000.0, true, false),
                g.max,
                "{class:?}: a long title is cut at the cap"
            );
            assert_eq!(g.max, 200.0);
            assert!(
                g.natural(60.0, true, true) > g.natural(60.0, true, false),
                "{class:?}: the mark of unsaved changes takes room"
            );
            assert!(
                g.natural(60.0, false, false) < g.natural(60.0, true, false),
                "{class:?}: a tab without an icon is narrower"
            );
            assert!(g.natural(0.0, true, false) >= g.min);
        }
    }

    #[test]
    fn a_tab_gives_up_its_title_under_the_title_minimum_when_the_strip_has_cut_it() {
        for class in CLASSES {
            let g = Geometry::in_class(class);
            let natural = g.max;
            assert!(
                !g.icon_only(g.title_min, true, natural),
                "{class:?}: at the minimum"
            );
            assert!(
                g.icon_only(g.title_min - 1.0, true, natural),
                "{class:?}: under it"
            );
            assert!(g.icon_only(g.min, true, natural), "{class:?}: the floor");
            assert!(
                !g.icon_only(g.min, false, natural),
                "{class:?}: nothing else to show without an icon"
            );
            // A tab that is as wide as it needs keeps a short title, even under the minimum.
            let short = g.title_min - 20.0;
            assert!(
                !g.icon_only(short, true, short),
                "{class:?}: whole title, not cut"
            );
            assert!(g.icon_only(short - 2.0, true, short), "{class:?}: cut");
        }
    }
}
