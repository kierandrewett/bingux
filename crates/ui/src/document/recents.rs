//! The recently used files, in `recently-used.xbel`.
//!
//! One list for the whole desktop: GTK and Qt programs write
//! `$XDG_DATA_HOME/recently-used.xbel`, and the Files app and every file
//! chooser read it (the Recent place). A document that registers here shows
//! in all of them, and a file opened in another program shows in the
//! document's own "Open Recent" menu.
//!
//! [`register`] adds or refreshes a file (opening it, saving it), [`forget`]
//! removes one, [`clear`] empties the list and [`list`] reads it. Every
//! function reads or writes the disk: call them on a worker.
//!
//! Writing is a read, a change of the one bookmark and an atomic replace of
//! the file, so what other programs wrote is kept as it is: their other
//! bookmarks and, in a bookmark that is changed, the other programs'
//! application entries and the time it was first added. Two programs saving
//! at the same instant can lose one registration; the file is never torn.

use std::io;
use std::path::{Path, PathBuf};

use bingux_files::location::{file_uri, from_file_uri};

use super::write::write_atomic;
use crate::format::{Moment, Zone};

/// The most bookmarks the file keeps; the oldest go first.
pub const LIMIT: usize = 500;

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<xbel version=\"1.0\"\n      \
xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\"\n      \
xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\"\n>\n";
const TAIL: &str = "</xbel>\n";

/// `$XDG_DATA_HOME/recently-used.xbel`, or `~/.local/share/recently-used.xbel`.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    bingux_files::recent::default_path(&home)
}

/// What a document tells the list about itself.
#[derive(Clone, Copy, Debug)]
pub struct Registration<'a> {
    /// The file.
    pub path: &'a Path,
    /// Its MIME type, when known (`text/plain`).
    pub mime: Option<&'a str>,
    /// The application's name as the person knows it ("Text Editor").
    pub app_name: &'a str,
    /// The command that opens a file, without arguments (`bingux-text-editor`).
    pub exec: &'a str,
}

/// The text of the list at `xbel`, with what is not valid UTF-8 replaced (one
/// stray byte in another program's bookmark does not cost the whole list).
/// A list that is not there is an empty one. A list that cannot be read for
/// another reason is an error: writing over it would lose what is in it.
fn read_text(xbel: &Path) -> io::Result<String> {
    match std::fs::read(xbel) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

/// Adds `registration.path` to the list at `xbel`, or moves it to the front
/// with its use counted, as of `now`.
pub fn register_at(xbel: &Path, registration: &Registration, now: Moment) -> io::Result<()> {
    let text = read_text(xbel)?;
    write_atomic(xbel, with_bookmark(&text, registration, now).as_bytes())
        .map(|_| ())
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

/// [`register_at`] on the default list, as of now.
pub fn register(registration: &Registration) -> io::Result<()> {
    register_at(&default_path(), registration, Moment::now())
}

/// Removes `path` from the list at `xbel`.
pub fn forget_at(xbel: &Path, path: &Path) -> io::Result<()> {
    let text = read_text(xbel)?;
    let kept = without_bookmark(&text, path);
    if kept == text {
        return Ok(());
    }
    write_atomic(xbel, kept.as_bytes())
        .map(|_| ())
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

/// [`forget_at`] on the default list.
pub fn forget(path: &Path) -> io::Result<()> {
    forget_at(&default_path(), path)
}

/// Empties the list at `xbel`: every program's entries.
pub fn clear_at(xbel: &Path) -> io::Result<()> {
    if !xbel.exists() {
        return Ok(());
    }
    write_atomic(xbel, format!("{HEAD}{TAIL}").as_bytes())
        .map(|_| ())
        .map_err(|error| io::Error::other(format!("{error:?}")))
}

/// [`clear_at`] on the default list.
pub fn clear() -> io::Result<()> {
    clear_at(&default_path())
}

/// The files of the list that still exist, the most recently used first, at
/// most `limit`. A file on a network mount is listed without looking at it.
pub fn list_at(xbel: &Path, limit: usize) -> Vec<PathBuf> {
    bingux_files::recent::list(xbel, limit)
        .entries
        .into_iter()
        .map(|entry| entry.path)
        .collect()
}

/// [`list_at`] on the default list.
pub fn list(limit: usize) -> Vec<PathBuf> {
    list_at(&default_path(), limit)
}

/// The files of the list at `xbel` that `app_name` registered and that still
/// exist, the most recently used first, at most `limit`: an editor's "Open
/// Recent" lists the documents it opened, not the pictures another program
/// showed. A list that cannot be read is an empty one.
pub fn list_of_app_at(xbel: &Path, app_name: &str, limit: usize) -> Vec<PathBuf> {
    let Ok(text) = read_text(xbel) else {
        return Vec::new();
    };
    let (_, chunks, _) = split(&text);
    let mut found: Vec<(String, PathBuf)> = chunks
        .iter()
        .filter(|chunk| {
            applications(chunk).iter().any(|application| {
                attribute(application, "name").map(unescape).as_deref() == Some(app_name)
            })
        })
        .filter_map(|chunk| {
            let visited = chunk
                .find('>')
                .and_then(|close| attribute(&chunk[..close], "visited"))
                .unwrap_or_default()
                .to_string();
            Some((visited, bookmark_path(chunk)?))
        })
        .collect();
    // ISO 8601 times in one zone sort as text.
    found.sort_by(|a, b| b.0.cmp(&a.0));
    let mut paths: Vec<PathBuf> = Vec::new();
    for (_, path) in found {
        if !paths.contains(&path) && path.exists() {
            paths.push(path);
            if paths.len() == limit {
                break;
            }
        }
    }
    paths
}

/// [`list_of_app_at`] on the default list.
pub fn list_of_app(app_name: &str, limit: usize) -> Vec<PathBuf> {
    list_of_app_at(&default_path(), app_name, limit)
}

// --- the text of the file ---------------------------------------------------

/// The file's text in three parts: what comes before the first bookmark,
/// the bookmarks, and the closing tag.
fn split(text: &str) -> (&str, Vec<&str>, &str) {
    let tail_at = text.rfind("</xbel>").unwrap_or(text.len());
    let body = &text[..tail_at];
    let first = body.find("<bookmark ").unwrap_or(body.len());
    let mut chunks = Vec::new();
    let mut rest = &body[first..];
    while let Some(start) = rest.find("<bookmark ") {
        let from = &rest[start..];
        let end = match (from.find("</bookmark>"), from.find("/>"), from.find('>')) {
            // A bookmark that closes itself: `<bookmark ... />`.
            (_, Some(slash), Some(close)) if slash + 1 == close => close + 1,
            (Some(end), ..) => end + "</bookmark>".len(),
            _ => break,
        };
        // A bookmark that was cut off (a crash while another program wrote
        // the list) runs into the next one: it is dropped, and the next one
        // is kept.
        if let Some(next) = from[1..].find("<bookmark ")
            && next + 1 < end
        {
            rest = &from[next + 1..];
            continue;
        }
        chunks.push(&from[..end]);
        rest = &from[end..];
    }
    (&body[..first], chunks, &text[tail_at..])
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let length = tag[start..].find('"')?;
    Some(&tag[start..start + length])
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The path a bookmark is for.
fn bookmark_path(chunk: &str) -> Option<PathBuf> {
    let tag = &chunk[..chunk.find('>')?];
    from_file_uri(&unescape(attribute(tag, "href")?))
}

fn iso(moment: Moment) -> String {
    let c = Zone::Utc.civil(moment);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// The `<bookmark:application .../>` elements of a bookmark.
fn applications(chunk: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = chunk;
    while let Some(start) = rest.find("<bookmark:application ") {
        let from = &rest[start..];
        let Some(end) = from.find("/>") else { break };
        found.push(&from[..end + 2]);
        rest = &from[end + 2..];
    }
    found
}

/// What a list starts with when it is written again: the file's own opening
/// (the XML declaration and the `<xbel ...>` tag, with its namespaces), cut
/// free of any junk before it, or a fresh one when the file had no complete
/// `<xbel>` tag.
fn clean_head(head: &str) -> &str {
    match head.find("<xbel") {
        Some(xbel) if head[xbel..].contains('>') => {
            let start = head[..xbel].rfind("<?xml").unwrap_or(xbel);
            &head[start..]
        }
        _ => HEAD,
    }
}

fn with_bookmark(text: &str, registration: &Registration, now: Moment) -> String {
    let (head, mut chunks, _) = split(text);
    let stamp = iso(now);
    let existing = chunks
        .iter()
        .position(|chunk| bookmark_path(chunk).as_deref() == Some(registration.path));
    let previous = existing.map(|at| chunks.remove(at));
    let added = previous
        .and_then(|chunk| attribute(&chunk[..chunk.find('>')?], "added"))
        .map_or_else(|| stamp.clone(), str::to_string);
    let mime = registration.mime.map(str::to_string).or_else(|| {
        let chunk = previous?;
        let at = chunk.find("<mime:mime-type ")?;
        attribute(&chunk[at..chunk[at..].find('>')? + at], "type").map(unescape)
    });
    let mut count = 1;
    let mut others: Vec<&str> = Vec::new();
    for application in previous.map(applications).unwrap_or_default() {
        if attribute(application, "name").map(unescape).as_deref() == Some(registration.app_name) {
            count = attribute(application, "count")
                .and_then(|text| text.parse::<u64>().ok())
                .map_or(1, |count| count.saturating_add(1));
        } else {
            others.push(application);
        }
    }
    let mut bookmark = format!(
        "  <bookmark href=\"{}\" added=\"{added}\" modified=\"{stamp}\" visited=\"{stamp}\">\n    <info>\n      <metadata owner=\"http://freedesktop.org\">\n",
        escape(&file_uri(registration.path)),
    );
    if let Some(mime) = mime {
        bookmark.push_str(&format!(
            "        <mime:mime-type type=\"{}\"/>\n",
            escape(&mime)
        ));
    }
    bookmark.push_str("        <bookmark:applications>\n");
    bookmark.push_str(&format!(
        "          <bookmark:application name=\"{}\" exec=\"&apos;{} %u&apos;\" modified=\"{stamp}\" count=\"{count}\"/>\n",
        escape(registration.app_name),
        escape(registration.exec),
    ));
    for other in others {
        bookmark.push_str("          ");
        bookmark.push_str(other);
        bookmark.push('\n');
    }
    bookmark.push_str(
        "        </bookmark:applications>\n      </metadata>\n    </info>\n  </bookmark>\n",
    );
    let head = clean_head(head);
    let skip = (chunks.len() + 1).saturating_sub(LIMIT);
    let mut out = String::from(head);
    for chunk in chunks.iter().skip(skip) {
        out.push_str("  ");
        out.push_str(chunk.trim_start());
        out.push('\n');
    }
    out.push_str(&bookmark);
    out.push_str(TAIL);
    out
}

fn without_bookmark(text: &str, path: &Path) -> String {
    let (head, chunks, _) = split(text);
    if !chunks
        .iter()
        .any(|chunk| bookmark_path(chunk).as_deref() == Some(path))
    {
        return text.to_string();
    }
    let mut out = String::from(clean_head(head));
    for chunk in chunks
        .iter()
        .filter(|chunk| bookmark_path(chunk).as_deref() != Some(path))
    {
        out.push_str("  ");
        out.push_str(chunk.trim_start());
        out.push('\n');
    }
    out.push_str(TAIL);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    const SOMEONE_ELSES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0"
      xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks"
      xmlns:mime="http://www.freedesktop.org/standards/shared-mime-info"
>
  <bookmark href="file:///home/a/old.txt" added="2024-01-01T00:00:00Z" modified="2024-01-02T00:00:00Z" visited="2024-01-03T00:00:00Z">
    <info>
      <metadata owner="http://freedesktop.org">
        <mime:mime-type type="text/plain"/>
        <bookmark:applications>
          <bookmark:application name="gedit" exec="&apos;gedit %u&apos;" modified="2024-01-03T00:00:00Z" count="4"/>
        </bookmark:applications>
      </metadata>
    </info>
  </bookmark>
</xbel>
"#;

    fn reg<'a>(path: &'a Path, app: &'a str) -> Registration<'a> {
        Registration {
            path,
            mime: Some("text/plain"),
            app_name: app,
            exec: "bingux-text-editor",
        }
    }

    fn paths(text: &str) -> Vec<PathBuf> {
        bingux_files::recent::parse(text)
            .into_iter()
            .map(|item| item.path)
            .collect()
    }

    #[test]
    fn a_new_file_is_added_beside_what_other_programs_wrote() {
        let now = Moment::from_unix(1_762_700_000);
        let out = with_bookmark(
            SOMEONE_ELSES,
            &reg(Path::new("/home/a/new.txt"), "Text Editor"),
            now,
        );
        assert_eq!(
            paths(&out),
            [
                PathBuf::from("/home/a/new.txt"),
                PathBuf::from("/home/a/old.txt")
            ]
        );
        assert!(
            out.contains("name=\"gedit\""),
            "the other program's bookmark is kept"
        );
        assert!(out.contains("visited=\"2025-11-09T14:53:20Z\""));
        assert!(out.ends_with("</xbel>\n"));
    }

    #[test]
    fn using_a_file_again_counts_the_use_and_keeps_when_it_was_added() {
        let first = Moment::from_unix(1_762_700_000);
        let later = Moment::from_unix(1_762_800_000);
        let path = Path::new("/home/a/new.txt");
        let once = with_bookmark("", &reg(path, "Text Editor"), first);
        let twice = with_bookmark(&once, &reg(path, "Text Editor"), later);
        assert_eq!(paths(&twice).len(), 1);
        assert!(twice.contains("added=\"2025-11-09T14:53:20Z\""));
        assert!(twice.contains("visited=\"2025-11-10T18:40:00Z\""));
        assert!(twice.contains("count=\"2\""));
    }

    #[test]
    fn another_program_using_the_same_file_keeps_both_application_entries() {
        let now = Moment::from_unix(1_762_700_000);
        let path = Path::new("/home/a/old.txt");
        let out = with_bookmark(SOMEONE_ELSES, &reg(path, "Text Editor"), now);
        assert_eq!(paths(&out), [path.to_path_buf()]);
        assert!(out.contains("name=\"gedit\"") && out.contains("count=\"4\""));
        assert!(out.contains("name=\"Text Editor\"") && out.contains("count=\"1\""));
        // The file's MIME type is kept when the caller gives none.
        let none = Registration {
            mime: None,
            ..reg(path, "Other")
        };
        assert!(with_bookmark(SOMEONE_ELSES, &none, now).contains("type=\"text/plain\""));
    }

    #[test]
    fn names_with_markup_spaces_and_bytes_that_are_not_text_survive() {
        let now = Moment::from_unix(1_762_700_000);
        let odd = PathBuf::from("/tmp/R&D <draft> \"final\" 'v2'.txt");
        let bytes = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/caf\xe9.txt".to_vec()));
        let mut text = String::new();
        for path in [&odd, &bytes] {
            text = with_bookmark(&text, &reg(path, "Text & Notes <1>"), now);
        }
        assert_eq!(paths(&text), [odd, bytes]);
        assert!(!text.contains("R&D"), "the ampersand is escaped");
    }

    #[test]
    fn a_missing_empty_or_foreign_file_starts_a_list() {
        let now = Moment::from_unix(1_762_700_000);
        let path = Path::new("/tmp/a.txt");
        for given in ["", "not xml at all", "<xbel></xbel>"] {
            let out = with_bookmark(given, &reg(path, "Text Editor"), now);
            assert_eq!(paths(&out), [path.to_path_buf()], "{given:?}");
            assert!(
                out.starts_with("<?xml") || out.starts_with("<xbel"),
                "{given:?}"
            );
            assert!(out.trim_end().ends_with("</xbel>"));
        }
    }

    const THEIRS_TWO: &str = r#"  <bookmark href="file:///home/a/two.txt" added="2024-02-01T00:00:00Z" modified="2024-02-02T00:00:00Z" visited="2024-02-03T00:00:00Z">
    <info>
      <metadata owner="http://freedesktop.org">
        <bookmark:applications>
          <bookmark:application name="gedit" exec="&apos;gedit %u&apos;" modified="2024-02-03T00:00:00Z" count="1"/>
        </bookmark:applications>
      </metadata>
    </info>
  </bookmark>
"#;

    #[test]
    fn a_list_cut_off_in_the_middle_of_a_bookmark_keeps_the_whole_ones() {
        let now = Moment::from_unix(1_762_700_000);
        let whole = SOMEONE_ELSES.replace("</xbel>\n", "");
        // The second bookmark stops half way, and the closing tag is missing.
        let cut = format!(
            "{whole}  <bookmark href=\"file:///home/a/cut.txt\" added=\"2024-02-01T00:00:00Z\">\n    <info>\n      <meta"
        );
        let out = with_bookmark(&cut, &reg(Path::new("/home/a/new.txt"), "Text Editor"), now);
        assert_eq!(
            paths(&out),
            [
                PathBuf::from("/home/a/new.txt"),
                PathBuf::from("/home/a/old.txt")
            ]
        );
        assert!(out.trim_end().ends_with("</xbel>"));
        // What is written parses as whole bookmarks only.
        let (_, chunks, _) = split(&out);
        assert!(chunks.iter().all(|chunk| chunk.ends_with("</bookmark>")));
    }

    #[test]
    fn a_bookmark_that_never_closes_does_not_swallow_the_one_after_it() {
        let now = Moment::from_unix(1_762_700_000);
        let broken = "  <bookmark href=\"file:///home/a/broken.txt\" added=\"2024-02-01T00:00:00Z\">\n    <info>\n";
        let text = SOMEONE_ELSES.replacen("  <bookmark", &format!("{broken}  <bookmark"), 1);
        let out = with_bookmark(
            &text,
            &reg(Path::new("/home/a/new.txt"), "Text Editor"),
            now,
        );
        assert_eq!(
            paths(&out),
            [
                PathBuf::from("/home/a/new.txt"),
                PathBuf::from("/home/a/old.txt")
            ]
        );
    }

    #[test]
    fn garbage_around_the_bookmarks_is_not_carried_into_the_new_list() {
        let now = Moment::from_unix(1_762_700_000);
        let text = format!(
            "\u{0}\u{1}binary \u{fffd}\u{fffd} junk\n<xbel version=\"1.0\">\n{THEIRS_TWO}\u{0}\u{0}</xbel>trailing junk"
        );
        let out = with_bookmark(
            &text,
            &reg(Path::new("/home/a/new.txt"), "Text Editor"),
            now,
        );
        assert_eq!(
            paths(&out),
            [
                PathBuf::from("/home/a/new.txt"),
                PathBuf::from("/home/a/two.txt")
            ]
        );
        assert!(
            out.starts_with("<xbel"),
            "the head is the first <xbel…> the file had or a fresh one"
        );
        assert!(!out.contains("binary"), "{out}");
    }

    #[test]
    fn a_list_with_a_stray_byte_is_read_lossily_and_one_that_cannot_be_read_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("bingux-recents-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xbel = dir.join("recently-used.xbel");
        let real = dir.join("real.txt");
        std::fs::write(&real, "x").unwrap();
        // One byte that is not UTF-8 inside another program's bookmark.
        let mut bytes = SOMEONE_ELSES.as_bytes().to_vec();
        let at = bytes.windows(5).position(|w| w == b"gedit").unwrap();
        bytes[at] = 0xff;
        std::fs::write(&xbel, bytes).unwrap();
        register_at(&xbel, &reg(&real, "Text Editor"), Moment::from_unix(200)).unwrap();
        let written = std::fs::read_to_string(&xbel).unwrap();
        assert!(
            written.contains("file:///home/a/old.txt"),
            "their bookmark is kept"
        );
        assert!(written.contains(&real.display().to_string()));
        // A path that is a folder cannot be read as a list: nothing is
        // written over it, and the caller is told.
        let folder = dir.join("folder.xbel");
        std::fs::create_dir(&folder).unwrap();
        assert!(register_at(&folder, &reg(&real, "Text Editor"), Moment::from_unix(1)).is_err());
        assert!(folder.is_dir());
        assert!(forget_at(&folder, &real).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_documents_of_one_app_are_listed_newest_first_and_other_apps_are_left_out() {
        let dir = std::env::temp_dir().join(format!("bingux-recents-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xbel = dir.join("recently-used.xbel");
        let (a, b, c) = (dir.join("a.txt"), dir.join("b.txt"), dir.join("c.png"));
        for path in [&a, &b, &c] {
            std::fs::write(path, "x").unwrap();
        }
        register_at(&xbel, &reg(&a, "Text Editor"), Moment::from_unix(100)).unwrap();
        register_at(&xbel, &reg(&c, "Preview"), Moment::from_unix(200)).unwrap();
        register_at(&xbel, &reg(&b, "Text Editor"), Moment::from_unix(300)).unwrap();
        register_at(&xbel, &reg(&a, "Text Editor"), Moment::from_unix(400)).unwrap();
        assert_eq!(
            list_of_app_at(&xbel, "Text Editor", 10),
            [a.clone(), b.clone()]
        );
        assert_eq!(list_of_app_at(&xbel, "Preview", 10), [c.clone()]);
        assert_eq!(list_of_app_at(&xbel, "Text Editor", 1), [a]);
        // Both programs used the file: both lists have it.
        register_at(&xbel, &reg(&b, "Preview"), Moment::from_unix(500)).unwrap();
        assert_eq!(list_of_app_at(&xbel, "Preview", 10), [b, c]);
        assert!(list_of_app_at(&dir.join("none.xbel"), "Preview", 10).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_list_is_capped_and_the_oldest_go() {
        let now = Moment::from_unix(1_762_700_000);
        let mut text = String::new();
        for n in 0..LIMIT + 3 {
            let path = PathBuf::from(format!("/tmp/f{n}.txt"));
            text = with_bookmark(
                &text,
                &reg(&path, "Text Editor"),
                now.plus_seconds(n as i64),
            );
        }
        let (_, chunks, _) = split(&text);
        assert_eq!(chunks.len(), LIMIT);
        assert!(bookmark_path(chunks[0]).unwrap().ends_with("f3.txt"));
        assert!(
            bookmark_path(chunks[LIMIT - 1])
                .unwrap()
                .ends_with(format!("f{}.txt", LIMIT + 2))
        );
    }

    #[test]
    fn forgetting_removes_one_file_and_changes_nothing_else() {
        let text = SOMEONE_ELSES.to_string();
        assert_eq!(without_bookmark(&text, Path::new("/nope")), text);
        let out = without_bookmark(&text, Path::new("/home/a/old.txt"));
        assert!(paths(&out).is_empty());
        assert!(out.contains("<xbel") && out.trim_end().ends_with("</xbel>"));
    }

    #[test]
    fn the_file_on_disk_is_written_whole_and_listed() {
        let dir = std::env::temp_dir().join(format!("bingux-recents-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let xbel = dir.join("recently-used.xbel");
        let real = dir.join("real.txt");
        std::fs::write(&real, "x").unwrap();
        let gone = dir.join("gone.txt");
        register_at(&xbel, &reg(&gone, "Text Editor"), Moment::from_unix(100)).unwrap();
        register_at(&xbel, &reg(&real, "Text Editor"), Moment::from_unix(200)).unwrap();
        // A file that is no longer there is not offered.
        assert_eq!(list_at(&xbel, 10), [real.clone()]);
        forget_at(&xbel, &real).unwrap();
        assert!(list_at(&xbel, 10).is_empty());
        register_at(&xbel, &reg(&real, "Text Editor"), Moment::from_unix(300)).unwrap();
        clear_at(&xbel).unwrap();
        assert!(list_at(&xbel, 10).is_empty());
        // Nothing to forget or clear in a list that is not there.
        forget_at(&dir.join("absent.xbel"), &real).unwrap();
        clear_at(&dir.join("absent.xbel")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
