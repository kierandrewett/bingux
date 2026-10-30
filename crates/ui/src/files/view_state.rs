//! What the file views remember between runs: column widths and order, and
//! the icon size (the zoom level) and sort of each folder. A folder is
//! remembered under its exact path bytes (see [`folder_key`]).
//!
//! It is one small JSON file at `$XDG_STATE_HOME/bingux/files/view.json`
//! (`~/.local/state/...` when that is unset). `BINGUX_FILES_STATE` names
//! another file, which is how documentation captures keep a real person's
//! state out of the picture. A test thread keeps a store of its own in
//! memory and never reads or writes the file.
//!
//! The file is read once, on first use, and kept in memory. A change is
//! written by a background thread half a second after the last one, so a
//! person dragging a column edge or zooming does not write on every frame. A
//! file that cannot be read or does not parse is ignored, as if it were not
//! there; a write that fails is dropped, because this is a convenience and
//! nothing depends on it.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use bingux_files::{Sort, SortKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Folders remembered. The oldest is forgotten past this.
const FOLDER_LIMIT: usize = 400;
/// How long a change waits for the next one before it is written.
const WRITE_DELAY: Duration = Duration::from_millis(500);
/// The format number of the file. A file with another number is ignored.
const FORMAT: u32 = 1;

/// How one folder is shown: remembered when the person changes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderView {
    /// The icon size in logical pixels, one of `zoom::LADDER`. It decides
    /// whether the folder shows as a list or a grid. A folder saved before
    /// the zoom was one ladder has none and reads as the default.
    #[serde(default = "default_icon_px")]
    pub icon_px: u16,
    pub sort: SortSetting,
}

fn default_icon_px() -> u16 {
    super::zoom::DEFAULT_PX
}

/// [`Sort`] as the file keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortSetting {
    pub key: SortKeySetting,
    pub descending: bool,
    pub folders_first: bool,
}

/// [`SortKey`] as the file keeps it. A key this build does not know reads
/// back as the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKeySetting {
    Name,
    Size,
    Modified,
    Kind,
    Deleted,
    #[serde(other)]
    Unknown,
}

impl From<Sort> for SortSetting {
    fn from(sort: Sort) -> Self {
        SortSetting {
            key: match sort.key {
                SortKey::Name => SortKeySetting::Name,
                SortKey::Size => SortKeySetting::Size,
                SortKey::Modified => SortKeySetting::Modified,
                SortKey::Kind => SortKeySetting::Kind,
                SortKey::Deleted => SortKeySetting::Deleted,
            },
            descending: sort.descending,
            folders_first: sort.folders_first,
        }
    }
}

impl From<SortSetting> for Sort {
    fn from(setting: SortSetting) -> Self {
        Sort {
            key: match setting.key {
                SortKeySetting::Name | SortKeySetting::Unknown => SortKey::Name,
                SortKeySetting::Size => SortKey::Size,
                SortKeySetting::Modified => SortKey::Modified,
                SortKeySetting::Kind => SortKey::Kind,
                SortKeySetting::Deleted => SortKey::Deleted,
            },
            descending: setting.descending,
            folders_first: setting.folders_first,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    format: u32,
    /// Free-form settings by key: `columns`.
    #[serde(default)]
    values: BTreeMap<String, serde_json::Value>,
    /// Newest last.
    #[serde(default)]
    folders: Vec<(String, FolderView)>,
}

struct Store {
    file: File,
    path: Option<PathBuf>,
    /// A writer thread is waiting to save.
    pending: bool,
}

/// Run `f` on the store. A test thread has a store of its own, kept in memory: a test never
/// reads what another one remembered (a folder shown at another zoom) and never writes the
/// person's file.
fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    #[cfg(test)]
    {
        thread_local! {
            static STORE: std::cell::RefCell<Store> = const {
                std::cell::RefCell::new(Store {
                    file: File {
                        format: 0,
                        values: BTreeMap::new(),
                        folders: Vec::new(),
                    },
                    path: None,
                    pending: false,
                })
            };
        }
        STORE.with(|store| f(&mut store.borrow_mut()))
    }
    #[cfg(not(test))]
    {
        static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| {
            let path = state_path();
            let file = path
                .as_deref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .and_then(|text| serde_json::from_str::<File>(&text).ok())
                .filter(|file| file.format == FORMAT)
                .unwrap_or_default();
            Mutex::new(Store {
                file,
                path,
                pending: false,
            })
        });
        f(&mut STORE.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[cfg(not(test))]
fn state_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BINGUX_FILES_STATE") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("bingux/files/view.json"))
}

/// The value saved under `key`, if there is one this build can read.
pub fn load<T: DeserializeOwned>(key: &str) -> Option<T> {
    with_store(|store| serde_json::from_value(store.file.values.get(key)?.clone()).ok())
}

/// Remember `value` under `key`. Written to disk shortly after the last call.
pub fn save<T: Serialize>(key: &str, value: &T) {
    let Ok(value) = serde_json::to_value(value) else {
        return;
    };
    with_store(|store| {
        if store.file.values.get(key) == Some(&value) {
            return;
        }
        store.file.values.insert(key.to_string(), value);
        schedule_write(store);
    });
}

/// The key a folder is remembered under: its path, byte for byte, so two
/// folders whose names are not valid UTF-8 never share a view because they
/// display alike (a lossy conversion gives both the same replacement
/// characters).
///
/// A path that is valid UTF-8 is its own text, so the keys an earlier build
/// wrote still match. Any other path is a NUL, which no path contains and so
/// no other key starts with, then its bytes, with `%` and every byte outside
/// printable ASCII written as `%XX`: different bytes, different key.
fn folder_key(folder: &Path) -> Cow<'_, str> {
    if let Some(text) = folder.to_str() {
        return Cow::Borrowed(text);
    }
    let mut key = String::from("\0");
    for &byte in folder.as_os_str().as_bytes() {
        if (0x20..0x7f).contains(&byte) && byte != b'%' {
            key.push(char::from(byte));
        } else {
            let _ = write!(key, "%{byte:02X}");
        }
    }
    Cow::Owned(key)
}

/// The view saved under `key`, the newest first.
fn find_folder(folders: &[(String, FolderView)], key: &str) -> Option<FolderView> {
    folders
        .iter()
        .rev()
        .find(|(path, _)| path == key)
        .map(|(_, view)| *view)
}

/// Puts `view` under `key` as the newest, and forgets the oldest past
/// [`FOLDER_LIMIT`]. False when nothing changed.
fn put_folder(folders: &mut Vec<(String, FolderView)>, key: String, view: FolderView) -> bool {
    if let Some(at) = folders.iter().position(|(path, _)| *path == key) {
        if folders[at].1 == view && at + 1 == folders.len() {
            return false;
        }
        folders.remove(at);
    }
    folders.push((key, view));
    if folders.len() > FOLDER_LIMIT {
        let extra = folders.len() - FOLDER_LIMIT;
        folders.drain(..extra);
    }
    true
}

/// How `folder` was last shown, if the person ever changed it.
pub fn folder(folder: &Path) -> Option<FolderView> {
    with_store(|store| find_folder(&store.file.folders, &folder_key(folder)))
}

/// Remember how `folder` is shown.
pub fn remember_folder(folder: &Path, view: FolderView) {
    with_store(|store| {
        if put_folder(&mut store.file.folders, folder_key(folder).into_owned(), view) {
            schedule_write(store);
        }
    });
}

fn schedule_write(guard: &mut Store) {
    if guard.pending || guard.path.is_none() {
        return;
    }
    guard.pending = true;
    std::thread::spawn(|| {
        std::thread::sleep(WRITE_DELAY);
        with_store(|guard| {
            guard.pending = false;
            guard.file.format = FORMAT;
            let (Some(path), Ok(text)) = (&guard.path, serde_json::to_string(&guard.file)) else {
                return;
            };
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let temporary = path.with_extension("json.tmp");
            if std::fs::write(&temporary, text).is_ok() {
                let _ = std::fs::rename(&temporary, path);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_survives_the_file() {
        let sort = Sort {
            key: SortKey::Modified,
            descending: true,
            folders_first: false,
        };
        let text = serde_json::to_string(&SortSetting::from(sort)).unwrap();
        let back: SortSetting = serde_json::from_str(&text).unwrap();
        assert_eq!(Sort::from(back), sort);
    }

    #[test]
    fn an_unknown_sort_key_reads_as_name() {
        let back: SortSetting = serde_json::from_str(
            r#"{"key":"colour","descending":true,"folders_first":true}"#,
        )
        .unwrap();
        assert_eq!(Sort::from(back).key, SortKey::Name);
    }

    #[test]
    fn a_folder_remembers_its_icon_size() {
        let view = FolderView {
            icon_px: 96,
            sort: Sort::default().into(),
        };
        let text = serde_json::to_string(&view).unwrap();
        assert_eq!(serde_json::from_str::<FolderView>(&text).unwrap(), view);
    }

    #[test]
    fn a_folder_saved_with_the_old_list_or_grid_flag_opens_at_the_default() {
        let back: FolderView = serde_json::from_str(
            r#"{"grid":true,"sort":{"key":"size","descending":false,"folders_first":true}}"#,
        )
        .unwrap();
        assert_eq!(back.icon_px, super::super::zoom::DEFAULT_PX);
        assert_eq!(Sort::from(back.sort).key, SortKey::Size);
    }
}

#[cfg(test)]
mod folder_keys {
    use std::ffi::OsStr;

    use super::*;

    fn odd(name: &[u8]) -> PathBuf {
        PathBuf::from("/home/me").join(OsStr::from_bytes(name))
    }

    fn view(key: SortKey, icon_px: u16) -> FolderView {
        FolderView {
            icon_px,
            sort: Sort {
                key,
                descending: false,
                folders_first: true,
            }
            .into(),
        }
    }

    #[test]
    fn two_folders_that_display_alike_have_two_keys() {
        let a = odd(b"caf\xe9-\xff");
        let b = odd(b"caf\xe9-\xfe");
        let text = PathBuf::from("/home/me/caf\u{fffd}-\u{fffd}");
        // The text they used to share.
        assert_eq!(a.to_string_lossy(), b.to_string_lossy());
        assert_eq!(a.to_string_lossy(), text.to_string_lossy());
        let keys = [folder_key(&a), folder_key(&b), folder_key(&text)];
        for (at, key) in keys.iter().enumerate() {
            for other in &keys[at + 1..] {
                assert_ne!(key, other);
            }
        }
        assert_eq!(folder_key(&a), folder_key(&odd(b"caf\xe9-\xff")));
    }

    #[test]
    fn a_valid_path_is_its_own_key_so_a_file_from_an_earlier_build_still_matches() {
        let path = Path::new("/srv/with space/a%20b/\u{e9}");
        assert_eq!(folder_key(path), "/srv/with space/a%20b/\u{e9}");
        assert!(matches!(folder_key(path), Cow::Borrowed(_)));
    }

    #[test]
    fn an_escape_typed_as_text_is_not_the_byte_it_spells() {
        // `%FF` as three characters and the byte 0xFF, before the same byte.
        assert_ne!(folder_key(&odd(b"%FF\xfe")), folder_key(&odd(b"\xff\xfe")));
        // No valid path starts with the NUL that marks an escaped one.
        assert_ne!(folder_key(&odd(b"x\xff")), folder_key(Path::new("/home/me/x%FF")));
        assert!(folder_key(&odd(b"x\xff")).starts_with('\0'));
    }

    #[test]
    fn each_odd_folder_keeps_its_own_view() {
        let (a, b) = (odd(b"caf\xe9-\xff"), odd(b"caf\xe9-\xfe"));
        let mut folders = Vec::new();
        assert!(put_folder(&mut folders, folder_key(&a).into_owned(), view(SortKey::Size, 96)));
        assert_eq!(find_folder(&folders, &folder_key(&a)), Some(view(SortKey::Size, 96)));
        // The lookalike has not been given a view.
        assert_eq!(find_folder(&folders, &folder_key(&b)), None);
        assert!(put_folder(&mut folders, folder_key(&b).into_owned(), view(SortKey::Modified, 32)));
        assert_eq!(folders.len(), 2);
        assert_eq!(find_folder(&folders, &folder_key(&a)), Some(view(SortKey::Size, 96)));
        assert_eq!(find_folder(&folders, &folder_key(&b)), Some(view(SortKey::Modified, 32)));
        // Putting the same view again changes nothing.
        assert!(!put_folder(&mut folders, folder_key(&b).into_owned(), view(SortKey::Modified, 32)));
    }

    #[test]
    fn an_odd_folder_survives_the_file() {
        let (a, b) = (odd(b"caf\xe9-\xff"), odd(b"caf\xe9-\xfe"));
        let mut file = File {
            format: FORMAT,
            ..File::default()
        };
        put_folder(&mut file.folders, folder_key(&a).into_owned(), view(SortKey::Kind, 64));
        put_folder(&mut file.folders, folder_key(&b).into_owned(), view(SortKey::Name, 128));
        let text = serde_json::to_string(&file).unwrap();
        let back: File = serde_json::from_str(&text).unwrap();
        assert_eq!(find_folder(&back.folders, &folder_key(&a)), Some(view(SortKey::Kind, 64)));
        assert_eq!(find_folder(&back.folders, &folder_key(&b)), Some(view(SortKey::Name, 128)));
    }

    #[test]
    fn the_oldest_folders_are_forgotten_past_the_limit() {
        let mut folders = Vec::new();
        for index in 0..FOLDER_LIMIT + 5 {
            put_folder(&mut folders, format!("/f/{index}"), view(SortKey::Name, 64));
        }
        assert_eq!(folders.len(), FOLDER_LIMIT);
        assert_eq!(find_folder(&folders, "/f/0"), None);
        assert!(find_folder(&folders, &format!("/f/{}", FOLDER_LIMIT + 4)).is_some());
    }
}
