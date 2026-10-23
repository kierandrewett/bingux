//! Window-level pieces: header bars, window controls, the navigation
//! split view, status pages, banners, dialogs and toasts.

pub mod menu;
pub mod navigation;
pub mod overlays;
pub mod sheet;
pub mod status_page;
pub mod window;

pub use menu::{MenuButton, MenuItem, menu_button, menu_item, menu_separator};
pub use navigation::{
    NavItem, NavigationSplitView, PageAction, PageHeader, SearchField, SearchHit, Subpage,
    use_page_busy, use_page_header,
};
pub use overlays::{
    Dialog, DialogResponse, OverlayHost, Overlays, ResponseKind, Toast, dialog, use_overlays,
};
pub use sheet::{Sheet, SheetAction, sheet};
pub use status_page::{Banner, BannerKind, StatusPage, banner, status_page};
pub use window::{HeaderBar, WindowControls, drag_on_pointer_down, with_window};
