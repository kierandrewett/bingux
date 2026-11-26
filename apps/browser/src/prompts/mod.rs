//! What a page asks of the person that stops the page until it is answered: a
//! permission, a dialog, a password, a list, a file. The first prompt of a tab
//! is the one on screen.

mod host;

use bingux_ui::web_view::RequestId;
use bingux_ui::web_view::requests::{
    AuthRequest, DialogRequest, PermissionRequest, PickerRequest, SelectRequest,
};

pub use host::PromptHost;

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

#[cfg(test)]
mod tests;
