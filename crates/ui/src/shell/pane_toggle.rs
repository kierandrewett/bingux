//! The button that shows and hides a sidebar or a side pane, and the rule that
//! keeps it on the side of the window its panel is on.
//!
//! A toggle for a panel at the start of the window (the places of Files, the
//! pages of Preview, the calendars of Calendar) belongs at the start of the
//! header and draws the panel at the start edge. A toggle for a panel at the end
//! (Files' preview pane) belongs at the end and draws the panel at the end
//! edge. A person finds the button by looking at the side the panel will open
//! on, so a button on the wrong side, or one whose glyph shows the other side,
//! says the wrong thing before it is pressed.
//!
//! The rule is in the types rather than in each app:
//!
//! * [`pane_toggle`] takes the [`PanelSide`] of the panel it controls. The glyph
//!   is chosen from the side ([`PanelSide::icon`]); there is no way to ask for
//!   the other one.
//! * A [`PaneToggle`] is not a [`ChromeItem`](crate::shell::ChromeItem), so it
//!   does not go through [`HeaderBar::leading`](crate::shell::HeaderBar::leading)
//!   or `trailing`, where the cluster would be the app's choice. It goes to
//!   [`HeaderBar::pane_toggle`](crate::shell::HeaderBar::pane_toggle), which puts
//!   it in the cluster on its side; [`pane_toggle_item`](crate::toolbar::pane_toggle_item)
//!   does the same for a [`Toolbar`](crate::toolbar::Toolbar).
//! * A source scan (`pane_toggle_scan_tests`) fails on a sidebar glyph named
//!   anywhere else, so a toggle built from [`icon_button`](crate::controls::icon_button)
//!   by hand cannot get past `make check`.
//!
//! *Start* and *end* are the reading order, as everywhere in bingux-ui: in a
//! right to left interface the start is the right edge, the layout puts the
//! start slot there, and both glyphs are mirrored (`#mirror_rtl` in
//! `scripts/icons/labels.tsv`), so the toggle is still on its panel's side.

use freya::prelude::*;

use crate::controls::{IconButton, icon_button};
use crate::icons::Icon;
use crate::style::Dim;

/// The edge of the window a panel is attached to, in the reading order: the
/// left edge in a left to right interface, the right edge in a right to left
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    /// A sidebar: places, pages, calendars, a list of things to pick from.
    Start,
    /// A pane that shows the selection beside the content: a preview, details.
    End,
}

impl PanelSide {
    /// The glyph of a toggle for a panel on this side: the window with its panel
    /// drawn at the start edge or at the end edge.
    pub const fn icon(self) -> Icon {
        match self {
            PanelSide::Start => Icon::SidebarShow,
            PanelSide::End => Icon::SidebarShowRight,
        }
    }
}

/// A button that shows or hides the panel on `side`. Build it with
/// [`pane_toggle`] and give it to
/// [`HeaderBar::pane_toggle`](crate::shell::HeaderBar::pane_toggle) or to
/// [`pane_toggle_item`](crate::toolbar::pane_toggle_item).
///
/// It is deliberately not something [`HeaderBar::leading`](crate::shell::HeaderBar::leading)
/// takes: the side of the panel decides the slot, not the caller.
pub struct PaneToggle {
    side: PanelSide,
    button: IconButton,
}

/// A toggle for the panel on `side`. `on_press` shows or hides the panel.
///
/// Name it with [`tooltip`](PaneToggle::tooltip) ("Places", "Preview pane") and
/// say whether the panel is open with [`active`](PaneToggle::active), so the
/// button reads as on while the panel is shown.
pub fn pane_toggle(side: PanelSide, on_press: impl Into<EventHandler<()>>) -> PaneToggle {
    PaneToggle {
        side,
        button: icon_button(side.icon(), on_press),
    }
}

impl PaneToggle {
    /// The name of the panel, shown on hover and read out as the button's name.
    pub fn tooltip(mut self, text: impl AsRef<str>) -> Self {
        self.button = self.button.tooltip(text);
        self
    }

    /// Whether the panel is open: the button is drawn on while it is.
    pub fn active(mut self, open: bool) -> Self {
        self.button = self.button.active(open);
        self
    }

    /// The size of the button, for a toolbar row that is not a header bar.
    pub fn size(mut self, size: Dim) -> Self {
        self.button = self.button.size(size);
        self
    }

    /// The side of the window the panel, and so the button, is on.
    pub const fn side(&self) -> PanelSide {
        self.side
    }

    /// The button, for a host that places it itself: the bingux-ui toolbars
    /// that are not a [`HeaderBar`](crate::shell::HeaderBar) (the file chooser's
    /// row) and put the button at the edge on its side. An app never needs it.
    pub(crate) fn into_button(self) -> IconButton {
        self.button
    }
}
