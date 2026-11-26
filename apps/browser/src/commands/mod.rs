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
    /// Ask whether to forget the history, and forget it when the person says so.
    ClearHistory,
    /// Toggle the developer tools of the tab on show: open its toolbox docked
    /// under the page, or close it when it is open. A tab with no web page
    /// (blank, Settings) has nothing to inspect, so it does nothing there.
    DevTools,
    /// An editing command for the page, only while the page has the focus.
    Edit(EditCommand),
}

impl Command {
    /// The shortcut a menu and a palette show for the command, as the person
    /// reads it; `None` for a command with no key of its own. `Platform::window_command`
    /// is where the key is bound, and a test holds the two together.
    pub fn keys(self) -> Option<&'static str> {
        Some(match self {
            Command::NewTab => "Ctrl+T",
            Command::NewPrivateWindow => "Ctrl+Shift+P",
            Command::ReopenTab => "Ctrl+Shift+T",
            Command::CloseTab => "Ctrl+W",
            Command::NextTab => "Ctrl+Tab",
            Command::PreviousTab => "Ctrl+Shift+Tab",
            Command::FocusAddress => "Ctrl+L",
            Command::Find => "Ctrl+F",
            Command::Reload => "Ctrl+R",
            Command::Back => "Alt+Left",
            Command::Forward => "Alt+Right",
            Command::ZoomIn => "Ctrl+=",
            Command::ZoomOut => "Ctrl+-",
            Command::ZoomReset => "Ctrl+0",
            Command::Fullscreen => "F11",
            Command::Downloads => "Ctrl+Shift+Y",
            Command::ClearHistory => "Ctrl+Shift+Delete",
            Command::Settings => "Ctrl+,",
            Command::PageInfo => "Ctrl+I",
            Command::ViewSource => "Ctrl+U",
            Command::Print => "Ctrl+P",
            Command::DevTools => "F12",
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
