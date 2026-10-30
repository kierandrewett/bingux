//! Making an archive: [`scan`] says what would go in, [`pack`] makes it.
//!
//! # What it promises
//!
//! - The programs work in a hidden folder beside the result (on the same
//!   drive), never in the person's folder proper. The archive gets its name
//!   by a rename, only after the programs finished, the archive was read back
//!   whole (the programs' own integrity check, and the number of entries it
//!   lists against the number that was meant to go in) and its bytes were
//!   written out (`fsync`, and `syncfs` on removable media and shares).
//!   Cancelling, or any failure, removes the hidden folder: no partial file
//!   is ever left behind.
//! - Nothing is overwritten unless asked. A name that is taken is numbered
//!   (`Archive 2.zip`) or, on request, the old file goes to the Trash first.
//! - Originals go to the Trash, never away for good, and only once the
//!   archive is in place.
//! - The programs get the names of what to archive from a NUL list, from
//!   standard input or from a list file, so a name that starts with a dash,
//!   holds a space, a bracket or a star, or is not UTF-8 is carried exactly,
//!   or refused with its name when a format cannot carry it. A password
//!   reaches the program on its standard input only.

use std::collections::{HashMap, HashSet};
use std::fs::{self, DirBuilder};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use crate::Cancel;

use super::durable::{sync_dirs, sync_written};
use super::pack_plan::{
    Built, Format, Heard, PackOptions, Steps, build, check_specs, file_name, free_base, hear,
    is_listing_row,
    is_hidden_name,
};
use super::tool::{self, Stream};
use super::trash::{Trashed, trash};
use super::{OpError, Progress, rename_noreplace};

/// What kind of thing an archive entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Folder,
    /// A link that is stored as a link.
    Link,
}

/// One thing that goes into an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its path inside the archive, from the folder the selection shares.
    pub rel: PathBuf,
    pub kind: ItemKind,
    pub size: u64,
}

/// What would go into an archive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub items: Vec<Item>,
    pub files: u64,
    pub folders: u64,
    /// The bytes of the files that go in.
    pub bytes: u64,
    /// Sockets, pipes and devices, which no archive here holds.
    pub skipped: u64,
}

/// The deepest folder that holds every path's parent.
fn common_parent(paths: &[PathBuf]) -> PathBuf {
    let mut parents = paths.iter().filter_map(|path| path.parent());
    let Some(first) = parents.next() else {
        return PathBuf::new();
    };
    let mut common: Vec<Component<'_>> = first.components().collect();
    for parent in parents {
        let keep = common
            .iter()
            .zip(parent.components())
            .take_while(|(a, b)| *a == b)
            .count();
        common.truncate(keep);
    }
    common.iter().collect()
}

/// The folder an archive of `roots` is relative to.
pub fn parent_of(roots: &[PathBuf]) -> PathBuf {
    common_parent(roots)
}

fn readable(path: &Path) -> bool {
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: the string is NUL-terminated and outlives the call.
    unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 }
}

/// Is the entry named `name` at `rel` left out by `options`?
fn left_out(options: &PackOptions, name: &str, rel: &str) -> bool {
    (!options.include_hidden && is_hidden_name(name)) || options.excludes.matches(rel)
}

/// Walk `roots` the way [`pack`] will and say what would go in: hidden files
/// and excluded patterns left out, links kept or followed as `options` say.
/// Blocks; folders are read one at a time and `cancel` is looked at between
/// them.
pub fn scan(roots: &[PathBuf], options: &PackOptions, cancel: &Cancel) -> Result<Scan, OpError> {
    if roots.is_empty() {
        return Err(OpError::Other("There is nothing to compress.".into()));
    }
    if roots.iter().any(|path| !path.is_absolute()) {
        return Err(OpError::Other("The paths must be absolute.".into()));
    }
    let parent = common_parent(roots);
    let mut scan = Scan::default();
    let mut seen_roots = HashSet::new();
    for root in roots {
        if !seen_roots.insert(root.clone()) {
            continue;
        }
        let rel = root
            .strip_prefix(&parent)
            .map_err(|_| OpError::Other("The paths have nothing in common.".into()))?;
        if rel.as_os_str().is_empty() {
            return Err(OpError::Other("The root folder cannot be compressed.".into()));
        }
        let mut ancestry = Vec::new();
        visit(root, rel, options, cancel, &mut scan, &mut ancestry)?;
    }
    Ok(scan)
}

fn visit(
    path: &Path,
    rel: &Path,
    options: &PackOptions,
    cancel: &Cancel,
    scan: &mut Scan,
    ancestry: &mut Vec<(u64, u64)>,
) -> Result<(), OpError> {
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let name = rel
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let rel_text = rel.to_string_lossy();
    if left_out(options, &name, &rel_text) {
        return Ok(());
    }
    let link = path.symlink_metadata()?;
    let mut meta = link.clone();
    let mut is_link = link.file_type().is_symlink();
    if is_link && options.follow_links {
        match path.metadata() {
            Ok(target) => {
                meta = target;
                is_link = false;
            }
            // A link to nothing is kept as a link even when following.
            Err(_) => {}
        }
    }
    let kind = meta.file_type();
    if is_link {
        scan.items.push(Item {
            rel: rel.to_path_buf(),
            kind: ItemKind::Link,
            size: 0,
        });
        scan.files += 1;
        return Ok(());
    }
    if kind.is_dir() {
        let id = (meta.dev(), meta.ino());
        if ancestry.contains(&id) {
            // A link that leads back up: following it would never end.
            return Ok(());
        }
        let mut entries: Vec<_> = match fs::read_dir(path) {
            Ok(entries) => entries.collect::<Result<_, _>>()?,
            Err(error) => {
                return Err(OpError::Other(format!(
                    "“{}” could not be read: {}",
                    name,
                    super::plain(&error)
                )));
            }
        };
        entries.sort_by_key(|entry| entry.file_name());
        scan.items.push(Item {
            rel: rel.to_path_buf(),
            kind: ItemKind::Folder,
            size: 0,
        });
        scan.folders += 1;
        ancestry.push(id);
        for entry in entries {
            let child_rel = rel.join(entry.file_name());
            visit(&entry.path(), &child_rel, options, cancel, scan, ancestry)?;
        }
        ancestry.pop();
        return Ok(());
    }
    if kind.is_file() {
        if !readable(path) {
            return Err(OpError::Other(format!(
                "“{name}” cannot be read: you do not have permission."
            )));
        }
        scan.items.push(Item {
            rel: rel.to_path_buf(),
            kind: ItemKind::File,
            size: meta.len(),
        });
        scan.files += 1;
        scan.bytes += meta.len();
        return Ok(());
    }
    if kind.is_socket() || kind.is_fifo() || kind.is_block_device() || kind.is_char_device() {
        scan.skipped += 1;
    }
    Ok(())
}

/// What to do when the archive's name is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnConflict {
    /// Use the next free name: `Archive 2.zip`.
    KeepBoth,
    /// The old file goes to the Trash, once the new one is checked.
    Replace,
    /// Stop with [`OpError::Exists`].
    Refuse,
}

/// One archive to make.
#[derive(Clone, Debug)]
pub struct PackJob {
    /// Absolute paths, all in one folder.
    pub roots: Vec<PathBuf>,
    /// The folder the archive goes into.
    pub dest_dir: PathBuf,
    /// The name as typed, with or without the format's ending.
    pub base: String,
    pub options: PackOptions,
    pub on_conflict: OnConflict,
}

/// What [`pack`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed {
    /// The archive; the first volume of several is `archive`, the rest are
    /// in `files`.
    pub archive: PathBuf,
    /// Every file made: the archive and its other volumes.
    pub files: Vec<PathBuf>,
    /// The size of everything made.
    pub bytes: u64,
    /// The size of what went in.
    pub original_bytes: u64,
    pub entries: u64,
    /// The files the new archive replaced, now in the Trash.
    pub replaced: Vec<Trashed>,
    /// The originals, now in the Trash, when that was asked for.
    pub trashed: Vec<Trashed>,
    /// Why the originals were not trashed, if they were meant to be.
    pub trash_failed: Option<String>,
}

/// The hidden folder the programs work in. Removed when dropped.
pub(super) struct Work(pub(super) PathBuf);

impl Work {
    pub(super) fn make(dir: &Path) -> Result<Work, OpError> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        for _ in 0..1000 {
            let path = dir.join(format!(
                ".bingux-compress-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            match DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Work(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        Err(OpError::Other("A work folder could not be made.".into()))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The files in `dir` that are `name` or one of its volumes: `a.7z.001`,
/// `a.z01`.
fn existing(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let path = dir.join(name);
    if path.symlink_metadata().is_ok() {
        found.push(path);
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    let stem = name.strip_suffix(".zip");
    for entry in entries.flatten() {
        let other = entry.file_name().to_string_lossy().into_owned();
        let is_volume = |rest: &str| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit());
        let seven = other
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(is_volume);
        let zip = stem
            .and_then(|stem| other.strip_prefix(stem))
            .and_then(|rest| rest.strip_prefix(".z"))
            .is_some_and(is_volume);
        if seven || zip {
            found.push(entry.path());
        }
    }
    found
}

/// Is `name` (or a volume of it) in `dir`?
fn name_taken(dir: &Path, name: &str) -> bool {
    !existing(dir, name).is_empty()
}

/// The first format's reason it cannot carry these names, if one cannot.
fn carry_problem(format: Format, items: &[Item]) -> Option<String> {
    if format != Format::SevenZip {
        return None;
    }
    items.iter().find_map(|item| {
        let bytes = item.rel.as_os_str().as_bytes();
        if std::str::from_utf8(bytes).is_err() {
            Some(format!(
                "“{}” has a name that {} cannot store. Choose .zip or a .tar format.",
                item.rel.to_string_lossy(),
                format.extension()
            ))
        } else {
            None
        }
    })
}

/// 7-Zip drops a name that is not valid UTF-8 without a word, so a zip made
/// by it (a locked one) cannot carry such names either.
fn locked_zip_problem(options: &PackOptions, items: &[Item]) -> Option<String> {
    if options.format == Format::Zip && options.password.is_some() {
        return items.iter().find_map(|item| {
            std::str::from_utf8(item.rel.as_os_str().as_bytes()).is_err().then(|| {
                format!(
                    "“{}” has a name that a locked .zip cannot store. Remove the password or choose .tar.",
                    item.rel.to_string_lossy()
                )
            })
        });
    }
    None
}

/// Make the archive. Blocks; call it from a worker. [`Progress`] counts the
/// entries handled and the bytes that went in. See the [module](self).
pub fn pack(job: &PackJob, progress: &Progress, cancel: &Cancel) -> Result<Packed, OpError> {
    let options = job.options.clone().normalised();
    if let Some(reason) = options.format.unavailable() {
        return Err(OpError::Unsupported(reason));
    }
    if !fs::metadata(&job.dest_dir)?.is_dir() {
        return Err(OpError::Other("The destination is not a folder.".into()));
    }
    for root in &job.roots {
        root.symlink_metadata()?;
    }

    progress.set_scanning(true);
    let scanned = scan(&job.roots, &options, cancel);
    progress.set_scanning(false);
    let scanned = scanned?;
    if scanned.items.is_empty() {
        return Err(OpError::Other(
            "There is nothing to compress: every item was left out.".into(),
        ));
    }
    if let Some(problem) = carry_problem(options.format, &scanned.items)
        .or_else(|| locked_zip_problem(&options, &scanned.items))
    {
        return Err(OpError::Other(problem));
    }
    let parent = common_parent(&job.roots);
    let canonical_dest = fs::canonicalize(&job.dest_dir)?;
    let inside = job.roots.iter().any(|root| {
        root.is_dir()
            && !root.is_symlink()
            && fs::canonicalize(root).is_ok_and(|root| canonical_dest.starts_with(root))
    });
    if inside && options.delete_originals {
        return Err(OpError::Other(
            "The originals cannot be removed when the archive is saved inside them.".into(),
        ));
    }
    progress.add_total(
        scanned.bytes,
        scanned.items.len() as u64,
    );

    // The name.
    let wanted = file_name(&job.base, options.format);
    let taken = |name: &str| name_taken(&job.dest_dir, name);
    let mut replace = false;
    let name = if taken(&wanted) {
        match job.on_conflict {
            OnConflict::Refuse => return Err(OpError::Exists),
            OnConflict::Replace => {
                replace = true;
                wanted
            }
            OnConflict::KeepBoth => file_name(
                &free_base(&job.base, options.format, taken),
                options.format,
            ),
        }
    } else {
        wanted
    };

    let work = Work::make(&job.dest_dir)?;
    let names: Vec<PathBuf> = scanned.items.iter().map(|item| item.rel.clone()).collect();
    let Built { steps, list_file } = build(&options, &parent, &work.0, &name, &names)?;
    if let Some((path, bytes)) = &list_file {
        let mut file = fs::File::create(path)?;
        file.write_all(bytes)?;
    }
    let seven = match &steps {
        Steps::One(spec) => spec
            .program
            .file_name()
            .is_some_and(|program| program.to_string_lossy().starts_with("7z")),
        Steps::Pipe { .. } => false,
    };
    let sizes: HashMap<Vec<u8>, u64> = scanned
        .items
        .iter()
        .map(|item| (item.rel.as_os_str().as_bytes().to_vec(), item.size))
        .collect();
    let mut reported = 0u64;
    let total_bytes = scanned.bytes;
    let format = options.format;
    let mut on_line = |_: Stream, line: &[u8]| match hear(seven, format, line) {
        Some(Heard::Entry(entry)) => {
            let key = entry.strip_suffix(b"/").unwrap_or(&entry);
            progress.add_files(1);
            progress.set_current(&String::from_utf8_lossy(key));
            if !seven {
                let size = sizes.get(key).copied().unwrap_or(0);
                progress.add_bytes(size);
                reported += size;
            }
        }
        Some(Heard::Percent(percent)) => {
            let target = total_bytes / 100 * u64::from(percent);
            if target > reported {
                progress.add_bytes(target - reported);
                reported = target;
            }
        }
        None => {}
    };

    let archive_made = work.0.join(&name);
    let finished = match &steps {
        Steps::One(spec) => tool::run(spec, cancel, &mut on_line)?,
        Steps::Pipe { tar, squeeze } => {
            let out = fs::File::create(&archive_made)?;
            tool::run_pipe(tar, squeeze, out, cancel, &mut on_line)?
        }
    };
    if !finished.success() {
        return Err(finished.sentence("The archive could not be made"));
    }
    // A split 7-Zip archive is `name.001`, `name.002`, ...: the first is the one to open.
    let first_name = if options.format == Format::SevenZip && options.split_mib.is_some() {
        format!("{name}.001")
    } else {
        name.clone()
    };
    let archive_in_work = work.0.join(&first_name);
    if !archive_in_work.exists() {
        return Err(OpError::Other("The archive could not be made: the program wrote nothing.".into()));
    }

    // Read it back before it gets its name.
    progress.set_current("Checking the archive");
    let check = check_specs(&options, &archive_in_work, &work.0)?;
    let tested = tool::run(&check.test, cancel, &mut |_, _| {})?;
    if !tested.success() {
        return Err(tested.sentence("The new archive did not pass its check"));
    }
    let lists_seven = check
        .list
        .program
        .file_name()
        .is_some_and(|program| program.to_string_lossy().starts_with("7z"));
    let mut listed = 0u64;
    let listing = tool::run(&check.list, cancel, &mut |stream, line| {
        if stream == Stream::Out && is_listing_row(lists_seven, line) {
            listed += 1;
        }
    })?;
    let has_line_breaks = names.iter().any(|name| tool::bytes_of(name).contains(&b'\n'));
    if !listing.success() {
        return Err(listing.sentence("The new archive could not be listed"));
    }
    if !has_line_breaks && listed != scanned.items.len() as u64 {
        return Err(OpError::Other(format!(
            "The new archive is missing items: it holds {listed} of {}.",
            scanned.items.len()
        )));
    }
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }

    // Everything the programs made, in the work folder: the archive and its volumes.
    let mut made: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&work.0)? {
        let entry = entry?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if file == "names.list" {
            continue;
        }
        made.push(entry.path());
    }
    made.sort();
    for file in &made {
        sync_written(file)?;
    }

    // The old files, if they are to be replaced, go to the Trash first.
    let replaced = if replace {
        let old = existing(&job.dest_dir, &name);
        if old.is_empty() { Vec::new() } else { trash(&old)? }
    } else {
        Vec::new()
    };
    let mut placed: Vec<PathBuf> = Vec::new();
    for file in &made {
        let to = job
            .dest_dir
            .join(file.file_name().unwrap_or_default());
        match rename_noreplace(file, &to) {
            Ok(()) => placed.push(to),
            Err(error) => {
                for done in &placed {
                    let _ = fs::remove_file(done);
                }
                return Err(error.into());
            }
        }
    }
    sync_dirs(&[&job.dest_dir])?;
    let archive = job.dest_dir.join(&first_name);
    let bytes = placed
        .iter()
        .filter_map(|path| fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum();

    let (trashed, trash_failed) = if options.delete_originals {
        match trash(&job.roots) {
            Ok(items) => (items, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        }
    } else {
        (Vec::new(), None)
    };
    progress.settle();
    Ok(Packed {
        archive,
        files: placed,
        bytes,
        original_bytes: scanned.bytes,
        entries: scanned.items.len() as u64,
        replaced,
        trashed,
        trash_failed,
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    use super::super::pack_plan::{Excludes, Level, Password};
    use super::super::testing::{Temp, with_data_home};
    use super::*;

    /// A tree with the things that go wrong: dotfiles, links, an empty
    /// folder, spaces, a dash, unicode and a name that is not UTF-8.
    fn tree(temp: &Temp) -> PathBuf {
        let root = temp.dir("src");
        fs::write(root.join("plain.txt"), "plain text\n".repeat(200)).unwrap();
        fs::write(root.join(".hidden"), "secret").unwrap();
        fs::write(root.join("-dash"), "dash").unwrap();
        fs::write(root.join("sp ace.txt"), "space").unwrap();
        fs::write(root.join("star*[x]"), "glob").unwrap();
        fs::write(root.join("üñí.txt"), "unicode").unwrap();
        fs::write(root.join("trail "), "trailing space").unwrap();
        fs::create_dir(root.join("empty")).unwrap();
        fs::create_dir_all(root.join("sub/.cache")).unwrap();
        fs::write(root.join("sub/a.log"), "log").unwrap();
        fs::write(root.join("sub/b.txt"), "b").unwrap();
        fs::write(root.join("sub/.cache/c"), "c").unwrap();
        std::os::unix::fs::symlink("plain.txt", root.join("link")).unwrap();
        root
    }

    fn with_bad_name(root: &Path) {
        let name = std::ffi::OsString::from_vec(b"bad-\xff-name".to_vec());
        fs::write(root.join(name), "not utf8").unwrap();
    }

    fn job(root: &Path, dest: &Path, format: Format) -> PackJob {
        PackJob {
            roots: vec![root.to_path_buf()],
            dest_dir: dest.to_path_buf(),
            base: "out".into(),
            options: PackOptions { format, ..PackOptions::default() },
            on_conflict: OnConflict::KeepBoth,
        }
    }

    fn run(job: &PackJob) -> Result<Packed, OpError> {
        pack(job, &Progress::new(), &Cancel::new())
    }

    fn unpack(archive: &Path, into: &Path, password: Option<&str>) {
        fs::create_dir_all(into).unwrap();
        let name = archive.to_string_lossy().into_owned();
        let status = if name.ends_with(".zip") && password.is_none() {
            Command::new("unzip").arg("-q").arg(archive).arg("-d").arg(into).status().unwrap()
        } else if name.ends_with(".zip") || name.ends_with(".7z") || name.contains(".7z.") {
            let mut command = Command::new(tool::seven_zip().unwrap());
            command.arg("x").arg("-y").arg(format!("-o{}", into.display()));
            if let Some(password) = password {
                // 7-Zip asks for the password on standard input when the archive
                // is locked; `-p` with nothing after it would mean an empty one.
                command.stdin(std::process::Stdio::piped());
                command.arg(archive);
                let mut child = command.stdout(std::process::Stdio::null()).spawn().unwrap();
                child.stdin.take().unwrap().write_all(format!("{password}\n").as_bytes()).unwrap();
                assert!(child.wait().unwrap().success(), "7z could not open {name}");
                return;
            }
            command.arg(archive).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).status().unwrap()
        } else {
            Command::new("tar").arg("-xf").arg(archive).arg("-C").arg(into).status().unwrap()
        };
        assert!(status.success(), "could not open {name}");
    }

    /// Every path below `root` with what it is, as comparable text.
    fn contents(root: &Path) -> Vec<String> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
            let mut entries: Vec<_> = fs::read_dir(dir).unwrap().flatten().collect();
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().into_owned();
                let meta = path.symlink_metadata().unwrap();
                if meta.file_type().is_symlink() {
                    out.push(format!("{rel} -> {}", fs::read_link(&path).unwrap().display()));
                } else if meta.is_dir() {
                    out.push(format!("{rel}/"));
                    walk(&path, base, out);
                } else {
                    out.push(format!("{rel} = {}", String::from_utf8_lossy(&fs::read(&path).unwrap()).len()));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out
    }

    #[test]
    fn scan_counts_what_goes_in() {
        let temp = Temp::new("scan");
        let root = tree(&temp);
        let scanned = scan(&[root], &PackOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(scanned.folders, 4, "src, empty, sub and sub/.cache");
        assert_eq!(scanned.files, 11, "eight files in src, three below it; the link counts");
        assert!(scanned.bytes > 0 && scanned.skipped == 0);
    }

    #[test]
    fn scan_leaves_out_hidden_and_excluded() {
        let temp = Temp::new("scan-filter");
        let root = tree(&temp);
        let names = |options: &PackOptions| -> Vec<String> {
            scan(&[root.clone()], options, &Cancel::new())
                .unwrap()
                .items
                .iter()
                .map(|item| item.rel.to_string_lossy().into_owned())
                .collect()
        };
        let all = names(&PackOptions::default());
        assert!(all.contains(&"src/.hidden".to_string()));
        assert!(all.contains(&"src/sub/.cache/c".to_string()));
        assert!(all.contains(&"src/empty".to_string()), "an empty folder is kept");
        let plain = names(&PackOptions { include_hidden: false, ..PackOptions::default() });
        assert!(!plain.iter().any(|name| name.contains("/.")), "{plain:?}");
        assert!(plain.contains(&"src/sub/b.txt".to_string()));
        let excluded = names(&PackOptions {
            excludes: Excludes::parse("*.log, empty").unwrap(),
            ..PackOptions::default()
        });
        assert!(!excluded.contains(&"src/sub/a.log".to_string()));
        assert!(!excluded.contains(&"src/empty".to_string()));
        assert!(excluded.contains(&"src/sub/b.txt".to_string()));
    }

    #[test]
    fn scan_keeps_links_or_follows_them() {
        let temp = Temp::new("scan-links");
        let root = temp.dir("src");
        fs::create_dir(root.join("real")).unwrap();
        fs::write(root.join("real/f"), "data").unwrap();
        std::os::unix::fs::symlink("real", root.join("dirlink")).unwrap();
        std::os::unix::fs::symlink("..", root.join("real/loop")).unwrap();
        let kept = scan(&[root.clone()], &PackOptions::default(), &Cancel::new()).unwrap();
        let kinds = |scan: &Scan, name: &str| {
            scan.items.iter().find(|item| item.rel.ends_with(name)).map(|item| item.kind)
        };
        assert_eq!(kinds(&kept, "dirlink"), Some(ItemKind::Link));
        assert!(!kept.items.iter().any(|item| item.rel.ends_with("dirlink/f")));
        let followed = scan(
            &[root],
            &PackOptions { follow_links: true, ..PackOptions::default() },
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(kinds(&followed, "dirlink"), Some(ItemKind::Folder));
        assert!(followed.items.iter().any(|item| item.rel.ends_with("dirlink/f")));
    }

    #[test]
    fn scan_refuses_nonsense() {
        assert!(scan(&[], &PackOptions::default(), &Cancel::new()).is_err());
        assert!(scan(&[PathBuf::from("rel")], &PackOptions::default(), &Cancel::new()).is_err());
        assert!(scan(&[PathBuf::from("/")], &PackOptions::default(), &Cancel::new()).is_err());
    }

    /// Every format this machine can make: build it, read it back with the
    /// system's own tools and compare every path and link.
    #[test]
    fn every_available_format_round_trips() {
        for format in Format::ALL {
            if !format.is_available() {
                eprintln!("skipped {format:?}: {:?}", format.unavailable());
                continue;
            }
            let temp = Temp::new("round-trip");
            let root = tree(&temp);
            let dest = temp.dir("out");
            // 7-Zip cannot carry a name that is not UTF-8; every other format can.
            if format != Format::SevenZip {
                with_bad_name(&root);
            }
            let packed = run(&job(&root, &dest, format)).unwrap_or_else(|error| panic!("{format:?}: {error}"));
            assert_eq!(packed.archive, dest.join(format!("out{}", format.extension())));
            assert!(packed.bytes > 0);
            let opened = temp.dir("opened");
            unpack(&packed.archive, &opened, None);
            let (mut found, mut expected) = (contents(&opened.join("src")), contents(&root));
            if format == Format::Zip {
                // Info-ZIP's unzip rewrites a name that is not UTF-8 as it
                // unpacks, so check that name in the archive itself.
                let raw = fs::read(&packed.archive).unwrap();
                assert!(raw.windows(14).any(|window| window == b"src/bad-\xff-name"));
                found.retain(|line| !line.starts_with("bad-"));
                expected.retain(|line| !line.starts_with("bad-"));
            }
            assert_eq!(found, expected, "{format:?}");
            // No work folder is left.
            let left: Vec<_> = fs::read_dir(&dest).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(left.len(), 1, "{format:?}: {left:?}");
        }
    }

    #[test]
    fn a_seven_zip_with_a_name_it_cannot_carry_is_refused_by_name() {
        if !Format::SevenZip.is_available() {
            return;
        }
        let temp = Temp::new("seven-bad-name");
        let root = tree(&temp);
        with_bad_name(&root);
        let dest = temp.dir("out");
        let error = run(&job(&root, &dest, Format::SevenZip)).unwrap_err();
        assert!(error.to_string().contains("bad-"), "{error}");
        assert!(error.to_string().contains(".7z"), "{error}");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0, "nothing is left behind");
    }

    #[test]
    fn levels_change_the_size_of_a_squeezable_archive() {
        if !Format::TarXz.is_available() {
            return;
        }
        let temp = Temp::new("levels");
        let root = temp.dir("src");
        // Text built from a small vocabulary in a scrambled order, so a bigger
        // dictionary finds more to squeeze than a small one.
        let words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel"];
        let mut seed = 7u64;
        let mut text = String::new();
        while text.len() < 3_000_000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            text.push_str(words[(seed >> 61) as usize]);
            text.push(' ');
        }
        fs::write(root.join("text"), text).unwrap();
        let dest = temp.dir("out");
        let size = |level| {
            let mut job = job(&root, &dest, Format::TarXz);
            job.options.level = level;
            job.base = format!("{level:?}");
            run(&job).unwrap().bytes
        };
        assert!(size(Level::Smallest) <= size(Level::Fastest));
    }

    #[test]
    fn hidden_files_and_excludes_are_left_out_of_the_archive() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("filters");
        let root = tree(&temp);
        let dest = temp.dir("out");
        let mut job = job(&root, &dest, Format::Zip);
        job.options.include_hidden = false;
        job.options.excludes = Excludes::parse("*.log, empty").unwrap();
        let packed = run(&job).unwrap();
        let opened = temp.dir("opened");
        unpack(&packed.archive, &opened, None);
        let found = contents(&opened.join("src"));
        assert!(!found.iter().any(|line| line.contains("/.") || line.starts_with('.')), "{found:?}");
        assert!(!found.iter().any(|line| line.contains("a.log") || line.starts_with("empty")), "{found:?}");
        assert!(found.iter().any(|line| line.starts_with("sub/b.txt")));
    }

    #[test]
    fn following_links_stores_the_file_not_the_link() {
        for format in [Format::Zip, Format::TarGz, Format::SevenZip] {
            if !format.is_available() {
                continue;
            }
            let temp = Temp::new("follow");
            let root = temp.dir("src");
            fs::write(root.join("real"), "real file").unwrap();
            std::os::unix::fs::symlink("real", root.join("link")).unwrap();
            let dest = temp.dir("out");
            let mut job = job(&root, &dest, format);
            job.options.follow_links = true;
            let packed = run(&job).unwrap();
            let opened = temp.dir("opened");
            unpack(&packed.archive, &opened, None);
            let meta = opened.join("src/link").symlink_metadata().unwrap();
            assert!(meta.is_file(), "{format:?}: the link was stored as a file");
        }
    }

    #[test]
    fn an_encrypted_archive_needs_its_password() {
        if tool::seven_zip().is_none() {
            return;
        }
        for (format, hide) in [(Format::Zip, false), (Format::SevenZip, false), (Format::SevenZip, true)] {
            let temp = Temp::new("locked");
            let root = tree(&temp);
            let dest = temp.dir("out");
            let mut job = job(&root, &dest, format);
            job.options.password = Some(Password::new("correct horse"));
            job.options.hide_names = hide;
            let packed = run(&job).unwrap_or_else(|error| panic!("{format:?}: {error}"));
            let opened = temp.dir("opened");
            unpack(&packed.archive, &opened, Some("correct horse"));
            assert_eq!(contents(&opened.join("src")), contents(&root), "{format:?} hide={hide}");
            // The password is not in the file, and a wrong one fails.
            let raw = fs::read(&packed.archive).unwrap();
            assert!(!raw.windows(13).any(|window| window == b"correct horse"));
            if hide {
                assert!(!raw.windows(8).any(|window| window == b"plain.tx"), "names are hidden");
            }
            let wrong = Command::new(tool::seven_zip().unwrap())
                .args(["t", "-pnot the password"])
                .arg(&packed.archive)
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert!(!wrong.status.success(), "{format:?}: a wrong password must fail");
        }
    }

    #[test]
    fn volumes_are_made_and_open_together() {
        for format in [Format::Zip, Format::SevenZip] {
            if !format.is_available() {
                continue;
            }
            let temp = Temp::new("volumes");
            let root = temp.dir("src");
            // Incompressible bytes, so the archive is as big as the input.
            let mut seed = 12345u64;
            let noise: Vec<u8> = (0..3_200_000)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (seed >> 33) as u8
                })
                .collect();
            fs::write(root.join("noise.bin"), &noise).unwrap();
            let dest = temp.dir("out");
            let mut job = job(&root, &dest, format);
            job.options.split_mib = Some(1);
            if format == Format::Zip && tool::find("zip").is_none() && tool::seven_zip().is_none() {
                continue;
            }
            let packed = run(&job).unwrap_or_else(|error| panic!("{format:?}: {error}"));
            assert!(packed.files.len() >= 3, "{format:?}: {:?}", packed.files);
            let opened = temp.dir("opened");
            let first = if format == Format::SevenZip { dest.join("out.7z.001") } else { packed.archive.clone() };
            let status = Command::new(tool::seven_zip().unwrap())
                .args(["x", "-y"])
                .arg(format!("-o{}", opened.display()))
                .arg(&first)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
            assert_eq!(fs::read(opened.join("src/noise.bin")).unwrap(), noise);
        }
    }

    #[test]
    fn cancel_leaves_no_partial_file() {
        if !Format::TarXz.is_available() {
            return;
        }
        let temp = Temp::new("cancel");
        let root = temp.dir("src");
        let mut seed = 99u64;
        let noise: Vec<u8> = (0..24_000_000)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 40) as u8
            })
            .collect();
        fs::write(root.join("noise.bin"), &noise).unwrap();
        let dest = temp.dir("out");
        let mut job = job(&root, &dest, Format::TarXz);
        job.options.level = Level::Smallest;
        let cancel = Cancel::new();
        let stopper = cancel.clone();
        let watcher = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            stopper.cancel();
        });
        let outcome = pack(&job, &Progress::new(), &cancel);
        watcher.join().unwrap();
        assert_eq!(outcome.unwrap_err(), OpError::Cancelled);
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0, "no archive, no part, no work folder");
    }

    #[test]
    fn a_taken_name_is_numbered_replaced_or_refused() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("conflict");
        let root = tree(&temp);
        let dest = temp.dir("out");
        fs::write(dest.join("out.zip"), "old").unwrap();
        let kept = run(&job(&root, &dest, Format::Zip)).unwrap();
        assert_eq!(kept.archive, dest.join("out 2.zip"));
        assert_eq!(fs::read(dest.join("out.zip")).unwrap(), b"old");
        let mut refuse = job(&root, &dest, Format::Zip);
        refuse.on_conflict = OnConflict::Refuse;
        assert_eq!(run(&refuse).unwrap_err(), OpError::Exists);
        assert_eq!(fs::read(dest.join("out.zip")).unwrap(), b"old");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 2, "nothing else was left");
    }

    #[test]
    fn replacing_sends_the_old_file_to_the_trash_and_can_be_undone() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("replace");
        let data = temp.dir("data");
        // The trash lives under the data home, which the test points at the temp folder.
        let root = tree(&temp);
        let dest = temp.dir("out");
        fs::write(dest.join("out.zip"), "old").unwrap();
        let mut replace = job(&root, &dest, Format::Zip);
        replace.on_conflict = OnConflict::Replace;
        let outcome = with_data_home(&data, || run(&replace));
        let packed = outcome.unwrap();
        assert_eq!(packed.archive, dest.join("out.zip"));
        assert_eq!(packed.replaced.len(), 1);
        assert_ne!(fs::read(dest.join("out.zip")).unwrap(), b"old");
    }

    #[test]
    fn nothing_left_after_the_filters_is_an_error_not_an_empty_archive() {
        let temp = Temp::new("nothing");
        let root = temp.dir("src");
        fs::write(root.join("a.log"), "x").unwrap();
        let dest = temp.dir("out");
        let mut job = job(&root.join("a.log"), &dest, Format::Zip);
        job.options.excludes = Excludes::parse("*.log").unwrap();
        let error = run(&job).unwrap_err();
        assert!(error.to_string().contains("left out"), "{error}");
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 0);
    }

    #[test]
    fn bad_requests_are_refused() {
        let temp = Temp::new("bad");
        let file = temp.write("a.txt", "x");
        let dest = temp.dir("out");
        let mut empty = job(&file, &dest, Format::Zip);
        empty.roots.clear();
        assert!(run(&empty).is_err());
        let mut relative = job(&file, &dest, Format::Zip);
        relative.roots = vec![PathBuf::from("a.txt")];
        assert!(run(&relative).is_err());
        let mut gone = job(&file, &dest, Format::Zip);
        gone.dest_dir = dest.join("missing");
        assert!(run(&gone).is_err());
        let mut inside = job(&temp.path().join("out"), &temp.path().join("out"), Format::Zip);
        inside.options.delete_originals = true;
        assert!(run(&inside).unwrap_err().to_string().contains("saved inside"));
    }

    #[test]
    fn progress_counts_entries_and_bytes() {
        if !Format::TarGz.is_available() {
            return;
        }
        let temp = Temp::new("progress");
        let root = tree(&temp);
        let dest = temp.dir("out");
        let progress = Progress::new();
        pack(&job(&root, &dest, Format::TarGz), &progress, &Cancel::new()).unwrap();
        let scanned = scan(&[root], &PackOptions::default(), &Cancel::new()).unwrap();
        assert_eq!(progress.files_total(), scanned.items.len() as u64);
        assert_eq!(progress.files_done(), scanned.items.len() as u64);
        assert_eq!(progress.bytes_done(), scanned.bytes);
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn deleting_originals_trashes_them_after_the_archive_is_in_place() {
        if !Format::Zip.is_available() {
            return;
        }
        let temp = Temp::new("originals");
        let data = temp.dir("data");
        let root = temp.dir("src");
        fs::write(root.join("keep.txt"), "kept").unwrap();
        let dest = temp.dir("out");
        let mut job = job(&root, &dest, Format::Zip);
        job.options.delete_originals = true;
        let outcome = with_data_home(&data, || run(&job));
        let packed = outcome.unwrap();
        assert!(packed.archive.exists());
        assert!(!root.exists(), "the original is gone from its place");
        assert_eq!(packed.trashed.len(), 1);
        assert!(packed.trashed[0].in_trash.exists(), "and is in the Trash");
    }
}
