//! Recently used files, read from `~/.local/share/recently-used.xbel`.
//!
//! The file is the freedesktop recent-files list that GTK and Qt write. This
//! module only reads it, with a small scanner for the three things it needs
//! (`href`, `visited` and the MIME type of each `<bookmark>`), not a general
//! XML parser.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::entry::Entry;
use crate::listing::Listing;
use crate::location::from_file_uri;

/// One bookmark of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    /// Seconds since the epoch of the last visit, when the file says.
    pub visited: Option<i64>,
}

/// Every local-file bookmark in `xbel`, newest visit first.
pub fn parse(xbel: &str) -> Vec<Item> {
    let mut items: Vec<Item> = xbel
        .split("<bookmark ")
        .skip(1)
        .filter_map(|chunk| {
            let tag = &chunk[..chunk.find('>')?];
            let path = from_file_uri(&unescape(attribute(tag, "href")?))?;
            let visited = attribute(tag, "visited")
                .or_else(|| attribute(tag, "modified"))
                .and_then(parse_time);
            Some(Item { path, visited })
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.visited));
    items.dedup_by(|a, b| a.path == b.path);
    items
}

/// The value of `name="..."` in a tag's text.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let length = tag[start..].find('"')?;
    Some(&tag[start..start + length])
}

/// XML entities decoded: the five named ones and numeric references.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `2024-03-09T12:34:56Z` (with optional fraction) as epoch seconds.
fn parse_time(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-');
    let (year, month, day): (i64, i64, i64) = (
        date.next()?.parse().ok()?,
        date.next()?.parse().ok()?,
        date.next()?.parse().ok()?,
    );
    let time = time.trim_end_matches('Z');
    let mut time = time.split(':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: i64 = time.next()?.split('.').next()?.parse().ok()?;
    Some(days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second)
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

/// The recent files that still exist, newest first, at most `limit`, as a
/// listing. Reads the disk: call it on a worker.
pub fn list(xbel_path: &Path, limit: usize) -> Listing {
    let xbel = fs::read_to_string(xbel_path).unwrap_or_default();
    let mut entries = Vec::new();
    for item in parse(&xbel) {
        if entries.len() >= limit {
            break;
        }
        if let Ok(mut entry) = Entry::from_path(&item.path, false)
            && !entry.is_dir()
        {
            // The list is about when you used a file, not when it was saved.
            if let Some(visited) = item.visited.filter(|seconds| *seconds >= 0) {
                entry.modified = Some(UNIX_EPOCH + Duration::from_secs(visited as u64));
            }
            entries.push(entry);
        }
    }
    Listing {
        path: PathBuf::new(),
        recent: true,
        entries,
    }
}

/// `$XDG_DATA_HOME/recently-used.xbel`, or `~/.local/share/recently-used.xbel`.
pub fn default_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
        .join("recently-used.xbel")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file:///tmp/a%20b.txt" added="2024-01-01T00:00:00Z" modified="2024-01-02T00:00:00Z" visited="2024-01-03T10:00:00Z">
    <info><metadata owner="x"><mime:mime-type type="text/plain"/></metadata></info>
  </bookmark>
  <bookmark href="file:///tmp/R&amp;D.pdf" visited="2024-02-03T10:00:00.123Z"></bookmark>
  <bookmark href="https://example.org/x" visited="2025-01-01T00:00:00Z"></bookmark>
</xbel>"#;

    #[test]
    fn reads_local_bookmarks_newest_first() {
        let items = parse(SAMPLE);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, Path::new("/tmp/R&D.pdf"));
        assert_eq!(items[1].path, Path::new("/tmp/a b.txt"));
        assert_eq!(items[1].visited, Some(1_704_276_000));
    }
}
