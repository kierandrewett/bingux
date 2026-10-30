//! Reading a directory.
//!
//! [`list_dir`] blocks on the disk, so call it on a worker thread. It checks
//! a [`Cancel`] between entries, so a person who has already moved on does
//! not wait for a huge folder to finish. The listing keeps every entry,
//! hidden ones too; [`Listing::select`] then hides, filters and sorts
//! without touching the disk again, so toggling "Show hidden files" or
//! clicking a column is instant.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::entry::{Entry, Kind};
use crate::filter::Filter;
use crate::loader::Event;
use crate::mounts::MountKind;
use crate::sort::Sort;
use crate::stat;

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
    /// The drive the folder is on did not answer in time.
    Unresponsive,
    Other(String),
}

impl ListError {
    pub(crate) fn from_io(error: &io::Error) -> Self {
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
            ListError::Unresponsive => f.write_str("The drive this folder is on is not answering."),
            ListError::Other(detail) => write!(f, "This folder could not be read: {detail}"),
        }
    }
}

impl std::error::Error for ListError {}

/// The entries of one place: a folder, or the recent files.
///
/// [`list_dir`] returns a finished one. The [`loader`](crate::loader)
/// instead sends [`Event`]s, and a view keeps a `Listing` up to date by
/// passing each to [`Listing::apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The folder. Empty for the recent-files listing.
    pub path: PathBuf,
    pub recent: bool,
    /// Every entry, hidden ones included. Disk order, except that a listing
    /// shown from the cache is in name order.
    pub entries: Vec<Entry>,
    /// Counts the changes that move existing entries to other positions
    /// (a cached listing replacing the entries, a removal). Stat results
    /// carry it, so ones for a layout that has gone are ignored.
    pub revision: u64,
    /// Everything has been read. False while batches are still coming.
    pub complete: bool,
    /// The entries came from the cache and the folder has not been read
    /// again yet.
    pub from_cache: bool,
    /// The folder's drive has stopped answering; what is shown is what was
    /// known. Cleared when it answers again.
    pub unresponsive: bool,
    /// The person stopped the read before it was done (the Stop button):
    /// the entries are what had been read, and the folder may hold more.
    /// Cleared by the next `Done` or `Cached`.
    pub stopped: bool,
    /// What kind of drive the folder is on.
    pub mount: MountKind,
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

/// [`Listing::select`] with the counts a status line needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selected {
    /// The indices into [`Listing::entries`] of what the view shows.
    pub order: Vec<usize>,
    /// Files the filter hides (folders always pass), not counting hidden
    /// files that are not shown anyway.
    pub hidden_by_filter: usize,
    /// Hidden files left out because "show hidden files" is off.
    pub hidden_dotfiles: usize,
}

impl Listing {
    /// An empty listing of the folder `path`, to apply events to.
    pub fn new(path: impl Into<PathBuf>) -> Listing {
        Listing {
            path: path.into(),
            recent: false,
            entries: Vec::new(),
            revision: 0,
            complete: false,
            from_cache: false,
            unresponsive: false,
            stopped: false,
            // Until the loader says what the drive is, assume the careful
            // kind: no thumbnails, no reading of file content.
            mount: MountKind::Other,
        }
    }

    /// The indices into [`Listing::entries`] of what `view` shows, in order.
    pub fn select(&self, view: &View<'_>) -> Vec<usize> {
        self.select_counted(view).order
    }

    /// [`Listing::select`], and how many entries the filter and the hidden
    /// setting left out.
    pub fn select_counted(&self, view: &View<'_>) -> Selected {
        let matcher = view.filter.map(Filter::matcher);
        let mut selected = Selected {
            order: Vec::with_capacity(self.entries.len()),
            ..Selected::default()
        };
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.hidden && !view.show_hidden {
                selected.hidden_dotfiles += 1;
                continue;
            }
            if view.directories_only && !entry.is_dir() {
                continue;
            }
            if matcher.as_ref().is_some_and(|matcher| !matcher.matches(entry)) {
                selected.hidden_by_filter += 1;
                continue;
            }
            selected.order.push(index);
        }
        view.sort.order(&self.entries, &mut selected.order);
        selected
    }

    /// Follow a loader event, copying what it carries. See
    /// [`Listing::apply_event`].
    pub fn apply(&mut self, event: &Event) -> bool {
        self.apply_event(event.clone())
    }

    /// Follow a loader event, taking what it carries: no entry is copied.
    /// Returns whether the entries changed, so a view knows to select again.
    pub fn apply_event(&mut self, event: Event) -> bool {
        match event {
            Event::Kind(kind) => {
                self.mount = kind;
                false
            }
            Event::Cached { entries } => {
                self.entries = entries;
                self.revision += 1;
                self.complete = false;
                self.stopped = false;
                self.from_cache = true;
                true
            }
            Event::Batch(entries) => {
                let any = !entries.is_empty();
                self.entries.extend(entries);
                self.unresponsive = false;
                any
            }
            Event::Done { .. } => {
                self.complete = true;
                self.from_cache = false;
                self.stopped = false;
                self.unresponsive = false;
                false
            }
            Event::Diff(diff) => {
                let any = !diff.is_empty();
                if !diff.removed.is_empty() {
                    let gone: HashSet<&str> = diff.removed.iter().map(String::as_str).collect();
                    self.entries.retain(|entry| !gone.contains(entry.name.as_str()));
                    self.revision += 1;
                }
                if !diff.changed.is_empty() {
                    let at: HashMap<String, usize> = self
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| (entry.name.clone(), index))
                        .collect();
                    for entry in diff.changed {
                        if let Some(&index) = at.get(&entry.name) {
                            self.entries[index] = entry;
                        }
                    }
                }
                self.entries.extend(diff.added);
                self.unresponsive = false;
                any
            }
            Event::Stats(batch) => {
                self.unresponsive = false;
                if batch.revision != self.revision {
                    return false;
                }
                let mut changed = false;
                for (index, info) in &batch.updates {
                    if let Some(entry) = self.entries.get_mut(*index as usize) {
                        entry.apply_stat(info);
                        changed = true;
                    }
                }
                changed
            }
            Event::Stopped => {
                // A listing that was whole stays as it is: stopping a re-read
                // takes nothing away.
                self.stopped = !self.complete;
                false
            }
            Event::Unresponsive => {
                self.unresponsive = true;
                false
            }
            Event::Failed(_) => false,
        }
    }
}

/// How the batches of a read are cut: the first after 50 entries or 16 ms,
/// later ones after a count or a time that doubles, up to 4096 entries or
/// 256 ms, so a huge folder repaints a few times rather than hundreds.
const FIRST_BATCH: usize = 50;
const FIRST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
const LAST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(256);
const LAST_BATCH: usize = 4096;

/// `statx` every entry of `batch`, on `threads` threads when there are
/// enough of them to be worth it. An entry that has vanished is dropped.
fn stat_batch(batch: &mut Vec<Entry>, threads: usize) {
    let work = |chunk: &mut [Entry]| {
        for entry in chunk {
            match stat::stat_entry(&entry.path) {
                Ok(info) => entry.apply_stat(&info),
                Err(_) => entry.sized = false,
            }
        }
    };
    if threads <= 1 || batch.len() < 128 {
        work(batch);
    } else {
        let size = batch.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let mut chunks = batch.chunks_mut(size);
            // The calling thread takes the first slice.
            let first = chunks.next();
            for chunk in chunks {
                scope.spawn(move || work(chunk));
            }
            if let Some(chunk) = first {
                work(chunk);
            }
        });
    }
    batch.retain(|entry| entry.sized);
}

/// Read the folder `path` in batches, handing each to `flush`. Stops with
/// [`ListError::Cancelled`] when `cancel` is set or `flush` returns false.
///
/// `stat` reads size, times and owner of every entry before it is handed
/// over (what a local disk can afford), on `threads` threads; without it an
/// entry has what the directory read gives: a name and a type, and a link is
/// left [`Entry::pending_link`].
pub(crate) fn read_folder(
    path: &Path,
    stat: bool,
    threads: usize,
    cancel: &Cancel,
    flush: &mut dyn FnMut(Vec<Entry>) -> bool,
) -> Result<(), ListError> {
    let reader = fs::read_dir(path).map_err(|error| ListError::from_io(&error))?;
    let listed_hidden = hidden_names(path);
    let special = crate::places::system_user_dirs().children_of(path);
    // The Trash: each item also says where it was and when it was deleted.
    let bin = crate::ops::BinFolder::of(path);
    let mut batch: Vec<Entry> = Vec::with_capacity(FIRST_BATCH);
    let mut sent_first = false;
    let mut interval = FIRST_INTERVAL;
    let mut target = FIRST_BATCH;
    let mut last = std::time::Instant::now();
    for (count, item) in reader.enumerate() {
        if count % 32 == 0 && cancel.is_cancelled() {
            return Err(ListError::Cancelled);
        }
        // An entry that vanished between readdir and stat is simply gone.
        let Ok(item) = item else { continue };
        let file_name = item.file_name();
        let name = file_name.to_string_lossy().into_owned();
        let full = path.join(&file_name);
        let file_type = item.file_type().ok();
        let (kind, symlink) = match file_type {
            Some(kind) if kind.is_dir() => (Kind::Directory, false),
            Some(kind) if kind.is_file() => (Kind::File, false),
            Some(kind) if kind.is_symlink() => (Kind::File, true),
            _ => (Kind::Other, false),
        };
        let hidden = !listed_hidden.is_empty() && listed_hidden.contains(name.as_str());
        let mut entry = Entry::named(full, name, kind, symlink, hidden);
        if !special.is_empty() {
            entry.special = special
                .iter()
                .find(|(special_name, _)| *special_name == file_name)
                .map(|(_, kind)| *kind);
        }
        entry.pending_link = symlink;
        if let Some(bin) = &bin {
            entry.trash = Some(std::sync::Arc::new(bin.meta(&file_name)));
        }
        batch.push(entry);
        let due = batch.len() >= target
            || (count % 16 == 0 && last.elapsed() >= if sent_first { interval } else { FIRST_INTERVAL });
        if due {
            if stat {
                stat_batch(&mut batch, threads);
            }
            if !batch.is_empty() && !flush(std::mem::take(&mut batch)) {
                return Err(ListError::Cancelled);
            }
            batch = Vec::with_capacity(256);
            if sent_first {
                interval = (interval * 2).min(LAST_INTERVAL);
            }
            target = if sent_first { (target * 2).min(LAST_BATCH) } else { 256 };
            sent_first = true;
            last = std::time::Instant::now();
        }
    }
    if stat {
        stat_batch(&mut batch, threads);
    }
    if !batch.is_empty() && !flush(batch) {
        return Err(ListError::Cancelled);
    }
    Ok(())
}

/// Read the folder `path`: every entry, with size, times and owner. Blocks
/// on the disk, with no cache and no deadline: the [`loader`](crate::loader)
/// is what a view should use.
pub fn list_dir(path: &Path, cancel: &Cancel) -> Result<Listing, ListError> {
    let mut entries = Vec::new();
    read_folder(path, true, 4, cancel, &mut |batch| {
        entries.extend(batch);
        true
    })?;
    let mut listing = Listing::new(path);
    listing.entries = entries;
    listing.complete = true;
    listing.mount = crate::mounts::kind_of(path);
    Ok(listing)
}

/// The names in the folder's `.hidden` file, one per line, as GIO reads it.
pub(crate) fn hidden_names(dir: &Path) -> HashSet<String> {
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

    /// The files folder of a drive's bin lists its items with what their info
    /// files say, and sorts by when they were deleted.
    #[test]
    fn a_bins_files_folder_lists_where_each_item_was_and_when_it_went() {
        let dir = scratch("bin-listing");
        let files = dir.join("drive/.Trash-1000/files");
        let info = dir.join("drive/.Trash-1000/info");
        fs::create_dir_all(&files).unwrap();
        fs::create_dir_all(&info).unwrap();
        for (name, path, date) in [
            ("late.txt", "docs/late.txt", "2024-05-03T10:00:00"),
            ("early.txt", "early%20one.txt", "2024-05-01T09:30:00"),
            ("odd", "x/odd", "not a date"),
        ] {
            fs::write(files.join(name), b"x").unwrap();
            fs::write(
                info.join(format!("{name}.trashinfo")),
                format!("[Trash Info]\nPath={path}\nDeletionDate={date}\n"),
            )
            .unwrap();
        }
        // An item with no info file at all.
        fs::write(files.join("stray"), b"s").unwrap();
        let listing = list_dir(&files, &Cancel::new()).unwrap();
        let meta = |name: &str| {
            let entry = listing.entries.iter().find(|entry| entry.name == name).unwrap();
            entry.trash.clone().unwrap_or_else(|| panic!("{name} has trash info"))
        };
        assert_eq!(meta("late.txt").original, Some(dir.join("drive/docs/late.txt")), "relative to the drive's top");
        assert_eq!(meta("early.txt").original, Some(dir.join("drive/early one.txt")));
        assert!(meta("early.txt").deleted.is_some() && meta("late.txt").deleted > meta("early.txt").deleted);
        assert_eq!(meta("odd").deleted, None, "a date that cannot be read");
        assert_eq!(meta("stray").original, None, "no info file");
        let view = View {
            show_hidden: true,
            filter: None,
            directories_only: false,
            sort: Sort::by(SortKey::Deleted),
        };
        let names: Vec<_> = listing
            .select(&view)
            .into_iter()
            .map(|index| listing.entries[index].name.clone())
            .collect();
        assert_eq!(names[2..], ["early.txt", "late.txt"], "oldest first, the unknown ones before them: {names:?}");
        // An ordinary folder's entries have no such info.
        let plain = list_dir(&dir.join("drive"), &Cancel::new()).unwrap();
        assert!(plain.entries.iter().all(|entry| entry.trash.is_none()));
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

    #[test]
    fn a_stopped_first_read_keeps_what_it_had_and_says_so() {
        let mut listing = Listing::new("/x");
        listing.apply_event(Event::Batch(vec![Entry::named("/x/a".into(), "a".into(), Kind::File, false, false)]));
        listing.apply_event(Event::Stopped);
        assert!(listing.stopped && !listing.complete);
        assert_eq!(listing.entries.len(), 1);
        // Reading on to the end makes it whole again.
        listing.apply_event(Event::Done { total: 1 });
        assert!(listing.complete && !listing.stopped);
    }

    #[test]
    fn stopping_a_re_read_of_a_whole_listing_changes_nothing() {
        let mut listing = Listing::new("/x");
        listing.apply_event(Event::Done { total: 0 });
        let before = listing.clone();
        listing.apply_event(Event::Stopped);
        assert_eq!(listing, before);
    }

    #[test]
    fn a_cached_listing_whose_check_was_stopped_stays_unchecked() {
        let mut listing = Listing::new("/x");
        listing.apply_event(Event::Cached {
            entries: vec![Entry::named("/x/a".into(), "a".into(), Kind::File, false, false)],
        });
        listing.apply_event(Event::Stopped);
        assert!(listing.stopped && listing.from_cache && !listing.complete);
        assert_eq!(listing.entries.len(), 1);
    }
}
