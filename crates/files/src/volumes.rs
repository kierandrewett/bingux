//! Drives that are plugged in and not mounted: a USB stick that has not been
//! opened yet, a card in a reader.
//!
//! [`list_volumes`] finds them without asking any service: `/dev/disk/by-uuid`
//! names every device that holds a file system, `/sys` says how big it is and
//! whether it is removable, and udev's own database (`/run/udev/data`) says
//! what the file system is and whether it is a bus the person plugged
//! something into. Nothing here opens a device. A volume that is mounted is
//! not listed; it is a [`Mount`](crate::mounts::Mount) then.
//!
//! [`mount_volume`] asks `udisksctl`, which mounts it under
//! `/run/media/$USER` with the person's own rights (no password for a stick
//! in a seat that is theirs) and tells where.
//!
//! Only drives a person plugs in are listed. The partitions of the machine's
//! own disks (another system's root, a recovery partition) stay out of the
//! sidebar: they ask for a password, and nobody looks for them there.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::display;
use crate::mounts::{Mount, MountKind, sysfs_removable, unescape_udev};

/// The type of the EFI system partition in a GPT.
const EFI_SYSTEM: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

/// A plugged-in file system that nothing has mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// `/dev/sdb1`.
    pub device: String,
    /// The volume's label, else its size ("16 GB Volume").
    pub label: String,
    pub fs_type: String,
    /// Bytes.
    pub size: u64,
    /// The `major:minor` of the device.
    pub device_number: (u32, u32),
    pub kind: MountKind,
}

impl Volume {
    /// A number that names this volume while it is being mounted, apart from
    /// the kernel's mount ids (which this one never equals: the top bit is
    /// set).
    pub fn busy_id(&self) -> u32 {
        0x8000_0000 | (self.device_number.0 << 12) | (self.device_number.1 & 0xfff)
    }
}

/// What udev knows about one block device: the `E:` lines of its database
/// entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Udev {
    pub fs_usage: Option<String>,
    pub fs_type: Option<String>,
    pub fs_label: Option<String>,
    pub bus: Option<String>,
    pub partition_type: Option<String>,
    pub ignored: bool,
    /// A card of flash media (SD, MMC, Memory Stick), which is removable
    /// whatever bus its reader sits on.
    pub flash_card: bool,
    pub optical_media: bool,
}

/// Read the `E:KEY=VALUE` lines of a udev database entry.
pub fn parse_udev(text: &str) -> Udev {
    let mut info = Udev::default();
    for line in text.lines() {
        let Some((key, value)) = line.strip_prefix("E:").and_then(|rest| rest.split_once('=')) else {
            continue;
        };
        match key {
            "ID_FS_USAGE" => info.fs_usage = Some(value.to_string()),
            "ID_FS_TYPE" => info.fs_type = Some(value.to_string()),
            "ID_FS_LABEL" => info.fs_label = Some(value.to_string()),
            "ID_BUS" => info.bus = Some(value.to_string()),
            "ID_PART_ENTRY_TYPE" => info.partition_type = Some(value.to_ascii_lowercase()),
            "UDISKS_IGNORE" | "UDISKS_SYSTEM" => info.ignored |= value == "1",
            "ID_DRIVE_FLASH_SD" | "ID_DRIVE_MEDIA_FLASH_SD" | "ID_DRIVE_FLASH_MS"
            | "ID_DRIVE_FLASH_SM" | "ID_DRIVE_FLASH_CF" => info.flash_card |= value == "1",
            "ID_CDROM_MEDIA" => info.optical_media |= value == "1",
            _ => {}
        }
    }
    info
}

/// What kind of drive a device is, or `None` when it is not one a person
/// plugged in and can mount.
pub fn volume_kind(info: &Udev, removable_flag: bool) -> Option<MountKind> {
    if info.fs_usage.as_deref() != Some("filesystem") || info.ignored {
        return None;
    }
    if info.partition_type.as_deref() == Some(EFI_SYSTEM) {
        return None;
    }
    if info.optical_media {
        return Some(MountKind::Optical);
    }
    (removable_flag || info.bus.as_deref() == Some("usb") || info.flash_card)
        .then_some(MountKind::Removable)
}

/// The volumes that are plugged in and not in `mounts`. Cheap: a directory
/// listing and a few small files per volume; no device is opened.
pub fn list_volumes(mounts: &[Mount]) -> Vec<Volume> {
    let taken: HashSet<(u32, u32)> = mounts.iter().map(|mount| mount.device_number).collect();
    let Ok(entries) = fs::read_dir("/dev/disk/by-uuid") else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut volumes = Vec::new();
    for entry in entries.flatten() {
        let Ok(target) = fs::read_link(entry.path()) else {
            continue;
        };
        let Some(name) = target.file_name() else {
            continue;
        };
        let device = Path::new("/dev").join(name);
        let Some(number) = device_number(&device) else {
            continue;
        };
        if taken.contains(&number) || !seen.insert(number) {
            continue;
        }
        if let Some(volume) = volume_at(&device, number) {
            volumes.push(volume);
        }
    }
    volumes.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    volumes
}

fn volume_at(device: &Path, number: (u32, u32)) -> Option<Volume> {
    let text = fs::read_to_string(format!("/run/udev/data/b{}:{}", number.0, number.1)).ok()?;
    let info = parse_udev(&text);
    let kind = volume_kind(&info, sysfs_removable(number))?;
    let sectors: u64 = fs::read_to_string(format!("/sys/dev/block/{}:{}/size", number.0, number.1))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let size = sectors * 512;
    let label = info
        .fs_label
        .as_deref()
        .map(unescape_udev)
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| format!("{} Volume", display::size(size)));
    Some(Volume {
        device: device.to_string_lossy().into_owned(),
        label,
        fs_type: info.fs_type.unwrap_or_default(),
        size,
        device_number: number,
        kind,
    })
}

/// `major:minor` of a block device file.
fn device_number(device: &Path) -> Option<(u32, u32)> {
    let rdev = fs::metadata(device).ok()?.rdev();
    Some((libc::major(rdev), libc::minor(rdev)))
}

/// Mount `volume` and return where it is mounted. Blocks: call it on a worker.
/// A failure is a sentence in the words of `udisksctl`.
pub fn mount_volume(volume: &Volume) -> Result<PathBuf, String> {
    let output = std::process::Command::new("udisksctl")
        .args(["mount", "--no-user-interaction", "-b", &volume.device])
        .output()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                "udisksctl is not installed, so drives cannot be mounted here.".to_string()
            } else {
                format!("udisksctl could not run: {error}.")
            }
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return parse_mounted(&stdout)
            .ok_or_else(|| format!("{} was mounted, but not where it could be found.", volume.label));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(failure_sentence(&stderr, &volume.label))
}

/// The folder in `Mounted /dev/sdb1 at /run/media/me/STICK.`
pub fn parse_mounted(stdout: &str) -> Option<PathBuf> {
    let line = stdout.lines().find(|line| line.starts_with("Mounted "))?;
    let (_, at) = line.split_once(" at ")?;
    let at = at.trim();
    let at = at.strip_suffix('.').unwrap_or(at);
    (!at.is_empty()).then(|| PathBuf::from(at))
}

/// One sentence for what `udisksctl` said when it failed: the part after
/// the last `GDBus.Error:` or `Error mounting ...:`.
pub fn failure_sentence(stderr: &str, label: &str) -> String {
    let first = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let message = first
        .rsplit_once("GDBus.Error:")
        .map_or(first, |(_, rest)| rest)
        .trim();
    // `org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain: Not authorized...`
    let message = match message.split_once(": ") {
        Some((name, rest)) if name.starts_with("org.freedesktop.") => rest,
        _ => message,
    };
    if message.is_empty() {
        return format!("{label} could not be mounted.");
    }
    let mut sentence = message.to_string();
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

#[cfg(test)]
mod tests {
    use super::*;

    const STICK: &str = "\
S:disk/by-uuid/1234-ABCD
E:ID_BUS=usb
E:ID_FS_LABEL=My Stick
E:ID_FS_TYPE=vfat
E:ID_FS_USAGE=filesystem
E:ID_PART_ENTRY_TYPE=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
";

    #[test]
    fn a_usb_stick_is_a_removable_volume() {
        let info = parse_udev(STICK);
        assert_eq!(info.fs_label.as_deref(), Some("My Stick"));
        assert_eq!(volume_kind(&info, false), Some(MountKind::Removable));
    }

    #[test]
    fn an_internal_partition_is_not_listed() {
        let info = parse_udev("E:ID_BUS=nvme\nE:ID_FS_TYPE=ext4\nE:ID_FS_USAGE=filesystem\n");
        assert_eq!(volume_kind(&info, false), None);
    }

    #[test]
    fn the_kernels_removable_flag_counts() {
        let info = parse_udev("E:ID_BUS=ata\nE:ID_FS_TYPE=exfat\nE:ID_FS_USAGE=filesystem\n");
        assert_eq!(volume_kind(&info, true), Some(MountKind::Removable));
    }

    #[test]
    fn swap_and_encrypted_volumes_are_not_mountable() {
        for usage in ["other", "crypto", "raid"] {
            let info = parse_udev(&format!("E:ID_BUS=usb\nE:ID_FS_USAGE={usage}\n"));
            assert_eq!(volume_kind(&info, true), None);
        }
    }

    #[test]
    fn the_efi_partition_of_a_stick_is_not_listed() {
        let info = parse_udev(&format!(
            "E:ID_BUS=usb\nE:ID_FS_USAGE=filesystem\nE:ID_PART_ENTRY_TYPE={}\n",
            EFI_SYSTEM.to_ascii_uppercase()
        ));
        assert_eq!(volume_kind(&info, true), None);
    }

    #[test]
    fn an_ignored_device_is_not_listed() {
        let info = parse_udev("E:ID_BUS=usb\nE:ID_FS_USAGE=filesystem\nE:UDISKS_IGNORE=1\n");
        assert_eq!(volume_kind(&info, true), None);
    }

    #[test]
    fn a_card_reader_is_removable_on_any_bus() {
        let info = parse_udev("E:ID_FS_USAGE=filesystem\nE:ID_DRIVE_FLASH_SD=1\n");
        assert_eq!(volume_kind(&info, false), Some(MountKind::Removable));
    }

    #[test]
    fn a_disc_is_optical() {
        let info = parse_udev("E:ID_FS_USAGE=filesystem\nE:ID_CDROM_MEDIA=1\n");
        assert_eq!(volume_kind(&info, true), Some(MountKind::Optical));
    }

    #[test]
    fn udisks_says_where_it_mounted() {
        assert_eq!(
            parse_mounted("Mounted /dev/sdb1 at /run/media/me/My Stick.\n"),
            Some(PathBuf::from("/run/media/me/My Stick"))
        );
        assert_eq!(parse_mounted("nothing"), None);
    }

    #[test]
    fn a_failure_is_one_sentence() {
        assert_eq!(
            failure_sentence(
                "Error mounting /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorizedCanObtain: Not authorized to perform operation\n",
                "Stick"
            ),
            "Not authorized to perform operation."
        );
        assert_eq!(failure_sentence("", "Stick"), "Stick could not be mounted.");
    }

    #[test]
    fn busy_ids_never_meet_mount_ids() {
        let volume = Volume {
            device: "/dev/sdb1".into(),
            label: "x".into(),
            fs_type: "vfat".into(),
            size: 0,
            device_number: (8, 17),
            kind: MountKind::Removable,
        };
        assert!(volume.busy_id() >= 0x8000_0000);
    }
}
