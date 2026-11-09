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

pub mod a11y;
#[cfg(all(test, feature = "testing"))]
mod a11y_tests;
pub mod about;
pub mod access;
pub mod app;
pub mod app_icon;
pub mod calendar;
pub mod charts;
#[cfg(feature = "chat-icons")]
pub mod chat_icons;
pub mod component;
pub mod contrast;
pub mod contrast_matrix;
pub mod controls;
pub mod crop;
pub mod document;
pub mod drag;
pub mod elide;
#[cfg(all(test, feature = "testing"))]
mod expander_tests;
pub mod files;
pub mod focus;
pub mod format;
pub mod group;
pub mod i18n;
pub mod icons;
pub mod input;
pub(crate) mod inspector;
pub mod layers;
pub mod layout;
pub mod list;
#[cfg(all(test, feature = "testing"))]
mod loading_shape_tests;
pub mod locale;
#[cfg(feature = "location")]
pub mod location;
pub mod media;
pub mod motion;
#[cfg(all(test, feature = "testing"))]
mod motion_stress;
#[cfg(all(test, feature = "testing"))]
mod mount_tests;
pub mod nav;
pub mod platform;
pub(crate) mod popup_windows;
pub mod preview;
pub mod reorder;
pub mod resilience;
pub mod rows;
pub mod runtime;
pub mod saved;
pub mod search;
pub mod search_highlight;
pub mod selection;
pub mod shell;
#[cfg(feature = "testing")]
pub mod snapshot;
pub mod stage;
pub mod stress;
pub mod style;
pub mod surface;
pub mod table;
pub mod tabs;
pub mod task;
pub mod text_editor;
pub mod theme;
pub mod toolbar;
pub mod typography;
mod warm;
pub mod window;
pub mod zoom_view;

// Drag and drop between windows and with other programs, and the typed
// clipboard. Not in the prelude: `Clipboard` would clash with Freya's.
pub mod clipboard;
pub mod dnd;
// Commands, shortcuts, menus from commands, the shortcuts sheet, the
// command palette, and undo (UiCommands).
pub mod commands;
pub mod undo;

/// Everything an application window normally needs.
pub mod prelude {
    pub use crate::a11y::{
        Announcer, FieldLabel, Politeness, Semantics, SemanticsExt, use_announcer, use_field_label,
    };
    pub use crate::about::{AppInfo, app_menu};
    pub use crate::app::{Application, Title};
    pub use crate::app_icon::{AppChoice, AppIcon, app_icon};
    pub use crate::charts::{
        Curve, Segment, Series, Unit, bar_chart, compass, day_arc, donut, line_chart, range_bar,
        ring, sparkline,
    };
    pub use crate::commands::{
        Command, CommandGroup, Commands, Shortcut, command_scope, use_commands, use_window_commands,
    };
    pub use crate::controls::*;
    pub use crate::files::{self, ChooserRequest, Chosen};
    pub use crate::group::{
        Group, Status, StatusBlock, action_row, caption, centered_caption, form_row, group_head,
        heading, section_label, skeleton, skeleton_caption, skeleton_row, skeleton_row_slot,
        skeleton_row_subtitled, skeleton_rows, skeleton_shape, status,
    };
    pub use crate::i18n::Message;
    pub use crate::icons::{Icon, icon};
    pub use crate::layout::{
        boxed, clamp, divider, hstack, page_column, popup_column, spacer, tile_rows, vstack,
    };
    pub use crate::list::{ListRow, ListView, SwipeAction, list_row, list_view, swipe_action};
    pub use crate::motion::{
        AnimatedNumber, Crossfade, NumberFormat, Presence, Reveal, RevealBlock, animated_number,
        crossfade, presence, reveal, reveal_block, use_presence, use_tween,
    };
    pub use crate::nav::{
        Confirmation, Location, ModalKind, Mode, NavModel, Params, PopGuard, RouteKind, RouteTable,
        Router, Target, Transition, navigate, use_location, use_page_title, use_pop_guard,
        use_retained_state, use_router,
    };
    pub use crate::resilience::{
        ErrorBoundary, Look as BoundaryLook, error_boundary, error_boundary_with,
    };
    pub use crate::rows::{
        ChoiceList, EntryRow, ExpanderRow, LevelRow, Row, choice_list, choice_row, entry_row,
        expander_row, level_row, property_row, radio_row, radio_rows, row, slider_row, switch_row,
        text_row,
    };
    pub use crate::search::{SearchState, use_search};
    pub use crate::selection::{Gesture, Selection, SelectionMode};
    pub use crate::shell::{
        BackButton, Banner, BannerKind, CANCEL_ID, Dialog, DialogIcon, ForwardButton, HeaderBar,
        MenuButton, MenuItem, NavigationSplitView, OverlayHost, Overlays, PageAction, PageHead,
        Profile, ResponseKind, StatusPage, Toast, WindowControls, app_menu_item, app_menu_items,
        banner, choice_sheet, dialog, framed_dialog, history_menu, menu_button, menu_item,
        menu_separator, status_page, submenu, use_overlays, use_page_busy,
    };
    pub use crate::style::*;
    pub use crate::surface::use_tokens;
    pub use crate::table::{
        BatchAction, CellEdit, Column, Columns, DataTable, RowDensity, RowSource, Rows,
        SortDirection, SortOrder, TableFailure, TableStatus, batch_action, data_table,
    };
    pub use crate::theme::{
        Scheme, SchemeScope, SharedTokens, Tokens, provide_tokens, scheme_scope,
    };
    pub use crate::toolbar::{
        ButtonGroup, GroupButton, Placement, Toolbar, ToolbarGroup, ToolbarItem, button_group,
        group_button, toolbar, toolbar_element, toolbar_group, toolbar_item, toolbar_menu,
    };
    pub use crate::typography::{self, Ink, TextStyle, styled_label, styled_paragraph};
    pub use crate::undo::{UndoStack, UndoStep, use_undo};
    pub use crate::window::{
        Popup, PopupAction, PopupActions, PopupFlags, Window, WindowHost, WindowLike, WindowParent,
        popup, use_main_window, use_popup_actions, use_window,
    };
    pub use crate::{i18n_id, tr};
}
