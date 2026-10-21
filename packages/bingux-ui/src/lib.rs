//! Bingux UI: the design system for Bingux's Freya applications.
//!
//! The visual language is the Bingux shell's (`shell/bingux/*.qml`): one
//! quiet interaction surface, 36 x 20 switches, 6 px sliders, pill
//! secondary actions, segmented controls and coloured icon tiles. The
//! component vocabulary follows libadwaita (preference groups and rows,
//! header bars, status pages, navigation split views) and macOS System
//! Settings (sidebar search, profile card, icon tiles).
//!
//! Every component reads its colours from [`theme::Tokens`], provided by
//! [`theme::provide_tokens`] at the window root and read with
//! [`surface::use_tokens`].

pub mod controls;
pub mod group;
pub mod hero;
pub mod icons;
pub mod rows;
pub mod runtime;
pub mod shell;
pub mod surface;
pub mod theme;

/// Everything an application window normally needs.
pub mod prelude {
    pub use crate::controls::*;
    pub use crate::group::{Group, Status, StatusBlock, caption, heading, section_label, status};
    pub use crate::hero::{Hero, hero};
    pub use crate::icons::{Icon, icon, icon_tile};
    pub use crate::rows::{Row, choice_row, row, slider_row, switch_row, text_row};
    pub use crate::shell::{
        HeaderBar, NavItem, NavigationSplitView, ProfileCard, SearchField, WindowControls,
    };
    pub use crate::surface::{CORNER, ROW_HEIGHT, TALL_ROW_HEIGHT, use_tokens};
    pub use crate::theme::{Scheme, SharedTokens, Tokens, provide_tokens};
}
