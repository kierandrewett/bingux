//! What an application's controls remember between runs: the column widths
//! and order of a table, the items a person put in a toolbar.
//!
//! It is one small JSON file per application at
//! `$XDG_STATE_HOME/bingux/<app id>/ui.json` (`~/.local/state/...` when that
//! is unset; `bingux-ui` stands in for an app with no id, as in a headless
//! snapshot). `BINGUX_UI_STATE` names another file, which is how
//! documentation captures keep a real person's state out of the picture. A
//! test thread keeps a store of its own in memory and never reads or writes
//! the file.
//!
//! The file is read once, on first use, and kept in memory. A change is
//! written by a background thread half a second after the last one, so a
//! person dragging a column edge does not write on every frame. A file that
//! cannot be read or does not parse is ignored, as if it were not there; a
//! write that fails is dropped, because this is a convenience and nothing
//! depends on it.

use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::{LazyLock, Mutex, PoisonError};
#[cfg(not(test))]
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// How long a change waits for the next one before it is written.
#[cfg(not(test))]
const WRITE_DELAY: Duration = Duration::from_millis(500);
/// The format number of the file. A file with another number is ignored.
const FORMAT: u32 = 1;

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    format: u32,
    #[serde(default)]
    values: BTreeMap<String, serde_json::Value>,
}

struct Store {
    file: File,
    path: Option<PathBuf>,
    /// A writer thread is waiting to save.
    #[cfg_attr(test, allow(dead_code))]
    pending: bool,
}

/// Run `f` on the store. A test thread has a store of its own, kept in memory.
fn with_store<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    #[cfg(test)]
    {
        thread_local! {
            static STORE: std::cell::RefCell<Store> = const {
                std::cell::RefCell::new(Store {
                    file: File { format: 0, values: BTreeMap::new() },
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
    if let Some(path) = std::env::var_os("BINGUX_UI_STATE") {
        return Some(PathBuf::from(path));
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let id = crate::app::app_id();
    let id = if id.is_empty() { "bingux-ui" } else { id };
    Some(base.join("bingux").join(id).join("ui.json"))
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

/// Forget what is saved under `key`.
pub fn forget(key: &str) {
    with_store(|store| {
        if store.file.values.remove(key).is_some() {
            schedule_write(store);
        }
    });
}

#[cfg(test)]
fn schedule_write(_store: &mut Store) {}

#[cfg(not(test))]
fn schedule_write(store: &mut Store) {
    if store.pending || store.path.is_none() {
        return;
    }
    store.pending = true;
    std::thread::spawn(|| {
        std::thread::sleep(WRITE_DELAY);
        with_store(|store| {
            store.pending = false;
            store.file.format = FORMAT;
            let Some(path) = store.path.clone() else {
                return;
            };
            let Ok(text) = serde_json::to_string_pretty(&store.file) else {
                return;
            };
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let temporary = path.with_extension("json.tmp");
            if std::fs::write(&temporary, text).is_ok() {
                let _ = std::fs::rename(&temporary, &path);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_value_reads_back() {
        save("saved.test", &vec![1, 2, 3]);
        assert_eq!(load::<Vec<i32>>("saved.test"), Some(vec![1, 2, 3]));
        forget("saved.test");
        assert_eq!(load::<Vec<i32>>("saved.test"), None);
    }

    #[test]
    fn a_value_of_another_shape_is_ignored() {
        save("saved.shape", &"text");
        assert_eq!(load::<Vec<i32>>("saved.shape"), None);
    }
}
