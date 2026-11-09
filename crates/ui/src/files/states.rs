//! What a file view shows when there is no list: loading, empty, failed.

use crate::prelude::tr;
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
    let (icon, title, description) = if recent {
        (
            Icon::DocumentOpenRecent,
            tr!("ui-files-state-recent-title"),
            tr!("ui-files-state-recent-description"),
        )
    } else if filtered {
        (
            Icon::Filter,
            tr!("ui-files-state-filtered-title"),
            tr!("ui-files-state-filtered-description"),
        )
    } else if hidden_items {
        (
            Icon::ViewConceal,
            tr!("ui-files-state-hidden-title"),
            tr!("ui-files-state-hidden-description"),
        )
    } else {
        (
            FileKind::Folder.glyph(),
            tr!("ui-files-state-empty-title"),
            tr!("ui-files-state-empty-description"),
        )
    };
    Element::from(status_page(icon, title).description(description))
}

/// A folder whose read was stopped before anything was in: say so, and how
/// to read it.
pub fn stopped_state() -> Element {
    Element::from(
        status_page(Icon::MediaPlaybackStop, tr!("ui-files-state-stopped-title"))
            .description(tr!("ui-files-state-stopped-description")),
    )
}

/// The Trash with nothing in it: a status page, not an empty list.
pub fn trash_empty_state() -> Element {
    Element::from(
        status_page(Icon::UserTrash, tr!("ui-files-state-trash-title"))
            .description(tr!("ui-files-state-trash-description")),
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
        return Element::from(
            status_page(Icon::DialogWarning, tr!("ui-files-search-invalid-title"))
                .description(reason.to_string()),
        );
    }
    let (title, description) = match running {
        Some(true) => (
            tr!("ui-files-search-searching-title"),
            tr!("ui-files-search-searching-description"),
        ),
        _ if words.trim().is_empty() => (
            tr!("ui-files-search-none-title"),
            tr!("ui-files-search-none-all-description", place = place),
        ),
        _ => (
            tr!("ui-files-search-none-title"),
            tr!(
                "ui-files-search-none-words-description",
                place = place,
                words = words
            ),
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
            .tw(
                &tokens,
                &[
                    flex_row(),
                    justify(Align::Center),
                    items(Align::Center),
                    gap(S2),
                    w_full(),
                ],
            )
            .children(self.hints.iter().map(|(label, on_press)| {
                Element::from(action_button(label.clone(), on_press.clone()))
            }))
    }
}

/// A folder that could not be read, with the reason in plain words.
pub fn error_state(error: &ListError) -> Element {
    // Each reason has its own picture: a padlock for no permission, a folder
    // that is not there, a file for "not a folder", the drive for one that
    // does not answer. A warning sign says none of that.
    let (icon, title) = match error {
        ListError::PermissionDenied => (
            Icon::NetworkWirelessEncrypted,
            tr!("ui-files-error-permission-title"),
        ),
        ListError::NotFound => (Icon::FolderMissing, tr!("ui-files-error-not-found-title")),
        ListError::NotADirectory => (
            Icon::ApplicationXGeneric,
            tr!("ui-files-error-not-folder-title"),
        ),
        ListError::Unresponsive => (
            Icon::DriveHarddisk,
            tr!("ui-files-error-unresponsive-title"),
        ),
        ListError::Cancelled | ListError::Other(_) => {
            (Icon::DialogError, tr!("ui-files-error-open-title"))
        }
    };
    Element::from(status_page(icon, title).description(error.to_string()))
}
