//! Window-level pieces: header bars, window controls, the navigation
//! split view, status pages, banners, dialogs and toasts.

pub mod navigation;
pub mod overlays;
pub mod status_page;
pub mod window;

pub use navigation::{NavItem, NavigationSplitView, SearchField};
pub use overlays::{
    Dialog, DialogResponse, OverlayHost, Overlays, ResponseKind, Toast, dialog, use_overlays,
};
pub use status_page::{Banner, BannerKind, StatusPage, banner, status_page};
pub use window::{HeaderBar, WindowControls, drag_on_pointer_down, with_window};
