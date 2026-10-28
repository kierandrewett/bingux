//! The facts every preview shows about a file, whatever the viewer.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::sniff::Sniffed;
use crate::source::PreviewSource;

/// Name, type, size and date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFacts {
    pub name: String,
    /// "PNG image", "Folder": what a person calls it.
    pub description: &'static str,
    pub mime: &'static str,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub is_dir: bool,
    pub path: Option<PathBuf>,
}

impl FileFacts {
    /// Read the facts of `source`. One `stat` for a path.
    pub fn of(source: &PreviewSource, sniffed: &Sniffed) -> Self {
        let meta = source.as_path().and_then(|path| std::fs::metadata(path).ok());
        FileFacts {
            name: source.name(),
            description: sniffed.mime.description,
            mime: sniffed.mime.mime,
            size: match (&meta, source) {
                (Some(meta), _) if meta.is_file() => Some(meta.len()),
                (None, PreviewSource::Bytes { data, .. }) => Some(data.len() as u64),
                _ => None,
            },
            modified: meta.as_ref().and_then(|meta| meta.modified().ok()),
            is_dir: source.is_dir(),
            path: source.as_path().map(ToOwned::to_owned),
        }
    }
}
