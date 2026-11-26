//! What the browser says to the person and to the window.

use crate::model::shortcuts::Command;

/// A message for the person that is not part of a page: a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub error: bool,
    pub text: String,
    /// A button on the toast: its label and what it runs.
    pub action: Option<(String, Command)>,
}

/// Something only the window can do. The UI takes these after a command and
/// does them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    /// Put the cursor in the address bar, with its text selected.
    FocusAddress,
    /// Open the find bar and focus its field.
    OpenFind,
    OpenDownloads,
    OpenPageInfo,
    OpenMainMenu,
    /// Make the window fullscreen, or take it out of it.
    Fullscreen(bool),
    /// Start another browser window whose tabs are all private.
    NewPrivateWindow,
    /// Close the window: its last tab is gone.
    CloseWindow,
}
