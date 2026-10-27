//! The pill over a file view: how many items, how many are selected, what is
//! hidden, and what the view is doing.
//!
//! It floats over the bottom-left corner of the list, sized to its text, like a
//! browser's link preview, so it takes no row from the list. Its surface, border
//! and padding are the tooltip's. When the pointer reaches it, it moves to the
//! bottom-right corner (and back), so it never covers a row for good. Nothing
//! shows when there is nothing to say. Figures use tabular digits, so a count
//! that ticks up while a big folder loads does not shimmer. A running
//! operation replaces the text with its label, a progress bar and a cancel
//! button, and stays put so the button can be pressed; a notice replaces the
//! text for a few seconds once the operation is over.

use bingux_files::display;
use freya::prelude::*;

use super::actions::Ui;
use crate::component::ComponentName;
use crate::controls::{icon_button, progress_bar};
use crate::icons::Icon;
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink, TextStyle};

/// The status line's text: a caption with digits of one width.
const STATUS: TextStyle = TextStyle {
    tabular: true,
    ..CAPTION
};

/// What the line reports about the folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct StatusInfo {
    /// Entries on show.
    pub shown: usize,
    /// Entries read so far, hidden and filtered ones included.
    pub total_listed: usize,
    pub selected: usize,
    /// The size of the selected files; zero when none or only folders.
    pub selected_bytes: u64,
    /// Dot files that are not shown.
    pub hidden_files: usize,
    /// Entries the file type filter hides.
    pub filtered_out: usize,
    /// The folder is still being read.
    pub loading: bool,
    /// The whole folder has been read.
    pub complete: bool,
    /// The drive has not answered for a while.
    pub unresponsive: bool,
    /// A search is narrowing or replacing the list. Not the file type
    /// filter: that is `filtered_out`.
    pub search: Option<SearchLine>,
}

/// What a search says about the list.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SearchLine {
    /// The folder on show is filtered: `matches` of its `of` items.
    Folder { matches: usize, of: usize },
    /// The subfolders are being or have been searched.
    Subfolders {
        found: usize,
        folders: usize,
        running: bool,
        truncated: bool,
    },
}

/// `1204` as `1,204`.
fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

fn item_count(count: usize) -> String {
    format!("{} {}", grouped(count), if count == 1 { "item" } else { "items" })
}

fn folder_count(count: usize) -> String {
    format!("{} {}", grouped(count), if count == 1 { "folder" } else { "folders" })
}

/// The text of the line while nothing else is going on.
pub(super) fn status_text(info: &StatusInfo) -> String {
    if info.unresponsive {
        return "Waiting for the drive to answer…".to_owned();
    }
    if info.loading && !info.complete {
        return format!("Loading… {}", item_count(info.total_listed));
    }
    let mut parts = match &info.search {
        Some(SearchLine::Folder { matches, of }) => vec![format!(
            "{} of {} {}",
            grouped(*matches),
            grouped(*of),
            if *of == 1 { "item matches" } else { "items match" }
        )],
        Some(SearchLine::Subfolders {
            found,
            folders,
            running,
            truncated,
        }) => vec![match (running, truncated) {
            (true, _) => format!(
                "Searching subfolders… {} found · {} read",
                grouped(*found),
                folder_count(*folders)
            ),
            (false, true) => format!("{} found · limit reached", grouped(*found)),
            (false, false) => format!(
                "{} found in subfolders · {} read",
                grouped(*found),
                folder_count(*folders)
            ),
        }],
        None => vec![item_count(info.shown)],
    };
    if info.selected > 0 {
        let mut selected = format!("{} selected", grouped(info.selected));
        if info.selected_bytes > 0 {
            selected.push_str(&format!(" ({})", display::size(info.selected_bytes)));
        }
        parts.push(selected);
    }
    if info.hidden_files > 0 {
        parts.push(format!("{} hidden", grouped(info.hidden_files)));
    }
    if info.filtered_out > 0 {
        parts.push(format!("{} hidden by filter", grouped(info.filtered_out)));
    }
    parts.join(" · ")
}

/// The status line.
pub(super) fn status_line(info: StatusInfo, ui: Ui) -> Element {
    StatusLine { info, ui }.into()
}

/// Whether the pill has something to say. An empty folder says nothing: its
/// empty state already does.
fn has_news(info: &StatusInfo) -> bool {
    info.loading
        || info.unresponsive
        || info.shown > 0
        || info.selected > 0
        || info.hidden_files > 0
        || info.filtered_out > 0
        || info.search.is_some()
}

/// The bottom corner of the view the pill sits in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Corner {
    Left,
    Right,
}

#[derive(PartialEq)]
struct StatusLine {
    info: StatusInfo,
    ui: Ui,
}

impl Component for StatusLine {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut corner = use_state(|| Corner::Left);
        let busy = self.ui.busy.read().clone();
        let notice = self.ui.notice.read().clone();
        if busy.is_none() && notice.is_none() && !has_news(&self.info) {
            return rect().interactive(false).into_element();
        }
        let edge = S2.px();
        let anchor = Position::new_absolute().bottom(edge);
        let anchor = match corner() {
            Corner::Left => anchor.left(edge),
            Corner::Right => anchor.right(edge),
        };
        let pill = rect()
            .component("StatusLine")
            .a11y_role(AccessibilityRole::Status)
            .position(anchor)
            .tw(
                &tokens,
                &[flex_row(), grow_children(), items(Align::Center), gap(S2)],
            )
            .tw(&tokens, TOOLTIP)
            .tw(
                &tokens,
                &[border(Line::Stroke(Dim::Hairline, Paint::FloatingOutline))],
            );
        if let Some(busy) = busy {
            let cancel = busy.cancel.clone();
            return pill
                .child(
                    STATUS
                        .in_ink(Ink::OnAccent)
                        .label(&tokens, busy.label)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis),
                )
                .child(
                    rect()
                        .tw(&tokens, &[w(Dim::Field)])
                        .child(progress_bar(busy.fraction.map(f64::from))),
                )
                .maybe_child(cancel.map(|cancel| {
                    icon_button(Icon::WindowClose, move |_| cancel.cancel())
                        .size(Dim::Box5)
                        .tooltip("Cancel")
                }))
                .into_element();
        }
        let (text, ink) = match notice {
            Some(notice) => (
                notice.text,
                if notice.error { Ink::Danger } else { Ink::OnAccent },
            ),
            None => (status_text(&self.info), Ink::OnAccent),
        };
        // A pointer that reaches the pill sends it to the other corner, as a
        // browser's link preview does, so it never covers a row for good.
        pill.on_pointer_enter(move |_| {
            corner.set(match corner() {
                Corner::Left => Corner::Right,
                Corner::Right => Corner::Left,
            })
        })
        .child(
            STATUS
                .in_ink(ink)
                .label(&tokens, text)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis),
        )
        .into_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(shown: usize) -> StatusInfo {
        StatusInfo {
            shown,
            total_listed: shown,
            complete: true,
            ..StatusInfo::default()
        }
    }

    #[test]
    fn an_empty_folder_has_nothing_to_say_until_something_is_hidden() {
        assert!(!has_news(&info(0)));
        assert!(has_news(&info(3)));
        assert!(has_news(&StatusInfo {
            hidden_files: 2,
            ..info(0)
        }));
        assert!(has_news(&StatusInfo {
            loading: true,
            complete: false,
            ..info(0)
        }));
    }

    #[test]
    fn a_search_says_how_many_items_match() {
        let search = |matches, of| StatusInfo {
            search: Some(SearchLine::Folder { matches, of }),
            ..info(matches)
        };
        assert_eq!(status_text(&search(3, 120)), "3 of 120 items match");
        assert_eq!(status_text(&search(1, 1)), "1 of 1 item matches");
        assert_eq!(status_text(&search(0, 1204)), "0 of 1,204 items match");
        // Nothing matching is still news: the pill must not vanish.
        assert!(has_news(&search(0, 5)));
        let deep = |running, truncated| StatusInfo {
            search: Some(SearchLine::Subfolders {
                found: 12,
                folders: 340,
                running,
                truncated,
            }),
            selected: 1,
            filtered_out: 2,
            ..info(12)
        };
        assert_eq!(
            status_text(&deep(true, false)),
            "Searching subfolders… 12 found · 340 folders read · 1 selected · 2 hidden by filter"
        );
        assert!(status_text(&deep(false, false)).starts_with("12 found in subfolders · 340 folders read"));
        assert!(status_text(&deep(false, true)).starts_with("12 found · limit reached"));
    }

    #[test]
    fn thousands_are_separated() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_204), "1,204");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn one_item_is_singular() {
        assert_eq!(status_text(&info(1)), "1 item");
        assert_eq!(status_text(&info(0)), "0 items");
        assert_eq!(status_text(&info(1204)), "1,204 items");
    }

    #[test]
    fn everything_at_once() {
        let full = StatusInfo {
            selected: 3,
            selected_bytes: 4_200_000,
            hidden_files: 12,
            filtered_out: 40,
            ..info(1204)
        };
        let text = status_text(&full);
        assert!(text.starts_with("1,204 items · 3 selected ("), "{text}");
        assert!(text.ends_with(") · 12 hidden · 40 hidden by filter"), "{text}");
    }

    #[test]
    fn a_selection_of_folders_has_no_size() {
        let folders = StatusInfo {
            selected: 1,
            ..info(10)
        };
        assert_eq!(status_text(&folders), "10 items · 1 selected");
    }

    #[test]
    fn loading_and_unresponsive() {
        let loading = StatusInfo {
            loading: true,
            complete: false,
            total_listed: 1204,
            ..info(0)
        };
        assert_eq!(status_text(&loading), "Loading… 1,204 items");
        let stuck = StatusInfo {
            unresponsive: true,
            ..loading
        };
        assert_eq!(status_text(&stuck), "Waiting for the drive to answer…");
    }
}
