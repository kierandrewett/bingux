//! Keyboard shortcuts, as data: a key and the modifiers held, and the command
//! it is. The same table serves the window (a key typed anywhere in it) and
//! the page (a key typed while the page has the focus, where the page keeps
//! everything that is not a browser shortcut).
//!
//! The conventions are the platform's: Ctrl on Linux and Windows, Command on
//! macOS; Alt+1 to 9 for a tab on Linux and Ctrl+1 to 9 elsewhere, as Firefox
//! does.

mod keys;
mod menu_item;
mod run;

use bingux_ui::web_view::input::EditCommand;

pub use keys::Platform;

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
    MainMenu,
    /// An editing command for the page, only while the page has the focus.
    Edit(EditCommand),
}

impl Command {
    /// The shortcut shown beside a menu entry.
    pub fn label(self, platform: Platform) -> Option<String> {
        let main = platform.main_key();
        let join = |keys: &str| {
            if platform == Platform::Mac {
                keys.replace('+', "")
            } else {
                keys.to_string()
            }
        };
        let text = match self {
            Command::NewTab => format!("{main}+T"),
            Command::NewPrivateWindow => format!("{main}+Shift+P"),
            Command::CloseTab => format!("{main}+W"),
            Command::ReopenTab => format!("{main}+Shift+T"),
            Command::FocusAddress => format!("{main}+L"),
            Command::Find => format!("{main}+F"),
            Command::FindNext => "F3".to_string(),
            Command::FindPrevious => "Shift+F3".to_string(),
            Command::Reload => format!("{main}+R"),
            Command::ReloadBypassCache => format!("{main}+Shift+R"),
            Command::Back => {
                if platform == Platform::Mac {
                    format!("{main}+[")
                } else {
                    "Alt+Left".to_string()
                }
            }
            Command::Forward => {
                if platform == Platform::Mac {
                    format!("{main}+]")
                } else {
                    "Alt+Right".to_string()
                }
            }
            Command::ZoomIn if platform == Platform::Mac => return Some("\u{2318}+".to_string()),
            Command::ZoomIn => format!("{main}++"),
            Command::ZoomOut => format!("{main}+-"),
            Command::ZoomReset => format!("{main}+0"),
            Command::Fullscreen => "F11".to_string(),
            Command::Downloads => format!("{main}+Shift+Y"),
            Command::Settings => format!("{main}+,"),
            Command::PageInfo => format!("{main}+I"),
            Command::ViewSource => format!("{main}+U"),
            Command::Print => format!("{main}+P"),
            Command::NextTab => format!("{main}+Tab"),
            Command::PreviousTab => format!("{main}+Shift+Tab"),
            _ => return None,
        };
        Some(join(&text))
    }
}

#[cfg(test)]
mod tests;
