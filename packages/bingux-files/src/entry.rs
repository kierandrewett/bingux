//! One entry of a directory.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::mime::{self, Category, DIRECTORY, Type};

/// What an entry is, after following a symbolic link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    /// A socket, a device node or a link that points nowhere.
    Other,
}

/// A file or folder, as a listing shows it.
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
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Directory
    }

    pub fn category(&self) -> Category {
        self.mime.category
    }

    /// Read `path` from the disk. `hidden_listed` is true when the parent's
    /// `.hidden` file names it.
    pub fn from_path(path: &Path, hidden_listed: bool) -> io::Result<Entry> {
        let link = fs::symlink_metadata(path)?;
        let symlink = link.file_type().is_symlink();
        // A link is described by its target; a broken link by itself.
        let meta = if symlink {
            fs::metadata(path).unwrap_or(link)
        } else {
            link
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let kind = if meta.is_dir() {
            Kind::Directory
        } else if meta.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let mime = match kind {
            Kind::Directory => DIRECTORY,
            _ => mime::for_name(&name),
        };
        Ok(Entry {
            hidden: hidden_listed || name.starts_with('.'),
            size: if kind == Kind::File { meta.len() } else { 0 },
            modified: meta.modified().ok(),
            name,
            path: path.to_path_buf(),
            kind,
            symlink,
            mime,
        })
    }
}
