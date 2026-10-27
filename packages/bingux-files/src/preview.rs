//! What Quick Look shows for a file, decided and read without a window.
//!
//! [`kind_of`] picks how a file is shown from its type; [`read_text`] reads
//! the head of a text file for the text view; [`neighbour`] is the arithmetic
//! of stepping to the next file with the arrow keys. All of it is pure or
//! blocks on the disk (call [`read_text`] from a worker).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::entry::Kind;
use crate::listing::Cancel;
use crate::mime::{self, Category};

/// The most of a text file Quick Look reads: enough to see what a file is,
/// little enough that a log of a gigabyte opens at once.
pub const TEXT_LIMIT: usize = 64 * 1024;

/// How a file is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewKind {
    /// The picture, scaled to fit.
    Image,
    /// The head of the file in a monospace face: text, code, markup, JSON.
    Text,
    /// A document, a song or a film: the largest thumbnail the desktop has
    /// made for it, else its type's icon.
    Poster,
    /// The folder's item count and total size.
    Folder,
    /// A type nothing is known about: [`read_text`] decides whether it is
    /// text. A file that is not is shown as an icon.
    Sniff,
    /// A type that has no preview, such as an archive: its icon and facts.
    Icon,
}

/// How to show a file of `kind` and type `mime`, from the name alone.
pub fn kind_of(kind: Kind, mime: &mime::Type) -> PreviewKind {
    if kind == Kind::Directory {
        return PreviewKind::Folder;
    }
    if kind == Kind::Other {
        return PreviewKind::Icon;
    }
    match mime.category {
        Category::Folder => PreviewKind::Folder,
        Category::Image => PreviewKind::Image,
        Category::Text | Category::Code => PreviewKind::Text,
        Category::Document | Category::Audio | Category::Video => PreviewKind::Poster,
        Category::Archive => PreviewKind::Icon,
        Category::Other if mime.mime == mime::UNKNOWN.mime => PreviewKind::Sniff,
        Category::Other => PreviewKind::Icon,
    }
}

/// The head of a text file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextHead {
    pub text: String,
    /// The file goes on past `text`.
    pub truncated: bool,
}

/// Why a file's head could not be shown as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// It holds binary data.
    Binary,
    /// The load was cancelled.
    Cancelled,
    /// The disk said no, in words for a person.
    Unreadable(String),
}

/// Read up to [`TEXT_LIMIT`] bytes of `path` as text. A file with NUL bytes
/// or mostly invalid UTF-8 is [`TextError::Binary`]. Blocks on the disk.
pub fn read_text(path: &Path, cancel: &Cancel) -> Result<TextHead, TextError> {
    if cancel.is_cancelled() {
        return Err(TextError::Cancelled);
    }
    let file = File::open(path).map_err(|error| TextError::Unreadable(plain(&error)))?;
    // One byte more than the limit says whether the file goes on.
    let mut bytes = Vec::with_capacity(TEXT_LIMIT + 1);
    file.take(TEXT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| TextError::Unreadable(plain(&error)))?;
    if cancel.is_cancelled() {
        return Err(TextError::Cancelled);
    }
    let truncated = bytes.len() > TEXT_LIMIT;
    bytes.truncate(TEXT_LIMIT);
    decode_head(&bytes, truncated).map(|text| TextHead { text, truncated })
}

/// `bytes` as text, or `Binary`. A multi-byte character cut off by the limit
/// is dropped, not counted as damage.
fn decode_head(bytes: &[u8], truncated: bool) -> Result<String, TextError> {
    if bytes.contains(&0) {
        return Err(TextError::Binary);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            // The cut is at the very end: only its last character is lost.
            if truncated && error.error_len().is_none() {
                return Ok(String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned());
            }
            let text = String::from_utf8_lossy(bytes);
            let damaged = text.matches('\u{FFFD}').count();
            if damaged * 20 > text.chars().count() {
                Err(TextError::Binary)
            } else {
                Ok(text.into_owned())
            }
        }
    }
}

fn plain(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "You do not have permission to read this file.".to_owned(),
        std::io::ErrorKind::NotFound => "This file no longer exists.".to_owned(),
        _ => "This file could not be read.".to_owned(),
    }
}

/// A step between neighbours with the arrow keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
    Up,
    Down,
}

/// The row after stepping from `from` among `len` rows laid out `columns`
/// to a line (a list has one). `None` at an edge: the preview stays on the
/// last file instead of wrapping round.
pub fn neighbour(from: usize, len: usize, step: Step, columns: usize) -> Option<usize> {
    let columns = columns.max(1);
    let to = match step {
        Step::Previous => from.checked_sub(1)?,
        Step::Next => from + 1,
        Step::Up => from.checked_sub(columns)?,
        Step::Down => from + columns,
    };
    (from < len && to < len).then_some(to)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::mime::{DIRECTORY, for_name};

    fn kind(name: &str) -> PreviewKind {
        kind_of(Kind::File, &for_name(name))
    }

    #[test]
    fn the_kind_follows_the_type() {
        assert_eq!(kind_of(Kind::Directory, &DIRECTORY), PreviewKind::Folder);
        assert_eq!(kind("photo.JPG"), PreviewKind::Image);
        assert_eq!(kind("logo.svg"), PreviewKind::Image);
        for name in ["notes.txt", "main.rs", "README.md", "data.json", "site.html"] {
            assert_eq!(kind(name), PreviewKind::Text, "{name}");
        }
        for name in ["paper.pdf", "song.mp3", "film.mkv", "letter.docx"] {
            assert_eq!(kind(name), PreviewKind::Poster, "{name}");
        }
        assert_eq!(kind("backup.zip"), PreviewKind::Icon);
        assert_eq!(kind("mystery.bin7"), PreviewKind::Sniff);
        assert_eq!(kind_of(Kind::Other, &for_name("sock")), PreviewKind::Icon);
    }

    #[test]
    fn stepping_stops_at_the_edges() {
        assert_eq!(neighbour(0, 5, Step::Next, 1), Some(1));
        assert_eq!(neighbour(0, 5, Step::Previous, 1), None);
        assert_eq!(neighbour(4, 5, Step::Next, 1), None);
        // A grid of three columns: down from 1 is 4, and 4 has nothing below.
        assert_eq!(neighbour(1, 5, Step::Down, 3), Some(4));
        assert_eq!(neighbour(4, 5, Step::Down, 3), None);
        assert_eq!(neighbour(4, 5, Step::Up, 3), Some(1));
        assert_eq!(neighbour(1, 5, Step::Up, 3), None);
        // A list has one column, so up and down step by one.
        assert_eq!(neighbour(2, 5, Step::Down, 0), Some(3));
        assert_eq!(neighbour(9, 5, Step::Previous, 1), None, "from past the end");
    }

    fn temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-preview-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn a_short_file_is_read_whole() {
        let path = temp("short.txt", "héllo\nworld\n".as_bytes());
        let head = read_text(&path, &Cancel::new()).unwrap();
        assert_eq!(head.text, "héllo\nworld\n");
        assert!(!head.truncated);
    }

    #[test]
    fn a_long_file_is_cut_at_the_limit_on_a_character() {
        // 'é' is two bytes, so a limit that is odd cuts one in half.
        let text = "é".repeat(TEXT_LIMIT);
        let path = temp("long.txt", text.as_bytes());
        let head = read_text(&path, &Cancel::new()).unwrap();
        assert!(head.truncated);
        assert!(head.text.len() <= TEXT_LIMIT);
        assert!(head.text.chars().all(|character| character == 'é'));
        assert!(head.text.len() >= TEXT_LIMIT - 1);
    }

    #[test]
    fn a_file_of_exactly_the_limit_is_not_truncated() {
        let path = temp("exact.txt", &vec![b'a'; TEXT_LIMIT]);
        let head = read_text(&path, &Cancel::new()).unwrap();
        assert!(!head.truncated);
        assert_eq!(head.text.len(), TEXT_LIMIT);
    }

    #[test]
    fn binary_files_are_not_text() {
        let path = temp("data.bin", &[0x7f, b'E', b'L', b'F', 0, 1, 2, 3]);
        assert_eq!(read_text(&path, &Cancel::new()), Err(TextError::Binary));
        let noise: Vec<u8> = (0..400u32).map(|n| 0x80 + (n % 0x40) as u8).collect();
        let path = temp("noise.bin", &noise);
        assert_eq!(read_text(&path, &Cancel::new()), Err(TextError::Binary));
    }

    #[test]
    fn a_cancelled_read_and_a_missing_file_say_so() {
        let cancel = Cancel::new();
        cancel.cancel();
        let path = temp("cancel.txt", b"x");
        assert_eq!(read_text(&path, &cancel), Err(TextError::Cancelled));
        let missing = path.with_file_name("not-here.txt");
        assert!(matches!(
            read_text(&missing, &Cancel::new()),
            Err(TextError::Unreadable(_))
        ));
    }

    #[test]
    fn mostly_text_with_a_stray_byte_is_still_text() {
        let mut bytes = b"mostly fine text here, with one \xff stray byte".to_vec();
        bytes.extend_from_slice(b" and more text after it to keep the share low");
        let path = temp("stray.txt", &bytes);
        let head = read_text(&path, &Cancel::new()).unwrap();
        assert!(head.text.contains('\u{FFFD}'));
    }
}
