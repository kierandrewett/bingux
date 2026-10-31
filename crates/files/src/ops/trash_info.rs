//! What a bin keeps beside the items it holds, and where a path stands in a
//! bin: the `info/NAME.trashinfo` file of each item, the `directorysizes`
//! cache of the folders in it, and the shape of a bin's own location.
//!
//! A bin is `$XDG_DATA_HOME/Trash` at home, `$topdir/.Trash/$uid` where the
//! admin made a sticky `.Trash`, or `$topdir/.Trash-$uid`. Inside it, `files`
//! holds the items, `info` one `.trashinfo` file per item and `directorysizes`
//! the sizes of the folders in `files`.
//!
//! Everything here reads or writes plain bytes: a path is percent-encoded in
//! the `.trashinfo` file and decoded back to the exact bytes it had, so a
//! name that is not UTF-8 survives the round trip.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::listing::Cancel;
use crate::location::{file_uri, percent_decode};

use super::durable::sync_written;

/// `/path/with%20escapes`, as `Path=` holds it.
pub(super) fn encode_path(path: &Path) -> String {
    file_uri(path).trim_start_matches("file://").to_string()
}

/// The name of a folder as `directorysizes` holds it.
fn encode_name(name: &OsStr) -> String {
    encode_path(Path::new(name))
}

fn seconds_of(time: SystemTime) -> libc::time_t {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs()) as libc::time_t
}

/// Local time as `YYYY-MM-DDThh:mm:ss`, the form `DeletionDate=` has.
pub(super) fn format_date(time: SystemTime) -> String {
    let seconds = seconds_of(time);
    // SAFETY: `tm` is plain data; `localtime_r` fills it from a valid `time_t`.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&seconds, &mut tm);
        tm
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

/// The moment a `DeletionDate=` value names. The value is local time with no
/// zone, so the system's zone decides (daylight saving time included).
pub(super) fn parse_date(text: &str) -> Option<SystemTime> {
    let (date, time) = text.trim().split_once('T')?;
    let mut date = date.split('-');
    let year: i32 = date.next()?.parse().ok()?;
    let month: i32 = date.next()?.parse().ok()?;
    let day: i32 = date.next()?.parse().ok()?;
    // Seconds may carry a fraction or a zone suffix that nothing writes here.
    let mut time = time.split(':');
    let hour: i32 = time.next()?.parse().ok()?;
    let minute: i32 = time.next()?.parse().ok()?;
    let second: i32 = time
        .next()?
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .split('.')
        .next()?
        .parse()
        .ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || date.next().is_some() {
        return None;
    }
    // SAFETY: `tm` is plain data; `mktime` reads and normalises it.
    let seconds = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = year - 1900;
        tm.tm_mon = month - 1;
        tm.tm_mday = day;
        tm.tm_hour = hour;
        tm.tm_min = minute;
        tm.tm_sec = second;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm)
    };
    if seconds == -1 {
        return None;
    }
    Some(if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    })
}

/// What a `.trashinfo` file says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Info {
    /// `Path=` decoded. Relative to the drive's top folder for a drive's bin.
    pub path: PathBuf,
    pub deleted: Option<SystemTime>,
}

/// Read the text of a `.trashinfo` file: the `[Trash Info]` group with its
/// `Path` and `DeletionDate`. `None` when there is no group or no path.
pub(super) fn parse_info(text: &[u8]) -> Option<Info> {
    let mut in_group = false;
    let mut path = None;
    let mut deleted = None;
    for line in text.split(|byte| *byte == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b"[") {
            in_group = line == b"[Trash Info]";
            continue;
        }
        if !in_group {
            continue;
        }
        let Some(split) = line.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let (key, value) = (&line[..split], &line[split + 1..]);
        match key {
            b"Path" if path.is_none() => {
                let value = String::from_utf8_lossy(value);
                path = Some(PathBuf::from(OsString::from_vec(percent_decode(&value))));
            }
            b"DeletionDate" if deleted.is_none() => {
                deleted = parse_date(&String::from_utf8_lossy(value));
            }
            _ => {}
        }
    }
    path.filter(|path| !path.as_os_str().is_empty())
        .map(|path| Info { path, deleted })
}

/// The text of a `.trashinfo` file for an item that was at `recorded` and was
/// deleted at `when`.
pub(super) fn info_text(recorded: &Path, when: SystemTime) -> String {
    format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        encode_path(recorded),
        format_date(when)
    )
}

// ---- where a path stands in a bin ---------------------------------------------

/// Is `dir` a bin: the one at home, `$topdir/.Trash-$uid`, or
/// `$topdir/.Trash/$uid`?
pub(super) fn is_bin(dir: &Path, home_trash: Option<&Path>) -> bool {
    if home_trash == Some(dir) {
        return true;
    }
    let Some(name) = dir.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if name.starts_with(".Trash-") && dir.parent().is_some() {
        return true;
    }
    name.bytes().all(|byte| byte.is_ascii_digit())
        && !name.is_empty()
        && dir
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|parent| parent == ".Trash")
}

/// The top folder of the drive a bin is on, which a drive bin's relative
/// `Path=` starts from. `None` for the bin at home, whose paths are absolute.
pub(super) fn bin_topdir(bin: &Path) -> Option<PathBuf> {
    let name = bin.file_name()?.to_str()?;
    if name.starts_with(".Trash-") {
        return bin.parent().map(Path::to_path_buf);
    }
    let shared = bin.parent()?;
    if shared.file_name()? == ".Trash" {
        return shared.parent().map(Path::to_path_buf);
    }
    None
}

/// Where a path is in a bin's `files` folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InBin {
    /// The bin: the folder that holds `files` and `info`.
    pub bin: PathBuf,
    /// The path below `files`: empty for the `files` folder itself, one
    /// component for an item of the bin, more for something inside a folder
    /// that was binned.
    pub below: PathBuf,
}

impl InBin {
    /// The item of the bin this path is or is inside, as it is named in
    /// `files`.
    pub fn item_name(&self) -> Option<&OsStr> {
        self.below.components().next().map(|part| part.as_os_str())
    }

    /// The path is itself an item of the bin, not something inside one.
    pub fn is_item(&self) -> bool {
        self.below.components().count() == 1
    }

    /// The path is the `files` folder.
    pub fn is_root(&self) -> bool {
        self.below.as_os_str().is_empty()
    }

    /// The item's folder in `files`.
    pub fn item_path(&self) -> Option<PathBuf> {
        self.item_name()
            .map(|name| self.bin.join("files").join(name))
    }
}

/// Where `path` is in a bin's `files` folder, by its text alone.
pub(super) fn locate(path: &Path, home_trash: Option<&Path>) -> Option<InBin> {
    for ancestor in path.ancestors() {
        if ancestor.file_name() != Some(OsStr::new("files")) {
            continue;
        }
        let Some(bin) = ancestor.parent() else {
            continue;
        };
        if is_bin(bin, home_trash) {
            let below = path.strip_prefix(ancestor).ok()?.to_path_buf();
            return Some(InBin {
                bin: bin.to_path_buf(),
                below,
            });
        }
    }
    None
}

/// The `.trashinfo` file of the item `name` of `bin`.
pub(super) fn info_path(bin: &Path, name: &OsStr) -> PathBuf {
    let mut file = name.to_os_string();
    file.push(".trashinfo");
    bin.join("info").join(file)
}

/// One item of a bin, as the Trash lists it: where it is, and what its
/// `.trashinfo` says about where it came from and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashItem {
    /// The bin that holds it.
    pub bin: PathBuf,
    /// The item in the bin's `files` folder.
    pub in_trash: PathBuf,
    /// Its `.trashinfo` file (which may not exist).
    pub info: PathBuf,
    /// Where it was, absolute. `None` when the info file is missing or has no
    /// usable path: nobody knows where it came from.
    pub original: Option<PathBuf>,
    /// When it was deleted, local time. `None` when the info does not say.
    pub deleted: Option<SystemTime>,
}

impl TrashItem {
    /// The name the item had before it was deleted, else the name it has in
    /// the bin (the two differ when a clash numbered it).
    pub fn original_name(&self) -> OsString {
        self.original
            .as_deref()
            .and_then(Path::file_name)
            .or_else(|| self.in_trash.file_name())
            .map(OsStr::to_os_string)
            .unwrap_or_default()
    }

    /// The name of the item in `files`.
    pub fn name_in_trash(&self) -> &OsStr {
        self.in_trash.file_name().unwrap_or_default()
    }
}

/// Read the info of the item `in_trash`, a direct child of a bin's `files`.
/// `None` when the path is not one (the `files` folder itself, something
/// inside a binned folder, or outside every bin).
pub(super) fn item_in(in_trash: &Path, home_trash: Option<&Path>) -> Option<TrashItem> {
    let place = locate(in_trash, home_trash)?;
    if !place.is_item() {
        return None;
    }
    let info = info_path(&place.bin, place.item_name()?);
    let (original, deleted) = match fs::read(&info).ok().and_then(|text| parse_info(&text)) {
        Some(parsed) => (
            absolute_original(parsed.path, bin_topdir(&place.bin).as_deref()),
            parsed.deleted,
        ),
        None => (None, None),
    };
    Some(TrashItem {
        bin: place.bin,
        in_trash: in_trash.to_path_buf(),
        info,
        original,
        deleted,
    })
}

/// Where `path` is in a bin's `files` folder (the home bin's or a drive's),
/// by its text alone: nothing is read. `None` for a path that is not in one.
pub fn trash_place(path: &Path) -> Option<InBin> {
    locate(path, super::trash::home_trash().as_deref())
}

/// The `files` folder of the home bin, `$XDG_DATA_HOME/Trash/files`: the
/// folder the Trash place lists. The folder may not exist until something is
/// binned. `None` when there is no data home.
pub fn trash_folder() -> Option<PathBuf> {
    super::trash::home_trash().map(|bin| bin.join("files"))
}

/// The item of a bin that `path` is, with what its `.trashinfo` says. Reads
/// one small file. `None` when `path` is not a direct child of a bin's
/// `files` folder.
pub fn trash_item(path: &Path) -> Option<TrashItem> {
    item_in(path, super::trash::home_trash().as_deref())
}

/// [`trash_item`] for each of `paths` that is an item of a bin, in order.
pub fn trash_items(paths: &[PathBuf]) -> Vec<TrashItem> {
    let home = super::trash::home_trash();
    paths
        .iter()
        .filter_map(|path| item_in(path, home.as_deref()))
        .collect()
}

// ---- directorysizes -----------------------------------------------------------

/// One line of `directorysizes`: the disk use of a folder in `files`, valid
/// while the `.trashinfo` of the folder still has the time recorded here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SizeLine {
    pub size: u64,
    pub info_mtime: i64,
    pub name: OsString,
}

fn sizes_path(bin: &Path) -> PathBuf {
    bin.join("directorysizes")
}

pub(super) fn parse_sizes(text: &[u8]) -> Vec<SizeLine> {
    text.split(|byte| *byte == b'\n')
        .filter_map(|line| {
            let line = std::str::from_utf8(line).ok()?;
            let mut parts = line.splitn(3, ' ');
            let size = parts.next()?.parse().ok()?;
            let info_mtime = parts.next()?.parse().ok()?;
            let name = OsString::from_vec(percent_decode(parts.next()?));
            Some(SizeLine {
                size,
                info_mtime,
                name,
            })
        })
        .collect()
}

pub(super) fn format_sizes(lines: &[SizeLine]) -> String {
    lines
        .iter()
        .map(|line| {
            format!(
                "{} {} {}\n",
                line.size,
                line.info_mtime,
                encode_name(&line.name)
            )
        })
        .collect()
}

fn read_sizes(bin: &Path) -> Vec<SizeLine> {
    fs::read(sizes_path(bin))
        .map(|text| parse_sizes(&text))
        .unwrap_or_default()
}

/// Write `lines` as the bin's `directorysizes`, all at once: to a file of
/// its own name first and renamed over the cache, so a reader never sees half
/// of it. Nothing is written when there is nothing to keep and no cache yet.
fn write_sizes(bin: &Path, lines: &[SizeLine]) -> std::io::Result<()> {
    let target = sizes_path(bin);
    if lines.is_empty() {
        return match fs::remove_file(&target) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let staging = bin.join(format!("directorysizes.{}.tmp", std::process::id()));
    let mut file = fs::File::create(&staging)?;
    file.write_all(format_sizes(lines).as_bytes())?;
    drop(file);
    // On removable media and shares the new cache is written out before it
    // takes the name.
    if let Err(error) = sync_written(&staging) {
        let _ = fs::remove_file(&staging);
        return Err(std::io::Error::other(error.to_string()));
    }
    fs::rename(&staging, &target)
}

/// Add or replace the line of the folder `name`.
pub(super) fn record_size(
    bin: &Path,
    name: &OsStr,
    size: u64,
    info_mtime: i64,
) -> std::io::Result<()> {
    let mut lines = read_sizes(bin);
    lines.retain(|line| line.name != name);
    lines.push(SizeLine {
        size,
        info_mtime,
        name: name.to_os_string(),
    });
    write_sizes(bin, &lines)
}

/// Remove the lines of the folders `names`, and any line whose folder is no
/// longer in `files`. Does nothing when there is no cache.
pub(super) fn forget_sizes(bin: &Path, names: &[&OsStr]) {
    let before = read_sizes(bin);
    if before.is_empty() {
        return;
    }
    let after: Vec<SizeLine> = before
        .iter()
        .filter(|line| {
            !names.contains(&line.name.as_os_str())
                && bin
                    .join("files")
                    .join(&line.name)
                    .symlink_metadata()
                    .is_ok()
        })
        .cloned()
        .collect();
    if after != before {
        let _ = write_sizes(bin, &after);
    }
}

/// The disk use of the binned folder `name`, when the cache has it and it is
/// still the folder that was measured.
pub fn cached_dir_size(bin: &Path, name: &OsStr) -> Option<u64> {
    let line = read_sizes(bin).into_iter().find(|line| line.name == name)?;
    let mtime = fs::metadata(info_path(bin, name)).ok()?.mtime();
    (mtime == line.info_mtime).then_some(line.size)
}

/// The disk use of everything under `path`, in bytes: the blocks the files
/// and folders occupy, as `du -B1` counts them. Links count as themselves and
/// are not followed. `None` when `cancel` stopped the walk or when the tree
/// holds more than `limit` entries (a cache line is then better left out than
/// paid for with a long walk).
pub(super) fn disk_use(path: &Path, cancel: &Cancel, limit: Option<u64>) -> Option<u64> {
    let mut total = 0u64;
    let mut seen = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(next) = pending.pop() {
        if cancel.is_cancelled() {
            return None;
        }
        seen += 1;
        if limit.is_some_and(|limit| seen > limit) {
            return None;
        }
        let Ok(meta) = next.symlink_metadata() else {
            continue;
        };
        total += meta.blocks() * 512;
        if meta.is_dir()
            && let Ok(children) = fs::read_dir(&next)
        {
            pending.extend(children.flatten().map(|child| child.path()));
        }
    }
    Some(total)
}

/// The `mtime` the cache keys a folder's line on: its `.trashinfo`'s.
pub(super) fn info_mtime(bin: &Path, name: &OsStr) -> Option<i64> {
    fs::metadata(info_path(bin, name))
        .ok()
        .map(|meta| meta.mtime())
}

/// The extension of a file name as bytes, dot included, by the rule
/// [`super::names::split_extension`] has for text: `.tar.gz` is one, a leading
/// dot is not an extension, a long tail with spaces is part of the name. It
/// works on bytes so a name that is not UTF-8 keeps every one of them.
pub(super) fn split_extension_bytes(name: &[u8]) -> (&[u8], &[u8]) {
    let Some(dot) = name.iter().rposition(|byte| *byte == b'.') else {
        return (name, b"");
    };
    let tail = &name[dot + 1..];
    if dot == 0 || tail.is_empty() || tail.len() > 8 || tail.iter().any(u8::is_ascii_whitespace) {
        return (name, b"");
    }
    let stem = &name[..dot];
    if let Some(inner) = stem.strip_suffix(b".tar")
        && !inner.is_empty()
    {
        return (inner, &name[inner.len()..]);
    }
    (stem, &name[dot..])
}

/// The name an item gets in a bin on its `attempt`th try: `name`, then
/// `stem.2.ext`, `stem.3.ext`, and so on, made of the original bytes.
pub(super) fn unique_name(name: &OsStr, attempt: u32) -> OsString {
    let (stem, extension) = split_extension_bytes(name.as_bytes());
    let tag = if attempt <= 1 {
        String::new()
    } else {
        format!(".{attempt}")
    };
    // The info file adds `.trashinfo` to the name, and a name is 255 bytes at
    // most, so a very long name is cut (the real one is in the info file).
    let room = NAME_MAX - ".trashinfo".len();
    let keep = room
        .saturating_sub(tag.len() + extension.len())
        .min(stem.len());
    let mut out = stem[..keep].to_vec();
    out.extend_from_slice(tag.as_bytes());
    out.extend_from_slice(extension);
    OsString::from_vec(out)
}

/// The longest a name in a folder may be, in bytes.
const NAME_MAX: usize = 255;

/// Reads the `.trashinfo` files of the items of one bin, for a listing of its
/// `files` folder.
#[derive(Clone, Debug)]
pub(crate) struct BinFolder {
    bin: PathBuf,
    top: Option<PathBuf>,
}

impl BinFolder {
    /// The reader for `folder` when it is exactly the `files` folder of a bin,
    /// else `None`.
    pub(crate) fn of(folder: &Path) -> Option<BinFolder> {
        let home = super::trash::home_trash();
        let place = locate(folder, home.as_deref())?;
        place.is_root().then(|| BinFolder {
            top: bin_topdir(&place.bin),
            bin: place.bin,
        })
    }

    /// What the info file of the item `name` says.
    pub(crate) fn meta(&self, name: &OsStr) -> crate::entry::TrashMeta {
        let parsed = fs::read(info_path(&self.bin, name))
            .ok()
            .and_then(|text| parse_info(&text));
        match parsed {
            Some(info) => crate::entry::TrashMeta {
                original: absolute_original(info.path, self.top.as_deref()),
                deleted: info.deleted,
            },
            None => crate::entry::TrashMeta {
                original: None,
                deleted: None,
            },
        }
    }
}

/// `Path=` as an absolute path: a drive's bin records it from the drive's top
/// folder.
fn absolute_original(path: PathBuf, top: Option<&Path>) -> Option<PathBuf> {
    if path.is_absolute() {
        Some(path)
    } else {
        top.map(|top| top.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/me/.local/share/Trash";

    #[test]
    fn a_date_round_trips_through_the_local_zone() {
        let when = UNIX_EPOCH + Duration::from_secs(1_700_000_123);
        let text = format_date(when);
        assert_eq!(text.len(), 19);
        assert_eq!(parse_date(&text), Some(when));
        assert_eq!(
            parse_date("2024-02-30T25:61:00").is_some(),
            true,
            "mktime normalises"
        );
        assert_eq!(parse_date("2024-13-01T00:00:00"), None);
        assert_eq!(parse_date("yesterday"), None);
        assert_eq!(parse_date("2024-05-01"), None);
    }

    #[test]
    fn an_info_file_gives_its_path_and_date() {
        let text = b"[Trash Info]\nPath=/home/me/a%20b/c.txt\nDeletionDate=2024-05-01T13:45:09\n";
        let info = parse_info(text).unwrap();
        assert_eq!(info.path, PathBuf::from("/home/me/a b/c.txt"));
        assert_eq!(info.deleted, parse_date("2024-05-01T13:45:09"));
    }

    #[test]
    fn bytes_that_are_not_utf8_come_back_exactly() {
        let name = OsString::from_vec(b"/home/me/caf\xe9 \xff.txt".to_vec());
        let text = info_text(Path::new(&name), UNIX_EPOCH);
        assert!(text.is_ascii(), "{text}");
        let info = parse_info(text.as_bytes()).unwrap();
        assert_eq!(info.path.as_os_str().as_bytes(), name.as_bytes());
    }

    #[test]
    fn a_broken_info_file_says_nothing_instead_of_guessing() {
        assert_eq!(parse_info(b""), None);
        assert_eq!(parse_info(b"Path=/a\n"), None, "no group");
        assert_eq!(
            parse_info(b"[Trash Info]\nDeletionDate=2024-05-01T13:45:09\n"),
            None
        );
        assert_eq!(parse_info(b"[Other]\nPath=/a\n"), None);
        // A date that cannot be read leaves the path.
        let info = parse_info(b"[Trash Info]\nPath=/a\nDeletionDate=soon\n").unwrap();
        assert_eq!(info.deleted, None);
        // The first of two paths wins; CRLF line ends are fine.
        let info = parse_info(b"[Trash Info]\r\nPath=/first\r\nPath=/second\r\n").unwrap();
        assert_eq!(info.path, PathBuf::from("/first"));
    }

    #[test]
    fn a_bin_is_recognised_by_its_place() {
        let home = Some(Path::new(HOME));
        assert!(is_bin(Path::new(HOME), home));
        assert!(is_bin(Path::new("/media/usb/.Trash-1000"), home));
        assert!(is_bin(Path::new("/media/usb/.Trash/1000"), home));
        assert!(
            !is_bin(Path::new("/media/usb/.Trash"), home),
            "the shared folder is not a bin"
        );
        assert!(!is_bin(Path::new("/media/usb/Trash"), home));
        assert!(!is_bin(Path::new("/home/me/Trash"), home));
        assert_eq!(
            bin_topdir(Path::new("/media/usb/.Trash-1000")),
            Some("/media/usb".into())
        );
        assert_eq!(
            bin_topdir(Path::new("/media/usb/.Trash/1000")),
            Some("/media/usb".into())
        );
        assert_eq!(bin_topdir(Path::new(HOME)), None);
    }

    #[test]
    fn a_path_is_placed_in_its_bin() {
        let home = Some(Path::new(HOME));
        let place = locate(Path::new("/home/me/.local/share/Trash/files/a.txt"), home).unwrap();
        assert_eq!(place.bin, Path::new(HOME));
        assert!(place.is_item());
        assert_eq!(place.item_name(), Some(OsStr::new("a.txt")));
        let inside = locate(
            Path::new("/home/me/.local/share/Trash/files/dir/sub/b"),
            home,
        )
        .unwrap();
        assert!(!inside.is_item());
        assert_eq!(inside.item_name(), Some(OsStr::new("dir")));
        assert_eq!(
            inside.item_path().unwrap(),
            Path::new("/home/me/.local/share/Trash/files/dir")
        );
        let root = locate(Path::new("/home/me/.local/share/Trash/files"), home).unwrap();
        assert!(root.is_root());
        let usb = locate(Path::new("/media/usb/.Trash-1000/files/x"), home).unwrap();
        assert_eq!(usb.bin, Path::new("/media/usb/.Trash-1000"));
        // The info folder and ordinary folders called files are not a bin's files.
        assert_eq!(
            locate(
                Path::new("/home/me/.local/share/Trash/info/a.trashinfo"),
                home
            ),
            None
        );
        assert_eq!(locate(Path::new("/home/me/files/a"), home), None);
    }

    #[test]
    fn the_sizes_cache_round_trips_and_keeps_odd_names() {
        let lines = vec![
            SizeLine {
                size: 4096,
                info_mtime: 1_700_000_000,
                name: "a folder".into(),
            },
            SizeLine {
                size: 0,
                info_mtime: 5,
                name: OsString::from_vec(b"caf\xe9".to_vec()),
            },
        ];
        let text = format_sizes(&lines);
        assert_eq!(text, "4096 1700000000 a%20folder\n0 5 caf%E9\n");
        assert_eq!(parse_sizes(text.as_bytes()), lines);
        assert_eq!(parse_sizes(b"junk\n12 x y\n"), []);
    }
}
