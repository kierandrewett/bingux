//! What a page asks of the person that stops the page until it is answered: a
//! permission, a dialog, a password, a list, a file. The first prompt of a tab
//! is the one on screen.

mod auth;
mod dialog;
mod host;
mod picker;
mod queue;
mod select;
mod state;

use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::{
    AuthRequest, DialogRequest, PermissionRequest, PickerRequest, SelectRequest,
};

pub use auth::AuthPrompt;
pub use dialog::DialogPrompt;
pub use host::PromptHost;
pub use picker::PickerPrompt;
pub use queue::Prompts;
pub use select::SelectPrompt;
pub use state::DialogFields;

/// Something the page asked that waits for the person. The tab keeps them in
/// the order they came and shows the first.
#[derive(Clone, Debug, PartialEq)]
pub enum Prompt {
    Permission(RequestId, PermissionRequest),
    Dialog(RequestId, DialogRequest),
    Auth(RequestId, AuthRequest),
    Select(RequestId, SelectRequest),
    FilePicker(RequestId, PickerRequest),
}

impl Prompt {
    pub fn id(&self) -> RequestId {
        match self {
            Self::Permission(id, _)
            | Self::Dialog(id, _)
            | Self::Auth(id, _)
            | Self::Select(id, _)
            | Self::FilePicker(id, _) => *id,
        }
    }
}

#[cfg(feature = "snapshot")]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(feature = "snapshot")]
pub(crate) use snapshot::scenarios;
