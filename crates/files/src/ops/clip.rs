//! Text that carries file lists: `text/uri-list` (drag and drop, copy) and
//! `x-special/gnome-copied-files` (the clipboard format file managers share).

use std::path::{Path, PathBuf};

use crate::location::{file_uri, from_file_uri};

/// The `file://` URI of a path, percent-encoded.
pub fn uri_of(path: &Path) -> String {
    file_uri(path)
}

/// The files in a `text/uri-list`: one per line, `#` lines are comments,
/// CRLF or LF. Only `file://` URIs count; they are percent-decoded. A line
/// that is a plain absolute path is taken as it is. Everything else (web
/// links, blank lines) is dropped.
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            if line.starts_with("file://") {
                from_file_uri(line)
            } else if line.starts_with('/') {
                Some(PathBuf::from(line))
            } else {
                None
            }
        })
        .collect()
}

/// The clipboard text for files that were copied or cut:
/// `copy\nfile:///a\nfile:///b`, or `cut` for a cut.
pub fn gnome_copied_files(paths: &[PathBuf], cut: bool) -> String {
    let mut text = String::from(if cut { "cut" } else { "copy" });
    for path in paths {
        text.push('\n');
        text.push_str(&uri_of(path));
    }
    text
}

/// Read `x-special/gnome-copied-files`: whether it was a cut, and the
/// files. `None` if the first line is not `copy` or `cut`, or no file
/// follows.
pub fn parse_gnome_copied_files(text: &str) -> Option<(bool, Vec<PathBuf>)> {
    let text = text.trim_start();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let cut = match first.trim() {
        "copy" => false,
        "cut" => true,
        _ => return None,
    };
    let paths = parse_uri_list(rest);
    (!paths.is_empty()).then_some((cut, paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn uri_list_reads_files_comments_and_crlf() {
        let text = "# a comment\r\nfile:///home/me/a%20b.txt\r\n\r\nfile://localhost/tmp/c\r\nhttps://example.com/x\r\n/plain/path.txt\nrelative.txt\n";
        assert_eq!(
            parse_uri_list(text),
            paths(&["/home/me/a b.txt", "/tmp/c", "/plain/path.txt"])
        );
        assert!(parse_uri_list("").is_empty());
        assert!(parse_uri_list("# only a comment").is_empty());
    }

    #[test]
    fn uri_list_decodes_utf8_and_percent() {
        let uri = uri_of(Path::new("/home/me/ünï 100%.txt"));
        assert_eq!(uri, "file:///home/me/%C3%BCn%C3%AF%20100%25.txt");
        assert_eq!(parse_uri_list(&uri), paths(&["/home/me/ünï 100%.txt"]));
    }

    #[test]
    fn gnome_format_round_trip() {
        let files = paths(&["/a/one", "/b/t wo"]);
        let copied = gnome_copied_files(&files, false);
        assert_eq!(copied, "copy\nfile:///a/one\nfile:///b/t%20wo");
        assert_eq!(
            parse_gnome_copied_files(&copied),
            Some((false, files.clone()))
        );
        let cut = gnome_copied_files(&files, true);
        assert!(cut.starts_with("cut\n"));
        assert_eq!(parse_gnome_copied_files(&cut), Some((true, files)));
    }

    #[test]
    fn gnome_format_tolerates_other_writers() {
        assert_eq!(
            parse_gnome_copied_files("cut\r\nfile:///a\r\nfile:///b\r\n"),
            Some((true, paths(&["/a", "/b"])))
        );
        assert_eq!(parse_gnome_copied_files("copy"), None, "no files");
        assert_eq!(parse_gnome_copied_files("paste\nfile:///a"), None);
        assert_eq!(parse_gnome_copied_files("file:///a"), None);
        assert_eq!(parse_gnome_copied_files(""), None);
    }
}
