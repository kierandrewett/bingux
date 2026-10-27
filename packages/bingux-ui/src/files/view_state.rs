//! What the file views remember between runs: column widths and order, and
//! the icon size (the zoom level) and sort of each folder.
//!
//! It is one small JSON file at `$XDG_STATE_HOME/bingux/files/view.json`
//! (`~/.local/state/...` when that is unset). `BINGUX_FILES_STATE` names
//! another file, which is how tests and documentation captures keep a real
//! person's state out of the picture.
//!
//! The file is read once, on first use, and kept in memory. A change is
//! written by a background thread half a second after the last one, so a
//! person dragging a column edge or zooming does not write on every frame. A
//! file that cannot be read or does not parse is ignored, as if it were not
//! there; a write that fails is dropped, because this is a convenience and
//! nothing depends on it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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

fn store() -> &'static Mutex<Store> {
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
    &STORE
}

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
    let store = store().lock().unwrap_or_else(PoisonError::into_inner);
    serde_json::from_value(store.file.values.get(key)?.clone()).ok()
}

/// Remember `value` under `key`. Written to disk shortly after the last call.
pub fn save<T: Serialize>(key: &str, value: &T) {
    let Ok(value) = serde_json::to_value(value) else {
        return;
    };
    let mut store = store().lock().unwrap_or_else(PoisonError::into_inner);
    if store.file.values.get(key) == Some(&value) {
        return;
    }
    store.file.values.insert(key.to_string(), value);
    schedule_write(&mut store);
}

/// How `folder` was last shown, if the person ever changed it.
pub fn folder(folder: &Path) -> Option<FolderView> {
    let key = folder.to_string_lossy();
    let store = store().lock().unwrap_or_else(PoisonError::into_inner);
    store
        .file
        .folders
        .iter()
        .rev()
        .find(|(path, _)| *path == key)
        .map(|(_, view)| *view)
}

/// Remember how `folder` is shown.
pub fn remember_folder(folder: &Path, view: FolderView) {
    let key = folder.to_string_lossy().into_owned();
    let mut store = store().lock().unwrap_or_else(PoisonError::into_inner);
    let folders = &mut store.file.folders;
    if let Some(at) = folders.iter().position(|(path, _)| *path == key) {
        if folders[at].1 == view && at + 1 == folders.len() {
            return;
        }
        folders.remove(at);
    }
    folders.push((key, view));
    if folders.len() > FOLDER_LIMIT {
        let extra = folders.len() - FOLDER_LIMIT;
        folders.drain(..extra);
    }
    schedule_write(&mut store);
}

fn schedule_write(guard: &mut Store) {
    if guard.pending || guard.path.is_none() {
        return;
    }
    guard.pending = true;
    std::thread::spawn(|| {
        std::thread::sleep(WRITE_DELAY);
        let mut guard = store().lock().unwrap_or_else(PoisonError::into_inner);
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
