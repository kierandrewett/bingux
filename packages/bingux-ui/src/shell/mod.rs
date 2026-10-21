//! Window-level pieces: header bars, window controls and the navigation
//! split view used by every Bingux application window.

pub mod navigation;
pub mod window;

pub use navigation::{NavItem, NavigationSplitView, ProfileCard, SearchField};
pub use window::{HeaderBar, WindowControls, drag_on_pointer_down, with_window};
