//! The bin, following the freedesktop.org Trash specification 1.0.
//!
//! Files go to `$XDG_DATA_HOME/Trash` when they are on the same drive as it.
//! On any other drive they go to that drive's own bin: `$topdir/.Trash/$uid`
//! when the admin made a sticky `.Trash` (not a link), else `$topdir/.Trash-$uid`
//! (ours, mode 0700). Each bin has `files`, `info` (one `.trashinfo` per item:
//! `Path` and `DeletionDate` in local time) and `directorysizes` (the disk use
//! of the folders in `files`).
//!
//! The decision is made from the mount table ([`plan_bin`]), so it can be
//! tested with a made-up one. A file is never copied to another drive to bin
//! it, except from the home drive's own mount points when a `rename(2)` there
//! says `EXDEV` (a bind mount, a btrfs subvolume): that copy is made only for
//! small trees, and a big one is answered with [`OpError::CopyNeeded`] so a
//! person decides ([`trash_copying`]). Nothing is ever deleted instead: what
//! cannot be binned is refused with a [`TrashRefusal`] that says why.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::listing::Cancel;
use crate::mounts::{self, Mount, MountKind};
use crate::places::{SpecialFolder, UserDirs, system_user_dirs};

use super::durable::{sync_dirs, sync_written};
use super::transfer::{ConflictPolicy, move_to};
use super::trash_info::{
    disk_use, forget_sizes, info_mtime, info_path, info_text, is_bin, record_size, unique_name,
};
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

/// The most a tree may weigh to be copied into the home bin without asking,
/// in bytes.
const COPY_LIMIT: u64 = 64 << 20;

/// The most entries of a folder the `directorysizes` cache walks to fill a
/// line. A bigger folder is simply left out of the cache (it is only a cache).
const SIZE_WALK_LIMIT: u64 = 50_000;

/// Whether a `rename(2)` that cannot cross drives may fall back to a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Copying {
    /// Only for a small tree: a big one is [`OpError::CopyNeeded`].
    Small,
    /// Whatever the size: a person agreed.
    Any,
}

/// Why something cannot go to the bin. Each reads as a sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrashRefusal {
    NotAbsolute,
    /// `/`.
    Root,
    /// The bin, its `files` or `info` folder, or the shared `.Trash` folder.
    TrashItself,
    /// Inside a bin already.
    InTrash,
    /// The home folder.
    Home,
    /// A folder that has the home folder inside it.
    HoldsHome,
    /// A folder that has the home bin inside it.
    HoldsTrash,
    /// One of the user's folders (Documents, Pictures...), by name.
    UserFolder(&'static str),
    /// The drive is mounted read-only.
    ReadOnly,
    /// The drive has no bin and none can be made there (a share that has no
    /// `.Trash-$uid` yet).
    NoBin,
}

impl TrashRefusal {
    /// The reason as a sentence about one item, or about several (`plural`).
    pub fn reason(&self, plural: bool) -> String {
        let (it, is) = if plural {
            ("They", "are")
        } else {
            ("It", "is")
        };
        let cannot = if plural {
            "they cannot be moved to the Trash"
        } else {
            "it cannot be moved to the Trash"
        };
        match self {
            TrashRefusal::NotAbsolute => "The path must be absolute.".into(),
            TrashRefusal::Root => "The root folder cannot be moved to the Trash.".into(),
            TrashRefusal::TrashItself => "The Trash itself cannot be moved to the Trash.".into(),
            TrashRefusal::InTrash => format!("{it} {is} in the Trash already."),
            TrashRefusal::Home => "Your home folder cannot be moved to the Trash.".into(),
            TrashRefusal::HoldsHome => format!("{it} holds your home folder, so {cannot}."),
            TrashRefusal::HoldsTrash => format!("{it} holds the Trash, so {cannot}."),
            TrashRefusal::UserFolder(name) => {
                format!("{it} {is} your {name} folder, so {cannot}.")
            }
            TrashRefusal::ReadOnly => {
                format!("{it} {is} on a read-only drive, so {cannot}.")
            }
            TrashRefusal::NoBin => format!(
                "{} has no Trash and one cannot be made there, so {cannot}.",
                if plural { "Their drive" } else { "Its drive" }
            ),
        }
    }

    /// Whether Delete permanently (after a question) is a fair thing to offer
    /// instead. It is not for the folders the desktop stands on.
    pub fn offers_delete(&self) -> bool {
        matches!(
            self,
            TrashRefusal::InTrash | TrashRefusal::ReadOnly | TrashRefusal::NoBin
        )
    }
}

impl std::fmt::Display for TrashRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason(false))
    }
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

/// The home bin: `$XDG_DATA_HOME/Trash`.
pub(crate) fn home_trash() -> Option<PathBuf> {
    data_home().map(|dir| dir.join("Trash"))
}

/// Everything a decision about a bin depends on, so a test can make it up:
/// the home folder and its bin, the user, the mount table and the user's
/// folders.
pub(super) struct Context {
    pub(super) home: PathBuf,
    pub(super) home_trash: PathBuf,
    pub(super) uid: u32,
    pub(super) table: Vec<Mount>,
    pub(super) dirs: Arc<UserDirs>,
}

impl Context {
    pub(super) fn system() -> Option<Context> {
        let home_trash = home_trash()?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        Some(Context {
            home,
            home_trash,
            uid: uid(),
            table: mounts::all_mounts(),
            dirs: system_user_dirs(),
        })
    }
}

fn no_home() -> OpError {
    OpError::Other("Your home folder could not be found.".into())
}

/// Is `path` already inside a bin?
pub(super) fn in_a_bin(path: &Path, home_trash: Option<&Path>) -> bool {
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

/// Is `path` a bin or one of the folders a bin is made of?
fn is_bin_part(path: &Path, home_trash: &Path) -> bool {
    if is_bin(path, Some(home_trash)) || path.file_name() == Some(OsStr::new(".Trash")) {
        return true;
    }
    let part = path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| matches!(name, "files" | "info" | "directorysizes"));
    part && path
        .parent()
        .is_some_and(|dir| is_bin(dir, Some(home_trash)))
}

fn folder_name(kind: SpecialFolder) -> Option<&'static str> {
    Some(match kind {
        SpecialFolder::Desktop => "Desktop",
        SpecialFolder::Documents => "Documents",
        SpecialFolder::Downloads => "Downloads",
        SpecialFolder::Music => "Music",
        SpecialFolder::Pictures => "Pictures",
        SpecialFolder::Videos => "Videos",
        SpecialFolder::Templates => "Templates",
        SpecialFolder::PublicShare => "Public",
        SpecialFolder::Home | SpecialFolder::Trash | SpecialFolder::Root => return None,
    })
}

/// What the folders the desktop stands on refuse, from the path's text alone.
fn protection(path: &Path, ctx: &Context) -> Option<TrashRefusal> {
    if !path.is_absolute() {
        return Some(TrashRefusal::NotAbsolute);
    }
    if path.parent().is_none() {
        return Some(TrashRefusal::Root);
    }
    if is_bin_part(path, &ctx.home_trash) {
        return Some(TrashRefusal::TrashItself);
    }
    if in_a_bin(path, Some(&ctx.home_trash)) {
        return Some(TrashRefusal::InTrash);
    }
    match ctx.dirs.special(path) {
        Some(SpecialFolder::Home) => return Some(TrashRefusal::Home),
        Some(SpecialFolder::Root) => return Some(TrashRefusal::Root),
        Some(SpecialFolder::Trash) => return Some(TrashRefusal::TrashItself),
        Some(kind) => {
            if let Some(name) = folder_name(kind) {
                return Some(TrashRefusal::UserFolder(name));
            }
        }
        None => {}
    }
    if ctx.home.starts_with(path) {
        return Some(TrashRefusal::HoldsHome);
    }
    if ctx.home_trash.starts_with(path) {
        return Some(TrashRefusal::HoldsTrash);
    }
    None
}

/// Where an item goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BinPlan {
    /// The home bin: the item is on the same drive.
    Home,
    /// The bin of the drive mounted at this folder.
    Drive(PathBuf),
    /// The mount table does not know the path: decide by device numbers.
    ByDevice,
}

/// Is there a bin of ours at the top of the drive mounted at `top`?
fn has_drive_bin(top: &Path, uid: u32) -> bool {
    top.join(format!(".Trash-{uid}")).is_dir() || top.join(".Trash").join(uid.to_string()).is_dir()
}

/// Which bin `path` goes to, from the mount table: the home bin for anything
/// on the home bin's drive, else the bin at the top of its own drive. A
/// read-only drive and a share with no bin yet are refused. Reads nothing
/// but the table (and, for a share, whether a bin exists).
pub(super) fn plan_bin(path: &Path, ctx: &Context) -> Result<BinPlan, TrashRefusal> {
    let Some(mount) = mounts::mount_for(path, &ctx.table) else {
        return Ok(BinPlan::ByDevice);
    };
    if mount.read_only {
        return Err(TrashRefusal::ReadOnly);
    }
    let home_mount = mounts::mount_for(&ctx.home_trash, &ctx.table);
    if home_mount.is_some_and(|home| home.mount_point == mount.mount_point) {
        return Ok(BinPlan::Home);
    }
    if mount.kind == MountKind::Network && !has_drive_bin(&mount.mount_point, ctx.uid) {
        return Err(TrashRefusal::NoBin);
    }
    Ok(BinPlan::Drive(mount.mount_point.clone()))
}

fn decide(path: &Path, ctx: &Context) -> Result<BinPlan, TrashRefusal> {
    match protection(path, ctx) {
        Some(refusal) => Err(refusal),
        None => plan_bin(path, ctx),
    }
}

pub(super) fn check_trash_in(path: &Path, ctx: &Context) -> Result<(), TrashRefusal> {
    decide(path, ctx).map(|_| ())
}

/// Can `path` go to the bin, and if not, why not? Cheap: reads the mount
/// table (cached for two seconds) and, for a network drive, looks for a bin
/// that is already there. It never touches `path` itself, so it is safe to
/// ask for every row.
///
/// Refused: the root, the home folder and the user's folders (Documents and
/// the like), a folder that holds the home folder or the Trash, the Trash
/// itself, anything in the Trash already, anything on a read-only drive, and
/// anything on a network drive that has no bin of ours.
pub fn check_trash(path: &Path) -> Result<(), TrashRefusal> {
    match Context::system() {
        Some(ctx) => check_trash_in(path, &ctx),
        None => Ok(()),
    }
}

/// [`check_trash`] as a yes or no.
pub fn can_trash(path: &Path) -> bool {
    check_trash(path).is_ok()
}

/// Is `path` inside a bin: the one in your home folder, or one at the top of
/// a drive? Cheap: it only reads the name.
pub fn in_trash(path: &Path) -> bool {
    in_a_bin(path, home_trash().as_deref())
}

/// Every bin that exists: yours at home, and the one for this user at the top
/// of each writable drive.
pub(super) fn existing_bins(table: &[Mount], home: Option<&Path>, uid: u32) -> Vec<PathBuf> {
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

/// Move items to the bin. All or nothing: if one fails, the ones already
/// moved are put back and the error is returned.
///
/// `paths` must be absolute. Links are binned as links, never followed. An
/// item that cannot go is refused with [`OpError::CannotTrash`] and nothing is
/// moved. On the home drive's own mount points a `rename(2)` that cannot cross
/// into the bin falls back to a copy, but only for a small tree: a big one is
/// [`OpError::CopyNeeded`] (see [`trash_copying`]).
pub fn trash(paths: &[PathBuf]) -> Result<Vec<Trashed>, OpError> {
    let ctx = Context::system().ok_or_else(no_home)?;
    trash_in(&ctx, paths, Copying::Small)
}

/// [`trash`] that copies whatever the size when a `rename(2)` cannot do it:
/// what a person agreed to after [`OpError::CopyNeeded`].
pub fn trash_copying(paths: &[PathBuf]) -> Result<Vec<Trashed>, OpError> {
    let ctx = Context::system().ok_or_else(no_home)?;
    trash_in(&ctx, paths, Copying::Any)
}

pub(super) fn trash_in(
    ctx: &Context,
    paths: &[PathBuf],
    copying: Copying,
) -> Result<Vec<Trashed>, OpError> {
    // Refuse everything that cannot go before anything moves.
    for path in paths {
        decide(path, ctx).map_err(OpError::CannotTrash)?;
    }
    let mut done = Vec::with_capacity(paths.len());
    for path in paths {
        match trash_one(ctx, path, copying) {
            Ok(item) => done.push(item),
            Err(error) => {
                let _ = untrash(&done);
                return Err(error);
            }
        }
    }
    Ok(done)
}

fn trash_one(ctx: &Context, path: &Path, copying: Copying) -> Result<Trashed, OpError> {
    let plan = decide(path, ctx).map_err(OpError::CannotTrash)?;
    let meta = path.symlink_metadata()?;
    let (bin, top) = match plan {
        BinPlan::Home => (ctx.home_trash.clone(), None),
        BinPlan::Drive(top) => (topdir_trash(&top, ctx.uid)?, Some(top)),
        BinPlan::ByDevice => {
            home_dirs(&ctx.home_trash)?;
            if fs::metadata(ctx.home_trash.join("files"))?.dev() == meta.dev() {
                (ctx.home_trash.clone(), None)
            } else {
                let top = topdir(path, meta.dev())?;
                (topdir_trash(&top, ctx.uid)?, Some(top))
            }
        }
    };
    if top.is_none() {
        home_dirs(&bin)?;
    }
    let bin_dev = fs::metadata(bin.join("files"))?.dev();
    let placing = match (bin_dev == meta.dev(), &top) {
        (true, None) => Placing::Rename(Some(copying)),
        (true, Some(_)) => Placing::Rename(None),
        (false, None) => Placing::Copy(copying),
        // A drive's own bin is on another device: the drive has no usable bin.
        (false, Some(_)) => return Err(OpError::CannotTrash(TrashRefusal::NoBin)),
    };
    trash_into(path, &bin, top.as_deref(), placing)
}

/// How an item gets into the bin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Placing {
    /// One `rename(2)`. When it says `EXDEV` and a fallback is allowed, copy.
    Rename(Option<Copying>),
    /// Copy, then delete the original, for an item on another device than the
    /// bin.
    Copy(Copying),
}

/// The home bin, made when it is missing. A failure is the person's problem
/// to hear about, not a drive without a bin.
fn home_dirs(bin: &Path) -> Result<(), OpError> {
    make_trash_dirs(bin).map_err(|error| {
        OpError::Other(format!(
            "The Trash folder {} could not be made: {}",
            bin.display(),
            super::plain(&error)
        ))
    })
}

/// The mount point of the drive `path` is on: the highest ancestor with the
/// same device number.
fn topdir(path: &Path, dev: u64) -> Result<PathBuf, OpError> {
    let mut top = path.parent().map(Path::to_path_buf).unwrap_or_default();
    if fs::metadata(&top)?.dev() != dev {
        return Err(OpError::CannotTrash(TrashRefusal::NoBin));
    }
    while let Some(parent) = top.parent() {
        if fs::metadata(parent)?.dev() != dev {
            break;
        }
        top = parent.to_path_buf();
    }
    Ok(top)
}

/// The bin to use on the drive mounted at `top`: `.Trash/$uid` when a sticky
/// `.Trash` that is not a link is there, else `.Trash-$uid`, made with mode
/// 0700. A bin that is a link, or is not ours, is not used.
pub(super) fn topdir_trash(top: &Path, uid: u32) -> Result<PathBuf, OpError> {
    let no_bin = || OpError::CannotTrash(TrashRefusal::NoBin);
    let shared = top.join(".Trash");
    if let Ok(meta) = shared.symlink_metadata()
        && meta.is_dir()
        && meta.mode() & 0o1000 != 0
    {
        let own = shared.join(uid.to_string());
        if own_dir_ok(&own, uid) && make_trash_dirs(&own).is_ok() {
            return Ok(own);
        }
    }
    let own = top.join(format!(".Trash-{uid}"));
    if !own_dir_ok(&own, uid) {
        return Err(no_bin());
    }
    make_trash_dirs(&own).map_err(|_| no_bin())?;
    Ok(own)
}

/// Is `dir` missing, or a real folder (not a link) that belongs to `uid`?
fn own_dir_ok(dir: &Path, uid: u32) -> bool {
    match dir.symlink_metadata() {
        Ok(meta) => meta.is_dir() && meta.uid() == uid,
        Err(error) => error.kind() == io::ErrorKind::NotFound,
    }
}

/// `dir`, `dir/files` and `dir/info`, mode 0700 where made.
fn make_trash_dirs(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(dir.join("files"))?;
    builder.create(dir.join("info"))?;
    Ok(())
}

/// Does the tree at `path` weigh more than `limit` bytes? Stops as soon as it
/// is sure; every entry counts for at least a kibibyte, so a tree of a million
/// empty files is big too.
fn weighs_more_than(path: &Path, limit: u64) -> bool {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(meta) = next.symlink_metadata() else {
            continue;
        };
        total += meta.len().max(1024);
        if total > limit {
            return true;
        }
        if meta.is_dir()
            && let Ok(children) = fs::read_dir(&next)
        {
            pending.extend(children.flatten().map(|child| child.path()));
        }
    }
    false
}

/// Copy `src` into the bin as `in_trash` and remove it: for an item on a
/// drive the bin is not on. The copy is made in a folder of its own inside
/// the bin and renamed into `files` when whole, so `files` never holds half
/// of one.
fn copy_into(src: &Path, bin: &Path, in_trash: &Path, copying: Copying) -> Result<(), OpError> {
    if copying == Copying::Small && weighs_more_than(src, COPY_LIMIT) {
        return Err(OpError::CopyNeeded);
    }
    let staging = bin.join(format!(".moving-{}", std::process::id()));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(&staging)?;
    let landed = match move_to(
        &[src.to_path_buf()],
        &staging,
        &ConflictPolicy::KeepBoth,
        &Progress::new(),
        &Cancel::new(),
    ) {
        Ok(landed) => landed,
        Err(error) => {
            // The original is still where it was: only the copy goes.
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    };
    let Some(landed) = landed.first() else {
        let _ = fs::remove_dir(&staging);
        return Err(OpError::Other(
            "It could not be copied into the Trash.".into(),
        ));
    };
    // The original is gone now, so a failure here leaves the only copy in the
    // staging folder and says so.
    rename_noreplace(landed, in_trash).map_err(|error| {
        OpError::Other(format!(
            "It was copied to {} but could not be put in the Trash: {}",
            staging.display(),
            super::plain(&error)
        ))
    })?;
    let _ = fs::remove_dir(&staging);
    Ok(())
}

/// What a failed `rename(2)` into the bin means.
fn trash_io_error(error: io::Error) -> OpError {
    match error.raw_os_error() {
        Some(libc::EROFS) => OpError::CannotTrash(TrashRefusal::ReadOnly),
        _ => OpError::from(error),
    }
}

/// Move `src` into the bin at `bin` (which holds `files` and `info`).
/// `top` is set for a drive's bin: `Path=` is then relative to it.
///
/// The `.trashinfo` is made first with `O_EXCL`, which reserves the name.
/// On removable media, shares and FAT the info file, the folders and the
/// drive are made durable before this returns.
pub(super) fn trash_into(
    src: &Path,
    bin: &Path,
    top: Option<&Path>,
    placing: Placing,
) -> Result<Trashed, OpError> {
    make_trash_dirs(bin)?;
    let name = src
        .file_name()
        .ok_or_else(|| OpError::Other("That cannot be moved to the Trash.".into()))?
        .to_os_string();
    let was_dir = src.symlink_metadata().is_ok_and(|meta| meta.is_dir());
    let recorded = match top {
        Some(top) => src.strip_prefix(top).unwrap_or(src),
        None => src,
    };
    let text = info_text(recorded, SystemTime::now());
    let mut attempt = 1u32;
    loop {
        let unique = unique_name(&name, attempt);
        attempt += 1;
        let info = info_path(bin, &unique);
        let in_trash = bin.join("files").join(&unique);
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        if in_trash.symlink_metadata().is_ok() {
            // A file with no info (left by another program): leave it alone.
            drop(file);
            let _ = fs::remove_file(&info);
            continue;
        }
        let written = file.write_all(text.as_bytes());
        drop(file);
        let placed = written.map_err(OpError::from).and_then(|()| match placing {
            Placing::Rename(fallback) => match rename_noreplace(src, &in_trash) {
                Err(error) if error.raw_os_error() == Some(libc::EXDEV) && fallback.is_some() => {
                    copy_into(src, bin, &in_trash, fallback.unwrap_or(Copying::Small))
                }
                other => other.map_err(trash_io_error),
            },
            Placing::Copy(copying) => copy_into(src, bin, &in_trash, copying),
        });
        return match placed {
            Ok(()) => {
                // One `rename(2)` did the move; on removable media, shares and
                // FAT the info file and the three folders it touched are made
                // durable too.
                sync_written(&info)?;
                sync_dirs(&[
                    src.parent().unwrap_or(Path::new("/")),
                    &bin.join("files"),
                    &bin.join("info"),
                ])?;
                if was_dir {
                    remember_size(bin, &unique, &in_trash);
                }
                Ok(Trashed {
                    original: src.to_path_buf(),
                    in_trash,
                    info,
                })
            }
            Err(error) => {
                let _ = fs::remove_file(&info);
                Err(error)
            }
        };
    }
}

/// Add the folder just binned to the bin's `directorysizes`, when it is cheap
/// to measure. The file is a cache: a failure or a huge folder only means the
/// line is not there yet.
fn remember_size(bin: &Path, name: &OsStr, in_trash: &Path) {
    let Some(size) = disk_use(in_trash, &Cancel::new(), Some(SIZE_WALK_LIMIT)) else {
        return;
    };
    if let Some(mtime) = info_mtime(bin, name) {
        let _ = record_size(bin, name, size, mtime);
    }
}

/// Put items back where they were and remove their `.trashinfo`. Makes
/// missing parent folders. Refuses to replace anything: if any original path
/// is taken the answer is [`OpError::Exists`] and nothing has moved. This is
/// what Undo of a move to the Trash does; [`super::restore`] is what Restore
/// does, and asks.
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
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
        if let Some(bin) = item.info.parent().and_then(Path::parent)
            && let Some(name) = item.in_trash.file_name()
        {
            forget_sizes(bin, &[name]);
        }
    }
    Ok(restored)
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::testing::Temp;
    use super::super::trash_info::parse_info;
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    /// A made-up system: a home folder inside `temp`, its bin, and a mount
    /// table with `/` plus the given `(folder under temp, file system,
    /// read-only)` rows.
    pub(crate) fn context(temp: &Temp, rows: &[(&str, &str, bool)]) -> Context {
        let home = temp.dir("home");
        let mut lines = vec!["1 0 0:1 / / rw - ext4 /dev/disk1 rw".to_string()];
        for (index, (folder, fs_type, read_only)) in rows.iter().enumerate() {
            let point = temp.dir(folder);
            let access = if *read_only { "ro" } else { "rw" };
            lines.push(format!(
                "{id} 1 0:{id} / {point} {access} - {fs_type} /dev/disk{id} rw",
                id = index + 10,
                point = point.display()
            ));
        }
        Context {
            home_trash: home.join(".local/share/Trash"),
            dirs: Arc::new(UserDirs::parse(None, &home)),
            home,
            uid: 4242,
            table: mounts::parse_mountinfo(&lines.join("\n")),
        }
    }

    fn info_of(item: &Trashed) -> String {
        fs::read_to_string(&item.info).unwrap()
    }

    fn rename_into(src: &Path, bin: &Path) -> Trashed {
        trash_into(src, bin, None, Placing::Rename(None)).unwrap()
    }

    #[test]
    fn round_trip_through_a_bin() {
        let temp = Temp::new("trash");
        let bin = temp.path().join("Trash");
        let file = temp.write("docs/note.txt", "hello");
        let item = rename_into(&file, &bin);
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
        let item = rename_into(&file, &bin);
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
    fn the_deletion_date_is_the_local_time_now() {
        let temp = Temp::new("trash-date");
        let bin = temp.path().join("Trash");
        let file = temp.write("a.txt", "x");
        let before = SystemTime::now();
        let item = rename_into(&file, &bin);
        let info = parse_info(&fs::read(&item.info).unwrap()).unwrap();
        let deleted = info.deleted.unwrap();
        let slack = std::time::Duration::from_secs(2);
        assert!(deleted + slack >= before, "{deleted:?} vs {before:?}");
        assert!(deleted <= SystemTime::now() + slack);
    }

    #[test]
    fn names_that_clash_get_a_number() {
        let temp = Temp::new("trash-clash");
        let bin = temp.path().join("Trash");
        let first = temp.write("one/a.txt", "1");
        let second = temp.write("two/a.txt", "2");
        let third = temp.write("three/a.txt", "3");
        let a = rename_into(&first, &bin);
        let b = rename_into(&second, &bin);
        let c = rename_into(&third, &bin);
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
    fn a_name_with_no_extension_numbers_after_the_name() {
        let temp = Temp::new("trash-clash-bare");
        let bin = temp.path().join("Trash");
        let one = rename_into(&temp.write("x/report", "1"), &bin);
        let two = rename_into(&temp.write("y/report", "2"), &bin);
        let tar = rename_into(&temp.write("x/data.tar.gz", "t"), &bin);
        let tar2 = rename_into(&temp.write("y/data.tar.gz", "t"), &bin);
        assert_eq!(one.in_trash.file_name().unwrap(), "report");
        assert_eq!(two.in_trash.file_name().unwrap(), "report.2");
        assert_eq!(tar.in_trash.file_name().unwrap(), "data.tar.gz");
        assert_eq!(tar2.in_trash.file_name().unwrap(), "data.2.tar.gz");
    }

    #[test]
    fn an_orphan_in_files_is_not_overwritten() {
        let temp = Temp::new("trash-orphan");
        let bin = temp.path().join("Trash");
        fs::create_dir_all(bin.join("files")).unwrap();
        fs::write(bin.join("files/a.txt"), "orphan").unwrap();
        let file = temp.write("a.txt", "new");
        let item = rename_into(&file, &bin);
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
        let dangling = temp.path().join("dangling");
        std::os::unix::fs::symlink(temp.path().join("nowhere"), &dangling).unwrap();
        let dir_item = rename_into(&dir, &bin);
        let link_item = rename_into(&link, &bin);
        let dangling_item = rename_into(&dangling, &bin);
        assert!(dir_item.in_trash.join("inner.txt").exists());
        assert!(link_item.in_trash.symlink_metadata().unwrap().is_symlink());
        assert!(target.exists(), "the target of a link stays");
        untrash(&[dir_item, link_item, dangling_item]).unwrap();
        assert!(dir.join("inner.txt").exists());
        assert_eq!(fs::read_link(&link).unwrap(), target);
        assert_eq!(
            fs::read_link(&dangling).unwrap(),
            temp.path().join("nowhere")
        );
    }

    #[test]
    fn odd_names_survive_the_round_trip_byte_for_byte() {
        let temp = Temp::new("trash-odd");
        let bin = temp.path().join("Trash");
        let names: Vec<OsString> = vec![
            "sp ace %41 100%.txt".into(),
            "new\nline".into(),
            "ünï–☃.txt".into(),
            ".hidden".into(),
            OsString::from_vec(b"caf\xe9 \xff.txt".to_vec()),
            OsString::from_vec(b"\xc3\x28-broken".to_vec()),
        ];
        for name in names {
            let file = temp.path().join("src").join(&name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, "x").unwrap();
            let item = rename_into(&file, &bin);
            assert_eq!(
                item.in_trash.file_name().unwrap(),
                name,
                "{name:?} keeps its bytes in the bin"
            );
            let info = parse_info(&fs::read(&item.info).unwrap()).unwrap();
            assert_eq!(info.path, file, "{name:?} is recorded exactly");
            assert!(
                info_of_bytes(&item).is_ascii(),
                "the info file is plain ASCII"
            );
            assert_eq!(untrash(&[item]).unwrap(), vec![file.clone()]);
            assert!(file.exists(), "{name:?} is back");
        }
    }

    fn info_of_bytes(item: &Trashed) -> Vec<u8> {
        fs::read(&item.info).unwrap()
    }

    #[test]
    fn a_very_long_name_still_gets_an_info_file() {
        let temp = Temp::new("trash-long");
        let bin = temp.path().join("Trash");
        let long = format!("{}.txt", "n".repeat(250));
        let file = temp.write(&format!("d/{long}"), "x");
        let first = rename_into(&file, &bin);
        let info = parse_info(&fs::read(&first.info).unwrap()).unwrap();
        assert_eq!(info.path, file, "the real name is in the info file");
        assert!(first.info.file_name().unwrap().len() <= 255);
        assert!(first.in_trash.file_name().unwrap().len() <= 255);
        untrash(&[first]).unwrap();
        assert!(file.exists());
    }

    #[test]
    fn untrash_refuses_to_overwrite_and_moves_nothing() {
        let temp = Temp::new("trash-refuse");
        let bin = temp.path().join("Trash");
        let a = temp.write("a.txt", "a");
        let b = temp.write("b.txt", "b");
        let item_a = rename_into(&a, &bin);
        let item_b = rename_into(&b, &bin);
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
        let item = rename_into(&file, &bin);
        fs::remove_dir_all(temp.path().join("deep")).unwrap();
        untrash(&[item]).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "a");
    }

    #[test]
    fn bins_are_private() {
        let temp = Temp::new("trash-mode");
        let bin = temp.path().join("Trash");
        let file = temp.write("a.txt", "a");
        rename_into(&file, &bin);
        for dir in [bin.clone(), bin.join("files"), bin.join("info")] {
            assert_eq!(fs::metadata(dir).unwrap().mode() & 0o777, 0o700);
        }
    }

    // ---- directorysizes ---------------------------------------------------

    fn sizes_text(bin: &Path) -> String {
        fs::read_to_string(bin.join("directorysizes")).unwrap_or_default()
    }

    #[test]
    fn trashing_a_folder_records_its_size_and_a_file_does_not() {
        let temp = Temp::new("trash-sizes");
        let bin = temp.path().join("Trash");
        let dir = temp.dir("My Folder");
        temp.write("My Folder/a.bin", &"x".repeat(10_000));
        let file = temp.write("plain.txt", "p");
        let folder_item = rename_into(&dir, &bin);
        rename_into(&file, &bin);
        let text = sizes_text(&bin);
        let mut lines = text.lines();
        let line = lines.next().expect("one line for the folder");
        assert_eq!(lines.next(), None, "none for the file: {text}");
        let mut parts = line.splitn(3, ' ');
        let size: u64 = parts.next().unwrap().parse().unwrap();
        let mtime: i64 = parts.next().unwrap().parse().unwrap();
        assert_eq!(parts.next(), Some("My%20Folder"));
        assert!(size >= 10_000, "the blocks of the file are counted: {size}");
        assert_eq!(
            mtime,
            fs::metadata(&folder_item.info).unwrap().mtime(),
            "the time of the info file, not of the folder"
        );
        assert_eq!(
            super::super::trash_info::cached_dir_size(&bin, OsStr::new("My Folder")),
            Some(size)
        );
        // The temporary file the update was written through is gone.
        let leftovers: Vec<_> = fs::read_dir(&bin)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn two_folders_with_one_name_have_two_lines_and_restoring_removes_its_line() {
        let temp = Temp::new("trash-sizes-two");
        let bin = temp.path().join("Trash");
        let one = temp.dir("a/proj");
        temp.write("a/proj/x", "1");
        let two = temp.dir("b/proj");
        temp.write("b/proj/y", "2");
        let first = rename_into(&one, &bin);
        let second = rename_into(&two, &bin);
        assert_eq!(second.in_trash.file_name().unwrap(), "proj.2");
        let names: Vec<String> = sizes_text(&bin)
            .lines()
            .map(|line| line.rsplit(' ').next().unwrap().to_string())
            .collect();
        assert_eq!(names, ["proj", "proj.2"]);
        untrash(&[first]).unwrap();
        let names: Vec<String> = sizes_text(&bin)
            .lines()
            .map(|line| line.rsplit(' ').next().unwrap().to_string())
            .collect();
        assert_eq!(names, ["proj.2"], "the restored folder left the cache");
        untrash(&[second]).unwrap();
        assert!(
            !bin.join("directorysizes").exists(),
            "an empty cache is no file"
        );
    }

    #[test]
    fn a_folder_name_with_a_newline_is_escaped_in_the_cache() {
        let temp = Temp::new("trash-sizes-nl");
        let bin = temp.path().join("Trash");
        let dir = temp.dir("two\nlines");
        temp.write("two\nlines/f", "x");
        rename_into(&dir, &bin);
        let text = sizes_text(&bin);
        assert_eq!(text.lines().count(), 1, "{text:?}");
        assert!(text.trim_end().ends_with(" two%0Alines"), "{text:?}");
    }

    // ---- bins on other drives ---------------------------------------------

    #[test]
    fn a_drive_bin_records_a_path_relative_to_the_top() {
        let temp = Temp::new("trash-top");
        let top = temp.dir("drive");
        let file = temp.write("drive/sub/a.txt", "a");
        let bin = topdir_trash(&top, 4242).unwrap();
        assert_eq!(bin, top.join(".Trash-4242"));
        assert_eq!(fs::metadata(&bin).unwrap().mode() & 0o777, 0o700);
        let item = trash_into(&file, &bin, Some(&top), Placing::Rename(None)).unwrap();
        assert!(
            info_of(&item).contains("\nPath=sub/a.txt\n"),
            "{}",
            info_of(&item)
        );
        untrash(&[item]).unwrap();
        assert!(file.exists(), "restoring uses the absolute original");
    }

    #[test]
    fn a_sticky_shared_bin_is_preferred_and_a_plain_or_linked_one_is_not() {
        use std::os::unix::fs::PermissionsExt;
        let temp = Temp::new("trash-sticky");
        let top = temp.dir("drive");
        let shared = top.join(".Trash");
        fs::create_dir(&shared).unwrap();
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).unwrap();
        let me = uid();
        assert_eq!(topdir_trash(&top, me).unwrap(), shared.join(me.to_string()));
        assert_eq!(
            fs::metadata(shared.join(me.to_string())).unwrap().mode() & 0o777,
            0o700
        );
        // Without the sticky bit it is not trusted.
        fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(
            topdir_trash(&top, me).unwrap(),
            top.join(format!(".Trash-{me}"))
        );
        // A link is not trusted either, sticky or not.
        let other = temp.dir("drive2");
        let real = temp.dir("elsewhere");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o1777)).unwrap();
        std::os::unix::fs::symlink(&real, other.join(".Trash")).unwrap();
        assert_eq!(
            topdir_trash(&other, me).unwrap(),
            other.join(format!(".Trash-{me}"))
        );
    }

    #[test]
    fn a_bin_that_is_not_ours_or_is_a_link_is_never_used() {
        let temp = Temp::new("trash-foreign");
        let top = temp.dir("drive");
        let me = uid();
        // `.Trash-$uid` that is a plain file.
        fs::write(top.join(format!(".Trash-{me}")), "x").unwrap();
        assert_eq!(
            topdir_trash(&top, me),
            Err(OpError::CannotTrash(TrashRefusal::NoBin))
        );
        // `.Trash-$uid` that is a link to somewhere else.
        let top2 = temp.dir("drive2");
        let elsewhere = temp.dir("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, top2.join(format!(".Trash-{me}"))).unwrap();
        assert_eq!(
            topdir_trash(&top2, me),
            Err(OpError::CannotTrash(TrashRefusal::NoBin))
        );
        // `.Trash-$uid` made by another user.
        let top3 = temp.dir("drive3");
        fs::create_dir(top3.join(".Trash-4242")).unwrap();
        if me != 4242 {
            assert_eq!(
                topdir_trash(&top3, 4242),
                Err(OpError::CannotTrash(TrashRefusal::NoBin)),
                "owned by uid {me}, not 4242"
            );
        }
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

    // ---- the decision, from a made-up mount table -------------------------

    #[test]
    fn the_bin_is_chosen_by_the_mount_the_item_is_on() {
        let temp = Temp::new("plan");
        let ctx = context(
            &temp,
            &[
                ("usb", "vfat", false),
                ("cd", "iso9660", true),
                ("nas", "nfs4", false),
                ("data", "ext4", false),
            ],
        );
        let at = |relative: &str| temp.path().join(relative);
        // Home and its siblings on the home drive: the home bin.
        assert_eq!(plan_bin(&at("home/a.txt"), &ctx), Ok(BinPlan::Home));
        assert_eq!(plan_bin(&at("elsewhere/a.txt"), &ctx), Ok(BinPlan::Home));
        // Another local drive and a stick: their own bins, never the home bin.
        assert_eq!(
            plan_bin(&at("usb/a.txt"), &ctx),
            Ok(BinPlan::Drive(at("usb")))
        );
        assert_eq!(
            plan_bin(&at("data/x/a.txt"), &ctx),
            Ok(BinPlan::Drive(at("data")))
        );
        // Read-only.
        assert_eq!(plan_bin(&at("cd/a.txt"), &ctx), Err(TrashRefusal::ReadOnly));
        // A share with no bin of ours: refused; with one: its own bin.
        assert_eq!(plan_bin(&at("nas/a.txt"), &ctx), Err(TrashRefusal::NoBin));
        fs::create_dir(at("nas").join(".Trash-4242")).unwrap();
        assert_eq!(
            plan_bin(&at("nas/a.txt"), &ctx),
            Ok(BinPlan::Drive(at("nas")))
        );
    }

    #[test]
    fn a_home_on_a_share_still_trashes_to_the_home_bin() {
        let temp = Temp::new("plan-nfs-home");
        // The mount that holds the home bin is the share itself.
        let home = temp.dir("home");
        let rows = format!(
            "1 0 0:1 / / rw - ext4 /dev/disk1 rw\n2 1 0:2 / {} rw - nfs4 srv:/home rw",
            home.display()
        );
        let ctx = Context {
            home_trash: home.join(".local/share/Trash"),
            dirs: Arc::new(UserDirs::parse(None, &home)),
            home: home.clone(),
            uid: 1,
            table: mounts::parse_mountinfo(&rows),
        };
        assert_eq!(plan_bin(&home.join("a.txt"), &ctx), Ok(BinPlan::Home));
    }

    #[test]
    fn an_unknown_mount_is_decided_by_device_numbers() {
        let temp = Temp::new("plan-unknown");
        let mut ctx = context(&temp, &[]);
        ctx.table.clear();
        assert_eq!(
            plan_bin(&temp.path().join("home/a"), &ctx),
            Ok(BinPlan::ByDevice)
        );
    }

    #[test]
    fn what_the_desktop_stands_on_is_refused_with_a_reason() {
        let temp = Temp::new("protect");
        let ctx = context(&temp, &[]);
        let home = ctx.home.clone();
        let refused = |path: &Path| check_trash_in(path, &ctx).unwrap_err();
        assert_eq!(refused(Path::new("/")), TrashRefusal::Root);
        assert_eq!(refused(Path::new("relative/a")), TrashRefusal::NotAbsolute);
        assert_eq!(refused(&home), TrashRefusal::Home);
        assert_eq!(
            refused(&home.join("Documents")),
            TrashRefusal::UserFolder("Documents")
        );
        assert_eq!(
            refused(&home.join("Desktop")),
            TrashRefusal::UserFolder("Desktop")
        );
        assert_eq!(
            refused(&home.join("Public")),
            TrashRefusal::UserFolder("Public")
        );
        assert_eq!(refused(home.parent().unwrap()), TrashRefusal::HoldsHome);
        // The bin, the folders it is made of, and what holds it.
        let bin = home.join(".local/share/Trash");
        assert_eq!(refused(&bin), TrashRefusal::TrashItself);
        assert_eq!(refused(&bin.join("files")), TrashRefusal::TrashItself);
        assert_eq!(refused(&bin.join("info")), TrashRefusal::TrashItself);
        assert_eq!(
            refused(&home.join(".local/share")),
            TrashRefusal::HoldsTrash
        );
        assert_eq!(refused(&home.join(".local")), TrashRefusal::HoldsTrash);
        assert_eq!(
            refused(Path::new("/media/usb/.Trash-1000")),
            TrashRefusal::TrashItself
        );
        assert_eq!(
            refused(Path::new("/media/usb/.Trash")),
            TrashRefusal::TrashItself
        );
        assert_eq!(
            refused(Path::new("/media/usb/.Trash/1000")),
            TrashRefusal::TrashItself
        );
        // Inside a bin.
        assert_eq!(refused(&bin.join("files/a.txt")), TrashRefusal::InTrash);
        assert_eq!(refused(&bin.join("files/dir/inner")), TrashRefusal::InTrash);
        assert_eq!(
            refused(Path::new("/media/usb/.Trash-1000/files/x")),
            TrashRefusal::InTrash
        );
        // An ordinary file and a folder that merely looks like one of those.
        assert_eq!(check_trash_in(&home.join("Documents/a.txt"), &ctx), Ok(()));
        assert_eq!(check_trash_in(&home.join("files"), &ctx), Ok(()));
        assert_eq!(check_trash_in(&home.join("Trash"), &ctx), Ok(()));
    }

    #[test]
    fn a_refusal_reads_as_a_sentence_and_only_some_offer_a_delete() {
        for plural in [false, true] {
            for refusal in [
                TrashRefusal::Root,
                TrashRefusal::InTrash,
                TrashRefusal::HoldsHome,
                TrashRefusal::UserFolder("Pictures"),
                TrashRefusal::ReadOnly,
                TrashRefusal::NoBin,
            ] {
                let text = refusal.reason(plural);
                assert!(text.ends_with('.'), "{text}");
                assert!(text.chars().next().unwrap().is_uppercase(), "{text}");
            }
        }
        assert_eq!(
            TrashRefusal::ReadOnly.reason(false),
            "It is on a read-only drive, so it cannot be moved to the Trash."
        );
        assert_eq!(
            TrashRefusal::NoBin.reason(true),
            "Their drive has no Trash and one cannot be made there, so they cannot be moved to the Trash."
        );
        assert_eq!(
            TrashRefusal::InTrash.reason(true),
            "They are in the Trash already."
        );
        assert!(TrashRefusal::NoBin.offers_delete());
        assert!(TrashRefusal::ReadOnly.offers_delete());
        assert!(TrashRefusal::InTrash.offers_delete());
        for never in [
            TrashRefusal::Root,
            TrashRefusal::Home,
            TrashRefusal::TrashItself,
            TrashRefusal::HoldsHome,
            TrashRefusal::HoldsTrash,
            TrashRefusal::UserFolder("Music"),
        ] {
            assert!(!never.offers_delete(), "{never:?}");
        }
    }

    #[test]
    fn trashing_through_the_context_uses_the_home_bin_and_a_drive_bin() {
        let temp = Temp::new("trash-ctx");
        let ctx = context(&temp, &[("usb", "vfat", false)]);
        let home_file = temp.write("home/Documents/a.txt", "h");
        let usb_file = temp.write("usb/b.txt", "u");
        let items = trash_in(&ctx, &[home_file.clone(), usb_file.clone()], Copying::Small).unwrap();
        assert_eq!(items[0].in_trash, ctx.home_trash.join("files/a.txt"));
        assert_eq!(
            items[1].in_trash,
            temp.path().join("usb/.Trash-4242/files/b.txt")
        );
        assert!(
            info_of(&items[1]).contains("\nPath=b.txt\n"),
            "a drive's bin is relative to its top: {}",
            info_of(&items[1])
        );
        assert!(!home_file.exists() && !usb_file.exists());
        untrash(&items).unwrap();
        assert!(home_file.exists() && usb_file.exists());
    }

    #[test]
    fn nothing_moves_when_one_of_the_items_is_refused() {
        let temp = Temp::new("trash-refused");
        let ctx = context(&temp, &[("cd", "iso9660", true)]);
        let fine = temp.write("home/a.txt", "a");
        let on_cd = temp.write("cd/b.txt", "b");
        assert_eq!(
            trash_in(&ctx, &[fine.clone(), on_cd.clone()], Copying::Small),
            Err(OpError::CannotTrash(TrashRefusal::ReadOnly))
        );
        assert!(fine.exists() && on_cd.exists(), "neither was moved");
        assert!(
            !ctx.home_trash.join("files").exists(),
            "no bin was even made"
        );
        assert_eq!(
            trash_in(&ctx, &[ctx.home.clone()], Copying::Small),
            Err(OpError::CannotTrash(TrashRefusal::Home))
        );
        assert!(ctx.home.is_dir());
    }

    #[test]
    fn a_failure_part_way_puts_back_what_was_moved() {
        let temp = Temp::new("trash-rollback");
        let ctx = context(&temp, &[]);
        let one = temp.write("home/one.txt", "1");
        let missing = temp.path().join("home/missing.txt");
        let error = trash_in(&ctx, &[one.clone(), missing], Copying::Small).unwrap_err();
        assert_eq!(error, OpError::NotFound);
        assert_eq!(
            fs::read_to_string(&one).unwrap(),
            "1",
            "the first one is back"
        );
        assert_eq!(
            fs::read_dir(ctx.home_trash.join("info")).unwrap().count(),
            0
        );
    }

    // ---- copying across a mount point on the home drive --------------------

    #[test]
    fn the_copy_fallback_keeps_the_name_the_info_and_the_sizes() {
        let temp = Temp::new("trash-copy");
        let bin = temp.path().join("Trash");
        let dir = temp.dir("proj");
        temp.write("proj/inner.txt", "inside");
        std::os::unix::fs::symlink("inner.txt", dir.join("link")).unwrap();
        let clash = rename_into(&temp.write("x/proj", "older"), &bin);
        assert_eq!(clash.in_trash.file_name().unwrap(), "proj");
        let item = trash_into(&dir, &bin, None, Placing::Copy(Copying::Small)).unwrap();
        assert_eq!(item.in_trash.file_name().unwrap(), "proj.2");
        assert!(!dir.exists(), "the original went");
        assert_eq!(
            fs::read_to_string(item.in_trash.join("inner.txt")).unwrap(),
            "inside"
        );
        assert_eq!(
            fs::read_link(item.in_trash.join("link")).unwrap(),
            Path::new("inner.txt")
        );
        let info = parse_info(&fs::read(&item.info).unwrap()).unwrap();
        assert_eq!(info.path, dir);
        assert!(sizes_text(&bin).contains(" proj.2"), "{}", sizes_text(&bin));
        let staging: Vec<_> = fs::read_dir(&bin)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".moving"))
            .collect();
        assert!(
            staging.is_empty(),
            "the staging folder is removed: {staging:?}"
        );
        untrash(&[item]).unwrap();
        assert!(dir.join("inner.txt").exists());
    }

    #[test]
    fn a_big_tree_asks_before_it_is_copied() {
        let temp = Temp::new("trash-copy-big");
        let bin = temp.path().join("Trash");
        let big = temp.write("big.bin", "x");
        // Make it look big without writing 64 MiB: a sparse file.
        let file = fs::OpenOptions::new().write(true).open(&big).unwrap();
        file.set_len(COPY_LIMIT + 1).unwrap();
        drop(file);
        assert_eq!(
            trash_into(&big, &bin, None, Placing::Copy(Copying::Small)),
            Err(OpError::CopyNeeded)
        );
        assert!(big.exists(), "nothing was touched");
        assert_eq!(
            fs::read_dir(bin.join("info")).unwrap().count(),
            0,
            "the reserved info is gone"
        );
        // Agreed to: it goes.
        let item = trash_into(&big, &bin, None, Placing::Copy(Copying::Any)).unwrap();
        assert!(item.in_trash.exists());
        assert!(!big.exists());
    }

    #[test]
    fn a_small_tree_is_not_big_and_a_million_empty_files_would_be() {
        let temp = Temp::new("trash-weigh");
        let small = temp.write("s/a.txt", "tiny");
        assert!(!weighs_more_than(&small, COPY_LIMIT));
        assert!(
            weighs_more_than(&small, 10),
            "every entry counts for at least a kibibyte"
        );
        let dir = temp.dir("many");
        for index in 0..5 {
            temp.write(&format!("many/{index}"), "");
        }
        assert!(
            weighs_more_than(&dir, 4 * 1024),
            "six entries are over four kibibytes"
        );
        assert!(!weighs_more_than(&dir, 7 * 1024));
    }

    #[test]
    fn io_errors_into_the_bin_read_plainly() {
        assert_eq!(
            trash_io_error(io::Error::from_raw_os_error(libc::EROFS)),
            OpError::CannotTrash(TrashRefusal::ReadOnly)
        );
        assert_eq!(
            trash_io_error(io::Error::from_raw_os_error(libc::EACCES)),
            OpError::Permission
        );
        assert_eq!(
            trash_io_error(io::Error::from_raw_os_error(libc::ENOENT)),
            OpError::NotFound
        );
    }

    #[test]
    fn can_trash_reads_the_mount_kind() {
        let temp = Temp::new("can-trash");
        let ctx = context(
            &temp,
            &[
                ("usb", "vfat", false),
                ("cd", "iso9660", true),
                ("nas", "nfs4", false),
            ],
        );
        let ok = |relative: &str| check_trash_in(&temp.path().join(relative), &ctx).is_ok();
        assert!(ok("home/a.txt"), "the home drive");
        assert!(ok("usb/a.txt"), "a local drive gets its own bin");
        assert!(!ok("cd/a.txt"), "read-only");
        assert!(!ok("nas/a.txt"), "a network drive without a bin");
        fs::create_dir(temp.path().join("nas/.Trash-4242")).unwrap();
        assert!(ok("nas/a.txt"));
        assert!(!ok("usb/.Trash-4242/files/a.txt"));
    }

    #[test]
    fn a_path_in_a_bin_is_known_by_its_name() {
        assert!(in_a_bin(
            Path::new("/home/a/.local/share/Trash/files/x"),
            Some(Path::new("/home/a/.local/share/Trash"))
        ));
        assert!(in_a_bin(Path::new("/media/usb/.Trash-1000/files/x"), None));
        assert!(!in_a_bin(
            Path::new("/home/a/Documents/x"),
            Some(Path::new("/home/a/.local/share/Trash"))
        ));
    }
}
