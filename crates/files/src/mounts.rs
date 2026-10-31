//! Mounted drives: what the sidebar lists, and how big they are.
//!
//! Two rules shape this module.
//!
//! 1. **Listing mounts never touches a mount.** [`list_mounts`] reads
//!    `/proc/self/mountinfo` and a few small files in `/dev/disk` and
//!    `/sys`. It is as cheap on a machine with a dead NFS server as on one
//!    without.
//! 2. **Sizes come from a worker with a deadline.** `statvfs` on a hung
//!    network mount blocks for minutes, so [`fill_space`] runs it on helper
//!    threads and gives up at the deadline: the mount shows
//!    [`Usage::NotResponding`] and is not asked again until the thread that
//!    is stuck comes back. Answers are cached for a few seconds.
//!
//! The kind of a mount ([`MountKind`]) is what the loader uses to choose how
//! hard to work: see [`crate::loader::Policy`].

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How much a drive may be trusted to answer quickly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MountKind {
    /// An internal disk.
    Local,
    /// A USB stick, a card, an external disk.
    Removable,
    /// NFS, SMB, SSHFS and friends: every call may take a network round
    /// trip, or never return.
    Network,
    /// A CD or DVD.
    Optical,
    /// Anything else a person can browse: an encrypted overlay, a FUSE
    /// filesystem.
    Other,
}

/// How full a drive is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// Not asked yet, or the filesystem reports nothing.
    Unknown,
    /// Being asked.
    Pending,
    Known {
        total: u64,
        free: u64,
    },
    /// `statvfs` missed its deadline.
    NotResponding,
}

/// One mounted drive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    /// The kernel's mount id.
    pub id: u32,
    /// What to call it: the volume label, else the mount point's name.
    pub label: String,
    /// `/dev/sdb1`, `192.168.0.39:/export`.
    pub device: String,
    pub mount_point: PathBuf,
    pub fs_type: String,
    pub kind: MountKind,
    pub usage: Usage,
    pub read_only: bool,
    /// Mounted by the person, from a file manager or `udisksctl`: under
    /// `/run/media/$USER` or `/media/$USER`.
    pub mounted_by_user: bool,
    /// The `major:minor` of the device, for telling two views of one disk
    /// apart.
    pub device_number: (u32, u32),
    /// Shown in drive lists. False for pseudo filesystems, system mounts and
    /// a second view of a disk that is listed already. [`all_mounts`] keeps
    /// them, because the loader needs the kind of any path.
    pub visible: bool,
}

impl MountKind {
    /// Whether a view may decode thumbnails of files on this kind of drive.
    /// False where doing so reads file content over a network or from a disc.
    pub fn thumbnails(self) -> bool {
        matches!(self, MountKind::Local | MountKind::Removable)
    }

    /// Whether a view may open files to find out their type.
    pub fn sniff_content(self) -> bool {
        self.thumbnails()
    }
}

impl Mount {
    pub fn total_bytes(&self) -> Option<u64> {
        match self.usage {
            Usage::Known { total, .. } => Some(total),
            _ => None,
        }
    }

    pub fn free_bytes(&self) -> Option<u64> {
        match self.usage {
            Usage::Known { free, .. } => Some(free),
            _ => None,
        }
    }

    /// Used divided by total, from 0 to 1.
    pub fn usage_fraction(&self) -> Option<f32> {
        match self.usage {
            Usage::Known { total, free } if total > 0 => {
                Some((total.saturating_sub(free)) as f32 / total as f32)
            }
            _ => None,
        }
    }

    /// Whether the person may unmount it from a file manager: everything
    /// except the disks the system lives on.
    pub fn can_unmount(&self) -> bool {
        self.mount_point != Path::new("/")
            && (self.kind != MountKind::Local || self.mounted_by_user)
    }
}

const PSEUDO: &[&str] = &[
    "proc",
    "sysfs",
    "devtmpfs",
    "devpts",
    "tmpfs",
    "ramfs",
    "cgroup",
    "cgroup2",
    "securityfs",
    "debugfs",
    "tracefs",
    "configfs",
    "fusectl",
    "mqueue",
    "hugetlbfs",
    "pstore",
    "bpf",
    "autofs",
    "binfmt_misc",
    "efivarfs",
    "selinuxfs",
    "nsfs",
    "overlay",
    "squashfs",
    "rpc_pipefs",
    "nfsd",
    "fuse.gvfsd-fuse",
    "fuse.portal",
    "fuse.lxcfs",
    "fuse.snapfuse",
    "binder",
    "bdev",
    "devmapper",
    "ecryptfs",
];

const NETWORK: &[&str] = &[
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "9p",
    "afs",
    "ncpfs",
    "ceph",
    "glusterfs",
    "lustre",
    "gpfs",
    "davfs",
    "fuse.sshfs",
    "fuse.rclone",
    "fuse.s3fs",
    "fuse.davfs2",
    "fuse.curlftpfs",
    "fuse.glusterfs",
    "fuse.cephfs",
    "fuse.smbnetfs",
];

/// Folders whose mounts belong to the system, not to the person.
const SYSTEM_ROOTS: &[&str] = &[
    "/boot",
    "/efi",
    "/sys",
    "/proc",
    "/dev",
    "/var/lib",
    "/var/tmp",
    "/var/snap",
    "/snap",
    "/usr",
    "/etc",
    "/tmp",
];

/// `/proc/self/mountinfo` writes a space as `\040`, a tab as `\011`, a
/// newline as `\012` and a backslash as `\134`.
fn unescape(text: &str) -> String {
    if !text.contains('\\') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut raw = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let octal = (bytes[index] == b'\\' && index + 3 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[index + 1..index + 4]).ok())
            .flatten()
            .and_then(|digits| u8::from_str_radix(digits, 8).ok());
        match octal {
            Some(value) => {
                raw.push(value);
                index += 4;
            }
            None => {
                raw.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn user_media_roots() -> Vec<String> {
    let user = std::env::var("USER").unwrap_or_default();
    if user.is_empty() {
        return vec![];
    }
    vec![format!("/run/media/{user}/"), format!("/media/{user}/")]
}

/// Parse the text of `/proc/self/mountinfo` into mounts, every one of them,
/// with `visible` telling which belong in a drive list. Sizes are
/// [`Usage::Unknown`]. Pure: reads nothing.
pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    parse_with(text, &user_media_roots())
}

fn parse_with(text: &str, media_roots: &[String]) -> Vec<Mount> {
    let mut mounts: Vec<Mount> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split(' ');
        let id = fields.next().and_then(|field| field.parse::<u32>().ok());
        let _parent = fields.next();
        let device_number = fields.next().and_then(|field| {
            let (major, minor) = field.split_once(':')?;
            Some((major.parse().ok()?, minor.parse().ok()?))
        });
        let _root = fields.next();
        let mount_point = fields.next();
        let options = fields.next();
        let (Some(id), Some(device_number), Some(mount_point), Some(options)) =
            (id, device_number, mount_point, options)
        else {
            continue;
        };
        // Optional fields run up to a lone `-`.
        let rest: Vec<&str> = fields.skip_while(|field| *field != "-").skip(1).collect();
        let (Some(fs_type), Some(device)) = (rest.first(), rest.get(1)) else {
            continue;
        };
        let mount_point = unescape(mount_point);
        let device = unescape(device);
        let fs_type = (*fs_type).to_string();
        let read_only = options.split(',').any(|option| option == "ro");
        let under_media = media_roots
            .iter()
            .any(|root| mount_point.starts_with(root.as_str()))
            || mount_point.starts_with("/run/media/")
            || mount_point.starts_with("/media/");
        let mounted_by_user = media_roots
            .iter()
            .any(|root| mount_point.starts_with(root.as_str()));
        let kind = classify(&fs_type, &device, under_media);
        let visible = is_listed(&fs_type, &device, &mount_point, kind, under_media);
        mounts.push(Mount {
            id,
            label: label_from_text(&mount_point, &device, kind),
            device,
            mount_point: PathBuf::from(mount_point),
            fs_type,
            kind,
            usage: Usage::Unknown,
            read_only,
            mounted_by_user,
            device_number,
            visible,
        });
    }
    // One drive, one row: bind mounts and subvolumes of a disk that is
    // listed already are hidden. The shortest mount point is the one kept.
    let mut best: HashMap<(u32, u32), usize> = HashMap::new();
    for (index, mount) in mounts.iter().enumerate() {
        if !mount.visible || mount.kind == MountKind::Network {
            continue;
        }
        best.entry(mount.device_number)
            .and_modify(|kept| {
                if mount.mount_point.as_os_str().len() < mounts[*kept].mount_point.as_os_str().len()
                {
                    *kept = index;
                }
            })
            .or_insert(index);
    }
    for (index, mount) in mounts.iter_mut().enumerate() {
        if mount.visible
            && mount.kind != MountKind::Network
            && best.get(&mount.device_number) != Some(&index)
        {
            mount.visible = false;
        }
    }
    mounts
}

fn classify(fs_type: &str, device: &str, under_media: bool) -> MountKind {
    if NETWORK.contains(&fs_type) {
        return MountKind::Network;
    }
    // Memory and kernel filesystems answer as fast as anything can.
    if PSEUDO.contains(&fs_type) && !fs_type.starts_with("fuse") {
        return MountKind::Local;
    }
    if fs_type.starts_with("fuse.")
        && !device.starts_with('/')
        && (device.contains(':') || device.contains('@'))
    {
        return MountKind::Network;
    }
    let name = device.rsplit('/').next().unwrap_or("");
    if matches!(fs_type, "iso9660" | "udf") || name.starts_with("sr") && device.starts_with("/dev/")
    {
        return MountKind::Optical;
    }
    if under_media {
        return MountKind::Removable;
    }
    if device.starts_with("/dev/") {
        MountKind::Local
    } else {
        MountKind::Other
    }
}

fn is_listed(
    fs_type: &str,
    device: &str,
    mount_point: &str,
    kind: MountKind,
    under_media: bool,
) -> bool {
    if PSEUDO.contains(&fs_type) {
        return false;
    }
    if mount_point == "/" {
        return true;
    }
    if under_media {
        return true;
    }
    if mount_point.starts_with("/run/") {
        return false;
    }
    if SYSTEM_ROOTS.iter().any(|root| {
        mount_point == *root
            || mount_point
                .strip_prefix(root)
                .is_some_and(|tail| tail.starts_with('/'))
    }) {
        return false;
    }
    match kind {
        MountKind::Network | MountKind::Optical | MountKind::Removable => true,
        MountKind::Local => !device.starts_with("/dev/loop") && !device.starts_with("/dev/zram"),
        // A FUSE filesystem is listed when a person is likely to have
        // mounted it (not an AppImage's own image).
        MountKind::Other => fs_type.starts_with("fuse.") && !mount_point.starts_with("/tmp/"),
    }
}

/// The label for a mount when no volume label is known: the mount point's
/// last part, for the root "File System", else the device.
fn label_from_text(mount_point: &str, device: &str, kind: MountKind) -> String {
    if mount_point == "/" {
        return "File System".into();
    }
    if let Some(name) = Path::new(mount_point)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
    {
        return name;
    }
    match kind {
        MountKind::Network => device.to_string(),
        _ => device.rsplit('/').next().unwrap_or(device).to_string(),
    }
}

/// `by-label` names escape awkward characters as `\xNN`.
pub(crate) fn unescape_udev(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut raw = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let value = (bytes[index] == b'\\'
            && bytes.get(index + 1) == Some(&b'x')
            && index + 3 < bytes.len())
        .then(|| std::str::from_utf8(&bytes[index + 2..index + 4]).ok())
        .flatten()
        .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match value {
            Some(value) => {
                raw.push(value);
                index += 4;
            }
            None => {
                raw.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

/// Volume labels by device name (`sdb1` to `My Disk`), from `/dev/disk/by-label`.
fn volume_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let Ok(reader) = fs::read_dir("/dev/disk/by-label") else {
        return labels;
    };
    for item in reader.flatten() {
        let Ok(target) = fs::read_link(item.path()) else {
            continue;
        };
        let (Some(device), name) = (
            target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            unescape_udev(&item.file_name().to_string_lossy()),
        ) else {
            continue;
        };
        if !name.is_empty() && !name.contains('/') {
            labels.insert(device, name);
        }
    }
    labels
}

/// Whether the kernel calls the block device `major:minor` removable. A
/// partition asks its disk.
pub(crate) fn sysfs_removable(device_number: (u32, u32)) -> bool {
    let base = format!("/sys/dev/block/{}:{}", device_number.0, device_number.1);
    let read = |path: String| fs::read_to_string(path).ok().map(|text| text.trim() == "1");
    read(format!("{base}/removable"))
        .or_else(|| read(format!("{base}/../removable")))
        .unwrap_or(false)
}

fn read_table() -> Vec<Mount> {
    let text = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    let mut mounts = parse_mountinfo(&text);
    let labels = volume_labels();
    for mount in &mut mounts {
        if !mount.visible {
            continue;
        }
        if mount.kind == MountKind::Local && sysfs_removable(mount.device_number) {
            mount.kind = MountKind::Removable;
        }
        if let Some(label) = mount
            .device
            .strip_prefix("/dev/")
            .and_then(|name| labels.get(name))
        {
            mount.label = label.clone();
        }
        mount.usage = cached_usage(&mount.mount_point).unwrap_or(Usage::Unknown);
    }
    mounts
}

/// Every mount, visible or not. Cheap; touches no mount.
pub fn all_mounts() -> Vec<Mount> {
    read_table()
}

/// The drives a person would list, in mount order. Cheap; touches no mount.
/// Sizes are the last known ones, or [`Usage::Unknown`]: call
/// [`fill_space`] on a worker to refresh them.
pub fn list_mounts() -> Vec<Mount> {
    let mut mounts = read_table();
    mounts.retain(|mount| mount.visible);
    mounts
}

/// The deepest mount that contains `path`, comparing text only.
pub fn mount_for<'a>(path: &Path, mounts: &'a [Mount]) -> Option<&'a Mount> {
    mounts
        .iter()
        .filter(|mount| path.starts_with(&mount.mount_point))
        .max_by_key(|mount| mount.mount_point.components().count())
}

static TABLE: Mutex<Option<(Instant, Arc<Vec<Mount>>)>> = Mutex::new(None);

fn table() -> Arc<Vec<Mount>> {
    let mut guard = TABLE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((read_at, table)) = guard.as_ref()
        && read_at.elapsed() < Duration::from_secs(2)
    {
        return table.clone();
    }
    let table = Arc::new(parse_mountinfo(
        &fs::read_to_string("/proc/self/mountinfo").unwrap_or_default(),
    ));
    *guard = Some((Instant::now(), table.clone()));
    table
}

/// The kind of the mount whose device is `dev`, as `DirStamp::dev` has it
/// (`major << 32 | minor`). This is how the loader finds the real drive of
/// a folder without following links: the folder's own `st_dev`.
pub fn kind_for_device(dev: u64) -> Option<MountKind> {
    let device = ((dev >> 32) as u32, dev as u32);
    table()
        .iter()
        .filter(|mount| mount.device_number == device)
        .map(|mount| mount.kind)
        .next()
}

/// The kind of the mount `path` is on. Reads the mount table (at most every
/// two seconds) and touches nothing else. A path on no known mount is
/// [`MountKind::Local`].
pub fn kind_of(path: &Path) -> MountKind {
    let table = table();
    mount_for(path, &table).map_or(MountKind::Local, |mount| mount.kind)
}

/// The mount `path` is on, with its kind, for callers that need more.
pub fn mount_of(path: &Path) -> Option<Mount> {
    let table = table();
    mount_for(path, &table).cloned()
}

// ---- durability -------------------------------------------------------------

/// Why writes to a drive are flushed to the device before an operation
/// reports that it is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlushReason {
    /// A USB stick, a card or an external disk: it may be pulled out the
    /// moment the file manager says it is done.
    Removable,
    /// A network share: the server may hold the data in a cache.
    Network,
    /// FAT, exFAT, NTFS and friends: no journal worth trusting, and the
    /// usual format of the media above.
    SimpleFilesystem,
}

/// File systems that are written back lazily and keep no journal of what
/// they were doing, so a pulled cable can leave them half-written.
const SIMPLE_FILESYSTEMS: &[&str] = &[
    "vfat",
    "msdos",
    "fat",
    "exfat",
    "ntfs",
    "ntfs3",
    "fuseblk",
    "fuse.ntfs-3g",
    "fuse.exfat",
    "hfs",
    "hfsplus",
];

/// The drive an operation writes to, when "done" has to mean "on the device".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteTarget {
    /// What to call it in a sentence: the volume label, else the mount
    /// point's name.
    pub label: String,
    pub reason: FlushReason,
    pub mount_point: PathBuf,
}

/// Decide from one mount whether writes must be flushed. Pure: `hotplug`
/// says whether the kernel puts the device on a USB bus or calls it
/// removable, and is only asked when the mount table cannot tell already.
/// A read-only mount takes no writes, so it needs no flush.
pub fn flush_reason(mount: &Mount, hotplug: impl FnOnce(&Mount) -> bool) -> Option<FlushReason> {
    if mount.read_only {
        return None;
    }
    match mount.kind {
        MountKind::Network => return Some(FlushReason::Network),
        MountKind::Removable => return Some(FlushReason::Removable),
        MountKind::Optical => return None,
        MountKind::Local | MountKind::Other => {}
    }
    if mount.device.starts_with("/dev/") && hotplug(mount) {
        return Some(FlushReason::Removable);
    }
    SIMPLE_FILESYSTEMS
        .contains(&mount.fs_type.as_str())
        .then_some(FlushReason::SimpleFilesystem)
}

/// [`write_target`] over a given mount table.
pub fn write_target_in(
    path: &Path,
    mounts: &[Mount],
    hotplug: impl FnOnce(&Mount) -> bool,
) -> Option<WriteTarget> {
    let mount = mount_for(path, mounts)?;
    let reason = flush_reason(mount, hotplug)?;
    Some(WriteTarget {
        label: mount.label.clone(),
        reason,
        mount_point: mount.mount_point.clone(),
    })
}

/// Whether writes under `path` must reach the device before an operation
/// says it is done: removable media (by the mount table, the device's
/// `removable` flag or a USB parent in sysfs), network shares, and FAT,
/// exFAT and NTFS. `None` for an ordinary disk. Reads the mount table (at
/// most every two seconds) and `/sys`; it never touches the drive.
pub fn write_target(path: &Path) -> Option<WriteTarget> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let table = table();
    let mut target = write_target_in(&path, &table, sysfs_hotplug)?;
    if let Some(mount) = mount_for(&path, &table)
        && let Some(name) = mount.device.strip_prefix("/dev/")
        && let Some(label) = volume_labels().get(name)
    {
        target.label = label.clone();
    }
    Some(target)
}

/// The `major:minor` of the block device under `mount`. Btrfs and friends
/// report an anonymous device in the mount table, so the device node is
/// asked instead.
fn block_device_of(mount: &Mount) -> Option<(u32, u32)> {
    if mount.device_number.0 != 0 {
        return Some(mount.device_number);
    }
    use std::os::unix::fs::MetadataExt;
    let rdev = fs::metadata(&mount.device).ok()?.rdev();
    Some((libc::major(rdev), libc::minor(rdev)))
}

/// Is the block device `major:minor` removable, on a USB bus, or built on
/// something that is (an encrypted or RAID layer reads through `slaves`)?
fn hotplug_device(device: (u32, u32), depth: u32) -> bool {
    if sysfs_removable(device) {
        return true;
    }
    let base = PathBuf::from(format!("/sys/dev/block/{}:{}", device.0, device.1));
    if let Ok(real) = fs::canonicalize(&base)
        && real.components().any(|part| {
            part.as_os_str().to_str().is_some_and(|name| {
                name.strip_prefix("usb").is_some_and(|rest| {
                    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
                })
            })
        })
    {
        return true;
    }
    if depth >= 3 {
        return false;
    }
    let Ok(slaves) = fs::read_dir(base.join("slaves")) else {
        return false;
    };
    slaves.flatten().any(|slave| {
        let number = fs::read_to_string(slave.path().join("dev")).ok();
        number
            .as_deref()
            .and_then(|text| text.trim().split_once(':'))
            .and_then(|(major, minor)| Some((major.parse().ok()?, minor.parse().ok()?)))
            .is_some_and(|device| hotplug_device(device, depth + 1))
    })
}

fn sysfs_hotplug(mount: &Mount) -> bool {
    block_device_of(mount).is_some_and(|device| hotplug_device(device, 0))
}

/// Write everything the kernel holds for the file system under `path` to
/// its device (`syncfs(2)`) and wait for it.
pub fn sync_filesystem(path: &Path) -> io::Result<()> {
    let handle = fs::File::open(path)?;
    // SAFETY: the descriptor is open for the whole call.
    if unsafe { libc::syncfs(handle.as_raw_fd()) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

// ---- sizes ----------------------------------------------------------------

struct Cached {
    usage: Usage,
    at: Instant,
}

static SPACE: Mutex<Option<HashMap<PathBuf, Cached>>> = Mutex::new(None);
static STUCK: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

fn cached_usage(mount_point: &Path) -> Option<Usage> {
    let guard = SPACE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref()?.get(mount_point).map(|cached| cached.usage)
}

fn cache_fresh(mount_point: &Path, kind: MountKind) -> Option<Usage> {
    let guard = SPACE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cached = guard.as_ref()?.get(mount_point)?;
    let ttl = if kind == MountKind::Network {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(5)
    };
    (cached.at.elapsed() < ttl && cached.usage != Usage::NotResponding).then_some(cached.usage)
}

fn remember(mount_point: &Path, usage: Usage) {
    let mut guard = SPACE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get_or_insert_with(HashMap::new).insert(
        mount_point.to_path_buf(),
        Cached {
            usage,
            at: Instant::now(),
        },
    );
}

/// `statvfs` on `path`. Blocks as long as the mount does.
fn statvfs(path: &Path) -> Usage {
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return Usage::Unknown;
    };
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `buf` is
    // plain data that `statvfs` fills on success.
    unsafe {
        let mut buf: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut buf) != 0 {
            return Usage::Unknown;
        }
        let unit = if buf.f_frsize > 0 {
            buf.f_frsize
        } else {
            buf.f_bsize
        };
        let total = u64::from(buf.f_blocks).saturating_mul(unit as u64);
        if total == 0 {
            return Usage::Unknown;
        }
        Usage::Known {
            total,
            free: u64::from(buf.f_bavail).saturating_mul(unit as u64),
        }
    }
}

/// Ask each mount how big it is, giving up on slow ones after `deadline`.
/// Blocks for at most about `deadline`; call it on a worker. Mounts whose
/// answer is cached (5 s for local disks, 30 s for network ones) are not
/// asked again. A mount that did not answer is [`Usage::NotResponding`] and
/// is not asked again until its stuck call returns.
pub fn fill_space(mounts: &mut [Mount], deadline: Duration) {
    let (sender, receiver) = mpsc::channel::<(usize, Usage)>();
    let mut waiting = 0;
    for (index, mount) in mounts.iter_mut().enumerate() {
        if let Some(usage) = cache_fresh(&mount.mount_point, mount.kind) {
            mount.usage = usage;
            continue;
        }
        let stuck = STUCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .is_some_and(|set| set.contains(&mount.mount_point));
        if stuck {
            mount.usage = Usage::NotResponding;
            continue;
        }
        mount.usage = Usage::Pending;
        let path = mount.mount_point.clone();
        let sender = sender.clone();
        STUCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_or_insert_with(HashSet::new)
            .insert(path.clone());
        let spawned = thread::Builder::new()
            .name("bingux-files-statvfs".into())
            .spawn(move || {
                let usage = statvfs(&path);
                remember(&path, usage);
                if let Some(set) = STUCK
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_mut()
                {
                    set.remove(&path);
                }
                let _ = sender.send((index, usage));
            });
        if spawned.is_ok() {
            waiting += 1;
        } else {
            mount.usage = Usage::Unknown;
        }
    }
    drop(sender);
    let end = Instant::now() + deadline;
    while waiting > 0 {
        let left = end.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(left) {
            Ok((index, usage)) => {
                mounts[index].usage = usage;
                waiting -= 1;
            }
            Err(_) => break,
        }
    }
    for mount in mounts.iter_mut() {
        if mount.usage == Usage::Pending {
            mount.usage = Usage::NotResponding;
        }
    }
}

/// [`list_mounts`] and [`fill_space`] in one call, for a worker.
pub fn mounts_with_space(deadline: Duration) -> Vec<Mount> {
    let mut mounts = list_mounts();
    fill_space(&mut mounts, deadline);
    mounts
}

// ---- change notification ---------------------------------------------------

/// Calls back when the mount table changes. Dropping it stops the thread.
pub struct MountWatcher {
    stop: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

/// Watch `/proc/self/mountinfo` and `/dev/disk/by-uuid` (udev adds and removes
/// a link there when a drive with a file system is plugged in or pulled
/// out, mounted or not, so a stick that has not been opened shows up). Both
/// are waited for in the kernel, so an idle watcher uses no CPU. `on_change`
/// runs on the watcher's own thread once things have been quiet for 100 ms,
/// so plugging in a disk that mounts three partitions calls it once.
pub fn watch_mounts(on_change: impl Fn() + Send + 'static) -> io::Result<MountWatcher> {
    let file = fs::File::open("/proc/self/mountinfo")?;
    // SAFETY: a plain syscall; the descriptor is checked and owned once.
    let stop = unsafe {
        let fd = libc::eventfd(0, libc::EFD_CLOEXEC);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        OwnedFd::from_raw_fd(fd)
    };
    let stop_fd = stop.as_raw_fd();
    let devices = watch_devices();
    let thread = thread::Builder::new()
        .name("bingux-files-mounts".into())
        .spawn(move || watch_loop(file, stop_fd, devices, on_change))?;
    Ok(MountWatcher {
        stop,
        thread: Some(thread),
    })
}

/// An inotify descriptor on `/dev/disk/by-uuid`, or `None` where there is
/// none (a container, a machine with no udev).
fn watch_devices() -> Option<OwnedFd> {
    // SAFETY: plain syscalls; the descriptor is checked and owned once.
    unsafe {
        let fd = libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK);
        if fd < 0 {
            return None;
        }
        let owned = OwnedFd::from_raw_fd(fd);
        let path = CString::new("/dev/disk/by-uuid").ok()?;
        let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO;
        (libc::inotify_add_watch(fd, path.as_ptr(), mask) >= 0).then_some(owned)
    }
}

/// Read and drop whatever inotify has queued.
fn drain_events(fd: i32) {
    let mut sink = [0_u8; 4096];
    loop {
        // SAFETY: `sink` is a live buffer of the length given.
        let read = unsafe { libc::read(fd, sink.as_mut_ptr().cast(), sink.len()) };
        if read <= 0 {
            return;
        }
    }
}

fn watch_loop(mut file: fs::File, stop: i32, devices: Option<OwnedFd>, on_change: impl Fn()) {
    use std::io::{Read, Seek, SeekFrom};
    let drain = |file: &mut fs::File| {
        let _ = file.seek(SeekFrom::Start(0));
        let mut sink = Vec::new();
        let _ = file.read_to_end(&mut sink);
    };
    drain(&mut file);
    // A negative descriptor is skipped by `poll`.
    let device_fd = devices.as_ref().map_or(-1, AsRawFd::as_raw_fd);
    loop {
        let mut fds = [
            libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLPRI | libc::POLLERR,
                revents: 0,
            },
            libc::pollfd {
                fd: stop,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: device_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` is a valid array of three `pollfd`s.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 3, -1) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        // Wait for quiet, but wake at once to stop.
        let mut quiet = [libc::pollfd {
            fd: stop,
            events: libc::POLLIN,
            revents: 0,
        }];
        // SAFETY: `quiet` is a valid array of one `pollfd`.
        if unsafe { libc::poll(quiet.as_mut_ptr(), 1, 100) } > 0 {
            return;
        }
        drain(&mut file);
        if device_fd >= 0 {
            drain_events(device_fd);
        }
        on_change();
    }
}

impl Drop for MountWatcher {
    fn drop(&mut self) {
        let one: u64 = 1;
        // SAFETY: writes 8 bytes from a live `u64` to an eventfd we own.
        unsafe {
            libc::write(self.stop.as_raw_fd(), (&raw const one).cast(), 8);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// ---- unmount ---------------------------------------------------------------

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                format!("{program} is not installed.")
            } else {
                format!("{program} could not run: {error}")
            }
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    Err(if first.is_empty() {
        format!("{program} failed.")
    } else {
        let mut sentence = first.to_string();
        if !sentence.ends_with('.') {
            sentence.push('.');
        }
        sentence
    })
}

/// Unmount `mount`. Blocks: call it on a worker.
///
/// The file system is written out first (`syncfs`). If that fails, nothing
/// has been detached and the error says the data could not be written, so
/// the drive is not reported as safe to remove. Disks the person mounted and
/// network shares then go through `udisksctl` or `umount`; a busy drive
/// keeps the real message of the tool, which is the only thing that knows.
pub fn unmount(mount: &Mount) -> Result<(), String> {
    if !mount.can_unmount() {
        return Err("This drive is in use by the system and cannot be unmounted.".into());
    }
    if !mount.read_only {
        sync_filesystem(&mount.mount_point).map_err(|error| {
            let text = error.to_string();
            let text = text.split(" (os error").next().unwrap_or(&text);
            format!("The data could not be written to the drive: {text}.")
        })?;
    }
    let device = std::ffi::OsString::from(&mount.device);
    let via_udisks = if mount.device.starts_with("/dev/") {
        match run(
            "udisksctl",
            &[
                std::ffi::OsStr::new("unmount"),
                std::ffi::OsStr::new("-b"),
                &device,
            ],
        ) {
            Ok(()) => return Ok(()),
            Err(error) => Some(error),
        }
    } else {
        None
    };
    run("umount", &[mount.mount_point.as_os_str()])
        .map_err(|plain| pick_unmount_error(via_udisks, plain))
}

/// Which of two failures to show. `umount` run by a person who is not root
/// fails with a permission message whatever the real trouble is, so the
/// message of `udisksctl`, which ran with the right to unmount, wins then.
fn pick_unmount_error(via_udisks: Option<String>, via_umount: String) -> String {
    let lower = via_umount.to_lowercase();
    let permission = lower.contains("superuser")
        || lower.contains("permission denied")
        || lower.contains("not permitted");
    match via_udisks {
        Some(udisks) if permission && !udisks.ends_with("is not installed.") => udisks,
        _ => via_umount,
    }
}

/// Unmount and, where the drive allows it, eject or power it off.
pub fn eject(mount: &Mount) -> Result<(), String> {
    unmount(mount)?;
    if mount.kind == MountKind::Optical {
        let device = std::ffi::OsString::from(&mount.device);
        return run("eject", &[&device]);
    }
    if mount.kind == MountKind::Removable && mount.device.starts_with("/dev/") {
        let disk = whole_disk(&mount.device);
        let disk = std::ffi::OsString::from(disk);
        // A drive that cannot be powered off is still safe to remove.
        let _ = run(
            "udisksctl",
            &[
                std::ffi::OsStr::new("power-off"),
                std::ffi::OsStr::new("-b"),
                &disk,
            ],
        );
    }
    Ok(())
}

/// `/dev/sdb1` is on `/dev/sdb`; `/dev/mmcblk0p1` on `/dev/mmcblk0`.
fn whole_disk(device: &str) -> String {
    let name = device.trim_start_matches("/dev/");
    let stem = if name.starts_with("mmcblk") || name.starts_with("nvme") {
        name.rsplit_once('p').map_or(name, |(disk, _)| disk)
    } else {
        name.trim_end_matches(|c: char| c.is_ascii_digit())
    };
    format!("/dev/{stem}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
42 1 0:35 / / rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw,subvolid=5
44 42 0:26 / /sys rw,nosuid shared:5 - sysfs sysfs rw
52 42 0:29 / /run rw,nosuid shared:14 - tmpfs tmpfs rw
54 42 0:44 / /tmp rw,nosuid shared:77 - tmpfs tmpfs rw
57 42 259:4 / /boot rw,relatime shared:80 - ext4 /dev/nvme0n1p2 rw
60 57 259:3 / /boot/efi rw,relatime shared:83 - vfat /dev/nvme0n1p1 rw
63 42 0:45 / /home rw,relatime shared:86 - btrfs /dev/mapper/luks-affa rw
2456 42 0:35 /var/tmp/cache /var/tmp/work/var/cache/apk rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw
1833 42 0:107 / /mnt/kieran rw,relatime shared:1462 - nfs4 192.168.0.39:/ rw,vers=4.2
2082 789 0:133 / /run/user/1000/gvfs rw,nosuid shared:2441 - fuse.gvfsd-fuse gvfsd-fuse rw
3000 42 8:17 / /run/media/kieran/My\\040Disk rw,nosuid,nodev shared:3 - vfat /dev/sdb1 rw
3001 42 11:0 / /run/media/kieran/DISC ro,nosuid shared:4 - iso9660 /dev/sr0 ro
3002 42 0:150 / /mnt/nas rw shared:5 - fuse.sshfs me@nas:/srv rw,user_id=1000
3003 42 0:151 / /mnt/enc rw shared:6 - fuse.gocryptfs gocryptfs rw
3004 42 7:0 / /mnt/snap ro shared:7 - squashfs /dev/loop0 ro
";

    fn parsed() -> Vec<Mount> {
        parse_with(SAMPLE, &["/run/media/kieran/".to_string()])
    }

    fn listed(mounts: &[Mount]) -> Vec<(&str, MountKind)> {
        mounts
            .iter()
            .filter(|mount| mount.visible)
            .map(|mount| (mount.mount_point.to_str().unwrap(), mount.kind))
            .collect()
    }

    #[test]
    fn lists_drives_and_hides_the_system() {
        let mounts = parsed();
        assert_eq!(
            listed(&mounts),
            [
                ("/", MountKind::Local),
                ("/home", MountKind::Local),
                ("/mnt/kieran", MountKind::Network),
                ("/run/media/kieran/My Disk", MountKind::Removable),
                ("/run/media/kieran/DISC", MountKind::Optical),
                ("/mnt/nas", MountKind::Network),
                ("/mnt/enc", MountKind::Other),
            ]
        );
        assert_eq!(mounts.len(), 15);
    }

    #[test]
    fn describes_a_mount() {
        let mounts = parsed();
        let nfs = mounts
            .iter()
            .find(|m| m.mount_point == Path::new("/mnt/kieran"))
            .unwrap();
        assert_eq!(
            (nfs.id, nfs.fs_type.as_str(), nfs.device.as_str()),
            (1833, "nfs4", "192.168.0.39:/")
        );
        assert_eq!(nfs.label, "kieran");
        let usb = mounts.iter().find(|m| m.device == "/dev/sdb1").unwrap();
        assert!(usb.mounted_by_user && !usb.read_only);
        assert_eq!(usb.label, "My Disk");
        let disc = mounts.iter().find(|m| m.device == "/dev/sr0").unwrap();
        assert!(disc.read_only && disc.can_unmount());
        let root = mounts
            .iter()
            .find(|m| m.mount_point == Path::new("/"))
            .unwrap();
        assert!(!root.can_unmount());
        assert_eq!(root.label, "File System");
    }

    #[test]
    fn finds_the_deepest_mount() {
        let mounts = parsed();
        let found = |path: &str| {
            mount_for(Path::new(path), &mounts).map(|m| m.mount_point.to_str().unwrap())
        };
        assert_eq!(found("/mnt/kieran/home/x"), Some("/mnt/kieran"));
        assert_eq!(found("/home/a"), Some("/home"));
        assert_eq!(found("/usr/bin"), Some("/"));
        assert_eq!(found("/boot/efi/EFI"), Some("/boot/efi"));
    }

    fn target(path: &str, hotplug: bool) -> Option<(FlushReason, String)> {
        write_target_in(Path::new(path), &parsed(), |_| hotplug)
            .map(|target| (target.reason, target.label))
    }

    #[test]
    fn writes_to_removable_media_are_flushed() {
        assert_eq!(
            target("/run/media/kieran/My Disk/photos", false),
            Some((FlushReason::Removable, "My Disk".to_owned()))
        );
    }

    #[test]
    fn writes_to_a_share_are_flushed() {
        assert_eq!(
            target("/mnt/kieran/docs", false).map(|t| t.0),
            Some(FlushReason::Network)
        );
        assert_eq!(
            target("/mnt/nas/x", false).map(|t| t.0),
            Some(FlushReason::Network)
        );
    }

    #[test]
    fn an_internal_disk_is_not_flushed() {
        assert_eq!(target("/home/a/file", false), None);
        assert_eq!(target("/usr/share", false), None);
        // The kernel's own opinion is only asked for real block devices.
        assert_eq!(target("/mnt/enc/x", true), None);
    }

    #[test]
    fn a_usb_disk_mounted_by_hand_is_removable_by_the_kernel_flag() {
        // Not under /run/media, so the mount table alone says Local.
        let text = "70 42 8:33 / /mnt/usbdisk rw,relatime shared:9 - ext4 /dev/sdc1 rw\n";
        let mounts = parse_with(text, &[]);
        let reason = |hotplug: bool| {
            write_target_in(Path::new("/mnt/usbdisk/a"), &mounts, |_| hotplug).map(|t| t.reason)
        };
        assert_eq!(reason(false), None);
        assert_eq!(reason(true), Some(FlushReason::Removable));
    }

    #[test]
    fn fat_exfat_and_ntfs_are_flushed_even_when_internal() {
        for fs_type in ["vfat", "exfat", "ntfs3", "fuseblk"] {
            let text = format!("71 42 8:34 / /mnt/data rw shared:9 - {fs_type} /dev/sdd1 rw\n");
            let mounts = parse_with(&text, &[]);
            let target = write_target_in(Path::new("/mnt/data/x"), &mounts, |_| false);
            assert_eq!(
                target.map(|t| t.reason),
                Some(FlushReason::SimpleFilesystem),
                "{fs_type}"
            );
        }
    }

    #[test]
    fn a_read_only_mount_takes_no_flush() {
        assert_eq!(target("/run/media/kieran/DISC/a", true), None);
        assert_eq!(target("/mnt/snap/a", true), None);
    }

    #[test]
    fn a_busy_drive_keeps_the_tools_own_message() {
        let busy = "umount: /mnt/x: target is busy.".to_owned();
        assert_eq!(pick_unmount_error(None, busy.clone()), busy);
        assert_eq!(
            pick_unmount_error(
                Some("Error unmounting: target is busy.".into()),
                busy.clone()
            ),
            busy
        );
        let denied = "umount: /mnt/x: must be superuser to unmount.".to_owned();
        assert_eq!(
            pick_unmount_error(Some("Error unmounting: target is busy.".into()), denied),
            "Error unmounting: target is busy."
        );
    }

    #[test]
    fn usage_is_a_fraction() {
        let mut mount = parsed().remove(0);
        assert_eq!(mount.usage_fraction(), None);
        mount.usage = Usage::Known {
            total: 200,
            free: 50,
        };
        assert_eq!(mount.usage_fraction(), Some(0.75));
        assert_eq!(
            (mount.total_bytes(), mount.free_bytes()),
            (Some(200), Some(50))
        );
        mount.usage = Usage::NotResponding;
        assert_eq!(mount.usage_fraction(), None);
    }

    #[test]
    fn udev_labels_are_unescaped() {
        assert_eq!(unescape_udev("My\\x20Disk"), "My Disk");
        assert_eq!(unescape_udev("\\x2f"), "/");
    }

    #[test]
    fn disks_are_named_from_partitions() {
        assert_eq!(whole_disk("/dev/sdb1"), "/dev/sdb");
        assert_eq!(whole_disk("/dev/mmcblk0p1"), "/dev/mmcblk0");
        assert_eq!(whole_disk("/dev/nvme0n1p3"), "/dev/nvme0n1");
    }

    #[test]
    fn the_real_table_has_a_root() {
        let all = all_mounts();
        assert!(all.iter().any(|mount| mount.mount_point == Path::new("/")));
        let mut listed = list_mounts();
        fill_space(&mut listed, Duration::from_secs(5));
        let root = listed
            .iter()
            .find(|mount| mount.mount_point == Path::new("/"))
            .unwrap();
        assert!(root.total_bytes().is_some());
    }

    #[test]
    fn watcher_stops_when_dropped() {
        let watcher = watch_mounts(|| {}).unwrap();
        drop(watcher);
    }
}
