//! Bingux UI: the design system for Bingux's Freya applications.
//!
//! One visual system, the GNOME Settings one: opaque layered greys, boxed
//! preference groups of 50 px rows, monochrome sidebar icons, a single
//! accent colour and one control size. Accent colours follow the Bingux
//! shell's `Theme.qml`. `DESIGN.md` lists the rules every component keeps.
//!
//! Every component reads its colours from [`theme::Tokens`], provided by
//! [`theme::provide_tokens`] at the window root and read with
//! [`surface::use_tokens`].

pub mod app_icon;
pub mod charts;
pub mod controls;
pub mod group;
pub mod icons;
pub mod layout;
pub mod rows;
pub mod runtime;
pub mod search_highlight;
pub mod shell;
#[cfg(feature = "testing")]
pub mod snapshot;
pub mod style;
pub mod surface;
pub mod theme;
pub mod typography;

/// Everything an application window normally needs.
pub mod prelude {
    pub use crate::app_icon::{AppIcon, app_icon};
    pub use crate::charts::{
        Curve, Segment, Series, Unit, bar_chart, donut, line_chart, ring, sparkline,
    };
    pub use crate::controls::*;
    pub use crate::group::{
        Group, Status, StatusBlock, action_row, caption, centered_caption, form_row, heading,
        section_label, skeleton, status,
    };
    pub use crate::icons::{Icon, icon};
    pub use crate::layout::{card, clamp, divider, hstack, page_column, spacer, vstack};
    pub use crate::rows::{
        ChoiceList, EntryRow, ExpanderRow, LevelRow, Row, choice_list, choice_row, entry_row,
        expander_row, level_row, property_row, row, slider_row, switch_row, text_row,
    };
    pub use crate::shell::{
        Banner, BannerKind, Dialog, HeaderBar, MenuButton, MenuItem, NavItem, NavigationSplitView,
        OverlayHost, Overlays, PageAction, PageHeader, Profile, ResponseKind, SearchField, Sheet,
        SheetAction, StatusPage, WindowControls, banner, dialog, menu_button, menu_item,
        menu_separator, sheet, status_page, use_overlays, use_page_busy, use_page_header,
    };
    pub use crate::style::*;
    pub use crate::surface::use_tokens;
    pub use crate::theme::{Scheme, SharedTokens, Tokens, provide_tokens};
    pub use crate::typography::{self, Ink, TextStyle, styled_label, styled_paragraph};
}
