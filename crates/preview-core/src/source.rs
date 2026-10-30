//! The input of a preview: something with a name and bytes.
//!
//! A [`PreviewSource`] is what a wallpaper picker, a mail attachment and the
//! file view all hand over. It names no file-list row and no window.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingux_files::location::from_file_uri;

/// What to preview.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PreviewSource {
    /// A file or folder on disk.
    Path(PathBuf),
    /// Bytes in memory, with the name a person knows them by: a mail
    /// attachment, a clipboard image.
    Bytes { name: String, data: Arc<[u8]> },
}

impl PreviewSource {
    pub fn path(path: impl Into<PathBuf>) -> Self {
        PreviewSource::Path(path.into())
    }

    pub fn bytes(name: impl Into<String>, data: impl Into<Arc<[u8]>>) -> Self {
        PreviewSource::Bytes {
            name: name.into(),
            data: data.into(),
        }
    }

    /// A `file://` address as a path source. Other schemes are not previewed:
    /// a viewer never opens the network.
    pub fn uri(uri: &str) -> Result<Self, SourceError> {
        from_file_uri(uri)
            .map(PreviewSource::Path)
            .ok_or_else(|| SourceError::Scheme(uri.split(':').next().unwrap_or("").to_owned()))
    }

    /// The name a person sees: the file name, or the attachment's.
    pub fn name(&self) -> String {
        match self {
            PreviewSource::Path(path) => path
                .file_name()
                .map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned()),
            PreviewSource::Bytes { name, .. } => name.clone(),
        }
    }

    /// The path, for a source on disk.
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            PreviewSource::Path(path) => Some(path),
            PreviewSource::Bytes { .. } => None,
        }
    }

    /// Open the content for reading. A folder is not content.
    pub fn open(&self) -> io::Result<Box<dyn ReadSeek>> {
        match self {
            PreviewSource::Path(path) => Ok(Box::new(File::open(path)?)),
            PreviewSource::Bytes { data, .. } => Ok(Box::new(Cursor::new(data.clone()))),
        }
    }

    /// The length of the content in bytes, when it has one.
    pub fn len(&self) -> Option<u64> {
        match self {
            PreviewSource::Path(path) => std::fs::metadata(path).ok().filter(|meta| meta.is_file()).map(|meta| meta.len()),
            PreviewSource::Bytes { data, .. } => Some(data.len() as u64),
        }
    }

    /// Whether the source is a folder.
    pub fn is_dir(&self) -> bool {
        matches!(self, PreviewSource::Path(path) if path.is_dir())
    }
}

/// A source could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    /// Only `file://` addresses are previewed.
    Scheme(String),
}

/// A reader that can also seek.
pub trait ReadSeek: Read + Seek + Send {}

impl<T: Read + Seek + Send> ReadSeek for T {}

/// Read up to `limit` bytes from `offset`.
pub fn read_at(reader: &mut dyn ReadSeek, offset: u64, limit: usize) -> io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity(limit.min(1 << 20));
    reader.take(limit as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_is_a_path_and_other_schemes_are_refused() {
        assert_eq!(
            PreviewSource::uri("file:///tmp/a%20b.txt"),
            Ok(PreviewSource::Path(PathBuf::from("/tmp/a b.txt")))
        );
        assert_eq!(PreviewSource::uri("https://example.org/a.png"), Err(SourceError::Scheme("https".into())));
    }

    #[test]
    fn bytes_read_like_a_file() {
        let source = PreviewSource::bytes("a.txt", b"hello world".to_vec());
        assert_eq!(source.name(), "a.txt");
        assert_eq!(source.len(), Some(11));
        let mut reader = source.open().unwrap();
        assert_eq!(read_at(&mut *reader, 6, 100).unwrap(), b"world");
    }
}
