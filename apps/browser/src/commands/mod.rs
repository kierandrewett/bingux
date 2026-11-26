//! Keyboard shortcuts, as data: a key and the modifiers held, and the command
//! it is. The same table serves the window (a key typed anywhere in it) and
//! the page (a key typed while the page has the focus, where the page keeps
//! everything that is not a browser shortcut).
//!
//! The conventions are the platform's: Ctrl on Linux and Windows, Command on
//! macOS; Alt+1 to 9 for a tab on Linux and Ctrl+1 to 9 elsewhere, as Firefox
//! does.

mod keys;
mod registry;
mod run;

use bingux_ui::web_view::input::EditCommand;

pub use keys::Platform;
pub use registry::Registry;

/// What a shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewTab,
    NewPrivateWindow,
    CloseTab,
    ReopenTab,
    NextTab,
    PreviousTab,
    /// Tab `n`, counted from 1.
    GoToTab(usize),
    LastTab,
    MoveTabLeft,
    MoveTabRight,
    FocusAddress,
    Find,
    FindNext,
    FindPrevious,
    Reload,
    ReloadBypassCache,
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    Downloads,
    Settings,
    PageInfo,
    ViewSource,
    Print,
    /// Open the developer tools for the page. The engine cannot yet.
    DevTools,
    /// Play the page's video in a window of its own. The engine cannot yet.
    PictureInPicture,
    /// An editing command for the page, only while the page has the focus.
    Edit(EditCommand),
}

#[cfg(test)]
mod tests;
