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
pub mod controls;
pub mod group;
pub mod icons;
pub mod layout;
pub mod rows;
pub mod runtime;
pub mod scale;
pub mod shell;
#[cfg(feature = "testing")]
pub mod snapshot;
pub mod surface;
pub mod theme;

/// Everything an application window normally needs.
pub mod prelude {
    pub use crate::app_icon::{AppIcon, app_icon};
    pub use crate::controls::*;
    pub use crate::group::{
        Group, Status, StatusBlock, action_row, caption, heading, section_label, skeleton, status,
    };
    pub use crate::icons::{Icon, icon};
    pub use crate::layout::{card, clamp, divider, hstack, page_column, spacer, vstack};
    pub use crate::rows::{
        EntryRow, ExpanderRow, Row, choice_row, entry_row, expander_row, property_row, row,
        slider_row, switch_row, text_row,
    };
    pub use crate::scale::*;
    pub use crate::shell::{
        Banner, BannerKind, Dialog, HeaderBar, NavItem, NavigationSplitView, OverlayHost, Overlays,
        ResponseKind, SearchField, StatusPage, WindowControls, banner, dialog, status_page,
        use_overlays,
    };
    pub use crate::surface::use_tokens;
    pub use crate::theme::{Scheme, SharedTokens, Tokens, provide_tokens};
}
