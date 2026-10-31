//! Window-level pieces: header bars, window controls, the navigation
//! split view, status pages, banners, dialogs and toasts.

#[cfg(all(test, feature = "testing"))]
mod app_menu_tests;
pub mod back_button;
pub mod banner;
pub mod dialog;
pub mod form_dialog;
#[cfg(all(test, feature = "testing"))]
mod header_bar_tests;
pub(crate) mod hide_clock;
pub mod menu;
#[cfg(all(test, feature = "testing"))]
mod menu_cascade_tests;
#[cfg(all(test, feature = "testing"))]
mod menu_first_frame_tests;
pub(crate) mod menu_nav;
#[cfg(all(test, feature = "testing"))]
mod menu_over_dialog_tests;
pub(crate) mod menu_query;
pub mod navigation;
#[cfg(all(test, feature = "testing"))]
mod notice_tests;
pub mod overlays;
#[cfg(all(test, feature = "testing"))]
mod page_motion_tests;
pub(crate) mod panel;
#[cfg(all(test, feature = "testing"))]
mod rest_fill_tests;
#[cfg(all(test, feature = "testing"))]
mod scroll_slide_tests;
pub mod sections;
pub(crate) mod sheet_gesture;
pub(crate) mod sheet_motion;
#[cfg(all(test, feature = "testing"))]
mod sheet_motion_frame_tests;
pub mod status_page;
pub(crate) mod tooltip_place;
pub(crate) mod type_ahead;
pub mod window;

pub use back_button::{BackButton, ForwardButton, history_menu, history_menu_with};
pub use banner::{Banner, BannerKind, banner};
pub use dialog::{
    CANCEL_ID, Dialog, DialogFrame, DialogIcon, DialogResponse, ResponseKind, dialog, framed_dialog,
};
pub use form_dialog::{FORM_HEIGHT, FORM_WIDTH, FormDialog, form_dialog, open_form};
pub use menu::{
    MenuButton, MenuItem, app_menu_item, app_menu_items, menu_button, menu_item, menu_separator,
    submenu,
};
pub use navigation::{NavigationSplitView, PageAction, PageHead, Profile, use_page_busy};
pub use overlays::{OverlayHost, Overlays, Toast, use_overlays};
pub use status_page::{StatusPage, status_page};
pub use window::{HeaderBar, InertControls, WindowControls, drag_on_pointer_down, with_window};
