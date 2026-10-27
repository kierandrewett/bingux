//! The bin, following the freedesktop.org Trash specification 1.0.
//!
//! Files go to `$XDG_DATA_HOME/Trash` when they are on the same drive as it.
//! On any other drive they go to `$topdir/.Trash/$uid` (when the admin made
//! a sticky `.Trash`) or `$topdir/.Trash-$uid`. A file is never copied across
//! drives to bin it: that would turn "bin it" into a slow copy, so the answer
//! is [`OpError::Unsupported`].

use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::listing::Cancel;
use crate::location::file_uri;
use crate::mounts::{self, Mount, MountKind};

use super::durable::sync_dirs;
use super::names::split_extension;
use super::transfer::delete;
use super::{OpError, Progress, rename_noreplace};

/// One item moved to the bin: enough to put it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trashed {
    /// Where it was.
    pub original: PathBuf,
    /// Where it is now, inside the bin's `files` folder.
    pub in_trash: PathBuf,
    /// Its `.trashinfo` file.
    pub info: PathBuf,
}

fn uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// `$XDG_DATA_HOME` (absolute), else `~/.local/share`.
fn data_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME")
        && Path::new(&dir).is_absolute()
    {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    Some(PathBuf::from(home).join(".local/share"))
}

fn home_trash() -> Option<PathBuf> {
    data_home().map(|dir| dir.join("Trash"))
}

/// Is `path` already inside a bin?
fn in_a_bin(path: &Path, home_trash: Option<&Path>) -> bool {
    if home_trash.is_some_and(|trash| path.starts_with(trash)) {
        return true;
    }
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name == ".Trash" || name.starts_with(".Trash-"))
    })
}

/// Can `path` go to the bin? Cheap: reads the mount table (cached for two
/// seconds) and, for a network drive, looks for a bin that is already there.
/// It never touches `path` itself, so it is safe to ask for every row.
///
/// False for the root, for things already in a bin, for read-only drives and
/// for a network drive without a `.Trash-$uid` folder to use. True for the
/// home drive and other local drives.
pub fn can_trash(path: &Path) -> bool {
    can_trash_with(path, &mounts::all_mounts(), home_trash().as_deref(), uid())
}

fn can_trash_with(path: &Path, table: &[Mount], home_trash: Option<&Path>, uid: u32) -> bool {
    if !path.is_absolute() || path.parent().is_none() || in_a_bin(path, home_trash) {
        return false;
    }
    let Some(mount) = mounts::mount_for(path, table) else {
        return home_trash.is_some();
    };
    if mount.read_only {
        return false;
    }
    let home_mount = home_trash.and_then(|trash| mounts::mount_for(trash, table));
    if home_mount.is_some_and(|home| home.mount_point == mount.mount_point) {
        return true;
    }
    if mount.kind == MountKind::Network {
        let top = &mount.mount_point;
        return top.join(format!(".Trash-{uid}")).is_dir()
            || top.join(".Trash").join(uid.to_string()).is_dir();
    }
    true
}

/// Is `path` inside a bin: the one in your home folder, or one at the top of
/// a drive? Cheap: it only reads the name.
pub fn in_trash(path: &Path) -> bool {
    in_a_bin(path, home_trash().as_deref())
}

/// Every bin that exists: yours at home, and the one for this user at the top
/// of each writable drive.
fn existing_bins(table: &[Mount], home: Option<&Path>, uid: u32) -> Vec<PathBuf> {
    let mut bins: Vec<PathBuf> = home
        .filter(|bin| bin.join("files").is_dir())
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    for mount in table.iter().filter(|mount| !mount.read_only) {
        let shared = mount.mount_point.join(".Trash");
        // The shared bin counts only when the admin made it sticky, as the
        // specification says, and it is not a link.
        if let Ok(meta) = shared.symlink_metadata()
            && meta.is_dir()
            && meta.mode() & 0o1000 != 0
        {
            let own = shared.join(uid.to_string());
            if own.join("files").is_dir() {
                bins.push(own);
            }
        }
        let own = mount.mount_point.join(format!(".Trash-{uid}"));
        if own.join("files").is_dir() && !bins.contains(&own) {
            bins.push(own);
        }
    }
    bins.sort();
    bins.dedup();
    bins
}

/// Delete everything in every bin for good. This cannot be undone; ask
/// first. Progress counts what is removed, and [`Cancel`] stops it between
/// items. Nothing is done when the bins are empty.
pub fn empty_trash(progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    let table = mounts::all_mounts();
    empty_bins(
        &existing_bins(&table, home_trash().as_deref(), uid()),
        progress,
        cancel,
    )
}

fn empty_bins(bins: &[PathBuf], progress: &Progress, cancel: &Cancel) -> Result<(), OpError> {
    let mut doomed = Vec::new();
    for bin in bins {
        // The files first: an info file with no file is harmless, a file with
        // no info could not be put back.
        for part in ["files", "info"] {
            if let Ok(entries) = fs::read_dir(bin.join(part)) {
                doomed.extend(entries.flatten().map(|entry| entry.path()));
            }
        }
    }
    if doomed.is_empty() {
        return Ok(());
    }
    delete(&doomed, progress, cancel)
}

/// Move items to the bin. All or nothing: if one fails, the ones already
/// moved are put back and the error is returned.
///
/// `paths` must be absolute. Links are binned as links, never followed.
pub fn trash(paths: &[PathBuf]) -> Result<Vec<Trashed>, OpError> {
    let home = home_trash()
        .ok_or_else(|| OpError::Other("Your home folder could not be found.".into()))?;
    let uid = uid();
    let mut done = Vec::with_capacity(paths.len());
    for path in paths {
        match trash_one(path, &home, uid) {
            Ok(item) => done.push(item),
            Err(error) => {
                let _ = untrash(&done);
                return Err(error);
            }
        }
    }
    Ok(done)
}

fn trash_one(path: &Path, home: &Path, uid: u32) -> Result<Trashed, OpError> {
    if !path.is_absolute() {
        return Err(OpError::Other("The path must be absolute.".into()));
    }
    if path.parent().is_none() {
        return Err(OpError::Unsupported(
            "The root folder cannot be moved to the Trash.".into(),
        ));
    }
    let meta = path.symlink_metadata()?;
    make_trash_dirs(home)?;
    if fs::metadata(home.join("files"))?.dev() == meta.dev() {
        return trash_into(path, home, None);
    }
    let top = topdir(path, meta.dev())?;
    let dir = topdir_trash(&top, uid)?;
    trash_into(path, &dir, Some(&top))
}

/// The mount point of the drive `path` is on: the highest ancestor with the
/// same device number.
fn topdir(path: &Path, dev: u64) -> Result<PathBuf, OpError> {
    let mut top = path.parent().map(Path::to_path_buf).unwrap_or_default();
    if fs::metadata(&top)?.dev() != dev {
        return Err(OpError::Unsupported(
            "A drive cannot be moved to the Trash.".into(),
        ));
    }
    while let Some(parent) = top.parent() {
        if fs::metadata(parent)?.dev() != dev {
            break;
        }
        top = parent.to_path_buf();
    }
    Ok(top)
}

fn no_bin_here() -> OpError {
    OpError::Unsupported(
        "This drive has no Trash and one cannot be made there, so it cannot be moved to the Trash."
            .into(),
    )
}

/// The bin to use on the drive mounted at `top`: `.Trash/$uid` when a sticky
/// `.Trash` is there, else `.Trash-$uid`, made with mode 0700.
fn topdir_trash(top: &Path, uid: u32) -> Result<PathBuf, OpError> {
    let shared = top.join(".Trash");
    if let Ok(meta) = shared.symlink_metadata()
        && meta.is_dir()
        && meta.mode() & 0o1000 != 0
    {
        let own = shared.join(uid.to_string());
        if make_trash_dirs(&own).is_ok() {
            return Ok(own);
        }
    }
    let own = top.join(format!(".Trash-{uid}"));
    if let Ok(meta) = own.symlink_metadata()
        && !meta.is_dir()
    {
        return Err(no_bin_here());
    }
    make_trash_dirs(&own).map_err(|_| no_bin_here())?;
    Ok(own)
}

/// `dir`, `dir/files` and `dir/info`, mode 0700 where made.
fn make_trash_dirs(dir: &Path) -> Result<(), OpError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(dir.join("files"))?;
    builder.create(dir.join("info"))?;
    Ok(())
}

/// `/path/with%20escapes`, as `Path=` holds it.
fn encode_path(path: &Path) -> String {
    file_uri(path).trim_start_matches("file://").to_string()
}

/// Local time as `YYYY-MM-DDThh:mm:ss`.
fn deletion_date(now: SystemTime) -> String {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs()) as libc::time_t;
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

/// Move `src` into the bin at `trash_dir` (which holds `files` and `info`).
/// `topdir` is set for a drive bin: `Path=` is then relative to it.
///
/// The `.trashinfo` is made first with `O_EXCL`, which reserves the name.
fn trash_into(src: &Path, trash_dir: &Path, topdir: Option<&Path>) -> Result<Trashed, OpError> {
    make_trash_dirs(trash_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| OpError::Other("That cannot be moved to the Trash.".into()))?
        .to_string_lossy()
        .into_owned();
    let (stem, extension) = split_extension(&name);
    let recorded = match topdir {
        Some(top) => src.strip_prefix(top).unwrap_or(src),
        None => src,
    };
    let text = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        encode_path(recorded),
        deletion_date(SystemTime::now())
    );
    let mut attempt = 1u32;
    loop {
        let unique = if attempt == 1 {
            name.clone()
        } else {
            format!("{stem}.{attempt}{extension}")
        };
        attempt += 1;
        let info = trash_dir.join("info").join(format!("{unique}.trashinfo"));
        let in_trash = trash_dir.join("files").join(&unique);
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        if in_trash.symlink_metadata().is_ok() {
            // A file with no info (left by another program): leave it alone.
            drop(file);
            let _ = fs::remove_file(&info);
            continue;
        }
        let moved = file
            .write_all(text.as_bytes())
            .and_then(|()| fs::rename(src, &in_trash));
        drop(file);
        return match moved {
            Ok(()) => {
                // One `rename(2)` did the move; on removable media, shares and
                // FAT the three folders it touched are made durable too.
                sync_dirs(&[
                    src.parent().unwrap_or(Path::new("/")),
                    &trash_dir.join("files"),
                    &trash_dir.join("info"),
                ])?;
                Ok(Trashed {
                    original: src.to_path_buf(),
                    in_trash,
                    info,
                })
            }
            Err(error) => {
                let _ = fs::remove_file(&info);
                Err(error.into())
            }
        };
    }
}

/// Put items back where they were and remove their `.trashinfo`. Makes
/// missing parent folders. Refuses to replace anything: if any original path
/// is taken the answer is [`OpError::Exists`] and nothing has moved.
pub fn untrash(items: &[Trashed]) -> Result<Vec<PathBuf>, OpError> {
    for item in items {
        if item.original.symlink_metadata().is_ok() {
            return Err(OpError::Exists);
        }
    }
    let mut restored = Vec::with_capacity(items.len());
    for item in items {
        if let Some(parent) = item.original.parent() {
            fs::create_dir_all(parent)?;
        }
        rename_noreplace(&item.in_trash, &item.original)?;
        match fs::remove_file(&item.info) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        restored.push(item.original.clone());
    }
    for item in &restored {
        if let Some(parent) = item.parent() {
            sync_dirs(&[parent])?;
        }
    }
    for item in items {
        if let Some(info) = item.info.parent() {
            sync_dirs(&[info])?;
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;

    fn info_of(item: &Trashed) -> String {
        fs::read_to_string(&item.info).unwrap()
    }

    #[test]
    fn round_trip_through_a_bin() {
        let temp = Temp::new("trash");
        let bin = temp.path().join("Trash");
        let file = temp.write("docs/note.txt", "hello");
        let item = trash_into(&file, &bin, None).unwrap();
        assert!(!file.exists());
        assert_eq!(item.in_trash, bin.join("files/note.txt"));
        assert_eq!(item.info, bin.join("info/note.txt.trashinfo"));
        assert_eq!(fs::read_to_string(&item.in_trash).unwrap(), "hello");

        let restored = untrash(&[item.clone()]).unwrap();
        assert_eq!(restored, vec![file.clone()]);
        assert_eq!(fs::read_to_string(&file).unwrap(), "hello");
        assert!(!item.info.exists());
        assert!(!item.in_trash.exists());
    }

    #[test]
    fn the_info_file_follows_the_spec() {
        let temp = Temp::new("trash-info");
        let bin = temp.path().join("Trash");
        let file = temp.write("a b/ünï 100%.txt", "x");
        let item = trash_into(&file, &bin, None).unwrap();
        let text = info_of(&item);
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("[Trash Info]"));
        let path_line = lines.next().unwrap();
        let expected = format!(
            "Path={}/a%20b/%C3%BCn%C3%AF%20100%25.txt",
            temp.path().display()
        );
        assert_eq!(path_line, expected);
        let date = lines.next().unwrap().strip_prefix("DeletionDate=").unwrap();
        let bytes = date.as_bytes();
        assert_eq!(bytes.len(), 19, "{date}");
        assert_eq!(
            (bytes[4], bytes[7], bytes[10], bytes[13], bytes[16]),
            (b'-', b'-', b'T', b':', b':')
        );
        assert!(
            date.chars()
                .filter(|c| !"-T:".contains(*c))
                .all(|c| c.is_ascii_digit())
        );
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn names_that_clash_get_a_number() {
        let temp = Temp::new("trash-clash");
        let bin = temp.path().join("Trash");
        let first = temp.write("one/a.txt", "1");
        let second = temp.write("two/a.txt", "2");
        let third = temp.write("three/a.txt", "3");
        let a = trash_into(&first, &bin, None).unwrap();
        let b = trash_into(&second, &bin, None).unwrap();
        let c = trash_into(&third, &bin, None).unwrap();
        assert_eq!(a.in_trash.file_name().unwrap(), "a.txt");
        assert_eq!(b.in_trash.file_name().unwrap(), "a.2.txt");
        assert_eq!(c.in_trash.file_name().unwrap(), "a.3.txt");
        assert_eq!(fs::read_to_string(&b.in_trash).unwrap(), "2");
        // Each goes back to its own place.
        untrash(&[c, a, b]).unwrap();
        assert_eq!(fs::read_to_string(&first).unwrap(), "1");
        assert_eq!(fs::read_to_string(&second).unwrap(), "2");
        assert_eq!(fs::read_to_string(&third).unwrap(), "3");
    }

    #[test]
    fn an_orphan_in_files_is_not_overwritten() {
        let temp = Temp::new("trash-orphan");
        let bin = temp.path().join("Trash");
        fs::create_dir_all(bin.join("files")).unwrap();
        fs::write(bin.join("files/a.txt"), "orphan").unwrap();
        let file = temp.write("a.txt", "new");
        let item = trash_into(&file, &bin, None).unwrap();
        assert_eq!(item.in_trash.file_name().unwrap(), "a.2.txt");
        assert_eq!(
            fs::read_to_string(bin.join("files/a.txt")).unwrap(),
            "orphan"
        );
    }

    #[test]
    fn folders_and_links_are_binned_as_they_are() {
        let temp = Temp::new("trash-kinds");
        let bin = temp.path().join("Trash");
        let dir = temp.dir("folder");
        temp.write("folder/inner.txt", "x");
        let target = temp.write("target.txt", "t");
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let dir_item = trash_into(&dir, &bin, None).unwrap();
        let link_item = trash_into(&link, &bin, None).unwrap();
        assert!(dir_item.in_trash.join("inner.txt").exists());
        assert!(link_item.in_trash.symlink_metadata().unwrap().is_symlink());
        assert!(target.exists(), "the target of a link stays");
        untrash(&[dir_item, link_item]).unwrap();
        assert!(dir.join("inner.txt").exists());
        assert_eq!(fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn untrash_refuses_to_overwrite_and_moves_nothing() {
        let temp = Temp::new("trash-refuse");
        let bin = temp.path().join("Trash");
        let a = temp.write("a.txt", "a");
        let b = temp.write("b.txt", "b");
        let item_a = trash_into(&a, &bin, None).unwrap();
        let item_b = trash_into(&b, &bin, None).unwrap();
        fs::write(&b, "someone else").unwrap();
        assert_eq!(
            untrash(&[item_a.clone(), item_b.clone()]),
            Err(OpError::Exists)
        );
        assert!(item_a.in_trash.exists(), "the first was not moved either");
        assert!(item_b.in_trash.exists());
        assert_eq!(fs::read_to_string(&b).unwrap(), "someone else");
    }

    #[test]
    fn untrash_makes_missing_parents() {
        let temp = Temp::new("trash-parents");
        let bin = temp.path().join("Trash");
        let file = temp.write("deep/er/a.txt", "a");
        let item = trash_into(&file, &bin, None).unwrap();
        fs::remove_dir_all(temp.path().join("deep")).unwrap();
        untrash(&[item]).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "a");
    }

    #[test]
    fn bins_are_private() {
        let temp = Temp::new("trash-mode");
        let bin = temp.path().join("Trash");
        let file = temp.write("a.txt", "a");
        trash_into(&file, &bin, None).unwrap();
        for dir in [bin.clone(), bin.join("files"), bin.join("info")] {
            assert_eq!(fs::metadata(dir).unwrap().mode() & 0o777, 0o700);
        }
    }

    #[test]
    fn a_drive_bin_records_a_path_relative_to_the_top() {
        let temp = Temp::new("trash-top");
        let top = temp.dir("drive");
        let file = temp.write("drive/sub/a.txt", "a");
        let bin = topdir_trash(&top, 4242).unwrap();
        assert_eq!(bin, top.join(".Trash-4242"));
        assert_eq!(fs::metadata(&bin).unwrap().mode() & 0o777, 0o700);
        let item = trash_into(&file, &bin, Some(&top)).unwrap();
        assert!(
            info_of(&item).contains("\nPath=sub/a.txt\n"),
            "{}",
            info_of(&item)
        );
        untrash(&[item]).unwrap();
        assert!(file.exists(), "restoring uses the absolute original");
    }

    #[test]
    fn a_sticky_shared_bin_is_preferred() {
        use std::os::unix::fs::PermissionsExt;
        let temp = Temp::new("trash-sticky");
        let top = temp.dir("drive");
        let shared = top.join(".Trash");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(topdir_trash(&top, 4242).unwrap(), shared.join("4242"));
        // Without the sticky bit it is not trusted.
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(topdir_trash(&top, 4242).unwrap(), top.join(".Trash-4242"));
    }

    #[test]
    fn topdir_stops_at_the_drive() {
        let temp = Temp::new("trash-topdir");
        let file = temp.write("a/b/c.txt", "c");
        let dev = fs::symlink_metadata(&file).unwrap().dev();
        let top = topdir(&file, dev).unwrap();
        assert!(file.starts_with(&top));
        assert_eq!(fs::metadata(&top).unwrap().dev(), dev);
        if let Some(parent) = top.parent() {
            assert_ne!(fs::metadata(parent).unwrap().dev(), dev, "{top:?}");
        }
    }

    /// A mount table from `(mount point, file system, read-only)` rows.
    fn table(rows: &[(&str, &str, bool)]) -> Vec<Mount> {
        let text: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(index, (point, fs_type, read_only))| {
                let access = if *read_only { "ro" } else { "rw" };
                format!(
                    "{id} 1 0:{id} / {point} {access} - {fs_type} /dev/disk{id} rw",
                    id = index + 10
                )
            })
            .collect();
        mounts::parse_mountinfo(&text.join("\n"))
    }

    #[test]
    fn can_trash_reads_the_mount_kind() {
        let temp = Temp::new("can-trash");
        let nfs_top = temp.dir("net");
        let table = table(&[
            ("/", "ext4", false),
            ("/media/usb", "vfat", false),
            ("/media/cd", "iso9660", true),
            (nfs_top.to_str().unwrap(), "nfs4", false),
        ]);
        let home = Path::new("/home/me/.local/share/Trash");
        let ok = |path: &str| can_trash_with(Path::new(path), &table, Some(home), 4242);
        assert!(ok("/home/me/a.txt"), "the home drive");
        assert!(ok("/media/usb/a.txt"), "a local drive gets its own bin");
        assert!(!ok("/media/cd/a.txt"), "read-only");
        assert!(!ok("/"), "the root");
        assert!(!ok("relative/a.txt"));
        assert!(
            !ok("/home/me/.local/share/Trash/files/a.txt"),
            "already binned"
        );
        assert!(!ok("/media/usb/.Trash-1000/files/a.txt"));

        let net_file = nfs_top.join("a.txt");
        assert!(
            !can_trash_with(&net_file, &table, Some(home), 4242),
            "a network drive without a bin"
        );
        fs::create_dir(nfs_top.join(".Trash-4242")).unwrap();
        assert!(can_trash_with(&net_file, &table, Some(home), 4242));
    }

    #[test]
    fn a_home_on_the_network_can_still_trash() {
        let table = table(&[("/", "ext4", false), ("/home", "nfs4", false)]);
        let home = Path::new("/home/me/.local/share/Trash");
        assert!(can_trash_with(
            Path::new("/home/me/a.txt"),
            &table,
            Some(home),
            1
        ));
    }

    #[test]
    fn encoded_paths() {
        assert_eq!(encode_path(Path::new("/a b/c")), "/a%20b/c");
        assert_eq!(encode_path(Path::new("rel/x y")), "rel/x%20y");
    }
}

#[cfg(test)]
mod empty_tests {
    use super::super::testing::Temp;
    use super::*;

    fn mounts_at(points: &[&Path]) -> Vec<Mount> {
        let text: Vec<String> = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                format!(
                    "{id} 1 0:{id} / {point} rw - ext4 /dev/disk{id} rw",
                    id = index + 10,
                    point = point.display()
                )
            })
            .collect();
        mounts::parse_mountinfo(&text.join("\n"))
    }

    #[test]
    fn the_bins_that_exist_are_found_on_every_drive() {
        let temp = Temp::new("bins");
        let home = temp.dir("home/Trash");
        temp.dir("home/Trash/files");
        temp.dir("usb/.Trash-1000/files");
        temp.dir("other/.Trash-1000/info"); // no files folder: not a bin
        // A shared bin that is not sticky is not used.
        temp.dir("shared/.Trash/1000/files");
        let table = mounts_at(&[&temp.path().join("usb"), &temp.path().join("other"), &temp.path().join("shared")]);
        let bins = existing_bins(&table, Some(&home), 1000);
        assert_eq!(bins, [home, temp.path().join("usb/.Trash-1000")]);
    }

    #[test]
    fn emptying_deletes_files_and_info_in_every_bin_and_nothing_else() {
        let temp = Temp::new("empty");
        let bin_a = temp.dir("a/Trash");
        let bin_b = temp.dir("b/.Trash-1000");
        for bin in [&bin_a, &bin_b] {
            temp.write(&format!("{}/files/x.txt", bin.strip_prefix(temp.path()).unwrap().display()), "x");
            temp.write(&format!("{}/files/dir/y.txt", bin.strip_prefix(temp.path()).unwrap().display()), "y");
            temp.write(&format!("{}/info/x.txt.trashinfo", bin.strip_prefix(temp.path()).unwrap().display()), "i");
        }
        let outside = temp.write("keep/me.txt", "keep");
        let progress = Progress::new();
        empty_bins(&[bin_a.clone(), bin_b.clone()], &progress, &Cancel::new()).unwrap();
        for bin in [&bin_a, &bin_b] {
            assert_eq!(fs::read_dir(bin.join("files")).unwrap().count(), 0);
            assert_eq!(fs::read_dir(bin.join("info")).unwrap().count(), 0);
        }
        assert!(bin_a.join("files").is_dir(), "the bin itself stays");
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
        assert_eq!(progress.files_done(), 8);
    }

    #[test]
    fn emptying_an_empty_bin_is_not_an_error_and_cancel_is_obeyed() {
        let temp = Temp::new("empty-none");
        let bin = temp.dir("Trash");
        temp.dir("Trash/files");
        empty_bins(&[bin.clone()], &Progress::new(), &Cancel::new()).unwrap();
        temp.write("Trash/files/a.txt", "a");
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(
            empty_bins(&[bin.clone()], &Progress::new(), &cancel),
            Err(OpError::Cancelled)
        );
        assert!(bin.join("files/a.txt").exists());
    }

    #[test]
    fn a_path_in_a_bin_is_known_by_its_name() {
        assert!(in_a_bin(Path::new("/home/a/.local/share/Trash/files/x"), Some(Path::new("/home/a/.local/share/Trash"))));
        assert!(in_a_bin(Path::new("/media/usb/.Trash-1000/files/x"), None));
        assert!(!in_a_bin(Path::new("/home/a/Documents/x"), Some(Path::new("/home/a/.local/share/Trash"))));
    }
}
