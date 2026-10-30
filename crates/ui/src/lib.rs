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

pub mod about;
pub mod app;
pub mod nav;
pub mod preview;
pub mod app_icon;
pub mod charts;
pub mod contrast;
pub mod contrast_matrix;
pub mod controls;
pub mod elide;
pub mod files;
pub mod focus;
pub mod group;
pub mod icons;
#[cfg(feature = "chat-icons")]
pub mod chat_icons;
pub(crate) mod inspector;
pub mod component;
pub mod layout;
pub mod drag;
pub mod motion;
pub mod resilience;
pub(crate) mod popup_windows;
pub mod layers;
pub mod reorder;
pub mod rows;
pub mod runtime;
pub mod search;
pub mod search_highlight;
pub mod shell;
#[cfg(feature = "testing")]
pub mod snapshot;
pub mod stage;
pub mod stress;
pub mod style;
pub mod surface;
pub mod task;
pub mod theme;
pub mod typography;
mod warm;
#[cfg(all(test, feature = "testing"))]
mod mount_tests;
#[cfg(all(test, feature = "testing"))]
mod expander_tests;
pub mod window;

/// Everything an application window normally needs.
pub mod prelude {
    pub use crate::about::{AppInfo, app_menu};
    pub use crate::app::Application;
    pub use crate::nav::{
        Confirmation, Location, Mode, ModalKind, NavModel, Params, PopGuard, RouteKind, RouteTable,
        Router, Target, Transition, navigate, use_location, use_page_title, use_pop_guard,
        use_retained_state, use_router,
    };
    pub use crate::app_icon::{AppChoice, AppIcon, app_icon};
    pub use crate::charts::{
        Curve, Segment, Series, Unit, bar_chart, donut, line_chart, ring, sparkline,
    };
    pub use crate::controls::*;
    pub use crate::files::{self, ChooserRequest, Chosen};
    pub use crate::group::{
        Group, Status, StatusBlock, action_row, caption, centered_caption, form_row, group_head, heading,
        section_label, skeleton, skeleton_row, skeleton_rows, skeleton_shape, status,
    };
    pub use crate::icons::{Icon, icon};
    pub use crate::layout::{
        boxed, clamp, divider, hstack, page_column, popup_column, spacer, tile_rows, vstack,
    };
    pub use crate::motion::{
        AnimatedNumber, Crossfade, NumberFormat, Presence, Reveal, RevealBlock, animated_number, crossfade, presence, reveal,
        reveal_block,
        use_presence, use_tween,
    };
    pub use crate::resilience::{ErrorBoundary, Look as BoundaryLook, error_boundary, error_boundary_with};
    pub use crate::rows::{
        ChoiceList, EntryRow, ExpanderRow, LevelRow, Row, choice_list, choice_row, entry_row,
        expander_row, level_row, property_row, radio_row, radio_rows, row, slider_row, switch_row,
        text_row,
    };
    pub use crate::shell::{
        BackButton, Banner, BannerKind, CANCEL_ID, Dialog, DialogIcon, ForwardButton, HeaderBar,
        MenuButton, MenuItem, app_menu_item, app_menu_items,
        NavigationSplitView, OverlayHost, Overlays, PageAction, PageHead, Profile, ResponseKind,
        StatusPage, Toast, WindowControls, banner, dialog, framed_dialog, history_menu,
        menu_button, menu_item, menu_separator, status_page, submenu, use_overlays, use_page_busy,
    };
    pub use crate::search::{SearchState, use_search};
    pub use crate::style::*;
    pub use crate::surface::use_tokens;
    pub use crate::theme::{Scheme, SharedTokens, Tokens, provide_tokens};
    pub use crate::typography::{self, Ink, TextStyle, styled_label, styled_paragraph};
    pub use crate::window::{
        Popup, PopupAction, PopupActions, PopupFlags, Window, WindowHost, WindowLike, WindowParent,
        popup, use_main_window, use_popup_actions, use_window,
    };
}
