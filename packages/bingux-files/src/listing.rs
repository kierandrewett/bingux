//! Reading a directory.
//!
//! [`list_dir`] blocks on the disk, so call it on a worker thread. It checks
//! a [`Cancel`] between entries, so a person who has already moved on does
//! not wait for a huge folder to finish. The listing keeps every entry,
//! hidden ones too; [`Listing::select`] then hides, filters and sorts
//! without touching the disk again, so toggling "Show hidden files" or
//! clicking a column is instant.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::entry::Entry;
use crate::filter::Filter;
use crate::sort::Sort;

/// A flag a caller sets to stop a listing early. Clones share the flag.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Why a folder could not be listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    NotFound,
    PermissionDenied,
    NotADirectory,
    /// The caller's [`Cancel`] was set.
    Cancelled,
    Other(String),
}

impl ListError {
    fn from_io(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::NotFound => ListError::NotFound,
            io::ErrorKind::PermissionDenied => ListError::PermissionDenied,
            io::ErrorKind::NotADirectory => ListError::NotADirectory,
            _ => ListError::Other(error.to_string()),
        }
    }
}

impl fmt::Display for ListError {
    /// A sentence for a status page.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListError::NotFound => f.write_str("This folder does not exist."),
            ListError::PermissionDenied => {
                f.write_str("You do not have permission to open this folder.")
            }
            ListError::NotADirectory => f.write_str("This is not a folder."),
            ListError::Cancelled => f.write_str("Cancelled."),
            ListError::Other(detail) => write!(f, "This folder could not be read: {detail}"),
        }
    }
}

impl std::error::Error for ListError {}

/// The entries of one place: a folder, or the recent files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The folder. Empty for the recent-files listing.
    pub path: PathBuf,
    pub recent: bool,
    /// Every entry, hidden ones included, in disk order.
    pub entries: Vec<Entry>,
}

/// What a person sees of a [`Listing`].
#[derive(Clone, Copy, Debug)]
pub struct View<'a> {
    pub show_hidden: bool,
    pub filter: Option<&'a Filter>,
    /// Only folders: for choosing a folder.
    pub directories_only: bool,
    pub sort: Sort,
}

impl Listing {
    /// The indices into [`Listing::entries`] of what `view` shows, in order.
    pub fn select(&self, view: &View<'_>) -> Vec<usize> {
        let mut shown: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                (view.show_hidden || !entry.hidden)
                    && (!view.directories_only || entry.is_dir())
                    && view.filter.is_none_or(|filter| filter.accepts_entry(entry))
            })
            .map(|(index, _)| index)
            .collect();
        shown.sort_by(|a, b| view.sort.compare(&self.entries[*a], &self.entries[*b]));
        shown
    }
}

/// Read the folder `path`.
pub fn list_dir(path: &Path, cancel: &Cancel) -> Result<Listing, ListError> {
    let reader = fs::read_dir(path).map_err(|error| ListError::from_io(&error))?;
    let listed_hidden = hidden_names(path);
    let mut entries = Vec::new();
    for item in reader {
        if cancel.is_cancelled() {
            return Err(ListError::Cancelled);
        }
        // An entry that vanished between readdir and stat is simply gone.
        let Ok(item) = item else { continue };
        let hidden = listed_hidden.contains(item.file_name().to_string_lossy().as_ref());
        if let Ok(entry) = Entry::from_path(&item.path(), hidden) {
            entries.push(entry);
        }
    }
    Ok(Listing {
        path: path.to_path_buf(),
        recent: false,
        entries,
    })
}

/// The names in the folder's `.hidden` file, one per line, as GIO reads it.
fn hidden_names(dir: &Path) -> HashSet<String> {
    fs::read_to_string(dir.join(".hidden"))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort::SortKey;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_hides_filters_and_sorts() {
        let dir = scratch("list");
        fs::create_dir(dir.join("Folder")).unwrap();
        fs::write(dir.join("b.png"), b"12345").unwrap();
        fs::write(dir.join("a10.txt"), b"").unwrap();
        fs::write(dir.join("a2.txt"), b"").unwrap();
        fs::write(dir.join(".secret"), b"").unwrap();
        fs::write(dir.join("listed.txt"), b"").unwrap();
        fs::write(dir.join(".hidden"), "listed.txt\n").unwrap();
        let listing = list_dir(&dir, &Cancel::new()).unwrap();
        let names = |view: &View<'_>| -> Vec<String> {
            listing
                .select(view)
                .into_iter()
                .map(|i| listing.entries[i].name.clone())
                .collect()
        };
        let mut view = View {
            show_hidden: false,
            filter: None,
            directories_only: false,
            sort: Sort::default(),
        };
        assert_eq!(names(&view), ["Folder", "a2.txt", "a10.txt", "b.png"]);
        view.sort = Sort::by(SortKey::Size).toggled(SortKey::Size);
        assert_eq!(names(&view), ["Folder", "b.png", "a10.txt", "a2.txt"]);
        view.show_hidden = true;
        assert_eq!(names(&view).len(), 7);
        let filter = Filter::new("Images").glob("*.png");
        view.filter = Some(&filter);
        view.sort = Sort::default();
        assert_eq!(names(&view), ["Folder", "b.png"]);
        view.directories_only = true;
        assert_eq!(names(&view), ["Folder"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_are_classified() {
        let missing = Path::new("/definitely/not/here");
        assert_eq!(
            list_dir(missing, &Cancel::new()).unwrap_err(),
            ListError::NotFound
        );
        assert_eq!(
            list_dir(Path::new("/etc/passwd"), &Cancel::new()).unwrap_err(),
            ListError::NotADirectory
        );
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(
            list_dir(Path::new("/etc"), &cancel).unwrap_err(),
            ListError::Cancelled
        );
    }
}
