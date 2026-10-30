//! Cut, copy and paste of files.
//!
//! The held files live in a process-wide static, which is the source of
//! truth: it knows whether the files were cut, and it needs no round trip
//! through the windowing system. The same files are also written to the system
//! clipboard as a `file://` URI list, one per line, so another program can
//! paste them as text, and a URI list copied elsewhere can be pasted here.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use bingux_files::location::{file_uri, from_file_uri};

/// The files a person cut or copied last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Held {
    pub paths: Vec<PathBuf>,
    /// Cut files move when pasted; copied files are copied.
    pub cut: bool,
}

static HELD: Mutex<Option<Held>> = Mutex::new(None);

fn held() -> std::sync::MutexGuard<'static, Option<Held>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The URI list of `paths`, one `file://` URI per line.
pub(super) fn uri_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| file_uri(path))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The paths in a block of text that is a URI list. Lines that are not
/// `file://` URIs and blank lines are ignored; the comment lines of the
/// `text/uri-list` format start with `#`.
pub(super) fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(from_file_uri)
        .collect()
}

/// Hold `paths` for a later paste, and offer them to other programs.
pub(super) fn put(paths: Vec<PathBuf>, cut: bool) {
    // A system clipboard that cannot be written (no display, say) leaves the
    // in-app copy working.
    let _ = freya::clipboard::Clipboard::set(uri_list(&paths));
    *held() = Some(Held { paths, cut });
}

/// What a paste would bring: the held files, else a URI list on the system
/// clipboard (taken as a copy).
pub(super) fn take() -> Option<Held> {
    if let Some(held) = held().clone() {
        return Some(held);
    }
    let text = freya::clipboard::Clipboard::get().ok()?;
    let paths = parse_uri_list(&text);
    (!paths.is_empty()).then_some(Held { paths, cut: false })
}

/// Whether a paste would bring any files.
pub(super) fn has_files() -> bool {
    held().is_some()
        || freya::clipboard::Clipboard::get()
            .is_ok_and(|text| !parse_uri_list(&text).is_empty())
}

/// Forget the held files, after a cut was pasted: they are no longer there.
pub(super) fn clear() {
    *held() = None;
}

/// Whether `path` was cut and has not been pasted yet, so a view can draw it
/// faded.
pub(in crate::files) fn is_cut(path: &Path) -> bool {
    held()
        .as_ref()
        .is_some_and(|held| held.cut && held.paths.iter().any(|held| held == path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_round_trips() {
        let paths = vec![
            PathBuf::from("/home/a b/c.txt"),
            PathBuf::from("/tmp/é"),
        ];
        let text = uri_list(&paths);
        assert_eq!(text.lines().count(), 2);
        assert_eq!(parse_uri_list(&text), paths);
    }

    #[test]
    fn plain_text_is_not_a_uri_list() {
        assert!(parse_uri_list("hello world").is_empty());
        assert!(parse_uri_list("").is_empty());
        assert_eq!(
            parse_uri_list("# comment\r\nfile:///x\r\nhttps://example.org/\r\n"),
            vec![PathBuf::from("/x")]
        );
    }
}
