//! The places a chooser's sidebar lists.
//!
//! Three groups, in the order GTK and GNOME Files use: the standard places
//! (Recent, Home and the XDG user folders), the devices (the file system and
//! mounted volumes) and the user's bookmarks.
//!
//! [`Sources`] holds the text of the files the places come from, so
//! [`Places::build`] is a pure function that tests and other front ends can
//! feed. [`Places::load`] reads the real system and touches the disk, so call
//! it on a worker.

use std::fs;
use std::path::{Path, PathBuf};

use crate::location::{from_file_uri, normalize};

/// What a place is, which picks its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    Recent,
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    /// `/`.
    Computer,
    /// A mounted volume.
    Volume,
    Bookmark,
}

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub kind: PlaceKind,
    pub name: String,
    /// The folder. `None` for Recent, which has no folder.
    pub path: Option<PathBuf>,
}

/// The sidebar's three groups.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    pub standard: Vec<Place>,
    pub devices: Vec<Place>,
    pub bookmarks: Vec<Place>,
}

/// The raw text places are built from. A missing file is `None`.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub home: PathBuf,
    /// `~/.config/user-dirs.dirs`.
    pub user_dirs: Option<String>,
    /// `/proc/mounts`.
    pub mounts: Option<String>,
    /// `~/.config/gtk-3.0/bookmarks`.
    pub bookmarks: Option<String>,
}

impl Sources {
    /// The files of the running system. `$HOME` is the home folder.
    pub fn from_system() -> Sources {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        Sources {
            user_dirs: fs::read_to_string(config.join("user-dirs.dirs")).ok(),
            mounts: fs::read_to_string("/proc/mounts").ok(),
            bookmarks: fs::read_to_string(config.join("gtk-3.0/bookmarks")).ok(),
            home,
        }
    }
}

/// The folder an XDG user directory names on this system (`Pictures` for
/// [`PlaceKind::Pictures`]), without checking that it exists. `None` for a
/// kind that is not a user directory. It reads a few small files, so a
/// caller about to open a chooser may call it on the UI thread.
pub fn user_dir(kind: PlaceKind) -> Option<PathBuf> {
    let sources = Sources::from_system();
    user_dirs(sources.user_dirs.as_deref(), &sources.home)
        .into_iter()
        .find(|(candidate, _, _)| *candidate == kind)
        .map(|(_, _, path)| path)
}

impl Places {
    /// The places of this system.
    pub fn load() -> Places {
        let sources = Sources::from_system();
        Places::build(&sources, |path| path.is_dir())
    }

    /// Build the places from `sources`. `is_dir` says whether a folder
    /// exists, so places that point nowhere are left out.
    pub fn build(sources: &Sources, is_dir: impl Fn(&Path) -> bool) -> Places {
        let home = &sources.home;
        let mut standard = vec![
            Place {
                kind: PlaceKind::Recent,
                name: "Recent".into(),
                path: None,
            },
            Place {
                kind: PlaceKind::Home,
                name: "Home".into(),
                path: Some(home.clone()),
            },
        ];
        let dirs = user_dirs(sources.user_dirs.as_deref(), home);
        for (kind, name, path) in dirs {
            if path != *home && is_dir(&path) {
                standard.push(Place {
                    kind,
                    name: name.into(),
                    path: Some(path),
                });
            }
        }

        let mut devices = vec![Place {
            kind: PlaceKind::Computer,
            name: "Computer".into(),
            path: Some(PathBuf::from("/")),
        }];
        devices.extend(volumes(sources.mounts.as_deref().unwrap_or("")));

        let known: Vec<&PathBuf> = standard
            .iter()
            .chain(&devices)
            .filter_map(|place| place.path.as_ref())
            .collect();
        let bookmarks = bookmarks(sources.bookmarks.as_deref().unwrap_or(""))
            .into_iter()
            .filter(|place| {
                place
                    .path
                    .as_ref()
                    .is_some_and(|path| is_dir(path) && !known.contains(&path))
            })
            .collect();
        Places {
            standard,
            devices,
            bookmarks,
        }
    }

    /// Every place, in sidebar order.
    pub fn all(&self) -> impl Iterator<Item = &Place> {
        self.standard
            .iter()
            .chain(&self.devices)
            .chain(&self.bookmarks)
    }

    /// The place that is exactly `path`, or the deepest place that contains
    /// it, so the sidebar can highlight where a person is. The root counts
    /// only for itself.
    pub fn containing(&self, path: &Path) -> Option<&Place> {
        self.all()
            .filter_map(|place| Some((place, place.path.as_ref()?)))
            .filter(|(_, root)| {
                *root == path || (*root != Path::new("/") && path.starts_with(root))
            })
            .max_by_key(|(_, root)| root.components().count())
            .map(|(place, _)| place)
    }
}

/// The XDG user folders: `user-dirs.dirs` when present, else the English
/// defaults under home (what `xdg-user-dirs-update` would create).
fn user_dirs(text: Option<&str>, home: &Path) -> Vec<(PlaceKind, &'static str, PathBuf)> {
    const KNOWN: [(&str, PlaceKind, &str, &str); 6] = [
        ("XDG_DESKTOP_DIR", PlaceKind::Desktop, "Desktop", "Desktop"),
        (
            "XDG_DOCUMENTS_DIR",
            PlaceKind::Documents,
            "Documents",
            "Documents",
        ),
        (
            "XDG_DOWNLOAD_DIR",
            PlaceKind::Downloads,
            "Downloads",
            "Downloads",
        ),
        ("XDG_MUSIC_DIR", PlaceKind::Music, "Music", "Music"),
        (
            "XDG_PICTURES_DIR",
            PlaceKind::Pictures,
            "Pictures",
            "Pictures",
        ),
        ("XDG_VIDEOS_DIR", PlaceKind::Videos, "Videos", "Videos"),
    ];
    let configured = |key: &str| -> Option<PathBuf> {
        let line = text?.lines().map(str::trim).find(|line| {
            line.strip_prefix(key)
                .is_some_and(|rest| rest.starts_with('='))
        })?;
        let value = line.split_once('=')?.1.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(rest) => home.join(rest.trim_start_matches('/')),
            None => PathBuf::from(value),
        };
        path.is_absolute().then(|| normalize(&path))
    };
    KNOWN
        .iter()
        .map(|(key, kind, name, default)| {
            let path = configured(key).unwrap_or_else(|| home.join(default));
            (*kind, *name, path)
        })
        .collect()
}

/// Mounted volumes people care about: a block device mounted under
/// `/media`, `/run/media` or `/mnt`. The name is the mount point's last part,
/// which is the volume label for removable media.
fn volumes(mounts: &str) -> Vec<Place> {
    let mut places: Vec<Place> = Vec::new();
    for line in mounts.lines() {
        let mut fields = line.split(' ');
        let (Some(source), Some(mount)) = (fields.next(), fields.next()) else {
            continue;
        };
        if !source.starts_with("/dev/") || source.starts_with("/dev/loop") {
            continue;
        }
        let mount = unescape_mount(mount);
        let path = PathBuf::from(&mount);
        let removable = ["/media/", "/run/media/", "/mnt/"]
            .iter()
            .any(|root| mount.starts_with(root));
        if !removable
            || places
                .iter()
                .any(|place| place.path.as_deref() == Some(&path))
        {
            continue;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| mount.clone());
        places.push(Place {
            kind: PlaceKind::Volume,
            name,
            path: Some(path),
        });
    }
    places
}

/// `/proc/mounts` writes a space as `\040`, a tab as `\011` and a newline as
/// `\012`, a backslash as `\134`.
fn unescape_mount(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
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
    out.push_str(&String::from_utf8_lossy(&raw));
    out
}

/// GTK bookmarks: one `file:///path Optional label` per line.
fn bookmarks(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (uri, label) = line.split_once(' ').unwrap_or((line, ""));
            let path = from_file_uri(uri)?;
            let name = if label.trim().is_empty() {
                path.file_name()?.to_string_lossy().into_owned()
            } else {
                label.trim().to_string()
            };
            Some(Place {
                kind: PlaceKind::Bookmark,
                name,
                path: Some(normalize(&path)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_three_groups() {
        let sources = Sources {
            home: PathBuf::from("/home/a"),
            user_dirs: Some(
                "XDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\nXDG_MUSIC_DIR=\"$HOME/\"\n"
                    .into(),
            ),
            mounts: Some(
                "/dev/nvme0n1p2 / ext4 rw 0 0\n/dev/sda1 /run/media/a/My\\040Disk vfat rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n/dev/loop3 /mnt/x squashfs ro 0 0\n"
                    .into(),
            ),
            bookmarks: Some(
                "file:///home/a/Projects\nfile:///srv/data Data share\nfile:///home/a/Bilder\nfile:///gone\nsmb://x/y\n".into(),
            ),
        };
        let exists =
            |path: &Path| !path.starts_with("/gone") && !path.starts_with("/home/a/Documents");
        let places = Places::build(&sources, exists);
        let standard: Vec<_> = places.standard.iter().map(|p| p.name.as_str()).collect();
        // Music points at home (disabled), Documents does not exist.
        assert_eq!(
            standard,
            [
                "Recent",
                "Home",
                "Desktop",
                "Downloads",
                "Pictures",
                "Videos"
            ]
        );
        assert_eq!(
            places.standard[4].path.as_deref(),
            Some(Path::new("/home/a/Bilder"))
        );
        let devices: Vec<_> = places.devices.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(devices, ["Computer", "My Disk"]);
        let marks: Vec<_> = places.bookmarks.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(marks, ["Projects", "Data share"]);
        assert_eq!(
            places
                .containing(Path::new("/home/a/Bilder/2024"))
                .map(|p| p.kind),
            Some(PlaceKind::Pictures)
        );
        assert_eq!(places.containing(Path::new("/usr")).map(|p| p.kind), None);
    }
}
