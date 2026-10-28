//! Window-level pieces: header bars, window controls, the navigation
//! split view, status pages, banners, dialogs and toasts.

pub mod back_button;
pub mod banner;
pub mod dialog;
pub mod menu;
#[cfg(all(test, feature = "testing"))]
mod menu_first_frame_tests;
#[cfg(all(test, feature = "testing"))]
mod notice_tests;
pub(crate) mod menu_nav;
pub(crate) mod menu_query;
pub(crate) mod type_ahead;
pub mod navigation;
pub(crate) mod panel;
pub(crate) mod sheet_gesture;
pub(crate) mod tooltip_place;
pub mod overlays;
pub mod sections;
pub mod status_page;
pub mod window;

pub use menu::{MenuButton, MenuItem, menu_button, menu_item, menu_separator, submenu};
pub use back_button::{BackButton, ForwardButton, history_menu};
pub use navigation::{NavigationSplitView, PageAction, PageHead, Profile, SearchField, use_page_busy};
pub use dialog::{
    CANCEL_ID, Dialog, DialogFrame, DialogIcon, DialogResponse, ResponseKind, dialog, framed_dialog,
};
pub use overlays::{OverlayHost, Overlays, Toast, use_overlays};
pub use banner::{Banner, BannerKind, banner};
pub use status_page::{StatusPage, status_page};
pub use window::{HeaderBar, InertControls, WindowControls, drag_on_pointer_down, with_window};
