//! What the Properties dialog shows.

use std::collections::HashMap;
use std::ffi::CStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::entry::Kind;
use crate::listing::Cancel;
use crate::mime;
use crate::mounts;
use crate::stat::{self, StatInfo};

use super::OpError;

/// How long a folder's size may take before it is shown as "at least".
const FOLDER_DEADLINE: Duration = Duration::from_secs(2);
/// How long a drive may take to say how full it is.
const SPACE_DEADLINE: Duration = Duration::from_secs(2);

/// The drive a file is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// The volume label, else the mount point's name.
    pub label: String,
    /// `/dev/nvme0n1p2`, `server:/export`.
    pub device: String,
    pub fs_type: String,
    pub mount_point: PathBuf,
    pub read_only: bool,
    /// Size of the drive, when the filesystem says.
    pub total_bytes: Option<u64>,
    /// Free to an ordinary user.
    pub free_bytes: Option<u64>,
}

impl Volume {
    /// How full the drive is, from 0 to 1.
    pub fn used_fraction(&self) -> Option<f32> {
        let total = self.total_bytes.filter(|total| *total > 0)?;
        Some(total.saturating_sub(self.free_bytes?) as f32 / total as f32)
    }
}

/// Everything about one file or folder.
///
/// A link is described by its target (as listings do), with `symlink` set
/// and `link_target` holding what the link says. A link that points nowhere
/// has kind [`Kind::Other`] and the link's own times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Properties {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub symlink: bool,
    pub link_target: Option<PathBuf>,
    pub mime: mime::Type,
    /// Bytes. For a folder: everything under it, counted without following
    /// links, by [`properties`]; [`stat_properties`] leaves it zero. See
    /// `size_is_lower_bound`.
    pub size: u64,
    /// Folders: how many files, links and folders are under it.
    pub item_count: Option<u64>,
    /// Counting stopped early (a 2 s deadline, or a folder that could not be
    /// read), so `size` and `item_count` are "at least".
    pub size_is_lower_bound: bool,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    /// Permission bits, `0o755` style, including setuid, setgid and sticky.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// The owner's name, or the number if it has no name.
    pub owner: Option<String>,
    pub group: Option<String>,
    /// The drive it is on, if the mount table has it.
    pub volume: Option<Volume>,
}

impl Properties {
    /// Whether the person running this may change the permissions: the
    /// owner may, and so may root.
    pub fn can_chmod(&self) -> bool {
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let euid = unsafe { libc::geteuid() };
        euid == 0 || euid == self.uid
    }
}

/// Read the properties of `path`. Blocks on the disk; for a folder it counts
/// the contents for up to two seconds. Returns [`OpError::Cancelled`] if
/// `cancel` is set meanwhile.
pub fn properties(path: &Path, cancel: &Cancel) -> Result<Properties, OpError> {
    let mut props = stat_properties(path)?;
    if props.kind == Kind::Directory {
        let totals = folder_totals(path, cancel, FOLDER_DEADLINE)?;
        props.size = totals.bytes;
        props.item_count = Some(totals.items);
        props.size_is_lower_bound = totals.incomplete;
    }
    Ok(props)
}

/// The properties of `path` that one `stat` and the mount table give, with
/// no walk of a folder: its `size` is zero and `item_count` empty. A dialog
/// adds the folder up in the background with [`super::Counting`].
pub fn stat_properties(path: &Path) -> Result<Properties, OpError> {
    let info = stat::stat_entry(path)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let link_target = info.is_link.then(|| fs::read_link(path).ok()).flatten();
    let mime = match info.kind {
        Kind::Directory => mime::DIRECTORY,
        _ => mime::for_name(&name),
    };
    let size = if info.kind == Kind::Directory { 0 } else { info.size };
    let StatInfo {
        created,
        modified,
        accessed,
        mode,
        uid,
        gid,
        ..
    } = info;
    Ok(Properties {
        name,
        path: path.to_path_buf(),
        kind: info.kind,
        symlink: info.is_link,
        link_target,
        mime,
        size,
        item_count: None,
        size_is_lower_bound: false,
        created,
        modified,
        accessed,
        mode,
        uid,
        gid,
        owner: Some(user_name(uid).unwrap_or_else(|| uid.to_string())),
        group: Some(group_name(gid).unwrap_or_else(|| gid.to_string())),
        volume: volume_of(path),
    })
}

/// The drive `path` is on, with how full it is. Asks the drive for its
/// space, waiting at most [`SPACE_DEADLINE`].
fn volume_of(path: &Path) -> Option<Volume> {
    let mut mount = mounts::mount_of(path)?;
    mounts::fill_space(std::slice::from_mut(&mut mount), SPACE_DEADLINE);
    Some(Volume {
        total_bytes: mount.total_bytes(),
        free_bytes: mount.free_bytes(),
        label: mount.label,
        device: mount.device,
        fs_type: mount.fs_type,
        mount_point: mount.mount_point,
        read_only: mount.read_only,
    })
}

/// Set the permission bits of `path` to `mode` (`0o755` style, setuid,
/// setgid and sticky included), following a link to its target. Only the
/// owner and root may; anyone else gets [`OpError::Permission`].
pub fn chmod(path: &Path, mode: u32) -> Result<(), OpError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7777)).map_err(OpError::from)
}

/// The ownership and permissions of one item, which is all the Permissions
/// tab needs: one `statx` and two name lookups, with no drive or folder
/// walk. A link is described by its target, as [`chmod`] acts on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permissions {
    /// Permission bits, `0o755` style, including setuid, setgid and sticky.
    pub mode: u32,
    pub is_dir: bool,
    pub uid: u32,
    pub gid: u32,
    /// The owner's name, or the number if it has no name.
    pub owner: String,
    pub group: String,
    /// The person running this may change `mode`: the owner, and root.
    pub can_chmod: bool,
}

/// Read the [`Permissions`] of `path`.
pub fn permissions(path: &Path) -> Result<Permissions, OpError> {
    let info = stat::stat_entry(path)?;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    Ok(Permissions {
        mode: info.mode & 0o7777,
        is_dir: info.kind == Kind::Directory,
        uid: info.uid,
        gid: info.gid,
        owner: user_name(info.uid).unwrap_or_else(|| info.uid.to_string()),
        group: group_name(info.gid).unwrap_or_else(|| info.gid.to_string()),
        can_chmod: euid == 0 || euid == info.uid,
    })
}

/// The mode that "apply to enclosed items" gives one item under a folder
/// whose mode is now `folder`. Folders take the folder's bits as they are.
/// A file takes its read and write bits, and keeps an execute bit it had only
/// where the folder allows that class to execute, so a script stays a script
/// and a plain file never becomes a program. Setuid, setgid and sticky are
/// not passed on to files.
pub fn tree_mode(folder: u32, is_dir: bool, current: u32) -> u32 {
    let folder = folder & 0o7777;
    if is_dir {
        folder
    } else {
        (folder & 0o666) | (current & folder & 0o111)
    }
}

/// What [`chmod_tree`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeChange {
    /// Files and folders whose mode was set (or already was).
    pub changed: u64,
    /// Items that could not be changed or read: not owned, a folder that
    /// cannot be entered, a drive that is read-only.
    pub failed: u64,
}

/// Give everything under `root` (not `root` itself) the mode that
/// [`tree_mode`] works out from `folder`. Links are skipped, never followed.
/// A folder's own mode is set after its contents', so a folder that loses
/// the owner's access is still walked. Returns [`OpError::Cancelled`] if
/// `cancel` is set meanwhile; what was changed stays changed.
pub fn chmod_tree(root: &Path, folder: u32, cancel: &Cancel) -> Result<TreeChange, OpError> {
    let mut change = TreeChange::default();
    tree_step(root, folder, cancel, &mut change, false)?;
    Ok(change)
}

fn tree_step(
    path: &Path,
    folder: u32,
    cancel: &Cancel,
    change: &mut TreeChange,
    set_self: bool,
) -> Result<(), OpError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if cancel.is_cancelled() {
        return Err(OpError::Cancelled);
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        change.failed += 1;
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        match fs::read_dir(path) {
            Ok(children) => {
                for child in children.flatten() {
                    tree_step(&child.path(), folder, cancel, change, true)?;
                }
            }
            Err(_) => change.failed += 1,
        }
    }
    if set_self {
        let mode = tree_mode(folder, meta.is_dir(), meta.mode() & 0o7777);
        match fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
            Ok(()) => change.changed += 1,
            Err(_) => change.failed += 1,
        }
    }
    Ok(())
}

struct Totals {
    bytes: u64,
    items: u64,
    incomplete: bool,
}

/// Count under `root`, never following a link below it. Stops at `limit`
/// with `incomplete` set; unreadable folders set it too.
fn folder_totals(root: &Path, cancel: &Cancel, limit: Duration) -> Result<Totals, OpError> {
    let deadline = Instant::now() + limit;
    let mut totals = Totals {
        bytes: 0,
        items: 0,
        incomplete: false,
    };
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(children) = fs::read_dir(&dir) else {
            totals.incomplete = true;
            continue;
        };
        for child in children.flatten() {
            if cancel.is_cancelled() {
                return Err(OpError::Cancelled);
            }
            if Instant::now() >= deadline {
                totals.incomplete = true;
                return Ok(totals);
            }
            let Ok(meta) = child.metadata() else {
                totals.incomplete = true;
                continue;
            };
            totals.items += 1;
            if meta.is_dir() {
                pending.push(child.path());
            } else if meta.is_file() {
                totals.bytes += meta.len();
            }
        }
    }
    Ok(totals)
}

type NameCache = Mutex<HashMap<(bool, u32), Option<String>>>;

static CACHE: LazyLock<NameCache> = LazyLock::new(NameCache::default);

/// The login name of a user id, or `None` if it has none. Remembered.
pub fn user_name(uid: u32) -> Option<String> {
    cached_name(false, uid)
}

/// The name of a group id, or `None` if it has none. Remembered.
pub fn group_name(gid: u32) -> Option<String> {
    cached_name(true, gid)
}

fn cached_name(group: bool, id: u32) -> Option<String> {
    if let Some(known) = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&(group, id))
    {
        return known.clone();
    }
    // The lookup may read a network directory: do not hold the lock for it.
    let name = if group {
        lookup_group(id)
    } else {
        lookup_user(id)
    };
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((group, id), name.clone());
    name
}

fn buffer_size() -> usize {
    // SAFETY: `sysconf` only reads a constant.
    let hint = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    if hint > 0 { hint as usize } else { 4096 }
}

fn lookup_user(uid: u32) -> Option<String> {
    let mut buffer = vec![0u8; buffer_size()];
    loop {
        // SAFETY: the buffer outlives the call and its length is passed; on
        // success `pw_name` points into it.
        unsafe {
            let mut entry: libc::passwd = std::mem::zeroed();
            let mut found: *mut libc::passwd = std::ptr::null_mut();
            let code = libc::getpwuid_r(
                uid,
                &mut entry,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            );
            if code == libc::ERANGE && buffer.len() < 1 << 20 {
                buffer.resize(buffer.len() * 2, 0);
                continue;
            }
            if code != 0 || found.is_null() {
                return None;
            }
            return Some(CStr::from_ptr(entry.pw_name).to_string_lossy().into_owned());
        }
    }
}

fn lookup_group(gid: u32) -> Option<String> {
    let mut buffer = vec![0u8; buffer_size().max(4096)];
    loop {
        // SAFETY: as for `lookup_user`.
        unsafe {
            let mut entry: libc::group = std::mem::zeroed();
            let mut found: *mut libc::group = std::ptr::null_mut();
            let code = libc::getgrgid_r(
                gid,
                &mut entry,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            );
            if code == libc::ERANGE && buffer.len() < 1 << 20 {
                buffer.resize(buffer.len() * 2, 0);
                continue;
            }
            if code != 0 || found.is_null() {
                return None;
            }
            return Some(CStr::from_ptr(entry.gr_name).to_string_lossy().into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_file() {
        let temp = Temp::new("props-file");
        let file = temp.write("notes.txt", "hello");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let props = properties(&file, &Cancel::new()).unwrap();
        assert_eq!(props.name, "notes.txt");
        assert_eq!(props.path, file);
        assert_eq!(props.kind, Kind::File);
        assert!(!props.symlink);
        assert_eq!(props.link_target, None);
        assert_eq!(props.mime.mime, "text/plain");
        assert_eq!(props.size, 5);
        assert_eq!(props.item_count, None);
        assert!(!props.size_is_lower_bound);
        assert_eq!(props.mode, 0o640);
        assert!(props.modified.is_some());
        assert!(props.accessed.is_some());
        assert!(props.owner.is_some());
        assert!(props.group.is_some());
        // The drive's space is asked with a deadline, which a loaded machine
        // can miss; the drive itself is always known.
        assert!(props.volume.is_some());
        assert_eq!(props.uid, unsafe { libc::getuid() });
        assert!(props.can_chmod());
    }

    #[test]
    fn a_folder_is_counted_without_following_links() {
        let temp = Temp::new("props-dir");
        let outside = temp.write("outside/big.txt", "0123456789");
        let dir = temp.dir("d");
        temp.write("d/a.txt", "aaa");
        temp.write("d/sub/b.txt", "bb");
        temp.dir("d/sub/empty");
        std::os::unix::fs::symlink(outside.parent().unwrap(), dir.join("link")).unwrap();
        let props = properties(&dir, &Cancel::new()).unwrap();
        assert_eq!(props.kind, Kind::Directory);
        assert_eq!(props.size, 5);
        // a.txt, sub, b.txt, empty, link
        assert_eq!(props.item_count, Some(5));
        assert!(!props.size_is_lower_bound);
        assert_eq!(props.mime.mime, "inode/directory");
    }

    #[test]
    fn a_link_is_described_by_its_target() {
        let temp = Temp::new("props-link");
        let target = temp.write("target.txt", "four");
        let link = temp.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let props = properties(&link, &Cancel::new()).unwrap();
        assert!(props.symlink);
        assert_eq!(props.link_target, Some(target));
        assert_eq!(props.kind, Kind::File);
        assert_eq!(props.size, 4);
        assert_eq!(props.name, "link.txt");

        let broken = temp.path().join("broken");
        std::os::unix::fs::symlink("/nonexistent/x", &broken).unwrap();
        let props = properties(&broken, &Cancel::new()).unwrap();
        assert!(props.symlink);
        assert_eq!(props.kind, Kind::Other);
        assert_eq!(props.link_target, Some(PathBuf::from("/nonexistent/x")));
    }

    #[test]
    fn a_missing_path_is_not_found() {
        let temp = Temp::new("props-missing");
        assert_eq!(
            properties(&temp.path().join("nope"), &Cancel::new()),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn counting_obeys_cancel_and_the_deadline() {
        let temp = Temp::new("props-deadline");
        let dir = temp.dir("d");
        for index in 0..20 {
            temp.write(&format!("d/f{index}"), "x");
        }
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(properties(&dir, &cancel), Err(OpError::Cancelled));

        let totals = folder_totals(&dir, &Cancel::new(), Duration::ZERO).unwrap();
        assert!(totals.incomplete, "no time at all means a lower bound");
        assert!(totals.items < 20);
        let totals = folder_totals(&dir, &Cancel::new(), Duration::from_secs(30)).unwrap();
        assert!(!totals.incomplete);
        assert_eq!(totals.items, 20);
        assert_eq!(totals.bytes, 20);
    }

    #[test]
    fn an_unreadable_folder_makes_the_count_a_lower_bound() {
        if unsafe { libc::getuid() } == 0 {
            eprintln!("skipped: root can read everything");
            return;
        }
        let temp = Temp::new("props-unreadable");
        let dir = temp.dir("d");
        temp.write("d/a.txt", "aa");
        let locked = temp.dir("d/locked");
        temp.write("d/locked/b.txt", "b");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let props = properties(&dir, &Cancel::new()).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(props.size_is_lower_bound);
        assert_eq!(props.size, 2);
        assert_eq!(props.item_count, Some(2));
    }

    #[test]
    fn names_of_users_and_groups() {
        assert_eq!(user_name(0).as_deref(), Some("root"));
        assert_eq!(
            user_name(0).as_deref(),
            Some("root"),
            "second time is cached"
        );
        assert!(group_name(0).is_some());
        assert_eq!(user_name(4_000_000_123), None);
        assert_eq!(group_name(4_000_000_123), None);
    }

    #[test]
    fn a_volume_says_how_full_it_is() {
        let mut volume = Volume {
            label: "Data".into(),
            device: "/dev/sdb1".into(),
            fs_type: "ext4".into(),
            mount_point: PathBuf::from("/data"),
            read_only: false,
            total_bytes: Some(1000),
            free_bytes: Some(250),
        };
        assert_eq!(volume.used_fraction(), Some(0.75));
        volume.free_bytes = None;
        assert_eq!(volume.used_fraction(), None);
        volume.total_bytes = Some(0);
        volume.free_bytes = Some(0);
        assert_eq!(volume.used_fraction(), None, "an empty drive has no fraction");
    }

    #[test]
    fn stat_properties_does_not_walk_a_folder() {
        let temp = Temp::new("props-stat");
        let dir = temp.dir("d");
        temp.write("d/a.txt", "aaa");
        let props = stat_properties(&dir).unwrap();
        assert_eq!(props.kind, Kind::Directory);
        assert_eq!(props.size, 0);
        assert_eq!(props.item_count, None);
    }

    #[test]
    fn chmod_changes_the_bits_and_follows_links() {
        let temp = Temp::new("props-chmod");
        let file = temp.write("run.sh", "#!/bin/sh");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        chmod(&file, 0o755).unwrap();
        assert_eq!(stat_properties(&file).unwrap().mode, 0o755);
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        chmod(&link, 0o600).unwrap();
        assert_eq!(stat_properties(&file).unwrap().mode, 0o600);
        assert_eq!(
            chmod(&temp.path().join("missing"), 0o644),
            Err(OpError::NotFound)
        );
    }

    #[test]
    fn permissions_are_one_stat_with_names() {
        let temp = Temp::new("props-perms");
        let file = temp.write("a", "x");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let perms = permissions(&file).unwrap();
        assert_eq!((perms.mode, perms.is_dir, perms.can_chmod), (0o640, false, true));
        assert_eq!(perms.uid, unsafe { libc::getuid() });
        assert!(!perms.owner.is_empty() && !perms.group.is_empty());
        assert!(permissions(temp.path()).unwrap().is_dir);
        assert_eq!(permissions(&temp.path().join("gone")), Err(OpError::NotFound));
    }

    #[test]
    fn a_file_under_a_folder_keeps_only_the_execute_bits_it_had() {
        // A plain file never becomes a program.
        assert_eq!(tree_mode(0o755, false, 0o644), 0o644);
        // A script stays one, for the classes the folder lets execute.
        assert_eq!(tree_mode(0o755, false, 0o755), 0o755);
        assert_eq!(tree_mode(0o750, false, 0o755), 0o750);
        assert_eq!(tree_mode(0o700, false, 0o755), 0o700);
        // Read and write follow the folder; special bits are not passed on.
        assert_eq!(tree_mode(0o2770, false, 0o644), 0o660);
        // A folder takes the folder's mode whole.
        assert_eq!(tree_mode(0o2770, true, 0o755), 0o2770);
    }

    #[test]
    fn recursive_chmod_walks_before_it_locks_and_skips_links() {
        let temp = Temp::new("props-tree");
        let outside = temp.write("outside/keep", "x");
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o644)).unwrap();
        let root = temp.dir("root");
        let script = temp.write("root/sub/run.sh", "#!/bin/sh");
        let note = temp.write("root/note.txt", "n");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(&note, fs::Permissions::from_mode(0o644)).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        // Owner-only: the sub folder loses nothing, but others lose everything.
        let change = chmod_tree(&root, 0o700, &Cancel::new()).unwrap();
        assert_eq!(change, TreeChange { changed: 3, failed: 0 });
        let mode = |path: &Path| permissions(path).unwrap().mode;
        assert_eq!(mode(&script), 0o700);
        assert_eq!(mode(&note), 0o600);
        assert_eq!(mode(&root.join("sub")), 0o700);
        assert_eq!(mode(&outside), 0o644, "a link is not followed");
        // The root itself is the caller's to set.
        assert_ne!(mode(&root), 0);
        // A folder made unreadable (0o000 here) is still walked first.
        let locked = chmod_tree(&root, 0o000, &Cancel::new()).unwrap();
        assert_eq!(locked, TreeChange { changed: 3, failed: 0 });
        assert_eq!(mode(&root.join("sub")), 0o000);
        // Restore so the scratch folder can be removed.
        fs::set_permissions(root.join("sub"), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn a_cancelled_recursive_chmod_changes_nothing_more() {
        let temp = Temp::new("props-tree-cancel");
        temp.write("d/a", "x");
        let cancel = Cancel::new();
        cancel.cancel();
        assert_eq!(chmod_tree(&temp.path().join("d"), 0o600, &cancel), Err(OpError::Cancelled));
    }
}
