//! What a file view shows when there is no list: loading, empty, failed.

use bingux_files::ListError;
use freya::prelude::*;

use crate::controls::{SpinnerSize, spinner};
use crate::icons::Icon;
use crate::shell::status_page;
use crate::style::*;

/// Shown while a folder is being read.
pub fn loading_state() -> Element {
    rect()
        .tw_layout(&[w_full(), h_full(), center()])
        .child(Element::from(spinner(SpinnerSize::Large)))
        .into()
}

/// A folder with nothing to show. `hidden_items` says the folder has files
/// that "Show hidden files" would reveal, and `filtered` that a filter
/// removed them; both are offered as the reason.
pub fn empty_state(recent: bool, hidden_items: bool, filtered: bool) -> Element {
    let (title, description) = if recent {
        ("No recent files", "Files you open appear here.")
    } else if filtered {
        (
            "No matching files",
            "There are no files of the chosen type in this folder. Choose another type, or open another folder.",
        )
    } else if hidden_items {
        (
            "No files to show",
            "This folder only has hidden files. Choose Show hidden files in the view menu to see them.",
        )
    } else {
        ("Folder is empty", "There is nothing in this folder.")
    };
    Element::from(status_page(Icon::Folder, title).description(description))
}

/// A folder that could not be read, with the reason in plain words.
pub fn error_state(error: &ListError) -> Element {
    let (icon, title) = match error {
        ListError::PermissionDenied => (Icon::ChangesPrevent, "No permission"),
        ListError::NotFound => (Icon::DialogWarning, "Folder not found"),
        ListError::NotADirectory => (Icon::DialogWarning, "Not a folder"),
        ListError::Cancelled | ListError::Other(_) => (Icon::DialogError, "Cannot open folder"),
    };
    Element::from(status_page(icon, title).description(error.to_string()))
}
