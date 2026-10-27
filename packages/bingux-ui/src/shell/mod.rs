//! Window-level pieces: header bars, window controls, the navigation
//! split view, status pages, banners, dialogs and toasts.

pub mod menu;
pub(crate) mod menu_nav;
pub(crate) mod menu_query;
pub(crate) mod type_ahead;
pub mod navigation;
pub(crate) mod panel;
pub mod overlays;
pub mod sections;
pub mod status_page;
pub mod window;

pub use menu::{MenuButton, MenuItem, menu_button, menu_item, menu_separator, submenu};
pub use navigation::{
    NavItem, NavigationSplitView, PageAction, PageHead, Profile, SearchField, Subpage,
    use_page_busy,
};
pub use overlays::{
    Dialog, DialogFrame, DialogResponse, OverlayHost, Overlays, ResponseKind, Toast, dialog, framed_dialog,
    use_overlays,
};
pub use status_page::{Banner, BannerKind, StatusPage, banner, status_page};
pub use window::{HeaderBar, WindowControls, drag_on_pointer_down, with_window};
