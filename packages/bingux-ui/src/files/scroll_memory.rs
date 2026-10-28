//! Where a file view was scrolled, and when a new search sends it back to the
//! top.
//!
//! Leaving a folder records how far its list was scrolled and what was
//! selected; coming back (Back, Forward, Up, or a tab shown again) puts both
//! back. The record belongs to an [`Owner`], so two tabs on one folder each
//! return to their own place.
//!
//! The rules here are plain data so they can be tested without a window. The
//! view applies them from its render, never from an effect: an effect that
//! calls `ScrollController::scroll_to_y` reads the controller's bounds, which
//! the scroll view rewrites on every layout, so the effect runs again after
//! every wheel notch and key press and throws the list back to where it
//! started.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use super::view::Rows;

/// Who a record belongs to: the id of a tab of the Files app, or 0 for a
/// dialog, which has one view.
pub(super) type Owner = u64;

/// How many folders keep their scroll position and selection.
const FOLDERS: usize = 32;

/// One folder as it was left.
#[derive(Clone)]
pub(super) struct Remembered {
    owner: Owner,
    path: PathBuf,
    /// The scroll offset, as the controller stores it (zero or negative).
    pub scroll: i32,
    pub selection: Vec<PathBuf>,
}

#[derive(Default)]
struct Memory {
    /// Most recent last.
    folders: Vec<Remembered>,
    /// The folder each owner left last.
    left: Vec<(Owner, PathBuf)>,
}

thread_local! {
    static MEMORY: RefCell<Memory> = RefCell::new(Memory::default());
}

/// Record that `owner` left `path` scrolled to `scroll` with `selection`.
pub(super) fn remember(owner: Owner, path: &Path, scroll: i32, selection: Vec<PathBuf>) {
    MEMORY.with_borrow_mut(|memory| {
        memory
            .folders
            .retain(|entry| entry.owner != owner || entry.path != path);
        memory.folders.push(Remembered {
            owner,
            path: path.to_path_buf(),
            scroll,
            selection,
        });
        if memory.folders.len() > FOLDERS {
            memory.folders.remove(0);
        }
        memory.left.retain(|(other, _)| *other != owner);
        memory.left.push((owner, path.to_path_buf()));
    });
}

/// Where `owner` left `path`, if it was there before.
pub(super) fn recall(owner: Owner, path: &Path) -> Option<Remembered> {
    MEMORY.with_borrow(|memory| {
        memory
            .folders
            .iter()
            .find(|entry| entry.owner == owner && entry.path == path)
            .cloned()
    })
}

/// The folder `owner` left last, if it lies directly in `folder`: going up
/// selects the folder you came out of.
pub(super) fn came_from(owner: Owner, folder: &Path) -> Option<PathBuf> {
    MEMORY.with_borrow(|memory| {
        memory
            .left
            .iter()
            .find(|(other, _)| *other == owner)
            .map(|(_, left)| left.clone())
            .filter(|left| left.parent() == Some(folder))
    })
}

/// The search a view was last shown with: its text, and whether the rows are
/// results from the subfolders.
pub(super) type SearchKey = Option<(String, bool)>;

/// Decides when a search sends the list back to the top: when the search
/// changes and the rows it shows change with it. A search that keeps its text
/// while results stream in, and one whose new text leaves the very same rows,
/// keep the scroll where it is.
pub(super) struct SearchTop {
    key: SearchKey,
    rows: Rows,
}

impl SearchTop {
    pub fn new(key: SearchKey, rows: Rows) -> Self {
        Self { key, rows }
    }

    /// Note what this render shows. True when the list should go to the top.
    pub fn moves(&mut self, key: SearchKey, rows: &Rows) -> bool {
        let searched = self.key != key;
        let moved = searched && !same_rows(&self.rows, rows);
        if searched {
            self.key = key;
        }
        self.rows = rows.clone();
        moved
    }
}

/// Whether two sets of rows are the same files in the same order.
fn same_rows(a: &Rows, b: &Rows) -> bool {
    if std::sync::Arc::ptr_eq(&a.listing, &b.listing) && std::sync::Arc::ptr_eq(&a.order, &b.order) {
        return true;
    }
    a.len() == b.len() && (0..a.len()).all(|row| a.entry(row).path == b.entry(row).path)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bingux_files::{Entry, Kind, Listing};

    use super::*;

    fn rows(names: &[&str]) -> Rows {
        let mut listing = Listing::new("/folder");
        for name in names {
            listing.entries.push(Entry::named(
                PathBuf::from("/folder").join(name),
                (*name).to_string(),
                Kind::File,
                false,
                false,
            ));
        }
        let order = (0..names.len()).collect();
        Rows {
            listing: Arc::new(listing),
            order: Arc::new(order),
        }
    }

    fn key(text: &str, deep: bool) -> SearchKey {
        Some((text.to_string(), deep))
    }

    #[test]
    fn a_tab_returns_to_its_own_place_in_a_folder_two_tabs_share() {
        remember(1, Path::new("/big"), -500, vec![]);
        remember(2, Path::new("/big"), -9000, vec![PathBuf::from("/big/x")]);
        assert_eq!(recall(1, Path::new("/big")).map(|r| r.scroll), Some(-500));
        let second = recall(2, Path::new("/big")).expect("tab 2 left /big");
        assert_eq!(second.scroll, -9000);
        assert_eq!(second.selection, vec![PathBuf::from("/big/x")]);
        assert!(recall(3, Path::new("/big")).is_none());
        assert!(recall(1, Path::new("/other")).is_none());
    }

    #[test]
    fn leaving_a_folder_again_replaces_its_record_only_for_that_owner() {
        remember(1, Path::new("/big"), -500, vec![]);
        remember(2, Path::new("/big"), -700, vec![]);
        remember(1, Path::new("/big"), -1200, vec![]);
        assert_eq!(recall(1, Path::new("/big")).map(|r| r.scroll), Some(-1200));
        assert_eq!(recall(2, Path::new("/big")).map(|r| r.scroll), Some(-700));
    }

    #[test]
    fn the_oldest_folders_are_forgotten_past_the_limit() {
        for index in 0..=FOLDERS {
            remember(1, Path::new(&format!("/f{index}")), -(index as i32), vec![]);
        }
        assert!(recall(1, Path::new("/f0")).is_none());
        assert_eq!(recall(1, Path::new("/f1")).map(|r| r.scroll), Some(-1));
        assert!(recall(1, Path::new(&format!("/f{FOLDERS}"))).is_some());
    }

    #[test]
    fn going_up_selects_the_folder_that_owner_came_out_of() {
        remember(1, Path::new("/home/a/inner"), 0, vec![]);
        remember(2, Path::new("/home/b/inner"), 0, vec![]);
        assert_eq!(
            came_from(1, Path::new("/home/a")),
            Some(PathBuf::from("/home/a/inner"))
        );
        assert_eq!(came_from(1, Path::new("/home/b")), None);
        assert_eq!(
            came_from(2, Path::new("/home/b")),
            Some(PathBuf::from("/home/b/inner"))
        );
    }

    #[test]
    fn results_streaming_in_under_the_same_text_keep_the_scroll() {
        let mut top = SearchTop::new(key("re", true), rows(&["report"]));
        assert!(!top.moves(key("re", true), &rows(&["report", "readme"])));
        assert!(!top.moves(key("re", true), &rows(&["report", "readme", "rest"])));
    }

    #[test]
    fn a_new_text_that_changes_the_rows_goes_to_the_top() {
        let mut top = SearchTop::new(None, rows(&["a", "b", "c"]));
        assert!(top.moves(key("a", false), &rows(&["a"])));
        assert!(top.moves(key("ab", false), &rows(&["ab"])));
        // Clearing the search brings every file back.
        assert!(top.moves(None, &rows(&["a", "b", "c"])));
    }

    #[test]
    fn a_new_text_that_leaves_the_same_rows_keeps_the_scroll() {
        let mut top = SearchTop::new(key("r", false), rows(&["report", "readme"]));
        assert!(!top.moves(key("re", false), &rows(&["report", "readme"])));
    }

    #[test]
    fn rows_are_compared_by_file_not_by_position() {
        let mut top = SearchTop::new(key("a", true), rows(&["one", "two"]));
        // Both lists are 0..2, but they hold other files.
        assert!(top.moves(key("b", true), &rows(&["three", "four"])));
    }

    #[test]
    fn the_rows_streamed_since_the_last_render_are_what_a_new_text_is_compared_with() {
        let mut top = SearchTop::new(key("re", true), rows(&["report"]));
        assert!(!top.moves(key("re", true), &rows(&["report", "readme"])));
        // The new text shows the two files already on screen.
        assert!(!top.moves(key("rea", true), &rows(&["report", "readme"])));
    }
}
