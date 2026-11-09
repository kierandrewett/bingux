//! Cut, copy and paste of files, on the typed clipboard
//! ([`crate::clipboard`]), so files cut here paste in another file manager
//! and the other way round.
//!
//! The clipboard is the source of truth. A process-wide note of what this
//! app cut last is kept only to draw those files faded until they move; it
//! is dropped once the clipboard holds something else.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use bingux_files::location::from_file_uri;

use crate::clipboard::Clipboard;
use crate::dnd::{Format, GNOME_COPIED_FILES, Payload, Received};

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

/// Hold `paths` for a later paste, and offer them to other programs as file
/// managers offer them: a URI list, GNOME's copied-files type (which says
/// cut or copy, so Nautilus and Dolphin move cut files) and the paths as
/// text.
pub(super) fn put(paths: Vec<PathBuf>, cut: bool) {
    Clipboard::general().write(Payload::copied_files(&paths, cut));
    *held() = Some(Held { paths, cut });
}

/// What a paste would bring: the files on the clipboard, cut or copied as
/// the program that put them there said (this one, or another file
/// manager). A plain URI list is a copy.
pub(super) fn take() -> Option<Held> {
    let clipboard = Clipboard::general();
    let from = |data: Received| {
        let paths = data.files();
        (!paths.is_empty()).then(|| Held {
            cut: data.is_cut() == Some(true),
            paths,
        })
    };
    let found = clipboard
        .read(Format::Mime(GNOME_COPIED_FILES.into()))
        .ok()
        .and_then(from)
        .or_else(|| clipboard.read(Format::Uris).ok().and_then(from))
        .or_else(|| {
            // Text that is a URI list (a terminal's copy of `file://` lines).
            let paths = parse_uri_list(&clipboard.read_text()?);
            (!paths.is_empty()).then_some(Held { paths, cut: false })
        });
    // Something else took the clipboard: what this app cut is no longer
    // waiting to move, so it stops looking cut.
    let mut held = held();
    if held.as_ref() != found.as_ref() {
        *held = None;
    }
    found
}

/// Whether a paste would bring any files: files on the clipboard (asked of
/// the types on offer, with no round trip to the other program), or text
/// that is a URI list.
pub(super) fn has_files() -> bool {
    let clipboard = Clipboard::general();
    clipboard.has(&Format::Uris)
        || (clipboard.has(&Format::Text)
            && clipboard
                .read_text()
                .is_some_and(|text| !parse_uri_list(&text).is_empty()))
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
    fn a_uri_list_round_trips() {
        let paths = vec![PathBuf::from("/home/a b/c.txt"), PathBuf::from("/tmp/é")];
        let text = paths
            .iter()
            .map(|path| bingux_files::location::file_uri(path))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(parse_uri_list(&text), paths);
    }

    // Failure mode: files cut here paste as a copy, in this app or in
    // another file manager, because the clipboard lost the word "cut".
    #[test]
    fn cut_files_paste_as_a_move() {
        let paths = vec![PathBuf::from("/tmp/cut-a"), PathBuf::from("/tmp/cut b")];
        put(paths.clone(), true);
        assert!(has_files());
        assert_eq!(
            take(),
            Some(Held {
                paths: paths.clone(),
                cut: true
            })
        );
        assert!(is_cut(&paths[0]));
        put(paths.clone(), false);
        assert_eq!(take(), Some(Held { paths, cut: false }));
        clear();
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
