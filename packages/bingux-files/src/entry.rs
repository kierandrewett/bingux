//! One entry of a directory.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::mime::{self, Category, DIRECTORY, Type};
use crate::places::SpecialFolder;
use crate::stat::{self, StatInfo};

/// What an entry is, after following a symbolic link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    /// A socket, a device node or a link that points nowhere.
    Other,
}

/// What a bin's `.trashinfo` file says about an item of the Trash: where it
/// was and when it was deleted. Only an entry listed from a bin's `files`
/// folder has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashMeta {
    /// Where it was before it was deleted. `None` when there is no info file,
    /// or it does not say: nobody knows where the item came from.
    pub original: Option<PathBuf>,
    /// When it was deleted, in the local time the info file records.
    pub deleted: Option<SystemTime>,
}

/// A file or folder, as a listing shows it.
///
/// A listing creates entries from the directory read alone, which gives a
/// name and a kind and nothing else. `sized` says whether the rest (size,
/// times, permissions, owner) has been read yet. Views show a dash until it
/// is true and ask [`crate::loader::Load::want_stats`] for the rows they
/// show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The file name, lossily converted to UTF-8 for display.
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// Bytes. Zero for folders.
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// A leading dot, or listed in the folder's `.hidden` file.
    pub hidden: bool,
    /// The entry is a symbolic link (its other fields describe the target).
    pub symlink: bool,
    pub mime: Type,
    /// The home folder or an XDG user directory, decided by the path text
    /// alone (see [`crate::places::special_folder`]).
    pub special: Option<SpecialFolder>,
    /// `size`, `modified` and the fields below are read from the disk.
    pub sized: bool,
    /// A link whose target has not been looked at yet; `kind` is a guess.
    pub pending_link: bool,
    pub created: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Permission bits (`0o644`).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// What the bin's `.trashinfo` says about this item, when the entry was
    /// listed from a bin's `files` folder.
    pub trash: Option<std::sync::Arc<TrashMeta>>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }

    pub fn category(&self) -> Category {
        self.mime.category
    }

    /// An entry known by its name and kind only. `size` and `modified` are
    /// empty until [`Entry::apply_stat`].
    pub fn named(path: PathBuf, name: String, kind: Kind, symlink: bool, hidden: bool) -> Entry {
        let mime = match kind {
            Kind::Directory => DIRECTORY,
            _ => mime::for_name(&name),
        };
        Entry {
            hidden: hidden || name.starts_with('.'),
            name,
            path,
            kind,
            size: 0,
            modified: None,
            symlink,
            mime,
            special: None,
            sized: false,
            pending_link: false,
            created: None,
            accessed: None,
            mode: 0,
            uid: 0,
            gid: 0,
            trash: None,
        }
    }

    /// Fill in what `statx` found. A link changes the entry's kind, and so
    /// its MIME type, to its target's.
    pub fn apply_stat(&mut self, info: &StatInfo) {
        if self.kind != info.kind {
            self.kind = info.kind;
            self.mime = match info.kind {
                Kind::Directory => DIRECTORY,
                _ => mime::for_name(&self.name),
            };
        }
        self.symlink = self.symlink || info.is_link;
        self.size = info.size;
        self.modified = info.modified;
        self.created = info.created;
        self.accessed = info.accessed;
        self.mode = info.mode;
        self.uid = info.uid;
        self.gid = info.gid;
        self.sized = true;
        self.pending_link = false;
    }

    /// Read `path` from the disk. `hidden_listed` is true when the parent's
    /// `.hidden` file names it.
    pub fn from_path(path: &Path, hidden_listed: bool) -> io::Result<Entry> {
        let info = stat::stat_entry(path)?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let mut entry = Entry::named(
            path.to_path_buf(),
            name,
            info.kind,
            info.is_link,
            hidden_listed,
        );
        entry.special = crate::places::special_folder(path);
        entry.apply_stat(&info);
        Ok(entry)
    }
}
