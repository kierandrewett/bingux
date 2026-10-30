//! What a file view shows when there is no list: loading, empty, failed.

use bingux_files::ListError;
use freya::prelude::*;

use super::file_icon::FileKind;
use crate::component::ComponentName;
use crate::controls::{SpinnerSize, action_button, spinner};
use crate::icons::Icon;
use crate::shell::status_page;
use crate::style::*;
use crate::surface::use_tokens;

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
    Element::from(status_page(FileKind::Folder.glyph(), title).description(description))
}

/// A folder whose read was stopped before anything was in: say so, and how
/// to read it.
pub fn stopped_state() -> Element {
    Element::from(
        status_page(Icon::MediaPlaybackStop, "Loading stopped")
            .description("Press Refresh (F5) to read this folder."),
    )
}

/// The Trash with nothing in it: a status page, not an empty list.
pub fn trash_empty_state() -> Element {
    Element::from(
        status_page(Icon::UserTrash, "Trash is empty")
            .description("Things you move to the Trash appear here until you empty it."),
    )
}

/// A search with no results. `running` is `Some(true)` while a search of
/// folders still runs, `Some(false)` when it has ended and `None` for the
/// folder on show. `invalid` is why the search cannot run at all. `hints` are
/// what a person can do about it: a button each.
pub fn no_results_state(
    words: &str,
    place: &str,
    running: Option<bool>,
    invalid: Option<&str>,
    hints: Vec<(String, EventHandler<()>)>,
) -> Element {
    if let Some(reason) = invalid {
        return Element::from(status_page(Icon::DialogWarning, "Cannot search").description(reason.to_string()));
    }
    let (title, description) = match running {
        Some(true) => ("Searching…".to_string(), "Nothing has matched yet.".to_string()),
        _ if words.trim().is_empty() => (
            "Nothing found".to_string(),
            format!("Nothing in {place} fits these filters."),
        ),
        _ => (
            "Nothing found".to_string(),
            format!("No name in {place} matches “{words}”."),
        ),
    };
    let mut page = status_page(Icon::Search, title).description(description);
    if running != Some(true) && !hints.is_empty() {
        page = page.child(Element::from(HintRow { hints }));
    }
    Element::from(page)
}

/// The buttons under a "nothing found".
#[derive(PartialEq)]
struct HintRow {
    hints: Vec<(String, EventHandler<()>)>,
}

impl Component for HintRow {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        rect()
            .component("SearchHints")
            .tw(&tokens, &[flex_row(), justify(Align::Center), items(Align::Center), gap(S2), w_full()])
            .children(self.hints.iter().map(|(label, on_press)| {
                Element::from(action_button(label.clone(), on_press.clone()))
            }))
    }
}

/// A folder that could not be read, with the reason in plain words.
pub fn error_state(error: &ListError) -> Element {
    let (icon, title) = match error {
        ListError::PermissionDenied => (Icon::ChangesPrevent, "No permission"),
        ListError::NotFound => (Icon::DialogWarning, "Folder not found"),
        ListError::NotADirectory => (Icon::DialogWarning, "Not a folder"),
        ListError::Unresponsive => (Icon::DialogWarning, "Drive not responding"),
        ListError::Cancelled | ListError::Other(_) => (Icon::DialogError, "Cannot open folder"),
    };
    Element::from(status_page(icon, title).description(error.to_string()))
}
