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

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use crate::location::{file_uri, from_file_uri, normalize};

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

    /// The places of this system with `Computer` as the only device. The
    /// drives come from [`crate::mounts`], which knows their sizes.
    pub fn load_without_devices() -> Places {
        let mut sources = Sources::from_system();
        sources.mounts = None;
        Places::build(&sources, |path| path.is_dir())
    }

    /// Add `path` to the bookmarks file (`~/.config/gtk-3.0/bookmarks`).
    /// Adding a folder that is already there does nothing.
    pub fn add_bookmark(path: &Path) -> io::Result<()> {
        add_bookmark_to(&bookmarks_file(), path)
    }

    /// Remove `path` from the bookmarks file. Removing one that is not there
    /// does nothing.
    pub fn remove_bookmark(path: &Path) -> io::Result<()> {
        remove_bookmark_from(&bookmarks_file(), path)
    }

    /// Give the bookmark of `path` the label `name`; an empty name takes the
    /// folder's own name back. Renaming one that is not there does nothing.
    pub fn rename_bookmark(path: &Path, name: &str) -> io::Result<()> {
        rename_bookmark_in(&bookmarks_file(), path, name)
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
            name: crate::location::host_name(),
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

impl Place {
    /// The user folder this place is, if any. A bookmark counts when it
    /// points at one.
    pub fn special(&self) -> Option<SpecialFolder> {
        match self.kind {
            PlaceKind::Home => Some(SpecialFolder::Home),
            PlaceKind::Desktop => Some(SpecialFolder::Desktop),
            PlaceKind::Documents => Some(SpecialFolder::Documents),
            PlaceKind::Downloads => Some(SpecialFolder::Downloads),
            PlaceKind::Music => Some(SpecialFolder::Music),
            PlaceKind::Pictures => Some(SpecialFolder::Pictures),
            PlaceKind::Videos => Some(SpecialFolder::Videos),
            PlaceKind::Bookmark => self.path.as_deref().and_then(special_folder),
            PlaceKind::Recent | PlaceKind::Computer | PlaceKind::Volume => None,
        }
    }
}

fn bookmarks_file() -> PathBuf {
    let sources = Sources::from_system();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| sources.home.join(".config"));
    config.join("gtk-3.0/bookmarks")
}

fn write_atomically(file: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = file.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&temporary, text)?;
    fs::rename(&temporary, file).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

fn bookmark_line_is(line: &str, path: &Path) -> bool {
    let uri = line.trim().split(' ').next().unwrap_or("");
    from_file_uri(uri).is_some_and(|found| normalize(&found) == path)
}

fn add_bookmark_to(file: &Path, path: &Path) -> io::Result<()> {
    let path = normalize(path);
    let mut text = fs::read_to_string(file).unwrap_or_default();
    if text.lines().any(|line| bookmark_line_is(line, &path)) {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&file_uri(&path));
    text.push('\n');
    write_atomically(file, &text)
}

fn remove_bookmark_from(file: &Path, path: &Path) -> io::Result<()> {
    let path = normalize(path);
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(());
    };
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !bookmark_line_is(line, &path))
        .collect();
    if kept.len() == text.lines().count() {
        return Ok(());
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    write_atomically(file, &out)
}

fn rename_bookmark_in(file: &Path, path: &Path, name: &str) -> io::Result<()> {
    let path = normalize(path);
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(());
    };
    let name = name.trim();
    let mut found = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            if !bookmark_line_is(line, &path) {
                return line.to_string();
            }
            found = true;
            let uri = line.trim().split(' ').next().unwrap_or("");
            if name.is_empty() {
                uri.to_string()
            } else {
                format!("{uri} {name}")
            }
        })
        .collect();
    if !found {
        return Ok(());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    write_atomically(file, &out)
}

/// A folder the desktop gives a special icon: the home folder and the XDG
/// user directories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpecialFolder {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Templates,
    PublicShare,
}

/// Where the special folders are on this system, read from
/// `user-dirs.dirs`. Everything is lexical: the answers are about the path
/// as a person sees it, so `~/Downloads` stays `Downloads` when it is a link
/// to a network share, and the share's own path is a plain folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserDirs {
    dirs: Vec<(SpecialFolder, PathBuf)>,
}

impl UserDirs {
    /// Parse the text of `user-dirs.dirs`. A missing key falls back to the
    /// English name under `home`; a key that points at `home` itself (the
    /// way a disabled directory is written) is not special.
    pub fn parse(text: Option<&str>, home: &Path) -> UserDirs {
        const KEYS: [(&str, SpecialFolder, &str); 8] = [
            ("XDG_DESKTOP_DIR", SpecialFolder::Desktop, "Desktop"),
            ("XDG_DOCUMENTS_DIR", SpecialFolder::Documents, "Documents"),
            ("XDG_DOWNLOAD_DIR", SpecialFolder::Downloads, "Downloads"),
            ("XDG_MUSIC_DIR", SpecialFolder::Music, "Music"),
            ("XDG_PICTURES_DIR", SpecialFolder::Pictures, "Pictures"),
            ("XDG_VIDEOS_DIR", SpecialFolder::Videos, "Videos"),
            ("XDG_TEMPLATES_DIR", SpecialFolder::Templates, "Templates"),
            ("XDG_PUBLICSHARE_DIR", SpecialFolder::PublicShare, "Public"),
        ];
        let home = normalize(home);
        let mut dirs = vec![(SpecialFolder::Home, home.clone())];
        for (key, kind, default) in KEYS {
            let configured = text.and_then(|text| {
                let line = text.lines().map(str::trim).find(|line| {
                    line.strip_prefix(key)
                        .is_some_and(|rest| rest.starts_with('='))
                })?;
                let value = line.split_once('=')?.1.trim().trim_matches('"');
                let path = match value.strip_prefix("$HOME") {
                    Some(rest) => home.join(rest.trim_start_matches('/')),
                    None => PathBuf::from(value),
                };
                path.is_absolute().then(|| normalize(&path))
            });
            let path = configured.unwrap_or_else(|| home.join(default));
            if !dirs.iter().any(|(_, known)| *known == path) {
                dirs.push((kind, path));
            }
        }
        UserDirs { dirs }
    }

    /// Where the special folder `kind` is, if this system has one.
    pub fn path(&self, kind: SpecialFolder) -> Option<&Path> {
        self.dirs
            .iter()
            .find(|(known, _)| *known == kind)
            .map(|(_, dir)| dir.as_path())
    }

    /// The special folder `path` is, comparing text only.
    pub fn special(&self, path: &Path) -> Option<SpecialFolder> {
        let path = lexically_clean(path);
        self.dirs
            .iter()
            .find(|(_, dir)| *dir == *path)
            .map(|(kind, _)| *kind)
    }

    /// The special folders whose parent is `parent`, as `(name, kind)`.
    /// Usually empty, so a listing checks every entry only when it must.
    pub fn children_of(&self, parent: &Path) -> Vec<(OsString, SpecialFolder)> {
        let parent = lexically_clean(parent);
        self.dirs
            .iter()
            .filter(|(_, dir)| dir.parent() == Some(&*parent))
            .filter_map(|(kind, dir)| Some((dir.file_name()?.to_os_string(), *kind)))
            .collect()
    }
}

/// `path` with `.`, `..` and repeated slashes resolved on the text, copying
/// only when there is something to resolve.
fn lexically_clean(path: &Path) -> Cow<'_, Path> {
    let dirty = path
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir));
    if dirty {
        Cow::Owned(normalize(path))
    } else {
        Cow::Borrowed(path)
    }
}

struct UserDirsCache {
    dirs: Arc<UserDirs>,
    source: Option<PathBuf>,
    modified: Option<SystemTime>,
    home: PathBuf,
    checked: Instant,
}

static USER_DIRS: Mutex<Option<UserDirsCache>> = Mutex::new(None);

/// The special folders of this system. Parsed once and kept; reloaded when
/// `user-dirs.dirs` or `$HOME` changes (looked at most once a second).
pub fn system_user_dirs() -> Arc<UserDirs> {
    let mut guard = USER_DIRS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(cache) = guard.as_ref()
        && cache.checked.elapsed().as_millis() < 1000
    {
        return cache.dirs.clone();
    }
    let sources = Sources::from_system();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| sources.home.join(".config"))
        .join("user-dirs.dirs");
    let modified = fs::metadata(&config).and_then(|meta| meta.modified()).ok();
    if let Some(cache) = guard.as_mut()
        && cache.source.as_deref() == Some(config.as_path())
        && cache.modified == modified
        && cache.home == sources.home
    {
        cache.checked = Instant::now();
        return cache.dirs.clone();
    }
    let dirs = Arc::new(UserDirs::parse(sources.user_dirs.as_deref(), &sources.home));
    *guard = Some(UserDirsCache {
        dirs: dirs.clone(),
        source: Some(config),
        modified,
        home: sources.home,
        checked: Instant::now(),
    });
    dirs
}

/// Which special folder `path` is, by its text alone: no `stat`, no
/// `readlink`, no canonicalising, so it costs nothing on a network mount.
/// `~/Downloads` is [`SpecialFolder::Downloads`] even when it is a symbolic
/// link to `/mnt/nas/downloads`; `/mnt/nas/downloads` itself is `None`.
pub fn special_folder(path: &Path) -> Option<SpecialFolder> {
    system_user_dirs().special(path)
}

/// [`special_folder`] for the entry called `name` in the folder `parent`.
pub fn special_folder_in(parent: &Path, name: &OsStr) -> Option<SpecialFolder> {
    special_folder(&parent.join(name))
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
        assert_eq!(devices, [crate::location::host_name().as_str(), "My Disk"]);
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

    #[test]
    fn special_folders_are_decided_by_text() {
        let home = Path::new("/home/a");
        let dirs = UserDirs::parse(
            Some("XDG_DOWNLOAD_DIR=\"$HOME/Herunterladen\"\nXDG_MUSIC_DIR=\"$HOME/\"\nXDG_PUBLICSHARE_DIR=\"/srv/public\"\n"),
            home,
        );
        assert_eq!(dirs.special(home), Some(SpecialFolder::Home));
        assert_eq!(
            dirs.special(Path::new("/home/a/Herunterladen/")),
            Some(SpecialFolder::Downloads)
        );
        assert_eq!(dirs.special(Path::new("/home/a/Downloads")), None);
        assert_eq!(
            dirs.special(Path::new("/home/a/Pictures/../Documents")),
            Some(SpecialFolder::Documents)
        );
        // A disabled directory points at home and stays Home.
        assert_eq!(dirs.special(Path::new("/home/a/Music")), None);
        assert_eq!(
            dirs.special(Path::new("/srv/public")),
            Some(SpecialFolder::PublicShare)
        );
        assert_eq!(dirs.special(Path::new("/home/a/Documents/x")), None);
        let children = dirs.children_of(home);
        assert!(children.iter().any(|(name, _)| name == "Herunterladen"));
        assert!(dirs.children_of(Path::new("/etc")).is_empty());
    }

    #[test]
    fn a_linked_user_folder_keeps_its_icon_and_the_target_does_not() {
        let root = std::env::temp_dir().join(format!("bingux-files-special-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let share = root.join("share/downloads");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&share).unwrap();
        std::os::unix::fs::symlink(&share, home.join("Downloads")).unwrap();
        let dirs = UserDirs::parse(None, &home);
        assert_eq!(
            dirs.special(&home.join("Downloads")),
            Some(SpecialFolder::Downloads)
        );
        assert_eq!(dirs.special(&share), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn bookmarks_are_added_and_removed_in_the_gtk_file() {
        let dir = std::env::temp_dir().join(format!("bingux-files-bookmarks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("gtk-3.0/bookmarks");
        add_bookmark_to(&file, Path::new("/srv/a b")).unwrap();
        add_bookmark_to(&file, Path::new("/srv/a b/")).unwrap();
        add_bookmark_to(&file, Path::new("/srv/c")).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert_eq!(text, "file:///srv/a%20b\nfile:///srv/c\n");
        fs::write(&file, "file:///srv/a%20b My share\nfile:///srv/c\n").unwrap();
        remove_bookmark_from(&file, Path::new("/srv/a b")).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "file:///srv/c\n");
        remove_bookmark_from(&file, Path::new("/nowhere")).unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bookmark_is_renamed_in_place_and_an_empty_name_restores_the_default() {
        let dir = std::env::temp_dir().join(format!("bingux-files-rename-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("gtk-3.0/bookmarks");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "file:///srv/a%20b\nfile:///srv/c Old name\n").unwrap();
        rename_bookmark_in(&file, Path::new("/srv/c"), "  New name ").unwrap();
        rename_bookmark_in(&file, Path::new("/srv/a b/"), "Share").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "file:///srv/a%20b Share\nfile:///srv/c New name\n"
        );
        rename_bookmark_in(&file, Path::new("/srv/c"), " ").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "file:///srv/a%20b Share\nfile:///srv/c\n"
        );
        rename_bookmark_in(&file, Path::new("/nowhere"), "x").unwrap();
        let _ = fs::remove_dir_all(&dir);
    }
}
