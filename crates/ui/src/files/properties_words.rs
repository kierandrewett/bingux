//! The words of the Properties dialog: sizes with their byte counts, counts
//! while a folder is being added up, the kinds of several items, and where
//! they live. Plain functions of plain data, tested without a window.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bingux_files::ops::{ChildSize, FileDetails, Totals};
use bingux_files::{Entry, display};

/// A fact that has not arrived yet, and one that does not exist.
pub(super) const PENDING: &str = "…";
pub(super) const NONE: &str = "—";

/// `2,500,000`.
pub(super) fn grouped(number: u64) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `2.5 MB (2,500,000 bytes)`, or `512 bytes` when the unit is bytes.
pub(super) fn size_with_bytes(bytes: u64) -> String {
    let size = display::size(bytes);
    if bytes < 1000 {
        size
    } else {
        format!("{size} ({} bytes)", grouped(bytes))
    }
}

pub(super) fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The size line of a folder or a selection while and after it is counted.
pub(super) fn total_size(totals: Totals) -> String {
    let size = size_with_bytes(totals.bytes);
    if totals.cancelled {
        format!("At least {size} (stopped)")
    } else if !totals.done {
        format!("{size} so far…")
    } else if totals.unreadable > 0 {
        format!("At least {size}")
    } else {
        size
    }
}

/// How many items a folder or selection holds, while and after it is counted.
/// Files and folders are counted apart in [`kinds`].
pub(super) fn contents(totals: Totals) -> String {
    let mut text = plural(totals.items(), "item", "items");
    if totals.cancelled {
        text.insert_str(0, "At least ");
        text.push_str(" (stopped)");
    } else if !totals.done {
        text.push_str(" so far…");
    } else if totals.unreadable > 0 {
        text.insert_str(0, "At least ");
    }
    text
}

/// How many of the items are files and how many are folders: `("10", "62")`.
pub(super) fn kinds(totals: Totals) -> (String, String) {
    (grouped(totals.files), grouped(totals.folders))
}

/// The kinds of several items: "PNG image (3), Folder (2)", the most first.
pub(super) fn types_line(entries: &[Entry]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for entry in entries {
        match counts
            .iter_mut()
            .find(|(name, _)| *name == entry.mime.description)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((entry.mime.description, 1)),
        }
    }
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .iter()
        .map(|(name, count)| format!("{name} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The folder every one of `entries` is in, or `None` when they are in
/// several.
pub(super) fn common_parent(entries: &[Entry]) -> Option<PathBuf> {
    let first = entries.first()?.path.parent()?;
    entries
        .iter()
        .all(|entry| entry.path.parent() == Some(first))
        .then(|| first.to_path_buf())
}

/// `~/Pictures` for a folder under the home folder, else the whole path.
pub(super) fn home_relative(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

pub(super) fn time(value: Option<SystemTime>) -> Option<String> {
    value.map(display::full)
}

/// `kieran (1000)`, or just the number for an id with no name.
pub(super) fn named_id(name: &str, id: u32) -> String {
    if name == id.to_string() {
        name.to_owned()
    } else {
        format!("{name} ({id})")
    }
}

/// `259:2`, as `ls -l /dev` shows a device.
pub(super) fn device_line(details: &FileDetails) -> String {
    format!("{}:{}", details.device.0, details.device.1)
}

/// What a file takes on the drive, against what it holds: `4 KB on disk`.
pub(super) fn disk_usage(details: &FileDetails) -> String {
    format!("{} on disk", display::size(details.disk_bytes))
}

/// One of a folder's biggest items: its size, and for a folder how much is in
/// it.
pub(super) fn child_line(child: &ChildSize) -> String {
    let size = display::size(child.bytes);
    if child.is_dir {
        format!("{size}, {}", plural(child.items, "item", "items"))
    } else {
        size
    }
}

/// How far a read has got: `12 MB of 40 MB`, with the percentage.
pub(super) fn read_line(done: u64, total: u64) -> String {
    if total == 0 {
        return String::new();
    }
    let percent = (done as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
    format!("{} of {} ({percent:.0}%)", display::size(done), display::size(total))
}

/// A digest as two lines of 32 digits, so it fits a narrow card without a
/// cut and can be compared by eye.
pub(super) fn digest_lines(digest: &str) -> (String, String) {
    let (first, second) = digest.split_at(digest.len().min(32));
    (first.to_owned(), second.to_owned())
}

#[cfg(test)]
mod tests {
    use bingux_files::Kind;

    use super::*;

    fn entry(path: &str, kind: Kind) -> Entry {
        let path = PathBuf::from(path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        Entry::named(path, name, kind, false, false)
    }

    #[test]
    fn numbers_are_grouped_in_threes() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(2_500_000), "2,500,000");
        assert_eq!(grouped(123_456_789_012), "123,456,789,012");
    }

    #[test]
    fn a_size_has_its_byte_count_past_a_kilobyte() {
        assert_eq!(size_with_bytes(512), "512 bytes");
        assert_eq!(size_with_bytes(1), "1 byte");
        assert_eq!(size_with_bytes(2_500_000), "2.5 MB (2,500,000 bytes)");
    }

    #[test]
    fn a_running_count_says_so_and_a_stopped_one_says_at_least() {
        let running = Totals {
            bytes: 1_500_000,
            files: 3,
            folders: 1,
            ..Totals::default()
        };
        assert_eq!(total_size(running), "1.5 MB (1,500,000 bytes) so far…");
        assert_eq!(contents(running), "4 items so far…");
        let done = Totals {
            done: true,
            ..running
        };
        assert_eq!(total_size(done), "1.5 MB (1,500,000 bytes)");
        assert_eq!(contents(done), "4 items");
        assert_eq!(kinds(done), ("3".to_owned(), "1".to_owned()));
        let stopped = Totals {
            done: true,
            cancelled: true,
            ..running
        };
        assert_eq!(total_size(stopped), "At least 1.5 MB (1,500,000 bytes) (stopped)");
        assert_eq!(contents(stopped), "At least 4 items (stopped)");
        let locked = Totals {
            done: true,
            unreadable: 1,
            ..running
        };
        assert_eq!(total_size(locked), "At least 1.5 MB (1,500,000 bytes)");
    }

    #[test]
    fn several_items_name_their_types_most_first() {
        let entries = [
            entry("/a/one.png", Kind::File),
            entry("/a/dir", Kind::Directory),
            entry("/a/two.png", Kind::File),
        ];
        assert_eq!(types_line(&entries), "PNG image (2), Folder (1)");
    }

    #[test]
    fn a_location_under_home_is_written_with_a_tilde() {
        let home = Path::new("/home/ana");
        assert_eq!(home_relative(Path::new("/home/ana/Pictures"), home), "~/Pictures");
        assert_eq!(home_relative(Path::new("/home/ana"), home), "~");
        assert_eq!(home_relative(Path::new("/srv/data"), home), "/srv/data");
        assert_eq!(home_relative(Path::new("/home/anabel"), home), "/home/anabel");
    }

    #[test]
    fn items_share_a_location_only_in_one_folder() {
        let together = [entry("/a/one", Kind::File), entry("/a/two", Kind::File)];
        assert_eq!(common_parent(&together), Some(PathBuf::from("/a")));
        let apart = [entry("/a/one", Kind::File), entry("/b/two", Kind::File)];
        assert_eq!(common_parent(&apart), None);
    }

    #[test]
    fn an_owner_is_a_name_and_its_number() {
        assert_eq!(named_id("kieran", 1000), "kieran (1000)");
        assert_eq!(named_id("1234", 1234), "1234");
    }

    #[test]
    fn a_big_item_says_how_much_is_in_a_folder() {
        let file = ChildSize {
            path: PathBuf::from("/a/f"),
            name: "f".into(),
            is_dir: false,
            bytes: 2_000_000,
            items: 0,
        };
        assert_eq!(child_line(&file), "2.0 MB");
        let folder = ChildSize {
            is_dir: true,
            items: 1,
            ..file
        };
        assert_eq!(child_line(&folder), "2.0 MB, 1 item");
    }

    #[test]
    fn a_read_says_how_far_it_is() {
        assert_eq!(read_line(0, 0), "");
        assert_eq!(read_line(500_000, 2_000_000), "500 kB of 2.0 MB (25%)");
        assert_eq!(read_line(3_000_000, 2_000_000), "3.0 MB of 2.0 MB (100%)");
    }

    #[test]
    fn a_digest_splits_into_two_equal_lines() {
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let (a, b) = digest_lines(digest);
        assert_eq!((a.len(), b.len()), (32, 32));
        assert_eq!(format!("{a}{b}"), digest);
        assert_eq!(digest_lines(""), (String::new(), String::new()));
    }
}
