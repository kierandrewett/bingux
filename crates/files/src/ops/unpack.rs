//! Opening archives: [`inspect`] says what is inside and whether it is
//! locked, [`check_password`] tries a password, [`unpack`] extracts.
//!
//! # What it promises
//!
//! - An archive is listed first and refused whole if any entry is absolute or
//!   climbs out with `..`, or if a link inside points outside.
//! - The programs work in a hidden folder inside the destination; what they
//!   made is moved into place only when they finished. A failure or a cancel
//!   removes the hidden folder: nothing half-extracted is left.
//! - Nothing is overwritten unless the person chose [`Overwrite::Replace`],
//!   and then the old item goes to the Trash first, never away for good.
//! - A password reaches 7-Zip on its standard input, never on a command line
//!   (where `ps` shows it to everyone), and is never logged.
//!
//! | Archive | Opened by |
//! | ------- | --------- |
//! | `.zip` | `unzip`; `7z` when it is locked |
//! | `.tar`, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.tar.bz2` and short forms | `tar` |
//! | `.7z` | `7z` |

use std::fs;
use std::path::{Path, PathBuf};

use crate::Cancel;

use super::archive::{
    Kind, classify, entry_is_safe, folder_name, link_stays_inside, make_scratch, tar_links_are_safe,
};
use super::durable::{sync_dirs, sync_written};
use super::names::keep_both_with;
use super::pack_plan::{Format, Heard, Password, hear};
use super::tool::{self, Input, Spec, Stream};
use super::trash::{Trashed, trash};
use super::{OpError, Progress, rename_noreplace};

/// What to do when something with the same name is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overwrite {
    /// Use the next free name: `photos (copy)`.
    KeepBoth,
    /// The old item goes to the Trash.
    Replace,
    /// Leave the new item out.
    Skip,
}

/// How to unpack.
#[derive(Clone, Debug)]
pub struct UnpackOptions {
    pub dest_dir: PathBuf,
    /// Put the contents in a folder named after the archive (or the one
    /// folder it holds). Off, they land loose in `dest_dir`.
    pub create_folder: bool,
    pub overwrite: Overwrite,
    pub password: Option<Password>,
}

/// What is inside an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspection {
    pub entries: u64,
    /// The bytes the contents take, when the archive says (not for a tar).
    pub bytes: u64,
    /// Some of it is locked: a password is needed to open it.
    pub locked: bool,
    /// Even the names are locked: nothing was listed.
    pub names_hidden: bool,
    /// The one folder everything is in, when there is exactly one.
    pub single_folder: Option<String>,
    /// The archive's name without its ending, for the folder it makes.
    pub stem: String,
}

fn missing(tool: &str) -> OpError {
    OpError::Unsupported(format!("{tool} is not installed."))
}

fn kind_of(archive: &Path) -> Result<(Kind, String, usize), OpError> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OpError::Other("That is not an archive.".into()))?;
    let (kind, ending) = classify(name)
        .ok_or_else(|| OpError::Unsupported("This kind of archive cannot be opened here.".into()))?;
    Ok((kind, name.to_string(), ending))
}

fn seven() -> Result<PathBuf, OpError> {
    tool::seven_zip().ok_or_else(|| missing("7-Zip"))
}

/// A 7-Zip command with the password, if any, on standard input. No `-p`:
/// with it, 7-Zip takes an empty password instead of asking.
fn seven_spec(args: &[&str], archive: &Path, password: Option<&Password>) -> Result<Spec, OpError> {
    let mut spec = Spec::new(seven()?)
        .nice(true)
        .args(args)
        .args(["-scsUTF-8", "-bd"]);
    spec = spec.arg("--").arg(archive);
    spec.input = match password {
        Some(password) => password.input_once(),
        None => Input::Null,
    };
    Ok(spec)
}

/// One entry of `7z l -slt`.
#[derive(Debug, Default, Clone)]
struct SevenEntry {
    path: String,
    folder: bool,
    size: u64,
    encrypted: bool,
    /// A symbolic link: 7-Zip lists its attributes, not where it points.
    is_link: bool,
}

/// Read `7z l -slt -ba`: blocks of `Key = value` lines. A block starts at
/// its `Path`; the capture drops the blank lines between blocks.
fn parse_seven_listing(text: &str) -> Vec<SevenEntry> {
    let mut entries: Vec<SevenEntry> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        if key == "Path" {
            entries.push(SevenEntry {
                path: value.to_string(),
                ..SevenEntry::default()
            });
            continue;
        }
        let Some(current) = entries.last_mut() else {
            continue;
        };
        match key {
            "Folder" => current.folder = value == "+",
            "Size" => current.size = value.parse().unwrap_or(0),
            "Encrypted" => current.encrypted = value == "+",
            "Attributes" => {
                // A .7z has no `Folder` line: its attributes start with D.
                current.folder |= value.trim_start().starts_with('D');
                current.is_link = value
                    .split_whitespace()
                    .any(|part| part.len() == 10 && part.starts_with('l'));
            }
            _ => {}
        }
    }
    entries
}

fn looks_locked(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("wrong password")
        || lower.contains("encrypted archive")
        || lower.contains("incorrect password")
        || lower.contains("enter password")
}

/// The one folder every entry is under, if there is exactly one.
fn single_top(names: &[String]) -> Option<String> {
    let mut top: Option<&str> = None;
    for name in names {
        let first = name.trim_start_matches("./").split('/').next().unwrap_or("");
        if first.is_empty() {
            continue;
        }
        // A file at the top is no folder.
        let is_inside = name.trim_start_matches("./").contains('/');
        if !is_inside && !name.ends_with('/') {
            return None;
        }
        match top {
            None => top = Some(first),
            Some(seen) if seen == first => {}
            Some(_) => return None,
        }
    }
    top.map(str::to_string)
}

fn refuse_unsafe() -> OpError {
    OpError::Other("The archive has files that would be put outside its folder, so it was not extracted.".into())
}

/// Look inside `archive` without extracting it. With a `password`, a locked
/// archive is listed too.
pub fn inspect(archive: &Path, password: Option<&Password>, cancel: &Cancel) -> Result<Inspection, OpError> {
    let (kind, name, ending) = kind_of(archive)?;
    let stem = folder_name(&name, ending);
    fs::metadata(archive)?;
    match kind {
        Kind::Tar => {
            let tar = tool::find("tar").ok_or_else(|| missing("tar"))?;
            let verbose = Spec::new(&tar).nice(true).args(["-t", "-v", "--quoting-style=literal", "-f"]).arg(archive);
            let (finished, out) = tool::capture(&verbose, cancel, 64 * 1024 * 1024)?;
            if !finished.success() {
                return Err(finished.sentence("The archive could not be read"));
            }
            let listing = String::from_utf8_lossy(&out);
            if !tar_links_are_safe(&listing) {
                return Err(refuse_unsafe());
            }
            let plain = Spec::new(&tar).nice(true).args(["-t", "--quoting-style=literal", "-f"]).arg(archive);
            let (_, names) = tool::capture(&plain, cancel, 64 * 1024 * 1024)?;
            let names: Vec<String> = String::from_utf8_lossy(&names).lines().map(str::to_string).collect();
            if !names.iter().all(|name| entry_is_safe(name)) {
                return Err(refuse_unsafe());
            }
            Ok(Inspection {
                entries: names.len() as u64,
                bytes: 0,
                locked: false,
                names_hidden: false,
                single_folder: single_top(&names),
                stem,
            })
        }
        Kind::Zip | Kind::Seven if kind == Kind::Seven || tool::seven_zip().is_some() => {
            let spec = seven_spec(&["l", "-slt", "-ba"], archive, password)?;
            let (finished, out) = tool::capture(&spec, cancel, 64 * 1024 * 1024)?;
            let text = String::from_utf8_lossy(&out).into_owned();
            if !finished.success() {
                if looks_locked(&text) || looks_locked(&finished.complaint) {
                    return Ok(Inspection {
                        entries: 0,
                        bytes: 0,
                        locked: true,
                        names_hidden: true,
                        single_folder: None,
                        stem,
                    });
                }
                return Err(finished.sentence("The archive could not be read"));
            }
            let entries = parse_seven_listing(&text);
            let names: Vec<String> = entries
                .iter()
                .map(|entry| if entry.folder { format!("{}/", entry.path) } else { entry.path.clone() })
                .collect();
            if !names.iter().all(|name| entry_is_safe(name)) {
                return Err(refuse_unsafe());
            }
            // 7-Zip does not say where a link points, so the links are checked
            // after extraction (see `settle`); here, nothing may be written
            // through one.
            let links: Vec<&str> = entries.iter().filter(|e| e.is_link).map(|e| e.path.as_str()).collect();
            if entries
                .iter()
                .any(|entry| links.iter().any(|link| entry.path.starts_with(&format!("{link}/"))))
            {
                return Err(refuse_unsafe());
            }
            Ok(Inspection {
                entries: entries.len() as u64,
                bytes: entries.iter().map(|entry| entry.size).sum(),
                locked: entries.iter().any(|entry| entry.encrypted),
                names_hidden: false,
                single_folder: single_top(&names),
                stem,
            })
        }
        Kind::Zip | Kind::Seven => {
            let unzip = tool::find("unzip").ok_or_else(|| missing("unzip"))?;
            let list = Spec::new(&unzip).nice(true).args(["-Z1", "--"]).arg(archive);
            let (finished, out) = tool::capture(&list, cancel, 64 * 1024 * 1024)?;
            if !finished.success() {
                return Err(finished.sentence("The archive could not be read"));
            }
            let names: Vec<String> = String::from_utf8_lossy(&out).lines().map(str::to_string).collect();
            if !names.iter().all(|name| entry_is_safe(name)) {
                return Err(refuse_unsafe());
            }
            let detail = Spec::new(&unzip).nice(true).args(["-Zv", "--"]).arg(archive);
            let (_, text) = tool::capture(&detail, cancel, 64 * 1024 * 1024)?;
            let locked = String::from_utf8_lossy(&text).contains("file security status:                   encrypted");
            Ok(Inspection {
                entries: names.len() as u64,
                bytes: 0,
                locked,
                names_hidden: false,
                single_folder: single_top(&names),
                stem,
            })
        }
    }
}

/// Is `password` right for `archive`? Reads every byte of the locked
/// entries, so it takes as long as the archive is big. `Ok(false)` is a wrong
/// password; an error is anything else.
pub fn check_password(archive: &Path, password: &Password, cancel: &Cancel) -> Result<bool, OpError> {
    let (kind, ..) = kind_of(archive)?;
    if kind == Kind::Tar {
        return Ok(true);
    }
    let spec = seven_spec(&["t"], archive, Some(password))?;
    let finished = tool::run(&spec, cancel, &mut |_, _| {})?;
    if finished.success() {
        return Ok(true);
    }
    if looks_locked(&finished.complaint) || finished.status.code() == Some(2) {
        return Ok(false);
    }
    Err(finished.sentence("The password could not be tried"))
}

/// What [`unpack`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unpacked {
    /// The items now in the destination: the new folder, or each item that
    /// landed loose.
    pub items: Vec<PathBuf>,
    /// Items the new ones replaced, now in the Trash.
    pub replaced: Vec<Trashed>,
    /// Items left out because their names were taken.
    pub skipped: usize,
    pub entries: u64,
}

enum Placed {
    Put(PathBuf),
    Skipped,
}

fn taken(dir: &Path, name: &str) -> bool {
    dir.join(name).symlink_metadata().is_ok()
}

/// Move `from` to `dir/name` as `overwrite` says.
fn place(
    from: &Path,
    dir: &Path,
    name: &str,
    overwrite: Overwrite,
    replaced: &mut Vec<Trashed>,
) -> Result<Placed, OpError> {
    for _ in 0..1000 {
        if taken(dir, name) {
            match overwrite {
                Overwrite::Skip => return Ok(Placed::Skipped),
                Overwrite::Replace => {
                    replaced.extend(trash(&[dir.join(name)])?);
                }
                Overwrite::KeepBoth => {}
            }
        }
        let target = if taken(dir, name) {
            keep_both_with(name, "", |candidate| taken(dir, candidate))
        } else {
            name.to_string()
        };
        let to = dir.join(&target);
        match rename_noreplace(from, &to) {
            Ok(()) => return Ok(Placed::Put(to)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(OpError::Exists)
}

/// Extract `archive`. Blocks; call it from a worker. [`Progress`] counts the
/// entries written (and, for 7-Zip, the share of bytes).
pub fn unpack(
    archive: &Path,
    options: &UnpackOptions,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<Unpacked, OpError> {
    let (kind, _, _) = kind_of(archive)?;
    if !fs::metadata(&options.dest_dir)?.is_dir() {
        return Err(OpError::Other("The destination is not a folder.".into()));
    }
    progress.set_scanning(true);
    let info = inspect(archive, options.password.as_ref(), cancel);
    progress.set_scanning(false);
    let info = info?;
    if info.locked && options.password.is_none() {
        return Err(OpError::Other("This archive needs a password.".into()));
    }
    if info.names_hidden {
        return Err(OpError::Other("The password is not right.".into()));
    }
    progress.add_total(info.bytes, info.entries);

    let scratch = make_scratch(&options.dest_dir)?;
    let result = (|| {
        run_extractor(kind, archive, &scratch, &info, options, progress, cancel)?;
        settle(&scratch, &info, options)
    })();
    let _ = fs::remove_dir_all(&scratch);
    let unpacked = result?;
    if let Some(item) = unpacked.items.first() {
        sync_written(item)?;
    }
    sync_dirs(&[&options.dest_dir])?;
    progress.settle();
    Ok(unpacked)
}

fn run_extractor(
    kind: Kind,
    archive: &Path,
    scratch: &Path,
    info: &Inspection,
    options: &UnpackOptions,
    progress: &Progress,
    cancel: &Cancel,
) -> Result<(), OpError> {
    let mut reported = 0u64;
    let total = info.bytes;
    let seven_run = kind == Kind::Seven || (kind == Kind::Zip && (options.password.is_some() || info.locked));
    let spec = if seven_run {
        let mut spec = seven_spec(&["x", "-y", "-bsp1", "-bb1"], archive, options.password.as_ref())?;
        // `-o` takes its folder joined to it, before the `--`.
        let mut out = std::ffi::OsString::from("-o");
        out.push(scratch.as_os_str());
        let at = spec.args.iter().position(|arg| arg == "--").unwrap_or(spec.args.len());
        spec.args.insert(at, out);
        spec
    } else if kind == Kind::Zip {
        let unzip = tool::find("unzip").ok_or_else(|| missing("unzip"))?;
        Spec::new(unzip).nice(true).arg("-n").arg("--").arg(archive).arg("-d").arg(scratch)
    } else {
        let tar = tool::find("tar").ok_or_else(|| missing("tar"))?;
        Spec::new(tar)
            .nice(true)
            .args(["-x", "-v", "--no-same-owner", "--quoting-style=literal", "-f"])
            .arg(archive)
            .arg("-C")
            .arg(scratch)
    };
    let finished = tool::run(&spec, cancel, &mut |stream, line| {
        if stream != Stream::Out {
            return;
        }
        if seven_run {
            match hear(true, Format::SevenZip, line) {
                Some(Heard::Percent(percent)) => {
                    let target = total / 100 * u64::from(percent);
                    if target > reported {
                        progress.add_bytes(target - reported);
                        reported = target;
                    }
                }
                Some(Heard::Entry(name)) => {
                    progress.add_files(1);
                    progress.set_current(&String::from_utf8_lossy(&name));
                }
                None => {}
            }
        } else {
            let text = String::from_utf8_lossy(line);
            let name = ["  inflating: ", " extracting: ", "   creating: ", "  linking: "]
                .iter()
                .find_map(|prefix| text.strip_prefix(prefix))
                .map(str::trim);
            match (kind, name) {
                (Kind::Zip, Some(name)) => {
                    progress.add_files(1);
                    progress.set_current(name);
                }
                (Kind::Tar, _) => {
                    progress.add_files(1);
                    progress.set_current(&text);
                }
                _ => {}
            }
        }
    })?;
    // unzip exits 1 for warnings that left everything extracted; so does 7-Zip.
    let ok = finished.success() || (finished.status.code() == Some(1) && kind != Kind::Tar);
    if ok {
        return Ok(());
    }
    if looks_locked(&finished.complaint) {
        return Err(OpError::Other("The password is not right.".into()));
    }
    Err(finished.sentence("The archive could not be extracted"))
}

/// Does every link below `dir` (relative to the extraction root as `rel`)
/// point inside the root?
fn links_stay_inside(dir: &Path, rel: &Path) -> Result<bool, OpError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = rel.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let target = fs::read_link(entry.path())?;
            let inside = target
                .to_str()
                .is_some_and(|target| link_stays_inside(&name.to_string_lossy(), target));
            if !inside {
                return Ok(false);
            }
        } else if kind.is_dir() && !links_stay_inside(&entry.path(), &name)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Move what was extracted from `scratch` to its place.
fn settle(scratch: &Path, info: &Inspection, options: &UnpackOptions) -> Result<Unpacked, OpError> {
    if !links_stay_inside(scratch, Path::new(""))? {
        return Err(refuse_unsafe());
    }
    let mut replaced = Vec::new();
    let mut items = Vec::new();
    let mut skipped = 0;
    let mut top = fs::read_dir(scratch)?.collect::<Result<Vec<_>, _>>()?;
    top.sort_by_key(std::fs::DirEntry::file_name);
    if options.create_folder {
        let only_folder = match top.as_slice() {
            [only] if only.file_type()?.is_dir() => Some(only.path()),
            _ => None,
        };
        let (from, name) = match only_folder {
            Some(path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (path, name)
            }
            None => (scratch.to_path_buf(), info.stem.clone()),
        };
        match place(&from, &options.dest_dir, &name, options.overwrite, &mut replaced)? {
            Placed::Put(path) => items.push(path),
            Placed::Skipped => skipped += 1,
        }
    } else {
        for entry in top {
            let name = entry.file_name().to_string_lossy().into_owned();
            match place(&entry.path(), &options.dest_dir, &name, options.overwrite, &mut replaced)? {
                Placed::Put(path) => items.push(path),
                Placed::Skipped => skipped += 1,
            }
        }
    }
    Ok(Unpacked {
        items,
        replaced,
        skipped,
        entries: info.entries,
    })
}

/// Extract an archive into a new folder beside where the caller says, never
/// replacing anything: the plain "Extract". See [`unpack`] for the options.
pub fn extract(archive: &Path, dest_dir: &Path) -> Result<PathBuf, OpError> {
    let options = UnpackOptions {
        dest_dir: dest_dir.to_path_buf(),
        create_folder: true,
        overwrite: Overwrite::KeepBoth,
        password: None,
    };
    let done = unpack(archive, &options, &Progress::new(), &Cancel::new())?;
    done.items
        .into_iter()
        .next()
        .ok_or_else(|| OpError::Other("Nothing was extracted.".into()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::super::pack::{OnConflict, PackJob, pack};
    use super::super::pack_plan::PackOptions;
    use super::super::testing::{Temp, with_data_home};
    use super::*;

    fn make(temp: &Temp, format: Format, password: Option<&str>, hide: bool) -> Option<(PathBuf, PathBuf)> {
        if !format.is_available() {
            return None;
        }
        let root = temp.dir("src/proj");
        fs::write(root.join("a.txt"), "alpha").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/b.txt"), "beta").unwrap();
        fs::write(root.join(".hidden"), "h").unwrap();
        symlink("a.txt", root.join("link")).unwrap();
        let dest = temp.dir("out");
        let job = PackJob {
            roots: vec![root.clone()],
            dest_dir: dest.clone(),
            base: "bundle".into(),
            options: PackOptions {
                format,
                password: password.map(Password::new),
                hide_names: hide,
                ..PackOptions::default()
            },
            on_conflict: OnConflict::KeepBoth,
        };
        let packed = pack(&job, &Progress::new(), &Cancel::new()).ok()?;
        Some((root, packed.archive))
    }

    fn options(dir: &Path, create_folder: bool, overwrite: Overwrite, password: Option<&str>) -> UnpackOptions {
        UnpackOptions {
            dest_dir: dir.to_path_buf(),
            create_folder,
            overwrite,
            password: password.map(Password::new),
        }
    }

    fn go(archive: &Path, options: &UnpackOptions) -> Result<Unpacked, OpError> {
        unpack(archive, options, &Progress::new(), &Cancel::new())
    }

    #[test]
    fn listings_from_seven_zip_are_read() {
        let text = "Path = d\nFolder = +\nSize = 0\nPath = d/f\nFolder = -\nSize = 5\nEncrypted = +\nPath = d/l\nSize = 1\nAttributes =  lrwxrwxrwx\n";
        let entries = parse_seven_listing(text);
        assert_eq!(entries.len(), 3);
        assert!(entries[0].folder && entries[1].encrypted && entries[1].size == 5);
        assert!(entries[2].is_link && !entries[1].is_link);
        assert!(looks_locked("ERROR: Cannot open encrypted archive. Wrong password?"));
        assert!(!looks_locked("ERROR: no such file"));
    }

    #[test]
    fn one_folder_at_the_top_is_found() {
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(single_top(&names(&["proj/", "proj/a", "proj/b/c"])).as_deref(), Some("proj"));
        assert_eq!(single_top(&names(&["proj/a", "proj/b"])).as_deref(), Some("proj"));
        assert_eq!(single_top(&names(&["a", "proj/b"])), None);
        assert_eq!(single_top(&names(&["proj/a", "other/b"])), None);
        assert_eq!(single_top(&names(&["file"])), None);
    }

    #[test]
    fn every_format_unpacks_into_a_folder_and_back_to_the_same_files() {
        for format in [Format::Zip, Format::TarGz, Format::TarXz, Format::TarZst, Format::SevenZip, Format::Tar] {
            let temp = Temp::new("unpack-folder");
            let Some((_, archive)) = make(&temp, format, None, false) else { continue };
            let inspection = inspect(&archive, None, &Cancel::new()).unwrap();
            assert_eq!(inspection.single_folder.as_deref(), Some("proj"), "{format:?}");
            assert!(!inspection.locked && inspection.entries >= 5, "{format:?}: {inspection:?}");
            let dest = temp.dir("dest");
            let done = go(&archive, &options(&dest, true, Overwrite::KeepBoth, None)).unwrap_or_else(|e| panic!("{format:?}: {e}"));
            assert_eq!(done.items, [dest.join("proj")], "{format:?}");
            assert_eq!(fs::read_to_string(dest.join("proj/sub/b.txt")).unwrap(), "beta");
            assert_eq!(fs::read_link(dest.join("proj/link")).unwrap(), Path::new("a.txt"));
            assert!(dest.join("proj/.hidden").exists());
            let left: Vec<_> = fs::read_dir(&dest).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(left.len(), 1, "{format:?}: no scratch folder is left: {left:?}");
        }
    }

    #[test]
    fn without_a_folder_the_contents_land_loose_and_clashes_follow_the_policy() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("unpack-loose");
        let Some((_, archive)) = make(&temp, Format::Zip, None, false) else { return };
        let dest = temp.dir("dest");
        fs::create_dir(dest.join("proj")).unwrap();
        fs::write(dest.join("proj/mine.txt"), "mine").unwrap();
        // Skip leaves the old folder alone and counts it.
        let skipped = go(&archive, &options(&dest, false, Overwrite::Skip, None)).unwrap();
        assert_eq!((skipped.items.len(), skipped.skipped), (0, 1));
        assert!(dest.join("proj/mine.txt").exists() && !dest.join("proj/a.txt").exists());
        // Keep both makes `proj (copy)`.
        let both = go(&archive, &options(&dest, false, Overwrite::KeepBoth, None)).unwrap();
        assert_eq!(both.items, [dest.join("proj (copy)")]);
        assert!(dest.join("proj/mine.txt").exists());
        // Replace sends the old one to the Trash.
        let data = temp.dir("data");
        let replaced = with_data_home(&data, || go(&archive, &options(&dest, false, Overwrite::Replace, None))).unwrap();
        assert_eq!(replaced.items, [dest.join("proj")]);
        assert_eq!(replaced.replaced.len(), 1);
        assert!(replaced.replaced[0].in_trash.join("mine.txt").exists(), "the old folder is whole in the Trash");
        assert!(dest.join("proj/a.txt").exists() && !dest.join("proj/mine.txt").exists());
    }

    #[test]
    fn a_locked_archive_needs_the_right_password() {
        if tool::seven_zip().is_none() {
            return;
        }
        for (format, hide) in [(Format::Zip, false), (Format::SevenZip, false), (Format::SevenZip, true)] {
            let temp = Temp::new("unpack-locked");
            let Some((_, archive)) = make(&temp, format, Some("correct horse"), hide) else { continue };
            let inspection = inspect(&archive, None, &Cancel::new()).unwrap();
            assert!(inspection.locked, "{format:?} hide={hide}");
            assert_eq!(inspection.names_hidden, hide);
            let dest = temp.dir("dest");
            let none = go(&archive, &options(&dest, true, Overwrite::KeepBoth, None)).unwrap_err();
            assert!(none.to_string().contains("password"), "{none}");
            assert!(!check_password(&archive, &Password::new("wrong"), &Cancel::new()).unwrap(), "{format:?}");
            assert!(check_password(&archive, &Password::new("correct horse"), &Cancel::new()).unwrap(), "{format:?}");
            let bad = go(&archive, &options(&dest, true, Overwrite::KeepBoth, Some("wrong"))).unwrap_err();
            assert!(bad.to_string().contains("password"), "{format:?}: {bad}");
            assert_eq!(fs::read_dir(&dest).unwrap().count(), 0, "a wrong password leaves nothing");
            let done = go(&archive, &options(&dest, true, Overwrite::KeepBoth, Some("correct horse"))).unwrap();
            assert_eq!(fs::read_to_string(done.items[0].join("a.txt")).unwrap(), "alpha");
        }
    }

    #[test]
    fn an_archive_with_an_escaping_entry_is_refused_and_nothing_is_written() {
        if !tool::on_path("tar") {
            return;
        }
        let temp = Temp::new("unpack-slip");
        temp.write("inner/ok.txt", "ok");
        let sub = temp.dir("inner/deep");
        let archive = temp.path().join("slip.tar");
        let made = std::process::Command::new("tar").arg("-cPf").arg(&archive).arg("-C").arg(&sub).arg("../ok.txt").status().unwrap();
        assert!(made.success());
        let dest = temp.dir("dest");
        let error = go(&archive, &options(&dest, true, Overwrite::KeepBoth, None)).unwrap_err();
        assert!(error.to_string().contains("outside"), "{error}");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
        let work = temp.dir("work");
        symlink("/etc", work.join("etc-link")).unwrap();
        let links = temp.path().join("links.tar");
        assert!(std::process::Command::new("tar").arg("-cf").arg(&links).arg("-C").arg(&work).arg("etc-link").status().unwrap().success());
        assert!(go(&links, &options(&dest, true, Overwrite::KeepBoth, None)).is_err());
    }

    #[test]
    fn a_seven_zip_with_an_escaping_link_is_refused() {
        let Some(seven) = tool::seven_zip() else { return };
        let temp = Temp::new("unpack-seven-link");
        let work = temp.dir("work");
        symlink("/etc", work.join("etc-link")).unwrap();
        let archive = temp.path().join("evil.7z");
        let made = std::process::Command::new(seven)
            .current_dir(&work)
            .args(["a", "-snl", "-bd"])
            .arg(&archive)
            .arg("etc-link")
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stdout));
        let dest = temp.dir("dest");
        let error = go(&archive, &options(&dest, true, Overwrite::KeepBoth, None)).unwrap_err();
        assert!(error.to_string().contains("outside"), "{error}");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn cancel_leaves_nothing_behind() {
        if !Format::TarXz.is_available() {
            return;
        }
        let temp = Temp::new("unpack-cancel");
        let root = temp.dir("src/big");
        let mut seed = 5u64;
        let noise: Vec<u8> = (0..24_000_000)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 40) as u8
            })
            .collect();
        fs::write(root.join("noise.bin"), noise).unwrap();
        let dest = temp.dir("out");
        let job = PackJob {
            roots: vec![root],
            dest_dir: dest.clone(),
            base: "big".into(),
            options: PackOptions { format: Format::TarGz, ..PackOptions::default() },
            on_conflict: OnConflict::KeepBoth,
        };
        let archive = pack(&job, &Progress::new(), &Cancel::new()).unwrap().archive;
        let target = temp.dir("target");
        let cancel = Cancel::new();
        let stopper = cancel.clone();
        let watcher = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            stopper.cancel();
        });
        let outcome = unpack(&archive, &options(&target, true, Overwrite::KeepBoth, None), &Progress::new(), &cancel);
        watcher.join().unwrap();
        if outcome.is_ok() {
            // The extraction beat the cancel: nothing to check.
            return;
        }
        assert_eq!(outcome.unwrap_err(), OpError::Cancelled);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }

    #[test]
    fn extract_is_the_plain_unpack() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("unpack-extract");
        let Some((_, archive)) = make(&temp, Format::Zip, None, false) else { return };
        let dest = temp.dir("dest");
        assert_eq!(extract(&archive, &dest).unwrap(), dest.join("proj"));
        assert_eq!(extract(&archive, &dest).unwrap(), dest.join("proj (copy)"));
        assert!(extract(&temp.write("notes.txt", "x"), &dest).is_err());
    }
}
